//! Raw items content schema; no runtime mutation.

use super::{ArcaneContentToml, ArcaneEcologyToml, DiscoveryItemToml, ObservationToml, one_u8};
use crate::registry::{MaterialClass, MaterialVector, ToolKind};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct FoodToml {
    pub(in crate::registry) hunger: f32,
    #[serde(default)]
    pub(in crate::registry) eat_time: Option<f32>,
    #[serde(default)]
    pub(in crate::registry) nutrition: HashMap<String, f32>,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct ItemToml {
    pub(in crate::registry) id: String,
    pub(in crate::registry) name: Option<String>,
    pub(in crate::registry) texture: String,
    #[serde(default)]
    pub(in crate::registry) max_stack: Option<u32>,
    #[serde(default)]
    pub(in crate::registry) tool: Option<ToolKind>,
    #[serde(default)]
    pub(in crate::registry) tool_speed: Option<f32>,
    #[serde(default)]
    pub(in crate::registry) tool_tier: Option<u8>,
    #[serde(default)]
    pub(in crate::registry) durability: Option<u32>,
    #[serde(default)]
    pub(in crate::registry) food: Option<FoodToml>,
    #[serde(default)]
    pub(in crate::registry) places: Option<String>,
    #[serde(default)]
    pub(in crate::registry) damage: Option<f32>,
    /// Player-wielded damage class ("pierce", "blunt", "fire"...); the
    /// wild's resistances key on it. None = untyped (always full).
    #[serde(default)]
    pub(in crate::registry) damage_type: Option<String>,
    #[serde(default)]
    pub(in crate::registry) bow: Option<BowToml>,
    #[serde(default)]
    pub(in crate::registry) ammo: Option<String>,
    #[serde(default)]
    pub(in crate::registry) armor: Option<ArmorToml>,
    #[serde(default)]
    pub(in crate::registry) carry_weight: Option<u32>,
    #[serde(default)]
    pub(in crate::registry) stats: Vec<StatToml>,
    /// Modular equipment (E6): typed slots this frame accepts.
    #[serde(default)]
    pub(in crate::registry) frame: Option<FrameToml>,
    /// Modular equipment (E6): the slot type this component fills.
    #[serde(default)]
    pub(in crate::registry) component: Option<String>,
    #[serde(default)]
    pub(in crate::registry) bedroll: bool,
    #[serde(default)]
    pub(in crate::registry) shears: bool,
    #[serde(default)]
    pub(in crate::registry) charm: Option<CharmToml>,
    #[serde(default)]
    pub(in crate::registry) wand_component: Option<crate::implements::WandComponentDef>,
    #[serde(default)]
    pub(in crate::registry) implement: Option<crate::implements::ImplementItemDef>,
    #[serde(default)]
    pub(in crate::registry) tablet: bool,
    #[serde(default)]
    pub(in crate::registry) striker: bool,
    #[serde(default)]
    pub(in crate::registry) brush_tool: bool,
    /// Right-click throw: projectile speed (snowballs).
    #[serde(default)]
    pub(in crate::registry) throw: Option<ThrowToml>,
    /// Works blooms on an anvil.
    #[serde(default)]
    pub(in crate::registry) hammer: bool,
    /// Right-click disables (hacks) a construct instead of destroying it.
    #[serde(default)]
    pub(in crate::registry) hack: bool,
    /// Carried-light color for non-placeable glowing items.
    #[serde(default)]
    pub(in crate::registry) glow: Option<[f32; 3]>,
    #[serde(default)]
    pub(in crate::registry) material_class: Option<MaterialClass>,
    #[serde(default)]
    pub(in crate::registry) materials: MaterialVector,
    #[serde(default)]
    pub(in crate::registry) salvage: Option<SalvageToml>,
    #[serde(default)]
    pub(in crate::registry) arcane: Option<ArcaneContentToml>,
    #[serde(default)]
    pub(in crate::registry) arcane_ecology: Option<ArcaneEcologyToml>,
    #[serde(default)]
    pub(in crate::registry) observation: Option<ObservationToml>,
    #[serde(default)]
    pub(in crate::registry) discovery: Option<DiscoveryItemToml>,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct StatToml {
    pub(in crate::registry) kind: String,
    #[serde(default)]
    pub(in crate::registry) flat: Option<f32>,
    #[serde(default)]
    pub(in crate::registry) mult_permille: Option<u16>,
}

#[derive(Deserialize, Clone)]
#[serde(untagged)]
pub(in crate::registry) enum CharmToml {
    Legacy(String),
    Detailed(crate::implements::CharmDef),
}

impl CharmToml {
    pub(in crate::registry) fn effect_id(&self) -> String {
        match self {
            Self::Legacy(effect) => effect.clone(),
            Self::Detailed(definition) => definition.effect.id().into(),
        }
    }

    pub(in crate::registry) fn definition(&self) -> Option<crate::implements::CharmDef> {
        match self {
            Self::Legacy(effect) => {
                let effect = match effect.as_str() {
                    "quiet" => crate::implements::CharmEffect::Quiet,
                    "bark" => crate::implements::CharmEffect::Bark,
                    "hunger" => crate::implements::CharmEffect::Hunger,
                    _ => return None,
                };
                Some(crate::implements::CharmDef {
                    effect,
                    charge_per_trigger: match effect {
                        crate::implements::CharmEffect::Quiet => 2,
                        crate::implements::CharmEffect::Bark => 4,
                        crate::implements::CharmEffect::Hunger => 1,
                    },
                    capacity: 4_096,
                    stability: 800,
                    dross_per_transfer: 25,
                })
            }
            Self::Detailed(definition) => Some(definition.clone()),
        }
    }
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct SalvageToml {
    pub(in crate::registry) station: String,
    pub(in crate::registry) recovery: f32,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct ThrowToml {
    #[serde(default)]
    pub(in crate::registry) speed: Option<f32>,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct BowToml {
    pub(in crate::registry) damage: f32,
    #[serde(default)]
    pub(in crate::registry) speed: Option<f32>,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct ArmorToml {
    pub(in crate::registry) slot: String,
    pub(in crate::registry) points: u32,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct FrameToml {
    pub(in crate::registry) slots: Vec<FrameSlotToml>,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct FrameSlotToml {
    #[serde(rename = "type")]
    pub(in crate::registry) kind: String,
    #[serde(default = "one_u8")]
    pub(in crate::registry) max: u8,
}
