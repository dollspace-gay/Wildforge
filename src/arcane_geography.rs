//! Causal, finite magical geography over the qualified planetary atlas.
//!
//! The dense state is a subledger of the single [`ArcaneLedger`](crate::arcane::ArcaneLedger):
//! its exact six-resonance total must equal the ledger's `Geography` custody
//! account.  Transfers within this module therefore move existing units only.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::arcane::{
    BASE_RESONANCES, CURRENT_PER_ATLAS_CELL, Current, ECHO, EMBER, GALE, ROOT, STONE, TIDE,
};
#[cfg(test)]
use crate::planet::Face;
use crate::planet::{
    Direction4, FACE_BLOCKS, PLANET_RADIUS, SurfacePoint, SurfacePos, surface_to_unit,
};
use crate::planet_atlas::{
    AtlasError, AtlasPos, BIOME_ARCTIC, BIOME_MOUNTAINS, BIOME_OCEAN, BIOME_TUNDRA,
    EDAPHIC_LIMESTONE, EDAPHIC_VOLCANIC, HABITAT_AQUATIC_BRACKISH, HABITAT_AQUATIC_FRESH,
    HABITAT_AQUATIC_SALT, HABITAT_CAVE_OUTLET, HABITAT_RIPARIAN, HABITAT_SPRING, HABITAT_WETLAND,
    PlanetAtlas,
};
use crate::registry::{ArcaneSiteRule, Registry};

pub const ARCANE_GEOGRAPHY_SCHEMA_VERSION: u32 = 1;
pub const ARCANE_GEOGRAPHY_ALGORITHM_VERSION: u32 = 2;
pub const ARCANE_GEOGRAPHY_DYNAMIC_VERSION: u32 = 1;
pub const ARCANE_GEOGRAPHY_JOURNAL_VERSION: u32 = 1;
pub const ARCANE_GEOGRAPHY_SIMULATION_SECONDS: u64 = 60;
/// Goal 1 retains one eighth of genesis in hearts and the unsited deep vault;
/// the rest receives exact planetary geography here.
pub const GEOGRAPHY_GENESIS_NUMERATOR: u64 = 7;
pub const GEOGRAPHY_GENESIS_DENOMINATOR: u64 = 8;

const IMMUTABLE_FILE: &str = "arcane-geography.wag";
const IMMUTABLE_BACKUP_FILE: &str = "arcane-geography.wag.bak";
const DYNAMIC_FILE: &str = "arcane-geography.wad";
const DYNAMIC_BACKUP_FILE: &str = "arcane-geography.wad.bak";
const DYNAMIC_PENDING_FILE: &str = "arcane-geography.wad.pending";
const CATALOG_FILE: &str = "arcane-sites.toml";
const CATALOG_BACKUP_FILE: &str = "arcane-sites.toml.bak";
const MANIFEST_FILE: &str = "arcane-geography.toml";
const MANIFEST_BACKUP_FILE: &str = "arcane-geography.toml.bak";
const IMMUTABLE_MAGIC: &[u8; 4] = b"WAG1";
const DYNAMIC_MAGIC: &[u8; 4] = b"WAD1";
const MAX_IMMUTABLE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_DYNAMIC_BYTES: u64 = 160 * 1024 * 1024;
const MAX_CATALOG_BYTES: u64 = 32 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArcaneGeographyStage {
    ListeningToStone,
    FindingCurrent,
    SettlingDeep,
    MarkingConfluences,
}

impl ArcaneGeographyStage {
    pub const ALL: [Self; 4] = [
        Self::ListeningToStone,
        Self::FindingCurrent,
        Self::SettlingDeep,
        Self::MarkingConfluences,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::ListeningToStone => "listening_to_stone",
            Self::FindingCurrent => "finding_current",
            Self::SettlingDeep => "settling_deep",
            Self::MarkingConfluences => "marking_confluences",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::ListeningToStone => "LISTENING TO THE STONE",
            Self::FindingCurrent => "FINDING THE CURRENT",
            Self::SettlingDeep => "SETTLING THE DEEP",
            Self::MarkingConfluences => "MARKING THE CONFLUENCES",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ArcaneGeographyProgress {
    pub stage: ArcaneGeographyStage,
    pub completed_stages: usize,
    pub total_stages: usize,
}

/// Dense immutable fields.  Integer fields make genesis byte-identical on
/// every supported host and keep the production planet within its RAM budget.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArcaneControlCell {
    pub capacity: u16,
    pub deep_capacity: u16,
    pub exchange_permille: u16,
    /// East, north, west, and south effective conductance in the local
    /// tangent chart. Intrinsic geological conductivity is multiplied by the
    /// spherical shared-edge/center-spacing factor at genesis.
    pub conductivity: [u16; 4],
    pub dross_mobility: u8,
    pub dross_retention: u8,
    /// Normalized baseline mixture; weights sum to 255.
    pub baseline_resonance: [u8; 6],
    pub stability: u16,
    pub recovery_potential: u16,
    /// Primary catalog reference plus one; zero means no primary site.
    pub site_ref: u32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArcaneDynamicCell {
    /// Exact units by base resonance.  Genesis and ordinary transport cap a
    /// cell below `u16::MAX` per band; transactions reject overflow.
    pub deep: [u16; 6],
    pub ambient: [u16; 6],
    pub dross: [u16; 6],
    pub transport_remainder: i32,
    pub wake_id: u32,
    pub historical_min_ambient: u16,
    pub historical_max_ambient: u16,
}

impl ArcaneDynamicCell {
    pub fn deep_total(self) -> u64 {
        self.deep.into_iter().map(u64::from).sum()
    }

    pub fn ambient_total(self) -> u64 {
        self.ambient.into_iter().map(u64::from).sum()
    }

    pub fn dross_total(self) -> u64 {
        self.dross.into_iter().map(u64::from).sum()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArcanePlaceType {
    Confluence,
    Well,
    Still,
    Echo,
    Heartshadow,
    Wake,
    Scar,
    Modded,
}

impl ArcanePlaceType {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Confluence => "confluence",
            Self::Well => "well",
            Self::Still => "still",
            Self::Echo => "echo",
            Self::Heartshadow => "heartshadow",
            Self::Wake => "wake",
            Self::Scar => "scar",
            Self::Modded => "modded",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArcanePlace {
    pub id: u64,
    pub kind: ArcanePlaceType,
    pub center: AtlasPos,
    pub radius_cells: u16,
    pub extent_cells: u32,
    pub confidence: u16,
    pub signature: [u8; 6],
    pub name_seed: u64,
    pub generated_name: String,
    pub network_id: u32,
    pub country_id: u16,
    pub provider: String,
    pub predicate_id: String,
    pub active: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArcaneSiteCatalog {
    pub schema_version: u32,
    pub sites: Vec<ArcanePlace>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArcaneWake {
    pub id: u64,
    pub site_id: u64,
    pub path: Vec<AtlasPos>,
    pub cursor: u32,
    pub charge: [u16; 6],
    pub dross: [u16; 6],
    pub decay_per_step: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArcaneObservation {
    pub site_id: u64,
    pub approximate_center: AtlasPos,
    pub boundary_uncertainty_cells: u16,
    pub observed_name: String,
    pub kind: ArcanePlaceType,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArcaneDynamicState {
    pub version: u32,
    pub completed_steps: u64,
    pub last_authoritative_time: u64,
    pub cells: Vec<ArcaneDynamicCell>,
    pub wakes: Vec<ArcaneWake>,
    pub next_wake_id: u64,
    #[serde(default)]
    pub observations: BTreeMap<[u8; 16], Vec<ArcaneObservation>>,
    /// Living/exceptional sites are part of this exact subledger rather than
    /// a second balance which could drift on chunk unload.
    #[serde(default)]
    pub ecology: crate::arcane_ecology::ArcaneEcologyState,
    /// Goal-8 carrier partitions, warning history, scars, and bounded
    /// provenance. Soil/sediment units remain in `cells[*].dross`; airborne
    /// and waterborne units live here and are all owned by Geography.
    #[serde(default)]
    pub dross_state: crate::dross::DrossPlanetState,
}

/// Goal-7 dynamic payload before environmental carrier partitions joined the
/// same geography subledger. Postcard is sequence encoded, so this explicit
/// shape is the safe conservation-preserving migration path.
#[derive(Deserialize, Serialize)]
struct ArcaneDynamicStateBeforeDross {
    version: u32,
    completed_steps: u64,
    last_authoritative_time: u64,
    cells: Vec<ArcaneDynamicCell>,
    wakes: Vec<ArcaneWake>,
    next_wake_id: u64,
    observations: BTreeMap<[u8; 16], Vec<ArcaneObservation>>,
    ecology: crate::arcane_ecology::ArcaneEcologyState,
}

/// Goal-2 dynamic payload before living sites joined the same compact
/// subledger. Kept solely for deterministic, conservation-preserving
/// existing-world migration.
#[derive(Deserialize, Serialize)]
struct ArcaneDynamicStateBeforeEcology {
    version: u32,
    completed_steps: u64,
    last_authoritative_time: u64,
    cells: Vec<ArcaneDynamicCell>,
    wakes: Vec<ArcaneWake>,
    next_wake_id: u64,
    observations: BTreeMap<[u8; 16], Vec<ArcaneObservation>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArcaneGeographyManifest {
    pub schema_version: u32,
    pub algorithm_version: u32,
    pub dynamic_version: u32,
    pub journal_version: u32,
    pub seed: u32,
    pub side: u16,
    pub cell_count: u32,
    pub content_hash: u64,
    pub genesis_current: u64,
    pub genesis_resonance: [u64; 6],
    pub immutable_checksum: u64,
    pub dynamic_checksum: u64,
    pub site_catalog_checksum: u64,
    pub immutable_bytes: u64,
    pub dynamic_bytes: u64,
    pub site_catalog_bytes: u64,
    pub last_authoritative_time: u64,
    pub simulation_step_seconds: u64,
    pub complete: bool,
    pub retrogen_history: Vec<String>,
    pub stage_micros: BTreeMap<String, u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArcaneGeography {
    pub manifest: ArcaneGeographyManifest,
    pub controls: Vec<ArcaneControlCell>,
    pub dynamic: ArcaneDynamicState,
    pub catalog: ArcaneSiteCatalog,
    #[allow(dead_code)]
    path: PathBuf,
    transport: Option<TransportPass>,
    pub(crate) dross_transport: Option<crate::dross::DrossTransportPass>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TransportPass {
    target_step: u64,
    cursor: usize,
    delta: Vec<[i32; 18]>,
    remainders: Vec<i32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArcaneGeographyAudit {
    pub genesis_total: u64,
    pub external_imported_total: u64,
    pub accounted_total: u64,
    pub resonance_totals: [u64; 6],
    pub deep_total: u64,
    pub ambient_total: u64,
    pub dross_total: u64,
    pub wake_total: u64,
    pub ecology_total: u64,
    pub ecology_dross_total: u64,
    pub exported_total: u64,
    pub site_counts: BTreeMap<ArcanePlaceType, u64>,
    pub checksum: u64,
}

impl ArcaneGeographyAudit {
    pub fn is_balanced(&self) -> bool {
        self.genesis_total.checked_add(self.external_imported_total) == Some(self.accounted_total)
    }

    pub fn as_current(&self) -> Current {
        Current::from_parts(
            BASE_RESONANCES
                .into_iter()
                .zip(self.resonance_totals)
                .filter(|(_, units)| *units != 0)
                .map(|(name, units)| (name.to_string(), units)),
        )
        .expect("audited geography mixture is valid")
    }

    pub fn render(&self) -> String {
        let mut out = format!(
            "Arcane geography audit\nGenesis: {}\nExternal imported: {}\nAccounted: {}\nDeep: {}\nAmbient: {}\nDross: {}\nWakes: {}\nEcology: {} (dross {})\nNet exported: {}\nChecksum: {:016x}\nResonances:\n",
            self.genesis_total,
            self.external_imported_total,
            self.accounted_total,
            self.deep_total,
            self.ambient_total,
            self.dross_total,
            self.wake_total,
            self.ecology_total,
            self.ecology_dross_total,
            self.exported_total,
            self.checksum
        );
        for (name, units) in BASE_RESONANCES.into_iter().zip(self.resonance_totals) {
            out.push_str(&format!("  {name}: {units}\n"));
        }
        out.push_str("Places:\n");
        for (kind, count) in &self.site_counts {
            out.push_str(&format!("  {}: {count}\n", kind.label()));
        }
        out
    }
}

/// Maintain a signed Geography boundary with two non-negative arrays. An
/// outbound transfer first cancels prior net inbound Current of the same
/// resonance; only the remainder becomes outstanding exported custody.
pub(crate) fn record_geography_export(
    exported: &mut [u64; 6],
    imported: &mut [u64; 6],
    slot: usize,
    units: u64,
) -> Result<(), ArcaneGeographyError> {
    let cancelled = imported[slot].min(units);
    imported[slot] -= cancelled;
    exported[slot] = exported[slot]
        .checked_add(units - cancelled)
        .ok_or(ArcaneGeographyError::Overflow)?;
    Ok(())
}

/// Inbound Current first closes outstanding Geography exports. Any remainder
/// is a legitimate net import (for example Deep-origin waste) and extends the
/// geography audit's expected total rather than masquerading as creation.
pub(crate) fn record_geography_import(
    exported: &mut [u64; 6],
    imported: &mut [u64; 6],
    slot: usize,
    units: u64,
) -> Result<(), ArcaneGeographyError> {
    let cancelled = exported[slot].min(units);
    exported[slot] -= cancelled;
    imported[slot] = imported[slot]
        .checked_add(units - cancelled)
        .ok_or(ArcaneGeographyError::Overflow)?;
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SurveyStrength {
    Still,
    Faint,
    Steady,
    Strong,
    Saturated,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SurveyCondition {
    Stable,
    Strained,
    Fouled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArcaneSurvey {
    pub strength: SurveyStrength,
    pub condition: SurveyCondition,
    pub dominant_resonances: Vec<&'static str>,
    pub drift: Option<Direction4>,
    pub uncertainty: u8,
    pub nearby_place: Option<(u64, ArcanePlaceType, String)>,
}

impl ArcaneSurvey {
    /// What an unaided person can honestly perceive here. This deliberately
    /// contains no balance, atlas coordinate, site id, or exact drift vector;
    /// later discovery tools may expose the more precise survey fields.
    pub fn sensory_cue(&self) -> String {
        let current = match self.strength {
            SurveyStrength::Still => "The air is unnaturally quiet",
            SurveyStrength::Faint => "A faint static catches on stone",
            SurveyStrength::Steady => "Nearby minerals carry a low harmonic hum",
            SurveyStrength::Strong => "Polarized shimmer recurs at the edge of sight",
            SurveyStrength::Saturated => "Slow lights gather in the air",
        };
        let condition = match self.condition {
            SurveyCondition::Stable => "the signs hold a steady rhythm",
            SurveyCondition::Strained => "the signs flutter and lose their rhythm",
            SurveyCondition::Fouled => "the fog knots around a sour metallic haze",
        };
        let resonance = resonance_sensory_sign(self.dominant_resonances.first().copied());
        format!("{current}; {condition}; {resonance}.")
    }
}

fn resonance_sensory_sign(resonance: Option<&str>) -> &'static str {
    match resonance {
        Some(ROOT) => "plants lean toward no ordinary light",
        Some(TIDE) => "moisture beads in repeating lines",
        Some(EMBER) => "dry surfaces feel briefly warm",
        Some(STONE) => "loose mineral grains ring when disturbed",
        Some(GALE) => "hair and leaves lift against the wind",
        Some(ECHO) => "small sounds return with the wrong cadence",
        _ => "the signs have no clear character",
    }
}

/// Guest-safe sensory vocabulary derived only from two coarse bands and one
/// categorical base-resonance sign supplied by the authoritative host. A
/// dominant value of zero means no clear character; values 1..=6 identify the
/// six base resonances in their stable registry order. It never invents local
/// atlas knowledge on a remote client or exposes quantities and coordinates.
pub fn coarse_sensory_cue([current, dross]: [u8; 2], dominant: u8) -> String {
    let current = match current.min(4) {
        0 => "The air is unnaturally quiet",
        1 => "A faint static catches on stone",
        2 => "Nearby minerals carry a low harmonic hum",
        3 => "Polarized shimmer recurs at the edge of sight",
        _ => "Slow lights gather in the air",
    };
    let condition = match dross.min(crate::dross::DrossBand::BreachRisk.ordinal()) {
        0 | 1 => "the signs hold a steady rhythm",
        2 => "the signs flutter and lose their rhythm",
        3 => "a dry double-pulse repeats under a sour metallic haze",
        4 => "branching broken symmetry catches at surfaces and interrupts nearby sound",
        _ => "a slow shear-pulse repeats through air and ground like an evacuation bell",
    };
    let resonance = dominant
        .checked_sub(1)
        .and_then(|slot| BASE_RESONANCES.get(usize::from(slot)))
        .copied();
    let resonance = resonance_sensory_sign(resonance);
    format!("{current}; {condition}; {resonance}.")
}

#[derive(Debug)]
pub enum ArcaneGeographyError {
    Io(std::io::Error),
    Atlas(AtlasError),
    Corrupt(String),
    Unsupported(String),
    Cancelled,
    Overflow,
}

impl fmt::Display for ArcaneGeographyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Atlas(error) => write!(f, "{error}"),
            Self::Corrupt(message) => write!(f, "corrupt arcane geography: {message}"),
            Self::Unsupported(message) => write!(f, "unsupported arcane geography: {message}"),
            Self::Cancelled => f.write_str("arcane geography generation cancelled"),
            Self::Overflow => f.write_str("arcane geography arithmetic overflow"),
        }
    }
}

impl std::error::Error for ArcaneGeographyError {}

impl From<std::io::Error> for ArcaneGeographyError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<AtlasError> for ArcaneGeographyError {
    fn from(value: AtlasError) -> Self {
        Self::Atlas(value)
    }
}

fn stable_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn cell_hash(seed: u32, index: usize, salt: u64) -> u64 {
    mix64(u64::from(seed) ^ (index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ salt)
}

fn normalized_six(mut values: [u32; 6]) -> [u8; 6] {
    for value in &mut values {
        *value = (*value).max(1);
    }
    let total: u64 = values.into_iter().map(u64::from).sum();
    let mut out = [0u8; 6];
    let mut assigned = 0u16;
    for index in 0..6 {
        let value = ((u64::from(values[index]) * 255) / total).max(1) as u8;
        out[index] = value;
        assigned += u16::from(value);
    }
    while assigned > 255 {
        if let Some(index) = out
            .iter()
            .enumerate()
            .filter(|(_, value)| **value > 1)
            .max_by_key(|(_, value)| **value)
            .map(|(index, _)| index)
        {
            out[index] -= 1;
            assigned -= 1;
        }
    }
    while assigned < 255 {
        let index = usize::from((255 - assigned) % 6);
        out[index] += 1;
        assigned += 1;
    }
    out
}

fn edge_geometry_factor(atlas: &PlanetAtlas, index: usize, direction: Direction4) -> f64 {
    let side = atlas.side();
    let pos = AtlasPos::from_index(index, side).expect("geometry index");
    let cell = f64::from(FACE_BLOCKS / side);
    let u0 = f64::from(pos.u) * cell;
    let v0 = f64::from(pos.v) * cell;
    let u1 = u0 + cell;
    let v1 = v0 + cell;
    let (a, b) = match direction {
        Direction4::East => (
            SurfacePoint {
                face: pos.face,
                u: u1,
                v: v0,
            },
            SurfacePoint {
                face: pos.face,
                u: u1,
                v: v1,
            },
        ),
        Direction4::North => (
            SurfacePoint {
                face: pos.face,
                u: u0,
                v: v1,
            },
            SurfacePoint {
                face: pos.face,
                u: u1,
                v: v1,
            },
        ),
        Direction4::West => (
            SurfacePoint {
                face: pos.face,
                u: u0,
                v: v0,
            },
            SurfacePoint {
                face: pos.face,
                u: u0,
                v: v1,
            },
        ),
        Direction4::South => (
            SurfacePoint {
                face: pos.face,
                u: u0,
                v: v0,
            },
            SurfacePoint {
                face: pos.face,
                u: u1,
                v: v0,
            },
        ),
    };
    let edge_length = (surface_to_unit(a) - surface_to_unit(b)).length() * PLANET_RADIUS;
    let neighbor = pos.step(direction, side).pos;
    let center_spacing =
        (surface_to_unit(pos.center(side)) - surface_to_unit(neighbor.center(side))).length()
            * PLANET_RADIUS;
    (edge_length / center_spacing.max(f64::EPSILON)).clamp(0.5, 2.0)
}

fn effective_conductivity(intrinsic: i32, geometry_factor: f64) -> u16 {
    (f64::from(intrinsic.clamp(12, 1_000)) * geometry_factor)
        .round()
        .clamp(12.0, 1_000.0) as u16
}

fn derive_control(atlas: &PlanetAtlas, index: usize) -> ArcaneControlCell {
    let tectonic = atlas.genesis.tectonics.values()[index];
    let terrain = atlas.genesis.terrain.values()[index];
    let climate = atlas.genesis.climate.values()[index];
    let hydrology = atlas.genesis.hydrology.values()[index];
    let ground = atlas.genesis.ground.values()[index];
    let biome = atlas.genesis.biomes.values()[index];
    let resource = atlas.genesis.resources.values()[index];

    let water = (hydrology.mean_discharge.max(0.0).ln_1p() * 22.0
        + f32::from(ground.aquifer_permeability) / 2_048.0)
        .clamp(0.0, 220.0) as u32;
    let life = u32::from(biome.vegetation_potential)
        + u32::from(ground.organic) * 2
        + u32::from(ground.baseline_fertility);
    let fault = u32::from(tectonic.fault_intensity) / 256;
    let mineral = u32::from(resource.deposit_site_count).min(16) * 12;
    let heartshadow = u32::from(biome.heart_assignment != 0);
    let capacity_density = (2_200 + life * 3 + water * 2 + mineral + fault).clamp(512, 30_000);
    let deep_capacity_density = (3_000
        + u32::from(tectonic.crust_thickness) * 6
        + u32::from(tectonic.crust_age) * 2
        + mineral * 8
        + fault * 3)
        .clamp(1_024, 42_000);
    let mean_area = 4.0 * std::f64::consts::PI * atlas.manifest.planet_radius.powi(2)
        / atlas.genesis.geometry.len() as f64;
    let area_factor = f64::from(atlas.genesis.geometry.values()[index].physical_area) / mean_area;
    let capacity = (f64::from(capacity_density) * area_factor)
        .round()
        .clamp(256.0, 60_000.0) as u32;
    let deep_capacity = (f64::from(deep_capacity_density) * area_factor)
        .round()
        .clamp(512.0, 60_000.0) as u32;
    let exchange = (90
        + u32::from(ground.aquifer_permeability) / 96
        + fault * 2
        + u32::from(biome.habitat_flags & (HABITAT_SPRING | HABITAT_CAVE_OUTLET) != 0) * 360)
        .clamp(12, 1_000);
    let base_conductivity =
        (100 + u32::from(ground.aquifer_permeability) / 96 + fault * 2 + water * 2)
            .clamp(20, 1_000);
    let anisotropy =
        ((climate.prevailing_wind[0].abs() - climate.prevailing_wind[1].abs()) * 110.0) as i32;
    let east_west = base_conductivity as i32 + anisotropy;
    let north_south = base_conductivity as i32 - anisotropy;
    let conductivity = [
        effective_conductivity(
            east_west,
            edge_geometry_factor(atlas, index, Direction4::East),
        ),
        effective_conductivity(
            north_south,
            edge_geometry_factor(atlas, index, Direction4::North),
        ),
        effective_conductivity(
            east_west,
            edge_geometry_factor(atlas, index, Direction4::West),
        ),
        effective_conductivity(
            north_south,
            edge_geometry_factor(atlas, index, Direction4::South),
        ),
    ];
    let retention = (u32::from(ground.organic) / 2
        + u32::from(255u8.saturating_sub(ground.drainage)) / 2)
        .clamp(8, 255) as u8;
    let mobility = (u32::from(ground.aquifer_permeability) / 384
        + climate.prevailing_wind[0]
            .abs()
            .mul_add(16.0, climate.prevailing_wind[1].abs() * 16.0) as u32
        + water / 2)
        .clamp(4, 255) as u8;
    let stability =
        (780i32 + i32::from(tectonic.crust_age) / 2 + i32::from(tectonic.crust_thickness) / 4
            - i32::from(tectonic.fault_intensity) / 96
            - i32::from(tectonic.volcanic_history) * 3)
            .clamp(40, 1_000) as u16;
    let recovery = (u32::from(ground.organic)
        + u32::from(ground.baseline_fertility)
        + water
        + u32::from(biome.succession_potential))
    .saturating_add(heartshadow * 80)
    .clamp(1, 1_000) as u16;

    let root = 24 + life * 2;
    let tide = 24 + water * 4 + u32::from(hydrology.water_body != Default::default()) * 100;
    let ember = 24
        + u32::from(tectonic.volcanic_history) * 12
        + u32::from(biome.edaphic_flags & EDAPHIC_VOLCANIC != 0) * 220
        + climate.mean_temperature.max(0.0) as u32 * 3;
    let stone = 24
        + u32::from(tectonic.crust_age) * 2
        + u32::from(tectonic.crust_thickness) * 2
        + mineral * 3;
    let gale = 24
        + (climate.prevailing_wind[0].hypot(climate.prevailing_wind[1]) * 90.0) as u32
        + (terrain.eroded_elevation - 80.0).max(0.0) as u32 * 2
        + (atlas.genesis.geometry.values()[index]
            .latitude_radians
            .abs()
            * 90.0) as u32;
    let echo = 24
        + heartshadow * 180
        + u32::from(biome.habitat_flags & HABITAT_CAVE_OUTLET != 0) * 120
        + u32::from(tectonic.crust_age);

    ArcaneControlCell {
        capacity: capacity as u16,
        deep_capacity: deep_capacity as u16,
        exchange_permille: exchange as u16,
        conductivity,
        dross_mobility: mobility,
        dross_retention: retention,
        baseline_resonance: normalized_six([root, tide, ember, stone, gale, echo]),
        stability,
        recovery_potential: recovery,
        site_ref: 0,
    }
}

fn rule_matches(rule: &ArcaneSiteRule, atlas: &PlanetAtlas, index: usize) -> bool {
    let tectonic = atlas.genesis.tectonics.values()[index];
    let terrain = atlas.genesis.terrain.values()[index];
    let hydrology = atlas.genesis.hydrology.values()[index];
    let ground = atlas.genesis.ground.values()[index];
    let biome = atlas.genesis.biomes.values()[index];
    rule.requires
        .iter()
        .all(|requirement| match requirement.as_str() {
            "fault" => tectonic.fault_intensity > 16_000,
            "carbonate_rock" => biome.edaphic_flags & EDAPHIC_LIMESTONE != 0,
            "groundwater" => ground.aquifer_capacity > 0 && ground.aquifer_permeability > 12_000,
            "volcanic" => {
                tectonic.volcanic_history > 0 || biome.edaphic_flags & EDAPHIC_VOLCANIC != 0
            }
            "river" => hydrology.river_id != 0 || biome.habitat_flags & HABITAT_RIPARIAN != 0,
            "coast" => {
                hydrology.water_body != Default::default()
                    || terrain.eroded_elevation <= crate::chunk::SEA_LEVEL as f32 + 3.0
            }
            "old_crust" => tectonic.crust_age > 150,
            "heart" => biome.heart_assignment != 0,
            "wetland" => biome.habitat_flags & HABITAT_WETLAND != 0,
            _ => false,
        })
}

fn apply_creation_rules(
    atlas: &PlanetAtlas,
    registry: &Registry,
    controls: &mut [ArcaneControlCell],
) {
    for (index, control) in controls.iter_mut().enumerate() {
        for rule in &registry.arcane_sites {
            if !rule_matches(rule, atlas, index) {
                continue;
            }
            let roll =
                cell_hash(atlas.manifest.seed, index, stable_hash(rule.id.as_bytes())) % 1_000_000;
            if roll >= u64::from(rule.rarity_per_million) {
                continue;
            }
            let factor = u32::from(rule.capacity_factor_permille.clamp(250, 4_000));
            control.capacity =
                ((u32::from(control.capacity) * factor) / 1_000).clamp(256, 60_000) as u16;
            let mut weights = control.baseline_resonance.map(u32::from);
            for (slot, add) in rule.base_resonance_bias.into_iter().enumerate() {
                weights[slot] = weights[slot].saturating_add(u32::from(add) * 8);
            }
            control.baseline_resonance = normalized_six(weights);
        }
    }
}

fn weighted_allocation(
    total: u64,
    atlas: &PlanetAtlas,
    controls: &[ArcaneControlCell],
    deep: bool,
) -> Result<Vec<u16>, ArcaneGeographyError> {
    let weights = controls
        .iter()
        .map(|control| {
            let capacity = if deep {
                control.deep_capacity
            } else {
                control.capacity
            };
            u64::from(capacity)
        })
        .collect::<Vec<_>>();
    let weight_total: u128 = weights.iter().map(|weight| u128::from(*weight)).sum();
    let mut out = Vec::with_capacity(weights.len());
    let mut assigned = 0u64;
    for weight in &weights {
        let units = ((u128::from(total) * u128::from(*weight)) / weight_total) as u64;
        if units > u64::from(u16::MAX) {
            return Err(ArcaneGeographyError::Overflow);
        }
        out.push(units as u16);
        assigned = assigned
            .checked_add(units)
            .ok_or(ArcaneGeographyError::Overflow)?;
    }
    let mut remaining = total
        .checked_sub(assigned)
        .ok_or(ArcaneGeographyError::Overflow)?;
    // A seed-rotated coprime walk distributes floor remainders without cube
    // face/index privilege.  The floor error is strictly less than one cell.
    let count = out.len();
    let mut stride =
        (mix64(u64::from(atlas.manifest.seed) ^ u64::from(deep) ^ 0x9e37_79b9) as usize | 1)
            % count.max(1);
    while count > 1 && gcd(stride, count) != 1 {
        stride = (stride + 2) % count;
    }
    let mut index = (mix64(u64::from(atlas.manifest.seed) ^ 0xa11c_0ca7) as usize) % count.max(1);
    while remaining != 0 {
        out[index] = out[index]
            .checked_add(1)
            .ok_or(ArcaneGeographyError::Overflow)?;
        remaining -= 1;
        index = (index + stride.max(1)) % count;
    }
    Ok(out)
}

fn split_resonance(total: u16, weights: [u8; 6], rotation: usize) -> [u16; 6] {
    let mut out = [0u16; 6];
    let mut assigned = 0u16;
    for slot in 0..6 {
        out[slot] = ((u32::from(total) * u32::from(weights[slot])) / 255) as u16;
        assigned += out[slot];
    }
    let mut remaining = total - assigned;
    let mut slot = rotation % 6;
    while remaining != 0 {
        out[slot] += 1;
        remaining -= 1;
        slot = (slot + 1) % 6;
    }
    out
}

/// Preserve each capacity-weighted scalar cell allocation while making the
/// planet-wide six-band totals exact.  Units are relabelled preferentially in
/// cells whose baseline favors the deficit band over the surplus band.
fn assign_resonances(
    totals: &[u16],
    controls: &[ArcaneControlCell],
    targets: [u64; 6],
    seed: u32,
    salt: u64,
) -> Result<Vec<[u16; 6]>, ArcaneGeographyError> {
    let mut out = totals
        .iter()
        .enumerate()
        .map(|(index, total)| {
            split_resonance(
                *total,
                controls[index].baseline_resonance,
                cell_hash(seed, index, salt) as usize,
            )
        })
        .collect::<Vec<_>>();
    let mut actual = [0u64; 6];
    for mixture in &out {
        for slot in 0..6 {
            actual[slot] += u64::from(mixture[slot]);
        }
    }
    for deficit_slot in 0..6 {
        while actual[deficit_slot] < targets[deficit_slot] {
            let source_slot = (0..6)
                .filter(|slot| actual[*slot] > targets[*slot])
                .max_by_key(|slot| actual[*slot] - targets[*slot])
                .ok_or_else(|| {
                    ArcaneGeographyError::Corrupt(
                        "resonance targets do not sum to scalar allocation".into(),
                    )
                })?;
            let mut needed = (targets[deficit_slot] - actual[deficit_slot])
                .min(actual[source_slot] - targets[source_slot]);
            let count = out.len();
            let start = cell_hash(seed, deficit_slot * 6 + source_slot, salt) as usize % count;
            for preferred in [true, false] {
                if needed == 0 {
                    break;
                }
                for offset in 0..count {
                    if needed == 0 {
                        break;
                    }
                    let index = (start + offset) % count;
                    let bias = i16::from(controls[index].baseline_resonance[deficit_slot])
                        - i16::from(controls[index].baseline_resonance[source_slot]);
                    if (preferred && bias <= 0)
                        || (!preferred && bias > 0)
                        || out[index][source_slot] == 0
                    {
                        continue;
                    }
                    let moved = u64::from(out[index][source_slot]).min(needed);
                    out[index][source_slot] -= moved as u16;
                    out[index][deficit_slot] = out[index][deficit_slot]
                        .checked_add(moved as u16)
                        .ok_or(ArcaneGeographyError::Overflow)?;
                    actual[source_slot] -= moved;
                    actual[deficit_slot] += moved;
                    needed -= moved;
                }
            }
            if needed != 0 {
                return Err(ArcaneGeographyError::Corrupt(
                    "could not reconcile exact resonance targets".into(),
                ));
            }
        }
    }
    if actual != targets {
        return Err(ArcaneGeographyError::Corrupt(
            "resonance reconciliation ended off target".into(),
        ));
    }
    Ok(out)
}

/// Mature genesis ecology has already withdrawn its starting charge from the
/// circulating surface pool. Settle the remaining circulation back to the
/// same capacity-weighted no-flux state instead of making the first server
/// minutes pay off an artificial world-creation transient.
fn settle_genesis_circulation_after_ecology(
    atlas: &PlanetAtlas,
    controls: &[ArcaneControlCell],
    cells: &mut [ArcaneDynamicCell],
) -> Result<(), ArcaneGeographyError> {
    let mut circulating = [0u64; 6];
    for cell in cells.iter() {
        for (slot, units) in cell.deep.into_iter().chain(cell.ambient).enumerate() {
            circulating[slot % 6] = circulating[slot % 6]
                .checked_add(u64::from(units))
                .ok_or(ArcaneGeographyError::Overflow)?;
        }
    }
    let deep_capacity = controls
        .iter()
        .map(|control| u64::from(control.deep_capacity))
        .sum::<u64>();
    let ambient_capacity = controls
        .iter()
        .map(|control| u64::from(control.capacity))
        .sum::<u64>();
    let combined_capacity = deep_capacity
        .checked_add(ambient_capacity)
        .ok_or(ArcaneGeographyError::Overflow)?;
    let mut deep_targets = [0u64; 6];
    let mut ambient_targets = [0u64; 6];
    for slot in 0..6 {
        deep_targets[slot] = (u128::from(circulating[slot]) * u128::from(deep_capacity)
            / u128::from(combined_capacity)) as u64;
        ambient_targets[slot] = circulating[slot] - deep_targets[slot];
    }
    let deep_totals = weighted_allocation(deep_targets.iter().sum(), atlas, controls, true)?;
    let ambient_totals = weighted_allocation(ambient_targets.iter().sum(), atlas, controls, false)?;
    let deep = assign_resonances(
        &deep_totals,
        controls,
        deep_targets,
        atlas.manifest.seed,
        0xec01_06d3,
    )?;
    let ambient = assign_resonances(
        &ambient_totals,
        controls,
        ambient_targets,
        atlas.manifest.seed,
        0xec01_06a6,
    )?;
    for (index, cell) in cells.iter_mut().enumerate() {
        cell.deep = deep[index];
        cell.ambient = ambient[index];
        cell.transport_remainder = 0;
        let ambient_scalar = cell
            .ambient
            .into_iter()
            .map(u32::from)
            .sum::<u32>()
            .min(u32::from(u16::MAX)) as u16;
        cell.historical_min_ambient = ambient_scalar;
        cell.historical_max_ambient = ambient_scalar;
    }
    validate_genesis_equilibrium(atlas, controls, cells)
}

fn gcd(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        let remainder = a % b;
        a = b;
        b = remainder;
    }
    a
}

fn place_name(seed: u64, kind: ArcanePlaceType) -> String {
    const FIRST: [&str; 12] = [
        "Amber",
        "Ashen",
        "Bright",
        "Deep",
        "Elder",
        "Glass",
        "Hollow",
        "Iron",
        "Murmuring",
        "Pale",
        "Rain",
        "Star",
    ];
    const LAST: [&str; 12] = [
        "Bend",
        "Cairn",
        "Crossing",
        "Fold",
        "Hollow",
        "Reach",
        "Rift",
        "Run",
        "Threshold",
        "Vale",
        "Watch",
        "Weir",
    ];
    let first = FIRST[(seed as usize) % FIRST.len()];
    let last = LAST[((seed >> 16) as usize) % LAST.len()];
    format!("{first} {last} {}", kind.label())
}

struct SiteSpec<'a> {
    kind: ArcanePlaceType,
    index: usize,
    radius_cells: u16,
    extent_cells: u32,
    confidence: u16,
    network_id: u32,
    provider: &'a str,
    predicate_id: &'a str,
}

fn add_site(
    sites: &mut Vec<ArcanePlace>,
    controls: &mut [ArcaneControlCell],
    atlas: &PlanetAtlas,
    spec: SiteSpec<'_>,
) {
    let pos = AtlasPos::from_index(spec.index, atlas.side()).expect("site index");
    let hash = cell_hash(
        atlas.manifest.seed,
        spec.index,
        (spec.kind as u64).wrapping_mul(0xd1b5_4a32_d192_ed03)
            ^ stable_hash(spec.predicate_id.as_bytes()),
    );
    let id = hash | 1;
    let site_ref = sites.len() as u32 + 1;
    if controls[spec.index].site_ref == 0 {
        controls[spec.index].site_ref = site_ref;
    }
    let country_id = atlas.genesis.biomes.values()[spec.index].country_id;
    sites.push(ArcanePlace {
        id,
        kind: spec.kind,
        center: pos,
        radius_cells: spec.radius_cells,
        extent_cells: spec.extent_cells,
        confidence: spec.confidence,
        signature: controls[spec.index].baseline_resonance,
        name_seed: hash,
        generated_name: place_name(hash, spec.kind),
        network_id: spec.network_id,
        country_id,
        provider: spec.provider.to_string(),
        predicate_id: spec.predicate_id.to_string(),
        active: true,
    });
}

fn local_extreme(
    atlas: &PlanetAtlas,
    index: usize,
    score: impl Fn(usize) -> u32,
    maximum: bool,
) -> bool {
    let pos = AtlasPos::from_index(index, atlas.side()).expect("cell index");
    let own = score(index);
    pos.neighbors8(atlas.side()).into_iter().all(|neighbor| {
        let neighbor_index = neighbor.index(atlas.side());
        if maximum {
            own > score(neighbor_index) || (own == score(neighbor_index) && index < neighbor_index)
        } else {
            own < score(neighbor_index) || (own == score(neighbor_index) && index < neighbor_index)
        }
    })
}

fn well_score(controls: &[ArcaneControlCell], index: usize) -> u32 {
    u32::from(controls[index].exchange_permille) * 4 + u32::from(controls[index].deep_capacity) / 12
}

fn mean_conductivity(control: ArcaneControlCell) -> u32 {
    control.conductivity.into_iter().map(u32::from).sum::<u32>() / control.conductivity.len() as u32
}

fn max_conductivity(control: ArcaneControlCell) -> u16 {
    control.conductivity.into_iter().max().unwrap_or_default()
}

fn still_score(controls: &[ArcaneControlCell], index: usize) -> u32 {
    u32::from(controls[index].capacity) + mean_conductivity(controls[index]) * 16
}

fn confluence_score(atlas: &PlanetAtlas, controls: &[ArcaneControlCell], index: usize) -> u32 {
    let pos = AtlasPos::from_index(index, atlas.side()).expect("cell index");
    let diversity = pos
        .neighbors4(atlas.side())
        .into_iter()
        .map(|neighbor| {
            controls[neighbor.index(atlas.side())]
                .baseline_resonance
                .iter()
                .enumerate()
                .max_by_key(|(_, value)| **value)
                .map_or(0, |(slot, _)| slot as u8)
        })
        .collect::<BTreeSet<_>>()
        .len() as u32;
    mean_conductivity(controls[index]) * 4
        + diversity * 350
        + u32::from(atlas.genesis.tectonics.values()[index].fault_intensity) / 32
        + atlas.genesis.hydrology.values()[index]
            .mean_discharge
            .max(0.0)
            .ln_1p() as u32
            * 80
}

fn ruin_biome(atlas: &PlanetAtlas, index: usize) -> crate::worldgen::Biome {
    let cell = atlas.genesis.biomes.values()[index];
    if cell.habitat_flags
        & (HABITAT_AQUATIC_FRESH | HABITAT_AQUATIC_BRACKISH | HABITAT_AQUATIC_SALT)
        != 0
    {
        return crate::worldgen::Biome::Ocean;
    }
    if cell.habitat_flags & HABITAT_WETLAND != 0
        && !matches!(
            cell.baseline_biome,
            BIOME_ARCTIC | BIOME_TUNDRA | BIOME_MOUNTAINS
        )
    {
        return crate::worldgen::Biome::Swamp;
    }
    crate::worldgen::Biome::from_index(cell.baseline_biome)
        .unwrap_or(crate::worldgen::Biome::Plains)
}

fn identify_ruin_echoes(
    atlas: &PlanetAtlas,
    registry: &Registry,
    controls: &mut [ArcaneControlCell],
    sites: &mut Vec<ArcanePlace>,
) {
    let cell_blocks = FACE_BLOCKS / atlas.side();
    if cell_blocks != crate::planet_atlas::ATLAS_CELL_BLOCKS {
        // Small test atlases do not preserve the production one-cell/four-
        // chunk relationship and use heart-backed Echo fixtures instead.
        return;
    }
    for index in 0..controls.len() {
        let terrain = atlas.genesis.terrain.values()[index];
        if terrain.eroded_elevation <= crate::chunk::SEA_LEVEL as f32 + 1.0
            || terrain.eroded_elevation >= crate::chunk::CHUNK_Y as f32 - 24.0
        {
            continue;
        }
        let biome = ruin_biome(atlas, index);
        let barren = matches!(
            biome,
            crate::worldgen::Biome::Badlands
                | crate::worldgen::Biome::Scrubland
                | crate::worldgen::Biome::Tundra
                | crate::worldgen::Biome::Desert
        );
        let pos = AtlasPos::from_index(index, atlas.side()).expect("ruin echo index");
        let base_u = pos.u * cell_blocks;
        let base_v = pos.v * cell_blocks;
        'chunks: for local_v in [
            crate::chunk::CHUNK_Z as u16 / 2,
            crate::chunk::CHUNK_Z as u16 + crate::chunk::CHUNK_Z as u16 / 2,
        ] {
            for local_u in [
                crate::chunk::CHUNK_X as u16 / 2,
                crate::chunk::CHUNK_X as u16 + crate::chunk::CHUNK_X as u16 / 2,
            ] {
                let center = SurfacePos::new(pos.face, base_u + local_u, base_v + local_v)
                    .expect("production atlas chunk center is canonical");
                for (structure_index, structure) in registry.structures.iter().enumerate() {
                    if !structure
                        .biomes
                        .iter()
                        .any(|allowed| allowed.eq_ignore_ascii_case(biome.name()))
                    {
                        continue;
                    }
                    let hash = crate::planet::seeded_surface_roll(
                        atlas.manifest.seed,
                        center,
                        9_000 + structure_index as u32,
                    );
                    let rarity = if barren {
                        (structure.rarity / 2).max(1)
                    } else {
                        structure.rarity
                    };
                    if !hash.is_multiple_of(rarity) {
                        continue;
                    }
                    add_site(
                        sites,
                        controls,
                        atlas,
                        SiteSpec {
                            kind: ArcanePlaceType::Echo,
                            index,
                            radius_cells: 1,
                            extent_cells: 1,
                            confidence: 900,
                            network_id: 0,
                            provider: structure.name.split(':').next().unwrap_or("base"),
                            predicate_id: &structure.name,
                        },
                    );
                    break 'chunks;
                }
            }
        }
    }
}

fn identify_sites(
    atlas: &PlanetAtlas,
    registry: &Registry,
    controls: &mut [ArcaneControlCell],
) -> ArcaneSiteCatalog {
    let mut sites = Vec::new();

    // Heartshadows and their Echo anchors are topology-backed, never noise.
    for country in &atlas.biomes.countries {
        let index = country.heart_site.index(atlas.side());
        add_site(
            &mut sites,
            controls,
            atlas,
            SiteSpec {
                kind: ArcanePlaceType::Heartshadow,
                index,
                radius_cells: ((country.cell_count as f64 / std::f64::consts::PI).sqrt() as u16)
                    .max(1),
                extent_cells: country.cell_count,
                confidence: 1_000,
                network_id: u32::from(country.id),
                provider: "base",
                predicate_id: "country_heart",
            },
        );
        add_site(
            &mut sites,
            controls,
            atlas,
            SiteSpec {
                kind: ArcanePlaceType::Echo,
                index,
                radius_cells: 2,
                extent_cells: 9,
                confidence: 950,
                network_id: u32::from(country.id),
                provider: "base",
                predicate_id: "heart_history",
            },
        );
    }

    // Ruins are lazy voxel structures, but their rolls are finite and
    // deterministic. Record their coarse historical Echo before any chunk is
    // loaded so observation geography cannot depend on exploration order.
    identify_ruin_echoes(atlas, registry, controls, &mut sites);

    let count = controls.len();
    // Density scales with physical planet size while fixtures still receive
    // enough examples to exercise every place contract.
    let spacing = (atlas.side() / 16).max(2);
    for index in 0..count {
        let pos = AtlasPos::from_index(index, atlas.side()).expect("cell index");
        let lattice =
            (pos.u.is_multiple_of(spacing) && pos.v.is_multiple_of(spacing)) || atlas.side() <= 8;
        if lattice && local_extreme(atlas, index, |cell| well_score(controls, cell), true) {
            add_site(
                &mut sites,
                controls,
                atlas,
                SiteSpec {
                    kind: ArcanePlaceType::Well,
                    index,
                    radius_cells: 2,
                    extent_cells: 13,
                    confidence: controls[index].exchange_permille,
                    network_id: 0,
                    provider: "base",
                    predicate_id: "deep_exchange_maximum",
                },
            );
        }
        if lattice && local_extreme(atlas, index, |cell| still_score(controls, cell), false) {
            add_site(
                &mut sites,
                controls,
                atlas,
                SiteSpec {
                    kind: ArcanePlaceType::Still,
                    index,
                    radius_cells: 2,
                    extent_cells: 13,
                    confidence: 1_000u16.saturating_sub(mean_conductivity(controls[index]) as u16),
                    network_id: 0,
                    provider: "base",
                    predicate_id: "low_capacity_conductivity",
                },
            );
        }
        if lattice
            && local_extreme(
                atlas,
                index,
                |cell| confluence_score(atlas, controls, cell),
                true,
            )
        {
            add_site(
                &mut sites,
                controls,
                atlas,
                SiteSpec {
                    kind: ArcanePlaceType::Confluence,
                    index,
                    radius_cells: 3,
                    extent_cells: 29,
                    confidence: ((confluence_score(atlas, controls, index) / 8).min(1_000)) as u16,
                    network_id: 0,
                    provider: "base",
                    predicate_id: "conductive_resonance_junction",
                },
            );
        }
    }

    // Guarantee causal observation anchors on every viable major continent by
    // choosing the best actual candidate on that continent, never rerolling.
    for continent in atlas
        .geology
        .continents
        .iter()
        .filter(|record| record.major)
    {
        let candidates =
            atlas
                .genesis
                .terrain
                .values()
                .iter()
                .enumerate()
                .filter(|(index, terrain)| {
                    terrain.landmass_id == continent.id
                        && atlas.genesis.biomes.values()[*index].baseline_biome != BIOME_OCEAN
                });
        let Some((index, _)) =
            candidates.max_by_key(|(index, _)| confluence_score(atlas, controls, *index))
        else {
            continue;
        };
        if sites.iter().any(|site| {
            site.kind == ArcanePlaceType::Confluence
                && atlas.genesis.terrain.values()[site.center.index(atlas.side())].landmass_id
                    == continent.id
        }) {
            continue;
        }
        add_site(
            &mut sites,
            controls,
            atlas,
            SiteSpec {
                kind: ArcanePlaceType::Confluence,
                index,
                radius_cells: 3,
                extent_cells: 29,
                confidence: ((confluence_score(atlas, controls, index) / 8).min(1_000)) as u16,
                network_id: u32::from(continent.id),
                provider: "base",
                predicate_id: "continent_progression_anchor",
            },
        );
    }

    for rule in &registry.arcane_sites {
        for index in 0..count {
            if !rule_matches(rule, atlas, index) {
                continue;
            }
            let roll =
                cell_hash(atlas.manifest.seed, index, stable_hash(rule.id.as_bytes())) % 1_000_000;
            if roll < u64::from(rule.rarity_per_million) {
                add_site(
                    &mut sites,
                    controls,
                    atlas,
                    SiteSpec {
                        kind: ArcanePlaceType::Modded,
                        index,
                        radius_cells: rule.radius_cells,
                        extent_cells: u32::from(rule.radius_cells)
                            .saturating_mul(8)
                            .saturating_add(1),
                        confidence: 800,
                        network_id: 0,
                        provider: &rule.provider,
                        predicate_id: &rule.id,
                    },
                );
            }
        }
    }

    assign_confluence_networks(atlas, controls, &mut sites);
    ArcaneSiteCatalog {
        schema_version: ARCANE_GEOGRAPHY_SCHEMA_VERSION,
        sites,
    }
}

fn assign_confluence_networks(
    atlas: &PlanetAtlas,
    controls: &[ArcaneControlCell],
    sites: &mut [ArcanePlace],
) {
    let threshold = 500u16;
    let mut component = vec![0u32; controls.len()];
    let mut next = 1u32;
    for start in 0..controls.len() {
        if component[start] != 0 || max_conductivity(controls[start]) < threshold {
            continue;
        }
        let mut queue = VecDeque::from([start]);
        component[start] = next;
        while let Some(index) = queue.pop_front() {
            let pos = AtlasPos::from_index(index, atlas.side()).expect("component index");
            for neighbor in pos.neighbors4(atlas.side()) {
                let neighbor_index = neighbor.index(atlas.side());
                if component[neighbor_index] == 0
                    && max_conductivity(controls[neighbor_index]) >= threshold
                    && atlas.genesis.tectonics.values()[neighbor_index].plate_id
                        == atlas.genesis.tectonics.values()[index].plate_id
                {
                    component[neighbor_index] = next;
                    queue.push_back(neighbor_index);
                }
            }
        }
        next += 1;
    }
    for site in sites {
        if site.kind == ArcanePlaceType::Confluence {
            site.network_id = component[site.center.index(atlas.side())];
        }
    }
}

impl ArcaneGeography {
    pub fn genesis_total_for(atlas: &PlanetAtlas) -> Result<u64, ArcaneGeographyError> {
        u64::from(atlas.manifest.atlas_cell_count)
            .checked_mul(CURRENT_PER_ATLAS_CELL)
            .and_then(|total| total.checked_mul(GEOGRAPHY_GENESIS_NUMERATOR))
            .map(|total| total / GEOGRAPHY_GENESIS_DENOMINATOR)
            .ok_or(ArcaneGeographyError::Overflow)
    }

    pub fn generate(
        atlas: &PlanetAtlas,
        registry: &Registry,
        cancel: &crate::planet_atlas::CancellationToken,
        mut progress: impl FnMut(ArcaneGeographyProgress),
    ) -> Result<Self, ArcaneGeographyError> {
        atlas.validate()?;
        let mut stage_micros = BTreeMap::new();
        let mut announce = |stage: ArcaneGeographyStage| -> Result<Instant, ArcaneGeographyError> {
            if cancel.is_cancelled() {
                return Err(ArcaneGeographyError::Cancelled);
            }
            let completed_stages = ArcaneGeographyStage::ALL
                .iter()
                .position(|candidate| *candidate == stage)
                .unwrap_or_default();
            progress(ArcaneGeographyProgress {
                stage,
                completed_stages,
                total_stages: ArcaneGeographyStage::ALL.len(),
            });
            Ok(Instant::now())
        };

        let started = announce(ArcaneGeographyStage::ListeningToStone)?;
        let mut controls = (0..atlas.genesis.geometry.len())
            .map(|index| derive_control(atlas, index))
            .collect::<Vec<_>>();
        apply_creation_rules(atlas, registry, &mut controls);
        stage_micros.insert(
            ArcaneGeographyStage::ListeningToStone.id().into(),
            started.elapsed().as_micros().try_into().unwrap_or(u64::MAX),
        );

        let started = announce(ArcaneGeographyStage::FindingCurrent)?;
        let genesis_current = Self::genesis_total_for(atlas)?;
        let mut resonance_targets = [genesis_current / 6; 6];
        resonance_targets[5] += genesis_current % 6;
        let deep_capacity_total = controls
            .iter()
            .map(|control| u64::from(control.deep_capacity))
            .sum::<u64>();
        let ambient_capacity_total = controls
            .iter()
            .map(|control| u64::from(control.capacity))
            .sum::<u64>();
        let combined_capacity = deep_capacity_total
            .checked_add(ambient_capacity_total)
            .ok_or(ArcaneGeographyError::Overflow)?;
        let mut deep_targets = [0u64; 6];
        let mut ambient_targets = [0u64; 6];
        for (slot, target) in resonance_targets.into_iter().enumerate() {
            deep_targets[slot] = (u128::from(target) * u128::from(deep_capacity_total)
                / u128::from(combined_capacity)) as u64;
            ambient_targets[slot] = target - deep_targets[slot];
        }
        let deep_total = deep_targets.iter().sum();
        let ambient_total = ambient_targets.iter().sum();
        let deep_totals = weighted_allocation(deep_total, atlas, &controls, true)?;
        let ambient_totals = weighted_allocation(ambient_total, atlas, &controls, false)?;
        let deep_allocations = assign_resonances(
            &deep_totals,
            &controls,
            deep_targets,
            atlas.manifest.seed,
            0xd33f,
        )?;
        let ambient_allocations = assign_resonances(
            &ambient_totals,
            &controls,
            ambient_targets,
            atlas.manifest.seed,
            0xa6b1,
        )?;
        let mut cells = Vec::with_capacity(controls.len());
        for index in 0..controls.len() {
            let deep = deep_allocations[index];
            let ambient = ambient_allocations[index];
            let ambient_scalar = ambient
                .iter()
                .copied()
                .map(u32::from)
                .sum::<u32>()
                .min(u32::from(u16::MAX)) as u16;
            cells.push(ArcaneDynamicCell {
                deep,
                ambient,
                dross: [0; 6],
                transport_remainder: 0,
                wake_id: 0,
                historical_min_ambient: ambient_scalar,
                historical_max_ambient: ambient_scalar,
            });
        }
        stage_micros.insert(
            ArcaneGeographyStage::FindingCurrent.id().into(),
            started.elapsed().as_micros().try_into().unwrap_or(u64::MAX),
        );

        let started = announce(ArcaneGeographyStage::SettlingDeep)?;
        // Capacity-weighted allocation is the deterministic bounded equilibrium
        // for the initial no-flux state.  A validation pass proves every seam
        // edge sees the same integer potential within its one-unit floor error.
        validate_genesis_equilibrium(atlas, &controls, &cells)?;
        stage_micros.insert(
            ArcaneGeographyStage::SettlingDeep.id().into(),
            started.elapsed().as_micros().try_into().unwrap_or(u64::MAX),
        );

        let started = announce(ArcaneGeographyStage::MarkingConfluences)?;
        let catalog = identify_sites(atlas, registry, &mut controls);
        stage_micros.insert(
            ArcaneGeographyStage::MarkingConfluences.id().into(),
            started.elapsed().as_micros().try_into().unwrap_or(u64::MAX),
        );
        let dynamic = ArcaneDynamicState {
            version: ARCANE_GEOGRAPHY_DYNAMIC_VERSION,
            completed_steps: 0,
            last_authoritative_time: 0,
            cells,
            wakes: Vec::new(),
            next_wake_id: 1,
            observations: BTreeMap::new(),
            ecology: crate::arcane_ecology::ArcaneEcologyState::default(),
            dross_state: crate::dross::DrossPlanetState::initialized(controls.len()),
        };
        let mut geography = Self {
            manifest: ArcaneGeographyManifest {
                schema_version: ARCANE_GEOGRAPHY_SCHEMA_VERSION,
                algorithm_version: ARCANE_GEOGRAPHY_ALGORITHM_VERSION,
                dynamic_version: ARCANE_GEOGRAPHY_DYNAMIC_VERSION,
                journal_version: ARCANE_GEOGRAPHY_JOURNAL_VERSION,
                seed: atlas.manifest.seed,
                side: atlas.side(),
                cell_count: controls.len() as u32,
                content_hash: registry.content_hash,
                genesis_current,
                genesis_resonance: [0; 6],
                immutable_checksum: 0,
                dynamic_checksum: 0,
                site_catalog_checksum: 0,
                immutable_bytes: 0,
                dynamic_bytes: 0,
                site_catalog_bytes: 0,
                last_authoritative_time: 0,
                simulation_step_seconds: ARCANE_GEOGRAPHY_SIMULATION_SECONDS,
                complete: true,
                retrogen_history: Vec::new(),
                stage_micros,
            },
            controls,
            dynamic,
            catalog,
            path: PathBuf::new(),
            transport: None,
            dross_transport: None,
        };
        crate::arcane_ecology::initialize_genesis(atlas, registry, &mut geography)
            .map_err(ArcaneGeographyError::Corrupt)?;
        settle_genesis_circulation_after_ecology(
            atlas,
            &geography.controls,
            &mut geography.dynamic.cells,
        )?;
        geography.manifest.genesis_resonance = geography.audit()?.resonance_totals;
        geography.validate(atlas)?;
        validate_genesis_distribution(atlas, &geography)?;
        Ok(geography)
    }

    #[cfg(test)]
    pub fn load_or_generate(
        world_dir: &Path,
        atlas: &PlanetAtlas,
        registry: &Registry,
    ) -> Result<Self, ArcaneGeographyError> {
        let path = Self::planet_dir(world_dir).join(MANIFEST_FILE);
        if path.exists() {
            return Self::load(world_dir, atlas);
        }
        let geography = Self::generate(
            atlas,
            registry,
            &crate::planet_atlas::CancellationToken::default(),
            |_| {},
        )?;
        geography.write_new(world_dir)?;
        Self::load(world_dir, atlas)
    }

    pub fn planet_dir(world_dir: &Path) -> PathBuf {
        PlanetAtlas::planet_dir(world_dir)
    }

    pub fn write_new(&self, world_dir: &Path) -> Result<(), ArcaneGeographyError> {
        let planet_dir = Self::planet_dir(world_dir);
        fs::create_dir_all(&planet_dir)?;
        if planet_dir.join(MANIFEST_FILE).exists() {
            return Err(ArcaneGeographyError::Corrupt(
                "arcane geography already exists".into(),
            ));
        }
        let mut manifest = self.manifest.clone();
        let immutable = encode_container(IMMUTABLE_MAGIC, &self.controls)?;
        let dynamic = encode_container(DYNAMIC_MAGIC, &self.dynamic)?;
        let catalog = toml::to_string_pretty(&self.catalog)
            .map_err(|error| ArcaneGeographyError::Corrupt(error.to_string()))?;
        check_size("immutable geography", immutable.len(), MAX_IMMUTABLE_BYTES)?;
        check_size("dynamic geography", dynamic.len(), MAX_DYNAMIC_BYTES)?;
        check_size("site catalog", catalog.len(), MAX_CATALOG_BYTES)?;
        manifest.immutable_checksum = stable_hash(&immutable);
        manifest.dynamic_checksum = stable_hash(&dynamic);
        manifest.site_catalog_checksum = stable_hash(catalog.as_bytes());
        manifest.immutable_bytes = immutable.len() as u64;
        manifest.dynamic_bytes = dynamic.len() as u64;
        manifest.site_catalog_bytes = catalog.len() as u64;
        crate::persist::atomic_write(&planet_dir.join(IMMUTABLE_FILE), &immutable, false)?;
        crate::persist::atomic_write(&planet_dir.join(DYNAMIC_FILE), &dynamic, false)?;
        crate::persist::atomic_write(&planet_dir.join(CATALOG_FILE), catalog.as_bytes(), false)?;
        write_manifest(&planet_dir, &manifest)?;
        crate::planet_atlas::update_arcane_geography_manifest(
            world_dir,
            &crate::planet_atlas::ArcaneGeographyManifestCheckpoint {
                schema_version: manifest.schema_version,
                algorithm_version: manifest.algorithm_version,
                dynamic_version: manifest.dynamic_version,
                genesis_total: manifest.genesis_current,
                immutable_checksum: manifest.immutable_checksum,
                dynamic_checksum: manifest.dynamic_checksum,
                site_catalog_checksum: manifest.site_catalog_checksum,
                last_authoritative_time: manifest.last_authoritative_time,
            },
        )?;
        Ok(())
    }

    pub fn load(world_dir: &Path, atlas: &PlanetAtlas) -> Result<Self, ArcaneGeographyError> {
        let planet_dir = Self::planet_dir(world_dir);
        recover_pending(&planet_dir)?;
        let current = load_geography_candidate(
            world_dir,
            &planet_dir,
            MANIFEST_FILE,
            IMMUTABLE_FILE,
            DYNAMIC_FILE,
            CATALOG_FILE,
            true,
        );
        let (manifest, controls, dynamic, catalog) = match current {
            Ok(candidate) => candidate,
            Err(primary) => {
                let backup = load_geography_candidate(
                    world_dir,
                    &planet_dir,
                    MANIFEST_BACKUP_FILE,
                    IMMUTABLE_BACKUP_FILE,
                    DYNAMIC_BACKUP_FILE,
                    CATALOG_BACKUP_FILE,
                    false,
                )
                .or_else(|_| {
                    load_geography_candidate(
                        world_dir,
                        &planet_dir,
                        MANIFEST_BACKUP_FILE,
                        IMMUTABLE_FILE,
                        DYNAMIC_BACKUP_FILE,
                        CATALOG_BACKUP_FILE,
                        false,
                    )
                })
                .map_err(|backup| {
                    ArcaneGeographyError::Corrupt(format!(
                        "geography checkpoint and backup are unrecoverable (primary: {primary}; backup: {backup})"
                    ))
                })?;
                let (manifest, controls, dynamic, catalog) = backup;
                let immutable = encode_container(IMMUTABLE_MAGIC, &controls)?;
                let dynamic_bytes = encode_container(DYNAMIC_MAGIC, &dynamic)?;
                let catalog_text = toml::to_string_pretty(&catalog)
                    .map_err(|error| ArcaneGeographyError::Corrupt(error.to_string()))?;
                crate::persist::atomic_write(&planet_dir.join(IMMUTABLE_FILE), &immutable, false)?;
                crate::persist::atomic_write(
                    &planet_dir.join(DYNAMIC_FILE),
                    &dynamic_bytes,
                    false,
                )?;
                crate::persist::atomic_write(
                    &planet_dir.join(CATALOG_FILE),
                    catalog_text.as_bytes(),
                    false,
                )?;
                write_manifest(&planet_dir, &manifest)?;
                update_atlas_geography_checkpoint(world_dir, &manifest)?;
                eprintln!("warning: restored arcane geography checkpoint after: {primary}");
                (manifest, controls, dynamic, catalog)
            }
        };
        let geography = Self {
            manifest,
            controls,
            dynamic: {
                let mut dynamic = dynamic;
                dynamic
                    .dross_state
                    .ensure_cells(atlas.genesis.geometry.len());
                dynamic
            },
            catalog,
            path: planet_dir,
            transport: None,
            dross_transport: None,
        };
        geography.validate(atlas)?;
        Ok(geography)
    }

    pub fn save_dynamic(&mut self, world_dir: &Path) -> Result<(), ArcaneGeographyError> {
        // In-progress passes are deliberately discarded on checkpoint and
        // restart from the last accepted state, making crash timing irrelevant.
        self.transport = None;
        self.dynamic.last_authoritative_time = self.manifest.last_authoritative_time;
        let planet_dir = Self::planet_dir(world_dir);
        let bytes = encode_container(DYNAMIC_MAGIC, &self.dynamic)?;
        let catalog = toml::to_string_pretty(&self.catalog)
            .map_err(|error| ArcaneGeographyError::Corrupt(error.to_string()))?;
        check_size("dynamic geography", bytes.len(), MAX_DYNAMIC_BYTES)?;
        check_size("site catalog", catalog.len(), MAX_CATALOG_BYTES)?;
        let checksum = stable_hash(&bytes);
        crate::persist::atomic_write(&planet_dir.join(DYNAMIC_PENDING_FILE), &bytes, false)?;
        if let Ok(previous) = read_bounded(&planet_dir.join(DYNAMIC_FILE), MAX_DYNAMIC_BYTES) {
            crate::persist::atomic_write(&planet_dir.join(DYNAMIC_BACKUP_FILE), &previous, false)?;
        }
        if let Ok(previous) = read_bounded(&planet_dir.join(CATALOG_FILE), MAX_CATALOG_BYTES) {
            crate::persist::atomic_write(&planet_dir.join(CATALOG_BACKUP_FILE), &previous, false)?;
        }
        if let Ok(previous) = read_bounded(&planet_dir.join(MANIFEST_FILE), MAX_MANIFEST_BYTES) {
            crate::persist::atomic_write(&planet_dir.join(MANIFEST_BACKUP_FILE), &previous, false)?;
        }
        crate::persist::atomic_write(&planet_dir.join(DYNAMIC_FILE), &bytes, false)?;
        crate::persist::atomic_write(&planet_dir.join(CATALOG_FILE), catalog.as_bytes(), false)?;
        self.manifest.dynamic_checksum = checksum;
        self.manifest.dynamic_bytes = bytes.len() as u64;
        self.manifest.site_catalog_checksum = stable_hash(catalog.as_bytes());
        self.manifest.site_catalog_bytes = catalog.len() as u64;
        self.manifest.last_authoritative_time = self.dynamic.last_authoritative_time;
        write_manifest(&planet_dir, &self.manifest)?;
        crate::planet_atlas::update_arcane_geography_manifest(
            world_dir,
            &crate::planet_atlas::ArcaneGeographyManifestCheckpoint {
                schema_version: self.manifest.schema_version,
                algorithm_version: self.manifest.algorithm_version,
                dynamic_version: self.manifest.dynamic_version,
                genesis_total: self.manifest.genesis_current,
                immutable_checksum: self.manifest.immutable_checksum,
                dynamic_checksum: self.manifest.dynamic_checksum,
                site_catalog_checksum: self.manifest.site_catalog_checksum,
                last_authoritative_time: self.manifest.last_authoritative_time,
            },
        )?;
        let _ = crate::persist::remove_if_exists(&planet_dir.join(DYNAMIC_PENDING_FILE));
        Ok(())
    }

    /// Bytes used by the arcane journal's linked coordinator when charge
    /// leaves a plant/crystal/mineral for an item. Dynamic geography, its own
    /// manifest, and the parent planet checkpoint then recover forward with
    /// the ledger transaction as one operation.
    pub(crate) fn linked_dynamic_replacements(
        &self,
        world_dir: &Path,
        operation_id: u64,
    ) -> Result<
        (
            ArcaneGeographyManifest,
            Vec<crate::arcane::LinkedFileReplacement>,
        ),
        ArcaneGeographyError,
    > {
        self.linked_dynamic_replacements_for(world_dir, operation_id, "ecology")
    }

    pub(crate) fn linked_dross_replacements(
        &self,
        world_dir: &Path,
        operation_id: u64,
    ) -> Result<
        (
            ArcaneGeographyManifest,
            Vec<crate::arcane::LinkedFileReplacement>,
        ),
        ArcaneGeographyError,
    > {
        self.linked_dynamic_replacements_for(world_dir, operation_id, "dross")
    }

    fn linked_dynamic_replacements_for(
        &self,
        world_dir: &Path,
        operation_id: u64,
        subsystem: &str,
    ) -> Result<
        (
            ArcaneGeographyManifest,
            Vec<crate::arcane::LinkedFileReplacement>,
        ),
        ArcaneGeographyError,
    > {
        let dynamic = encode_container(DYNAMIC_MAGIC, &self.dynamic)?;
        check_size("dynamic geography", dynamic.len(), MAX_DYNAMIC_BYTES)?;
        let mut manifest = self.manifest.clone();
        manifest.dynamic_checksum = stable_hash(&dynamic);
        manifest.dynamic_bytes = dynamic.len() as u64;
        manifest.last_authoritative_time = self.dynamic.last_authoritative_time;
        let manifest_text = toml::to_string_pretty(&manifest)
            .map_err(|error| ArcaneGeographyError::Corrupt(error.to_string()))?;
        check_size(
            "arcane geography manifest",
            manifest_text.len(),
            MAX_MANIFEST_BYTES,
        )?;
        let checkpoint = manifest_checkpoint(&manifest);
        let planet_manifest =
            crate::planet_atlas::arcane_geography_manifest_payload(world_dir, &checkpoint)?;
        Ok((
            manifest,
            vec![
                crate::arcane::LinkedFileReplacement {
                    subsystem: subsystem.into(),
                    operation_id,
                    relative_path: "planet/arcane-geography.wad".into(),
                    after: Some(dynamic),
                },
                crate::arcane::LinkedFileReplacement {
                    subsystem: subsystem.into(),
                    operation_id,
                    relative_path: "planet/arcane-geography.toml".into(),
                    after: Some(manifest_text.into_bytes()),
                },
                crate::arcane::LinkedFileReplacement {
                    subsystem: subsystem.into(),
                    operation_id,
                    relative_path: "planet/manifest.toml".into(),
                    after: Some(planet_manifest),
                },
            ],
        ))
    }

    pub(crate) fn accept_linked_manifest(&mut self, manifest: ArcaneGeographyManifest) {
        self.manifest = manifest;
    }

    pub fn validate(&self, atlas: &PlanetAtlas) -> Result<(), ArcaneGeographyError> {
        if !self.manifest.complete
            || self.manifest.seed != atlas.manifest.seed
            || self.manifest.side != atlas.side()
            || usize::try_from(self.manifest.cell_count).ok() != Some(self.controls.len())
            || self.controls.len() != self.dynamic.cells.len()
            || self.controls.len() != atlas.genesis.geometry.len()
        {
            return Err(ArcaneGeographyError::Corrupt(
                "dimensions or seed do not match the qualified atlas".into(),
            ));
        }
        self.dynamic
            .dross_state
            .validate(self.dynamic.cells.len(), self.manifest.side)
            .map_err(ArcaneGeographyError::Corrupt)?;
        for (index, control) in self.controls.iter().enumerate() {
            if control.capacity == 0
                || control.deep_capacity == 0
                || control.exchange_permille > 1_000
                || control.conductivity.into_iter().any(|value| value > 1_000)
                || control
                    .baseline_resonance
                    .into_iter()
                    .map(u16::from)
                    .sum::<u16>()
                    != 255
                || usize::try_from(control.site_ref)
                    .ok()
                    .is_some_and(|site_ref| site_ref > self.catalog.sites.len())
            {
                return Err(ArcaneGeographyError::Corrupt(format!(
                    "invalid immutable control at cell {index}"
                )));
            }
        }
        let audit = self.audit()?;
        let conserved_resonance: [u64; 6] = std::array::from_fn(|slot| {
            audit.resonance_totals[slot].saturating_add(self.dynamic.ecology.exported[slot])
        });
        let expected_resonance: [u64; 6] = std::array::from_fn(|slot| {
            self.manifest.genesis_resonance[slot]
                .saturating_add(self.dynamic.dross_state.external_imported[slot])
        });
        if !audit.is_balanced() || conserved_resonance != expected_resonance {
            return Err(ArcaneGeographyError::Corrupt(
                "dynamic state does not reconcile with genesis".into(),
            ));
        }
        validate_sites(atlas, &self.catalog)?;
        Ok(())
    }

    pub fn audit(&self) -> Result<ArcaneGeographyAudit, ArcaneGeographyError> {
        let mut resonance_totals = [0u64; 6];
        let mut deep_total = 0u64;
        let mut ambient_total = 0u64;
        let mut dross_total = 0u64;
        for (index, cell) in self.dynamic.cells.iter().enumerate() {
            for (slot, resonance_total) in resonance_totals.iter_mut().enumerate() {
                let deep = u64::from(cell.deep[slot]);
                let ambient = u64::from(cell.ambient[slot]);
                let soil_dross = u64::from(cell.dross[slot]);
                let airborne_dross =
                    u64::from(self.dynamic.dross_state.cells[index].airborne[slot]);
                let waterborne_dross =
                    u64::from(self.dynamic.dross_state.cells[index].waterborne[slot]);
                let dross = soil_dross
                    .checked_add(airborne_dross)
                    .and_then(|value| value.checked_add(waterborne_dross))
                    .ok_or(ArcaneGeographyError::Overflow)?;
                *resonance_total = resonance_total
                    .checked_add(deep)
                    .and_then(|value| value.checked_add(ambient))
                    .and_then(|value| value.checked_add(dross))
                    .ok_or(ArcaneGeographyError::Overflow)?;
                deep_total += deep;
                ambient_total += ambient;
                dross_total += dross;
            }
        }
        let mut wake_total = 0u64;
        for wake in &self.dynamic.wakes {
            for (slot, resonance_total) in resonance_totals.iter_mut().enumerate() {
                let units = u64::from(wake.charge[slot]) + u64::from(wake.dross[slot]);
                *resonance_total = resonance_total
                    .checked_add(units)
                    .ok_or(ArcaneGeographyError::Overflow)?;
                wake_total += units;
            }
        }
        let (ecology_charge, ecology_dross, ecology_checksum) =
            crate::arcane_ecology::custody_totals(&self.dynamic.ecology)
                .map_err(ArcaneGeographyError::Corrupt)?;
        let mut ecology_total = 0u64;
        let mut ecology_dross_total = 0u64;
        for slot in 0..6 {
            resonance_totals[slot] = resonance_totals[slot]
                .checked_add(ecology_charge[slot])
                .and_then(|value| value.checked_add(ecology_dross[slot]))
                .ok_or(ArcaneGeographyError::Overflow)?;
            ecology_total = ecology_total
                .checked_add(ecology_charge[slot])
                .ok_or(ArcaneGeographyError::Overflow)?;
            ecology_dross_total = ecology_dross_total
                .checked_add(ecology_dross[slot])
                .ok_or(ArcaneGeographyError::Overflow)?;
        }
        let exported_total = self
            .dynamic
            .ecology
            .exported
            .iter()
            .try_fold(0u64, |sum, units| sum.checked_add(*units))
            .ok_or(ArcaneGeographyError::Overflow)?;
        let external_imported_total = self
            .dynamic
            .dross_state
            .external_imported
            .iter()
            .try_fold(0u64, |sum, units| sum.checked_add(*units))
            .ok_or(ArcaneGeographyError::Overflow)?;
        let accounted_total = resonance_totals
            .iter()
            .try_fold(exported_total, |sum, units| sum.checked_add(*units))
            .ok_or(ArcaneGeographyError::Overflow)?;
        let mut site_counts = BTreeMap::new();
        for site in self.catalog.sites.iter().filter(|site| site.active) {
            *site_counts.entry(site.kind).or_insert(0) += 1;
        }
        let mut checksum =
            mix64(self.dynamic.completed_steps ^ self.dynamic.last_authoritative_time);
        for (index, cell) in self.dynamic.cells.iter().enumerate() {
            let mut value = index as u64;
            for units in cell.deep.into_iter().chain(cell.ambient).chain(cell.dross) {
                value = mix64(value ^ u64::from(units));
            }
            for units in self.dynamic.dross_state.cells[index]
                .airborne
                .into_iter()
                .chain(self.dynamic.dross_state.cells[index].waterborne)
            {
                value = mix64(value ^ u64::from(units));
            }
            value ^= (cell.transport_remainder as u32 as u64) << 17;
            value ^= u64::from(cell.wake_id) << 31;
            checksum = mix64(checksum ^ value);
        }
        for wake in &self.dynamic.wakes {
            checksum = mix64(checksum ^ wake.id ^ wake.site_id);
        }
        checksum = mix64(checksum ^ ecology_checksum);
        for (slot, units) in self
            .dynamic
            .dross_state
            .external_imported
            .iter()
            .enumerate()
        {
            checksum = mix64(checksum ^ units.rotate_left((slot * 7) as u32));
        }
        Ok(ArcaneGeographyAudit {
            genesis_total: self.manifest.genesis_current,
            external_imported_total,
            accounted_total,
            resonance_totals,
            deep_total,
            ambient_total,
            dross_total,
            wake_total,
            ecology_total,
            ecology_dross_total,
            exported_total,
            site_counts,
            checksum,
        })
    }

    pub fn custody_current(&self) -> Result<Current, ArcaneGeographyError> {
        Ok(self.audit()?.as_current())
    }

    /// Move a bounded amount of local Ambient Current out of the planetary
    /// geography subledger. The caller must durably pair the returned mixture
    /// with an equal `ArcaneOwner::Geography` debit and either accept the
    /// staged geography files or roll this mutation back.
    pub(crate) fn export_ambient_for_apparatus(
        &mut self,
        pos: AtlasPos,
        requested: u64,
    ) -> Result<Current, ArcaneGeographyError> {
        if requested == 0 || pos.u >= self.manifest.side || pos.v >= self.manifest.side {
            return Err(ArcaneGeographyError::Corrupt(
                "apparatus requested an invalid geography export".into(),
            ));
        }
        let cell = self
            .dynamic
            .cells
            .get_mut(pos.index(self.manifest.side))
            .ok_or_else(|| {
                ArcaneGeographyError::Corrupt("apparatus source cell is absent".into())
            })?;
        let available = cell.ambient.into_iter().map(u64::from).sum::<u64>();
        let wanted = requested.min(available);
        if wanted == 0 {
            return Err(ArcaneGeographyError::Corrupt(
                "the attached magical place has no Ambient Current to export".into(),
            ));
        }
        let mut remaining = wanted;
        let mut exported = Current::default();
        for (slot, resonance) in BASE_RESONANCES.iter().enumerate() {
            let amount = remaining.min(u64::from(cell.ambient[slot]));
            if amount == 0 {
                continue;
            }
            let amount_u16 = u16::try_from(amount).map_err(|_| ArcaneGeographyError::Overflow)?;
            cell.ambient[slot] = cell.ambient[slot]
                .checked_sub(amount_u16)
                .ok_or(ArcaneGeographyError::Overflow)?;
            record_geography_export(
                &mut self.dynamic.ecology.exported,
                &mut self.dynamic.dross_state.external_imported,
                slot,
                amount,
            )?;
            exported
                .checked_add(&Current::single(*resonance, amount))
                .map_err(|_| ArcaneGeographyError::Overflow)?;
            remaining -= amount;
            if remaining == 0 {
                break;
            }
        }
        if remaining != 0 || exported.total() != wanted {
            return Err(ArcaneGeographyError::Corrupt(
                "apparatus geography export did not settle exactly".into(),
            ));
        }
        self.dynamic.ecology.event_sequence = self
            .dynamic
            .ecology
            .event_sequence
            .checked_add(1)
            .ok_or(ArcaneGeographyError::Overflow)?;
        Ok(exported)
    }

    pub fn survey(&self, pos: AtlasPos, tuning_lens: bool) -> ArcaneSurvey {
        let index = pos.index(self.manifest.side);
        let control = self.controls[index];
        let cell = self.dynamic.cells[index];
        let ambient = cell.ambient_total();
        let capacity = u64::from(control.capacity);
        let strength = match ambient.saturating_mul(5) / capacity.max(1) {
            0 => SurveyStrength::Still,
            1 => SurveyStrength::Faint,
            2 => SurveyStrength::Steady,
            3 | 4 => SurveyStrength::Strong,
            _ => SurveyStrength::Saturated,
        };
        let dross_band = self.dynamic.dross_state.cells[index].band;
        let condition = if dross_band >= crate::dross::DrossBand::Seep {
            SurveyCondition::Fouled
        } else if dross_band >= crate::dross::DrossBand::Strained || control.stability < 350 {
            SurveyCondition::Strained
        } else {
            SurveyCondition::Stable
        };
        let mut bands = cell.ambient.into_iter().enumerate().collect::<Vec<_>>();
        bands.sort_by_key(|(_, units)| std::cmp::Reverse(*units));
        let dominant_resonances = bands
            .into_iter()
            .take(if tuning_lens { 2 } else { 1 })
            .filter(|(_, units)| *units != 0)
            .map(|(slot, _)| BASE_RESONANCES[slot])
            .collect();
        let drift = self.drift_direction(pos);
        let nearby_place = self
            .catalog
            .sites
            .iter()
            .filter(|site| site.active)
            .find(|site| {
                site.center == pos || site.center.neighbors8(self.manifest.side).contains(&pos)
            })
            .map(|site| (site.id, site.kind, site.generated_name.clone()));
        ArcaneSurvey {
            strength,
            condition,
            dominant_resonances,
            drift,
            uncertainty: if tuning_lens { 12 } else { 65 },
            nearby_place,
        }
    }

    pub fn local_bands(&self, pos: AtlasPos) -> [u8; 2] {
        let index = pos.index(self.manifest.side);
        let cell = self.dynamic.cells[index];
        let capacity = u64::from(self.controls[index].capacity.max(1));
        [
            ((cell.ambient_total().saturating_mul(5) / capacity).min(4)) as u8,
            self.dynamic.dross_state.cells[index].band.ordinal(),
        ]
    }

    /// One guest-safe categorical sign for the strongest local base
    /// resonance. Zero means that no Ambient Current is present; otherwise
    /// 1..=6 follows `BASE_RESONANCES`. Ties resolve to the first stable base
    /// resonance, matching unaided surveys without exposing exact mixtures.
    pub fn local_dominant_resonance(&self, pos: AtlasPos) -> u8 {
        let ambient = self.dynamic.cells[pos.index(self.manifest.side)].ambient;
        let Some(maximum) = ambient.iter().copied().max().filter(|units| *units != 0) else {
            return 0;
        };
        ambient
            .iter()
            .position(|units| *units == maximum)
            .map_or(0, |slot| slot as u8 + 1)
    }

    pub fn early_discovery_reachable(&self, atlas: &PlanetAtlas, from: AtlasPos) -> bool {
        let landmass = atlas.genesis.terrain.values()[from.index(atlas.side())].landmass_id;
        landmass != 0
            && self.catalog.sites.iter().any(|site| {
                site.active
                    && matches!(
                        site.kind,
                        ArcanePlaceType::Confluence | ArcanePlaceType::Well | ArcanePlaceType::Echo
                    )
                    && atlas.genesis.terrain.values()[site.center.index(atlas.side())].landmass_id
                        == landmass
            })
    }

    pub fn drift_direction(&self, pos: AtlasPos) -> Option<Direction4> {
        let own = potential(
            self.dynamic.cells[pos.index(self.manifest.side)].ambient_total(),
            self.controls[pos.index(self.manifest.side)].capacity,
        );
        [
            Direction4::East,
            Direction4::North,
            Direction4::West,
            Direction4::South,
        ]
        .into_iter()
        .map(|direction| {
            let neighbor = pos.step(direction, self.manifest.side).pos;
            let index = neighbor.index(self.manifest.side);
            let neighbor_potential = potential(
                self.dynamic.cells[index].ambient_total(),
                self.controls[index].capacity,
            );
            (direction, own - neighbor_potential)
        })
        .filter(|(_, difference)| *difference > 0.000_001)
        .max_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(direction, _)| direction)
    }

    /// Sliced authoritative transport. A pass treats the authoritative cells
    /// as immutable, writes integer deltas, and commits only after every cell
    /// has contributed. Thus worker count, residency, and call slicing cannot
    /// alter it without allocating a second full copy of the geography.
    pub fn advance_toward(
        &mut self,
        authoritative_seconds: u64,
        cell_budget: usize,
    ) -> Result<usize, ArcaneGeographyError> {
        let target_step = authoritative_seconds / ARCANE_GEOGRAPHY_SIMULATION_SECONDS;
        if self.dynamic.completed_steps >= target_step {
            self.manifest.last_authoritative_time = authoritative_seconds;
            return Ok(0);
        }
        if self.transport.is_none() {
            self.transport = Some(TransportPass {
                target_step: self.dynamic.completed_steps + 1,
                cursor: 0,
                delta: Vec::with_capacity(self.dynamic.cells.len()),
                remainders: Vec::with_capacity(self.dynamic.cells.len()),
            });
        }
        let pass = self.transport.as_mut().expect("transport initialized");
        if pass.delta.len() < self.dynamic.cells.len() {
            let start = pass.delta.len();
            let end = (start + cell_budget.max(1)).min(self.dynamic.cells.len());
            pass.delta.resize(end, [0; 18]);
            pass.remainders.extend(
                self.dynamic.cells[start..end]
                    .iter()
                    .map(|cell| cell.transport_remainder.max(0)),
            );
            return Ok(end - start);
        }
        let end = (pass.cursor + cell_budget.max(1)).min(self.dynamic.cells.len());
        for index in pass.cursor..end {
            compute_cell_flux(
                self.manifest.side,
                &self.controls,
                &self.dynamic.cells,
                pass,
                index,
            )?;
        }
        let processed = end - pass.cursor;
        pass.cursor = end;
        if pass.cursor == self.dynamic.cells.len() {
            let pass = self.transport.take().expect("completed pass");
            apply_flux(&mut self.dynamic.cells, &pass.delta, &pass.remainders)?;
            decay_wakes(&mut self.dynamic, &mut self.catalog, self.manifest.side)?;
            self.dynamic.completed_steps = pass.target_step;
            let completed_time = pass
                .target_step
                .checked_mul(ARCANE_GEOGRAPHY_SIMULATION_SECONDS)
                .ok_or(ArcaneGeographyError::Overflow)?;
            self.dynamic.last_authoritative_time = completed_time;
            self.manifest.last_authoritative_time = completed_time;
        }
        Ok(processed)
    }

    pub fn advance_steps_exact(&mut self, steps: u64) -> Result<(), ArcaneGeographyError> {
        let target = self
            .dynamic
            .completed_steps
            .checked_add(steps)
            .ok_or(ArcaneGeographyError::Overflow)?
            .checked_mul(ARCANE_GEOGRAPHY_SIMULATION_SECONDS)
            .ok_or(ArcaneGeographyError::Overflow)?;
        while self.dynamic.completed_steps < target / ARCANE_GEOGRAPHY_SIMULATION_SECONDS {
            self.advance_toward(target, self.dynamic.cells.len())?;
        }
        Ok(())
    }

    #[allow(dead_code)] // Goal 8 pollution paths call this finite transfer hook.
    pub fn foul_ambient(&mut self, pos: AtlasPos, units: u16) -> Result<u16, ArcaneGeographyError> {
        self.transport = None;
        let index = pos.index(self.manifest.side);
        let available = self.dynamic.cells[index]
            .ambient_total()
            .min(u64::from(u16::MAX)) as u16;
        let units = units.min(available);
        let moved = take_proportional(
            self.dynamic.cells[index].ambient,
            units,
            index ^ self.dynamic.completed_steps as usize,
        );
        for (slot, moved) in moved.into_iter().enumerate() {
            self.dynamic.cells[index].ambient[slot] -= moved;
            self.dynamic.cells[index].dross[slot] = self.dynamic.cells[index].dross[slot]
                .checked_add(moved)
                .ok_or(ArcaneGeographyError::Overflow)?;
        }
        Ok(units)
    }

    #[allow(dead_code)] // Warden, storm, and ritual producers arrive in later goals.
    pub fn begin_wake(
        &mut self,
        path: Vec<AtlasPos>,
        charge_units: u16,
        dross_units: u16,
        decay_per_step: u16,
    ) -> Result<u64, ArcaneGeographyError> {
        self.transport = None;
        let Some(&origin) = path.first() else {
            return Err(ArcaneGeographyError::Corrupt(
                "a wake requires a nonempty planetary path".into(),
            ));
        };
        if path
            .iter()
            .any(|pos| pos.u >= self.manifest.side || pos.v >= self.manifest.side)
            || decay_per_step == 0
        {
            return Err(ArcaneGeographyError::Corrupt(
                "wake path or decay rate is invalid".into(),
            ));
        }
        let index = origin.index(self.manifest.side);
        let charge = take_proportional(
            self.dynamic.cells[index].ambient,
            charge_units.min(
                self.dynamic.cells[index]
                    .ambient_total()
                    .min(u64::from(u16::MAX)) as u16,
            ),
            index,
        );
        let dross = take_proportional(
            self.dynamic.cells[index].dross,
            dross_units.min(
                self.dynamic.cells[index]
                    .dross_total()
                    .min(u64::from(u16::MAX)) as u16,
            ),
            index + 3,
        );
        for slot in 0..6 {
            self.dynamic.cells[index].ambient[slot] -= charge[slot];
            self.dynamic.cells[index].dross[slot] -= dross[slot];
        }
        let id = self.dynamic.next_wake_id;
        self.dynamic.next_wake_id = id.checked_add(1).ok_or(ArcaneGeographyError::Overflow)?;
        let site_id = mix64(
            u64::from(self.manifest.seed) ^ id ^ (index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15),
        ) | 1;
        let name_seed = mix64(site_id ^ 0x7a6e);
        self.catalog.sites.push(ArcanePlace {
            id: site_id,
            kind: ArcanePlaceType::Wake,
            center: origin,
            radius_cells: 1,
            extent_cells: path.len() as u32,
            confidence: 500,
            signature: self.controls[index].baseline_resonance,
            name_seed,
            generated_name: place_name(name_seed, ArcanePlaceType::Wake),
            network_id: 0,
            country_id: 0,
            provider: "base".into(),
            predicate_id: "temporary_wake".into(),
            active: true,
        });
        self.dynamic.cells[index].wake_id = id.min(u64::from(u32::MAX)) as u32;
        self.dynamic.wakes.push(ArcaneWake {
            id,
            site_id,
            path,
            cursor: 0,
            charge,
            dross,
            decay_per_step,
        });
        Ok(id)
    }

    #[allow(dead_code)] // Goal 8 manifestations call this persistent identity hook.
    pub fn mark_scar(&mut self, atlas: &PlanetAtlas, pos: AtlasPos) -> u64 {
        if let Some(site) = self
            .catalog
            .sites
            .iter()
            .find(|site| site.kind == ArcanePlaceType::Scar && site.center == pos)
        {
            return site.id;
        }
        let index = pos.index(self.manifest.side);
        let id = cell_hash(
            atlas.manifest.seed,
            index,
            stable_hash(b"dross_manifestation"),
        ) | 1;
        let name_seed = mix64(id ^ 0x5ca4);
        self.catalog.sites.push(ArcanePlace {
            id,
            kind: ArcanePlaceType::Scar,
            center: pos,
            radius_cells: 1,
            extent_cells: 1,
            confidence: 1_000,
            signature: self.controls[index].baseline_resonance,
            name_seed,
            generated_name: place_name(name_seed, ArcanePlaceType::Scar),
            network_id: 0,
            country_id: atlas.genesis.biomes.values()[index].country_id,
            provider: "base".into(),
            predicate_id: "dross_manifestation".into(),
            active: true,
        });
        id
    }

    #[allow(dead_code)] // Goal 4 map/research UI records these bounded observations.
    pub fn record_observation(
        &mut self,
        player: [u8; 16],
        from: AtlasPos,
        tuning_lens: bool,
    ) -> Option<ArcaneObservation> {
        let side = self.manifest.side;
        let nearest = self
            .catalog
            .sites
            .iter()
            .filter(|site| site.active)
            .min_by_key(|site| {
                let du = site.center.u.abs_diff(from.u);
                let dv = site.center.v.abs_diff(from.v);
                u32::from(du).saturating_mul(u32::from(du))
                    + u32::from(dv).saturating_mul(u32::from(dv))
                    + u32::from(site.center.face != from.face) * u32::from(side).pow(2)
            })?;
        let uncertainty = if tuning_lens { 2 } else { 8.min(side) };
        let quantize = uncertainty.max(1);
        let approximate_center = AtlasPos {
            face: nearest.center.face,
            u: (nearest.center.u / quantize) * quantize,
            v: (nearest.center.v / quantize) * quantize,
        };
        let observation = ArcaneObservation {
            site_id: nearest.id,
            approximate_center,
            boundary_uncertainty_cells: uncertainty,
            observed_name: nearest.generated_name.clone(),
            kind: nearest.kind,
        };
        let observations = self.dynamic.observations.entry(player).or_default();
        if let Some(saved) = observations
            .iter_mut()
            .find(|saved| saved.site_id == observation.site_id)
        {
            if observation.boundary_uncertainty_cells < saved.boundary_uncertainty_cells {
                *saved = observation.clone();
            }
        } else {
            observations.push(observation.clone());
        }
        Some(observation)
    }
}

fn validate_genesis_equilibrium(
    atlas: &PlanetAtlas,
    controls: &[ArcaneControlCell],
    cells: &[ArcaneDynamicCell],
) -> Result<(), ArcaneGeographyError> {
    for index in 0..cells.len() {
        let deep_surface_lhs =
            u128::from(cells[index].deep_total()) * u128::from(controls[index].capacity);
        let deep_surface_rhs =
            u128::from(cells[index].ambient_total()) * u128::from(controls[index].deep_capacity);
        let deep_surface_tolerance =
            u128::from(controls[index].capacity.max(controls[index].deep_capacity)) * 4;
        if deep_surface_lhs.abs_diff(deep_surface_rhs) > deep_surface_tolerance {
            return Err(ArcaneGeographyError::Corrupt(format!(
                "deep/surface genesis equilibrium discontinuity at cell {index}"
            )));
        }
        let pos = AtlasPos::from_index(index, atlas.side()).expect("cell index");
        for neighbor in pos.neighbors4(atlas.side()) {
            let other = neighbor.index(atlas.side());
            let lhs =
                u128::from(cells[index].ambient_total()) * u128::from(controls[other].capacity);
            let rhs =
                u128::from(cells[other].ambient_total()) * u128::from(controls[index].capacity);
            let tolerance = u128::from(controls[index].capacity.max(controls[other].capacity));
            if lhs.abs_diff(rhs) > tolerance.saturating_mul(4) {
                // Area weighting deliberately bends uniform potential by cell
                // area.  Only reject an implausible discontinuity, not that
                // physically caused variation.
                let area_a = atlas.genesis.geometry.values()[index].physical_area;
                let area_b = atlas.genesis.geometry.values()[other].physical_area;
                if (area_a / area_b).max(area_b / area_a) < 1.02 {
                    return Err(ArcaneGeographyError::Corrupt(format!(
                        "genesis equilibrium discontinuity at {pos:?}"
                    )));
                }
            }
        }
    }
    Ok(())
}

fn potential(units: u64, capacity: u16) -> f64 {
    units as f64 / f64::from(capacity.max(1))
}

fn axis_conductivity(control: ArcaneControlCell, direction: Direction4) -> u16 {
    match direction {
        Direction4::East => control.conductivity[0],
        Direction4::North => control.conductivity[1],
        Direction4::West => control.conductivity[2],
        Direction4::South => control.conductivity[3],
    }
}

fn bounded_flow(
    source_units: u64,
    source_capacity: u16,
    target_units: u64,
    target_capacity: u16,
    conductivity: u16,
    divisor: u64,
    remainder: &mut i64,
) -> u16 {
    let lhs = u128::from(source_units) * u128::from(target_capacity);
    let rhs = u128::from(target_units) * u128::from(source_capacity);
    if lhs <= rhs || source_units == 0 {
        return 0;
    }
    // Capacity-weighted genesis can differ by a few indivisible subunits.
    // Treat that discrete floor as equilibrium instead of accumulating it in
    // the fixed-point remainder until a unit shuttles back and forth forever.
    let equilibrium_floor = u128::from(source_capacity.max(target_capacity)) * 4;
    if lhs - rhs <= equilibrium_floor {
        *remainder = 0;
        return 0;
    }
    let denominator = u128::from(source_capacity.max(target_capacity)) * u128::from(divisor.max(1));
    let fixed = (lhs - rhs) * u128::from(conductivity) * 1_000_000 / 1_000 / denominator
        + u128::try_from((*remainder).max(0)).unwrap_or_default();
    let raw = fixed / 1_000_000;
    let capped = raw
        .min(u128::from(source_units / 8 + 1))
        .min(u128::from(u16::MAX));
    *remainder = if capped == raw {
        (fixed % 1_000_000) as i64
    } else {
        0
    };
    capped as u16
}

fn take_proportional(source: [u16; 6], units: u16, rotation: usize) -> [u16; 6] {
    let total: u32 = source.into_iter().map(u32::from).sum();
    if units == 0 || total == 0 {
        return [0; 6];
    }
    let units = u32::from(units).min(total);
    let mut out = [0u16; 6];
    let mut assigned = 0u32;
    for slot in 0..6 {
        out[slot] = ((units * u32::from(source[slot])) / total) as u16;
        assigned += u32::from(out[slot]);
    }
    let mut remainder = units - assigned;
    let mut cursor = rotation % 6;
    while remainder != 0 {
        if out[cursor] < source[cursor] {
            out[cursor] += 1;
            remainder -= 1;
        }
        cursor = (cursor + 1) % 6;
    }
    out
}

fn add_transfer(
    delta: &mut [[i32; 18]],
    from: usize,
    to: usize,
    offset: usize,
    source: [u16; 6],
    mut moved: [u16; 6],
) -> u16 {
    let mut moved_total = 0u16;
    for slot in 0..6 {
        let already_debited = delta[from][offset + slot].saturating_neg().max(0) as u16;
        moved[slot] = moved[slot].min(source[slot].saturating_sub(already_debited));
        let units = i32::from(moved[slot]);
        delta[from][offset + slot] -= units;
        delta[to][offset + slot] += units;
        moved_total = moved_total.saturating_add(moved[slot]);
    }
    moved_total
}

fn resonance_deviation(mixture: [u16; 6], baseline: [u8; 6], slot: usize) -> i32 {
    let total: u32 = mixture.into_iter().map(u32::from).sum();
    i32::from(mixture[slot]) - ((total * u32::from(baseline[slot])) / 255) as i32
}

/// Slowly trades equal scalar quantities between two resonance mixtures.
/// This runs only when scalar Current actually crosses the edge: an idle
/// equilibrium is therefore motionless, while a disturbed region relaxes
/// toward the saved geographic baselines without creating Current or changing
/// any planet-wide base-resonance total.
fn add_resonance_swap(
    pass: &mut TransportPass,
    controls: &[ArcaneControlCell],
    cells: &[ArcaneDynamicCell],
    first: usize,
    second: usize,
    conductivity: u16,
) {
    if conductivity < 64
        || !cell_hash(pass.target_step as u32, first ^ second, 0xba5e_11ae).is_multiple_of(8)
    {
        return;
    }
    let a = cells[first].ambient;
    let b = cells[second].ambient;
    let mut best = None::<(i32, usize, usize)>;
    for from_a in 0..6 {
        let a_excess = resonance_deviation(a, controls[first].baseline_resonance, from_a);
        let b_need = -resonance_deviation(b, controls[second].baseline_resonance, from_a);
        if a_excess <= 0 || b_need <= 0 {
            continue;
        }
        for from_b in 0..6 {
            if from_a == from_b {
                continue;
            }
            let b_excess = resonance_deviation(b, controls[second].baseline_resonance, from_b);
            let a_need = -resonance_deviation(a, controls[first].baseline_resonance, from_b);
            let score = a_excess.min(b_need).min(b_excess).min(a_need);
            if score > best.map_or(0, |candidate| candidate.0) {
                best = Some((score, from_a, from_b));
            }
        }
    }
    let Some((score, from_a, from_b)) = best else {
        return;
    };
    let mut units = score.min(1 + i32::from(conductivity) / 500) as u16;
    let a_debited = pass.delta[first][6 + from_a].saturating_neg().max(0) as u16;
    let b_debited = pass.delta[second][6 + from_b].saturating_neg().max(0) as u16;
    units = units
        .min(a[from_a].saturating_sub(a_debited))
        .min(b[from_b].saturating_sub(b_debited));
    if units == 0 {
        return;
    }
    let units = i32::from(units);
    pass.delta[first][6 + from_a] -= units;
    pass.delta[second][6 + from_a] += units;
    pass.delta[second][6 + from_b] -= units;
    pass.delta[first][6 + from_b] += units;
}

fn compute_cell_flux(
    side: u16,
    controls: &[ArcaneControlCell],
    cells: &[ArcaneDynamicCell],
    pass: &mut TransportPass,
    index: usize,
) -> Result<(), ArcaneGeographyError> {
    let pos = AtlasPos::from_index(index, side).expect("transport index");
    let source = cells[index];
    let mut remainder = i64::from(pass.remainders[index]);
    for direction in [
        Direction4::East,
        Direction4::North,
        Direction4::West,
        Direction4::South,
    ] {
        let step = pos.step(direction, side);
        let other = step.pos.index(side);
        if index >= other {
            continue;
        }
        let target = cells[other];
        let conductivity = axis_conductivity(controls[index], direction).min(axis_conductivity(
            controls[other],
            step.direction.opposite(),
        ));
        let (from, to, moved) = if potential(source.ambient_total(), controls[index].capacity)
            > potential(target.ambient_total(), controls[other].capacity)
        {
            let units = bounded_flow(
                source.ambient_total(),
                controls[index].capacity,
                target.ambient_total(),
                controls[other].capacity,
                conductivity,
                16,
                &mut remainder,
            );
            (
                index,
                other,
                take_proportional(source.ambient, units, index + other),
            )
        } else {
            let units = bounded_flow(
                target.ambient_total(),
                controls[other].capacity,
                source.ambient_total(),
                controls[index].capacity,
                conductivity,
                16,
                &mut remainder,
            );
            (
                other,
                index,
                take_proportional(target.ambient, units, index + other),
            )
        };
        let ambient_moved = add_transfer(&mut pass.delta, from, to, 6, cells[from].ambient, moved);
        if ambient_moved != 0 {
            add_resonance_swap(pass, controls, cells, index, other, conductivity);
        }

        let source_dross = source.dross_total();
        let target_dross = target.dross_total();
        let dross_mobility = u32::from(
            controls[index]
                .dross_mobility
                .min(controls[other].dross_mobility),
        ) * 4;
        let dross_conductivity =
            ((dross_mobility * u32::from(conductivity)) / 1_000).clamp(1, 1_000) as u16;
        let (from, to, moved) = if potential(source_dross, controls[index].capacity)
            > potential(target_dross, controls[other].capacity)
        {
            let units = bounded_flow(
                source_dross,
                controls[index].capacity,
                target_dross,
                controls[other].capacity,
                dross_conductivity,
                96,
                &mut remainder,
            );
            (
                index,
                other,
                take_proportional(source.dross, units, index ^ other),
            )
        } else {
            let units = bounded_flow(
                target_dross,
                controls[other].capacity,
                source_dross,
                controls[index].capacity,
                dross_conductivity,
                96,
                &mut remainder,
            );
            (
                other,
                index,
                take_proportional(target.dross, units, index ^ other),
            )
        };
        let _ = add_transfer(&mut pass.delta, from, to, 12, cells[from].dross, moved);
    }

    // Deep/surface exchange is a single equal debit/credit in this cell.
    let ambient_potential = potential(source.ambient_total(), controls[index].capacity);
    let deep_potential = potential(source.deep_total(), controls[index].deep_capacity);
    if deep_potential > ambient_potential {
        let units = bounded_flow(
            source.deep_total(),
            controls[index].deep_capacity,
            source.ambient_total(),
            controls[index].capacity,
            controls[index].exchange_permille,
            32,
            &mut remainder,
        );
        let moved = take_proportional(source.deep, units, index);
        for slot in 0..6 {
            let available = source.deep[slot]
                .saturating_sub(pass.delta[index][slot].saturating_neg().max(0) as u16);
            let moved = moved[slot].min(available);
            pass.delta[index][slot] -= i32::from(moved);
            pass.delta[index][6 + slot] += i32::from(moved);
        }
    } else if ambient_potential > deep_potential {
        let units = bounded_flow(
            source.ambient_total(),
            controls[index].capacity,
            source.deep_total(),
            controls[index].deep_capacity,
            controls[index].exchange_permille,
            32,
            &mut remainder,
        );
        let moved = take_proportional(source.ambient, units, index);
        for slot in 0..6 {
            let available = source.ambient[slot]
                .saturating_sub(pass.delta[index][6 + slot].saturating_neg().max(0) as u16);
            let moved = moved[slot].min(available);
            pass.delta[index][6 + slot] -= i32::from(moved);
            pass.delta[index][slot] += i32::from(moved);
        }
    }
    pass.remainders[index] = remainder.clamp(0, 999_999) as i32;
    Ok(())
}

fn apply_flux(
    cells: &mut [ArcaneDynamicCell],
    delta: &[[i32; 18]],
    remainders: &[i32],
) -> Result<(), ArcaneGeographyError> {
    for ((cell, changes), remainder) in cells.iter_mut().zip(delta).zip(remainders) {
        for slot in 0..6 {
            cell.deep[slot] = apply_signed(cell.deep[slot], changes[slot])?;
            cell.ambient[slot] = apply_signed(cell.ambient[slot], changes[6 + slot])?;
            cell.dross[slot] = apply_signed(cell.dross[slot], changes[12 + slot])?;
        }
        let ambient = cell
            .ambient
            .iter()
            .copied()
            .map(u32::from)
            .sum::<u32>()
            .min(u32::from(u16::MAX)) as u16;
        cell.historical_min_ambient = cell.historical_min_ambient.min(ambient);
        cell.historical_max_ambient = cell.historical_max_ambient.max(ambient);
        cell.transport_remainder = (*remainder).clamp(0, 999_999);
    }
    Ok(())
}

fn apply_signed(value: u16, change: i32) -> Result<u16, ArcaneGeographyError> {
    let next = i64::from(value) + i64::from(change);
    u16::try_from(next).map_err(|_| {
        ArcaneGeographyError::Corrupt(format!(
            "transport would move a cell band outside u16: {value} + {change}"
        ))
    })
}

fn decay_wakes(
    state: &mut ArcaneDynamicState,
    catalog: &mut ArcaneSiteCatalog,
    side: u16,
) -> Result<(), ArcaneGeographyError> {
    let mut retained = Vec::with_capacity(state.wakes.len());
    for mut wake in state.wakes.drain(..) {
        let Some(&pos) = wake.path.get(wake.cursor as usize) else {
            continue;
        };
        let index = pos.index(side);
        for slot in 0..6 {
            let charge = wake.charge[slot]
                .min(wake.decay_per_step)
                .min(u16::MAX - state.cells[index].ambient[slot]);
            let dross = wake.dross[slot]
                .min(wake.decay_per_step)
                .min(u16::MAX - state.cells[index].dross[slot]);
            state.cells[index].ambient[slot] += charge;
            state.cells[index].dross[slot] += dross;
            wake.charge[slot] -= charge;
            wake.dross[slot] -= dross;
        }
        wake.cursor = (wake.cursor + 1).min(wake.path.len().saturating_sub(1) as u32);
        if wake
            .charge
            .into_iter()
            .chain(wake.dross)
            .any(|units| units != 0)
        {
            retained.push(wake);
        } else {
            state.cells[index].wake_id = 0;
            if let Some(site) = catalog
                .sites
                .iter_mut()
                .find(|site| site.id == wake.site_id)
            {
                site.active = false;
            }
        }
    }
    state.wakes = retained;
    Ok(())
}

fn validate_genesis_distribution(
    atlas: &PlanetAtlas,
    geography: &ArcaneGeography,
) -> Result<(), ArcaneGeographyError> {
    if atlas.side() < 16 {
        return Ok(());
    }
    let available_landmasses = atlas
        .genesis
        .terrain
        .values()
        .iter()
        .map(|terrain| terrain.landmass_id)
        .filter(|landmass| *landmass != 0)
        .collect::<BTreeSet<_>>()
        .len();
    let required_landmasses = available_landmasses.min(2);
    for continent in atlas
        .geology
        .continents
        .iter()
        .filter(|continent| continent.major)
    {
        let usable = atlas
            .genesis
            .terrain
            .values()
            .iter()
            .enumerate()
            .filter(|(_, terrain)| terrain.landmass_id == continent.id)
            .map(|(index, _)| geography.dynamic.cells[index].ambient_total())
            .sum::<u64>();
        if usable == 0 {
            return Err(ArcaneGeographyError::Corrupt(format!(
                "major continent {} has no usable genesis Current",
                continent.id
            )));
        }
    }
    for (slot, resonance) in BASE_RESONANCES.iter().enumerate() {
        let mut provinces = BTreeSet::new();
        let mut faces = BTreeSet::new();
        let mut landmasses = BTreeSet::new();
        for (index, control) in geography.controls.iter().enumerate() {
            if control.baseline_resonance[slot] == 0
                || geography.dynamic.cells[index].ambient[slot] == 0
            {
                continue;
            }
            let pos = AtlasPos::from_index(index, atlas.side()).expect("distribution index");
            provinces.insert(atlas.genesis.tectonics.values()[index].geological_province);
            faces.insert(pos.face);
            let landmass = atlas.genesis.terrain.values()[index].landmass_id;
            if landmass != 0 {
                landmasses.insert(landmass);
            }
        }
        if provinces.len() < 2 || faces.len() < 2 || landmasses.len() < required_landmasses {
            return Err(ArcaneGeographyError::Corrupt(format!(
                "{resonance} genesis is restricted: provinces={}, faces={}, landmasses={}",
                provinces.len(),
                faces.len(),
                landmasses.len()
            )));
        }
    }
    for country in &atlas.biomes.countries {
        let usable = atlas
            .genesis
            .biomes
            .values()
            .iter()
            .enumerate()
            .filter(|(_, biome)| biome.country_id == country.id)
            .map(|(index, _)| geography.dynamic.cells[index].ambient_total())
            .sum::<u64>();
        if usable == 0 {
            return Err(ArcaneGeographyError::Corrupt(format!(
                "country {} is magically inert at genesis",
                country.id
            )));
        }
    }
    let mut capacities = geography
        .controls
        .iter()
        .map(|control| control.capacity)
        .collect::<Vec<_>>();
    capacities.sort_unstable();
    let median = u32::from(capacities[capacities.len() / 2]);
    let rich = u32::from(capacities[capacities.len() * 99 / 100]);
    if rich.saturating_mul(100) < median.saturating_mul(110) {
        return Err(ArcaneGeographyError::Corrupt(
            "genesis has no exceptional high-capacity tail".into(),
        ));
    }
    Ok(())
}

fn validate_sites(
    atlas: &PlanetAtlas,
    catalog: &ArcaneSiteCatalog,
) -> Result<(), ArcaneGeographyError> {
    let mut ids = BTreeSet::new();
    for site in &catalog.sites {
        if site.id == 0
            || !ids.insert(site.id)
            || site.center.u >= atlas.side()
            || site.center.v >= atlas.side()
            || site.generated_name.trim().is_empty()
            || site.provider.trim().is_empty()
        {
            return Err(ArcaneGeographyError::Corrupt(
                "site catalog contains an invalid or duplicate site".into(),
            ));
        }
    }
    let active_networks = catalog
        .sites
        .iter()
        .filter(|site| site.active && site.kind == ArcanePlaceType::Confluence)
        .map(|site| site.network_id)
        .filter(|network| *network != 0)
        .collect::<BTreeSet<_>>();
    if atlas.side() >= 16 && active_networks.len() < 3 {
        return Err(ArcaneGeographyError::Corrupt(format!(
            "only {} independent confluence networks; three required",
            active_networks.len()
        )));
    }
    if atlas.side() >= 16 {
        for kind in [
            ArcanePlaceType::Confluence,
            ArcanePlaceType::Well,
            ArcanePlaceType::Still,
            ArcanePlaceType::Echo,
            ArcanePlaceType::Heartshadow,
        ] {
            if !catalog
                .sites
                .iter()
                .any(|site| site.active && site.kind == kind)
            {
                return Err(ArcaneGeographyError::Corrupt(format!(
                    "qualified planet has no active {}",
                    kind.label()
                )));
            }
        }
        for kind in [ArcanePlaceType::Well, ArcanePlaceType::Still] {
            let climates = catalog
                .sites
                .iter()
                .filter(|site| site.active && site.kind == kind)
                .map(|site| {
                    let climate = atlas.genesis.climate.values()[site.center.index(atlas.side())];
                    if climate.aridity > 1.5 {
                        0
                    } else if climate.mean_temperature < 2.0 {
                        1
                    } else if climate.mean_precipitation > 1_100.0 {
                        2
                    } else {
                        3
                    }
                })
                .collect::<BTreeSet<_>>();
            if climates.len() < 2 {
                return Err(ArcaneGeographyError::Corrupt(format!(
                    "{} sites occur in fewer than two climate regimes",
                    kind.label()
                )));
            }
        }
        for (slot, resonance) in BASE_RESONANCES.iter().enumerate() {
            let mut provinces = BTreeSet::new();
            let mut faces = BTreeSet::new();
            for index in 0..atlas.genesis.geometry.len() {
                let pos = AtlasPos::from_index(index, atlas.side()).expect("resonance index");
                // The baseline threshold excludes a merely one-unit registry
                // trace while retaining mixed causal regions.
                let site_signature = catalog
                    .sites
                    .iter()
                    .any(|site| site.active && site.center == pos && site.signature[slot] >= 16);
                if site_signature {
                    provinces.insert(atlas.genesis.tectonics.values()[index].geological_province);
                    faces.insert(pos.face);
                }
            }
            if provinces.len() < 2 || faces.len() < 2 {
                return Err(ArcaneGeographyError::Corrupt(format!(
                    "{} is progression-critical in only one region or cube face",
                    resonance
                )));
            }
        }
    }
    for continent in atlas
        .geology
        .continents
        .iter()
        .filter(|continent| continent.major)
    {
        let has_site = catalog.sites.iter().any(|site| {
            site.active
                && matches!(
                    site.kind,
                    ArcanePlaceType::Confluence | ArcanePlaceType::Well
                )
                && atlas.genesis.terrain.values()[site.center.index(atlas.side())].landmass_id
                    == continent.id
        });
        if !has_site {
            return Err(ArcaneGeographyError::Corrupt(format!(
                "major continent {} has no stable observation site",
                continent.id
            )));
        }
    }
    Ok(())
}

fn encode_container<T: Serialize>(
    magic: &[u8; 4],
    value: &T,
) -> Result<Vec<u8>, ArcaneGeographyError> {
    let payload = postcard::to_allocvec(value)
        .map_err(|error| ArcaneGeographyError::Corrupt(error.to_string()))?;
    let mut out = Vec::with_capacity(20 + payload.len());
    out.extend_from_slice(magic);
    out.extend_from_slice(&ARCANE_GEOGRAPHY_SCHEMA_VERSION.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    out.extend_from_slice(&stable_hash(&payload).to_le_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

fn decode_container<T: for<'de> Deserialize<'de>>(
    magic: &[u8; 4],
    bytes: &[u8],
) -> Result<T, ArcaneGeographyError> {
    if bytes.len() < 24 || &bytes[..4] != magic {
        return Err(ArcaneGeographyError::Corrupt(
            "invalid container header".into(),
        ));
    }
    let version = u32::from_le_bytes(bytes[4..8].try_into().expect("header slice"));
    let length = u64::from_le_bytes(bytes[8..16].try_into().expect("header slice"));
    let checksum = u64::from_le_bytes(bytes[16..24].try_into().expect("header slice"));
    if version != ARCANE_GEOGRAPHY_SCHEMA_VERSION
        || usize::try_from(length).ok() != Some(bytes.len() - 24)
        || stable_hash(&bytes[24..]) != checksum
    {
        return Err(ArcaneGeographyError::Corrupt(
            "container version, length, or checksum mismatch".into(),
        ));
    }
    postcard::from_bytes(&bytes[24..])
        .map_err(|error| ArcaneGeographyError::Corrupt(error.to_string()))
}

fn decode_dynamic_container(bytes: &[u8]) -> Result<ArcaneDynamicState, ArcaneGeographyError> {
    match decode_container(DYNAMIC_MAGIC, bytes) {
        Ok(dynamic) => Ok(dynamic),
        Err(current_error) => {
            match decode_container::<ArcaneDynamicStateBeforeDross>(DYNAMIC_MAGIC, bytes) {
                Ok(legacy) => {
                    let cell_count = legacy.cells.len();
                    Ok(ArcaneDynamicState {
                        version: legacy.version,
                        completed_steps: legacy.completed_steps,
                        last_authoritative_time: legacy.last_authoritative_time,
                        cells: legacy.cells,
                        wakes: legacy.wakes,
                        next_wake_id: legacy.next_wake_id,
                        observations: legacy.observations,
                        ecology: legacy.ecology,
                        dross_state: crate::dross::DrossPlanetState::initialized(cell_count),
                    })
                }
                Err(before_dross_error) => {
                    let legacy: ArcaneDynamicStateBeforeEcology =
                        decode_container(DYNAMIC_MAGIC, bytes).map_err(|legacy_error| {
                            ArcaneGeographyError::Corrupt(format!(
                                "dynamic geography is neither current ({current_error}), pre-dross ({before_dross_error}), nor pre-ecology ({legacy_error})"
                            ))
                        })?;
                    let cell_count = legacy.cells.len();
                    Ok(ArcaneDynamicState {
                        version: legacy.version,
                        completed_steps: legacy.completed_steps,
                        last_authoritative_time: legacy.last_authoritative_time,
                        cells: legacy.cells,
                        wakes: legacy.wakes,
                        next_wake_id: legacy.next_wake_id,
                        observations: legacy.observations,
                        ecology: crate::arcane_ecology::ArcaneEcologyState::default(),
                        dross_state: crate::dross::DrossPlanetState::initialized(cell_count),
                    })
                }
            }
        }
    }
}

fn check_size(label: &str, size: usize, limit: u64) -> Result<(), ArcaneGeographyError> {
    if size as u64 > limit {
        return Err(ArcaneGeographyError::Corrupt(format!(
            "{label} is {size} bytes; limit is {limit}"
        )));
    }
    Ok(())
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, ArcaneGeographyError> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > limit {
        return Err(ArcaneGeographyError::Corrupt(format!(
            "{} exceeds its size bound",
            path.display()
        )));
    }
    Ok(fs::read(path)?)
}

fn write_manifest(
    planet_dir: &Path,
    manifest: &ArcaneGeographyManifest,
) -> Result<(), ArcaneGeographyError> {
    let text = toml::to_string_pretty(manifest)
        .map_err(|error| ArcaneGeographyError::Corrupt(error.to_string()))?;
    check_size("arcane geography manifest", text.len(), MAX_MANIFEST_BYTES)?;
    crate::persist::atomic_write(&planet_dir.join(MANIFEST_FILE), text.as_bytes(), false)?;
    Ok(())
}

fn manifest_checkpoint(
    manifest: &ArcaneGeographyManifest,
) -> crate::planet_atlas::ArcaneGeographyManifestCheckpoint {
    crate::planet_atlas::ArcaneGeographyManifestCheckpoint {
        schema_version: manifest.schema_version,
        algorithm_version: manifest.algorithm_version,
        dynamic_version: manifest.dynamic_version,
        genesis_total: manifest.genesis_current,
        immutable_checksum: manifest.immutable_checksum,
        dynamic_checksum: manifest.dynamic_checksum,
        site_catalog_checksum: manifest.site_catalog_checksum,
        last_authoritative_time: manifest.last_authoritative_time,
    }
}

fn update_atlas_geography_checkpoint(
    world_dir: &Path,
    manifest: &ArcaneGeographyManifest,
) -> Result<(), ArcaneGeographyError> {
    crate::planet_atlas::update_arcane_geography_manifest(
        world_dir,
        &manifest_checkpoint(manifest),
    )?;
    Ok(())
}

fn load_geography_candidate(
    world_dir: &Path,
    planet_dir: &Path,
    manifest_file: &str,
    immutable_file: &str,
    dynamic_file: &str,
    catalog_file: &str,
    verify_atlas: bool,
) -> Result<
    (
        ArcaneGeographyManifest,
        Vec<ArcaneControlCell>,
        ArcaneDynamicState,
        ArcaneSiteCatalog,
    ),
    ArcaneGeographyError,
> {
    let manifest_bytes = read_bounded(&planet_dir.join(manifest_file), MAX_MANIFEST_BYTES)?;
    let manifest: ArcaneGeographyManifest = toml::from_str(
        std::str::from_utf8(&manifest_bytes)
            .map_err(|_| ArcaneGeographyError::Corrupt("manifest is not UTF-8".into()))?,
    )
    .map_err(|error| ArcaneGeographyError::Corrupt(format!("manifest TOML: {error}")))?;
    if manifest.schema_version != ARCANE_GEOGRAPHY_SCHEMA_VERSION
        || manifest.algorithm_version != ARCANE_GEOGRAPHY_ALGORITHM_VERSION
        || manifest.dynamic_version != ARCANE_GEOGRAPHY_DYNAMIC_VERSION
    {
        return Err(ArcaneGeographyError::Unsupported(format!(
            "versions {}/{}/{}",
            manifest.schema_version, manifest.algorithm_version, manifest.dynamic_version
        )));
    }
    let immutable = read_bounded(&planet_dir.join(immutable_file), MAX_IMMUTABLE_BYTES)?;
    let dynamic = read_bounded(&planet_dir.join(dynamic_file), MAX_DYNAMIC_BYTES)?;
    let catalog_bytes = read_bounded(&planet_dir.join(catalog_file), MAX_CATALOG_BYTES)?;
    if stable_hash(&immutable) != manifest.immutable_checksum
        || stable_hash(&dynamic) != manifest.dynamic_checksum
        || stable_hash(&catalog_bytes) != manifest.site_catalog_checksum
    {
        return Err(ArcaneGeographyError::Corrupt(
            "one or more committed checksums do not match".into(),
        ));
    }
    if verify_atlas {
        crate::planet_atlas::verify_arcane_geography_manifest(
            world_dir,
            &manifest_checkpoint(&manifest),
        )?;
    }
    let controls = decode_container(IMMUTABLE_MAGIC, &immutable)?;
    let dynamic = decode_dynamic_container(&dynamic)?;
    let catalog = toml::from_str(
        std::str::from_utf8(&catalog_bytes)
            .map_err(|_| ArcaneGeographyError::Corrupt("catalog is not UTF-8".into()))?,
    )
    .map_err(|error| ArcaneGeographyError::Corrupt(format!("catalog TOML: {error}")))?;
    Ok((manifest, controls, dynamic, catalog))
}

fn recover_pending(planet_dir: &Path) -> Result<(), ArcaneGeographyError> {
    let pending = planet_dir.join(DYNAMIC_PENDING_FILE);
    if !pending.exists() {
        return Ok(());
    }
    let bytes = read_bounded(&pending, MAX_DYNAMIC_BYTES)?;
    let _ = decode_dynamic_container(&bytes)?;
    // The manifest is authoritative.  A pending file matching it is a
    // completed data write whose cleanup was interrupted; otherwise discard it.
    let manifest_bytes = read_bounded(&planet_dir.join(MANIFEST_FILE), MAX_MANIFEST_BYTES)?;
    let manifest: ArcaneGeographyManifest = toml::from_str(
        std::str::from_utf8(&manifest_bytes)
            .map_err(|_| ArcaneGeographyError::Corrupt("manifest is not UTF-8".into()))?,
    )
    .map_err(|error| ArcaneGeographyError::Corrupt(error.to_string()))?;
    if stable_hash(&bytes) == manifest.dynamic_checksum {
        crate::persist::atomic_write(&planet_dir.join(DYNAMIC_FILE), &bytes, false)?;
    }
    let _ = crate::persist::remove_if_exists(&pending);
    Ok(())
}

#[derive(Clone, Debug, Serialize)]
pub struct ArcaneGeographyExportReport {
    pub validation: String,
    pub schema_version: u32,
    pub algorithm_version: u32,
    pub dynamic_version: u32,
    pub cell_count: usize,
    pub site_count: usize,
    pub genesis_total: u64,
    pub immutable_bytes: u64,
    pub dynamic_bytes: u64,
    pub site_catalog_bytes: u64,
    pub estimated_loaded_bytes: u64,
    pub export_elapsed_micros: u64,
    pub maps: Vec<String>,
}

#[derive(Clone, Copy)]
enum DiagnosticLayer {
    Capacity,
    Conductivity,
    Stability,
    DeepReserve,
    Ambient,
    Dross,
    DrossBand,
    Resonance(usize),
    Potential,
    Drift,
    PlaceId,
    PlaceType,
    Recovery,
}

impl DiagnosticLayer {
    fn all() -> Vec<(&'static str, Self)> {
        vec![
            ("capacity", Self::Capacity),
            ("conductivity", Self::Conductivity),
            ("stability", Self::Stability),
            ("deep_reserve", Self::DeepReserve),
            ("ambient_current", Self::Ambient),
            ("dross", Self::Dross),
            ("dross_band", Self::DrossBand),
            ("resonance_root", Self::Resonance(0)),
            ("resonance_tide", Self::Resonance(1)),
            ("resonance_ember", Self::Resonance(2)),
            ("resonance_stone", Self::Resonance(3)),
            ("resonance_gale", Self::Resonance(4)),
            ("resonance_echo", Self::Resonance(5)),
            ("potential", Self::Potential),
            ("drift", Self::Drift),
            ("place_id", Self::PlaceId),
            ("place_type", Self::PlaceType),
            ("recovery_potential", Self::Recovery),
        ]
    }
}

impl ArcaneGeography {
    fn diagnostic_value(&self, index: usize, layer: DiagnosticLayer) -> f64 {
        let control = self.controls[index];
        let dynamic = self.dynamic.cells[index];
        match layer {
            DiagnosticLayer::Capacity => f64::from(control.capacity),
            DiagnosticLayer::Conductivity => f64::from(mean_conductivity(control)),
            DiagnosticLayer::Stability => f64::from(control.stability),
            DiagnosticLayer::DeepReserve => dynamic.deep_total() as f64,
            DiagnosticLayer::Ambient => dynamic.ambient_total() as f64,
            DiagnosticLayer::Dross => self.dense_dross_total_at(
                AtlasPos::from_index(index, self.manifest.side).expect("diagnostic index"),
            ) as f64,
            DiagnosticLayer::DrossBand => {
                f64::from(self.dynamic.dross_state.cells[index].band.ordinal())
            }
            DiagnosticLayer::Resonance(slot) => f64::from(dynamic.ambient[slot]),
            DiagnosticLayer::Potential => potential(dynamic.ambient_total(), control.capacity),
            DiagnosticLayer::Drift => {
                let pos =
                    AtlasPos::from_index(index, self.manifest.side).expect("diagnostic index");
                match self.drift_direction(pos) {
                    None => 0.0,
                    Some(Direction4::East) => 1.0,
                    Some(Direction4::North) => 2.0,
                    Some(Direction4::West) => 3.0,
                    Some(Direction4::South) => 4.0,
                }
            }
            DiagnosticLayer::PlaceId => control.site_ref.into(),
            DiagnosticLayer::PlaceType => control.site_ref.checked_sub(1).map_or(0.0, |site| {
                self.catalog.sites[site as usize].kind as u8 as f64 + 1.0
            }),
            DiagnosticLayer::Recovery => f64::from(control.recovery_potential),
        }
    }

    fn equal_area_diagnostic_value(
        &self,
        atlas: &PlanetAtlas,
        index: usize,
        layer: DiagnosticLayer,
    ) -> f64 {
        let value = self.diagnostic_value(index, layer);
        if !matches!(
            layer,
            DiagnosticLayer::Capacity
                | DiagnosticLayer::DeepReserve
                | DiagnosticLayer::Ambient
                | DiagnosticLayer::Dross
                | DiagnosticLayer::Resonance(_)
        ) {
            return value;
        }
        // These are extensive per-cell quantities. Display their density on
        // an equal-area map; otherwise smaller cubed-sphere edge cells look
        // like false magical seams even though their per-area value agrees.
        let mean_area = 4.0 * std::f64::consts::PI * atlas.manifest.planet_radius.powi(2)
            / atlas.genesis.geometry.len() as f64;
        let cell_area = f64::from(atlas.genesis.geometry.values()[index].physical_area);
        value * mean_area / cell_area.max(f64::EPSILON)
    }

    pub fn export_diagnostics(
        &self,
        atlas: &PlanetAtlas,
        output: &Path,
    ) -> Result<ArcaneGeographyExportReport, ArcaneGeographyError> {
        let started = Instant::now();
        self.validate(atlas)?;
        fs::create_dir_all(output)?;
        let maps_dir = output.join("maps");
        fs::create_dir_all(&maps_dir)?;
        let mut maps = Vec::new();
        for (name, layer) in DiagnosticLayer::all() {
            export_diagnostic_map(self, atlas, layer, &maps_dir.join(format!("{name}.png")))?;
            export_equal_area_map(
                self,
                atlas,
                layer,
                &maps_dir.join(format!("{name}-equal-area.png")),
            )?;
            maps.push(format!("maps/{name}.png"));
            maps.push(format!("maps/{name}-equal-area.png"));
        }
        export_census(self, atlas, &output.join("census.csv"))?;
        export_cross_sections(self, atlas, &output.join("cross-sections.csv"))?;
        export_connectivity(self, atlas, &output.join("connectivity.csv"))?;
        export_histograms(self, atlas, &output.join("histograms.csv"))?;
        let audit = self.audit()?;
        crate::persist::atomic_write(
            &output.join("arcane-geography-audit.txt"),
            audit.render().as_bytes(),
            false,
        )?;
        let catalog = toml::to_string_pretty(&self.catalog)
            .map_err(|error| ArcaneGeographyError::Corrupt(error.to_string()))?;
        crate::persist::atomic_write(&output.join("site-catalog.toml"), catalog.as_bytes(), false)?;
        let report = ArcaneGeographyExportReport {
            validation: "passed".into(),
            schema_version: self.manifest.schema_version,
            algorithm_version: self.manifest.algorithm_version,
            dynamic_version: self.manifest.dynamic_version,
            cell_count: self.controls.len(),
            site_count: self.catalog.sites.len(),
            genesis_total: self.manifest.genesis_current,
            immutable_bytes: self.manifest.immutable_bytes,
            dynamic_bytes: self.manifest.dynamic_bytes,
            site_catalog_bytes: self.manifest.site_catalog_bytes,
            estimated_loaded_bytes: (self.controls.len()
                * (std::mem::size_of::<ArcaneControlCell>()
                    + std::mem::size_of::<ArcaneDynamicCell>()))
                as u64,
            export_elapsed_micros: started.elapsed().as_micros().try_into().unwrap_or(u64::MAX),
            maps,
        };
        let report_text = toml::to_string_pretty(&report)
            .map_err(|error| ArcaneGeographyError::Corrupt(error.to_string()))?;
        crate::persist::atomic_write(
            &output.join("validation-report.toml"),
            report_text.as_bytes(),
            false,
        )?;
        Ok(report)
    }

    pub fn export_dross_diagnostics(
        &self,
        atlas: &PlanetAtlas,
        output: &Path,
    ) -> Result<Vec<String>, ArcaneGeographyError> {
        self.validate(atlas)?;
        fs::create_dir_all(output)?;
        let mut written = Vec::new();
        for (name, layer) in [
            ("dross", DiagnosticLayer::Dross),
            ("dross_band", DiagnosticLayer::DrossBand),
        ] {
            let cube = output.join(format!("{name}.png"));
            let equal_area = output.join(format!("{name}-equal-area.png"));
            export_diagnostic_map(self, atlas, layer, &cube)?;
            export_equal_area_map(self, atlas, layer, &equal_area)?;
            written.push(cube.display().to_string());
            written.push(equal_area.display().to_string());
        }
        Ok(written)
    }

    /// Explicit finite-world retrogen. Existing sites and balances remain;
    /// eligible untouched cells receive bounded new modifiers/sites, followed
    /// by conservative equilibration. Removed providers are never erased.
    pub fn apply_retrogen(
        &mut self,
        atlas: &PlanetAtlas,
        registry: &Registry,
    ) -> Result<usize, ArcaneGeographyError> {
        self.transport = None;
        let known = self
            .catalog
            .sites
            .iter()
            .map(|site| site.predicate_id.as_str())
            .collect::<BTreeSet<_>>();
        let rules = registry
            .arcane_sites
            .iter()
            .filter(|rule| !known.contains(rule.id.as_str()))
            .filter(|rule| {
                matches!(
                    rule.retrogen,
                    crate::registry::RetrogenPolicy::UntouchedHostOnly
                        | crate::registry::RetrogenPolicy::SecondaryRecovery
                )
            })
            .cloned()
            .collect::<Vec<_>>();
        drop(known);
        if rules.is_empty() {
            return Ok(0);
        }
        let before = self.audit()?.resonance_totals;
        let mut added = 0usize;
        for rule in rules {
            let before_rule = added;
            for index in 0..self.controls.len() {
                let pos = AtlasPos::from_index(index, atlas.side()).expect("retrogen index");
                if atlas.history.touched_cells.contains(&pos)
                    || !rule_matches(&rule, atlas, index)
                    || cell_hash(atlas.manifest.seed, index, stable_hash(rule.id.as_bytes()))
                        % 1_000_000
                        >= u64::from(rule.rarity_per_million)
                {
                    continue;
                }
                let factor = u32::from(rule.capacity_factor_permille.clamp(250, 4_000));
                self.controls[index].capacity = ((u32::from(self.controls[index].capacity)
                    * factor)
                    / 1_000)
                    .clamp(256, 60_000) as u16;
                let mut weights = self.controls[index].baseline_resonance.map(u32::from);
                for (slot, add) in rule.base_resonance_bias.into_iter().enumerate() {
                    weights[slot] = weights[slot].saturating_add(u32::from(add) * 8);
                }
                self.controls[index].baseline_resonance = normalized_six(weights);
                add_site(
                    &mut self.catalog.sites,
                    &mut self.controls,
                    atlas,
                    SiteSpec {
                        kind: ArcanePlaceType::Modded,
                        index,
                        radius_cells: rule.radius_cells,
                        extent_cells: u32::from(rule.radius_cells)
                            .saturating_mul(8)
                            .saturating_add(1),
                        confidence: 700,
                        network_id: 0,
                        provider: &rule.provider,
                        predicate_id: &rule.id,
                    },
                );
                added += 1;
            }
            let rule_added = added - before_rule;
            self.manifest.retrogen_history.push(format!(
                "provider={} predicate={} policy={:?} added={rule_added}",
                rule.provider, rule.id, rule.retrogen
            ));
        }
        self.advance_steps_exact(8)?;
        if self.audit()?.resonance_totals != before {
            return Err(ArcaneGeographyError::Corrupt(
                "retrogen changed the finite Current total".into(),
            ));
        }
        self.manifest.content_hash = registry.content_hash;
        Ok(added)
    }

    pub fn save_retrogen(&mut self, world_dir: &Path) -> Result<(), ArcaneGeographyError> {
        self.transport = None;
        let planet_dir = Self::planet_dir(world_dir);
        let immutable = encode_container(IMMUTABLE_MAGIC, &self.controls)?;
        let dynamic = encode_container(DYNAMIC_MAGIC, &self.dynamic)?;
        let catalog = toml::to_string_pretty(&self.catalog)
            .map_err(|error| ArcaneGeographyError::Corrupt(error.to_string()))?;
        check_size("immutable geography", immutable.len(), MAX_IMMUTABLE_BYTES)?;
        check_size("dynamic geography", dynamic.len(), MAX_DYNAMIC_BYTES)?;
        check_size("site catalog", catalog.len(), MAX_CATALOG_BYTES)?;
        self.manifest.immutable_checksum = stable_hash(&immutable);
        self.manifest.dynamic_checksum = stable_hash(&dynamic);
        self.manifest.site_catalog_checksum = stable_hash(catalog.as_bytes());
        self.manifest.immutable_bytes = immutable.len() as u64;
        self.manifest.dynamic_bytes = dynamic.len() as u64;
        self.manifest.site_catalog_bytes = catalog.len() as u64;
        for (source, backup, limit) in [
            (IMMUTABLE_FILE, IMMUTABLE_BACKUP_FILE, MAX_IMMUTABLE_BYTES),
            (DYNAMIC_FILE, DYNAMIC_BACKUP_FILE, MAX_DYNAMIC_BYTES),
            (CATALOG_FILE, CATALOG_BACKUP_FILE, MAX_CATALOG_BYTES),
            (MANIFEST_FILE, MANIFEST_BACKUP_FILE, MAX_MANIFEST_BYTES),
        ] {
            if let Ok(previous) = read_bounded(&planet_dir.join(source), limit) {
                crate::persist::atomic_write(&planet_dir.join(backup), &previous, false)?;
            }
        }
        crate::persist::atomic_write(&planet_dir.join(IMMUTABLE_FILE), &immutable, false)?;
        crate::persist::atomic_write(&planet_dir.join(DYNAMIC_FILE), &dynamic, false)?;
        crate::persist::atomic_write(&planet_dir.join(CATALOG_FILE), catalog.as_bytes(), false)?;
        write_manifest(&planet_dir, &self.manifest)?;
        crate::planet_atlas::update_arcane_geography_manifest(
            world_dir,
            &crate::planet_atlas::ArcaneGeographyManifestCheckpoint {
                schema_version: self.manifest.schema_version,
                algorithm_version: self.manifest.algorithm_version,
                dynamic_version: self.manifest.dynamic_version,
                genesis_total: self.manifest.genesis_current,
                immutable_checksum: self.manifest.immutable_checksum,
                dynamic_checksum: self.manifest.dynamic_checksum,
                site_catalog_checksum: self.manifest.site_catalog_checksum,
                last_authoritative_time: self.manifest.last_authoritative_time,
            },
        )?;
        Ok(())
    }
}

fn layer_colors(values: &[f64]) -> (f64, f64) {
    values.iter().fold(
        (f64::INFINITY, f64::NEG_INFINITY),
        |(minimum, maximum), value| (minimum.min(*value), maximum.max(*value)),
    )
}

fn scalar_color(value: f64, minimum: f64, maximum: f64) -> [u8; 3] {
    let t = if maximum > minimum {
        ((value - minimum) / (maximum - minimum)).clamp(0.0, 1.0)
    } else {
        0.5
    };
    if t < 0.33 {
        let q = t / 0.33;
        [
            (8.0 + 20.0 * q) as u8,
            (18.0 + 170.0 * q) as u8,
            (60.0 + 150.0 * q) as u8,
        ]
    } else if t < 0.66 {
        let q = (t - 0.33) / 0.33;
        [
            (28.0 + 205.0 * q) as u8,
            (188.0 + 47.0 * q) as u8,
            (210.0 - 150.0 * q) as u8,
        ]
    } else {
        let q = (t - 0.66) / 0.34;
        [255, (235.0 - 190.0 * q) as u8, (60.0 - 35.0 * q) as u8]
    }
}

fn export_diagnostic_map(
    geography: &ArcaneGeography,
    atlas: &PlanetAtlas,
    layer: DiagnosticLayer,
    path: &Path,
) -> Result<(), ArcaneGeographyError> {
    let side = u32::from(atlas.side());
    let width = side * 3;
    let height = side * 2;
    let values = (0..geography.controls.len())
        .map(|index| geography.diagnostic_value(index, layer))
        .collect::<Vec<_>>();
    let (minimum, maximum) = layer_colors(&values);
    let mut pixels = vec![0u8; width as usize * height as usize * 3];
    for (index, value) in values.into_iter().enumerate() {
        let pos = AtlasPos::from_index(index, atlas.side()).expect("map index");
        let face = pos.face.index() as u32;
        let x = (face % 3) * side + u32::from(pos.u);
        let y = (face / 3) * side + u32::from(pos.v);
        let target = (y * width + x) as usize * 3;
        pixels[target..target + 3].copy_from_slice(&scalar_color(value, minimum, maximum));
    }
    write_png(path, width, height, &pixels)
}

fn export_equal_area_map(
    geography: &ArcaneGeography,
    atlas: &PlanetAtlas,
    layer: DiagnosticLayer,
    path: &Path,
) -> Result<(), ArcaneGeographyError> {
    let width = u32::from(atlas.side()) * 2;
    let height = u32::from(atlas.side());
    let values = (0..geography.controls.len())
        .map(|index| geography.equal_area_diagnostic_value(atlas, index, layer))
        .collect::<Vec<_>>();
    let (minimum, maximum) = layer_colors(&values);
    let mut sums = vec![[0.0f64; 3]; width as usize * height as usize];
    let mut area = vec![0.0f64; sums.len()];
    for (index, value) in values.into_iter().enumerate() {
        let unit = atlas.genesis.geometry.values()[index].unit_direction;
        let longitude = f64::from(unit[2]).atan2(f64::from(unit[0]));
        let x = (((longitude / std::f64::consts::TAU) + 0.5) * f64::from(width))
            .floor()
            .clamp(0.0, f64::from(width - 1)) as u32;
        // Lambert cylindrical equal-area projection: y is linear in sin(lat).
        let y = ((1.0 - (f64::from(unit[1]) * 0.5 + 0.5)) * f64::from(height))
            .floor()
            .clamp(0.0, f64::from(height - 1)) as u32;
        let pixel = (y * width + x) as usize;
        let color = scalar_color(value, minimum, maximum);
        let cell_area = f64::from(atlas.genesis.geometry.values()[index].physical_area);
        for channel in 0..3 {
            sums[pixel][channel] += f64::from(color[channel]) * cell_area;
        }
        area[pixel] += cell_area;
    }
    fill_projection_holes(&mut sums, &mut area, width, height)?;
    let mut pixels = vec![0u8; sums.len() * 3];
    for index in 0..sums.len() {
        if area[index] > 0.0 {
            for channel in 0..3 {
                pixels[index * 3 + channel] = (sums[index][channel] / area[index]) as u8;
            }
        }
    }
    write_png(path, width, height, &pixels)
}

/// Cell centers do not form a regular longitude/sine-latitude lattice. Even
/// when the source atlas is denser than the exported projection, a handful of
/// bins (especially polar ones) can receive no center and would otherwise be
/// rendered as false black voids. Fill only those missing bins from the
/// nearest populated projection neighborhood, wrapping at the date line.
fn fill_projection_holes(
    sums: &mut [[f64; 3]],
    area: &mut [f64],
    width: u32,
    height: u32,
) -> Result<(), ArcaneGeographyError> {
    let expected = width as usize * height as usize;
    if width == 0 || height == 0 || sums.len() != expected || area.len() != expected {
        return Err(ArcaneGeographyError::Corrupt(
            "invalid equal-area projection dimensions".into(),
        ));
    }
    let mut missing = area.iter().filter(|weight| **weight == 0.0).count();
    while missing > 0 {
        let previous_sums = sums.to_vec();
        let previous_area = area.to_vec();
        let mut filled = 0usize;
        for y in 0..height as i32 {
            for x in 0..width as i32 {
                let target = y as usize * width as usize + x as usize;
                if previous_area[target] > 0.0 {
                    continue;
                }
                let mut color = [0.0; 3];
                let mut neighbors = 0u32;
                for dy in -1..=1 {
                    let neighbor_y = y + dy;
                    if !(0..height as i32).contains(&neighbor_y) {
                        continue;
                    }
                    for dx in -1..=1 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let neighbor_x = (x + dx).rem_euclid(width as i32);
                        let neighbor = neighbor_y as usize * width as usize + neighbor_x as usize;
                        let weight = previous_area[neighbor];
                        if weight == 0.0 {
                            continue;
                        }
                        for channel in 0..3 {
                            color[channel] += previous_sums[neighbor][channel] / weight;
                        }
                        neighbors += 1;
                    }
                }
                if neighbors > 0 {
                    for channel in 0..3 {
                        sums[target][channel] = color[channel] / f64::from(neighbors);
                    }
                    area[target] = 1.0;
                    filled += 1;
                }
            }
        }
        if filled == 0 {
            return Err(ArcaneGeographyError::Corrupt(
                "equal-area projection contains no populated sample".into(),
            ));
        }
        missing -= filled;
    }
    Ok(())
}

fn write_png(
    path: &Path,
    width: u32,
    height: u32,
    pixels: &[u8],
) -> Result<(), ArcaneGeographyError> {
    let file = fs::File::create(path)?;
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|error| ArcaneGeographyError::Corrupt(error.to_string()))?;
    writer
        .write_image_data(pixels)
        .map_err(|error| ArcaneGeographyError::Corrupt(error.to_string()))?;
    Ok(())
}

#[derive(Default)]
struct CensusRow {
    cells: u64,
    area: f64,
    capacity: u64,
    deep: u64,
    ambient: u64,
    dross: u64,
    resonance: [u64; 6],
}

fn export_census(
    geography: &ArcaneGeography,
    atlas: &PlanetAtlas,
    path: &Path,
) -> Result<(), ArcaneGeographyError> {
    let mut rows = BTreeMap::<(String, String), CensusRow>::new();
    for index in 0..geography.controls.len() {
        let terrain = atlas.genesis.terrain.values()[index];
        let climate = atlas.genesis.climate.values()[index];
        let biome = atlas.genesis.biomes.values()[index];
        let hydrology = atlas.genesis.hydrology.values()[index];
        let climate_band = if climate.aridity > 1.5 {
            "dry"
        } else if climate.mean_temperature < -5.0 {
            "polar"
        } else if climate.mean_temperature < 8.0 {
            "cold"
        } else if climate.mean_temperature < 20.0 {
            "temperate"
        } else if climate.mean_precipitation > 1_400.0 {
            "warm_wet"
        } else {
            "warm"
        };
        let dimensions = [
            ("continent", terrain.landmass_id.to_string()),
            ("climate", climate_band.to_string()),
            ("biome", biome.baseline_biome.to_string()),
            ("country", biome.country_id.to_string()),
            ("watershed", hydrology.watershed_id.to_string()),
        ];
        for (dimension, key) in dimensions {
            let row = rows.entry((dimension.into(), key)).or_default();
            add_census_cell(row, geography, atlas, index);
        }
    }
    for site in &geography.catalog.sites {
        let row = rows
            .entry(("site".into(), format!("{}:{}", site.kind.label(), site.id)))
            .or_default();
        add_census_cell(row, geography, atlas, site.center.index(atlas.side()));
    }
    let mut out = String::from(
        "dimension,key,cells,physical_area,capacity,deep,ambient,dross,root,tide,ember,stone,gale,echo\n",
    );
    for ((dimension, key), row) in rows {
        out.push_str(&format!(
            "{dimension},{key},{},{:.6},{},{},{},{},{},{},{},{},{},{}\n",
            row.cells,
            row.area,
            row.capacity,
            row.deep,
            row.ambient,
            row.dross,
            row.resonance[0],
            row.resonance[1],
            row.resonance[2],
            row.resonance[3],
            row.resonance[4],
            row.resonance[5]
        ));
    }
    crate::persist::atomic_write(path, out.as_bytes(), false)?;
    Ok(())
}

fn add_census_cell(
    row: &mut CensusRow,
    geography: &ArcaneGeography,
    atlas: &PlanetAtlas,
    index: usize,
) {
    row.cells += 1;
    row.area += f64::from(atlas.genesis.geometry.values()[index].physical_area);
    row.capacity += u64::from(geography.controls[index].capacity);
    row.deep += geography.dynamic.cells[index].deep_total();
    row.ambient += geography.dynamic.cells[index].ambient_total();
    row.dross += geography.dynamic.cells[index].dross_total();
    for slot in 0..6 {
        row.resonance[slot] += u64::from(geography.dynamic.cells[index].ambient[slot]);
    }
}

fn export_cross_sections(
    geography: &ArcaneGeography,
    atlas: &PlanetAtlas,
    path: &Path,
) -> Result<(), ArcaneGeographyError> {
    let mut representatives = Vec::<(String, AtlasPos)>::new();
    let maximum = |score: fn(&PlanetAtlas, usize) -> f64| {
        (0..geography.controls.len()).max_by(|a, b| score(atlas, *a).total_cmp(&score(atlas, *b)))
    };
    fn mountain(atlas: &PlanetAtlas, index: usize) -> f64 {
        f64::from(atlas.genesis.terrain.values()[index].eroded_elevation)
    }
    fn fault(atlas: &PlanetAtlas, index: usize) -> f64 {
        f64::from(atlas.genesis.tectonics.values()[index].fault_intensity)
    }
    fn river(atlas: &PlanetAtlas, index: usize) -> f64 {
        f64::from(atlas.genesis.hydrology.values()[index].mean_discharge)
    }
    if let Some(index) = maximum(mountain) {
        representatives.push((
            "mountain".into(),
            AtlasPos::from_index(index, atlas.side()).unwrap(),
        ));
    }
    if let Some(index) = maximum(fault) {
        representatives.push((
            "fault".into(),
            AtlasPos::from_index(index, atlas.side()).unwrap(),
        ));
    }
    if let Some(index) = maximum(river) {
        representatives.push((
            "river".into(),
            AtlasPos::from_index(index, atlas.side()).unwrap(),
        ));
    }
    if let Some((index, _)) =
        atlas
            .genesis
            .terrain
            .values()
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                (a.eroded_elevation - crate::chunk::SEA_LEVEL as f32)
                    .abs()
                    .total_cmp(&(b.eroded_elevation - crate::chunk::SEA_LEVEL as f32).abs())
            })
    {
        representatives.push((
            "coast".into(),
            AtlasPos::from_index(index, atlas.side()).unwrap(),
        ));
    }
    for kind in [
        ArcanePlaceType::Well,
        ArcanePlaceType::Still,
        ArcanePlaceType::Confluence,
    ] {
        if let Some(site) = geography
            .catalog
            .sites
            .iter()
            .find(|site| site.kind == kind)
        {
            representatives.push((kind.label().into(), site.center));
        }
    }
    let mut out = String::from(
        "section,offset,face,u,v,elevation,fault,discharge,capacity,conductivity,stability,deep,ambient,dross,potential\n",
    );
    for (label, center) in representatives {
        let mut pos = center;
        for _ in 0..16 {
            pos = pos.step(Direction4::West, atlas.side()).pos;
        }
        for offset in -16..=16 {
            let index = pos.index(atlas.side());
            let control = geography.controls[index];
            let dynamic = geography.dynamic.cells[index];
            out.push_str(&format!(
                "{label},{offset},{:?},{},{},{:.3},{},{:.3},{},{},{},{},{},{},{:.8}\n",
                pos.face,
                pos.u,
                pos.v,
                atlas.genesis.terrain.values()[index].eroded_elevation,
                atlas.genesis.tectonics.values()[index].fault_intensity,
                atlas.genesis.hydrology.values()[index].mean_discharge,
                control.capacity,
                mean_conductivity(control),
                control.stability,
                dynamic.deep_total(),
                dynamic.ambient_total(),
                dynamic.dross_total(),
                potential(dynamic.ambient_total(), control.capacity)
            ));
            pos = pos.step(Direction4::East, atlas.side()).pos;
        }
    }
    crate::persist::atomic_write(path, out.as_bytes(), false)?;
    Ok(())
}

fn export_connectivity(
    geography: &ArcaneGeography,
    atlas: &PlanetAtlas,
    path: &Path,
) -> Result<(), ArcaneGeographyError> {
    let mut out = String::from("network_id,site_id,face,u,v,conductivity,country,landmass\n");
    for site in geography
        .catalog
        .sites
        .iter()
        .filter(|site| site.kind == ArcanePlaceType::Confluence)
    {
        let index = site.center.index(atlas.side());
        out.push_str(&format!(
            "{},{},{:?},{},{},{},{},{}\n",
            site.network_id,
            site.id,
            site.center.face,
            site.center.u,
            site.center.v,
            mean_conductivity(geography.controls[index]),
            site.country_id,
            atlas.genesis.terrain.values()[index].landmass_id
        ));
    }
    crate::persist::atomic_write(path, out.as_bytes(), false)?;
    Ok(())
}

fn export_histograms(
    geography: &ArcaneGeography,
    atlas: &PlanetAtlas,
    path: &Path,
) -> Result<(), ArcaneGeographyError> {
    let mut out = String::from("layer,percentile,value,physical_area_weight_below\n");
    for (name, layer) in DiagnosticLayer::all() {
        if matches!(
            layer,
            DiagnosticLayer::Drift | DiagnosticLayer::PlaceId | DiagnosticLayer::PlaceType
        ) {
            continue;
        }
        let mut values = (0..geography.controls.len())
            .map(|index| {
                (
                    geography.diagnostic_value(index, layer),
                    f64::from(atlas.genesis.geometry.values()[index].physical_area),
                )
            })
            .collect::<Vec<_>>();
        values.sort_by(|a, b| a.0.total_cmp(&b.0));
        let total_area: f64 = values.iter().map(|(_, area)| *area).sum();
        for percentile in [0.0, 1.0, 5.0, 25.0, 50.0, 75.0, 95.0, 99.0, 100.0] {
            let target = total_area * percentile / 100.0;
            let mut cumulative = 0.0;
            let mut selected = values.last().map_or(0.0, |value| value.0);
            for (value, area) in &values {
                cumulative += *area;
                if cumulative >= target {
                    selected = *value;
                    break;
                }
            }
            out.push_str(&format!(
                "{name},{percentile:.1},{selected:.9},{target:.3}\n"
            ));
        }
    }
    crate::persist::atomic_write(path, out.as_bytes(), false)?;
    Ok(())
}

pub fn audit_world(world_dir: &Path) -> Result<(ArcaneGeographyAudit, bool), ArcaneGeographyError> {
    let atlas = PlanetAtlas::load(world_dir)?;
    let geography = ArcaneGeography::load(world_dir, &atlas)?;
    let audit = geography.audit()?;
    let custody_matches = crate::arcane::ArcaneLedger::load(world_dir)
        .map_err(|error| ArcaneGeographyError::Corrupt(error.to_string()))?
        .account(&crate::arcane::ArcaneOwner::Geography)
        .is_some_and(|account| account.current == audit.as_current());
    Ok((audit, custody_matches))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str, seed: u64) -> PathBuf {
        std::env::temp_dir().join(format!(
            "wildforge-arcane-geography-{tag}-{}-{}",
            std::process::id(),
            mix64(seed)
        ))
    }

    fn fixture(seed: u32, side: u16) -> (PlanetAtlas, Registry, ArcaneGeography) {
        let atlas = PlanetAtlas::fixture(seed, side).unwrap();
        let registry = crate::registry::load(Path::new("this-directory-does-not-exist"));
        let geography = ArcaneGeography::generate(
            &atlas,
            &registry,
            &crate::planet_atlas::CancellationToken::default(),
            |_| {},
        )
        .unwrap();
        (atlas, registry, geography)
    }

    #[test]
    fn pre_ecology_dynamic_payload_migrates_without_changing_existing_state() {
        let (_, _, geography) = fixture(0xec0109, 8);
        let legacy = ArcaneDynamicStateBeforeEcology {
            version: geography.dynamic.version,
            completed_steps: geography.dynamic.completed_steps,
            last_authoritative_time: geography.dynamic.last_authoritative_time,
            cells: geography.dynamic.cells.clone(),
            wakes: geography.dynamic.wakes.clone(),
            next_wake_id: geography.dynamic.next_wake_id,
            observations: geography.dynamic.observations.clone(),
        };
        let bytes = encode_container(DYNAMIC_MAGIC, &legacy).unwrap();
        let migrated = decode_dynamic_container(&bytes).unwrap();
        assert_eq!(migrated.version, geography.dynamic.version);
        assert_eq!(migrated.completed_steps, geography.dynamic.completed_steps);
        assert_eq!(migrated.cells, geography.dynamic.cells);
        assert_eq!(migrated.wakes, geography.dynamic.wakes);
        assert_eq!(migrated.observations, geography.dynamic.observations);
        assert_eq!(
            migrated.ecology,
            crate::arcane_ecology::ArcaneEcologyState::default()
        );
    }

    #[test]
    fn genesis_is_byte_identical_and_causal() {
        let (atlas, registry, first) = fixture(0x5eed, 8);
        let second = ArcaneGeography::generate(
            &atlas,
            &registry,
            &crate::planet_atlas::CancellationToken::default(),
            |_| {},
        )
        .unwrap();
        assert_eq!(first.controls, second.controls);
        assert_eq!(first.dynamic, second.dynamic);
        assert_eq!(first.catalog, second.catalog);
        let mut by_water = atlas
            .genesis
            .ground
            .values()
            .iter()
            .enumerate()
            .map(|(index, ground)| {
                let hydrology = atlas.genesis.hydrology.values()[index];
                let wetness = (hydrology.mean_discharge.max(0.0).ln_1p() * 22.0
                    + f32::from(ground.aquifer_permeability) / 2_048.0)
                    .clamp(0.0, 220.0) as u16;
                (wetness, index)
            })
            .collect::<Vec<_>>();
        by_water.sort_unstable();
        let quartile = by_water.len() / 4;
        let dry_tide: u64 = by_water[..quartile]
            .iter()
            .map(|(_, index)| u64::from(first.controls[*index].baseline_resonance[1]))
            .sum();
        let wet_tide: u64 = by_water[by_water.len() - quartile..]
            .iter()
            .map(|(_, index)| u64::from(first.controls[*index].baseline_resonance[1]))
            .sum();
        assert!(wet_tide > dry_tide);
        assert!(first.audit().unwrap().is_balanced());
    }

    #[test]
    fn transport_is_exact_and_slicing_independent() {
        let (_, _, geography) = fixture(77, 8);
        let before = geography.audit().unwrap();
        let mut one = geography.clone();
        let mut sliced = geography;
        one.advance_steps_exact(7).unwrap();
        let target = 7 * ARCANE_GEOGRAPHY_SIMULATION_SECONDS;
        while sliced.dynamic.completed_steps < 7 {
            sliced.advance_toward(target, 13).unwrap();
        }
        assert_eq!(one.dynamic, sliced.dynamic);
        assert_eq!(
            one.audit().unwrap().resonance_totals,
            before.resonance_totals
        );
        assert_eq!(one.audit().unwrap().accounted_total, before.accounted_total);
    }

    #[test]
    fn genesis_is_an_idle_deep_surface_equilibrium_and_catch_up_records_only_completed_time() {
        let (_, _, geography) = fixture(0x51e7, 8);
        let initial_cells = geography.dynamic.cells.clone();
        let mut idle = geography.clone();
        idle.advance_steps_exact(256).unwrap();
        assert_eq!(idle.dynamic.cells, initial_cells);

        let mut catch_up = geography;
        let far_future = 10 * ARCANE_GEOGRAPHY_SIMULATION_SECONDS;
        let budget = catch_up.dynamic.cells.len();
        catch_up.advance_toward(far_future, budget).unwrap();
        catch_up.advance_toward(far_future, budget).unwrap();
        assert_eq!(catch_up.dynamic.completed_steps, 1);
        assert_eq!(
            catch_up.dynamic.last_authoritative_time,
            ARCANE_GEOGRAPHY_SIMULATION_SECONDS
        );
        assert_eq!(
            catch_up.manifest.last_authoritative_time,
            ARCANE_GEOGRAPHY_SIMULATION_SECONDS
        );
        while catch_up.dynamic.completed_steps < 10 {
            catch_up.advance_toward(far_future, budget).unwrap();
        }
        assert_eq!(catch_up.dynamic.last_authoritative_time, far_future);
    }

    #[test]
    fn dross_flows_down_potential_even_when_the_source_has_fewer_units() {
        let (_, _, mut geography) = fixture(0xd2055, 8);
        let side = geography.manifest.side;
        let source_pos = AtlasPos {
            face: Face::PosZ,
            u: 3,
            v: 3,
        };
        let target_pos = source_pos.step(Direction4::East, side).pos;
        let source = source_pos.index(side);
        let target = target_pos.index(side);
        for control in &mut geography.controls {
            control.conductivity = [0; 4];
            control.dross_mobility = 0;
            control.exchange_permille = 0;
        }
        geography.controls[source].capacity = 100;
        geography.controls[target].capacity = 1_000;
        geography.controls[source].conductivity = [1_000; 4];
        geography.controls[target].conductivity = [1_000; 4];
        geography.controls[source].dross_mobility = 255;
        geography.controls[target].dross_mobility = 255;
        for cell in &mut geography.dynamic.cells {
            cell.deep = [0; 6];
            cell.ambient = [0; 6];
            cell.dross = [0; 6];
            cell.transport_remainder = 0;
        }
        geography.dynamic.cells[source].dross[0] = 20;
        geography.dynamic.cells[target].dross[0] = 30;
        geography.manifest.genesis_current = 50;
        geography.manifest.genesis_resonance = geography.audit().unwrap().resonance_totals;
        geography.advance_steps_exact(10).unwrap();
        assert!(geography.dynamic.cells[source].dross[0] < 20);
        assert!(geography.dynamic.cells[target].dross[0] > 30);
    }

    #[test]
    fn seams_have_equal_neighbors_and_pulses_do_not_leak() {
        let (_, _, mut geography) = fixture(91, 8);
        let side = geography.manifest.side;
        let edge = AtlasPos {
            face: Face::PosZ,
            u: side - 1,
            v: side / 2,
        };
        let across = edge.step(Direction4::East, side).pos;
        let source = edge.index(side);
        let target = across.index(side);
        geography.dynamic.cells[source].ambient[0] -= 50;
        geography.dynamic.cells[target].ambient[0] += 50;
        geography.manifest.genesis_resonance = geography.audit().unwrap().resonance_totals;
        let before = geography.audit().unwrap();
        geography.advance_steps_exact(12).unwrap();
        let after = geography.audit().unwrap();
        assert_eq!(before.resonance_totals, after.resonance_totals);
        assert_eq!(before.accounted_total, after.accounted_total);
        assert_ne!(
            geography.dynamic.cells[source],
            geography.dynamic.cells[target]
        );
    }

    #[test]
    fn save_load_preserves_sites_and_state() {
        let (atlas, _, mut geography) = fixture(123, 8);
        geography.advance_steps_exact(2).unwrap();
        let root = std::env::temp_dir().join(format!(
            "wildforge-arcane-geography-{}-{}",
            std::process::id(),
            mix64(123)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        atlas.write_new(&root).unwrap();
        geography.write_new(&root).unwrap();
        let loaded = ArcaneGeography::load(&root, &atlas).unwrap();
        assert_eq!(loaded.controls, geography.controls);
        assert_eq!(loaded.dynamic, geography.dynamic);
        assert_eq!(loaded.catalog, geography.catalog);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_saved_atlas_without_geography_fails_instead_of_minting_a_second_genesis() {
        let (atlas, _, _) = fixture(0x5155, 8);
        let root = temp_root("missing-geography", 0x5155);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        atlas.write_new(&root).unwrap();
        let error = ArcaneGeography::load(&root, &atlas).unwrap_err();
        assert!(matches!(error, ArcaneGeographyError::Corrupt(_)));
        assert!(
            !ArcaneGeography::planet_dir(&root)
                .join(MANIFEST_FILE)
                .exists()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn every_directed_face_edge_is_reciprocal_and_uniform_fields_stay_idle() {
        let (atlas, _, mut geography) = fixture(0xface, 8);
        let side = geography.manifest.side;
        for index in 0..geography.controls.len() {
            let pos = AtlasPos::from_index(index, side).unwrap();
            for direction in [
                Direction4::East,
                Direction4::North,
                Direction4::West,
                Direction4::South,
            ] {
                let step = pos.step(direction, side);
                assert!(step.pos.neighbors4(side).contains(&pos));
                let forward = edge_geometry_factor(&atlas, index, direction);
                let reverse =
                    edge_geometry_factor(&atlas, step.pos.index(side), step.direction.opposite());
                assert!((forward - reverse).abs() < 0.000_001);
            }
        }
        for control in &mut geography.controls {
            control.capacity = 4_096;
            control.deep_capacity = 4_096;
            control.exchange_permille = 0;
            control.conductivity = [1_000; 4];
        }
        for cell in &mut geography.dynamic.cells {
            cell.deep = [0; 6];
            cell.ambient = [597, 597, 597, 597, 597, 599];
            cell.dross = [0; 6];
            cell.transport_remainder = 0;
            cell.historical_min_ambient = 3_584;
            cell.historical_max_ambient = 3_584;
        }
        geography.manifest.genesis_resonance = geography.audit().unwrap().resonance_totals;
        let before = geography.dynamic.clone();
        geography.advance_steps_exact(20).unwrap();
        assert_eq!(geography.dynamic.cells, before.cells);
    }

    #[test]
    fn long_randomized_transport_and_deep_exchange_never_leak_or_change_band_identity() {
        let (_, _, mut geography) = fixture(0x10_5eed, 8);
        let initial = geography.audit().unwrap();
        let mut random = 0x1234_5678u32;
        for _ in 0..128 {
            random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let from = random as usize % geography.dynamic.cells.len();
            random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let to = random as usize % geography.dynamic.cells.len();
            let slot = random as usize % 6;
            if geography.dynamic.cells[from].ambient[slot] != 0 {
                geography.dynamic.cells[from].ambient[slot] -= 1;
                geography.dynamic.cells[to].deep[slot] += 1;
            }
        }
        assert_eq!(
            geography.audit().unwrap().resonance_totals,
            initial.resonance_totals
        );
        geography.advance_steps_exact(1_000).unwrap();
        let after = geography.audit().unwrap();
        assert_eq!(after.resonance_totals, initial.resonance_totals);
        assert_eq!(after.accounted_total, initial.accounted_total);
    }

    #[test]
    fn wells_stills_wakes_scars_and_observations_obey_information_and_accounting_rules() {
        let (atlas, _, mut geography) = fixture(0x51_7e5, 8);
        let well = geography
            .catalog
            .sites
            .iter()
            .find(|site| site.kind == ArcanePlaceType::Well)
            .unwrap()
            .center;
        let still = geography
            .catalog
            .sites
            .iter()
            .find(|site| site.kind == ArcanePlaceType::Still)
            .unwrap()
            .center;
        for control in &mut geography.controls {
            control.conductivity = [0; 4];
        }
        let well_index = well.index(atlas.side());
        geography.controls[well_index].exchange_permille = 1_000;
        geography.controls[well_index].deep_capacity = geography.controls[well_index].capacity;
        for slot in 0..6 {
            let moved = geography.dynamic.cells[well_index].ambient[slot];
            geography.dynamic.cells[well_index].ambient[slot] = 0;
            geography.dynamic.cells[well_index].deep[slot] += moved;
        }
        let deep_before = geography.dynamic.cells[well_index].deep_total();
        geography.manifest.genesis_resonance = geography.audit().unwrap().resonance_totals;
        geography.advance_steps_exact(4).unwrap();
        assert!(geography.dynamic.cells[well_index].deep_total() < deep_before);

        let before = geography.audit().unwrap();
        assert_eq!(geography.foul_ambient(still, 20).unwrap(), 20);
        let fouled = geography.audit().unwrap();
        assert_eq!(fouled.accounted_total, before.accounted_total);
        assert_eq!(fouled.dross_total, before.dross_total + 20);
        let path = vec![still, still.step(Direction4::East, atlas.side()).pos];
        let wake_id = geography.begin_wake(path, 30, 10, 10).unwrap();
        assert!(
            geography
                .dynamic
                .wakes
                .iter()
                .any(|wake| wake.id == wake_id)
        );
        let wake_site = geography.dynamic.wakes[0].site_id;
        assert_eq!(
            geography.audit().unwrap().accounted_total,
            before.accounted_total
        );
        geography.advance_steps_exact(20).unwrap();
        assert!(geography.dynamic.wakes.is_empty());
        assert!(
            !geography
                .catalog
                .sites
                .iter()
                .find(|site| site.id == wake_site)
                .unwrap()
                .active
        );
        assert_eq!(
            geography.audit().unwrap().accounted_total,
            before.accounted_total
        );

        let scar = geography.mark_scar(&atlas, still);
        assert_eq!(scar, geography.mark_scar(&atlas, still));
        let observation = geography.record_observation([7; 16], still, false).unwrap();
        assert!(observation.boundary_uncertainty_cells >= 4);
        assert_eq!(
            observation.approximate_center.u % observation.boundary_uncertainty_cells,
            0
        );
        let improved = geography.record_observation([7; 16], still, true).unwrap();
        assert!(improved.boundary_uncertainty_cells < observation.boundary_uncertainty_cells);
        assert_eq!(geography.dynamic.observations[&[7; 16]].len(), 1);
    }

    #[test]
    fn unaided_sensory_cues_are_qualitative_and_resonance_specific() {
        let survey = ArcaneSurvey {
            strength: SurveyStrength::Strong,
            condition: SurveyCondition::Fouled,
            dominant_resonances: vec![TIDE],
            drift: Some(Direction4::East),
            uncertainty: 65,
            nearby_place: Some((u64::MAX, ArcanePlaceType::Well, "Hidden Well".into())),
        };
        let cue = survey.sensory_cue();
        assert!(cue.contains("moisture beads"));
        assert!(cue.contains("sour metallic haze"));
        assert!(!cue.chars().any(|character| character.is_ascii_digit()));
        assert!(!cue.contains("Hidden Well"));
        assert!(!cue.contains("east"));
        let remote = coarse_sensory_cue([1, 0], 2);
        assert_eq!(
            remote,
            "A faint static catches on stone; the signs hold a steady rhythm; moisture beads in repeating lines."
        );
        assert!(!remote.chars().any(|character| character.is_ascii_digit()));
        assert!(coarse_sensory_cue([4, 4], u8::MAX).contains("no clear character"));
    }

    #[test]
    fn guest_resonance_category_matches_the_unaided_authoritative_survey() {
        let (atlas, _, geography) = fixture(0x5e115e, 8);
        for index in 0..geography.dynamic.cells.len() {
            let pos = AtlasPos::from_index(index, atlas.side()).unwrap();
            let surveyed = geography
                .survey(pos, false)
                .dominant_resonances
                .first()
                .and_then(|name| BASE_RESONANCES.iter().position(|base| base == name))
                .map_or(0, |slot| slot as u8 + 1);
            assert_eq!(
                geography.local_dominant_resonance(pos),
                surveyed,
                "remote and solo sensory character diverged at {pos:?}"
            );
        }
    }

    #[test]
    fn dynamic_checkpoint_recovers_the_last_atomic_bundle() {
        let (atlas, _, mut geography) = fixture(0xc4a5, 8);
        let root = temp_root("recovery", 0xc4a5);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        atlas.write_new(&root).unwrap();
        geography.write_new(&root).unwrap();
        let previous = geography.dynamic.clone();
        geography.advance_steps_exact(3).unwrap();
        geography.save_dynamic(&root).unwrap();
        fs::write(
            ArcaneGeography::planet_dir(&root).join(DYNAMIC_FILE),
            b"confirmed truncated checkpoint",
        )
        .unwrap();
        let recovered = ArcaneGeography::load(&root, &atlas).unwrap();
        assert_eq!(recovered.dynamic, previous);
        assert!(recovered.audit().unwrap().is_balanced());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn bounded_mod_retrogen_skips_touched_cells_and_preserves_current() {
        let (mut atlas, mut registry, mut geography) = fixture(0x4d0d, 8);
        let touched = AtlasPos::from_index(0, atlas.side()).unwrap();
        atlas.history.touched_cells.insert(touched);
        registry.content_hash ^= 0x55aa;
        registry.arcane_sites.push(ArcaneSiteRule {
            id: "fixture:singing_fault".into(),
            provider: "fixture".into(),
            requires: Vec::new(),
            capacity_factor_permille: 1_150,
            base_resonance_bias: [0, 0, 0, 2, 0, 1],
            rarity_per_million: 1_000_000,
            radius_cells: 2,
            retrogen: crate::registry::RetrogenPolicy::UntouchedHostOnly,
        });
        let touched_before = geography.controls[0];
        let eligible_before = geography.controls[1];
        let current_before = geography.audit().unwrap().resonance_totals;
        let added = geography.apply_retrogen(&atlas, &registry).unwrap();
        assert_eq!(added, geography.controls.len() - 1);
        assert_eq!(geography.controls[0], touched_before);
        assert_ne!(
            geography.controls[1].baseline_resonance,
            eligible_before.baseline_resonance
        );
        assert_eq!(geography.audit().unwrap().resonance_totals, current_before);
        assert!(
            geography
                .manifest
                .retrogen_history
                .iter()
                .any(|record| record.contains("fixture:singing_fault"))
        );
    }

    #[test]
    fn exports_cover_every_operator_layer_and_reconcile() {
        let (atlas, _, geography) = fixture(0xe7a0, 8);
        let root = temp_root("export", 0xe7a0);
        let _ = fs::remove_dir_all(&root);
        let report = geography.export_diagnostics(&atlas, &root).unwrap();
        assert_eq!(report.maps.len(), DiagnosticLayer::all().len() * 2);
        for required in [
            "maps/capacity.png",
            "maps/capacity-equal-area.png",
            "maps/resonance_echo.png",
            "census.csv",
            "cross-sections.csv",
            "connectivity.csv",
            "histograms.csv",
            "arcane-geography-audit.txt",
            "validation-report.toml",
        ] {
            assert!(root.join(required).is_file(), "missing {required}");
        }
        assert!(
            fs::read_to_string(root.join("census.csv"))
                .unwrap()
                .contains("watershed")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn equal_area_projection_fills_sparse_bins_and_wraps_the_date_line() {
        let width = 7;
        let height = 5;
        let mut sums = vec![[0.0; 3]; width * height];
        let mut area = vec![0.0; width * height];
        for (index, color) in [(0, [30.0, 60.0, 90.0]), (width * height - 1, [90.0; 3])] {
            sums[index] = color;
            area[index] = 1.0;
        }
        fill_projection_holes(&mut sums, &mut area, width as u32, height as u32).unwrap();
        assert!(area.iter().all(|weight| *weight > 0.0));
        assert!(sums.iter().flatten().all(|channel| channel.is_finite()));
        assert!(
            sums[width - 1][0] > 0.0,
            "longitude must wrap at the date line"
        );
    }

    #[test]
    fn qualification_seed_suite_has_no_face_or_corner_privilege() {
        let mut face_density = [[0.0f64; 6]; 4];
        let mut zone_capacity = [0.0f64; 4];
        let mut zone_area = [0.0f64; 4];
        for (seed_index, seed) in [3, 17, 41, 89].into_iter().enumerate() {
            let (atlas, _, geography) = fixture(seed, 16);
            geography.validate(&atlas).unwrap();
            let mut face_area = [0.0f64; 6];
            for index in 0..geography.controls.len() {
                let pos = AtlasPos::from_index(index, atlas.side()).unwrap();
                let area = f64::from(atlas.genesis.geometry.values()[index].physical_area);
                let capacity = f64::from(geography.controls[index].capacity);
                face_density[seed_index][pos.face.index()] += capacity;
                face_area[pos.face.index()] += area;
                let last = atlas.side() - 1;
                let near_u_edge = pos.u.min(last - pos.u) < 2;
                let near_v_edge = pos.v.min(last - pos.v) < 2;
                let middle = atlas.side() / 2;
                let center = pos.u.abs_diff(middle) < 2 && pos.v.abs_diff(middle) < 2;
                let zone = if near_u_edge && near_v_edge {
                    0 // corner neighborhood
                } else if near_u_edge || near_v_edge {
                    1 // edge neighborhood
                } else if center {
                    2 // face-center neighborhood
                } else {
                    3 // ordinary face interior
                };
                zone_capacity[zone] += capacity;
                zone_area[zone] += area;
            }
            for face in 0..6 {
                face_density[seed_index][face] /= face_area[face];
            }
            let audit = geography.audit().unwrap();
            assert!(audit.site_counts[&ArcanePlaceType::Well] >= 2);
            assert!(audit.site_counts[&ArcanePlaceType::Still] >= 2);
            assert!(audit.site_counts[&ArcanePlaceType::Confluence] >= 3);
        }
        let mut means = [0.0; 6];
        for face in 0..6 {
            means[face] = face_density.iter().map(|suite| suite[face]).sum::<f64>() / 4.0;
        }
        let min = means.into_iter().fold(f64::INFINITY, f64::min);
        let max = means.into_iter().fold(f64::NEG_INFINITY, f64::max);
        assert!(max / min < 1.2, "face capacity-density ratio {min}..{max}");
        let zone_density =
            std::array::from_fn::<_, 4, _>(|zone| zone_capacity[zone] / zone_area[zone]);
        let min = zone_density.into_iter().fold(f64::INFINITY, f64::min);
        let max = zone_density.into_iter().fold(f64::NEG_INFINITY, f64::max);
        assert!(
            max / min < 1.25,
            "corner/edge/center/interior capacity-density ratio {min}..{max}: {zone_density:?}"
        );
    }

    #[test]
    #[ignore = "operator probe for WILDFORGE_PROBE_WORLD production save"]
    fn production_transport_memory_and_server_budget_probe() {
        let root = std::env::var("WILDFORGE_PROBE_WORLD")
            .map(PathBuf::from)
            .expect("set WILDFORGE_PROBE_WORLD to a qualified production save");
        let atlas = PlanetAtlas::load(&root).unwrap();
        let mut geography = ArcaneGeography::load(&root, &atlas).unwrap();
        let before = geography.audit().unwrap();
        let target = (geography.dynamic.completed_steps + 1) * ARCANE_GEOGRAPHY_SIMULATION_SECONDS;
        let started = Instant::now();
        let mut worst_slice = std::time::Duration::ZERO;
        while geography.dynamic.completed_steps * ARCANE_GEOGRAPHY_SIMULATION_SECONDS < target {
            let slice = Instant::now();
            geography.advance_toward(target, 4_096).unwrap();
            worst_slice = worst_slice.max(slice.elapsed());
        }
        let elapsed = started.elapsed();
        assert_eq!(
            geography.audit().unwrap().resonance_totals,
            before.resonance_totals
        );
        assert!(
            worst_slice.as_millis() < 25,
            "worst server slice {worst_slice:?}"
        );
        assert!(elapsed.as_secs() < 20, "whole pass {elapsed:?}");
        println!(
            "arcane geography production profile: cells={} loaded={} MiB pass={elapsed:?} worst_slice={worst_slice:?}",
            geography.controls.len(),
            geography.controls.len()
                * (std::mem::size_of::<ArcaneControlCell>()
                    + std::mem::size_of::<ArcaneDynamicCell>())
                / (1024 * 1024)
        );
    }
}
