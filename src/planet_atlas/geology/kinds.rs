//! Stable tectonic, basin, and bedrock classifications used by the atlas codec.

use serde::{Deserialize, Serialize};
use crate::planet_atlas::{AtlasError, BoundaryClass};

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum DetailedBoundary {
    #[default]
    Interior = 0,
    ContinentalCollision = 1,
    OceanContinentSubduction = 2,
    OceanOceanSubduction = 3,
    ContinentalRift = 4,
    OceanRidge = 5,
    Transform = 6,
    PassiveWeak = 7,
}

impl DetailedBoundary {
    pub(in crate::planet_atlas) fn from_u8(value: u8) -> Result<Self, AtlasError> {
        match value {
            0 => Ok(Self::Interior),
            1 => Ok(Self::ContinentalCollision),
            2 => Ok(Self::OceanContinentSubduction),
            3 => Ok(Self::OceanOceanSubduction),
            4 => Ok(Self::ContinentalRift),
            5 => Ok(Self::OceanRidge),
            6 => Ok(Self::Transform),
            7 => Ok(Self::PassiveWeak),
            _ => Err(AtlasError::Corrupt(format!(
                "unknown detailed boundary classification {value}"
            ))),
        }
    }

    pub const fn summary(self) -> BoundaryClass {
        match self {
            Self::Interior => BoundaryClass::Interior,
            Self::ContinentalCollision
            | Self::OceanContinentSubduction
            | Self::OceanOceanSubduction => BoundaryClass::Convergent,
            Self::ContinentalRift | Self::OceanRidge => BoundaryClass::Divergent,
            Self::Transform => BoundaryClass::Transform,
            Self::PassiveWeak => BoundaryClass::Interior,
        }
    }

    pub const fn is_volcanic(self) -> bool {
        matches!(
            self,
            Self::OceanContinentSubduction
                | Self::OceanOceanSubduction
                | Self::ContinentalRift
                | Self::OceanRidge
        )
    }
}

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum BasinKind {
    #[default]
    None = 0,
    MarineShelf = 1,
    DeepMarine = 2,
    Foreland = 3,
    Rift = 4,
    Closed = 5,
    PassiveMargin = 6,
}

impl BasinKind {
    pub(in crate::planet_atlas) fn from_u8(value: u8) -> Result<Self, AtlasError> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::MarineShelf),
            2 => Ok(Self::DeepMarine),
            3 => Ok(Self::Foreland),
            4 => Ok(Self::Rift),
            5 => Ok(Self::Closed),
            6 => Ok(Self::PassiveMargin),
            _ => Err(AtlasError::Corrupt(format!(
                "unknown sediment basin {value}"
            ))),
        }
    }
}

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
#[repr(u16)]
#[serde(rename_all = "snake_case")]
pub enum BedrockFamily {
    #[default]
    MixedBasement = 0,
    Sandstone = 1,
    Limestone = 2,
    Shale = 3,
    Granite = 4,
    Marble = 5,
    Slate = 6,
    Quartzite = 7,
    Basalt = 8,
    Ultramafic = 9,
    Evaporite = 10,
}

impl BedrockFamily {
    pub fn from_id(value: u16) -> Self {
        match value {
            1 => Self::Sandstone,
            2 => Self::Limestone,
            3 => Self::Shale,
            4 => Self::Granite,
            5 => Self::Marble,
            6 => Self::Slate,
            7 => Self::Quartzite,
            8 => Self::Basalt,
            9 => Self::Ultramafic,
            10 => Self::Evaporite,
            _ => Self::MixedBasement,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::MixedBasement => "mixed basement",
            Self::Sandstone => "sandstone",
            Self::Limestone => "limestone",
            Self::Shale => "shale",
            Self::Granite => "granite",
            Self::Marble => "marble",
            Self::Slate => "slate",
            Self::Quartzite => "quartzite",
            Self::Basalt => "basalt",
            Self::Ultramafic => "ultramafic rock",
            Self::Evaporite => "evaporite",
        }
    }
}
