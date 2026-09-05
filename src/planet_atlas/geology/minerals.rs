//! Stable mineral identities and the closed volcanic/magmatic classification set.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum MineralKind {
    Copper = 1,
    Tin = 2,
    Iron = 3,
    Cobalt = 4,
    Cinnabar = 5,
    Manganese = 6,
    Coal = 7,
    Gold = 8,
    Galena = 9,
    Chromite = 10,
    Diamond = 11,
    RareEarth = 12,
    Halite = 13,
    Pitchblende = 14,
    Geode = 15,
    Other = 255,
}

impl MineralKind {
    pub const ALL_TRACKED: [Self; 15] = [
        Self::Copper,
        Self::Tin,
        Self::Iron,
        Self::Cobalt,
        Self::Cinnabar,
        Self::Manganese,
        Self::Coal,
        Self::Gold,
        Self::Galena,
        Self::Chromite,
        Self::Diamond,
        Self::RareEarth,
        Self::Halite,
        Self::Pitchblende,
        Self::Geode,
    ];

    pub fn from_block_name(name: &str) -> Self {
        if name.contains("copper") {
            Self::Copper
        } else if name.contains("tin") {
            Self::Tin
        } else if name.contains("iron") {
            Self::Iron
        } else if name.contains("cobalt") {
            Self::Cobalt
        } else if name.contains("cinnabar") {
            Self::Cinnabar
        } else if name.contains("manganese") {
            Self::Manganese
        } else if name.contains("coal") {
            Self::Coal
        } else if name.contains("quartz_vein")
            || name.contains("quartz_block")
            || name.contains("sulfur_crystal")
            || name.contains("amethyst")
        {
            Self::Geode
        } else if name.contains("gold") {
            Self::Gold
        } else if name.contains("galena") {
            Self::Galena
        } else if name.contains("chromite") {
            Self::Chromite
        } else if name.contains("diamond") {
            Self::Diamond
        } else if name.contains("monazite") || name.contains("bastnasite") {
            Self::RareEarth
        } else if name.contains("halite") {
            Self::Halite
        } else if name.contains("pitchblende") {
            Self::Pitchblende
        } else {
            Self::Other
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Copper => "copper",
            Self::Tin => "tin",
            Self::Iron => "iron",
            Self::Cobalt => "cobalt",
            Self::Cinnabar => "cinnabar",
            Self::Manganese => "manganese",
            Self::Coal => "coal",
            Self::Gold => "gold",
            Self::Galena => "galena",
            Self::Chromite => "chromite",
            Self::Diamond => "diamond",
            Self::RareEarth => "rare earth",
            Self::Halite => "halite",
            Self::Pitchblende => "pitchblende",
            Self::Geode => "geode",
            Self::Other => "modded mineral",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VolcanoSource {
    ContinentalArc,
    IslandArc,
    Rift,
    OceanRidge,
    Hotspot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MagmaChemistry {
    Basaltic,
    Andesitic,
    Rhyolitic,
    Carbonatitic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntrusionKind {
    Batholith,
    Pluton,
    Carbonatite,
    DikeSwarm,
}
