//! Atlas generation configuration, cancellation, and progress events.

use crate::planet_atlas::{ATLAS_FACE_SIDE};
use std::sync::{Arc};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationMode {
    Serial,
    Parallel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AtlasStage {
    Topology,
    Tectonics,
    Elevation,
    Climate,
    Drainage,
    Hydrology,
    Ground,
    Biomes,
    Resources,
    Validation,
}

impl AtlasStage {
    pub const ALL: [Self; 10] = [
        Self::Topology,
        Self::Tectonics,
        Self::Elevation,
        Self::Climate,
        Self::Drainage,
        Self::Hydrology,
        Self::Ground,
        Self::Biomes,
        Self::Resources,
        Self::Validation,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::Topology => "topology",
            Self::Tectonics => "tectonics_crust",
            Self::Elevation => "preliminary_elevation",
            Self::Climate => "climate_normals_winds",
            Self::Drainage => "erosion_basins_drainage",
            Self::Hydrology => "hydrological_equilibrium",
            Self::Ground => "soils_groundwater_habitats",
            Self::Biomes => "biomes_provinces",
            Self::Resources => "finite_resource_sites",
            Self::Validation => "validation",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Topology => "SHAPING PLANET",
            Self::Tectonics => "RAISING CONTINENTS",
            Self::Elevation => "RAISING CONTINENTS",
            Self::Climate => "MOVING AIR",
            Self::Drainage | Self::Hydrology => "FINDING THE WATERS",
            Self::Ground => "LAYING THE GROUND",
            Self::Biomes | Self::Resources => "WAKING THE COUNTRIES",
            Self::Validation => "PROVING THE PLANET",
        }
    }
}

#[derive(Clone, Debug)]
pub struct AtlasProgress {
    pub stage: AtlasStage,
    pub completed_stages: usize,
    pub total_stages: usize,
}

#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AtlasConfig {
    pub side: u16,
    pub mode: GenerationMode,
}

impl AtlasConfig {
    pub const fn production() -> Self {
        Self {
            side: ATLAS_FACE_SIDE,
            mode: GenerationMode::Parallel,
        }
    }

    #[cfg(test)]
    pub const fn fixture(side: u16) -> Self {
        Self {
            side,
            mode: GenerationMode::Serial,
        }
    }
}
