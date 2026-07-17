use crate::recipes::recipe::{Recipe, RecipeError, StringOrArray};
use crate::recipes::Item;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, thiserror::Error)]
pub enum TreeBuilderError {
    #[error(transparent)]
    RecipeError(#[from] RecipeError),
}

struct PreparedRecipe {
    count: u8,
    slots: Vec<HashSet<String>>,
}

fn recipe_slots(recipe: &Recipe, output: &str) -> Vec<HashSet<String>> {
    let mut slots: Vec<HashSet<String>> = Vec::new();
    let mut seen: HashSet<Vec<String>> = HashSet::new();

    let mut add_slot = |source: &StringOrArray| {
        let mut options: HashSet<String> = HashSet::new();
        match source {
            StringOrArray::String(id) => {
                options.insert(id.clone());
            }
            StringOrArray::Array(ids) => {
                for id in ids {
                    options.insert(id.clone());
                }
            }
        }
        options.remove(output);
        if options.is_empty() {
            return;
        }
        let mut canonical: Vec<String> = options.iter().cloned().collect();
        canonical.sort();
        if seen.insert(canonical) {
            slots.push(options);
        }
    };

    if let Some(key) = &recipe.key {
        for source in key.values() {
            add_slot(source);
        }
    }
    if let Some(ingredients) = &recipe.ingredients {
        for source in ingredients {
            add_slot(source);
        }
    }

    slots
}

fn craftable_count(recipes: &[PreparedRecipe], resolved: &HashSet<String>) -> Option<u8> {
    recipes
        .iter()
        .find(|recipe| {
            recipe
                .slots
                .iter()
                .all(|slot| slot.iter().any(|option| resolved.contains(option)))
        })
        .map(|recipe| recipe.count)
}

pub struct TreeBuilder {
    recipes: Vec<Recipe>,
}

impl TreeBuilder {
    pub fn new(recipes: Vec<Recipe>) -> Self {
        Self { recipes }
    }
    pub fn build(&self, items: Vec<Item>) -> Result<Vec<Item>, TreeBuilderError> {

        let mut by_output: HashMap<String, Vec<PreparedRecipe>> = HashMap::new();
        for recipe in &self.recipes {
            if let Some(result) = &recipe.result {
                let slots = recipe_slots(recipe, &result.id);
                by_output
                    .entry(result.id.clone())
                    .or_default()
                    .push(PreparedRecipe {
                        count: result.count,
                        slots,
                    });
            }
        }

        let mut resolved: HashSet<String> = HashSet::new();

        for item in &items {
            resolved.insert(item.id.clone());
        }

        let mut option_to_parents: HashMap<String, HashSet<String>> = HashMap::new();
        for (output, recipes) in &by_output {
            for recipe in recipes {
                for slot in &recipe.slots {
                    for option in slot {
                        if by_output.contains_key(option) {
                            option_to_parents
                                .entry(option.clone())
                                .or_default()
                                .insert(output.clone());
                        } else {
                            resolved.insert(option.clone());
                        }
                    }
                }
            }
        }

        let mut queue: VecDeque<String> = VecDeque::new();
        for output in by_output.keys() {
            if resolved.contains(output) {
                continue;
            }
            if craftable_count(&by_output[output], &resolved).is_some() {
                resolved.insert(output.clone());
                queue.push_back(output.clone());
            }
        }

        while let Some(item) = queue.pop_front() {
            let Some(parents) = option_to_parents.get(&item) else {
                continue;
            };
            let mut newly_resolved = Vec::new();
            for parent in parents {
                if resolved.contains(parent) {
                    continue;
                }
                if craftable_count(&by_output[parent], &resolved).is_some() {
                    newly_resolved.push(parent.clone());
                }
            }
            for parent in newly_resolved {
                resolved.insert(parent.clone());
                queue.push_back(parent);
            }
        }

        let mut result = Vec::new();
        for (output, recipes) in &by_output {
            if let Some(count) = craftable_count(recipes, &resolved) {
                result.push(Item {
                    id: output.clone(),
                    quantity: count.max(1),
                });
            }
        }

        Ok(result)
    }

    pub fn flat(&self) -> HashMap<String, Vec<Item>> {
        let mut result: HashMap<String, Vec<Item>> = HashMap::new();

        for recipe in &self.recipes {
            if let Some(recipe_result) = &recipe.result {
                let item = Item {
                    id: recipe_result.id.clone(),
                    quantity: recipe_result.count,
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

mod tests {
    #[test]
    fn flatten_recipes() {}
}
