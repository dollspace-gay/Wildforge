//! Deterministic whole-planet geological genesis and its persisted model.

use super::{
    AtlasError, AtlasGrid, AtlasPos, CancellationToken, GEOLOGY_SCHEMA_VERSION, GeometryCell,
    ResourceCell, TectonicCell, TerrainCell, mix64,
};
use glam::Vec2;
use serde::{Deserialize, Serialize};

mod kinds;
pub use kinds::{BasinKind, BedrockFamily, DetailedBoundary};
mod minerals;
pub use minerals::{IntrusionKind, MagmaChemistry, MineralKind, VolcanoSource};
mod records;
pub use records::{
    ContinentRecord, CratonRecord, DepositRecord, GeologicalProvinceRecord, GeologyAttemptRecord,
    IntrusionRecord, PlateRecord, StratigraphicStackRecord, VolcanoRecord,
};
mod geometry;
mod plates;
#[cfg(test)]
pub(crate) use plates::classify_pair_rotation_probe;
mod attempt;
mod deposits;
mod provinces;
mod relief;
mod sampling;
mod strata;
mod tectonics;
mod validation;
mod volcanism;
use attempt::build_attempt;
use plates::{plate_assignments, plate_sites};
use strata::default_stacks;

const MAX_GEOLOGY_ATTEMPTS: u8 = 8;
const LLOYD_PASSES: usize = 2;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GeologyModel {
    pub schema_version: u32,
    pub chosen_attempt: u8,
    pub rejected_attempts: Vec<GeologyAttemptRecord>,
    pub target_ocean_fraction: f64,
    pub achieved_ocean_fraction: f64,
    pub raw_sea_level: f32,
    pub largest_ocean_share: f64,
    pub plates: Vec<PlateRecord>,
    pub cratons: Vec<CratonRecord>,
    pub continents: Vec<ContinentRecord>,
    pub provinces: Vec<GeologicalProvinceRecord>,
    pub stratigraphic_stacks: Vec<StratigraphicStackRecord>,
    pub volcanoes: Vec<VolcanoRecord>,
    pub intrusions: Vec<IntrusionRecord>,
    pub deposits: Vec<DepositRecord>,
}

impl Default for GeologyModel {
    fn default() -> Self {
        Self {
            schema_version: GEOLOGY_SCHEMA_VERSION,
            chosen_attempt: 0,
            rejected_attempts: Vec::new(),
            target_ocean_fraction: 0.66,
            achieved_ocean_fraction: 0.66,
            raw_sea_level: 0.0,
            largest_ocean_share: 1.0,
            plates: Vec::new(),
            cratons: Vec::new(),
            continents: Vec::new(),
            provinces: Vec::new(),
            stratigraphic_stacks: default_stacks(),
            volcanoes: Vec::new(),
            intrusions: Vec::new(),
            deposits: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtlasGeologySample {
    pub plate_id: u16,
    pub neighbor_plate: u16,
    pub boundary: DetailedBoundary,
    pub boundary_distance_blocks: f32,
    pub boundary_strength: f32,
    pub boundary_strike: Vec2,
    pub continental_fraction: f32,
    pub craton_id: u16,
    pub oceanic_age_myr: u16,
    pub bedrock: BedrockFamily,
    pub geological_province: u16,
    pub stratigraphic_stack: u16,
    pub metamorphic_grade: u8,
    pub fault_intensity: u16,
    pub volcanic_history: u8,
    pub basin: BasinKind,
    pub landmass_id: u16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoundaryEdge {
    pub a: AtlasPos,
    pub b: AtlasPos,
    pub detail: DetailedBoundary,
    pub strength: f32,
}

pub(super) struct GeologyOutput {
    pub tectonics: AtlasGrid<TectonicCell>,
    pub terrain: AtlasGrid<TerrainCell>,
    pub resources: AtlasGrid<ResourceCell>,
    pub model: GeologyModel,
}

#[derive(Clone)]
struct AttemptOutput {
    tectonics: Vec<TectonicCell>,
    terrain: Vec<TerrainCell>,
    resources: Vec<ResourceCell>,
    cratons: Vec<CratonRecord>,
    continents: Vec<ContinentRecord>,
    provinces: Vec<GeologicalProvinceRecord>,
    volcanoes: Vec<VolcanoRecord>,
    intrusions: Vec<IntrusionRecord>,
    deposits: Vec<DepositRecord>,
    raw_sea_level: f32,
    achieved_ocean_fraction: f64,
    largest_ocean_share: f64,
    failures: Vec<String>,
}

pub(super) fn generate_geology(
    seed: u32,
    side: u16,
    geometry: &AtlasGrid<GeometryCell>,
    cancel: &CancellationToken,
) -> Result<GeologyOutput, AtlasError> {
    let plates = plate_sites(seed, geometry);
    let plate_ids = plate_assignments(&plates, geometry);
    let target_ocean = 0.64 + (mix64(u64::from(seed) ^ 0x006f_6365_616e) % 401) as f64 / 10_000.0;
    let mut rejected = Vec::new();
    let mut chosen = None;
    for attempt in 0..MAX_GEOLOGY_ATTEMPTS {
        if cancel.is_cancelled() {
            return Err(AtlasError::Cancelled);
        }
        let result = build_attempt(
            seed,
            attempt,
            side,
            geometry,
            &plates,
            &plate_ids,
            target_ocean,
        );
        if result.failures.is_empty() {
            chosen = Some((attempt, result));
            break;
        }
        rejected.push(GeologyAttemptRecord {
            attempt,
            failed_constraints: result.failures.clone(),
        });
    }
    let Some((chosen_attempt, chosen)) = chosen else {
        let failures = rejected
            .last()
            .map(|record| record.failed_constraints.join("; "))
            .unwrap_or_else(|| "no geological attempt ran".into());
        return Err(AtlasError::Incomplete(format!(
            "geology exhausted {MAX_GEOLOGY_ATTEMPTS} deterministic attempts: {failures}"
        )));
    };
    let model = GeologyModel {
        schema_version: GEOLOGY_SCHEMA_VERSION,
        chosen_attempt,
        rejected_attempts: rejected,
        target_ocean_fraction: target_ocean,
        achieved_ocean_fraction: chosen.achieved_ocean_fraction,
        raw_sea_level: chosen.raw_sea_level,
        largest_ocean_share: chosen.largest_ocean_share,
        plates,
        cratons: chosen.cratons,
        continents: chosen.continents,
        provinces: chosen.provinces,
        stratigraphic_stacks: default_stacks(),
        volcanoes: chosen.volcanoes,
        intrusions: chosen.intrusions,
        deposits: chosen.deposits,
    };
    model.validate(side, &chosen.tectonics, &chosen.terrain, &chosen.resources)?;
    Ok(GeologyOutput {
        tectonics: AtlasGrid::from_values(side, chosen.tectonics)?,
        terrain: AtlasGrid::from_values(side, chosen.terrain)?,
        resources: AtlasGrid::from_values(side, chosen.resources)?,
        model,
    })
}
