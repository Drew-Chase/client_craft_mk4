use serde::{Deserialize, Serialize};

pub mod recipe;
pub mod recipe_type;
pub mod tags;
pub mod tree_builder;

#[derive(Serialize, Deserialize, Clone)]
pub struct Item {
    pub id: String,
    // Max craftable counts can exceed a u8 (they cap at 999), so this must be wide.
    pub quantity: u32,
}
