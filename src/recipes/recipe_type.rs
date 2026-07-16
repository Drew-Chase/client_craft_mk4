use serde::{Deserialize, Deserializer};

#[derive( Debug, Clone)]
pub enum RecipeType {
    CraftingSpecialMapextending,
    SmithingTransform,
    CraftingDecoratedPot,
    CraftingShapeless,
    CraftingShaped,
    CraftingSpecialBannerduplicate,
    CraftingSpecialRepairitem,
    CraftingTransmute,
    Blasting,
    CampfireCooking,
    CraftingSpecialShielddecoration,
    Stonecutting,
    CraftingImbue,
    SmithingTrim,
    CraftingSpecialFireworkRocket,
    Smoking,
    CraftingSpecialFireworkStar,
    Smelting,
    CraftingSpecialBookcloning,
    CraftingSpecialFireworkStarFade,
    CraftingDye,
}

impl From<String> for RecipeType {
    fn from(value: String) -> Self {
        match value.to_lowercase().as_str() {
            "minecraft:crafting_special_mapextending" | "crafting_special_mapextending" => {
                Self::CraftingSpecialMapextending
            }
            "minecraft:smithing_transform" | "smithing_transform" => Self::SmithingTransform,
            "minecraft:crafting_decorated_pot" | "crafting_decorated_pot" => {
                Self::CraftingDecoratedPot
            }
            "minecraft:crafting_shapeless" | "crafting_shapeless" => Self::CraftingShapeless,
            "minecraft:crafting_shaped" | "crafting_shaped" => Self::CraftingShaped,
            "minecraft:crafting_special_bannerduplicate" | "crafting_special_bannerduplicate" => {
                Self::CraftingSpecialBannerduplicate
            }
            "minecraft:crafting_special_repairitem" | "crafting_special_repairitem" => {
                Self::CraftingSpecialRepairitem
            }
            "minecraft:crafting_transmute" | "crafting_transmute" => Self::CraftingTransmute,
            "minecraft:blasting" | "blasting" => Self::Blasting,
            "minecraft:campfire_cooking" | "campfire_cooking" => Self::CampfireCooking,
            "minecraft:crafting_special_shielddecoration" | "crafting_special_shielddecoration" => {
                Self::CraftingSpecialShielddecoration
            }
            "minecraft:stonecutting" | "stonecutting" => Self::Stonecutting,
            "minecraft:crafting_imbue" | "crafting_imbue" => Self::CraftingImbue,
            "minecraft:smithing_trim" | "smithing_trim" => Self::SmithingTrim,
            "minecraft:crafting_special_firework_rocket" | "crafting_special_firework_rocket" => {
                Self::CraftingSpecialFireworkRocket
            }
            "minecraft:smoking" | "smoking" => Self::Smoking,
            "minecraft:crafting_special_firework_star" | "crafting_special_firework_star" => {
                Self::CraftingSpecialFireworkStar
            }
            "minecraft:smelting" | "smelting" => Self::Smelting,
            "minecraft:crafting_special_bookcloning" | "crafting_special_bookcloning" => {
                Self::CraftingSpecialBookcloning
            }
            "minecraft:crafting_special_firework_star_fade"
            | "crafting_special_firework_star_fade" => Self::CraftingSpecialFireworkStarFade,
            "minecraft:crafting_dye" => Self::CraftingDye,

            _ => panic!("Unknown RecipeType"),
        }
    }
}

impl<'de> Deserialize<'de> for RecipeType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                      where
                          D: Deserializer<'de>
    {
        let str_value = String::deserialize(deserializer)?;
        Ok(RecipeType::from(str_value))
    }
}