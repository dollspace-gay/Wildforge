//! Causal, whole-planet geological genesis.
//!
//! Every large-scale geological decision is made here on the sphere before a
//! voxel chunk exists.  Chunk generation only samples these layers and site
//! manifests; it never rolls another continent, plate, volcano, or deposit.

use std::cmp::Ordering as CmpOrdering;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use glam::{DVec3, Vec2};
use noise::{NoiseFn, Perlin};
use serde::{Deserialize, Serialize};

use super::*;
use crate::chunk::{CHUNK_X, CHUNK_Z, ChunkPos, SEA_LEVEL};
use crate::planet::local_frame;

const MAX_GEOLOGY_ATTEMPTS: u8 = 8;
const LLOYD_PASSES: usize = 2;

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
    pub(super) fn from_u8(value: u8) -> Result<Self, AtlasError> {
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
    pub(super) fn from_u8(value: u8) -> Result<Self, AtlasError> {
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlateRecord {
    pub id: u16,
    pub site_unit: [f32; 3],
    pub euler_pole: [f32; 3],
    pub angular_speed: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CratonRecord {
    pub id: u16,
    pub center_unit: [f32; 3],
    pub nuclei: [[f32; 3]; 3],
    pub radius_radians: f32,
    pub interior_age_myr: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VolcanoRecord {
    pub id: u32,
    pub pos: AtlasPos,
    pub source: VolcanoSource,
    pub plate_id: u16,
    pub chain_id: u16,
    pub age_myr: u16,
    pub edifice_radius_blocks: u16,
    pub edifice_height_blocks: u16,
    pub crater_radius_blocks: u16,
    pub crater_depth_blocks: u16,
    pub chamber_depth_blocks: u16,
    pub chamber_radius_blocks: u16,
    pub chamber_volume_blocks: u32,
    pub chemistry: MagmaChemistry,
    pub hydrothermal_radius_blocks: u16,
    pub erosion: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IntrusionRecord {
    pub id: u32,
    pub pos: AtlasPos,
    pub kind: IntrusionKind,
    pub plate_id: u16,
    pub radius_blocks: u16,
    pub top_depth_blocks: u16,
    pub bottom_depth_blocks: u16,
    pub volume_blocks: u64,
    pub contact_radius_blocks: u16,
    pub chemistry: MagmaChemistry,
    pub age_myr: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DepositRecord {
    pub id: u32,
    pub mineral: MineralKind,
    pub pos: AtlasPos,
    pub host: BedrockFamily,
    pub geological_province: u16,
    pub landmass_id: u16,
    pub source_body_id: u32,
    pub radius_blocks: u16,
    pub depth_min: u16,
    pub depth_max: u16,
    pub grade_ppm: u32,
    /// Hard upper bound on materialized ore blocks.
    pub tonnage_blocks: u64,
    pub max_blocks_per_chunk: u16,
    pub eligible_chunk_upper_bound: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StratigraphicStackRecord {
    pub id: u16,
    pub name: String,
    pub layers_bottom_to_top: Vec<BedrockFamily>,
    pub nominal_thicknesses: Vec<u16>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GeologicalProvinceRecord {
    pub id: u16,
    pub name: String,
    pub dominant_bedrock: BedrockFamily,
    pub plate_id: u16,
    pub craton_id: u16,
    pub basin: BasinKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContinentRecord {
    pub id: u16,
    pub cell_count: u32,
    pub area: f64,
    pub share_of_land: f64,
    pub major: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GeologyAttemptRecord {
    pub attempt: u8,
    pub failed_constraints: Vec<String>,
}

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

fn dvec(array: [f32; 3]) -> DVec3 {
    DVec3::new(
        f64::from(array[0]),
        f64::from(array[1]),
        f64::from(array[2]),
    )
}

fn arr(value: DVec3) -> [f32; 3] {
    [value.x as f32, value.y as f32, value.z as f32]
}

fn unit_from_hash(seed: u32, salt: u64) -> DVec3 {
    let a = mix64(u64::from(seed) ^ salt);
    let b = mix64(a ^ 0x9e37_79b9_7f4a_7c15);
    let z = (a as f64 / u64::MAX as f64) * 2.0 - 1.0;
    let angle = (b as f64 / u64::MAX as f64) * std::f64::consts::TAU;
    let radius = (1.0 - z * z).max(0.0).sqrt();
    DVec3::new(radius * angle.cos(), z, radius * angle.sin())
}

fn rotate(vector: DVec3, axis: DVec3, angle: f64) -> DVec3 {
    let axis = axis.normalize();
    (vector * angle.cos()
        + axis.cross(vector) * angle.sin()
        + axis * axis.dot(vector) * (1.0 - angle.cos()))
    .normalize()
}

fn fibonacci_sites(count: usize, seed: u32, salt: u64, jitter: f64) -> Vec<DVec3> {
    let rotation_axis = unit_from_hash(seed, salt ^ 0x726f_7461_7465);
    let rotation_angle = (mix64(u64::from(seed) ^ salt ^ 0x0061_6e67_6c65) as f64
        / u64::MAX as f64)
        * std::f64::consts::TAU;
    let phase = (mix64(u64::from(seed) ^ salt) as f64 / u64::MAX as f64) * std::f64::consts::TAU;
    let golden = std::f64::consts::PI * (3.0 - 5.0f64.sqrt());
    (0..count)
        .map(|index| {
            let y = 1.0 - 2.0 * (index as f64 + 0.5) / count as f64;
            let radius = (1.0 - y * y).sqrt();
            let theta = phase + golden * index as f64;
            let mut site = DVec3::new(radius * theta.cos(), y, radius * theta.sin());
            site = rotate(site, rotation_axis, rotation_angle);
            let random = unit_from_hash(seed, salt ^ index as u64 ^ 0x6a69_7474_6572);
            let tangent = (random - site * random.dot(site)).normalize_or_zero();
            (site * jitter.cos() + tangent * jitter.sin()).normalize()
        })
        .collect()
}

fn nearest_two(sites: &[DVec3], point: DVec3) -> (usize, usize, f64, f64) {
    let mut best = (usize::MAX, f64::NEG_INFINITY);
    let mut second = (usize::MAX, f64::NEG_INFINITY);
    for (index, site) in sites.iter().enumerate() {
        let dot = site.dot(point);
        if dot > best.1 {
            second = best;
            best = (index, dot);
        } else if dot > second.1 {
            second = (index, dot);
        }
    }
    (best.0, second.0, best.1, second.1)
}

fn plate_sites(seed: u32, geometry: &AtlasGrid<GeometryCell>) -> Vec<PlateRecord> {
    let count = 16 + (mix64(u64::from(seed) ^ 0x504c_4154_4553) % 7) as usize;
    let mut sites = fibonacci_sites(count, seed, 0x504c_4154_4553, 0.055);
    for _ in 0..LLOYD_PASSES {
        let mut sums = vec![DVec3::ZERO; count];
        for cell in geometry.values() {
            let point = dvec(cell.unit_direction);
            let nearest = nearest_two(&sites, point).0;
            sums[nearest] += point * f64::from(cell.physical_area);
        }
        for (site, sum) in sites.iter_mut().zip(sums) {
            if sum.length_squared() > 0.0 {
                *site = sum.normalize();
            }
        }
    }
    sites
        .into_iter()
        .enumerate()
        .map(|(index, site)| {
            let pole = unit_from_hash(seed, 0x4555_4c45_5200 ^ index as u64);
            let speed_hash = mix64(u64::from(seed) ^ 0x5350_4545_4400 ^ index as u64);
            PlateRecord {
                id: index as u16,
                site_unit: arr(site),
                euler_pole: arr(pole),
                angular_speed: 0.25 + (speed_hash as f32 / u64::MAX as f32) * 0.95,
            }
        })
        .collect()
}

fn plate_assignments(plates: &[PlateRecord], geometry: &AtlasGrid<GeometryCell>) -> Vec<u16> {
    let sites: Vec<_> = plates.iter().map(|plate| dvec(plate.site_unit)).collect();
    geometry
        .values()
        .iter()
        .map(|cell| nearest_two(&sites, dvec(cell.unit_direction)).0 as u16)
        .collect()
}

fn craton_field(
    seed: u32,
    attempt: u8,
    geometry: &AtlasGrid<GeometryCell>,
) -> (Vec<u16>, Vec<u16>, Vec<u16>, Vec<CratonRecord>) {
    let count = 6 + (mix64(u64::from(seed) ^ u64::from(attempt) ^ 0x4352_4154_4f4e) % 4) as usize;
    let centers = fibonacci_sites(
        count,
        seed ^ u32::from(attempt).wrapping_mul(0x9e37_79b9),
        0x4352_4154_4f4e,
        0.045,
    );
    let noise = Perlin::new(seed ^ u32::from(attempt).wrapping_mul(0x85eb_ca6b) ^ 0xacc3_710a);
    let mut records = Vec::with_capacity(count);
    let mut nuclei_by_group = Vec::with_capacity(count);
    for (index, center) in centers.iter().copied().enumerate() {
        let mut nuclei = [[0.0; 3]; 3];
        for (nucleus_index, slot) in nuclei.iter_mut().enumerate() {
            let random = unit_from_hash(
                seed,
                0x4e55_434c_4555 ^ u64::from(attempt) ^ (index as u64) << 8 ^ nucleus_index as u64,
            );
            let tangent = (random - center * random.dot(center)).normalize_or_zero();
            let angle = if nucleus_index == 0 {
                0.0
            } else {
                0.08 + nucleus_index as f64 * 0.035
            };
            *slot = arr((center * angle.cos() + tangent * angle.sin()).normalize());
        }
        nuclei_by_group.push(nuclei);
        let radius =
            0.46 + (mix64(u64::from(seed) ^ index as u64 ^ 0x7261_6469_7573) % 45) as f32 / 1000.0;
        records.push(CratonRecord {
            id: index as u16 + 1,
            center_unit: arr(center),
            nuclei,
            radius_radians: radius,
            interior_age_myr: 2_700
                + (mix64(u64::from(seed) ^ index as u64 ^ 0x0061_6765) % 900) as u16,
        });
    }

    let mut fraction = Vec::with_capacity(geometry.len());
    let mut ids = Vec::with_capacity(geometry.len());
    let mut ages = Vec::with_capacity(geometry.len());
    for cell in geometry.values() {
        let point = dvec(cell.unit_direction);
        let n = noise.get([
            point.x * 3.7 + 11.0,
            point.y * 3.7 - 7.0,
            point.z * 3.7 + 3.0,
        ]);
        let mut best = (0.0f64, 0usize);
        for (group, record) in records.iter().enumerate() {
            let distance = nuclei_by_group[group]
                .iter()
                .map(|nucleus| dvec(*nucleus).dot(point).clamp(-1.0, 1.0).acos())
                .fold(f64::INFINITY, f64::min);
            let radius = f64::from(record.radius_radians) + n * 0.055;
            let value = ((radius - distance) / 0.14 + 0.5).clamp(0.0, 1.0);
            let value = value * value * (3.0 - 2.0 * value);
            if value > best.0 {
                best = (value, group);
            }
        }
        fraction.push((best.0 * 65_535.0).round() as u16);
        if best.0 > 0.04 {
            ids.push(records[best.1].id);
            let margin = 1.0 - best.0;
            ages.push(
                (f32::from(records[best.1].interior_age_myr) * (1.0 - margin as f32 * 0.72))
                    .max(180.0) as u16,
            );
        } else {
            ids.push(0);
            ages.push(0);
        }
    }
    (fraction, ids, ages, records)
}

fn plate_velocity(plate: &PlateRecord, point: DVec3) -> DVec3 {
    dvec(plate.euler_pole).cross(point) * f64::from(plate.angular_speed)
}

fn classify_pair(
    a_plate: u16,
    b_plate: u16,
    a_continental: bool,
    b_continental: bool,
    point: DVec3,
    plates: &[PlateRecord],
) -> (DetailedBoundary, f32, DVec3) {
    let b_site = dvec(plates[usize::from(b_plate)].site_unit);
    let normal = (b_site - point * b_site.dot(point)).normalize_or_zero();
    let relative = plate_velocity(&plates[usize::from(a_plate)], point)
        - plate_velocity(&plates[usize::from(b_plate)], point);
    let convergence = relative.dot(normal) as f32;
    let tangent = point.cross(normal).normalize_or_zero();
    let shear = relative.dot(tangent).abs() as f32;
    let detail = if convergence > 0.10 {
        match (a_continental, b_continental) {
            (true, true) => DetailedBoundary::ContinentalCollision,
            (false, false) => DetailedBoundary::OceanOceanSubduction,
            _ => DetailedBoundary::OceanContinentSubduction,
        }
    } else if convergence < -0.10 {
        if a_continental && b_continental {
            DetailedBoundary::ContinentalRift
        } else {
            DetailedBoundary::OceanRidge
        }
    } else if shear > 0.09 {
        DetailedBoundary::Transform
    } else {
        DetailedBoundary::PassiveWeak
    };
    (detail, convergence.abs().max(shear), tangent)
}

#[cfg(test)]
pub(crate) fn classify_pair_rotation_probe(
    a_plate: u16,
    b_plate: u16,
    a_continental: bool,
    b_continental: bool,
    point: DVec3,
    plates: &[PlateRecord],
) -> (DetailedBoundary, f32) {
    let (detail, strength, _) = classify_pair(
        a_plate,
        b_plate,
        a_continental,
        b_continental,
        point,
        plates,
    );
    (detail, strength)
}

fn build_tectonics(
    side: u16,
    geometry: &AtlasGrid<GeometryCell>,
    plates: &[PlateRecord],
    plate_ids: &[u16],
    continental: &[u16],
    craton_ids: &[u16],
    crust_ages: &[u16],
) -> Vec<TectonicCell> {
    let count = geometry.len();
    let mut detail = vec![DetailedBoundary::Interior; count];
    let mut neighbor_plate = plate_ids.to_vec();
    let mut strength = vec![0.0f32; count];
    let mut strike = vec![DVec3::ZERO; count];

    for index in 0..count {
        let pos = AtlasPos::from_index(index, side).expect("geology index");
        let own = plate_ids[index];
        let point = dvec(geometry.values()[index].unit_direction);
        let mut best: Option<(f32, u16, DetailedBoundary, DVec3)> = None;
        for neighbor in pos.neighbors4(side) {
            let other_index = neighbor.index(side);
            let other = plate_ids[other_index];
            if other == own {
                continue;
            }
            let (class, motion, run_strike) = classify_pair(
                own,
                other,
                continental[index] >= 32_768,
                continental[other_index] >= 32_768,
                point,
                plates,
            );
            if best.is_none_or(|candidate| motion > candidate.0) {
                best = Some((motion, other, class, run_strike));
            }
        }
        if let Some((motion, other, class, run_strike)) = best {
            detail[index] = class;
            neighbor_plate[index] = other;
            strength[index] = motion;
            strike[index] = run_strike;
        }
    }

    // Two categorical majority passes remove one-cell class flicker without
    // blurring junctions into the plate interiors.
    for _ in 0..2 {
        let previous = detail.clone();
        for index in 0..count {
            if previous[index] == DetailedBoundary::Interior {
                continue;
            }
            let pos = AtlasPos::from_index(index, side).expect("geology index");
            let mut counts = BTreeMap::<DetailedBoundary, u8>::new();
            *counts.entry(previous[index]).or_default() += 2;
            for neighbor in pos.neighbors8(side) {
                let class = previous[neighbor.index(side)];
                if class != DetailedBoundary::Interior {
                    *counts.entry(class).or_default() += 1;
                }
            }
            if let Some((class, _)) = counts.into_iter().max_by_key(|(_, count)| *count) {
                detail[index] = class;
            }
        }
    }

    let mut nearest_source = vec![u32::MAX; count];
    let mut distance = vec![u16::MAX; count];
    let mut queue = VecDeque::new();
    for index in 0..count {
        if detail[index] != DetailedBoundary::Interior {
            nearest_source[index] = index as u32;
            distance[index] = 0;
            queue.push_back(index);
        }
    }
    while let Some(index) = queue.pop_front() {
        let pos = AtlasPos::from_index(index, side).expect("geology index");
        let next = distance[index].saturating_add(1);
        if next > 32 {
            continue;
        }
        for neighbor in pos.neighbors4(side) {
            let target = neighbor.index(side);
            if next < distance[target]
                || (next == distance[target] && nearest_source[index] < nearest_source[target])
            {
                distance[target] = next;
                nearest_source[target] = nearest_source[index];
                queue.push_back(target);
            }
        }
    }

    // Ocean crust grows older away from spreading ridges.
    let mut ridge_age = vec![u16::MAX; count];
    let mut ridge_queue = VecDeque::new();
    for index in 0..count {
        if detail[index] == DetailedBoundary::OceanRidge && continental[index] < 32_768 {
            ridge_age[index] = 0;
            ridge_queue.push_back(index);
        }
    }
    while let Some(index) = ridge_queue.pop_front() {
        let pos = AtlasPos::from_index(index, side).expect("geology index");
        let next = ridge_age[index].saturating_add(1);
        if next > 40 {
            continue;
        }
        for neighbor in pos.neighbors4(side) {
            let target = neighbor.index(side);
            if continental[target] < 32_768 && next < ridge_age[target] {
                ridge_age[target] = next;
                ridge_queue.push_back(target);
            }
        }
    }

    (0..count)
        .map(|index| {
            let source = usize::try_from(nearest_source[index])
                .ok()
                .filter(|source| *source < count)
                .unwrap_or(index);
            let run_strike = strike[source];
            let quantized = [
                (run_strike.x * 32_767.0).round() as i16,
                (run_strike.y * 32_767.0).round() as i16,
                (run_strike.z * 32_767.0).round() as i16,
            ];
            let oceanic_age = if continental[index] >= 32_768 {
                0
            } else if ridge_age[index] == u16::MAX {
                160 + (cell_hash(0, AtlasPos::from_index(index, side).unwrap(), 0x6f636561) % 61)
                    as u16
            } else {
                ridge_age[index].saturating_mul(6).min(220)
            };
            let cont = f32::from(continental[index]) / 65_535.0;
            TectonicCell {
                plate_id: plate_ids[index],
                boundary: detail[source].summary(),
                boundary_detail: detail[source],
                neighbor_plate: neighbor_plate[source],
                boundary_strength: strength[source],
                boundary_distance: distance[index],
                boundary_strike: quantized,
                continental_crust: continental[index],
                craton_id: craton_ids[index],
                crust_age: if cont >= 0.5 {
                    crust_ages[index]
                } else {
                    oceanic_age
                },
                oceanic_age,
                crust_thickness: (70.0 + cont * 330.0) as u16,
                bedrock_family: BedrockFamily::MixedBasement as u16,
                geological_province: 0,
                stratigraphic_stack: 0,
                metamorphic_grade: 0,
                fault_intensity: ((strength[source] * 38_000.0)
                    / (1.0 + f32::from(distance[index]) * 0.24))
                    .clamp(0.0, 65_535.0) as u16,
                volcanic_history: u8::from(detail[source].is_volcanic()),
                sediment_basin: BasinKind::None,
            }
        })
        .collect()
}

fn moved_cell(
    start: AtlasPos,
    side: u16,
    tectonics: &[TectonicCell],
    steps: usize,
    required_continental: Option<bool>,
) -> AtlasPos {
    let own = tectonics[start.index(side)].plate_id;
    let mut at = start;
    for _ in 0..steps {
        let current_distance = tectonics[at.index(side)].boundary_distance;
        let next = at
            .neighbors4(side)
            .into_iter()
            .filter(|candidate| {
                let cell = tectonics[candidate.index(side)];
                cell.plate_id == own
                    && required_continental
                        .is_none_or(|continental| (cell.continental_crust >= 32_768) == continental)
            })
            .max_by_key(|candidate| tectonics[candidate.index(side)].boundary_distance);
        if let Some(next) = next
            && tectonics[next.index(side)].boundary_distance >= current_distance
        {
            at = next;
        }
    }
    at
}

fn volcano_record(
    seed: u32,
    id: u32,
    site: AtlasPos,
    source: VolcanoSource,
    plate_id: u16,
) -> VolcanoRecord {
    let hash = cell_hash(seed, site, 0x0065_6469_6669_6365);
    let chemistry = match source {
        VolcanoSource::ContinentalArc => MagmaChemistry::Andesitic,
        VolcanoSource::IslandArc
        | VolcanoSource::Rift
        | VolcanoSource::OceanRidge
        | VolcanoSource::Hotspot => MagmaChemistry::Basaltic,
    };
    let (radius, height) = match source {
        VolcanoSource::ContinentalArc => (90 + (hash % 56) as u16, 38 + ((hash >> 8) % 31) as u16),
        VolcanoSource::IslandArc => (82 + (hash % 54) as u16, 72 + ((hash >> 8) % 48) as u16),
        VolcanoSource::Rift => (62 + (hash % 45) as u16, 20 + ((hash >> 8) % 25) as u16),
        VolcanoSource::OceanRidge => (72 + (hash % 54) as u16, 12 + ((hash >> 8) % 18) as u16),
        VolcanoSource::Hotspot => unreachable!("hotspot chains have age-specific records"),
    };
    let chamber_radius = 14 + ((hash >> 24) % 22) as u16;
    VolcanoRecord {
        id,
        pos: site,
        source,
        plate_id,
        chain_id: 0,
        age_myr: (hash % 12) as u16,
        edifice_radius_blocks: radius,
        edifice_height_blocks: height,
        crater_radius_blocks: (radius / 7).max(6),
        crater_depth_blocks: (height / 4).max(4),
        chamber_depth_blocks: 18 + ((hash >> 16) % 30) as u16,
        chamber_radius_blocks: chamber_radius,
        chamber_volume_blocks: chamber_volume(chamber_radius),
        chemistry,
        hydrothermal_radius_blocks: radius.saturating_mul(2),
        erosion: (hash % 12_000) as u16,
    }
}

fn chamber_volume(radius: u16) -> u32 {
    // The chamber materializer uses a vertically compressed sphere with a
    // 0.72 height ratio.  Persist the corresponding finite voxel envelope,
    // rather than deriving magma volume from the unrelated surface edifice.
    ((4.0 / 3.0) * std::f64::consts::PI * f64::from(radius).powi(3) * 0.72).ceil() as u32
}

fn geology_sites(
    seed: u32,
    side: u16,
    geometry: &AtlasGrid<GeometryCell>,
    tectonics: &[TectonicCell],
    cratons: &[CratonRecord],
) -> (Vec<VolcanoRecord>, Vec<IntrusionRecord>) {
    let mut volcanoes = Vec::new();
    let mut accepted = Vec::<AtlasPos>::new();
    for index in 0..geometry.len() {
        let cell = tectonics[index];
        let pos = AtlasPos::from_index(index, side).expect("geology index");
        if cell.boundary_distance != 0 || !cell.boundary_detail.is_volcanic() {
            continue;
        }
        let eligible = match cell.boundary_detail {
            DetailedBoundary::OceanContinentSubduction => cell.continental_crust >= 32_768,
            DetailedBoundary::OceanOceanSubduction => {
                cell.continental_crust < 32_768 && cell.plate_id < cell.neighbor_plate
            }
            DetailedBoundary::ContinentalRift => cell.continental_crust >= 32_768,
            DetailedBoundary::OceanRidge => cell.continental_crust < 32_768,
            _ => false,
        };
        if !eligible {
            continue;
        }
        let stride = match cell.boundary_detail {
            DetailedBoundary::OceanContinentSubduction | DetailedBoundary::OceanOceanSubduction => {
                23
            }
            DetailedBoundary::ContinentalRift => 41,
            _ => 67,
        };
        if !cell_hash(seed, pos, 0x0076_6f6c_6361_6e6f).is_multiple_of(stride) {
            continue;
        }
        let source = match cell.boundary_detail {
            DetailedBoundary::OceanContinentSubduction => VolcanoSource::ContinentalArc,
            DetailedBoundary::OceanOceanSubduction => VolcanoSource::IslandArc,
            DetailedBoundary::ContinentalRift => VolcanoSource::Rift,
            DetailedBoundary::OceanRidge => VolcanoSource::OceanRidge,
            _ => unreachable!("only volcanic boundary classes pass eligibility"),
        };
        let required_continental = match source {
            VolcanoSource::ContinentalArc | VolcanoSource::Rift => Some(true),
            VolcanoSource::IslandArc | VolcanoSource::OceanRidge => Some(false),
            VolcanoSource::Hotspot => None,
        };
        let site = moved_cell(pos, side, tectonics, 2, required_continental);
        if accepted
            .iter()
            .any(|other| geodesic_distance(site.center(side), other.center(side)) < 190.0)
        {
            continue;
        }
        accepted.push(site);
        volcanoes.push(volcano_record(
            seed,
            volcanoes.len() as u32 + 1,
            site,
            source,
            cell.plate_id,
        ));
    }

    for (class, source) in [
        (
            DetailedBoundary::OceanContinentSubduction,
            VolcanoSource::ContinentalArc,
        ),
        (
            DetailedBoundary::OceanOceanSubduction,
            VolcanoSource::IslandArc,
        ),
        (DetailedBoundary::ContinentalRift, VolcanoSource::Rift),
        (DetailedBoundary::OceanRidge, VolcanoSource::OceanRidge),
    ] {
        if volcanoes.iter().any(|volcano| volcano.source == source) {
            continue;
        }
        let candidate = tectonics
            .iter()
            .enumerate()
            .filter(|(_, cell)| {
                cell.boundary_distance == 0
                    && cell.boundary_detail == class
                    && (class != DetailedBoundary::OceanContinentSubduction
                        || cell.continental_crust >= 32_768)
                    && (class != DetailedBoundary::OceanOceanSubduction
                        || (cell.continental_crust < 32_768 && cell.plate_id < cell.neighbor_plate))
                    && (class != DetailedBoundary::ContinentalRift
                        || cell.continental_crust >= 32_768)
                    && (class != DetailedBoundary::OceanRidge || cell.continental_crust < 32_768)
            })
            .min_by_key(|(index, _)| {
                let pos = AtlasPos::from_index(*index, side).unwrap();
                cell_hash(seed, pos, 0x6172_635f_6775_6172)
            });
        if let Some((index, cell)) = candidate {
            let boundary = AtlasPos::from_index(index, side).unwrap();
            let required_continental = match source {
                VolcanoSource::ContinentalArc | VolcanoSource::Rift => Some(true),
                VolcanoSource::IslandArc | VolcanoSource::OceanRidge => Some(false),
                VolcanoSource::Hotspot => None,
            };
            let site = moved_cell(boundary, side, tectonics, 2, required_continental);
            volcanoes.push(volcano_record(
                seed,
                volcanoes.len() as u32 + 1,
                site,
                source,
                cell.plate_id,
            ));
        }
    }

    // Hotspots are independent of boundaries.  A present-day source and four
    // progressively older edifices are advected along the carrying plate.
    let hotspot_count = 4 + (mix64(u64::from(seed) ^ 0x0068_6f74_7370_6f74) % 3) as usize;
    let interior_cells = (320 / (FACE_BLOCKS / side)).max(2);
    let mut candidates: Vec<_> = (0..geometry.len())
        .filter(|index| tectonics[*index].boundary_distance > interior_cells)
        .map(|index| {
            let pos = AtlasPos::from_index(index, side).unwrap();
            (cell_hash(seed, pos, 0x0068_6f74_7370_6f74), pos)
        })
        .collect();
    candidates.sort_unstable_by_key(|candidate| candidate.0);
    let mut hotspot_roots = Vec::new();
    for (_, root) in candidates {
        if hotspot_roots.iter().all(|other| {
            geodesic_distance(root.center(side), AtlasPos::center(*other, side)) > 1_300.0
        }) {
            hotspot_roots.push(root);
            if hotspot_roots.len() == hotspot_count {
                break;
            }
        }
    }
    for (chain, root) in hotspot_roots.into_iter().enumerate() {
        let plate = tectonics[root.index(side)].plate_id;
        let strike = {
            let point = dvec(geometry.get(root).unwrap().unit_direction);
            let pole = unit_from_hash(seed, 0x6873_706f_6c65 ^ u64::from(plate));
            pole.cross(point).normalize_or_zero()
        };
        let root_unit = dvec(geometry.get(root).unwrap().unit_direction);
        for age_step in 0..5 {
            let angle = age_step as f64 * 170.0 / PLANET_RADIUS;
            let target_unit = (root_unit * angle.cos() + strike * angle.sin()).normalize();
            let site = geometry
                .iter()
                .filter(|(pos, _)| tectonics[pos.index(side)].plate_id == plate)
                .max_by(|(_, a), (_, b)| {
                    dvec(a.unit_direction)
                        .dot(target_unit)
                        .partial_cmp(&dvec(b.unit_direction).dot(target_unit))
                        .unwrap_or(CmpOrdering::Equal)
                })
                .map(|(pos, _)| pos)
                .expect("atlas is nonempty");
            let hash = cell_hash(seed, site, 0x686f_7463_6861_696e ^ chain as u64);
            let erosion = age_step as u16 * 11_000;
            volcanoes.push(VolcanoRecord {
                id: volcanoes.len() as u32 + 1,
                pos: site,
                source: VolcanoSource::Hotspot,
                plate_id: plate,
                chain_id: chain as u16 + 1,
                age_myr: age_step as u16 * 9,
                edifice_radius_blocks: 72 + (hash % 38) as u16,
                edifice_height_blocks: 36u16.saturating_sub(age_step as u16 * 5),
                crater_radius_blocks: 8,
                crater_depth_blocks: 5,
                chamber_depth_blocks: 28,
                chamber_radius_blocks: 22,
                chamber_volume_blocks: 80_000,
                chemistry: MagmaChemistry::Basaltic,
                hydrothermal_radius_blocks: 150,
                erosion,
            });
        }
    }

    let mut intrusions = Vec::new();
    for volcano in volcanoes.iter().step_by(3) {
        let hash = cell_hash(seed, volcano.pos, 0x696e_7472_7573_696f);
        let kind = if hash.is_multiple_of(31) {
            IntrusionKind::Carbonatite
        } else if matches!(
            volcano.source,
            VolcanoSource::Rift | VolcanoSource::OceanRidge
        ) {
            IntrusionKind::DikeSwarm
        } else {
            IntrusionKind::Pluton
        };
        intrusions.push(IntrusionRecord {
            id: intrusions.len() as u32 + 1,
            pos: volcano.pos,
            kind,
            plate_id: volcano.plate_id,
            radius_blocks: 70 + (hash % 90) as u16,
            top_depth_blocks: 8 + ((hash >> 9) % 28) as u16,
            bottom_depth_blocks: 110 + ((hash >> 17) % 80) as u16,
            volume_blocks: 180_000 + hash % 900_000,
            contact_radius_blocks: 180 + ((hash >> 24) % 120) as u16,
            chemistry: if kind == IntrusionKind::Carbonatite {
                MagmaChemistry::Carbonatitic
            } else {
                volcano.chemistry
            },
            age_myr: volcano.age_myr.saturating_add(4),
        });
    }
    for craton in cratons {
        let center = dvec(craton.center_unit);
        let pos = geometry
            .iter()
            .max_by(|(_, a), (_, b)| {
                dvec(a.unit_direction)
                    .dot(center)
                    .partial_cmp(&dvec(b.unit_direction).dot(center))
                    .unwrap_or(CmpOrdering::Equal)
            })
            .map(|(pos, _)| pos)
            .unwrap();
        let hash = cell_hash(seed, pos, 0x6261_7468_6f6c_6974);
        intrusions.push(IntrusionRecord {
            id: intrusions.len() as u32 + 1,
            pos,
            kind: IntrusionKind::Batholith,
            plate_id: tectonics[pos.index(side)].plate_id,
            radius_blocks: 180 + (hash % 150) as u16,
            top_depth_blocks: 18,
            bottom_depth_blocks: 190,
            volume_blocks: 2_000_000 + hash % 4_000_000,
            contact_radius_blocks: 360,
            chemistry: MagmaChemistry::Rhyolitic,
            age_myr: craton.interior_age_myr.saturating_sub(600),
        });
    }
    (volcanoes, intrusions)
}

fn boundary_relief(cell: TectonicCell, cell_blocks: u16) -> f32 {
    let distance = f32::from(cell.boundary_distance) * f32::from(cell_blocks);
    let gaussian = |center: f32, width: f32| (-((distance - center) / width).powi(2)).exp();
    let strength = cell.boundary_strength.clamp(0.18, 1.4);
    let continental = f32::from(cell.continental_crust) / 65_535.0;
    match cell.boundary_detail {
        DetailedBoundary::ContinentalCollision => 46.0 * strength * gaussian(70.0, 190.0),
        DetailedBoundary::OceanContinentSubduction if continental >= 0.5 => {
            34.0 * strength * gaussian(105.0, 110.0) - 2.0 * strength * gaussian(0.0, 65.0)
        }
        DetailedBoundary::OceanContinentSubduction => -24.0 * strength * gaussian(0.0, 58.0),
        DetailedBoundary::OceanOceanSubduction => {
            if cell.plate_id < cell.neighbor_plate {
                25.0 * strength * gaussian(90.0, 95.0)
            } else {
                -23.0 * strength * gaussian(0.0, 55.0)
            }
        }
        DetailedBoundary::ContinentalRift => {
            -17.0 * strength * gaussian(0.0, 75.0) + 8.0 * gaussian(125.0, 65.0)
        }
        DetailedBoundary::OceanRidge => 17.0 * strength * gaussian(0.0, 110.0),
        DetailedBoundary::Transform => -6.0 * strength * gaussian(0.0, 52.0),
        DetailedBoundary::PassiveWeak | DetailedBoundary::Interior => 0.0,
    }
}

fn stamp_volcanic_relief(
    side: u16,
    geometry: &AtlasGrid<GeometryCell>,
    volcanoes: &[VolcanoRecord],
) -> Vec<f32> {
    let mut relief = vec![0.0f32; geometry.len()];
    for volcano in volcanoes {
        let max_steps = (volcano.edifice_radius_blocks / (FACE_BLOCKS / side)).saturating_add(3);
        let mut queue = VecDeque::from([(volcano.pos, 0u16)]);
        let mut seen = BTreeSet::new();
        while let Some((pos, steps)) = queue.pop_front() {
            if !seen.insert(pos) || steps > max_steps {
                continue;
            }
            let distance = geodesic_distance(pos.center(side), volcano.pos.center(side)) as f32;
            let radius = f32::from(volcano.edifice_radius_blocks);
            if distance <= radius {
                let t = 1.0 - distance / radius.max(1.0);
                let erosion = 1.0 - f32::from(volcano.erosion) / 65_535.0;
                let mut height = f32::from(volcano.edifice_height_blocks) * t.powf(1.45) * erosion;
                let crater = f32::from(volcano.crater_radius_blocks);
                if distance < crater {
                    height -= f32::from(volcano.crater_depth_blocks) * (1.0 - distance / crater);
                }
                relief[pos.index(side)] += height;
            }
            for neighbor in pos.neighbors4(side) {
                queue.push_back((neighbor, steps.saturating_add(1)));
            }
        }
    }
    relief
}

fn smooth_scalar(side: u16, values: &[f32], passes: usize) -> Vec<f32> {
    let mut current = values.to_vec();
    for _ in 0..passes {
        let previous = current.clone();
        for (index, target) in current.iter_mut().enumerate() {
            let pos = AtlasPos::from_index(index, side).unwrap();
            let sum: f32 = pos
                .neighbors4(side)
                .iter()
                .map(|neighbor| previous[neighbor.index(side)])
                .sum();
            *target = previous[index] * 0.58 + sum * 0.105;
        }
    }
    current
}

fn weighted_sea_level(raw: &[f32], geometry: &AtlasGrid<GeometryCell>, target_ocean: f64) -> f32 {
    let mut order: Vec<usize> = (0..raw.len()).collect();
    order.sort_unstable_by(|a, b| raw[*a].partial_cmp(&raw[*b]).unwrap_or(CmpOrdering::Equal));
    let total: f64 = geometry
        .values()
        .iter()
        .map(|cell| f64::from(cell.physical_area))
        .sum();
    let wanted = total * target_ocean;
    let mut accumulated = 0.0;
    for index in order {
        accumulated += f64::from(geometry.values()[index].physical_area);
        if accumulated >= wanted {
            return raw[index];
        }
    }
    raw.last().copied().unwrap_or(0.0)
}

fn label_components(
    side: u16,
    geometry: &AtlasGrid<GeometryCell>,
    terrain: &[TerrainCell],
    land: bool,
) -> (Vec<u16>, Vec<(u16, u32, f64)>) {
    let mut labels = vec![0u16; terrain.len()];
    let mut records = Vec::new();
    let mut next = 0u16;
    for index in 0..terrain.len() {
        let included = (terrain[index].eroded_elevation > SEA_LEVEL as f32) == land;
        if !included || labels[index] != 0 {
            continue;
        }
        next = next.saturating_add(1);
        let mut queue = VecDeque::from([index]);
        let mut cells = 0u32;
        let mut area = 0.0f64;
        while let Some(at) = queue.pop_front() {
            if labels[at] != 0 || ((terrain[at].eroded_elevation > SEA_LEVEL as f32) != land) {
                continue;
            }
            labels[at] = next;
            cells += 1;
            area += f64::from(geometry.values()[at].physical_area);
            let pos = AtlasPos::from_index(at, side).unwrap();
            for neighbor in pos.neighbors4(side) {
                let neighbor = neighbor.index(side);
                if labels[neighbor] == 0 {
                    queue.push_back(neighbor);
                }
            }
        }
        records.push((next, cells, area));
    }
    (labels, records)
}

fn basin_for(cell: TectonicCell, elevation: f32) -> BasinKind {
    if elevation <= SEA_LEVEL as f32 - 18.0 {
        BasinKind::DeepMarine
    } else if elevation <= SEA_LEVEL as f32 + 5.0 {
        BasinKind::MarineShelf
    } else {
        match cell.boundary_detail {
            DetailedBoundary::ContinentalCollision if cell.boundary_distance <= 7 => {
                BasinKind::Foreland
            }
            DetailedBoundary::ContinentalRift if cell.boundary_distance <= 5 => BasinKind::Rift,
            DetailedBoundary::PassiveWeak if cell.boundary_distance <= 5 => {
                BasinKind::PassiveMargin
            }
            _ if elevation < SEA_LEVEL as f32 + 13.0 && cell.continental_crust > 40_000 => {
                BasinKind::Closed
            }
            _ => BasinKind::None,
        }
    }
}

fn bedrock_for(cell: TectonicCell, basin: BasinKind, latitude: f32) -> BedrockFamily {
    if cell.continental_crust < 18_000 {
        if cell.oceanic_age < 35 || cell.volcanic_history != 0 {
            BedrockFamily::Basalt
        } else {
            BedrockFamily::Ultramafic
        }
    } else {
        match basin {
            BasinKind::DeepMarine => BedrockFamily::Shale,
            BasinKind::MarineShelf | BasinKind::PassiveMargin => {
                if cell.crust_age.is_multiple_of(2) {
                    BedrockFamily::Limestone
                } else {
                    BedrockFamily::Sandstone
                }
            }
            BasinKind::Foreland => BedrockFamily::Shale,
            BasinKind::Rift => BedrockFamily::Basalt,
            BasinKind::Closed
                if latitude.abs().to_degrees() > 15.0 && latitude.abs().to_degrees() < 42.0 =>
            {
                BedrockFamily::Evaporite
            }
            _ if cell.craton_id != 0 => BedrockFamily::Granite,
            _ => BedrockFamily::MixedBasement,
        }
    }
}

fn stack_for(bedrock: BedrockFamily, basin: BasinKind) -> u16 {
    match basin {
        BasinKind::MarineShelf | BasinKind::PassiveMargin => 2,
        BasinKind::DeepMarine => 3,
        BasinKind::Foreland => 4,
        BasinKind::Rift => 5,
        BasinKind::Closed => 6,
        BasinKind::None => match bedrock {
            BedrockFamily::Basalt | BedrockFamily::Ultramafic => 7,
            _ => 1,
        },
    }
}

fn default_stacks() -> Vec<StratigraphicStackRecord> {
    use BedrockFamily as B;
    [
        (
            1,
            "cratonic basement",
            vec![B::Granite, B::MixedBasement, B::Sandstone],
        ),
        (
            2,
            "passive shelf",
            vec![B::MixedBasement, B::Sandstone, B::Limestone, B::Shale],
        ),
        (
            3,
            "deep marine basin",
            vec![B::Basalt, B::Shale, B::Limestone],
        ),
        (
            4,
            "foreland basin",
            vec![B::MixedBasement, B::Sandstone, B::Shale, B::Sandstone],
        ),
        (
            5,
            "continental rift",
            vec![B::MixedBasement, B::Basalt, B::Shale, B::Sandstone],
        ),
        (
            6,
            "closed evaporite basin",
            vec![B::MixedBasement, B::Sandstone, B::Evaporite],
        ),
        (7, "oceanic crust", vec![B::Ultramafic, B::Basalt, B::Shale]),
    ]
    .into_iter()
    .map(|(id, name, layers)| StratigraphicStackRecord {
        id,
        name: name.into(),
        nominal_thicknesses: vec![28; layers.len()],
        layers_bottom_to_top: layers,
    })
    .collect()
}

fn province_name(seed: u32, id: u16) -> String {
    const A: [&str; 12] = [
        "Ar", "Bel", "Cor", "Dun", "Eld", "Fal", "Gor", "Hal", "Ith", "Kar", "Mor", "Nor",
    ];
    const B: [&str; 12] = [
        "adan", "bray", "cairn", "dell", "esh", "fold", "gard", "holm", "mere", "reach", "scar",
        "vale",
    ];
    let hash = mix64(u64::from(seed) ^ u64::from(id) ^ 0x7072_6f76_696e_6365);
    format!(
        "{}{} Province",
        A[hash as usize % A.len()],
        B[(hash >> 16) as usize % B.len()]
    )
}

fn assign_provinces(
    seed: u32,
    geometry: &AtlasGrid<GeometryCell>,
    tectonics: &mut [TectonicCell],
    terrain: &[TerrainCell],
) -> Vec<GeologicalProvinceRecord> {
    let mut ids = BTreeMap::<(u16, u16, BasinKind, BedrockFamily), u16>::new();
    let mut records = Vec::new();
    for index in 0..tectonics.len() {
        let basin = basin_for(tectonics[index], terrain[index].eroded_elevation);
        let bedrock = bedrock_for(
            tectonics[index],
            basin,
            geometry.values()[index].latitude_radians,
        );
        let key = (
            tectonics[index].plate_id,
            tectonics[index].craton_id,
            basin,
            bedrock,
        );
        let id = *ids.entry(key).or_insert_with(|| {
            let id = records.len() as u16 + 1;
            records.push(GeologicalProvinceRecord {
                id,
                name: province_name(seed, id),
                dominant_bedrock: bedrock,
                plate_id: key.0,
                craton_id: key.1,
                basin,
            });
            id
        });
        tectonics[index].sediment_basin = basin;
        tectonics[index].bedrock_family = bedrock as u16;
        tectonics[index].geological_province = id;
        tectonics[index].stratigraphic_stack = stack_for(bedrock, basin);
    }
    records
}

fn metamorphism(
    side: u16,
    geometry: &AtlasGrid<GeometryCell>,
    tectonics: &mut [TectonicCell],
    intrusions: &[IntrusionRecord],
) {
    for (index, cell) in tectonics.iter_mut().enumerate() {
        let pos = AtlasPos::from_index(index, side).unwrap();
        let contact = intrusions.iter().fold(0.0f32, |best, intrusion| {
            let distance = geodesic_distance(pos.center(side), intrusion.pos.center(side)) as f32;
            let value =
                (1.0 - distance / f32::from(intrusion.contact_radius_blocks)).clamp(0.0, 1.0);
            best.max(value)
        });
        let regional = if cell.boundary_detail == DetailedBoundary::ContinentalCollision {
            (1.0 - f32::from(cell.boundary_distance) / 11.0).clamp(0.0, 1.0)
        } else {
            0.0
        };
        cell.metamorphic_grade = ((contact.max(regional) * 5.0).round() as u8).min(5);
        if contact > 0.58 {
            cell.bedrock_family = match BedrockFamily::from_id(cell.bedrock_family) {
                BedrockFamily::Limestone => BedrockFamily::Marble as u16,
                BedrockFamily::Shale => BedrockFamily::Slate as u16,
                BedrockFamily::Sandstone => BedrockFamily::Quartzite as u16,
                other => other as u16,
            };
        }
        let _ = geometry;
    }
}

fn host_allows(kind: MineralKind, cell: TectonicCell, latitude: f32) -> bool {
    let rock = BedrockFamily::from_id(cell.bedrock_family);
    match kind {
        MineralKind::Copper => cell.volcanic_history != 0 || matches!(rock, BedrockFamily::Basalt),
        MineralKind::Tin => matches!(rock, BedrockFamily::Granite | BedrockFamily::Quartzite),
        MineralKind::Iron => matches!(
            rock,
            BedrockFamily::MixedBasement | BedrockFamily::Shale | BedrockFamily::Basalt
        ),
        MineralKind::Cobalt | MineralKind::Manganese => matches!(
            rock,
            BedrockFamily::Basalt | BedrockFamily::Ultramafic | BedrockFamily::MixedBasement
        ),
        MineralKind::Cinnabar => cell.volcanic_history != 0 || cell.fault_intensity > 8_000,
        MineralKind::Coal => matches!(
            cell.sediment_basin,
            BasinKind::MarineShelf
                | BasinKind::Foreland
                | BasinKind::Closed
                | BasinKind::PassiveMargin
        ),
        MineralKind::Gold => cell.fault_intensity > 9_000 || cell.volcanic_history != 0,
        MineralKind::Galena => matches!(rock, BedrockFamily::Limestone | BedrockFamily::Marble),
        MineralKind::Chromite => matches!(rock, BedrockFamily::Basalt | BedrockFamily::Ultramafic),
        MineralKind::Diamond => cell.craton_id != 0 && cell.crust_age > 1_800,
        MineralKind::RareEarth => {
            matches!(rock, BedrockFamily::Granite) && cell.metamorphic_grade > 0
        }
        MineralKind::Halite => {
            cell.sediment_basin == BasinKind::Closed
                && latitude.abs().to_degrees() > 12.0
                && latitude.abs().to_degrees() < 48.0
        }
        MineralKind::Pitchblende => matches!(rock, BedrockFamily::Granite) && cell.craton_id != 0,
        MineralKind::Geode => matches!(rock, BedrockFamily::Limestone | BedrockFamily::Marble),
        MineralKind::Other => cell.continental_crust >= 24_000,
    }
}

struct DepositPlacement<'a> {
    seed: u32,
    side: u16,
    geometry: &'a AtlasGrid<GeometryCell>,
    tectonics: &'a [TectonicCell],
    terrain: &'a [TerrainCell],
}

impl DepositPlacement<'_> {
    fn choose_site(
        &self,
        salt: u64,
        kind: MineralKind,
        landmass: Option<u16>,
        occupied: &[AtlasPos],
    ) -> Option<AtlasPos> {
        let mut best: Option<(u64, AtlasPos)> = None;
        for index in 0..self.tectonics.len() {
            if self.terrain[index].landmass_id == 0
                || landmass.is_some_and(|wanted| self.terrain[index].landmass_id != wanted)
                || !host_allows(
                    kind,
                    self.tectonics[index],
                    self.geometry.values()[index].latitude_radians,
                )
            {
                continue;
            }
            let pos = AtlasPos::from_index(index, self.side).unwrap();
            if occupied.iter().any(|other| {
                geodesic_distance(pos.center(self.side), other.center(self.side)) < 210.0
            }) {
                continue;
            }
            let score = cell_hash(self.seed, pos, salt ^ kind as u64);
            if best.is_none_or(|candidate| score < candidate.0) {
                best = Some((score, pos));
            }
        }
        best.map(|(_, pos)| pos)
    }
}

fn deposits(
    seed: u32,
    side: u16,
    geometry: &AtlasGrid<GeometryCell>,
    tectonics: &[TectonicCell],
    terrain: &[TerrainCell],
    continents: &[ContinentRecord],
    intrusions: &[IntrusionRecord],
) -> (Vec<DepositRecord>, Vec<ResourceCell>) {
    let mut out = Vec::<DepositRecord>::new();
    let mut occupied = BTreeMap::<MineralKind, Vec<AtlasPos>>::new();
    let major: Vec<u16> = continents
        .iter()
        .filter(|c| c.major)
        .map(|c| c.id)
        .collect();
    let placement = DepositPlacement {
        seed,
        side,
        geometry,
        tectonics,
        terrain,
    };

    let mut add = |kind: MineralKind, landmass: Option<u16>, ordinal: u64| {
        let pos = placement.choose_site(
            0x6465_706f_7369_7400 ^ ordinal,
            kind,
            landmass,
            occupied.get(&kind).map(Vec::as_slice).unwrap_or_default(),
        );
        let Some(pos) = pos else { return };
        occupied.entry(kind).or_default().push(pos);
        let cell = tectonics[pos.index(side)];
        let hash = cell_hash(seed, pos, 0x0067_7261_6465 ^ kind as u64);
        let (radius, quota) = match kind {
            MineralKind::Diamond | MineralKind::RareEarth | MineralKind::Pitchblende => {
                (64u16, 3u16)
            }
            MineralKind::Tin | MineralKind::Gold | MineralKind::Galena | MineralKind::Halite => {
                (112, 6)
            }
            _ => (176, 12),
        };
        let chunk_radius = u32::from(radius).div_ceil(CHUNK_X as u32).saturating_add(2);
        let upper = chunk_radius
            .saturating_mul(2)
            .saturating_add(1)
            .saturating_pow(2);
        let source_body_id = intrusions
            .iter()
            .min_by(|a, b| {
                geodesic_distance(pos.center(side), a.pos.center(side))
                    .partial_cmp(&geodesic_distance(pos.center(side), b.pos.center(side)))
                    .unwrap_or(CmpOrdering::Equal)
            })
            .filter(|intrusion| {
                geodesic_distance(pos.center(side), intrusion.pos.center(side)) < 420.0
            })
            .map_or(0, |intrusion| intrusion.id);
        out.push(DepositRecord {
            id: out.len() as u32 + 1,
            mineral: kind,
            pos,
            host: BedrockFamily::from_id(cell.bedrock_family),
            geological_province: cell.geological_province,
            landmass_id: terrain[pos.index(side)].landmass_id,
            source_body_id,
            radius_blocks: radius,
            depth_min: match kind {
                MineralKind::Coal | MineralKind::Halite => 28,
                _ => 4,
            },
            depth_max: match kind {
                MineralKind::Diamond => 90,
                _ => 120,
            },
            grade_ppm: 800 + (hash % 22_000) as u32,
            tonnage_blocks: u64::from(quota) * u64::from(upper),
            max_blocks_per_chunk: quota,
            eligible_chunk_upper_bound: upper,
        });
    };

    // Copper/iron/coal progression is present on every major continent.
    for (ordinal, landmass) in major.iter().copied().enumerate() {
        for (offset, kind) in [MineralKind::Copper, MineralKind::Iron, MineralKind::Coal]
            .into_iter()
            .enumerate()
        {
            add(kind, Some(landmass), (ordinal * 8 + offset) as u64);
        }
    }
    // Regional and treasure classes have separated redundant sites.
    for kind in MineralKind::ALL_TRACKED {
        for ordinal in 0..2 {
            add(kind, None, 0x1000 + kind as u64 * 16 + ordinal as u64);
        }
        if kind == MineralKind::Tin {
            add(kind, None, 0x1000 + kind as u64 * 16 + 2);
        }
    }
    // Unknown data-pack ores receive finite generic host provinces too.
    for ordinal in 0..major.len().max(2) {
        add(
            MineralKind::Other,
            major.get(ordinal).copied(),
            0x9000 + ordinal as u64,
        );
    }

    let mut resources = vec![ResourceCell::default(); tectonics.len()];
    for site in &out {
        let cell = &mut resources[site.pos.index(side)];
        if cell.deposit_site_ref == 0 {
            cell.deposit_site_ref = site.id;
        }
        cell.deposit_site_count = cell.deposit_site_count.saturating_add(1);
    }
    (out, resources)
}

fn build_attempt(
    seed: u32,
    attempt: u8,
    side: u16,
    geometry: &AtlasGrid<GeometryCell>,
    plates: &[PlateRecord],
    plate_ids: &[u16],
    target_ocean: f64,
) -> AttemptOutput {
    let (continental, craton_ids, crust_ages, cratons) = craton_field(seed, attempt, geometry);
    let mut tectonics = build_tectonics(
        side,
        geometry,
        plates,
        plate_ids,
        &continental,
        &craton_ids,
        &crust_ages,
    );
    let (volcanoes, intrusions) = geology_sites(seed, side, geometry, &tectonics, &cratons);
    let volcanic = stamp_volcanic_relief(side, geometry, &volcanoes);
    let mantle = Perlin::new(seed ^ u32::from(attempt).wrapping_mul(0x9e37_79b9) ^ 0x6d61_6e74);
    let regional = Perlin::new(seed ^ u32::from(attempt).wrapping_mul(0x85eb_ca6b) ^ 0x7265_6769);
    let mut raw = Vec::with_capacity(geometry.len());
    let mut tectonic_contribution = Vec::with_capacity(geometry.len());
    let mut dynamic_topography = Vec::with_capacity(geometry.len());
    for index in 0..geometry.len() {
        let point = dvec(geometry.values()[index].unit_direction);
        let cont = f32::from(continental[index]) / 65_535.0;
        let ocean_base = -37.0 - f32::from(tectonics[index].oceanic_age) / 220.0 * 23.0;
        let continent_base = 13.0 + cont * 26.0;
        let crust = ocean_base + (continent_base - ocean_base) * cont.powf(0.72);
        let dynamic = mantle.get([point.x * 1.45, point.y * 1.45, point.z * 1.45]) as f32 * 9.0;
        let detail = regional.get([
            point.x * 8.0 + 7.0,
            point.y * 8.0 - 3.0,
            point.z * 8.0 + 11.0,
        ]) as f32
            * 3.2;
        let tectonic = boundary_relief(tectonics[index], FACE_BLOCKS / side);
        raw.push(crust + dynamic + detail + tectonic);
        tectonic_contribution.push(tectonic);
        dynamic_topography.push(dynamic);
    }
    raw = smooth_scalar(side, &raw, 2);
    for (elevation, volcanic_relief) in raw.iter_mut().zip(&volcanic) {
        *elevation += volcanic_relief;
    }
    let erosion: Vec<_> = geometry
        .values()
        .iter()
        .map(|cell| {
            let point = dvec(cell.unit_direction);
            (regional.get([point.x * 5.0, point.y * 5.0, point.z * 5.0]) as f32 * 0.5 + 0.5) * 3.5
        })
        .collect();
    // Water classification is based on eroded_elevation.  Solving the
    // quantile on raw - erosion / relief_scale makes the requested fraction
    // apply to that final surface instead of the pre-erosion intermediate.
    let sea_basis: Vec<_> = raw
        .iter()
        .zip(&erosion)
        .map(|(elevation, erosion)| elevation - erosion / 1.05)
        .collect();
    let raw_sea_level = weighted_sea_level(&sea_basis, geometry, target_ocean);
    let mut terrain: Vec<_> = raw
        .iter()
        .enumerate()
        .map(|(index, elevation)| {
            let base = (SEA_LEVEL as f32 + (*elevation - raw_sea_level) * 1.05).clamp(5.0, 232.0);
            TerrainCell {
                base_elevation: base,
                eroded_elevation: (base - erosion[index]).clamp(4.0, 232.0),
                tectonic_contribution: tectonic_contribution[index],
                volcanic_contribution: volcanic[index],
                dynamic_topography: dynamic_topography[index],
                landmass_id: 0,
            }
        })
        .collect();

    let (land_labels, land_components) = label_components(side, geometry, &terrain, true);
    for (index, label) in land_labels.iter().copied().enumerate() {
        terrain[index].landmass_id = label;
    }
    let total_area: f64 = geometry
        .values()
        .iter()
        .map(|cell| f64::from(cell.physical_area))
        .sum();
    let land_area: f64 = land_components.iter().map(|record| record.2).sum();
    let major_minimum = total_area * 0.012;
    let continents: Vec<_> = land_components
        .iter()
        .map(|(id, cells, area)| ContinentRecord {
            id: *id,
            cell_count: *cells,
            area: *area,
            share_of_land: if land_area > 0.0 {
                *area / land_area
            } else {
                0.0
            },
            major: *area >= major_minimum,
        })
        .collect();
    let major_count = continents.iter().filter(|record| record.major).count();
    let max_land_share = continents
        .iter()
        .map(|record| record.share_of_land)
        .fold(0.0f64, f64::max);
    let (_, ocean_components) = label_components(side, geometry, &terrain, false);
    let ocean_area: f64 = ocean_components.iter().map(|record| record.2).sum();
    let largest_ocean_share = ocean_components
        .iter()
        .map(|record| {
            if ocean_area > 0.0 {
                record.2 / ocean_area
            } else {
                0.0
            }
        })
        .fold(0.0f64, f64::max);
    let achieved_ocean_fraction = ocean_area / total_area;
    let mut failures = Vec::new();
    if !(4..=7).contains(&major_count) {
        failures.push(format!("major continents {major_count}, required 4..=7"));
    }
    if !(0.62..=0.70).contains(&achieved_ocean_fraction) {
        failures.push(format!(
            "ocean fraction {achieved_ocean_fraction:.5}, required 0.62..=0.70"
        ));
    }
    if max_land_share > 0.65 {
        failures.push(format!(
            "largest continent owns {:.1}% of land",
            max_land_share * 100.0
        ));
    }
    if largest_ocean_share < 0.90 {
        failures.push(format!(
            "largest connected ocean owns only {:.1}% of ocean",
            largest_ocean_share * 100.0
        ));
    }
    if side >= 128 {
        for (source, label) in [
            (VolcanoSource::ContinentalArc, "continental arc"),
            (VolcanoSource::IslandArc, "island arc"),
        ] {
            let sites: Vec<_> = volcanoes
                .iter()
                .filter(|volcano| volcano.source == source)
                .collect();
            if !sites.is_empty()
                && !sites.iter().any(|volcano| {
                    terrain[volcano.pos.index(side)].eroded_elevation > SEA_LEVEL as f32 + 2.0
                })
            {
                failures.push(format!("{label} has no emergent volcanic edifice"));
            }
        }
    }

    let provinces = assign_provinces(seed, geometry, &mut tectonics, &terrain);
    metamorphism(side, geometry, &mut tectonics, &intrusions);
    let (deposits, resources) = deposits(
        seed,
        side,
        geometry,
        &tectonics,
        &terrain,
        &continents,
        &intrusions,
    );
    for kind in MineralKind::ALL_TRACKED {
        let count = deposits.iter().filter(|site| site.mineral == kind).count();
        if count < 2 {
            failures.push(format!(
                "{} has {count} deposit sites, required at least 2",
                kind.label()
            ));
        }
    }
    let bronze_regions = deposits
        .iter()
        .filter(|site| site.mineral == MineralKind::Tin)
        .count();
    if bronze_regions < 3 {
        failures.push(format!(
            "bronze bootstrap has {bronze_regions} independent tin regions, required 3"
        ));
    }
    for continent in continents.iter().filter(|continent| continent.major) {
        for kind in [MineralKind::Copper, MineralKind::Iron, MineralKind::Coal] {
            if !deposits
                .iter()
                .any(|site| site.landmass_id == continent.id && site.mineral == kind)
            {
                failures.push(format!(
                    "major continent {} lacks guaranteed {}",
                    continent.id,
                    kind.label()
                ));
            }
        }
        if !terrain
            .iter()
            .zip(tectonics.iter())
            .any(|(terrain, tectonic)| {
                terrain.landmass_id == continent.id
                    && matches!(
                        BedrockFamily::from_id(tectonic.bedrock_family),
                        BedrockFamily::Limestone | BedrockFamily::Marble
                    )
            })
        {
            failures.push(format!(
                "major continent {} lacks basic carbonate flux",
                continent.id
            ));
        }
    }
    if side < 32 {
        // Tiny persistence and seam fixtures execute this same algorithm but
        // cannot resolve production-scale statistical acceptance bands.
        failures.clear();
    }

    AttemptOutput {
        tectonics,
        terrain,
        resources,
        cratons,
        continents,
        provinces,
        volcanoes,
        intrusions,
        deposits,
        raw_sea_level,
        achieved_ocean_fraction,
        largest_ocean_share,
        failures,
    }
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

impl GeologyModel {
    pub(super) fn validate(
        &self,
        side: u16,
        tectonics: &[TectonicCell],
        terrain: &[TerrainCell],
        resources: &[ResourceCell],
    ) -> Result<(), AtlasError> {
        let expected = atlas_count(side)?;
        if self.schema_version != GEOLOGY_SCHEMA_VERSION {
            return Err(AtlasError::UnsupportedVersion(format!(
                "geology schema {} (supported {})",
                self.schema_version, GEOLOGY_SCHEMA_VERSION
            )));
        }
        if tectonics.len() != expected || terrain.len() != expected || resources.len() != expected {
            return Err(AtlasError::Corrupt(
                "geology cell layers have inconsistent lengths".into(),
            ));
        }
        if !(16..=22).contains(&self.plates.len()) || !(6..=9).contains(&self.cratons.len()) {
            return Err(AtlasError::Corrupt(
                "plate or craton count is outside its fixed band".into(),
            ));
        }
        if side >= 32 {
            if !(0.62..=0.70).contains(&self.achieved_ocean_fraction)
                || self.largest_ocean_share < 0.90
            {
                return Err(AtlasError::Corrupt(
                    "geological land/ocean constraints are not satisfied".into(),
                ));
            }
            let major = self.continents.iter().filter(|record| record.major).count();
            if !(4..=7).contains(&major)
                || self
                    .continents
                    .iter()
                    .any(|record| record.share_of_land > 0.65)
            {
                return Err(AtlasError::Corrupt(
                    "continent constraints are not satisfied".into(),
                ));
            }
            for source in [VolcanoSource::ContinentalArc, VolcanoSource::IslandArc] {
                let sites: Vec<_> = self
                    .volcanoes
                    .iter()
                    .filter(|volcano| volcano.source == source)
                    .collect();
                if side >= 128
                    && !sites.is_empty()
                    && !sites.iter().any(|volcano| {
                        terrain[volcano.pos.index(side)].eroded_elevation > SEA_LEVEL as f32 + 2.0
                    })
                {
                    return Err(AtlasError::Corrupt(format!(
                        "{source:?} has no emergent volcanic edifice"
                    )));
                }
            }
        }
        if tectonics
            .iter()
            .any(|cell| usize::from(cell.plate_id) >= self.plates.len())
        {
            return Err(AtlasError::Corrupt(
                "cell references an unknown tectonic plate".into(),
            ));
        }
        let expected_source = |detail, cell: &TectonicCell| match detail {
            DetailedBoundary::OceanContinentSubduction => cell.continental_crust >= 32_768,
            DetailedBoundary::OceanOceanSubduction => {
                cell.continental_crust < 32_768 && cell.plate_id < cell.neighbor_plate
            }
            DetailedBoundary::ContinentalRift => cell.continental_crust >= 32_768,
            DetailedBoundary::OceanRidge => cell.continental_crust < 32_768,
            _ => false,
        };
        for (detail, source) in [
            (
                DetailedBoundary::OceanContinentSubduction,
                VolcanoSource::ContinentalArc,
            ),
            (
                DetailedBoundary::OceanOceanSubduction,
                VolcanoSource::IslandArc,
            ),
            (DetailedBoundary::ContinentalRift, VolcanoSource::Rift),
            (DetailedBoundary::OceanRidge, VolcanoSource::OceanRidge),
        ] {
            if tectonics.iter().any(|cell| {
                cell.boundary_distance == 0
                    && cell.boundary_detail == detail
                    && expected_source(detail, cell)
            }) && !self
                .volcanoes
                .iter()
                .any(|volcano| volcano.source == source)
            {
                return Err(AtlasError::Corrupt(format!(
                    "geology has a {detail:?} run but no {source:?} volcanic site"
                )));
            }
        }
        for (index, volcano) in self.volcanoes.iter().enumerate() {
            if volcano.id != index as u32 + 1
                || volcano.pos.u >= side
                || volcano.pos.v >= side
                || usize::from(volcano.plate_id) >= self.plates.len()
                || tectonics[volcano.pos.index(side)].plate_id != volcano.plate_id
                || volcano.chamber_volume_blocks < chamber_volume(volcano.chamber_radius_blocks)
            {
                return Err(AtlasError::Corrupt(
                    "volcano manifest contains an invalid site or finite chamber budget".into(),
                ));
            }
            let site = tectonics[volcano.pos.index(side)];
            let required_continental = match volcano.source {
                VolcanoSource::ContinentalArc | VolcanoSource::Rift => Some(true),
                VolcanoSource::IslandArc | VolcanoSource::OceanRidge => Some(false),
                VolcanoSource::Hotspot => None,
            };
            if required_continental
                .is_some_and(|required| (site.continental_crust >= 32_768) != required)
            {
                return Err(AtlasError::Corrupt(format!(
                    "{:?} volcano {} at {:?} is on the wrong crustal side ({})",
                    volcano.source, volcano.id, volcano.pos, site.continental_crust
                )));
            }
        }
        for (index, site) in self.deposits.iter().enumerate() {
            if site.id != index as u32 + 1
                || site.pos.u >= side
                || site.pos.v >= side
                || site.tonnage_blocks
                    < u64::from(site.max_blocks_per_chunk)
                        * u64::from(site.eligible_chunk_upper_bound)
            {
                return Err(AtlasError::Corrupt(
                    "deposit manifest contains an invalid budget or address".into(),
                ));
            }
        }
        for cell in resources {
            if cell.deposit_site_ref != 0 && cell.deposit_site_ref as usize > self.deposits.len() {
                return Err(AtlasError::Corrupt(
                    "resource cell references an unknown deposit".into(),
                ));
            }
        }
        Ok(())
    }
}

impl PlanetAtlas {
    pub fn boundary_between(&self, a: AtlasPos, b: AtlasPos) -> Option<BoundaryEdge> {
        if !a.neighbors4(self.side()).contains(&b) {
            return None;
        }
        let a_cell = *self.genesis.tectonics.get(a)?;
        let b_cell = *self.genesis.tectonics.get(b)?;
        if a_cell.plate_id == b_cell.plate_id {
            return None;
        }
        let a_point = dvec(self.genesis.geometry.get(a)?.unit_direction);
        let b_point = dvec(self.genesis.geometry.get(b)?.unit_direction);
        let point = (a_point + b_point).normalize();
        let (first, second, first_continental, second_continental) =
            if a_cell.plate_id < b_cell.plate_id {
                (
                    a_cell.plate_id,
                    b_cell.plate_id,
                    a_cell.continental_crust >= 32_768,
                    b_cell.continental_crust >= 32_768,
                )
            } else {
                (
                    b_cell.plate_id,
                    a_cell.plate_id,
                    b_cell.continental_crust >= 32_768,
                    a_cell.continental_crust >= 32_768,
                )
            };
        let (detail, strength, _) = classify_pair(
            first,
            second,
            first_continental,
            second_continental,
            point,
            &self.geology.plates,
        );
        Some(BoundaryEdge {
            a,
            b,
            detail,
            strength,
        })
    }

    pub fn deposit_host_is_valid(&self, site: &DepositRecord) -> bool {
        let Some(cell) = self.genesis.tectonics.get(site.pos) else {
            return false;
        };
        let Some(geometry) = self.genesis.geometry.get(site.pos) else {
            return false;
        };
        if site.host != BedrockFamily::from_id(cell.bedrock_family) {
            return false;
        }
        let routed_placer = site.radius_blocks == 72
            && site.depth_min == SEA_LEVEL as u16
            && site.max_blocks_per_chunk == 2
            && site.eligible_chunk_upper_bound == 32
            && matches!(site.mineral, MineralKind::Gold | MineralKind::RareEarth);
        if routed_placer {
            let source_exists = self.geology.deposits.iter().any(|source| {
                source.id == site.source_body_id
                    && source.id != site.id
                    && source.mineral == site.mineral
            });
            let depositional = self
                .genesis
                .hydrology
                .get(site.pos)
                .is_some_and(|hydrology| hydrology.flags & (HYDRO_FLOODPLAIN | HYDRO_DELTA) != 0);
            return source_exists && depositional;
        }
        host_allows(site.mineral, *cell, geometry.latitude_radians)
    }

    pub fn geology_sample(&self, point: SurfacePoint) -> AtlasGeologySample {
        let surface = SurfacePos::new(
            point.face,
            point.u.floor().clamp(0.0, f64::from(FACE_BLOCKS - 1)) as u16,
            point.v.floor().clamp(0.0, f64::from(FACE_BLOCKS - 1)) as u16,
        )
        .expect("clamped geology sample is canonical");
        let pos = self.atlas_pos(surface);
        let cell = *self
            .genesis
            .tectonics
            .get(pos)
            .expect("validated geology query");
        let terrain = *self
            .genesis
            .terrain
            .get(pos)
            .expect("validated geology query");
        let global = DVec3::new(
            f64::from(cell.boundary_strike[0]) / 32_767.0,
            f64::from(cell.boundary_strike[1]) / 32_767.0,
            f64::from(cell.boundary_strike[2]) / 32_767.0,
        )
        .normalize_or_zero();
        let frame = local_frame(point);
        let strike = Vec2::new(
            global.dot(frame.east) as f32,
            global.dot(frame.north) as f32,
        )
        .normalize_or_zero();
        AtlasGeologySample {
            plate_id: cell.plate_id,
            neighbor_plate: cell.neighbor_plate,
            boundary: cell.boundary_detail,
            boundary_distance_blocks: f32::from(cell.boundary_distance)
                * f32::from(self.cell_blocks()),
            boundary_strength: cell.boundary_strength,
            boundary_strike: strike,
            continental_fraction: f32::from(cell.continental_crust) / 65_535.0,
            craton_id: cell.craton_id,
            oceanic_age_myr: cell.oceanic_age,
            bedrock: BedrockFamily::from_id(cell.bedrock_family),
            geological_province: cell.geological_province,
            stratigraphic_stack: cell.stratigraphic_stack,
            metamorphic_grade: cell.metamorphic_grade,
            fault_intensity: cell.fault_intensity,
            volcanic_history: cell.volcanic_history,
            basin: cell.sediment_basin,
            landmass_id: terrain.landmass_id,
        }
    }

    pub fn province_name(&self, id: u16) -> Option<&str> {
        self.geology
            .provinces
            .iter()
            .find(|province| province.id == id)
            .map(|province| province.name.as_str())
    }

    pub fn exact_volcanic_relief(&self, point: SurfacePoint) -> f32 {
        self.geology.volcanoes.iter().fold(0.0, |sum, volcano| {
            let distance = geodesic_distance(point, volcano.pos.center(self.side())) as f32;
            let radius = f32::from(volcano.edifice_radius_blocks);
            if distance >= radius {
                return sum;
            }
            let t = 1.0 - distance / radius;
            let erosion = 1.0 - f32::from(volcano.erosion) / 65_535.0;
            let mut relief = f32::from(volcano.edifice_height_blocks) * t.powf(1.45) * erosion;
            let crater = f32::from(volcano.crater_radius_blocks);
            if distance < crater {
                relief -= f32::from(volcano.crater_depth_blocks) * (1.0 - distance / crater);
            }
            sum + relief
        })
    }

    pub fn sampled_volcanic_relief(&self, point: SurfacePoint) -> f32 {
        self.sample_scalar(point, |pos| {
            self.genesis.terrain.get(pos).unwrap().volcanic_contribution
        })
    }

    pub fn intrusion_margin(&self, point: SurfacePoint, y: f32) -> f32 {
        self.geology
            .intrusions
            .iter()
            .fold(-1.0f32, |best, intrusion| {
                let horizontal = geodesic_distance(point, intrusion.pos.center(self.side())) as f32;
                let depth = (self.terrain_sample(point).eroded_elevation - y).max(0.0);
                if depth < f32::from(intrusion.top_depth_blocks)
                    || depth > f32::from(intrusion.bottom_depth_blocks)
                {
                    return best;
                }
                best.max(1.0 - horizontal / f32::from(intrusion.radius_blocks))
            })
    }

    pub fn magma_at(&self, point: SurfacePoint, y: f32) -> bool {
        self.magma_interval(point)
            .is_some_and(|(minimum, maximum)| y >= minimum && y <= maximum)
    }

    pub fn magma_interval(&self, point: SurfacePoint) -> Option<(f32, f32)> {
        let surface = self.terrain_sample(point).eroded_elevation;
        self.geology
            .volcanoes
            .iter()
            .filter_map(|volcano| {
                let horizontal = geodesic_distance(point, volcano.pos.center(self.side())) as f32;
                let radius = f32::from(volcano.chamber_radius_blocks);
                if horizontal >= radius {
                    return None;
                }
                let center_y = surface - f32::from(volcano.chamber_depth_blocks);
                let vertical_radius =
                    radius * 0.72 * (1.0 - (horizontal / radius).powi(2)).max(0.0).sqrt();
                Some((center_y - vertical_radius, center_y + vertical_radius))
            })
            .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
    }

    pub fn nearest_volcano(&self, point: SurfacePoint, reach: f64) -> Option<&VolcanoRecord> {
        self.geology
            .volcanoes
            .iter()
            .filter_map(|site| {
                let distance = geodesic_distance(point, site.pos.center(self.side()));
                (distance <= reach).then_some((distance, site))
            })
            .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(CmpOrdering::Equal))
            .map(|(_, site)| site)
    }

    pub fn nearest_intrusion(&self, point: SurfacePoint, reach: f64) -> Option<&IntrusionRecord> {
        self.geology
            .intrusions
            .iter()
            .filter_map(|site| {
                let distance = geodesic_distance(point, site.pos.center(self.side()));
                (distance <= reach).then_some((distance, site))
            })
            .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(CmpOrdering::Equal))
            .map(|(_, site)| site)
    }

    pub fn nearest_deposit(
        &self,
        point: SurfacePoint,
        mineral: MineralKind,
        reach: f64,
    ) -> Option<&DepositRecord> {
        self.geology
            .deposits
            .iter()
            .filter(|site| site.mineral == mineral)
            .filter_map(|site| {
                let distance = geodesic_distance(point, site.pos.center(self.side()));
                (distance <= reach).then_some((distance, site))
            })
            .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(CmpOrdering::Equal))
            .map(|(_, site)| site)
    }

    pub fn deposit_allowance(&self, chunk: ChunkPos, mineral: MineralKind) -> u32 {
        let center = SurfacePoint {
            face: chunk.face(),
            u: f64::from(chunk.u()) * CHUNK_X as f64 + CHUNK_X as f64 * 0.5,
            v: f64::from(chunk.v()) * CHUNK_Z as f64 + CHUNK_Z as f64 * 0.5,
        };
        self.geology
            .deposits
            .iter()
            .filter(|site| site.mineral == mineral)
            .filter(|site| {
                geodesic_distance(center, site.pos.center(self.side()))
                    <= f64::from(site.radius_blocks)
            })
            .map(|site| u32::from(site.max_blocks_per_chunk))
            .sum()
    }

    pub fn deposit_center_in_chunk(
        &self,
        chunk: ChunkPos,
        mineral: MineralKind,
    ) -> Option<&DepositRecord> {
        self.geology.deposits.iter().find(|site| {
            site.mineral == mineral
                && ChunkPos::from_surface(
                    SurfacePos::new(
                        site.pos.face,
                        site.pos.u * self.cell_blocks() + self.cell_blocks() / 2,
                        site.pos.v * self.cell_blocks() + self.cell_blocks() / 2,
                    )
                    .expect("atlas deposit center is canonical"),
                ) == chunk
        })
    }
}
