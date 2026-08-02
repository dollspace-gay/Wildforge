//! Persistent whole-planet atlas infrastructure.
//!
//! The atlas is the coarse, authoritative description of country that has
//! not been materialized as voxel chunks yet. Scientific goals replace the
//! placeholder values in these typed stage-owned layers; the address space,
//! persistence contract, topology, and query operations live here.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use glam::{DVec3, Vec2, Vec3};
use noise::{NoiseFn, Perlin};
use serde::{Deserialize, Serialize};

use crate::chunk::SEA_LEVEL;
use crate::planet::{
    Direction4, FACE_BLOCKS, Face, PLANET_RADIUS, QuarterTurn, SURFACE_FACES, SurfacePoint,
    SurfacePos, canonicalize_surface_point, geodesic_distance, surface_to_unit,
};

mod diagnostics;
pub use diagnostics::{AtlasCensus, AtlasExportReport, export_diagnostics};
mod biomes;
pub use biomes::*;
mod climate;
pub use climate::*;
mod geology;
pub use geology::*;
mod hydrology;
pub use hydrology::*;
mod water_cycle;
pub use water_cycle::*;

pub const ATLAS_CELL_BLOCKS: u16 = 32;
pub const ATLAS_FACE_SIDE: u16 = FACE_BLOCKS / ATLAS_CELL_BLOCKS;
pub const ATLAS_CELL_COUNT: usize =
    SURFACE_FACES * ATLAS_FACE_SIDE as usize * ATLAS_FACE_SIDE as usize;
pub const ATLAS_IMMUTABLE_FILE_BYTES: u64 =
    (FILE_HEADER_BYTES + ATLAS_CELL_COUNT * GENESIS_RECORD_BYTES) as u64;
pub const ATLAS_DYNAMIC_FILE_BYTES: u64 =
    (FILE_HEADER_BYTES + DYNAMIC_PREFIX_BYTES + ATLAS_CELL_COUNT * DYNAMIC_RECORD_BYTES) as u64;
pub const ATLAS_WATER_CYCLE_MIN_FILE_BYTES: u64 =
    (FILE_HEADER_BYTES + 156 + ATLAS_CELL_COUNT * 80) as u64;
pub const ATLAS_ESTIMATED_LOADED_BYTES: u64 = (ATLAS_CELL_COUNT
    * (std::mem::size_of::<GeometryCell>()
        + std::mem::size_of::<TectonicCell>()
        + std::mem::size_of::<TerrainCell>()
        + std::mem::size_of::<ClimateCell>()
        + std::mem::size_of::<HydrologyCell>()
        + std::mem::size_of::<GroundCell>()
        + std::mem::size_of::<BiomeCell>()
        + std::mem::size_of::<ResourceCell>()
        + std::mem::size_of::<DynamicCell>()
        + std::mem::size_of::<WaterCell>())) as u64;
/// Conservative generation peak: completed grids plus immutable and dynamic
/// encode buffers and one file container each.
pub const ATLAS_ESTIMATED_GENERATION_PEAK_BYTES: u64 =
    ATLAS_ESTIMATED_LOADED_BYTES + ATLAS_IMMUTABLE_FILE_BYTES * 2;

pub const ATLAS_FORMAT_VERSION: u32 = 6;
pub const ATLAS_ALGORITHM_VERSION: u32 = 7;
pub const ATLAS_DYNAMIC_VERSION: u32 = 3;
pub const WATER_CYCLE_SCHEMA_VERSION: u32 = 1;
pub const ATLAS_HISTORY_VERSION: u32 = 1;
pub const GEOLOGY_SCHEMA_VERSION: u32 = 1;

const GENESIS_MAGIC: &[u8; 4] = b"WFA6";
const DYNAMIC_MAGIC: &[u8; 4] = b"WFD3";
const WATER_CYCLE_MAGIC: &[u8; 4] = b"WFW1";
const GENESIS_RECORD_BYTES: usize = 335;
const DYNAMIC_PREFIX_BYTES: usize = 8;
const DYNAMIC_RECORD_BYTES: usize = 32;
const FILE_HEADER_BYTES: usize = 32;
const MAX_GENESIS_BYTES: u64 = 128 * 1024 * 1024;
const MAX_DYNAMIC_BYTES: u64 = 64 * 1024 * 1024;
const MAX_WATER_CYCLE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_HISTORY_BYTES: u64 = 16 * 1024 * 1024;
const MAX_GEOLOGY_BYTES: u64 = 16 * 1024 * 1024;
const MAX_HYDROLOGY_BYTES: u64 = 32 * 1024 * 1024;
const MAX_BIOMES_BYTES: u64 = 16 * 1024 * 1024;

const GENESIS_FILE: &str = "genesis.wfa";
const DYNAMIC_FILE: &str = "dynamic.wfd";
const DYNAMIC_BACKUP_FILE: &str = "dynamic.wfd.bak";
const WATER_CYCLE_FILE: &str = "water.wfw";
const WATER_CYCLE_BACKUP_FILE: &str = "water.wfw.bak";
const HISTORY_FILE: &str = "history.wfh";
const GEOLOGY_FILE: &str = "geology.wfg";
const HYDROLOGY_FILE: &str = "hydrology.wfy";
const BIOMES_FILE: &str = "biomes.wfb";
const MANIFEST_FILE: &str = "manifest.toml";

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct AtlasPos {
    pub face: Face,
    pub u: u16,
    pub v: u16,
}

impl AtlasPos {
    pub fn new(face: Face, u: u16, v: u16, side: u16) -> Result<Self, AtlasError> {
        if side == 0 || !FACE_BLOCKS.is_multiple_of(side) {
            return Err(AtlasError::InvalidDimensions {
                side,
                count: usize::from(side) * usize::from(side) * SURFACE_FACES,
            });
        }
        if u < side && v < side {
            Ok(Self { face, u, v })
        } else {
            Err(AtlasError::InvalidPosition { face, u, v, side })
        }
    }

    #[inline]
    pub fn index(self, side: u16) -> usize {
        self.face.index() * usize::from(side) * usize::from(side)
            + usize::from(self.v) * usize::from(side)
            + usize::from(self.u)
    }

    pub fn from_index(index: usize, side: u16) -> Option<Self> {
        let face_len = usize::from(side) * usize::from(side);
        let face = Face::from_u8((index / face_len).try_into().ok()?)?;
        let local = index % face_len;
        Some(Self {
            face,
            u: (local % usize::from(side)) as u16,
            v: (local / usize::from(side)) as u16,
        })
    }

    pub fn from_surface(pos: SurfacePos, side: u16) -> Self {
        let cell = FACE_BLOCKS / side;
        Self {
            face: pos.face(),
            u: pos.u() / cell,
            v: pos.v() / cell,
        }
    }

    pub fn center(self, side: u16) -> SurfacePoint {
        let cell = f64::from(FACE_BLOCKS / side);
        SurfacePoint {
            face: self.face,
            u: (f64::from(self.u) + 0.5) * cell,
            v: (f64::from(self.v) + 0.5) * cell,
        }
    }

    pub fn step(self, direction: Direction4, side: u16) -> AtlasStep {
        let (du, dv) = match direction {
            Direction4::East => (1, 0),
            Direction4::North => (0, 1),
            Direction4::West => (-1, 0),
            Direction4::South => (0, -1),
        };
        self.offset_oriented(du, dv, side, direction)
    }

    fn offset_oriented(self, du: i32, dv: i32, side: u16, direction: Direction4) -> AtlasStep {
        let cell = f64::from(FACE_BLOCKS / side);
        let target_u = (f64::from(self.u) + 0.5 + f64::from(du)) * cell;
        let target_v = (f64::from(self.v) + 0.5 + f64::from(dv)) * cell;
        let canonical = canonicalize_surface_point(self.face, target_u, target_v)
            .expect("an atlas neighbor crosses at most one face edge");
        let u = (canonical.point.u / cell)
            .floor()
            .clamp(0.0, f64::from(side - 1)) as u16;
        let v = (canonical.point.v / cell)
            .floor()
            .clamp(0.0, f64::from(side - 1)) as u16;
        AtlasStep {
            pos: Self {
                face: canonical.point.face,
                u,
                v,
            },
            direction: canonical.rotation.map_direction(direction),
            rotation: canonical.rotation,
        }
    }

    pub fn neighbors4(self, side: u16) -> [Self; 4] {
        [
            self.step(Direction4::East, side).pos,
            self.step(Direction4::North, side).pos,
            self.step(Direction4::West, side).pos,
            self.step(Direction4::South, side).pos,
        ]
    }

    pub fn neighbors8(self, side: u16) -> [Self; 8] {
        // At each cube vertex two diagonal walks canonically reach the same
        // cell. That valence singularity has seven unique neighbors spanning
        // three charts; graph algorithms must deduplicate this fixed array.
        let cardinals = self.neighbors4(side);
        let diagonal = |u_direction: Direction4, v_direction: Direction4| {
            let first = self.step(u_direction, side);
            first
                .pos
                .step(first.rotation.map_direction(v_direction), side)
                .pos
        };
        [
            cardinals[0],
            cardinals[1],
            cardinals[2],
            cardinals[3],
            diagonal(Direction4::East, Direction4::North),
            diagonal(Direction4::West, Direction4::North),
            diagonal(Direction4::West, Direction4::South),
            diagonal(Direction4::East, Direction4::South),
        ]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AtlasStep {
    pub pos: AtlasPos,
    pub direction: Direction4,
    pub rotation: QuarterTurn,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtlasCell {
    pub pos: AtlasPos,
    pub unit_direction: [f32; 3],
    pub latitude_radians: f32,
    pub physical_area: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AtlasGrid<T> {
    side: u16,
    values: Vec<T>,
}

impl<T> AtlasGrid<T> {
    pub fn from_values(side: u16, values: Vec<T>) -> Result<Self, AtlasError> {
        let expected = atlas_count(side)?;
        if values.len() != expected {
            return Err(AtlasError::InvalidDimensions {
                side,
                count: values.len(),
            });
        }
        Ok(Self { side, values })
    }

    pub fn filled(side: u16, value: T) -> Result<Self, AtlasError>
    where
        T: Clone,
    {
        Self::from_values(side, vec![value; atlas_count(side)?])
    }

    #[inline]
    pub const fn side(&self) -> u16 {
        self.side
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    #[inline]
    pub fn get(&self, pos: AtlasPos) -> Option<&T> {
        (pos.u < self.side && pos.v < self.side).then(|| &self.values[pos.index(self.side)])
    }

    #[inline]
    pub fn get_mut(&mut self, pos: AtlasPos) -> Option<&mut T> {
        (pos.u < self.side && pos.v < self.side).then(move || {
            let index = pos.index(self.side);
            &mut self.values[index]
        })
    }

    #[inline]
    pub fn values(&self) -> &[T] {
        &self.values
    }

    #[inline]
    pub fn values_mut(&mut self) -> &mut [T] {
        &mut self.values
    }

    pub fn iter(&self) -> impl Iterator<Item = (AtlasPos, &T)> {
        let side = self.side;
        self.values.iter().enumerate().map(move |(index, value)| {
            (
                AtlasPos::from_index(index, side).expect("grid index is in range"),
                value,
            )
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum BoundaryClass {
    #[default]
    Interior = 0,
    Convergent = 1,
    Divergent = 2,
    Transform = 3,
}

impl BoundaryClass {
    fn from_u8(value: u8) -> Result<Self, AtlasError> {
        match value {
            0 => Ok(Self::Interior),
            1 => Ok(Self::Convergent),
            2 => Ok(Self::Divergent),
            3 => Ok(Self::Transform),
            _ => Err(AtlasError::Corrupt(format!(
                "unknown boundary classification {value}"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GeometryCell {
    pub unit_direction: [f32; 3],
    pub latitude_radians: f32,
    pub physical_area: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TectonicCell {
    pub plate_id: u16,
    pub boundary: BoundaryClass,
    pub boundary_detail: DetailedBoundary,
    pub neighbor_plate: u16,
    pub boundary_strength: f32,
    /// Distance to the nearest plate boundary in atlas cells.
    pub boundary_distance: u16,
    /// Unit boundary strike in planet-space, quantized to signed 16-bit.
    pub boundary_strike: [i16; 3],
    /// Normalized 0..=65535.
    pub continental_crust: u16,
    pub craton_id: u16,
    pub crust_age: u16,
    pub oceanic_age: u16,
    /// Approximate crust thickness in hectometres.
    pub crust_thickness: u16,
    pub bedrock_family: u16,
    pub geological_province: u16,
    pub stratigraphic_stack: u16,
    pub metamorphic_grade: u8,
    pub fault_intensity: u16,
    pub volcanic_history: u8,
    pub sediment_basin: BasinKind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TerrainCell {
    pub base_elevation: f32,
    pub eroded_elevation: f32,
    pub tectonic_contribution: f32,
    pub volcanic_contribution: f32,
    pub dynamic_topography: f32,
    /// Connected emerged landmass; zero is ocean.
    pub landmass_id: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClimateCell {
    pub mean_temperature: f32,
    pub seasonality: f32,
    pub ocean_temperature_anomaly: f32,
    pub continentality: f32,
    /// East/north components in this cell's atlas-chart tangent frame.
    pub prevailing_wind: [f32; 2],
    /// Surface-current direction in this cell's atlas-chart tangent frame.
    pub ocean_current: [f32; 2],
    pub mean_atmospheric_moisture: f32,
    pub mean_precipitation: f32,
    pub precipitation_seasonality: f32,
    pub potential_evapotranspiration: f32,
    pub aridity: f32,
    pub snow_persistence: f32,
    pub seasonal_temperature: [f32; CLIMATE_SEASONS],
    pub seasonal_precipitation: [f32; CLIMATE_SEASONS],
    pub seasonal_wind: [[f32; 2]; CLIMATE_SEASONS],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HydrologyCell {
    /// Global atlas index, or `u32::MAX` for a sink.
    pub drainage_receiver: u32,
    pub watershed_id: u32,
    pub ocean_basin_id: u16,
    pub lake_basin_id: u32,
    pub spill_elevation: f32,
    /// Priority-flood elevation used to resolve flats and numerical pits.
    pub filled_elevation: f32,
    /// Climate-normal runoff after infiltration, in millimetres per year.
    pub mean_runoff: f32,
    /// Normalized seasonal runoff shares; the four values sum to 65,535.
    pub seasonal_runoff_fraction: [u16; CLIMATE_SEASONS],
    /// Accumulated climate-normal flow in coarse block cubed per year.
    pub mean_discharge: f32,
    /// Normalized seasonal discharge shares; the four values sum to 65,535.
    pub seasonal_discharge_fraction: [u16; CLIMATE_SEASONS],
    /// Contributing spherical surface area in block squared.
    pub catchment_area: f32,
    pub channel_bed_elevation: f32,
    pub water_surface_elevation: f32,
    /// Bankfull dimensions in hundredths of a block.
    pub channel_width_centiblocks: u16,
    pub channel_depth_centiblocks: u16,
    /// Normalized transport/deposition energy.
    pub sediment_energy: u16,
    /// Continuous atlas baseline water volume, in eighth-block units.
    pub baseline_water_units: u64,
    /// Atlas volume minus quantized voxel volume for this coarse cell.
    pub voxel_volume_residual: i32,
    pub river_id: u32,
    /// Signed terrain change in hundredths of a block.
    pub erosion_centiblocks: i16,
    pub deposition_centiblocks: i16,
    pub seasonal_level_range_centiblocks: u16,
    pub flags: u16,
    pub stream_order: u8,
    pub salinity: u8,
    pub water_body: WaterBodyKind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GroundCell {
    pub soil_parent_material: u16,
    pub aquifer_capacity: u32,
    /// Normalized 0..=65535.
    pub aquifer_permeability: u16,
    /// Normalized primary porosity seed.
    pub porosity: u16,
    pub baseline_groundwater_head: f32,
    /// Compact soil-profile fields; texture fractions use 0..=255 and clay
    /// is the remainder after sand and silt.
    pub soil_depth_decimeters: u8,
    pub sand: u8,
    pub silt: u8,
    pub organic: u8,
    pub baseline_fertility: u8,
    /// 0 is saturated/poorly drained; 255 is excessively drained.
    pub drainage: u8,
    pub soil_salinity: u8,
    pub freeze_flags: u8,
    pub erosion_susceptibility: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BiomeCell {
    pub baseline_biome: u8,
    pub edaphic_flags: u16,
    pub habitat_flags: u32,
    pub vegetation_potential: u8,
    pub tree_line_y: u8,
    pub succession_potential: u8,
    pub country_id: u16,
    pub heart_assignment: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ResourceCell {
    /// Index into the later finite-site manifest; zero means no site yet.
    pub deposit_site_ref: u32,
    pub deposit_site_count: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GenesisLayers {
    pub geometry: AtlasGrid<GeometryCell>,
    pub tectonics: AtlasGrid<TectonicCell>,
    pub terrain: AtlasGrid<TerrainCell>,
    pub climate: AtlasGrid<ClimateCell>,
    pub hydrology: AtlasGrid<HydrologyCell>,
    pub ground: AtlasGrid<GroundCell>,
    pub biomes: AtlasGrid<BiomeCell>,
    pub resources: AtlasGrid<ResourceCell>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DynamicCell {
    pub atmospheric_vapor: u32,
    pub cloud_water: u32,
    pub local_weather_anomaly: i32,
    /// Near-surface temperature anomaly in centi-degrees Celsius.
    pub weather_temperature_anomaly: i16,
    /// Pressure anomaly in compact arbitrary pascal-like units.
    pub pressure_anomaly: i16,
    /// Convective/storm energy, normalized 0..=65535.
    pub storm_energy: u16,
    /// Water transferred out of cloud in the most recent climate hour.
    pub precipitation_rate: u16,
    /// East/north wind anomaly in signed fixed point.
    pub wind_anomaly: [i16; 2],
    pub fire_moisture_anomaly: i32,
    pub vegetation_moisture_anomaly: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DynamicLayers {
    /// Last fully accepted whole-planet climate hour. In-progress sliced
    /// passes are deliberately not checkpointed; they restart deterministically.
    pub completed_climate_hours: u64,
    pub cells: AtlasGrid<DynamicCell>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NamedAtlasPlace {
    pub id: u64,
    pub name: String,
    pub pos: AtlasPos,
}

/// Sparse player history is intentionally not baked into immutable genesis.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct HistoryLayers {
    pub version: u32,
    #[serde(default)]
    pub regional_ire: BTreeMap<AtlasPos, i16>,
    #[serde(default)]
    pub regional_tending: BTreeMap<AtlasPos, u64>,
    #[serde(default)]
    pub heart_state: BTreeMap<u32, String>,
    #[serde(default)]
    pub named_places: Vec<NamedAtlasPlace>,
    #[serde(default)]
    pub waystones: Vec<NamedAtlasPlace>,
    #[serde(default)]
    pub touched_cells: BTreeSet<AtlasPos>,
    #[serde(default)]
    pub bloom: BTreeMap<AtlasPos, u32>,
    #[serde(default)]
    pub exhaustion: BTreeMap<AtlasPos, u32>,
    #[serde(default)]
    pub resource_extraction: BTreeMap<u32, u64>,
    #[serde(default)]
    pub retrogen_stamps: BTreeMap<String, Vec<u32>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StageRecord {
    pub id: String,
    pub schema_version: u32,
    pub algorithm_version: u32,
    pub checksum: u64,
    pub elapsed_micros: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AtlasManifest {
    pub format_version: u32,
    pub seed: u32,
    pub topology: String,
    pub face_blocks: u16,
    pub atlas_cell_blocks: u16,
    pub atlas_face_side: u16,
    pub atlas_cell_count: u32,
    pub topology_version: u32,
    pub atlas_algorithm_version: u32,
    pub dynamic_schema_version: u32,
    pub water_cycle_schema_version: u32,
    pub history_schema_version: u32,
    pub geology_schema_version: u32,
    pub hydrology_schema_version: u32,
    pub biome_schema_version: u32,
    pub planet_radius: f64,
    pub rotation_axis: [f64; 3],
    pub prime_meridian: [f64; 3],
    pub axial_tilt_degrees: f64,
    pub climate_convergence_iterations: [u16; CLIMATE_SEASONS],
    pub climate_max_residual: f32,
    pub climate_moisture_budget_error: f64,
    pub content_hash: u64,
    pub genesis_checksum: u64,
    pub dynamic_checksum: u64,
    pub water_cycle_checksum: u64,
    pub history_checksum: u64,
    pub geology_checksum: u64,
    pub hydrology_checksum: u64,
    pub biome_checksum: u64,
    #[serde(default)]
    pub arcane_schema_version: u32,
    #[serde(default)]
    pub arcane_algorithm_version: u32,
    #[serde(default)]
    pub arcane_unit_scale: u32,
    #[serde(default)]
    pub arcane_genesis_total: u64,
    #[serde(default)]
    pub arcane_last_clean_total: u64,
    /// Deep, Ambient, Bound, Active, Dross, Scar at the last ledger
    /// checkpoint. Fixed order keeps the qualified manifest compact.
    #[serde(default)]
    pub arcane_reservoir_totals: [u64; 6],
    #[serde(default)]
    pub arcane_registry_hash: u64,
    #[serde(default)]
    pub arcane_ledger_checksum: u64,
    #[serde(default)]
    pub arcane_delta_checksum: u64,
    #[serde(default)]
    pub arcane_geography_schema_version: u32,
    #[serde(default)]
    pub arcane_geography_algorithm_version: u32,
    #[serde(default)]
    pub arcane_geography_dynamic_version: u32,
    #[serde(default)]
    pub arcane_geography_genesis_total: u64,
    #[serde(default)]
    pub arcane_geography_immutable_checksum: u64,
    #[serde(default)]
    pub arcane_geography_dynamic_checksum: u64,
    #[serde(default)]
    pub arcane_geography_site_catalog_checksum: u64,
    #[serde(default)]
    pub arcane_geography_last_authoritative_time: u64,
    pub genesis_bytes: u64,
    pub dynamic_bytes: u64,
    pub water_cycle_bytes: u64,
    pub geology_bytes: u64,
    pub hydrology_bytes: u64,
    pub biome_bytes: u64,
    pub complete: bool,
    #[serde(default)]
    pub stages: Vec<StageRecord>,
    #[serde(default)]
    pub layer_versions: BTreeMap<String, u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlanetAtlas {
    pub manifest: AtlasManifest,
    pub genesis: GenesisLayers,
    pub dynamic: DynamicLayers,
    pub water_cycle: WaterCycleState,
    pub history: HistoryLayers,
    pub geology: GeologyModel,
    pub hydrology: HydrologyModel,
    pub biomes: BiomeModel,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtlasTerrainSample {
    pub base_elevation: f32,
    pub eroded_elevation: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtlasClimateSample {
    pub mean_temperature: f32,
    pub mean_precipitation: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtlasTectonicSample {
    pub boundary: BoundaryClass,
    pub oceanic: bool,
    pub east_neighbor_oceanic: bool,
}

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

#[derive(Debug)]
pub enum AtlasError {
    Io(std::io::Error),
    Cancelled,
    InvalidDimensions {
        side: u16,
        count: usize,
    },
    InvalidPosition {
        face: Face,
        u: u16,
        v: u16,
        side: u16,
    },
    UnsupportedVersion(String),
    Corrupt(String),
    Incomplete(String),
    AlreadyExists(PathBuf),
}

impl fmt::Display for AtlasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Cancelled => f.write_str("planet creation cancelled"),
            Self::InvalidDimensions { side, count } => write!(
                f,
                "invalid atlas dimensions: side {side}, cell count {count}"
            ),
            Self::InvalidPosition { face, u, v, side } => {
                write!(f, "atlas position {face}/{u}/{v} is outside side {side}")
            }
            Self::UnsupportedVersion(message) => write!(f, "unsupported planet atlas: {message}"),
            Self::Corrupt(message) => write!(f, "corrupt planet atlas: {message}"),
            Self::Incomplete(message) => write!(f, "incomplete planet atlas: {message}"),
            Self::AlreadyExists(path) => {
                write!(f, "planet atlas already exists at {}", path.display())
            }
        }
    }
}

impl std::error::Error for AtlasError {}

impl From<std::io::Error> for AtlasError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

fn atlas_count(side: u16) -> Result<usize, AtlasError> {
    if side == 0 || !FACE_BLOCKS.is_multiple_of(side) {
        return Err(AtlasError::InvalidDimensions { side, count: 0 });
    }
    Ok(SURFACE_FACES * usize::from(side) * usize::from(side))
}

fn stable_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x1000_0000_01b3)
    })
}

/// Genesis compatibility covers built-in data, the externally loaded mod
/// tree, atlas algorithm generation, and the package version. It deliberately
/// excludes scripts because host scripts never define baseline voxel content.
pub fn genesis_content_hash(mods_dir: &Path) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut extend = |bytes: &[u8]| {
        for byte in bytes {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x1000_0000_01b3);
        }
    };
    for content in [
        include_bytes!("../base/blocks.toml").as_slice(),
        include_bytes!("../base/items.toml").as_slice(),
        include_bytes!("../base/recipes.toml").as_slice(),
        include_bytes!("../base/tags.toml").as_slice(),
        include_bytes!("../base/features.toml").as_slice(),
        include_bytes!("../base/aliases.toml").as_slice(),
        include_bytes!("../base/animals.toml").as_slice(),
        include_bytes!("../base/structures.toml").as_slice(),
    ] {
        extend(content);
    }
    extend(env!("CARGO_PKG_VERSION").as_bytes());
    extend(&ATLAS_ALGORITHM_VERSION.to_le_bytes());
    extend(&crate::net::content_hash(mods_dir).to_le_bytes());
    hash
}

fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn cell_hash(seed: u32, pos: AtlasPos, salt: u64) -> u64 {
    mix64(
        u64::from(seed)
            ^ salt
            ^ (pos.face as u64) << 56
            ^ u64::from(pos.u) << 24
            ^ u64::from(pos.v),
    )
}

fn unit_noise(noise: &Perlin, point: SurfacePoint, scale: f64, offset: [f64; 3]) -> f32 {
    let unit = surface_to_unit(point);
    noise.get([
        unit.x * scale + offset[0],
        unit.y * scale + offset[1],
        unit.z * scale + offset[2],
    ]) as f32
}

fn triangle_area(a: DVec3, b: DVec3, c: DVec3) -> f64 {
    let numerator = a.dot(b.cross(c)).abs();
    let denominator = 1.0 + a.dot(b) + b.dot(c) + c.dot(a);
    2.0 * numerator.atan2(denominator) * PLANET_RADIUS * PLANET_RADIUS
}

fn cell_area(pos: AtlasPos, side: u16) -> f32 {
    let cell = f64::from(FACE_BLOCKS / side);
    let u0 = f64::from(pos.u) * cell;
    let v0 = f64::from(pos.v) * cell;
    let u1 = u0 + cell;
    let v1 = v0 + cell;
    let at = |u, v| {
        surface_to_unit(SurfacePoint {
            face: pos.face,
            u,
            v,
        })
    };
    let (a, b, c, d) = (at(u0, v0), at(u1, v0), at(u1, v1), at(u0, v1));
    (triangle_area(a, b, c) + triangle_area(a, c, d)) as f32
}

fn generate_grid<T, F>(side: u16, mode: GenerationMode, make: F) -> Result<AtlasGrid<T>, AtlasError>
where
    T: Send,
    F: Fn(AtlasPos) -> T + Sync,
{
    let one_face = usize::from(side) * usize::from(side);
    let mut values = Vec::with_capacity(atlas_count(side)?);
    match mode {
        GenerationMode::Serial => {
            for face in Face::ALL {
                for v in 0..side {
                    for u in 0..side {
                        values.push(make(AtlasPos { face, u, v }));
                    }
                }
            }
        }
        GenerationMode::Parallel => {
            let faces = std::thread::scope(|scope| {
                let mut workers = Vec::with_capacity(SURFACE_FACES);
                let make = &make;
                for face in Face::ALL {
                    workers.push(scope.spawn(move || {
                        let mut face_values = Vec::with_capacity(one_face);
                        for v in 0..side {
                            for u in 0..side {
                                face_values.push(make(AtlasPos { face, u, v }));
                            }
                        }
                        face_values
                    }));
                }
                workers
                    .into_iter()
                    .map(|worker| worker.join().expect("atlas worker panicked"))
                    .collect::<Vec<_>>()
            });
            for face in faces {
                values.extend(face);
            }
        }
    }
    AtlasGrid::from_values(side, values)
}

fn layer_versions() -> BTreeMap<String, u32> {
    let mut versions = [
        "geometry",
        "tectonics",
        "terrain",
        "climate",
        "hydrology",
        "ground",
        "biomes",
        "resources",
        "geology_manifest",
        "dynamic",
        "water_cycle",
        "history",
        "arcane_current_capacity",
        "arcane_deep_reserve_capacity",
        "arcane_surface_deep_exchange",
        "arcane_horizontal_conductivity",
        "arcane_dross_mobility_retention",
        "arcane_baseline_resonance",
        "arcane_stability",
        "arcane_recovery_potential",
        "arcane_site_references",
        "arcane_dynamic_state",
    ]
    .into_iter()
    .map(|name| (name.to_string(), 1))
    .collect::<BTreeMap<_, _>>();
    versions.insert("climate".to_string(), 2);
    versions.insert("arcane_horizontal_conductivity".to_string(), 2);
    versions.insert("hydrology".to_string(), 2);
    versions.insert("dynamic".to_string(), 3);
    versions.insert("water_cycle".to_string(), WATER_CYCLE_SCHEMA_VERSION);
    versions
}

impl PlanetAtlas {
    pub fn generate(
        seed: u32,
        content_hash: u64,
        config: AtlasConfig,
        cancel: &CancellationToken,
        mut progress: impl FnMut(AtlasProgress),
    ) -> Result<Self, AtlasError> {
        let side = config.side;
        let count = atlas_count(side)?;
        let mut stages = Vec::new();
        let mut announce = |stage: AtlasStage| -> Result<Instant, AtlasError> {
            if cancel.is_cancelled() {
                return Err(AtlasError::Cancelled);
            }
            let completed_stages = AtlasStage::ALL
                .iter()
                .position(|candidate| *candidate == stage)
                .unwrap_or(0);
            progress(AtlasProgress {
                stage,
                completed_stages,
                total_stages: AtlasStage::ALL.len(),
            });
            Ok(Instant::now())
        };
        let mut record = |stage: AtlasStage, started: Instant, checksum: u64| {
            stages.push(StageRecord {
                id: stage.id().to_string(),
                schema_version: 1,
                algorithm_version: ATLAS_ALGORITHM_VERSION,
                checksum,
                elapsed_micros: started.elapsed().as_micros().try_into().unwrap_or(u64::MAX),
            });
        };

        let started = announce(AtlasStage::Topology)?;
        let geometry = generate_grid(side, config.mode, |pos| {
            let unit = surface_to_unit(pos.center(side));
            GeometryCell {
                unit_direction: [unit.x as f32, unit.y as f32, unit.z as f32],
                latitude_radians: unit.y.asin() as f32,
                physical_area: cell_area(pos, side),
            }
        })?;
        validate_geometry(&geometry)?;
        record(
            AtlasStage::Topology,
            started,
            fingerprint_geometry(&geometry),
        );

        let started = announce(AtlasStage::Tectonics)?;
        let GeologyOutput {
            tectonics,
            terrain: geological_terrain,
            resources: mut geological_resources,
            model: mut geology,
        } = generate_geology(seed, side, &geometry, cancel)?;
        validate_tectonics(&tectonics)?;
        record(
            AtlasStage::Tectonics,
            started,
            fingerprint_tectonics(&tectonics),
        );

        let started = announce(AtlasStage::Elevation)?;
        let mut terrain = geological_terrain;
        validate_terrain(&terrain)?;
        record(
            AtlasStage::Elevation,
            started,
            fingerprint_terrain(&terrain),
        );

        let started = announce(AtlasStage::Climate)?;
        let (climate, climate_report) = generate_climate(seed, side, &geometry, &terrain, cancel)?;
        validate_climate(&climate)?;
        record(AtlasStage::Climate, started, fingerprint_climate(&climate));

        let started = announce(AtlasStage::Drainage)?;
        let HydrologyOutput {
            terrain: hydrological_terrain,
            cells: hydrology,
            model: hydrology_model,
        } = generate_hydrology(
            seed, side, &geometry, &tectonics, &terrain, &climate, cancel,
        )?;
        terrain = hydrological_terrain;
        route_placer_deposits(
            side,
            &terrain,
            &tectonics,
            &hydrology,
            &mut geological_resources,
            &mut geology,
        );
        validate_hydrology(&hydrology)?;
        record(
            AtlasStage::Drainage,
            started,
            fingerprint_hydrology(&hydrology),
        );

        let started = announce(AtlasStage::Hydrology)?;
        record(
            AtlasStage::Hydrology,
            started,
            fingerprint_hydrology(&hydrology),
        );

        let started = announce(AtlasStage::Ground)?;
        let ground = generate_ground_layer(
            seed,
            side,
            config.mode,
            &tectonics,
            &terrain,
            &climate,
            &hydrology,
        )?;
        validate_ground(&ground)?;
        record(AtlasStage::Ground, started, fingerprint_ground(&ground));

        let started = announce(AtlasStage::Biomes)?;
        let (biomes, biome_model) = generate_biomes_and_countries(
            seed,
            side,
            config.mode,
            &geometry,
            &tectonics,
            &terrain,
            &climate,
            &hydrology,
            &ground,
        )?;
        validate_biomes(&biomes)?;
        record(AtlasStage::Biomes, started, fingerprint_biomes(&biomes));

        let started = announce(AtlasStage::Resources)?;
        let resources = geological_resources;
        record(
            AtlasStage::Resources,
            started,
            fingerprint_resources(&resources),
        );

        let weather_noise = Perlin::new(seed ^ 0x5745_4154);
        let dynamic_cells = generate_grid(side, config.mode, |pos| {
            let climate_cell = climate.get(pos).expect("matching atlas grids");
            let atmospheric_vapor = ((climate_cell.mean_atmospheric_moisture * 180.0).max(1.0)
                as u32)
                .saturating_mul(HYDRO_UNITS_PER_VISIBLE_LEVEL as u32);
            let cloudiness =
                ((unit_noise(&weather_noise, pos.center(side), 4.2, [17.0, -29.0, 7.0]) + 1.0)
                    * 0.5)
                    .clamp(0.0, 1.0)
                    .powi(4);
            DynamicCell {
                atmospheric_vapor,
                cloud_water: (atmospheric_vapor as f32 * 0.20 * cloudiness) as u32,
                ..DynamicCell::default()
            }
        })?;

        let genesis = GenesisLayers {
            geometry,
            tectonics,
            terrain,
            climate,
            hydrology,
            ground,
            biomes,
            resources,
        };
        let dynamic = DynamicLayers {
            completed_climate_hours: 0,
            cells: dynamic_cells,
        };
        let atmospheric_mass = ReservoirMass::fresh(dynamic_water_total(&dynamic) as u64);
        let water_cycle = initial_water_cycle(side, &genesis, &hydrology_model, atmospheric_mass)?;
        let history = HistoryLayers {
            version: ATLAS_HISTORY_VERSION,
            ..HistoryLayers::default()
        };

        let started = announce(AtlasStage::Validation)?;
        validate_layers(side, &genesis, &dynamic)?;
        let (genesis_checksum, genesis_bytes) = {
            let payload = encode_genesis(&genesis)?;
            (
                stable_hash(&payload),
                (FILE_HEADER_BYTES + payload.len()) as u64,
            )
        };
        let (dynamic_checksum, dynamic_bytes) = {
            let payload = encode_dynamic(&dynamic)?;
            (
                stable_hash(&payload),
                (FILE_HEADER_BYTES + payload.len()) as u64,
            )
        };
        let (water_cycle_checksum, water_cycle_bytes) = {
            let payload = encode_water_cycle(&water_cycle)?;
            (
                stable_hash(&payload),
                (FILE_HEADER_BYTES + payload.len()) as u64,
            )
        };
        let (history_checksum, _history_bytes) = {
            let payload = encode_history(&history)?;
            (stable_hash(payload.as_bytes()), payload.len() as u64)
        };
        let (geology_checksum, geology_bytes) = {
            let payload = encode_geology(&geology)?;
            (stable_hash(payload.as_bytes()), payload.len() as u64)
        };
        let (hydrology_checksum, hydrology_bytes) = {
            let payload = encode_hydrology(&hydrology_model)?;
            (stable_hash(payload.as_bytes()), payload.len() as u64)
        };
        let (biome_checksum, biome_bytes) = {
            let payload = encode_biomes(&biome_model)?;
            (stable_hash(payload.as_bytes()), payload.len() as u64)
        };
        record(AtlasStage::Validation, started, genesis_checksum);

        let manifest = AtlasManifest {
            format_version: ATLAS_FORMAT_VERSION,
            seed,
            topology: crate::world::WORLD_TOPOLOGY.to_string(),
            face_blocks: FACE_BLOCKS,
            atlas_cell_blocks: FACE_BLOCKS / side,
            atlas_face_side: side,
            atlas_cell_count: count.try_into().expect("atlas cell count fits u32"),
            topology_version: 1,
            atlas_algorithm_version: ATLAS_ALGORITHM_VERSION,
            dynamic_schema_version: ATLAS_DYNAMIC_VERSION,
            water_cycle_schema_version: WATER_CYCLE_SCHEMA_VERSION,
            history_schema_version: ATLAS_HISTORY_VERSION,
            geology_schema_version: GEOLOGY_SCHEMA_VERSION,
            hydrology_schema_version: HYDROLOGY_SCHEMA_VERSION,
            biome_schema_version: BIOME_SCHEMA_VERSION,
            planet_radius: PLANET_RADIUS,
            rotation_axis: ROTATION_AXIS,
            prime_meridian: PRIME_MERIDIAN,
            axial_tilt_degrees: AXIAL_TILT_DEGREES,
            climate_convergence_iterations: climate_report.iterations,
            climate_max_residual: climate_report.max_residual,
            climate_moisture_budget_error: climate_report.max_moisture_budget_error,
            content_hash,
            genesis_checksum,
            dynamic_checksum,
            water_cycle_checksum,
            history_checksum,
            geology_checksum,
            hydrology_checksum,
            biome_checksum,
            arcane_schema_version: 0,
            arcane_algorithm_version: 0,
            arcane_unit_scale: 0,
            arcane_genesis_total: 0,
            arcane_last_clean_total: 0,
            arcane_reservoir_totals: [0; 6],
            arcane_registry_hash: 0,
            arcane_ledger_checksum: 0,
            arcane_delta_checksum: 0,
            arcane_geography_schema_version: 0,
            arcane_geography_algorithm_version: 0,
            arcane_geography_dynamic_version: 0,
            arcane_geography_genesis_total: 0,
            arcane_geography_immutable_checksum: 0,
            arcane_geography_dynamic_checksum: 0,
            arcane_geography_site_catalog_checksum: 0,
            arcane_geography_last_authoritative_time: 0,
            genesis_bytes,
            dynamic_bytes,
            water_cycle_bytes,
            geology_bytes,
            hydrology_bytes,
            biome_bytes,
            complete: true,
            stages,
            layer_versions: layer_versions(),
        };
        let atlas = Self {
            manifest,
            genesis,
            dynamic,
            water_cycle,
            history,
            geology,
            hydrology: hydrology_model,
            biomes: biome_model,
        };
        atlas.validate()?;
        Ok(atlas)
    }

    #[cfg(test)]
    pub fn fixture(seed: u32, side: u16) -> Result<Self, AtlasError> {
        Self::generate(
            seed,
            0,
            AtlasConfig::fixture(side),
            &CancellationToken::default(),
            |_| {},
        )
    }

    #[inline]
    pub const fn side(&self) -> u16 {
        self.manifest.atlas_face_side
    }

    #[inline]
    pub const fn cell_blocks(&self) -> u16 {
        self.manifest.atlas_cell_blocks
    }

    pub fn cell(&self, pos: AtlasPos) -> Option<AtlasCell> {
        let geometry = self.genesis.geometry.get(pos)?;
        Some(AtlasCell {
            pos,
            unit_direction: geometry.unit_direction,
            latitude_radians: geometry.latitude_radians,
            physical_area: geometry.physical_area,
        })
    }

    pub fn atlas_pos(&self, surface: SurfacePos) -> AtlasPos {
        AtlasPos::from_surface(surface, self.side())
    }

    /// Generator-facing terrain query. Chunk code depends on this typed
    /// interface rather than atlas storage layout or layer vectors.
    pub fn terrain_sample(&self, point: SurfacePoint) -> AtlasTerrainSample {
        AtlasTerrainSample {
            base_elevation: self.sample_scalar(point, |pos| {
                self.genesis
                    .terrain
                    .get(pos)
                    .expect("validated atlas query is in range")
                    .base_elevation
            }),
            eroded_elevation: self.sample_scalar(point, |pos| {
                self.genesis
                    .terrain
                    .get(pos)
                    .expect("validated atlas query is in range")
                    .eroded_elevation
            }),
        }
    }

    pub fn climate_sample(&self, point: SurfacePoint) -> AtlasClimateSample {
        AtlasClimateSample {
            mean_temperature: self.sample_scalar(point, |pos| {
                self.genesis
                    .climate
                    .get(pos)
                    .expect("validated atlas query is in range")
                    .mean_temperature
            }),
            mean_precipitation: self.sample_scalar(point, |pos| {
                self.genesis
                    .climate
                    .get(pos)
                    .expect("validated atlas query is in range")
                    .mean_precipitation
            }),
        }
    }

    pub fn tectonic_sample(&self, surface: SurfacePos) -> AtlasTectonicSample {
        let pos = self.atlas_pos(surface);
        let east = pos.step(Direction4::East, self.side()).pos;
        AtlasTectonicSample {
            boundary: self
                .genesis
                .tectonics
                .get(pos)
                .expect("validated atlas query is in range")
                .boundary,
            oceanic: self
                .genesis
                .terrain
                .get(pos)
                .is_some_and(|cell| cell.eroded_elevation <= SEA_LEVEL as f32),
            east_neighbor_oceanic: self
                .genesis
                .terrain
                .get(east)
                .is_some_and(|cell| cell.eroded_elevation <= SEA_LEVEL as f32),
        }
    }

    pub fn validate(&self) -> Result<(), AtlasError> {
        validate_manifest(&self.manifest, false)?;
        validate_layers(self.side(), &self.genesis, &self.dynamic)?;
        if self.history.version != ATLAS_HISTORY_VERSION {
            return Err(AtlasError::UnsupportedVersion(format!(
                "history schema {} (supported {})",
                self.history.version, ATLAS_HISTORY_VERSION
            )));
        }
        let genesis = encode_genesis(&self.genesis)?;
        if self.manifest.genesis_bytes != (FILE_HEADER_BYTES + genesis.len()) as u64 {
            return Err(AtlasError::Corrupt(
                "manifest immutable byte count is inconsistent".into(),
            ));
        }
        if stable_hash(&genesis) != self.manifest.genesis_checksum {
            return Err(AtlasError::Corrupt(
                "immutable layer checksum does not match manifest".into(),
            ));
        }
        drop(genesis);
        let dynamic = encode_dynamic(&self.dynamic)?;
        if self.manifest.dynamic_bytes != (FILE_HEADER_BYTES + dynamic.len()) as u64 {
            return Err(AtlasError::Corrupt(
                "manifest dynamic byte count is inconsistent".into(),
            ));
        }
        if stable_hash(&dynamic) != self.manifest.dynamic_checksum {
            return Err(AtlasError::Corrupt(
                "dynamic layer checksum does not match manifest".into(),
            ));
        }
        drop(dynamic);
        let water_cycle = encode_water_cycle(&self.water_cycle)?;
        if self.manifest.water_cycle_bytes != (FILE_HEADER_BYTES + water_cycle.len()) as u64
            || stable_hash(&water_cycle) != self.manifest.water_cycle_checksum
        {
            return Err(AtlasError::Corrupt(
                "water-cycle state does not match the committed manifest".into(),
            ));
        }
        drop(water_cycle);
        self.water_cycle.validate(
            self.side(),
            ReservoirMass::fresh(dynamic_water_total(&self.dynamic) as u64),
        )?;
        if self.water_cycle.completed_surface_hours != self.dynamic.completed_climate_hours {
            return Err(AtlasError::Corrupt(format!(
                "atmosphere hour {} and water-cycle hour {} are not one atomic snapshot",
                self.dynamic.completed_climate_hours, self.water_cycle.completed_surface_hours,
            )));
        }
        let history = encode_history(&self.history)?;
        if stable_hash(history.as_bytes()) != self.manifest.history_checksum {
            return Err(AtlasError::Corrupt(
                "history checksum does not match manifest".into(),
            ));
        }
        self.geology.validate(
            self.side(),
            self.genesis.tectonics.values(),
            self.genesis.terrain.values(),
            self.genesis.resources.values(),
        )?;
        let geology = encode_geology(&self.geology)?;
        if geology.len() as u64 != self.manifest.geology_bytes
            || stable_hash(geology.as_bytes()) != self.manifest.geology_checksum
        {
            return Err(AtlasError::Corrupt(
                "geology manifest does not match the committed atlas manifest".into(),
            ));
        }
        self.hydrology
            .validate(self.side(), &self.genesis.terrain, &self.genesis.hydrology)?;
        let hydrology = encode_hydrology(&self.hydrology)?;
        if hydrology.len() as u64 != self.manifest.hydrology_bytes
            || stable_hash(hydrology.as_bytes()) != self.manifest.hydrology_checksum
        {
            return Err(AtlasError::Corrupt(
                "hydrology manifest does not match the committed atlas manifest".into(),
            ));
        }
        self.biomes
            .validate(self.side(), &self.genesis.terrain, &self.genesis.biomes)?;
        let biomes = encode_biomes(&self.biomes)?;
        if biomes.len() as u64 != self.manifest.biome_bytes
            || stable_hash(biomes.as_bytes()) != self.manifest.biome_checksum
        {
            return Err(AtlasError::Corrupt(
                "biome/country manifest does not match the committed atlas manifest".into(),
            ));
        }
        Ok(())
    }

    pub fn sample_scalar(&self, point: SurfacePoint, value: impl Fn(AtlasPos) -> f32) -> f32 {
        let side = self.side();
        let cell = f64::from(self.cell_blocks());
        let x = point.u / cell - 0.5;
        let z = point.v / cell - 0.5;
        let base_u = x.floor() as i32;
        let base_v = z.floor() as i32;
        let tx = (x - f64::from(base_u)) as f32;
        let tz = (z - f64::from(base_v)) as f32;
        let sample = |du: i32, dv: i32| {
            let center = AtlasPos {
                face: point.face,
                u: base_u.clamp(0, i32::from(side) - 1) as u16,
                v: base_v.clamp(0, i32::from(side) - 1) as u16,
            };
            let source_u = (f64::from(base_u + du) + 0.5) * cell;
            let source_v = (f64::from(base_v + dv) + 0.5) * cell;
            let canonical = canonicalize_surface_point(point.face, source_u, source_v)
                .expect("bilinear stencil crosses at most one face edge");
            let pos = AtlasPos {
                face: canonical.point.face,
                u: (canonical.point.u / cell)
                    .floor()
                    .clamp(0.0, f64::from(side - 1)) as u16,
                v: (canonical.point.v / cell)
                    .floor()
                    .clamp(0.0, f64::from(side - 1)) as u16,
            };
            let _ = center;
            value(pos)
        };
        let a = sample(0, 0);
        let b = sample(1, 0);
        let c = sample(0, 1);
        let d = sample(1, 1);
        let ab = a + (b - a) * tx;
        let cd = c + (d - c) * tx;
        ab + (cd - ab) * tz
    }

    pub fn sample_tangent_vector(
        &self,
        point: SurfacePoint,
        value: impl Fn(AtlasPos) -> [f32; 2],
    ) -> Vec2 {
        let side = self.side();
        let cell = f64::from(self.cell_blocks());
        let x = point.u / cell - 0.5;
        let z = point.v / cell - 0.5;
        let base_u = x.floor() as i32;
        let base_v = z.floor() as i32;
        let tx = (x - f64::from(base_u)) as f32;
        let tz = (z - f64::from(base_v)) as f32;
        let sample = |du: i32, dv: i32| {
            let source_u = (f64::from(base_u + du) + 0.5) * cell;
            let source_v = (f64::from(base_v + dv) + 0.5) * cell;
            let canonical = canonicalize_surface_point(point.face, source_u, source_v)
                .expect("vector stencil crosses at most one face edge");
            let pos = AtlasPos {
                face: canonical.point.face,
                u: (canonical.point.u / cell)
                    .floor()
                    .clamp(0.0, f64::from(side - 1)) as u16,
                v: (canonical.point.v / cell)
                    .floor()
                    .clamp(0.0, f64::from(side - 1)) as u16,
            };
            let vector = value(pos);
            let inverse = QuarterTurn::new(4 - canonical.rotation.turns());
            let rotated = inverse.rotate_vec3(Vec3::new(vector[0], 0.0, vector[1]));
            Vec2::new(rotated.x, rotated.z)
        };
        let a = sample(0, 0);
        let b = sample(1, 0);
        let c = sample(0, 1);
        let d = sample(1, 1);
        let vector = a.lerp(b, tx).lerp(c.lerp(d, tx), tz);
        vector.normalize_or_zero()
    }

    pub fn bounded_stencil(
        &self,
        center: AtlasPos,
        radius: u16,
        max_cells: usize,
    ) -> Vec<AtlasPos> {
        if max_cells == 0 {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        let mut queue = VecDeque::from([(center, 0u16)]);
        while let Some((pos, distance)) = queue.pop_front() {
            if !seen.insert(pos) {
                continue;
            }
            out.push(pos);
            if out.len() >= max_cells || distance >= radius {
                continue;
            }
            for neighbor in pos.neighbors4(self.side()) {
                queue.push_back((neighbor, distance + 1));
            }
        }
        out
    }

    pub fn radius_query(&self, center: SurfacePoint, radius_blocks: f64) -> Vec<AtlasPos> {
        if radius_blocks.is_nan() || radius_blocks < 0.0 {
            return Vec::new();
        }
        let start = AtlasPos::from_surface(
            SurfacePos::new(
                center.face,
                center.u.floor().clamp(0.0, f64::from(FACE_BLOCKS - 1)) as u16,
                center.v.floor().clamp(0.0, f64::from(FACE_BLOCKS - 1)) as u16,
            )
            .expect("clamped radius-query center is canonical"),
            self.side(),
        );
        let radius_cells = if radius_blocks.is_infinite() {
            u16::MAX
        } else {
            ((radius_blocks / f64::from(self.cell_blocks())).ceil() as u16).saturating_add(2)
        };
        let diameter = usize::from(radius_cells)
            .saturating_mul(2)
            .saturating_add(1);
        let max_cells = diameter
            .saturating_mul(diameter)
            .saturating_mul(2)
            .min(self.genesis.geometry.len());
        self.bounded_stencil(start, radius_cells, max_cells)
            .into_iter()
            .filter(|pos| geodesic_distance(center, pos.center(self.side())) <= radius_blocks)
            .collect()
    }

    pub fn connected_components<T>(
        &self,
        grid: &AtlasGrid<T>,
        included: impl Fn(&T) -> bool,
    ) -> Result<(AtlasGrid<u32>, u32), AtlasError> {
        if grid.side() != self.side() {
            return Err(AtlasError::InvalidDimensions {
                side: grid.side(),
                count: grid.len(),
            });
        }
        let mut labels = vec![0u32; grid.len()];
        let mut next_label = 0u32;
        for (pos, value) in grid.iter() {
            if !included(value) || labels[pos.index(self.side())] != 0 {
                continue;
            }
            next_label += 1;
            let mut queue = VecDeque::from([pos]);
            labels[pos.index(self.side())] = next_label;
            while let Some(at) = queue.pop_front() {
                for neighbor in at.neighbors4(self.side()) {
                    let index = neighbor.index(self.side());
                    if labels[index] == 0 && grid.get(neighbor).is_some_and(&included) {
                        labels[index] = next_label;
                        queue.push_back(neighbor);
                    }
                }
            }
        }
        Ok((AtlasGrid::from_values(self.side(), labels)?, next_label))
    }

    pub fn downstream(&self, start: AtlasPos, max_steps: usize) -> Vec<AtlasPos> {
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        let mut at = start;
        while out.len() < max_steps && seen.insert(at) {
            out.push(at);
            let receiver = self
                .genesis
                .hydrology
                .get(at)
                .map_or(u32::MAX, |cell| cell.drainage_receiver);
            let Some(next) = (receiver != u32::MAX)
                .then(|| AtlasPos::from_index(receiver as usize, self.side()))
                .flatten()
            else {
                break;
            };
            at = next;
        }
        out
    }

    pub fn upstream(&self, start: AtlasPos, max_cells: usize) -> Vec<AtlasPos> {
        let mut out = Vec::new();
        let mut queue = VecDeque::from([start]);
        let mut seen = BTreeSet::new();
        while let Some(at) = queue.pop_front() {
            if !seen.insert(at) || out.len() >= max_cells {
                continue;
            }
            out.push(at);
            let wanted = at.index(self.side()) as u32;
            for (candidate, cell) in self.genesis.hydrology.iter() {
                if cell.drainage_receiver == wanted {
                    queue.push_back(candidate);
                }
            }
        }
        out
    }

    pub fn immutable_fingerprint(&self, layer: &str) -> Option<u64> {
        match layer {
            "geometry" => Some(fingerprint_geometry(&self.genesis.geometry)),
            "tectonics" => Some(fingerprint_tectonics(&self.genesis.tectonics)),
            "terrain" => Some(fingerprint_terrain(&self.genesis.terrain)),
            "climate" => Some(fingerprint_climate(&self.genesis.climate)),
            "hydrology" => Some(fingerprint_hydrology(&self.genesis.hydrology)),
            "ground" => Some(fingerprint_ground(&self.genesis.ground)),
            "biomes" => Some(fingerprint_biomes(&self.genesis.biomes)),
            "resources" => Some(fingerprint_resources(&self.genesis.resources)),
            _ => None,
        }
    }
}

impl PlanetAtlas {
    /// Return the separately versioned atlas directory for a world root.
    pub fn planet_dir(world_dir: &Path) -> PathBuf {
        world_dir.join("planet")
    }

    /// Commit a brand-new atlas as one directory rename. Existing immutable
    /// genesis data is never overwritten or regenerated by this operation.
    pub fn write_new(&self, world_dir: &Path) -> Result<PathBuf, AtlasError> {
        self.validate()?;
        fs::create_dir_all(world_dir)?;
        let destination = Self::planet_dir(world_dir);
        if destination.exists() {
            return Err(AtlasError::AlreadyExists(destination));
        }
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temporary = world_dir.join(format!(".planet.{}.{}.tmp", std::process::id(), stamp));
        fs::create_dir(&temporary)?;
        let result = (|| {
            self.write_bundle(&temporary)?;
            fs::rename(&temporary, &destination)?;
            sync_directory(world_dir)?;
            Ok(destination.clone())
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&temporary);
        }
        result
    }

    /// Load a production atlas. Immutable data is always read from disk; it
    /// is never silently regenerated after an algorithm update.
    pub fn load(world_dir: &Path) -> Result<Self, AtlasError> {
        Self::load_planet_dir(&Self::planet_dir(world_dir), true)
    }

    /// Cheap world-browser predicate: only a validated, complete manifest is
    /// selectable. Payload checks happen when the world is actually opened.
    pub fn is_committed(world_dir: &Path) -> bool {
        let path = Self::planet_dir(world_dir).join(MANIFEST_FILE);
        let Ok(bytes) = read_bounded(&path, MAX_MANIFEST_BYTES) else {
            return false;
        };
        let Ok(text) = std::str::from_utf8(&bytes) else {
            return false;
        };
        let Ok(manifest) = toml::from_str::<AtlasManifest>(text) else {
            return false;
        };
        validate_manifest(&manifest, !cfg!(test)).is_ok()
    }

    #[cfg(test)]
    pub fn load_fixture(world_dir: &Path) -> Result<Self, AtlasError> {
        Self::load_planet_dir(&Self::planet_dir(world_dir), false)
    }

    /// Atomically replace mutable hydrology/climate state while preserving a
    /// last-known file for interrupted-write recovery.
    pub fn save_dynamic(&mut self, world_dir: &Path) -> Result<(), AtlasError> {
        let planet_dir = Self::planet_dir(world_dir);
        let path = planet_dir.join(DYNAMIC_FILE);
        let water_path = planet_dir.join(WATER_CYCLE_FILE);
        if let Ok(previous) = read_bounded(&path, MAX_DYNAMIC_BYTES) {
            crate::persist::atomic_write(&planet_dir.join(DYNAMIC_BACKUP_FILE), &previous, false)?;
        }
        if let Ok(previous) = read_bounded(&water_path, MAX_WATER_CYCLE_BYTES) {
            crate::persist::atomic_write(
                &planet_dir.join(WATER_CYCLE_BACKUP_FILE),
                &previous,
                false,
            )?;
        }
        let payload = encode_dynamic(&self.dynamic)?;
        let water_payload = encode_water_cycle(&self.water_cycle)?;
        self.manifest.dynamic_checksum = stable_hash(&payload);
        self.manifest.water_cycle_checksum = stable_hash(&water_payload);
        let container = container_bytes_owned(
            DYNAMIC_MAGIC,
            ATLAS_DYNAMIC_VERSION,
            self.side(),
            DYNAMIC_RECORD_BYTES,
            payload,
        )?;
        self.manifest.dynamic_bytes = container.len() as u64;
        let water_container = variable_container_bytes_owned(
            WATER_CYCLE_MAGIC,
            WATER_CYCLE_SCHEMA_VERSION,
            self.side(),
            water_payload,
        )?;
        self.manifest.water_cycle_bytes = water_container.len() as u64;
        crate::persist::atomic_write(&path, &container, false)?;
        crate::persist::atomic_write(&water_path, &water_container, false)?;
        write_manifest(&planet_dir, &self.manifest)
    }

    /// Persist authoritative mutable state owned by the running world while
    /// immutable genesis remains shared with chunk workers.
    pub fn save_dynamic_snapshot(
        &self,
        world_dir: &Path,
        dynamic: &DynamicLayers,
        water_cycle: &WaterCycleState,
    ) -> Result<(), AtlasError> {
        if dynamic.cells.side() != self.side() || dynamic.cells.len() != self.dynamic.cells.len() {
            return Err(AtlasError::InvalidDimensions {
                side: dynamic.cells.side(),
                count: dynamic.cells.len(),
            });
        }
        let planet_dir = Self::planet_dir(world_dir);
        let path = planet_dir.join(DYNAMIC_FILE);
        let water_path = planet_dir.join(WATER_CYCLE_FILE);
        if let Ok(previous) = read_bounded(&path, MAX_DYNAMIC_BYTES) {
            crate::persist::atomic_write(&planet_dir.join(DYNAMIC_BACKUP_FILE), &previous, false)?;
        }
        if let Ok(previous) = read_bounded(&water_path, MAX_WATER_CYCLE_BYTES) {
            crate::persist::atomic_write(
                &planet_dir.join(WATER_CYCLE_BACKUP_FILE),
                &previous,
                false,
            )?;
        }
        let payload = encode_dynamic(dynamic)?;
        let water_payload = encode_water_cycle(water_cycle)?;
        let dynamic_checksum = stable_hash(&payload);
        let water_cycle_checksum = stable_hash(&water_payload);
        let container = container_bytes_owned(
            DYNAMIC_MAGIC,
            ATLAS_DYNAMIC_VERSION,
            self.side(),
            DYNAMIC_RECORD_BYTES,
            payload,
        )?;
        let mut manifest = self.manifest.clone();
        manifest.dynamic_checksum = dynamic_checksum;
        manifest.dynamic_bytes = container.len() as u64;
        let water_container = variable_container_bytes_owned(
            WATER_CYCLE_MAGIC,
            WATER_CYCLE_SCHEMA_VERSION,
            self.side(),
            water_payload,
        )?;
        manifest.water_cycle_checksum = water_cycle_checksum;
        manifest.water_cycle_bytes = water_container.len() as u64;
        crate::persist::atomic_write(&path, &container, false)?;
        crate::persist::atomic_write(&water_path, &water_container, false)?;
        write_manifest(&planet_dir, &manifest)
    }

    pub fn save_history(&mut self, world_dir: &Path) -> Result<(), AtlasError> {
        let planet_dir = Self::planet_dir(world_dir);
        let payload = encode_history(&self.history)?;
        if payload.len() as u64 > MAX_HISTORY_BYTES {
            return Err(AtlasError::Corrupt(format!(
                "history is {} bytes; limit is {MAX_HISTORY_BYTES}",
                payload.len()
            )));
        }
        self.manifest.history_checksum = stable_hash(payload.as_bytes());
        crate::persist::atomic_write(&planet_dir.join(HISTORY_FILE), payload.as_bytes(), false)?;
        write_manifest(&planet_dir, &self.manifest)
    }

    fn write_bundle(&self, planet_dir: &Path) -> Result<(), AtlasError> {
        {
            let payload = encode_genesis(&self.genesis)?;
            let container = container_bytes_owned(
                GENESIS_MAGIC,
                ATLAS_FORMAT_VERSION,
                self.side(),
                GENESIS_RECORD_BYTES,
                payload,
            )?;
            if container.len() as u64 > MAX_GENESIS_BYTES {
                return Err(AtlasError::Corrupt(
                    "genesis exceeds its file budget".into(),
                ));
            }
            crate::persist::atomic_write(&planet_dir.join(GENESIS_FILE), &container, false)?;
        }
        {
            let payload = encode_dynamic(&self.dynamic)?;
            let container = container_bytes_owned(
                DYNAMIC_MAGIC,
                ATLAS_DYNAMIC_VERSION,
                self.side(),
                DYNAMIC_RECORD_BYTES,
                payload,
            )?;
            if container.len() as u64 > MAX_DYNAMIC_BYTES {
                return Err(AtlasError::Corrupt(
                    "dynamic atmosphere exceeds its file budget".into(),
                ));
            }
            crate::persist::atomic_write(&planet_dir.join(DYNAMIC_FILE), &container, false)?;
        }
        {
            let payload = encode_water_cycle(&self.water_cycle)?;
            let container = variable_container_bytes_owned(
                WATER_CYCLE_MAGIC,
                WATER_CYCLE_SCHEMA_VERSION,
                self.side(),
                payload,
            )?;
            if container.len() as u64 > MAX_WATER_CYCLE_BYTES {
                return Err(AtlasError::Corrupt(
                    "water cycle exceeds its file budget".into(),
                ));
            }
            crate::persist::atomic_write(&planet_dir.join(WATER_CYCLE_FILE), &container, false)?;
        }
        for (file, payload, limit) in [
            (
                HISTORY_FILE,
                encode_history(&self.history)?,
                MAX_HISTORY_BYTES,
            ),
            (
                GEOLOGY_FILE,
                encode_geology(&self.geology)?,
                MAX_GEOLOGY_BYTES,
            ),
            (
                HYDROLOGY_FILE,
                encode_hydrology(&self.hydrology)?,
                MAX_HYDROLOGY_BYTES,
            ),
            (BIOMES_FILE, encode_biomes(&self.biomes)?, MAX_BIOMES_BYTES),
        ] {
            if payload.len() as u64 > limit {
                return Err(AtlasError::Corrupt(format!(
                    "{file} exceeds its file budget"
                )));
            }
            crate::persist::atomic_write(&planet_dir.join(file), payload.as_bytes(), false)?;
        }
        // The complete manifest is the commit marker and is deliberately last.
        write_manifest(planet_dir, &self.manifest)
    }

    fn load_planet_dir(planet_dir: &Path, production_only: bool) -> Result<Self, AtlasError> {
        let manifest_path = planet_dir.join(MANIFEST_FILE);
        let manifest_bytes = read_bounded(&manifest_path, MAX_MANIFEST_BYTES)?;
        let manifest_text = std::str::from_utf8(&manifest_bytes)
            .map_err(|_| AtlasError::Corrupt("manifest is not UTF-8".into()))?;
        let mut manifest: AtlasManifest = toml::from_str(manifest_text)
            .map_err(|error| AtlasError::Corrupt(format!("manifest TOML: {error}")))?;
        validate_manifest(&manifest, production_only)?;

        let genesis_payload = decode_container(
            &planet_dir.join(GENESIS_FILE),
            GENESIS_MAGIC,
            ATLAS_FORMAT_VERSION,
            manifest.atlas_face_side,
            GENESIS_RECORD_BYTES,
            MAX_GENESIS_BYTES,
        )?;
        if stable_hash(&genesis_payload) != manifest.genesis_checksum {
            return Err(AtlasError::Corrupt(
                "immutable genesis does not match the committed manifest".into(),
            ));
        }
        let genesis = decode_genesis(manifest.atlas_face_side, &genesis_payload)?;

        let dynamic_path = planet_dir.join(DYNAMIC_FILE);
        let dynamic_backup = planet_dir.join(DYNAMIC_BACKUP_FILE);
        let load_dynamic_payload =
            |path: &Path, expected_checksum: Option<u64>| -> Result<Vec<u8>, AtlasError> {
                let payload = decode_container(
                    path,
                    DYNAMIC_MAGIC,
                    ATLAS_DYNAMIC_VERSION,
                    manifest.atlas_face_side,
                    DYNAMIC_RECORD_BYTES,
                    MAX_DYNAMIC_BYTES,
                )?;
                if expected_checksum.is_some_and(|checksum| stable_hash(&payload) != checksum) {
                    return Err(AtlasError::Corrupt(format!(
                        "{} does not match the committed dynamic checksum",
                        path.display()
                    )));
                }
                Ok(payload)
            };
        let mut dynamic = match load_dynamic_payload(&dynamic_path, Some(manifest.dynamic_checksum))
        {
            Ok(payload) => decode_dynamic(manifest.atlas_face_side, &payload)?,
            Err(primary_error) => match load_dynamic_payload(&dynamic_backup, None) {
                Ok(payload) => {
                    let restored = container_bytes(
                        DYNAMIC_MAGIC,
                        ATLAS_DYNAMIC_VERSION,
                        manifest.atlas_face_side,
                        DYNAMIC_RECORD_BYTES,
                        &payload,
                    )?;
                    manifest.dynamic_checksum = stable_hash(&payload);
                    manifest.dynamic_bytes = restored.len() as u64;
                    let water_backup_payload = decode_variable_container(
                        &planet_dir.join(WATER_CYCLE_BACKUP_FILE),
                        WATER_CYCLE_MAGIC,
                        WATER_CYCLE_SCHEMA_VERSION,
                        manifest.atlas_face_side,
                        MAX_WATER_CYCLE_BYTES,
                    )?;
                    let water_restored = variable_container_bytes(
                        WATER_CYCLE_MAGIC,
                        WATER_CYCLE_SCHEMA_VERSION,
                        manifest.atlas_face_side,
                        &water_backup_payload,
                    )?;
                    manifest.water_cycle_checksum = stable_hash(&water_backup_payload);
                    manifest.water_cycle_bytes = water_restored.len() as u64;
                    crate::persist::atomic_write(&dynamic_path, &restored, false)?;
                    crate::persist::atomic_write(
                        &planet_dir.join(WATER_CYCLE_FILE),
                        &water_restored,
                        false,
                    )?;
                    write_manifest(planet_dir, &manifest)?;
                    eprintln!(
                        "warning: restored mutable planet atlas from backup after: {primary_error}"
                    );
                    decode_dynamic(manifest.atlas_face_side, &payload)?
                }
                Err(backup_error) => {
                    return Err(AtlasError::Corrupt(format!(
                        "dynamic atmosphere is unrecoverable; refusing to mint or destroy water (primary: {primary_error}; backup: {backup_error})"
                    )));
                }
            },
        };

        let history_bytes = read_bounded(&planet_dir.join(HISTORY_FILE), MAX_HISTORY_BYTES)?;
        if stable_hash(&history_bytes) != manifest.history_checksum {
            return Err(AtlasError::Corrupt(
                "history overlay does not match the committed manifest".into(),
            ));
        }
        let history_text = std::str::from_utf8(&history_bytes)
            .map_err(|_| AtlasError::Corrupt("history overlay is not UTF-8".into()))?;
        let history: HistoryLayers = toml::from_str(history_text)
            .map_err(|error| AtlasError::Corrupt(format!("history TOML: {error}")))?;
        let geology_bytes = read_bounded(&planet_dir.join(GEOLOGY_FILE), MAX_GEOLOGY_BYTES)?;
        if stable_hash(&geology_bytes) != manifest.geology_checksum
            || geology_bytes.len() as u64 != manifest.geology_bytes
        {
            return Err(AtlasError::Corrupt(
                "geology manifest does not match the committed manifest".into(),
            ));
        }
        let geology_text = std::str::from_utf8(&geology_bytes)
            .map_err(|_| AtlasError::Corrupt("geology manifest is not UTF-8".into()))?;
        let geology: GeologyModel = toml::from_str(geology_text)
            .map_err(|error| AtlasError::Corrupt(format!("geology TOML: {error}")))?;
        let hydrology_bytes = read_bounded(&planet_dir.join(HYDROLOGY_FILE), MAX_HYDROLOGY_BYTES)?;
        if stable_hash(&hydrology_bytes) != manifest.hydrology_checksum
            || hydrology_bytes.len() as u64 != manifest.hydrology_bytes
        {
            return Err(AtlasError::Corrupt(
                "hydrology manifest does not match the committed manifest".into(),
            ));
        }
        let hydrology_text = std::str::from_utf8(&hydrology_bytes)
            .map_err(|_| AtlasError::Corrupt("hydrology manifest is not UTF-8".into()))?;
        let hydrology: HydrologyModel = toml::from_str(hydrology_text)
            .map_err(|error| AtlasError::Corrupt(format!("hydrology TOML: {error}")))?;
        let biome_bytes = read_bounded(&planet_dir.join(BIOMES_FILE), MAX_BIOMES_BYTES)?;
        if stable_hash(&biome_bytes) != manifest.biome_checksum
            || biome_bytes.len() as u64 != manifest.biome_bytes
        {
            return Err(AtlasError::Corrupt(
                "biome/country manifest does not match the committed manifest".into(),
            ));
        }
        let biome_text = std::str::from_utf8(&biome_bytes)
            .map_err(|_| AtlasError::Corrupt("biome manifest is not UTF-8".into()))?;
        let biomes: BiomeModel = toml::from_str(biome_text)
            .map_err(|error| AtlasError::Corrupt(format!("biome TOML: {error}")))?;
        let water_path = planet_dir.join(WATER_CYCLE_FILE);
        let water_backup = planet_dir.join(WATER_CYCLE_BACKUP_FILE);
        let load_water_payload =
            |path: &Path, expected_checksum: Option<u64>| -> Result<Vec<u8>, AtlasError> {
                let payload = decode_variable_container(
                    path,
                    WATER_CYCLE_MAGIC,
                    WATER_CYCLE_SCHEMA_VERSION,
                    manifest.atlas_face_side,
                    MAX_WATER_CYCLE_BYTES,
                )?;
                if expected_checksum.is_some_and(|checksum| stable_hash(&payload) != checksum) {
                    return Err(AtlasError::Corrupt(format!(
                        "{} does not match the committed water-cycle checksum",
                        path.display()
                    )));
                }
                Ok(payload)
            };
        let water_cycle = match load_water_payload(&water_path, Some(manifest.water_cycle_checksum))
        {
            Ok(payload) => decode_water_cycle(manifest.atlas_face_side, &payload)?,
            Err(primary_error) => match load_water_payload(&water_backup, None) {
                Ok(payload) => {
                    let restored = variable_container_bytes(
                        WATER_CYCLE_MAGIC,
                        WATER_CYCLE_SCHEMA_VERSION,
                        manifest.atlas_face_side,
                        &payload,
                    )?;
                    manifest.water_cycle_checksum = stable_hash(&payload);
                    manifest.water_cycle_bytes = restored.len() as u64;
                    let dynamic_backup_payload = decode_container(
                        &planet_dir.join(DYNAMIC_BACKUP_FILE),
                        DYNAMIC_MAGIC,
                        ATLAS_DYNAMIC_VERSION,
                        manifest.atlas_face_side,
                        DYNAMIC_RECORD_BYTES,
                        MAX_DYNAMIC_BYTES,
                    )?;
                    let dynamic_restored = container_bytes(
                        DYNAMIC_MAGIC,
                        ATLAS_DYNAMIC_VERSION,
                        manifest.atlas_face_side,
                        DYNAMIC_RECORD_BYTES,
                        &dynamic_backup_payload,
                    )?;
                    manifest.dynamic_checksum = stable_hash(&dynamic_backup_payload);
                    manifest.dynamic_bytes = dynamic_restored.len() as u64;
                    dynamic = decode_dynamic(manifest.atlas_face_side, &dynamic_backup_payload)?;
                    crate::persist::atomic_write(&water_path, &restored, false)?;
                    crate::persist::atomic_write(
                        &planet_dir.join(DYNAMIC_FILE),
                        &dynamic_restored,
                        false,
                    )?;
                    write_manifest(planet_dir, &manifest)?;
                    eprintln!(
                        "warning: restored planetary water state from backup after: {primary_error}"
                    );
                    decode_water_cycle(manifest.atlas_face_side, &payload)?
                }
                Err(backup_error) => {
                    return Err(AtlasError::Corrupt(format!(
                        "water ledger is unrecoverable; refusing to reset planetary mass (primary: {primary_error}; backup: {backup_error})"
                    )));
                }
            },
        };
        let atlas = Self {
            manifest,
            genesis,
            dynamic,
            water_cycle,
            history,
            geology,
            hydrology,
            biomes,
        };
        atlas.validate()?;
        Ok(atlas)
    }
}

fn write_manifest(planet_dir: &Path, manifest: &AtlasManifest) -> Result<(), AtlasError> {
    let payload = toml::to_string_pretty(manifest)
        .map_err(|error| AtlasError::Corrupt(format!("manifest encoding failed: {error}")))?;
    if payload.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(AtlasError::Corrupt(
            "manifest exceeds its size limit".into(),
        ));
    }
    crate::persist::atomic_write(&planet_dir.join(MANIFEST_FILE), payload.as_bytes(), false)?;
    Ok(())
}

pub(crate) struct ArcaneManifestCheckpoint {
    pub schema_version: u32,
    pub algorithm_version: u32,
    pub unit_scale: u32,
    pub genesis_total: u64,
    pub last_clean_total: u64,
    pub reservoir_totals: [u64; 6],
    pub registry_hash: u64,
    pub ledger_checksum: u64,
    pub delta_checksum: u64,
}

pub(crate) fn update_arcane_manifest(
    world_dir: &Path,
    checkpoint: &ArcaneManifestCheckpoint,
) -> Result<(), AtlasError> {
    let planet_dir = PlanetAtlas::planet_dir(world_dir);
    let bytes = read_bounded(&planet_dir.join(MANIFEST_FILE), MAX_MANIFEST_BYTES)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|error| AtlasError::Corrupt(format!("manifest is not UTF-8: {error}")))?;
    let mut manifest: AtlasManifest = toml::from_str(text)
        .map_err(|error| AtlasError::Corrupt(format!("manifest decode failed: {error}")))?;
    manifest.arcane_schema_version = checkpoint.schema_version;
    manifest.arcane_algorithm_version = checkpoint.algorithm_version;
    manifest.arcane_unit_scale = checkpoint.unit_scale;
    manifest.arcane_genesis_total = checkpoint.genesis_total;
    manifest.arcane_last_clean_total = checkpoint.last_clean_total;
    manifest.arcane_reservoir_totals = checkpoint.reservoir_totals;
    manifest.arcane_registry_hash = checkpoint.registry_hash;
    manifest.arcane_ledger_checksum = checkpoint.ledger_checksum;
    manifest.arcane_delta_checksum = checkpoint.delta_checksum;
    write_manifest(&planet_dir, &manifest)
}

pub(crate) fn verify_arcane_manifest_checkpoint(
    world_dir: &Path,
    checkpoint: &ArcaneManifestCheckpoint,
) -> Result<(), AtlasError> {
    let planet_dir = PlanetAtlas::planet_dir(world_dir);
    let bytes = read_bounded(&planet_dir.join(MANIFEST_FILE), MAX_MANIFEST_BYTES)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|error| AtlasError::Corrupt(format!("manifest is not UTF-8: {error}")))?;
    let manifest: AtlasManifest = toml::from_str(text)
        .map_err(|error| AtlasError::Corrupt(format!("manifest decode failed: {error}")))?;
    let valid = manifest.arcane_schema_version == checkpoint.schema_version
        && manifest.arcane_algorithm_version == checkpoint.algorithm_version
        && manifest.arcane_unit_scale == checkpoint.unit_scale
        && manifest.arcane_genesis_total == checkpoint.genesis_total
        && manifest.arcane_last_clean_total == checkpoint.last_clean_total
        && manifest.arcane_reservoir_totals == checkpoint.reservoir_totals
        && manifest.arcane_registry_hash == checkpoint.registry_hash
        && manifest.arcane_ledger_checksum == checkpoint.ledger_checksum;
    if !valid {
        return Err(AtlasError::Corrupt(
            "arcane checkpoint does not match the qualified planet manifest".into(),
        ));
    }
    Ok(())
}

pub(crate) struct ArcaneGeographyManifestCheckpoint {
    pub schema_version: u32,
    pub algorithm_version: u32,
    pub dynamic_version: u32,
    pub genesis_total: u64,
    pub immutable_checksum: u64,
    pub dynamic_checksum: u64,
    pub site_catalog_checksum: u64,
    pub last_authoritative_time: u64,
}

pub(crate) fn update_arcane_geography_manifest(
    world_dir: &Path,
    checkpoint: &ArcaneGeographyManifestCheckpoint,
) -> Result<(), AtlasError> {
    let planet_dir = PlanetAtlas::planet_dir(world_dir);
    let bytes = read_bounded(&planet_dir.join(MANIFEST_FILE), MAX_MANIFEST_BYTES)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|error| AtlasError::Corrupt(format!("manifest is not UTF-8: {error}")))?;
    let mut manifest: AtlasManifest = toml::from_str(text)
        .map_err(|error| AtlasError::Corrupt(format!("manifest decode failed: {error}")))?;
    manifest.arcane_geography_schema_version = checkpoint.schema_version;
    manifest.arcane_geography_algorithm_version = checkpoint.algorithm_version;
    manifest.arcane_geography_dynamic_version = checkpoint.dynamic_version;
    manifest.arcane_geography_genesis_total = checkpoint.genesis_total;
    manifest.arcane_geography_immutable_checksum = checkpoint.immutable_checksum;
    manifest.arcane_geography_dynamic_checksum = checkpoint.dynamic_checksum;
    manifest.arcane_geography_site_catalog_checksum = checkpoint.site_catalog_checksum;
    manifest.arcane_geography_last_authoritative_time = checkpoint.last_authoritative_time;
    write_manifest(&planet_dir, &manifest)
}

pub(crate) fn arcane_geography_manifest_payload(
    world_dir: &Path,
    checkpoint: &ArcaneGeographyManifestCheckpoint,
) -> Result<Vec<u8>, AtlasError> {
    let planet_dir = PlanetAtlas::planet_dir(world_dir);
    let bytes = read_bounded(&planet_dir.join(MANIFEST_FILE), MAX_MANIFEST_BYTES)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|error| AtlasError::Corrupt(format!("manifest is not UTF-8: {error}")))?;
    let mut manifest: AtlasManifest = toml::from_str(text)
        .map_err(|error| AtlasError::Corrupt(format!("manifest decode failed: {error}")))?;
    manifest.arcane_geography_schema_version = checkpoint.schema_version;
    manifest.arcane_geography_algorithm_version = checkpoint.algorithm_version;
    manifest.arcane_geography_dynamic_version = checkpoint.dynamic_version;
    manifest.arcane_geography_genesis_total = checkpoint.genesis_total;
    manifest.arcane_geography_immutable_checksum = checkpoint.immutable_checksum;
    manifest.arcane_geography_dynamic_checksum = checkpoint.dynamic_checksum;
    manifest.arcane_geography_site_catalog_checksum = checkpoint.site_catalog_checksum;
    manifest.arcane_geography_last_authoritative_time = checkpoint.last_authoritative_time;
    let payload = toml::to_string_pretty(&manifest)
        .map_err(|error| AtlasError::Corrupt(format!("manifest encoding failed: {error}")))?;
    if payload.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(AtlasError::Corrupt(
            "manifest exceeds its size limit".into(),
        ));
    }
    Ok(payload.into_bytes())
}

pub(crate) fn verify_arcane_geography_manifest(
    world_dir: &Path,
    checkpoint: &ArcaneGeographyManifestCheckpoint,
) -> Result<(), AtlasError> {
    let planet_dir = PlanetAtlas::planet_dir(world_dir);
    let bytes = read_bounded(&planet_dir.join(MANIFEST_FILE), MAX_MANIFEST_BYTES)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|error| AtlasError::Corrupt(format!("manifest is not UTF-8: {error}")))?;
    let manifest: AtlasManifest = toml::from_str(text)
        .map_err(|error| AtlasError::Corrupt(format!("manifest decode failed: {error}")))?;
    let valid = manifest.arcane_geography_schema_version == checkpoint.schema_version
        && manifest.arcane_geography_algorithm_version == checkpoint.algorithm_version
        && manifest.arcane_geography_dynamic_version == checkpoint.dynamic_version
        && manifest.arcane_geography_genesis_total == checkpoint.genesis_total
        && manifest.arcane_geography_immutable_checksum == checkpoint.immutable_checksum
        && manifest.arcane_geography_dynamic_checksum == checkpoint.dynamic_checksum
        && manifest.arcane_geography_site_catalog_checksum == checkpoint.site_catalog_checksum
        && manifest.arcane_geography_last_authoritative_time == checkpoint.last_authoritative_time;
    if !valid {
        return Err(AtlasError::Corrupt(
            "arcane geography checkpoint does not match the planet manifest".into(),
        ));
    }
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), AtlasError> {
    #[cfg(unix)]
    fs::File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn read_bounded(path: &Path, max_bytes: u64) -> Result<Vec<u8>, AtlasError> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > max_bytes {
        return Err(AtlasError::Corrupt(format!(
            "{} is {} bytes; limit is {max_bytes}",
            path.display(),
            metadata.len()
        )));
    }
    Ok(fs::read(path)?)
}

fn container_bytes(
    magic: &[u8; 4],
    version: u32,
    side: u16,
    record_bytes: usize,
    payload: &[u8],
) -> Result<Vec<u8>, AtlasError> {
    let count = atlas_count(side)?;
    let expected = count
        .checked_mul(record_bytes)
        .and_then(|bytes| {
            bytes.checked_add(if magic == DYNAMIC_MAGIC {
                DYNAMIC_PREFIX_BYTES
            } else {
                0
            })
        })
        .ok_or_else(|| AtlasError::Corrupt("atlas payload size overflow".into()))?;
    if payload.len() != expected || record_bytes > u16::MAX as usize {
        return Err(AtlasError::Corrupt(format!(
            "payload has {} bytes; expected {expected}",
            payload.len()
        )));
    }
    let mut out = Vec::with_capacity(FILE_HEADER_BYTES + payload.len());
    out.extend_from_slice(magic);
    put_u32(&mut out, version);
    put_u16(&mut out, side);
    put_u16(&mut out, record_bytes as u16);
    put_u32(
        &mut out,
        count.try_into().expect("atlas cell count fits u32"),
    );
    put_u64(&mut out, payload.len() as u64);
    put_u64(&mut out, stable_hash(payload));
    debug_assert_eq!(out.len(), FILE_HEADER_BYTES);
    out.extend_from_slice(payload);
    Ok(out)
}

fn container_bytes_owned(
    magic: &[u8; 4],
    version: u32,
    side: u16,
    record_bytes: usize,
    payload: Vec<u8>,
) -> Result<Vec<u8>, AtlasError> {
    let count = atlas_count(side)?;
    let expected = count
        .checked_mul(record_bytes)
        .and_then(|bytes| {
            bytes.checked_add(if magic == DYNAMIC_MAGIC {
                DYNAMIC_PREFIX_BYTES
            } else {
                0
            })
        })
        .ok_or_else(|| AtlasError::Corrupt("atlas payload size overflow".into()))?;
    if payload.len() != expected || record_bytes > u16::MAX as usize {
        return Err(AtlasError::Corrupt(format!(
            "payload has {} bytes; expected {expected}",
            payload.len()
        )));
    }
    prepend_container_header(magic, version, side, record_bytes as u16, count, payload)
}

fn variable_container_bytes(
    magic: &[u8; 4],
    version: u32,
    side: u16,
    payload: &[u8],
) -> Result<Vec<u8>, AtlasError> {
    let count = atlas_count(side)?;
    let mut out = Vec::with_capacity(FILE_HEADER_BYTES + payload.len());
    out.extend_from_slice(magic);
    put_u32(&mut out, version);
    put_u16(&mut out, side);
    put_u16(&mut out, 0);
    put_u32(
        &mut out,
        count.try_into().expect("atlas cell count fits u32"),
    );
    put_u64(&mut out, payload.len() as u64);
    put_u64(&mut out, stable_hash(payload));
    debug_assert_eq!(out.len(), FILE_HEADER_BYTES);
    out.extend_from_slice(payload);
    Ok(out)
}

fn variable_container_bytes_owned(
    magic: &[u8; 4],
    version: u32,
    side: u16,
    payload: Vec<u8>,
) -> Result<Vec<u8>, AtlasError> {
    let count = atlas_count(side)?;
    prepend_container_header(magic, version, side, 0, count, payload)
}

fn prepend_container_header(
    magic: &[u8; 4],
    version: u32,
    side: u16,
    record_bytes: u16,
    count: usize,
    mut payload: Vec<u8>,
) -> Result<Vec<u8>, AtlasError> {
    let payload_len = payload.len();
    let checksum = stable_hash(&payload);
    payload.reserve(FILE_HEADER_BYTES);
    payload.resize(payload_len + FILE_HEADER_BYTES, 0);
    payload.copy_within(0..payload_len, FILE_HEADER_BYTES);
    let mut header = Vec::with_capacity(FILE_HEADER_BYTES);
    header.extend_from_slice(magic);
    put_u32(&mut header, version);
    put_u16(&mut header, side);
    put_u16(&mut header, record_bytes);
    put_u32(
        &mut header,
        count
            .try_into()
            .map_err(|_| AtlasError::Corrupt("atlas cell count exceeds u32".into()))?,
    );
    put_u64(&mut header, payload_len as u64);
    put_u64(&mut header, checksum);
    debug_assert_eq!(header.len(), FILE_HEADER_BYTES);
    payload[..FILE_HEADER_BYTES].copy_from_slice(&header);
    Ok(payload)
}

fn decode_variable_container(
    path: &Path,
    expected_magic: &[u8; 4],
    expected_version: u32,
    expected_side: u16,
    max_bytes: u64,
) -> Result<Vec<u8>, AtlasError> {
    let mut bytes = read_bounded(path, max_bytes)?;
    if bytes.len() < FILE_HEADER_BYTES {
        return Err(AtlasError::Corrupt(format!(
            "{} is shorter than its header",
            path.display()
        )));
    }
    let mut header = ByteReader::new(&bytes[..FILE_HEADER_BYTES]);
    let magic = header.array::<4>()?;
    let version = header.u32()?;
    let side = header.u16()?;
    let record_bytes = header.u16()?;
    let count = header.u32()? as usize;
    let payload_len = usize::try_from(header.u64()?)
        .map_err(|_| AtlasError::Corrupt("payload length does not fit memory".into()))?;
    let checksum = header.u64()?;
    if magic != *expected_magic
        || version != expected_version
        || side != expected_side
        || record_bytes != 0
        || count != atlas_count(expected_side)?
        || bytes.len() != FILE_HEADER_BYTES.saturating_add(payload_len)
    {
        return Err(AtlasError::Corrupt(format!(
            "{} has inconsistent water-container metadata",
            path.display()
        )));
    }
    let payload = bytes.split_off(FILE_HEADER_BYTES);
    if stable_hash(&payload) != checksum {
        return Err(AtlasError::Corrupt(format!(
            "{} failed its file checksum",
            path.display()
        )));
    }
    Ok(payload)
}

fn decode_container(
    path: &Path,
    expected_magic: &[u8; 4],
    expected_version: u32,
    expected_side: u16,
    expected_record_bytes: usize,
    max_bytes: u64,
) -> Result<Vec<u8>, AtlasError> {
    let mut bytes = read_bounded(path, max_bytes)?;
    if bytes.len() < FILE_HEADER_BYTES {
        return Err(AtlasError::Corrupt(format!(
            "{} is shorter than its header",
            path.display()
        )));
    }
    let mut header = ByteReader::new(&bytes[..FILE_HEADER_BYTES]);
    let magic = header.array::<4>()?;
    let version = header.u32()?;
    let side = header.u16()?;
    let record_bytes = usize::from(header.u16()?);
    let count = header.u32()? as usize;
    let payload_len = usize::try_from(header.u64()?)
        .map_err(|_| AtlasError::Corrupt("payload length does not fit memory".into()))?;
    let checksum = header.u64()?;
    let expected_count = atlas_count(expected_side)?;
    let expected_payload = expected_count
        .checked_mul(expected_record_bytes)
        .and_then(|bytes| {
            bytes.checked_add(if expected_magic == DYNAMIC_MAGIC {
                DYNAMIC_PREFIX_BYTES
            } else {
                0
            })
        })
        .ok_or_else(|| AtlasError::Corrupt("atlas payload size overflow".into()))?;
    if magic != *expected_magic {
        return Err(AtlasError::Corrupt(format!(
            "{} has the wrong file magic",
            path.display()
        )));
    }
    if version != expected_version {
        return Err(AtlasError::UnsupportedVersion(format!(
            "{} schema {version} (supported {expected_version})",
            path.display()
        )));
    }
    if side != expected_side
        || count != expected_count
        || record_bytes != expected_record_bytes
        || payload_len != expected_payload
        || bytes.len() != FILE_HEADER_BYTES + payload_len
    {
        return Err(AtlasError::Corrupt(format!(
            "{} has inconsistent dimensions or length",
            path.display()
        )));
    }
    let payload = bytes.split_off(FILE_HEADER_BYTES);
    if stable_hash(&payload) != checksum {
        return Err(AtlasError::Corrupt(format!(
            "{} failed its file checksum",
            path.display()
        )));
    }
    Ok(payload)
}

struct ByteReader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> ByteReader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], AtlasError> {
        let end = self
            .cursor
            .checked_add(N)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| AtlasError::Corrupt("record ended unexpectedly".into()))?;
        let mut out = [0; N];
        out.copy_from_slice(&self.bytes[self.cursor..end]);
        self.cursor = end;
        Ok(out)
    }

    fn bytes(&mut self, len: usize) -> Result<&'a [u8], AtlasError> {
        let end = self
            .cursor
            .checked_add(len)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| AtlasError::Corrupt("record ended unexpectedly".into()))?;
        let out = &self.bytes[self.cursor..end];
        self.cursor = end;
        Ok(out)
    }

    fn is_empty(&self) -> bool {
        self.cursor == self.bytes.len()
    }

    fn u8(&mut self) -> Result<u8, AtlasError> {
        Ok(self.array::<1>()?[0])
    }

    fn u16(&mut self) -> Result<u16, AtlasError> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    fn u32(&mut self) -> Result<u32, AtlasError> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, AtlasError> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    fn i32(&mut self) -> Result<i32, AtlasError> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    fn i16(&mut self) -> Result<i16, AtlasError> {
        Ok(i16::from_le_bytes(self.array()?))
    }

    fn f32(&mut self) -> Result<f32, AtlasError> {
        Ok(f32::from_bits(self.u32()?))
    }
}

fn decode_genesis(side: u16, payload: &[u8]) -> Result<GenesisLayers, AtlasError> {
    let count = atlas_count(side)?;
    if payload.len() != count * GENESIS_RECORD_BYTES {
        return Err(AtlasError::Corrupt("genesis payload width mismatch".into()));
    }
    let mut reader = ByteReader::new(payload);
    let mut geometry = Vec::with_capacity(count);
    let mut tectonics = Vec::with_capacity(count);
    let mut terrain = Vec::with_capacity(count);
    let mut climate = Vec::with_capacity(count);
    let mut hydrology = Vec::with_capacity(count);
    let mut ground = Vec::with_capacity(count);
    let mut biomes = Vec::with_capacity(count);
    let mut resources = Vec::with_capacity(count);
    for _ in 0..count {
        geometry.push(GeometryCell {
            unit_direction: [reader.f32()?, reader.f32()?, reader.f32()?],
            latitude_radians: reader.f32()?,
            physical_area: reader.f32()?,
        });
        tectonics.push(TectonicCell {
            plate_id: reader.u16()?,
            boundary: BoundaryClass::from_u8(reader.u8()?)?,
            boundary_detail: DetailedBoundary::from_u8(reader.u8()?)?,
            neighbor_plate: reader.u16()?,
            boundary_strength: reader.f32()?,
            boundary_distance: reader.u16()?,
            boundary_strike: [reader.i16()?, reader.i16()?, reader.i16()?],
            continental_crust: reader.u16()?,
            craton_id: reader.u16()?,
            crust_age: reader.u16()?,
            oceanic_age: reader.u16()?,
            crust_thickness: reader.u16()?,
            bedrock_family: reader.u16()?,
            geological_province: reader.u16()?,
            stratigraphic_stack: reader.u16()?,
            metamorphic_grade: reader.u8()?,
            fault_intensity: reader.u16()?,
            volcanic_history: reader.u8()?,
            sediment_basin: BasinKind::from_u8(reader.u8()?)?,
        });
        terrain.push(TerrainCell {
            base_elevation: reader.f32()?,
            eroded_elevation: reader.f32()?,
            tectonic_contribution: reader.f32()?,
            volcanic_contribution: reader.f32()?,
            dynamic_topography: reader.f32()?,
            landmass_id: reader.u16()?,
        });
        climate.push(ClimateCell {
            mean_temperature: reader.f32()?,
            seasonality: reader.f32()?,
            ocean_temperature_anomaly: reader.f32()?,
            continentality: reader.f32()?,
            prevailing_wind: [reader.f32()?, reader.f32()?],
            ocean_current: [reader.f32()?, reader.f32()?],
            mean_atmospheric_moisture: reader.f32()?,
            mean_precipitation: reader.f32()?,
            precipitation_seasonality: reader.f32()?,
            potential_evapotranspiration: reader.f32()?,
            aridity: reader.f32()?,
            snow_persistence: reader.f32()?,
            seasonal_temperature: [reader.f32()?, reader.f32()?, reader.f32()?, reader.f32()?],
            seasonal_precipitation: [reader.f32()?, reader.f32()?, reader.f32()?, reader.f32()?],
            seasonal_wind: [
                [reader.f32()?, reader.f32()?],
                [reader.f32()?, reader.f32()?],
                [reader.f32()?, reader.f32()?],
                [reader.f32()?, reader.f32()?],
            ],
        });
        hydrology.push(HydrologyCell {
            drainage_receiver: reader.u32()?,
            watershed_id: reader.u32()?,
            ocean_basin_id: reader.u16()?,
            lake_basin_id: reader.u32()?,
            spill_elevation: reader.f32()?,
            filled_elevation: reader.f32()?,
            mean_runoff: reader.f32()?,
            seasonal_runoff_fraction: [reader.u16()?, reader.u16()?, reader.u16()?, reader.u16()?],
            mean_discharge: reader.f32()?,
            seasonal_discharge_fraction: [
                reader.u16()?,
                reader.u16()?,
                reader.u16()?,
                reader.u16()?,
            ],
            catchment_area: reader.f32()?,
            channel_bed_elevation: reader.f32()?,
            water_surface_elevation: reader.f32()?,
            channel_width_centiblocks: reader.u16()?,
            channel_depth_centiblocks: reader.u16()?,
            sediment_energy: reader.u16()?,
            baseline_water_units: reader.u64()?,
            voxel_volume_residual: reader.i32()?,
            river_id: reader.u32()?,
            erosion_centiblocks: reader.i16()?,
            deposition_centiblocks: reader.i16()?,
            seasonal_level_range_centiblocks: reader.u16()?,
            flags: reader.u16()?,
            stream_order: reader.u8()?,
            salinity: reader.u8()?,
            water_body: WaterBodyKind::from_u8(reader.u8()?)?,
        });
        ground.push(GroundCell {
            soil_parent_material: reader.u16()?,
            aquifer_capacity: reader.u32()?,
            aquifer_permeability: reader.u16()?,
            porosity: reader.u16()?,
            baseline_groundwater_head: reader.f32()?,
            soil_depth_decimeters: reader.u8()?,
            sand: reader.u8()?,
            silt: reader.u8()?,
            organic: reader.u8()?,
            baseline_fertility: reader.u8()?,
            drainage: reader.u8()?,
            soil_salinity: reader.u8()?,
            freeze_flags: reader.u8()?,
            erosion_susceptibility: reader.u8()?,
        });
        biomes.push(BiomeCell {
            baseline_biome: reader.u8()?,
            edaphic_flags: reader.u16()?,
            habitat_flags: reader.u32()?,
            vegetation_potential: reader.u8()?,
            tree_line_y: reader.u8()?,
            succession_potential: reader.u8()?,
            country_id: reader.u16()?,
            heart_assignment: reader.u16()?,
        });
        resources.push(ResourceCell {
            deposit_site_ref: reader.u32()?,
            deposit_site_count: reader.u16()?,
        });
    }
    Ok(GenesisLayers {
        geometry: AtlasGrid::from_values(side, geometry)?,
        tectonics: AtlasGrid::from_values(side, tectonics)?,
        terrain: AtlasGrid::from_values(side, terrain)?,
        climate: AtlasGrid::from_values(side, climate)?,
        hydrology: AtlasGrid::from_values(side, hydrology)?,
        ground: AtlasGrid::from_values(side, ground)?,
        biomes: AtlasGrid::from_values(side, biomes)?,
        resources: AtlasGrid::from_values(side, resources)?,
    })
}

fn decode_dynamic(side: u16, payload: &[u8]) -> Result<DynamicLayers, AtlasError> {
    let count = atlas_count(side)?;
    if payload.len() != DYNAMIC_PREFIX_BYTES + count * DYNAMIC_RECORD_BYTES {
        return Err(AtlasError::Corrupt("dynamic payload width mismatch".into()));
    }
    let mut reader = ByteReader::new(payload);
    let completed_climate_hours = reader.u64()?;
    let mut cells = Vec::with_capacity(count);
    for _ in 0..count {
        cells.push(DynamicCell {
            atmospheric_vapor: reader.u32()?,
            cloud_water: reader.u32()?,
            local_weather_anomaly: reader.i32()?,
            weather_temperature_anomaly: reader.i16()?,
            pressure_anomaly: reader.i16()?,
            storm_energy: reader.u16()?,
            precipitation_rate: reader.u16()?,
            wind_anomaly: [reader.i16()?, reader.i16()?],
            fire_moisture_anomaly: reader.i32()?,
            vegetation_moisture_anomaly: reader.i32()?,
        });
    }
    Ok(DynamicLayers {
        completed_climate_hours,
        cells: AtlasGrid::from_values(side, cells)?,
    })
}

fn validate_geometry(grid: &AtlasGrid<GeometryCell>) -> Result<(), AtlasError> {
    if grid.values().iter().any(|cell| {
        !cell.latitude_radians.is_finite()
            || !cell.physical_area.is_finite()
            || cell.physical_area <= 0.0
            || cell.unit_direction.iter().any(|value| !value.is_finite())
            || (Vec3::from_array(cell.unit_direction).length() - 1.0).abs() > 0.001
    }) {
        return Err(AtlasError::Corrupt(
            "geometry contains an invalid direction, latitude, or area".into(),
        ));
    }
    Ok(())
}

fn validate_tectonics(grid: &AtlasGrid<TectonicCell>) -> Result<(), AtlasError> {
    if grid.values().iter().any(|cell| cell.plate_id >= 24) {
        return Err(AtlasError::Corrupt(
            "tectonic layer contains an invalid plate identifier".into(),
        ));
    }
    Ok(())
}

fn validate_terrain(grid: &AtlasGrid<TerrainCell>) -> Result<(), AtlasError> {
    if grid.values().iter().any(|cell| {
        !cell.base_elevation.is_finite()
            || !cell.eroded_elevation.is_finite()
            || !(0.0..crate::chunk::CHUNK_Y as f32).contains(&cell.base_elevation)
            || !(0.0..crate::chunk::CHUNK_Y as f32).contains(&cell.eroded_elevation)
    }) {
        return Err(AtlasError::Corrupt(
            "terrain layer contains an invalid elevation".into(),
        ));
    }
    Ok(())
}

fn validate_climate(grid: &AtlasGrid<ClimateCell>) -> Result<(), AtlasError> {
    if grid.values().iter().any(|cell| {
        !cell.mean_temperature.is_finite()
            || !cell.seasonality.is_finite()
            || !cell.ocean_temperature_anomaly.is_finite()
            || !cell.continentality.is_finite()
            || !(0.0..=1.0).contains(&cell.continentality)
            || !cell.mean_atmospheric_moisture.is_finite()
            || !cell.mean_precipitation.is_finite()
            || cell.mean_precipitation < 0.0
            || !cell.precipitation_seasonality.is_finite()
            || !cell.potential_evapotranspiration.is_finite()
            || !cell.aridity.is_finite()
            || !cell.snow_persistence.is_finite()
            || !(0.0..=1.0).contains(&cell.snow_persistence)
            || cell.prevailing_wind.iter().any(|value| !value.is_finite())
            || cell.ocean_current.iter().any(|value| !value.is_finite())
            || cell
                .seasonal_temperature
                .iter()
                .chain(&cell.seasonal_precipitation)
                .any(|value| !value.is_finite())
            || cell
                .seasonal_wind
                .iter()
                .flatten()
                .any(|value| !value.is_finite())
    }) {
        return Err(AtlasError::Corrupt(
            "climate layer contains a non-finite or negative value".into(),
        ));
    }
    Ok(())
}

fn validate_hydrology(grid: &AtlasGrid<HydrologyCell>) -> Result<(), AtlasError> {
    if grid.values().iter().any(|cell| {
        (cell.drainage_receiver != u32::MAX && cell.drainage_receiver as usize >= grid.len())
            || !cell.spill_elevation.is_finite()
            || !cell.filled_elevation.is_finite()
            || !cell.mean_runoff.is_finite()
            || cell.mean_runoff < 0.0
            || !cell.mean_discharge.is_finite()
            || cell.mean_discharge < 0.0
            || !cell.catchment_area.is_finite()
            || cell.catchment_area < 0.0
            || !cell.channel_bed_elevation.is_finite()
            || !cell.water_surface_elevation.is_finite()
            || cell
                .seasonal_runoff_fraction
                .iter()
                .map(|value| u32::from(*value))
                .sum::<u32>()
                != 65_535
            || cell
                .seasonal_discharge_fraction
                .iter()
                .map(|value| u32::from(*value))
                .sum::<u32>()
                != 65_535
    }) {
        return Err(AtlasError::Corrupt(
            "hydrology layer contains an invalid receiver, budget, or elevation".into(),
        ));
    }
    Ok(())
}

fn validate_ground(grid: &AtlasGrid<GroundCell>) -> Result<(), AtlasError> {
    if grid
        .values()
        .iter()
        .any(|cell| !cell.baseline_groundwater_head.is_finite())
    {
        return Err(AtlasError::Corrupt(
            "ground layer contains a non-finite groundwater head".into(),
        ));
    }
    Ok(())
}

fn validate_biomes(grid: &AtlasGrid<BiomeCell>) -> Result<(), AtlasError> {
    if grid
        .values()
        .iter()
        .any(|cell| !(1..=BIOME_OCEAN).contains(&cell.baseline_biome))
    {
        return Err(AtlasError::Corrupt(
            "biome layer contains an unknown zonal biome".into(),
        ));
    }
    Ok(())
}

fn validate_layers(
    side: u16,
    genesis: &GenesisLayers,
    dynamic: &DynamicLayers,
) -> Result<(), AtlasError> {
    let expected = atlas_count(side)?;
    for (name, grid_side, len) in [
        ("geometry", genesis.geometry.side(), genesis.geometry.len()),
        (
            "tectonics",
            genesis.tectonics.side(),
            genesis.tectonics.len(),
        ),
        ("terrain", genesis.terrain.side(), genesis.terrain.len()),
        ("climate", genesis.climate.side(), genesis.climate.len()),
        (
            "hydrology",
            genesis.hydrology.side(),
            genesis.hydrology.len(),
        ),
        ("ground", genesis.ground.side(), genesis.ground.len()),
        ("biomes", genesis.biomes.side(), genesis.biomes.len()),
        (
            "resources",
            genesis.resources.side(),
            genesis.resources.len(),
        ),
        ("dynamic", dynamic.cells.side(), dynamic.cells.len()),
    ] {
        if grid_side != side || len != expected {
            return Err(AtlasError::Corrupt(format!(
                "{name} layer has side {grid_side} and {len} cells; expected side {side} and {expected}"
            )));
        }
    }
    validate_geometry(&genesis.geometry)?;
    validate_tectonics(&genesis.tectonics)?;
    validate_terrain(&genesis.terrain)?;
    validate_climate(&genesis.climate)?;
    validate_hydrology(&genesis.hydrology)?;
    validate_ground(&genesis.ground)?;
    validate_biomes(&genesis.biomes)?;
    Ok(())
}

fn validate_manifest(manifest: &AtlasManifest, production_only: bool) -> Result<(), AtlasError> {
    if manifest.format_version != ATLAS_FORMAT_VERSION {
        return Err(AtlasError::UnsupportedVersion(format!(
            "format {} (supported {})",
            manifest.format_version, ATLAS_FORMAT_VERSION
        )));
    }
    if manifest.topology != crate::world::WORLD_TOPOLOGY
        || manifest.face_blocks != FACE_BLOCKS
        || manifest.topology_version != 1
        || (manifest.planet_radius - PLANET_RADIUS).abs() > 0.001
        || manifest.rotation_axis != ROTATION_AXIS
        || manifest.prime_meridian != PRIME_MERIDIAN
        || (manifest.axial_tilt_degrees - AXIAL_TILT_DEGREES).abs() > 1.0e-9
    {
        return Err(AtlasError::UnsupportedVersion(
            "topology parameters do not match this build".into(),
        ));
    }
    if manifest.atlas_algorithm_version > ATLAS_ALGORITHM_VERSION
        || manifest.dynamic_schema_version != ATLAS_DYNAMIC_VERSION
        || manifest.water_cycle_schema_version != WATER_CYCLE_SCHEMA_VERSION
        || manifest.history_schema_version != ATLAS_HISTORY_VERSION
        || manifest.geology_schema_version != GEOLOGY_SCHEMA_VERSION
        || manifest.hydrology_schema_version != HYDROLOGY_SCHEMA_VERSION
        || manifest.biome_schema_version != BIOME_SCHEMA_VERSION
        || manifest.layer_versions != layer_versions()
    {
        return Err(AtlasError::UnsupportedVersion(
            "one or more atlas layer schemas do not match this build".into(),
        ));
    }
    if manifest
        .climate_convergence_iterations
        .iter()
        .any(|iterations| *iterations == 0 || *iterations > CLIMATE_MAX_ITERATIONS)
        || !manifest.climate_max_residual.is_finite()
        || f64::from(manifest.climate_max_residual) > CLIMATE_CONVERGENCE_TOLERANCE
        || !manifest.climate_moisture_budget_error.is_finite()
        || manifest.climate_moisture_budget_error < 0.0
    {
        return Err(AtlasError::Corrupt(
            "climate convergence or moisture-budget evidence is invalid".into(),
        ));
    }
    let expected = atlas_count(manifest.atlas_face_side)?;
    if manifest.atlas_cell_count as usize != expected
        || manifest.atlas_cell_blocks != FACE_BLOCKS / manifest.atlas_face_side
    {
        return Err(AtlasError::Corrupt(
            "manifest dimensions are internally inconsistent".into(),
        ));
    }
    if production_only && manifest.atlas_face_side != ATLAS_FACE_SIDE {
        return Err(AtlasError::UnsupportedVersion(format!(
            "fixture atlas side {} cannot be opened as a production planet",
            manifest.atlas_face_side
        )));
    }
    if !manifest.complete {
        return Err(AtlasError::Incomplete(
            "manifest was not committed as complete".into(),
        ));
    }
    Ok(())
}

// Fixed-width encoders keep file allocation bounded before any payload is
// trusted. The explicit order is the immutable on-disk schema for version 1.
fn encode_history(history: &HistoryLayers) -> Result<String, AtlasError> {
    toml::to_string_pretty(history)
        .map_err(|error| AtlasError::Corrupt(format!("history encoding failed: {error}")))
}

fn encode_geology(geology: &GeologyModel) -> Result<String, AtlasError> {
    toml::to_string_pretty(geology)
        .map_err(|error| AtlasError::Corrupt(format!("geology encoding failed: {error}")))
}

fn encode_hydrology(hydrology: &HydrologyModel) -> Result<String, AtlasError> {
    toml::to_string_pretty(hydrology)
        .map_err(|error| AtlasError::Corrupt(format!("hydrology encoding failed: {error}")))
}

fn encode_biomes(biomes: &BiomeModel) -> Result<String, AtlasError> {
    toml::to_string_pretty(biomes)
        .map_err(|error| AtlasError::Corrupt(format!("biome encoding failed: {error}")))
}

fn put_u8(out: &mut Vec<u8>, value: u8) {
    out.push(value);
}
fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn put_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn put_i16(out: &mut Vec<u8>, value: i16) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn put_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_bits().to_le_bytes());
}

fn encode_genesis(genesis: &GenesisLayers) -> Result<Vec<u8>, AtlasError> {
    let count = genesis.geometry.len();
    let mut out = Vec::with_capacity(count * GENESIS_RECORD_BYTES + FILE_HEADER_BYTES);
    for index in 0..count {
        let geometry = genesis.geometry.values()[index];
        let tectonics = genesis.tectonics.values()[index];
        let terrain = genesis.terrain.values()[index];
        let climate = genesis.climate.values()[index];
        let hydrology = genesis.hydrology.values()[index];
        let ground = genesis.ground.values()[index];
        let biome = genesis.biomes.values()[index];
        let resources = genesis.resources.values()[index];
        for value in geometry.unit_direction {
            put_f32(&mut out, value);
        }
        put_f32(&mut out, geometry.latitude_radians);
        put_f32(&mut out, geometry.physical_area);
        put_u16(&mut out, tectonics.plate_id);
        put_u8(&mut out, tectonics.boundary as u8);
        put_u8(&mut out, tectonics.boundary_detail as u8);
        put_u16(&mut out, tectonics.neighbor_plate);
        put_f32(&mut out, tectonics.boundary_strength);
        put_u16(&mut out, tectonics.boundary_distance);
        for value in tectonics.boundary_strike {
            put_i16(&mut out, value);
        }
        put_u16(&mut out, tectonics.continental_crust);
        put_u16(&mut out, tectonics.craton_id);
        put_u16(&mut out, tectonics.crust_age);
        put_u16(&mut out, tectonics.oceanic_age);
        put_u16(&mut out, tectonics.crust_thickness);
        put_u16(&mut out, tectonics.bedrock_family);
        put_u16(&mut out, tectonics.geological_province);
        put_u16(&mut out, tectonics.stratigraphic_stack);
        put_u8(&mut out, tectonics.metamorphic_grade);
        put_u16(&mut out, tectonics.fault_intensity);
        put_u8(&mut out, tectonics.volcanic_history);
        put_u8(&mut out, tectonics.sediment_basin as u8);
        put_f32(&mut out, terrain.base_elevation);
        put_f32(&mut out, terrain.eroded_elevation);
        put_f32(&mut out, terrain.tectonic_contribution);
        put_f32(&mut out, terrain.volcanic_contribution);
        put_f32(&mut out, terrain.dynamic_topography);
        put_u16(&mut out, terrain.landmass_id);
        put_f32(&mut out, climate.mean_temperature);
        put_f32(&mut out, climate.seasonality);
        put_f32(&mut out, climate.ocean_temperature_anomaly);
        put_f32(&mut out, climate.continentality);
        put_f32(&mut out, climate.prevailing_wind[0]);
        put_f32(&mut out, climate.prevailing_wind[1]);
        put_f32(&mut out, climate.ocean_current[0]);
        put_f32(&mut out, climate.ocean_current[1]);
        put_f32(&mut out, climate.mean_atmospheric_moisture);
        put_f32(&mut out, climate.mean_precipitation);
        put_f32(&mut out, climate.precipitation_seasonality);
        put_f32(&mut out, climate.potential_evapotranspiration);
        put_f32(&mut out, climate.aridity);
        put_f32(&mut out, climate.snow_persistence);
        for value in climate.seasonal_temperature {
            put_f32(&mut out, value);
        }
        for value in climate.seasonal_precipitation {
            put_f32(&mut out, value);
        }
        for wind in climate.seasonal_wind {
            put_f32(&mut out, wind[0]);
            put_f32(&mut out, wind[1]);
        }
        put_u32(&mut out, hydrology.drainage_receiver);
        put_u32(&mut out, hydrology.watershed_id);
        put_u16(&mut out, hydrology.ocean_basin_id);
        put_u32(&mut out, hydrology.lake_basin_id);
        put_f32(&mut out, hydrology.spill_elevation);
        put_f32(&mut out, hydrology.filled_elevation);
        put_f32(&mut out, hydrology.mean_runoff);
        for value in hydrology.seasonal_runoff_fraction {
            put_u16(&mut out, value);
        }
        put_f32(&mut out, hydrology.mean_discharge);
        for value in hydrology.seasonal_discharge_fraction {
            put_u16(&mut out, value);
        }
        put_f32(&mut out, hydrology.catchment_area);
        put_f32(&mut out, hydrology.channel_bed_elevation);
        put_f32(&mut out, hydrology.water_surface_elevation);
        put_u16(&mut out, hydrology.channel_width_centiblocks);
        put_u16(&mut out, hydrology.channel_depth_centiblocks);
        put_u16(&mut out, hydrology.sediment_energy);
        put_u64(&mut out, hydrology.baseline_water_units);
        put_i32(&mut out, hydrology.voxel_volume_residual);
        put_u32(&mut out, hydrology.river_id);
        put_i16(&mut out, hydrology.erosion_centiblocks);
        put_i16(&mut out, hydrology.deposition_centiblocks);
        put_u16(&mut out, hydrology.seasonal_level_range_centiblocks);
        put_u16(&mut out, hydrology.flags);
        put_u8(&mut out, hydrology.stream_order);
        put_u8(&mut out, hydrology.salinity);
        put_u8(&mut out, hydrology.water_body as u8);
        put_u16(&mut out, ground.soil_parent_material);
        put_u32(&mut out, ground.aquifer_capacity);
        put_u16(&mut out, ground.aquifer_permeability);
        put_u16(&mut out, ground.porosity);
        put_f32(&mut out, ground.baseline_groundwater_head);
        put_u8(&mut out, ground.soil_depth_decimeters);
        put_u8(&mut out, ground.sand);
        put_u8(&mut out, ground.silt);
        put_u8(&mut out, ground.organic);
        put_u8(&mut out, ground.baseline_fertility);
        put_u8(&mut out, ground.drainage);
        put_u8(&mut out, ground.soil_salinity);
        put_u8(&mut out, ground.freeze_flags);
        put_u8(&mut out, ground.erosion_susceptibility);
        put_u8(&mut out, biome.baseline_biome);
        put_u16(&mut out, biome.edaphic_flags);
        put_u32(&mut out, biome.habitat_flags);
        put_u8(&mut out, biome.vegetation_potential);
        put_u8(&mut out, biome.tree_line_y);
        put_u8(&mut out, biome.succession_potential);
        put_u16(&mut out, biome.country_id);
        put_u16(&mut out, biome.heart_assignment);
        put_u32(&mut out, resources.deposit_site_ref);
        put_u16(&mut out, resources.deposit_site_count);
    }
    if out.len() != count * GENESIS_RECORD_BYTES {
        return Err(AtlasError::Corrupt(
            "internal genesis record width mismatch".into(),
        ));
    }
    Ok(out)
}

fn encode_dynamic(dynamic: &DynamicLayers) -> Result<Vec<u8>, AtlasError> {
    let mut out = Vec::with_capacity(
        DYNAMIC_PREFIX_BYTES + dynamic.cells.len() * DYNAMIC_RECORD_BYTES + FILE_HEADER_BYTES,
    );
    put_u64(&mut out, dynamic.completed_climate_hours);
    for cell in dynamic.cells.values() {
        put_u32(&mut out, cell.atmospheric_vapor);
        put_u32(&mut out, cell.cloud_water);
        put_i32(&mut out, cell.local_weather_anomaly);
        put_i16(&mut out, cell.weather_temperature_anomaly);
        put_i16(&mut out, cell.pressure_anomaly);
        put_u16(&mut out, cell.storm_energy);
        put_u16(&mut out, cell.precipitation_rate);
        put_i16(&mut out, cell.wind_anomaly[0]);
        put_i16(&mut out, cell.wind_anomaly[1]);
        put_i32(&mut out, cell.fire_moisture_anomaly);
        put_i32(&mut out, cell.vegetation_moisture_anomaly);
    }
    if out.len() != DYNAMIC_PREFIX_BYTES + dynamic.cells.len() * DYNAMIC_RECORD_BYTES {
        return Err(AtlasError::Corrupt(
            "internal dynamic record width mismatch".into(),
        ));
    }
    Ok(out)
}

fn fingerprint_geometry(grid: &AtlasGrid<GeometryCell>) -> u64 {
    let mut out = Vec::with_capacity(grid.len() * 20);
    for cell in grid.values() {
        for value in cell.unit_direction {
            put_f32(&mut out, value);
        }
        put_f32(&mut out, cell.latitude_radians);
        put_f32(&mut out, cell.physical_area);
    }
    stable_hash(&out)
}

fn fingerprint_tectonics(grid: &AtlasGrid<TectonicCell>) -> u64 {
    let mut out = Vec::with_capacity(grid.len() * 39);
    for cell in grid.values() {
        put_u16(&mut out, cell.plate_id);
        put_u8(&mut out, cell.boundary as u8);
        put_u8(&mut out, cell.boundary_detail as u8);
        put_u16(&mut out, cell.neighbor_plate);
        put_f32(&mut out, cell.boundary_strength);
        put_u16(&mut out, cell.boundary_distance);
        for value in cell.boundary_strike {
            put_i16(&mut out, value);
        }
        put_u16(&mut out, cell.continental_crust);
        put_u16(&mut out, cell.craton_id);
        put_u16(&mut out, cell.crust_age);
        put_u16(&mut out, cell.oceanic_age);
        put_u16(&mut out, cell.crust_thickness);
        put_u16(&mut out, cell.bedrock_family);
        put_u16(&mut out, cell.geological_province);
        put_u16(&mut out, cell.stratigraphic_stack);
        put_u8(&mut out, cell.metamorphic_grade);
        put_u16(&mut out, cell.fault_intensity);
        put_u8(&mut out, cell.volcanic_history);
        put_u8(&mut out, cell.sediment_basin as u8);
    }
    stable_hash(&out)
}

fn fingerprint_terrain(grid: &AtlasGrid<TerrainCell>) -> u64 {
    let mut out = Vec::with_capacity(grid.len() * 22);
    for cell in grid.values() {
        put_f32(&mut out, cell.base_elevation);
        put_f32(&mut out, cell.eroded_elevation);
        put_f32(&mut out, cell.tectonic_contribution);
        put_f32(&mut out, cell.volcanic_contribution);
        put_f32(&mut out, cell.dynamic_topography);
        put_u16(&mut out, cell.landmass_id);
    }
    stable_hash(&out)
}

fn fingerprint_climate(grid: &AtlasGrid<ClimateCell>) -> u64 {
    let mut out = Vec::with_capacity(grid.len() * 120);
    for cell in grid.values() {
        put_f32(&mut out, cell.mean_temperature);
        put_f32(&mut out, cell.seasonality);
        put_f32(&mut out, cell.ocean_temperature_anomaly);
        put_f32(&mut out, cell.continentality);
        put_f32(&mut out, cell.prevailing_wind[0]);
        put_f32(&mut out, cell.prevailing_wind[1]);
        put_f32(&mut out, cell.ocean_current[0]);
        put_f32(&mut out, cell.ocean_current[1]);
        put_f32(&mut out, cell.mean_atmospheric_moisture);
        put_f32(&mut out, cell.mean_precipitation);
        put_f32(&mut out, cell.precipitation_seasonality);
        put_f32(&mut out, cell.potential_evapotranspiration);
        put_f32(&mut out, cell.aridity);
        put_f32(&mut out, cell.snow_persistence);
        for value in cell.seasonal_temperature {
            put_f32(&mut out, value);
        }
        for value in cell.seasonal_precipitation {
            put_f32(&mut out, value);
        }
        for wind in cell.seasonal_wind {
            put_f32(&mut out, wind[0]);
            put_f32(&mut out, wind[1]);
        }
    }
    stable_hash(&out)
}

fn fingerprint_hydrology(grid: &AtlasGrid<HydrologyCell>) -> u64 {
    let mut out = Vec::with_capacity(grid.len() * 91);
    for cell in grid.values() {
        put_u32(&mut out, cell.drainage_receiver);
        put_u32(&mut out, cell.watershed_id);
        put_u16(&mut out, cell.ocean_basin_id);
        put_u32(&mut out, cell.lake_basin_id);
        put_f32(&mut out, cell.spill_elevation);
        put_f32(&mut out, cell.filled_elevation);
        put_f32(&mut out, cell.mean_runoff);
        for value in cell.seasonal_runoff_fraction {
            put_u16(&mut out, value);
        }
        put_f32(&mut out, cell.mean_discharge);
        for value in cell.seasonal_discharge_fraction {
            put_u16(&mut out, value);
        }
        put_f32(&mut out, cell.catchment_area);
        put_f32(&mut out, cell.channel_bed_elevation);
        put_f32(&mut out, cell.water_surface_elevation);
        put_u16(&mut out, cell.channel_width_centiblocks);
        put_u16(&mut out, cell.channel_depth_centiblocks);
        put_u16(&mut out, cell.sediment_energy);
        put_u64(&mut out, cell.baseline_water_units);
        put_i32(&mut out, cell.voxel_volume_residual);
        put_u32(&mut out, cell.river_id);
        put_i16(&mut out, cell.erosion_centiblocks);
        put_i16(&mut out, cell.deposition_centiblocks);
        put_u16(&mut out, cell.seasonal_level_range_centiblocks);
        put_u16(&mut out, cell.flags);
        put_u8(&mut out, cell.stream_order);
        put_u8(&mut out, cell.salinity);
        put_u8(&mut out, cell.water_body as u8);
    }
    stable_hash(&out)
}

fn fingerprint_ground(grid: &AtlasGrid<GroundCell>) -> u64 {
    let mut out = Vec::with_capacity(grid.len() * 23);
    for cell in grid.values() {
        put_u16(&mut out, cell.soil_parent_material);
        put_u32(&mut out, cell.aquifer_capacity);
        put_u16(&mut out, cell.aquifer_permeability);
        put_u16(&mut out, cell.porosity);
        put_f32(&mut out, cell.baseline_groundwater_head);
        put_u8(&mut out, cell.soil_depth_decimeters);
        put_u8(&mut out, cell.sand);
        put_u8(&mut out, cell.silt);
        put_u8(&mut out, cell.organic);
        put_u8(&mut out, cell.baseline_fertility);
        put_u8(&mut out, cell.drainage);
        put_u8(&mut out, cell.soil_salinity);
        put_u8(&mut out, cell.freeze_flags);
        put_u8(&mut out, cell.erosion_susceptibility);
    }
    stable_hash(&out)
}

fn fingerprint_biomes(grid: &AtlasGrid<BiomeCell>) -> u64 {
    let mut out = Vec::with_capacity(grid.len() * 14);
    for cell in grid.values() {
        put_u8(&mut out, cell.baseline_biome);
        put_u16(&mut out, cell.edaphic_flags);
        put_u32(&mut out, cell.habitat_flags);
        put_u8(&mut out, cell.vegetation_potential);
        put_u8(&mut out, cell.tree_line_y);
        put_u8(&mut out, cell.succession_potential);
        put_u16(&mut out, cell.country_id);
        put_u16(&mut out, cell.heart_assignment);
    }
    stable_hash(&out)
}

fn fingerprint_resources(grid: &AtlasGrid<ResourceCell>) -> u64 {
    let mut out = Vec::with_capacity(grid.len() * 6);
    for cell in grid.values() {
        put_u32(&mut out, cell.deposit_site_ref);
        put_u16(&mut out, cell.deposit_site_count);
    }
    stable_hash(&out)
}

/// Cursor used to slice one dynamic pass without ever scanning the atlas in a
/// single ordinary server tick.
#[derive(Clone, Debug, Default)]
pub struct DynamicScan {
    cursor: usize,
}

impl DynamicScan {
    pub fn step(
        &mut self,
        atlas: &mut PlanetAtlas,
        budget: usize,
        mut update: impl FnMut(AtlasPos, &mut DynamicCell),
    ) -> bool {
        let side = atlas.side();
        let end = self
            .cursor
            .saturating_add(budget)
            .min(atlas.dynamic.cells.len());
        for index in self.cursor..end {
            let pos = AtlasPos::from_index(index, side).expect("dynamic index is valid");
            update(pos, &mut atlas.dynamic.cells.values[index]);
        }
        self.cursor = end;
        if self.cursor == atlas.dynamic.cells.len() {
            self.cursor = 0;
            true
        } else {
            false
        }
    }
}
