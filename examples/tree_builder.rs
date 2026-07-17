use ccmk4::recipes::recipe::Recipe;
use ccmk4::recipes::tree_builder::TreeBuilder;
use ccmk4::recipes::Item;

fn main() {
    let recipes = Recipe::load_from_filesystem(env!("TEST_RECIPE_DIRECTORY")).unwrap();
    let items: Vec<Item> = serde_json::from_str(
        std::fs::read_to_string(env!("TEST_FAKE_INVENTORY_FILE"))
            .unwrap()
            .as_str(),
    )
    .unwrap();
    let tree = TreeBuilder::new(recipes.clone());
    let results = tree.build(items.clone()).unwrap();
    println!("{} items", results.len());
    std::fs::write("./target/benchmark.json", serde_json::to_string_pretty(&results).unwrap()).unwrap();

}
