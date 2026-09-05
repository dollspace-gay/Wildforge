//! Item identities, equipment, nutrition, and runtime item definitions.

use super::{ArcaneContentDef, ArcaneEcologyDef, BlockId, DiscoveryItemDef, MaterialClass, MaterialVector, ObservationDef, SalvageDef};
use serde::{Deserialize};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ItemId(pub u16);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolKind {
    Pickaxe,
    Axe,
    Shovel,
    Hoe,
}

pub const NUTRIENTS: [&str; 5] = ["grain", "vegetable", "fruit", "fungi", "protein"];

#[derive(Clone, Debug)]
pub struct FoodDef {
    pub hunger: f32,
    pub eat_time: f32,
    pub nutrition: [f32; 5],
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ArmorSlot {
    Head = 0,
    Chest = 1,
    Legs = 2,
    Feet = 3,
}

impl ArmorSlot {
    pub fn parse(s: &str) -> Option<ArmorSlot> {
        match s {
            "head" => Some(ArmorSlot::Head),
            "chest" => Some(ArmorSlot::Chest),
            "legs" => Some(ArmorSlot::Legs),
            "feet" => Some(ArmorSlot::Feet),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BowDef {
    /// Damage at full charge (half-hearts).
    pub damage: f32,
    /// Arrow velocity at full charge.
    pub speed: f32,
}

#[derive(Clone, Debug)]
pub struct ItemDef {
    pub name: String,
    pub label: String,
    pub icon: u16,
    pub max_stack: u32,
    /// (kind, speed multiplier on matching blocks, tier)
    pub tool: Option<(ToolKind, f32, u8)>,
    pub durability: u32,
    /// Placing this item puts down this block.
    pub places: Option<BlockId>,
    pub food: Option<FoodDef>,
    /// Attack damage in half-hearts (swords set it high; tools get a
    /// modest implicit value, bare items 1).
    pub damage: f32,
    /// Damage class wielded against the wild ("pierce", "blunt", "fire"...).
    pub damage_type: Option<String>,
    pub bow: Option<BowDef>,
    /// Ammo class this item belongs to ("arrow"); bows consume it.
    pub ammo: Option<String>,
    /// (slot, armor points) — each point blocks 4% damage from the wild.
    pub armor: Option<(ArmorSlot, u32)>,
    /// Weight in carry units (default 1). Survival pickup respects the
    /// player's Carry capacity; 0 means weightless.
    pub carry_weight: u32,
    /// Derived stat contributions granted while worn (equipment only).
    pub stats: Vec<crate::stats::StatModifier>,
    /// Modular equipment (E6): this item is a frame whose typed slots take
    /// components. Frames disable at durability 0 instead of being
    /// destroyed (repairable via the crafting repair path).
    pub frame: Option<crate::equipment::FrameDef>,
    /// Modular equipment (E6): the slot type this item fills when slotted
    /// into a frame that declares it.
    pub component: Option<String>,
    /// Right-click to camp: sleep to dawn, set spawn (bedrolls).
    pub bedroll: bool,
    /// Breaking leaves with this drops the leaf block itself.
    pub shears: bool,
    /// Passive charm effect: "quiet" | "bark" | "hunger" (one charm slot).
    pub charm: Option<String>,
    /// Bounded authoritative charm behavior. Legacy string declarations are
    /// upgraded into this form during registry load.
    pub charm_def: Option<crate::implements::CharmDef>,
    /// One physical role in a component-built wand.
    pub wand_component: Option<crate::implements::WandComponentDef>,
    /// A finished implement shell whose per-instance state lives in the
    /// world's implements sidecar.
    pub implement: Option<crate::implements::ImplementItemDef>,
    /// Right-click reads a line from the lost takers.
    pub tablet: bool,
    /// Right-click to set light to something. The one place a fire
    /// is marked as a player's.
    pub striker: bool,
    /// Reachable only from the creative browser. Every block gets one
    /// of these so a builder can place lava, fire, a heart, a crop
    /// mid-growth or a fluid at any level — the states a survival
    /// player meets in the world but can never hold.
    pub creative_only: bool,
    /// Sweeps remnant blocks (archaeology).
    pub brush_tool: bool,
    /// Right-click throw speed (None = not throwable).
    pub throw_speed: Option<f32>,
    /// Carried-light color x intensity (a glowing item that isn't a
    /// placeable lamp, e.g. a raw ember). Placeable emitters glow
    /// automatically from their block's light.
    pub glow: Option<[f32; 3]>,
    /// Works blooms on an anvil.
    pub hammer: bool,
    /// Right-click disables (hacks) a construct instead of destroying it.
    pub hack: bool,
    /// Recoverable finite constituents per item, in canonical integer units.
    pub materials: MaterialVector,
    /// True only when the content file fixes the vector. Derived vectors may
    /// be recomputed as upstream recipe identities reach their fixed point.
    pub materials_declared: bool,
    pub material_class: MaterialClass,
    pub salvage: Option<SalvageDef>,
    /// A zero-durability finite item changes identity instead of vanishing.
    pub broken_into: Option<ItemId>,
    pub arcane: Option<ArcaneContentDef>,
    pub arcane_ecology: Option<ArcaneEcologyDef>,
    pub observation: Option<ObservationDef>,
    pub discovery: Option<DiscoveryItemDef>,
}

