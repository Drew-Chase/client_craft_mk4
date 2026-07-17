use crate::recipes::recipe::{Recipe, RecipeError, StringOrArray};
use crate::recipes::recipe_type::RecipeType;
use crate::recipes::tags::Tags;
use crate::recipes::Item;
use std::collections::{HashMap, HashSet};

/// Craftable quantities are capped at this value in the reference output.
const MAX_QUANTITY: u64 = 999_999;

#[derive(Debug, thiserror::Error)]
pub enum TreeBuilderError {
    #[error(transparent)]
    RecipeError(#[from] RecipeError),
}

/// Maps item id strings to dense `u32` indices so the resolver can run on flat
/// vectors instead of hashing strings in the hot path.
#[derive(Default)]
struct Interner {
    ids: HashMap<String, u32>,
    names: Vec<String>,
}

impl Interner {
    fn intern(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.ids.get(name) {
            return id;
        }
        let id = self.names.len() as u32;
        self.ids.insert(name.to_string(), id);
        self.names.push(name.to_string());
        id
    }

    fn len(&self) -> usize {
        self.names.len()
    }
}

/// One ingredient requirement of a recipe: the interned ids that can fill it and
/// how many are consumed per craft.
struct Slot {
    options: Vec<u32>,
    count: u64,
}

/// A recipe pre-processed for quantity resolution: its per-craft yield and the
/// consolidated ingredient slots (identical option sets merged, counts summed).
struct PreparedRecipe {
    yield_count: u64,
    slots: Vec<Slot>,
}

const STATE_UNKNOWN: u8 = 0;
const STATE_VISITING: u8 = 1;
const STATE_DONE: u8 = 2;

/// Resolves max craftable counts over interned ids, following the reference
/// semantics: `available(item) = inventory for base resources, else max_craftable`,
/// where a slot's availability is the *sum* over its options. Memoized and
/// cycle-safe (an item mid-evaluation contributes 0 to itself).
struct Resolver<'a> {
    recipes: &'a [Vec<PreparedRecipe>],
    inventory: &'a [u64],
    in_inventory: &'a [bool],
    memo: Vec<u64>,
    state: Vec<u8>,
}

impl Resolver<'_> {
    /// A provided (inventory) item or a raw material (no recipe) is a *base resource*:
    /// it is never crafted further, which also prevents block<->ingot round-trips
    /// from inflating availability.
    fn available(&mut self, id: usize) -> u64 {
        if self.in_inventory[id] || self.recipes[id].is_empty() {
            self.inventory[id]
        } else {
            self.max_craftable(id)
        }
    }

    /// Maximum number of `id` producible from the inventory (uncapped). The 999 cap
    /// is applied only to the final reported list, never to intermediate
    /// availability, so it does not throttle downstream recipes.
    fn max_craftable(&mut self, id: usize) -> u64 {
        match self.state[id] {
            STATE_DONE => return self.memo[id],
            STATE_VISITING => return 0,
            _ => {}
        }
        let recipes = self.recipes;
        if recipes[id].is_empty() {
            self.state[id] = STATE_DONE;
            return 0;
        }

        self.state[id] = STATE_VISITING;
        let mut best = 0u64;
        for recipe in &recipes[id] {
            if recipe.slots.is_empty() {
                continue; // no known ingredients -> cannot be crafted from nothing
            }
            let mut crafts = u64::MAX;
            for slot in &recipe.slots {
                let mut available = 0u64;
                for &option in &slot.options {
                    available += self.available(option as usize);
                }
                crafts = crafts.min(available / slot.count);
                if crafts == 0 {
                    break;
                }
            }
            best = best.max(crafts.saturating_mul(recipe.yield_count));
        }
        self.state[id] = STATE_DONE;
        self.memo[id] = best;
        best
    }
}

pub struct TreeBuilder {
    recipes: Vec<Recipe>,
}

impl TreeBuilder {
    pub fn new(recipes: Vec<Recipe>) -> Self {
        Self { recipes }
    }

    pub fn build(&self, items: Vec<Item>, tags: &Tags) -> Result<Vec<Item>, TreeBuilderError> {
        let mut interner = Interner::default();
        // Tag/item tokens repeat heavily across recipes (#planks appears in dozens),
        // so each unique token is expanded and interned exactly once.
        let mut token_cache: HashMap<String, Vec<u32>> = HashMap::new();

        // Pre-process every recipe into interned, consolidated slots, grouped by output.
        let mut prepared: Vec<(u32, PreparedRecipe)> = Vec::with_capacity(self.recipes.len());
        for recipe in &self.recipes {
            let Some(result) = &recipe.result else {
                continue;
            };
            let output = interner.intern(&result.id);
            // An invalid recipe (a slot left empty after excluding the output) is
            // dropped entirely; it doesn't count toward the recipe split either.
            let Some(slots) = prepare_slots(recipe, output, tags, &mut interner, &mut token_cache)
            else {
                continue;
            };
            let yield_count = if result.count == 0 {
                1
            } else {
                result.count as u64
            };
            prepared.push((output, PreparedRecipe { yield_count, slots }));
        }
        for item in &items {
            interner.intern(&item.id);
        }

        let n = interner.len();
        let mut recipe_lists: Vec<Vec<PreparedRecipe>> = Vec::with_capacity(n);
        recipe_lists.resize_with(n, Vec::new);
        for (output, recipe) in prepared {
            recipe_lists[output as usize].push(recipe);
        }

        let mut inventory = vec![0u64; n];
        let mut in_inventory = vec![false; n];
        for item in &items {
            let id = interner.ids[&item.id] as usize;
            inventory[id] += item.quantity as u64;
            in_inventory[id] = true;
        }

        let mut resolver = Resolver {
            recipes: &recipe_lists,
            inventory: &inventory,
            in_inventory: &in_inventory,
            memo: vec![0; n],
            state: vec![STATE_UNKNOWN; n],
        };

        // Report every craftable output (>= 1). Provided inventory items are base
        // resources, not craft results, so they're excluded. When an item has several
        // recipes the reference splits its total evenly across them and reports one
        // recipe's share (`maxNewItems / recipeCount`), then caps at MAX_QUANTITY.
        let mut result = Vec::new();
        for id in 0..n {
            if in_inventory[id] || recipe_lists[id].is_empty() {
                continue;
            }
            let quantity = resolver.max_craftable(id) / recipe_lists[id].len() as u64;
            if quantity >= 1 {
                result.push(Item {
                    id: interner.names[id].clone(),
                    quantity: quantity.min(MAX_QUANTITY) as u32,
                });
            }
        }
        result.sort_unstable_by(|a, b| a.id.cmp(&b.id));

        Ok(result)
    }

    pub fn flat(&self) -> HashMap<String, Vec<Item>> {
        let mut result: HashMap<String, Vec<Item>> = HashMap::new();

        for recipe in &self.recipes {
            if let Some(recipe_result) = &recipe.result {
                let item = Item {
                    id: recipe_result.id.clone(),
                    quantity: recipe_result.count.into(),
                };
                result
                    .entry(recipe_result.id.clone())
                    .or_default()
                    .push(item);
            }
        }

        result
    }

    pub fn items(&self) -> HashSet<String> {
        let mut items: HashSet<String> = HashSet::new();
        for item in &self.recipes {
            if let Some(result) = &item.result {
                items.insert(result.id.clone());
            }
        }

        items
    }
}

/// Expands a recipe's ingredients into consolidated [`Slot`]s of interned ids.
/// Shaped recipes derive per-symbol counts from the pattern; shapeless recipes count
/// ingredient repeats. Slots sharing an identical option set are merged with their
/// counts summed. Returns `None` when any slot has no options left after excluding
/// the recipe's own output — the reference treats such a recipe as invalid (this is
/// what keeps self-duplicating recipes like armor-trim templates uncraftable).
fn prepare_slots(
    recipe: &Recipe,
    output: u32,
    tags: &Tags,
    interner: &mut Interner,
    token_cache: &mut HashMap<String, Vec<u32>>,
) -> Option<Vec<Slot>> {
    let mut merged: HashMap<Vec<u32>, u64> = HashMap::new();

    let mut add = |source: &StringOrArray, count: u64, interner: &mut Interner| {
        let mut options = resolve_source(source, output, tags, interner, token_cache);
        if options.is_empty() {
            return false;
        }
        options.sort_unstable();
        options.dedup();
        *merged.entry(options).or_default() += count;
        true
    };

    match recipe.recipe_type {
        RecipeType::CraftingShaped => {
            if let (Some(key), Some(pattern)) = (&recipe.key, &recipe.pattern) {
                for (symbol, source) in key {
                    let Some(symbol) = symbol.chars().next() else {
                        continue;
                    };
                    let count = pattern_rows(pattern)
                        .into_iter()
                        .flat_map(|row| row.chars())
                        .filter(|&ch| ch == symbol)
                        .count() as u64;
                    if count > 0 && !add(source, count, interner) {
                        return None;
                    }
                }
            }
        }
        RecipeType::CraftingShapeless => {
            if let Some(ingredients) = &recipe.ingredients {
                for source in ingredients {
                    if !add(source, 1, interner) {
                        return None;
                    }
                }
            }
        }
        _ => {}
    }

    Some(
        merged
            .into_iter()
            .map(|(options, count)| Slot { options, count })
            .collect(),
    )
}

/// Yields the string rows of a shaped recipe's `pattern` (a single string or array).
fn pattern_rows(pattern: &StringOrArray) -> Vec<&str> {
    match pattern {
        StringOrArray::String(row) => vec![row.as_str()],
        StringOrArray::Array(rows) => rows.iter().map(|r| r.as_str()).collect(),
    }
}

/// Resolves an ingredient source (a token or an array of tokens, each an item id or
/// a `#tag`) into interned option ids, excluding the recipe's own output. Expansion
/// per unique token is cached.
fn resolve_source(
    source: &StringOrArray,
    output: u32,
    tags: &Tags,
    interner: &mut Interner,
    token_cache: &mut HashMap<String, Vec<u32>>,
) -> Vec<u32> {
    let mut options = Vec::new();
    let mut add_token = |token: &str, interner: &mut Interner| {
        if !token_cache.contains_key(token) {
            let mut ids: Vec<u32> = tags
                .resolve(token)
                .iter()
                .map(|id| interner.intern(id))
                .collect();
            ids.sort_unstable();
            token_cache.insert(token.to_string(), ids);
        }
        options.extend(token_cache[token].iter().copied().filter(|&id| id != output));
    };
    match source {
        StringOrArray::String(token) => add_token(token, interner),
        StringOrArray::Array(tokens) => {
            for token in tokens {
                add_token(token, interner);
            }
        }
    }
    options
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipes::recipe::Recipe;
    use crate::recipes::recipe_type::RecipeType;
    use std::collections::BTreeMap;

    /// Compares `build` against the reference dump. The comparison is order-insensitive
    /// (by id -> quantity) because the reference's output order derives from Java
    /// identity-hash iteration and is not reproducible.
    #[test]
    fn benchmark_matches_original() {
        let recipes = Recipe::load_from_filesystem(env!("TEST_RECIPE_DIRECTORY")).unwrap();
        let recipes = recipes
            .iter()
            .filter(|i| {
                i.recipe_type == RecipeType::CraftingShapeless
                    || i.recipe_type == RecipeType::CraftingShaped
            })
            .cloned()
            .collect::<Vec<_>>();
        let items: Vec<Item> = serde_json::from_str(
            std::fs::read_to_string(env!("TEST_FAKE_INVENTORY_FILE"))
                .unwrap()
                .as_str(),
        )
        .unwrap();
        let tags = Tags::load_from_filesystem(env!("TEST_TAGS_DIRECTORY")).unwrap();

        let tree = TreeBuilder::new(recipes);
        let got: BTreeMap<String, u32> = tree
            .build(items, &tags)
            .unwrap()
            .into_iter()
            .map(|i| (i.id, i.quantity))
            .collect();

        let expected: BTreeMap<String, u32> = serde_json::from_str::<Vec<Item>>(
            std::fs::read_to_string("./target/clientcraft_recipes_dump.json")
                .unwrap()
                .as_str(),
        )
        .unwrap()
        .into_iter()
        .map(|i| (i.id, i.quantity))
        .collect();

        assert_eq!(got, expected);
    }
}
