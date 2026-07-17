use serde::{Deserialize, Serialize};

pub mod recipe;
pub mod recipe_type;
pub mod tree_builder;

#[derive(Serialize, Deserialize, Clone)]
pub struct Item {
    pub id: String,
    pub quantity: u8,
}
