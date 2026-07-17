use ccmk4::recipes::recipe::Recipe;
use ccmk4::recipes::tree_builder::TreeBuilder;
use rand::RngExt;
use serde_json::{json, Value};

fn main() {
    const MAX_RECIPES: usize = 4 * 9;
    let recipes: Vec<Recipe> = Recipe::load_from_filesystem(env!("TEST_RECIPE_DIRECTORY")).unwrap();
    let length = recipes.len();
    let mut random_recipes: Vec<Value> = vec![];
    let tree = TreeBuilder::new(recipes);
    let items = tree.items().into_iter().collect::<Vec<_>>();
    let mut rng = rand::rng();
    let mut index = 0;
    loop {
        let random_index = rng.random_range(0..length);
        let quantity = rng.random_range(1..64);
        if let Some(random_recipe) = items.get(random_index) {
            let item: Value = json!({
                "id": random_recipe.clone(),
                "quantity": quantity,
            });
            random_recipes.push(item);
            index += 1;
        }
        if index >= MAX_RECIPES {
            break;
        }
    }
    std::fs::write(
        env!("TEST_FAKE_INVENTORY_FILE"),
        serde_json::to_string_pretty(&random_recipes).unwrap(),
    )
    .unwrap();
}
