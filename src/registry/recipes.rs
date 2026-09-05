//! Crafting ingredients and station transformation definitions.

use super::{ItemId, MaterialVector};

/// A recipe slot requirement: one exact item, or any member of a tag.
#[derive(Clone, Debug)]
pub enum Ingredient {
    One(ItemId),
    Any(Vec<ItemId>),
}

impl Ingredient {
    pub fn matches(&self, item: ItemId) -> bool {
        match self {
            Ingredient::One(i) => *i == item,
            Ingredient::Any(list) => list.contains(&item),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RecipeDef {
    pub w: usize,
    pub h: usize,
    pub pattern: Vec<Option<Ingredient>>,
    pub output: ItemId,
    pub count: u32,
    /// Visible specialist assembly recipe. It participates in the browser,
    /// material graph, and survival census, but ordinary crafting cannot
    /// match it.
    pub station: Option<String>,
    /// Explicitly dispersed/consumed finite mass. The validator requires the
    /// input vector to equal output + byproducts + this vector.
    pub loss: MaterialVector,
    pub byproducts: Vec<(ItemId, u32)>,
    /// Per-player KV key that must read truthy to craft (spec 3.5). `None`
    /// means no tech gate; a recipe unlocked by a `learn_recipe` reward uses
    /// the runtime default `learned:<recipe_id>`.
    pub tech: Option<String>,
    /// Item consumed from the player's inventory (not the grid) on a
    /// successful craft (spec 3.5). Counts as extra input in the material
    /// graph.
    pub blueprint: Option<ItemId>,
}

#[derive(Clone, Debug)]
pub struct SmeltDef {
    pub input: Ingredient,
    pub output: ItemId,
    pub time: f32,
    /// Byproduct spat out the furnace mouth as item drops (cupellation:
    /// the silver stays in the slot, the lead pours out).
    pub spit: Option<(ItemId, u32)>,
    pub loss: MaterialVector,
}

#[derive(Clone, Debug)]
pub struct ForgeSalvageDef {
    pub input: ItemId,
    pub output: ItemId,
    pub byproduct: ItemId,
    pub recovery_permille: u16,
}

/// A bloomery batch chain: charge + fuel fire into blooms.
#[derive(Clone, Debug)]
pub struct BloomeryDef {
    pub charge: ItemId,
    pub fuel: ItemId,
    pub bloom: ItemId,
}

/// Station work: beat or grind an input into its output over N
/// strikes/turns at a block whose `interaction` matches `station`.
#[derive(Clone, Debug)]
pub struct WorkedDef {
    pub input: ItemId,
    pub output: ItemId,
    pub strikes: u32,
    pub station: String,
    pub needs_hammer: bool,
    pub count: u32,
    pub loss: MaterialVector,
}

#[derive(Clone, Debug)]
pub struct KilnDef {
    pub powder: ItemId,
    pub glass: ItemId,
    pub consumes: bool,
}

