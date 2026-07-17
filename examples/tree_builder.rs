use ccmk4::recipes::recipe::Recipe;
use ccmk4::recipes::recipe_type::RecipeType;
use ccmk4::recipes::tags::Tags;
use ccmk4::recipes::tree_builder::TreeBuilder;
use ccmk4::recipes::Item;

fn main() {
    let recipes = Recipe::load_from_filesystem(env!("TEST_RECIPE_DIRECTORY")).unwrap();
    let recipes = recipes
        .iter()
        .filter_map(|i| {
            if i.recipe_type == RecipeType::CraftingShapeless || i.recipe_type == RecipeType::CraftingShaped
            {
                Some(i.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let items: Vec<Item> = serde_json::from_str(
        std::fs::read_to_string(env!("TEST_FAKE_INVENTORY_FILE"))
            .unwrap()
            .as_str(),
    )
    .unwrap();
    let tags = Tags::load_from_filesystem(env!("TEST_TAGS_DIRECTORY")).unwrap();
    let tree = TreeBuilder::new(recipes.clone());
    let results = tree.build(items.clone(), &tags).unwrap();
    println!("{} items", results.len());
    // Regenerate the reference dump the test compares against, from the current
    // algorithm + extracted recipe/tag version (the Java-generated dump was stale).
    std::fs::write(
        "./target/clientcraft_recipes_dump.json",
        serde_json::to_string_pretty(&results).unwrap(),
    )
    .unwrap();
}
