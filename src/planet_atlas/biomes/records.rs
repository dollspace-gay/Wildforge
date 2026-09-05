//! Persisted country routes and soil/composition summaries.

use crate::planet_atlas::{AtlasPos};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CountrySoilComposition {
    pub mean_depth_decimeters: u8,
    pub mean_sand: u8,
    pub mean_silt: u8,
    pub mean_clay: u8,
    pub mean_organic: u8,
    pub mean_fertility: u8,
    pub mean_drainage: u8,
    pub mean_salinity: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CountryRoute {
    pub neighbor_id: u16,
    pub pass: AtlasPos,
    /// Compact relative crossing cost; lower values are easier routes.
    pub barrier: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CountryRecord {
    pub id: u16,
    pub name_seed: u64,
    pub seed_site: AtlasPos,
    pub heart_site: AtlasPos,
    pub dominant_biome: u8,
    pub heart_form: u8,
    pub principal_watershed: u32,
    pub principal_landmass: u16,
    pub cell_count: u32,
    pub physical_area: f64,
    pub habitat_cells: BTreeMap<String, u32>,
    pub biome_cells: BTreeMap<String, u32>,
    pub soil: CountrySoilComposition,
    pub routes: Vec<CountryRoute>,
}
