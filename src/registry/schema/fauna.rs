//! Raw fauna content schema; no runtime mutation.

use super::{ArcaneContentToml};
use serde::Deserialize;
use std::collections::{HashMap};

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct BoxToml {
    pub(in crate::registry) size: [f32; 3],
    pub(in crate::registry) at: [f32; 3],
    #[serde(default)]
    pub(in crate::registry) tex: Option<String>,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct AnimalDropToml {
    pub(in crate::registry) item: String,
    #[serde(default)]
    pub(in crate::registry) min: Option<u32>,
    #[serde(default)]
    pub(in crate::registry) max: Option<u32>,
}

/// One damage-class multiplier: `mult = 0.5` halves that class, `mult = 2.0`
/// doubles it. Absent classes pass untouched (full damage).
#[derive(Deserialize, Clone)]
pub(in crate::registry) struct ResistToml {
    #[serde(rename = "type")]
    pub(in crate::registry) kind: String,
    pub(in crate::registry) mult: f32,
}

/// `resist` accepts a single table or a list of tables.
#[derive(Deserialize, Clone)]
#[serde(untagged)]
pub(in crate::registry) enum ResistTomlList {
    One(ResistToml),
    Many(Vec<ResistToml>),
}

impl ResistTomlList {
    pub(in crate::registry) fn resolved(&self) -> HashMap<String, f32> {
        match self {
            Self::One(r) => {
                let mut map = HashMap::new();
                map.insert(r.kind.clone(), r.mult);
                map
            }
            Self::Many(list) => list.iter().map(|r| (r.kind.clone(), r.mult)).collect(),
        }
    }
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct AnimalToml {
    pub(in crate::registry) id: String,
    #[serde(default)]
    pub(in crate::registry) name: Option<String>,
    pub(in crate::registry) biomes: Vec<String>,
    #[serde(default)]
    pub(in crate::registry) habitats: Vec<String>,
    #[serde(default)]
    pub(in crate::registry) temperature_c: Option<[f32; 2]>,
    #[serde(default)]
    pub(in crate::registry) vegetation: Option<[u8; 2]>,
    #[serde(default)]
    pub(in crate::registry) elevation: Option<[i16; 2]>,
    #[serde(default)]
    pub(in crate::registry) health: Option<f32>,
    #[serde(default)]
    pub(in crate::registry) speed: Option<f32>,
    #[serde(default)]
    pub(in crate::registry) flee_range: Option<f32>,
    #[serde(default)]
    pub(in crate::registry) group: Option<[u32; 2]>,
    #[serde(default)]
    pub(in crate::registry) rarity: Option<u32>,
    pub(in crate::registry) tex: String,
    #[serde(default)]
    pub(in crate::registry) head_tex: Option<String>,
    #[serde(default)]
    pub(in crate::registry) sound_pitch: Option<f32>,
    #[serde(default)]
    pub(in crate::registry) drops: Vec<AnimalDropToml>,
    /// Damage-class multipliers ("fire", "pierce"...); see ResistTomlList.
    #[serde(default)]
    pub(in crate::registry) resist: Option<ResistTomlList>,
    /// Authored attack wheel; empty = the `attack` scalar becomes a
    /// single implicit melee (full back-compat).
    #[serde(default)]
    pub(in crate::registry) attacks: Vec<AttackToml>,
    /// Archetype: "standard" (default), "brute", "construct", "builder".
    #[serde(default)]
    pub(in crate::registry) behavior: Option<String>,
    /// Builder options (spec 3.6): template name, stamp cap, interval.
    #[serde(default)]
    pub(in crate::registry) builder: Option<BuilderToml>,
    /// Construct hack options (spec 3.6): tool tag + core drops.
    #[serde(default)]
    pub(in crate::registry) hack: Option<HackToml>,
    /// E9 archetype knobs (optional; engine defaults apply when omitted).
    #[serde(default)]
    pub(in crate::registry) rusher: Option<RusherToml>,
    #[serde(default)]
    pub(in crate::registry) tank: Option<TankToml>,
    #[serde(default)]
    pub(in crate::registry) sniper: Option<SniperToml>,
    #[serde(default)]
    pub(in crate::registry) support: Option<SupportToml>,
    #[serde(default)]
    pub(in crate::registry) swarm: Option<SwarmToml>,
    #[serde(default)]
    pub(in crate::registry) controller: Option<ControllerToml>,
    #[serde(default)]
    pub(in crate::registry) phaser: Option<PhaserToml>,
    #[serde(default)]
    pub(in crate::registry) shield: Option<ShieldToml>,
    #[serde(default)]
    pub(in crate::registry) model: HashMap<String, BoxToml>,
    #[serde(default)]
    pub(in crate::registry) hostile: bool,
    #[serde(default)]
    pub(in crate::registry) attack: Option<f32>,
    #[serde(default)]
    pub(in crate::registry) aggro_range: Option<f32>,
    #[serde(default)]
    pub(in crate::registry) ire_min: Option<f32>,
    #[serde(default)]
    pub(in crate::registry) movement: Option<String>,
    #[serde(default)]
    pub(in crate::registry) aquatic: Option<AquaticHabitatToml>,
    #[serde(default)]
    pub(in crate::registry) emissive: bool,
    #[serde(default)]
    pub(in crate::registry) glow: Option<[f32; 3]>,
    #[serde(default)]
    pub(in crate::registry) spawn_light_max: Option<u8>,
    #[serde(default)]
    pub(in crate::registry) projectile: Option<ProjectileToml>,
    #[serde(default)]
    pub(in crate::registry) breed_food: Option<String>,
    #[serde(default)]
    pub(in crate::registry) carrier: bool,
    #[serde(default)]
    pub(in crate::registry) belly: Option<f32>,
    #[serde(default)]
    pub(in crate::registry) grazes: bool,
    #[serde(default)]
    pub(in crate::registry) prey: Vec<String>,
    #[serde(default)]
    pub(in crate::registry) fierce: bool,
    #[serde(default)]
    pub(in crate::registry) guards: bool,
    #[serde(default)]
    pub(in crate::registry) vehicle: bool,
    #[serde(default)]
    pub(in crate::registry) arcane: Option<ArcaneContentToml>,
}

#[derive(Deserialize, Clone, Default)]
pub(in crate::registry) struct AquaticHabitatToml {
    #[serde(default)]
    pub(in crate::registry) temperature_c: Option<[f32; 2]>,
    #[serde(default)]
    pub(in crate::registry) depth_blocks: Option<[u8; 2]>,
    #[serde(default)]
    pub(in crate::registry) discharge: Option<[f32; 2]>,
    #[serde(default)]
    pub(in crate::registry) salinity: Option<[u8; 2]>,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct ProjectileToml {
    pub(in crate::registry) tex: String,
    pub(in crate::registry) damage: f32,
    #[serde(default)]
    pub(in crate::registry) damage_type: Option<String>,
    #[serde(default)]
    pub(in crate::registry) speed: Option<f32>,
    #[serde(default)]
    pub(in crate::registry) cooldown: Option<f32>,
}

/// One authored entry in a species' attack wheel (spec 3.6).
#[derive(Deserialize, Clone)]
pub(in crate::registry) struct AttackToml {
    #[serde(default)]
    pub(in crate::registry) name: Option<String>,
    /// "melee" | "charge" | "projectile".
    pub(in crate::registry) kind: String,
    #[serde(default)]
    pub(in crate::registry) damage: Option<f32>,
    #[serde(default)]
    pub(in crate::registry) cooldown: Option<f32>,
    /// Trigger distance in blocks; defaults to melee reach for melee,
    /// the cast range for projectile attacks.
    #[serde(default)]
    pub(in crate::registry) range: Option<f32>,
    #[serde(default)]
    pub(in crate::registry) damage_type: Option<String>,
    #[serde(default)]
    pub(in crate::registry) projectile: Option<ProjectileToml>,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct BuilderToml {
    #[serde(default)]
    pub(in crate::registry) template: Option<String>,
    #[serde(default)]
    pub(in crate::registry) cap: Option<u32>,
    #[serde(default)]
    pub(in crate::registry) interval: Option<f32>,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct HackToml {
    #[serde(default)]
    pub(in crate::registry) tool: Option<String>,
    #[serde(default)]
    pub(in crate::registry) drops: Vec<AnimalDropToml>,
}

#[derive(Deserialize, Clone, Default)]
pub(in crate::registry) struct RusherToml {
    #[serde(default)]
    pub(in crate::registry) rush_mult: Option<f32>,
}

#[derive(Deserialize, Clone, Default)]
pub(in crate::registry) struct TankToml {
    #[serde(default)]
    pub(in crate::registry) knockback_mult: Option<f32>,
}

#[derive(Deserialize, Clone, Default)]
pub(in crate::registry) struct SniperToml {
    #[serde(default)]
    pub(in crate::registry) keep_min: Option<f32>,
    #[serde(default)]
    pub(in crate::registry) keep_max: Option<f32>,
}

#[derive(Deserialize, Clone, Default)]
pub(in crate::registry) struct SupportToml {
    #[serde(default)]
    pub(in crate::registry) radius: Option<f32>,
    #[serde(default)]
    pub(in crate::registry) interval: Option<f32>,
    #[serde(default)]
    pub(in crate::registry) heal: Option<f32>,
}

#[derive(Deserialize, Clone, Default)]
pub(in crate::registry) struct SwarmToml {
    #[serde(default)]
    pub(in crate::registry) spawn: Option<String>,
    #[serde(default)]
    pub(in crate::registry) count: Option<u32>,
}

#[derive(Deserialize, Clone, Default)]
pub(in crate::registry) struct ControllerToml {
    #[serde(default)]
    pub(in crate::registry) spawn: Option<String>,
    #[serde(default)]
    pub(in crate::registry) count: Option<u32>,
    #[serde(default)]
    pub(in crate::registry) interval: Option<f32>,
    #[serde(default)]
    pub(in crate::registry) max: Option<u32>,
}

#[derive(Deserialize, Clone, Default)]
pub(in crate::registry) struct PhaserToml {
    #[serde(default)]
    pub(in crate::registry) blink_range: Option<f32>,
    #[serde(default)]
    pub(in crate::registry) blink_cd: Option<f32>,
}

#[derive(Deserialize, Clone, Default)]
pub(in crate::registry) struct ShieldToml {
    #[serde(default)]
    pub(in crate::registry) front_mult: Option<f32>,
    #[serde(default)]
    pub(in crate::registry) front_deg: Option<f32>,
}

