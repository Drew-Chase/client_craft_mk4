use crate::recipes::recipe_type::RecipeType;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Debug, Clone)]
pub struct Recipe {
    #[serde(rename = "type")]
    pub recipe_type: RecipeType,
    pub group: Option<String>,
    pub key: Option<HashMap<String, KeyValue>>,
    pub ingredients: Option<Vec<String>>,
    pub pattern: Option<Vec<String>>,
    pub result: RecipeResult,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum KeyValue{
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
    use super::*;
    #[test]
    fn parse_recipe_files() {
        for file in std::fs::read_dir("./benches/recipes").unwrap() {
            let file = file.unwrap();
            let content = std::fs::read_to_string(file.path()).unwrap();
            if let Err(e) = serde_json::from_str::<Recipe>(&content) {
                eprintln!("{}", content);
                panic!("{}", e)
            }
        }
    }
}
