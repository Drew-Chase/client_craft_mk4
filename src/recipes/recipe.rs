use crate::recipes::recipe_type::RecipeType;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Debug, Clone)]
pub struct Recipe {
    #[serde(rename = "type")]
    pub recipe_type: RecipeType,
    pub group: Option<String>,
    pub key: Option<HashMap<String, StringOrArray>>,
    pub ingredients: Option<Vec<StringOrArray>>,
    pub pattern: Option<StringOrArray>,
    pub result: Option<RecipeResult>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum StringOrArray {
    String(String),
    Array(Vec<String>),
}

#[derive(Deserialize, Debug, Clone)]
pub struct RecipeResult {
    pub id: String,
    #[serde(default)]
    pub count: u8,
}

mod tests {
    #[test]
    fn parse_recipe_files() {
        use super::Recipe;
        let mut items = 0;
        for file in std::fs::read_dir(env!("TEST_RECIPE_DIRECTORY")).unwrap() {
            let file = file.unwrap();
            let content = std::fs::read_to_string(file.path()).unwrap();
            if let Err(e) = serde_json::from_str::<Recipe>(&content) {
                eprintln!("{}", content);
                panic!("{}", e)
            }
            items += 1;
        }
        println!("All {items} recipes successfully were parsed!");
    }
}
