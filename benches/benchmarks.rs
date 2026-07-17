use ccmk4::recipes::recipe::Recipe;
use ccmk4::recipes::tree_builder::TreeBuilder;
use ccmk4::recipes::Item;
use divan::{AllocProfiler, Bencher};

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

fn main() {
    divan::main();
}

#[divan::bench]
fn builder(bencher: Bencher) {
    let recipes = Recipe::load_from_filesystem(env!("TEST_RECIPE_DIRECTORY")).unwrap();
    let items: Vec<Item> = serde_json::from_str(
        std::fs::read_to_string(env!("TEST_FAKE_INVENTORY_FILE"))
            .unwrap()
            .as_str(),
    )
    .unwrap();
    bencher
        .with_inputs(|| (recipes.clone(), items.clone()))
        .bench_refs(|(recipes, items)| {
            let tree = TreeBuilder::new(recipes.clone());
            tree.build(items.clone()).unwrap();
        });
}
