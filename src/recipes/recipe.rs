use crate::recipes::recipe_type::RecipeType;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(thiserror::Error, Debug)]
pub enum RecipeError {
    #[error(transparent)]
    SerializationError(#[from] serde_json::Error),
    #[error(transparent)]
    IOError(#[from] std::io::Error),
}

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

impl Recipe {
    pub fn load_from_filesystem(recipes_dir: impl AsRef<Path>) -> Result<Vec<Self>, RecipeError> {
        let recipes_dir = recipes_dir.as_ref();
        let mut items = vec![];
        for file in std::fs::read_dir(recipes_dir)? {
            let file = file?;
            let content = std::fs::read_to_string(file.path())?;
            let item = serde_json::from_str::<Recipe>(&content)?;
            items.push(item);
        }
        Ok(items)
    }
}

mod tests {
    #[test]
    fn parse_recipe_files() {
        use super::Recipe;
        let items = Recipe::load_from_filesystem(env!("TEST_RECIPE_DIRECTORY"))
            .unwrap()
            .len();
        println!("All {items} recipes successfully were parsed!");
    }
}
