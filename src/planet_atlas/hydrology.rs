//! Whole-planet static hydrology.
//!
//! This module turns climate-normal water and geological relief into one
//! acyclic drainage graph before any voxel chunk exists.  The dense cell
//! layer owns routing and materialization constraints; this sparse model owns
//! named rivers, reservoir curves, lake budgets, and ocean connections.

use std::cmp::Ordering as CmpOrdering;
use std::collections::{BTreeMap, BinaryHeap, VecDeque};

use serde::{Deserialize, Serialize};

use super::*;
use crate::chunk::SEA_LEVEL;
use crate::planet::{PLANET_RADIUS, geodesic_distance, surface_to_unit};

pub const HYDROLOGY_SCHEMA_VERSION: u32 = 1;
const MAX_EROSION_ITERATIONS: u8 = 5;
const EROSION_CONVERGENCE_BLOCKS: f32 = 0.035;

pub const HYDRO_RIVER: u16 = 1 << 0;
pub const HYDRO_PERENNIAL: u16 = 1 << 1;
pub const HYDRO_INTERMITTENT: u16 = 1 << 2;
pub const HYDRO_FLOODPLAIN: u16 = 1 << 3;
pub const HYDRO_WETLAND: u16 = 1 << 4;
pub const HYDRO_DELTA: u16 = 1 << 5;
pub const HYDRO_ESTUARY: u16 = 1 << 6;
pub const HYDRO_WATERFALL: u16 = 1 << 7;
pub const HYDRO_TERMINAL: u16 = 1 << 8;
pub const HYDRO_PLAYA: u16 = 1 << 9;
pub const HYDRO_KARST_LOSS: u16 = 1 << 10;
pub const HYDRO_OCEAN: u16 = 1 << 11;
pub const HYDRO_LAKE: u16 = 1 << 12;

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum WaterBodyKind {
    #[default]
    Land = 0,
    Ocean = 1,
    River = 2,
    Lake = 3,
    Playa = 4,
    Delta = 5,
    Estuary = 6,
    Wetland = 7,
}

impl WaterBodyKind {
    pub(super) fn from_u8(value: u8) -> Result<Self, AtlasError> {
        match value {
            0 => Ok(Self::Land),
            1 => Ok(Self::Ocean),
            2 => Ok(Self::River),
            3 => Ok(Self::Lake),
            4 => Ok(Self::Playa),
            5 => Ok(Self::Delta),
            6 => Ok(Self::Estuary),
            7 => Ok(Self::Wetland),
            _ => Err(AtlasError::Corrupt(format!(
                "unknown water-body classification {value}"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LakeClass {
    ThroughFlowFresh,
    TerminalFresh,
    SalineTerminal,
    SeasonalPlaya,
    Rift,
    VolcanicCrater,
    GlacialAlpine,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoragePoint {
    pub elevation: f32,
    pub volume_units: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OceanBasinRecord {
    pub id: u16,
    pub name: String,
    pub cell_count: u32,
    pub area: f64,
    pub baseline_volume_units: u64,
    pub salinity: u8,
    pub connection_basin_id: u16,
    pub connection_sill_elevation: f32,
    pub volume_elevation_curve: Vec<StoragePoint>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LakeRecord {
    pub id: u32,
    pub name: String,
    pub class: LakeClass,
    pub sink: AtlasPos,
    pub outlet: Option<AtlasPos>,
    pub cell_count: u32,
    pub catchment_area: f64,
    pub surface_elevation: f32,
    pub spill_elevation: f32,
    pub baseline_inflow: f64,
    pub baseline_evaporation: f64,
    pub baseline_outflow: f64,
    pub groundwater_exchange_coefficient: f32,
    pub salinity: u8,
    pub seasonal_level_range: f32,
    pub baseline_volume_units: u64,
    pub voxel_volume_residual: i64,
    pub volume_elevation_curve: Vec<StoragePoint>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RiverRecord {
    pub id: u32,
    pub name: String,
    pub watershed_id: u32,
    pub source: AtlasPos,
    pub mouth: AtlasPos,
    pub sink_name: String,
    pub length_blocks: f64,
    pub maximum_discharge: f32,
    pub maximum_width_blocks: f32,
    pub stream_order: u8,
    pub path: Vec<AtlasPos>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WatershedRecord {
    pub id: u32,
    pub name: String,
    pub outlet: AtlasPos,
    pub terminal: bool,
    pub area: f64,
    pub mean_runoff: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HydrologyModel {
    pub schema_version: u32,
    pub sea_level: f32,
    pub dominant_ocean_id: u16,
    pub erosion_iterations: u8,
    pub erosion_max_residual: f32,
    pub baseline_surface_water_units: u128,
    pub voxel_volume_residual: i128,
    pub oceans: Vec<OceanBasinRecord>,
    pub lakes: Vec<LakeRecord>,
    pub rivers: Vec<RiverRecord>,
    pub watersheds: Vec<WatershedRecord>,
}

impl Default for HydrologyModel {
    fn default() -> Self {
        Self {
            schema_version: HYDROLOGY_SCHEMA_VERSION,
            sea_level: SEA_LEVEL as f32,
            dominant_ocean_id: 1,
            erosion_iterations: 0,
            erosion_max_residual: 0.0,
            baseline_surface_water_units: 0,
            voxel_volume_residual: 0,
            oceans: Vec::new(),
            lakes: Vec::new(),
            rivers: Vec::new(),
            watersheds: Vec::new(),
        }
    }
}

pub(super) struct HydrologyOutput {
    pub terrain: AtlasGrid<TerrainCell>,
    pub cells: AtlasGrid<HydrologyCell>,
    pub model: HydrologyModel,
}

#[derive(Clone, Copy, Debug)]
struct FloodEntry {
    elevation: f32,
    index: usize,
}

impl PartialEq for FloodEntry {
    fn eq(&self, other: &Self) -> bool {
        self.elevation.to_bits() == other.elevation.to_bits() && self.index == other.index
    }
}

impl Eq for FloodEntry {}

impl PartialOrd for FloodEntry {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

impl Ord for FloodEntry {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        // Reverse the natural comparison: BinaryHeap then pops the lowest
        // elevation, with the lowest global index breaking flats.
        other
            .elevation
            .total_cmp(&self.elevation)
            .then_with(|| other.index.cmp(&self.index))
    }
}

fn neighbors8_indices(index: usize, side: u16) -> Vec<usize> {
    let pos = AtlasPos::from_index(index, side).expect("atlas index");
    let mut out = Vec::with_capacity(8);
    for neighbor in pos.neighbors8(side) {
        let candidate = neighbor.index(side);
        if candidate != index && !out.contains(&candidate) {
            out.push(candidate);
        }
    }
    out.sort_unstable();
    out
}

fn priority_flood(side: u16, elevations: &[f32]) -> (Vec<f32>, Vec<u32>) {
    let count = elevations.len();
    let mut filled = elevations.to_vec();
    let mut receiver = vec![u32::MAX; count];
    let mut visited = vec![false; count];
    let mut heap = BinaryHeap::new();
    for (index, elevation) in elevations.iter().copied().enumerate() {
        if elevation <= SEA_LEVEL as f32 {
            visited[index] = true;
            heap.push(FloodEntry { elevation, index });
        }
    }
    // Geology validation guarantees ocean, but retaining this deterministic
    // fallback makes tiny synthetic fixtures fail usefully rather than loop.
    if heap.is_empty()
        && let Some((index, elevation)) = elevations
            .iter()
            .copied()
            .enumerate()
            .min_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)))
    {
        visited[index] = true;
        heap.push(FloodEntry { elevation, index });
    }
    while let Some(entry) = heap.pop() {
        for neighbor in neighbors8_indices(entry.index, side) {
            if visited[neighbor] {
                continue;
            }
            visited[neighbor] = true;
            filled[neighbor] = elevations[neighbor].max(entry.elevation);
            receiver[neighbor] = entry.index as u32;
            heap.push(FloodEntry {
                elevation: filled[neighbor],
                index: neighbor,
            });
        }
    }
    (filled, receiver)
}

fn bedrock_resistance(cell: TectonicCell) -> f32 {
    match BedrockFamily::from_id(cell.bedrock_family) {
        BedrockFamily::Shale | BedrockFamily::Evaporite => 0.65,
        BedrockFamily::Sandstone | BedrockFamily::Limestone => 0.82,
        BedrockFamily::MixedBasement | BedrockFamily::Basalt => 1.0,
        BedrockFamily::Granite | BedrockFamily::Marble | BedrockFamily::Slate => 1.25,
        BedrockFamily::Quartzite | BedrockFamily::Ultramafic => 1.48,
    }
}

fn infiltration_fraction(cell: TectonicCell) -> f32 {
    match BedrockFamily::from_id(cell.bedrock_family) {
        BedrockFamily::Limestone => 0.52,
        BedrockFamily::Sandstone => 0.34,
        BedrockFamily::Shale => 0.10,
        BedrockFamily::Evaporite => 0.18,
        BedrockFamily::Granite | BedrockFamily::Quartzite => 0.13,
        BedrockFamily::Basalt | BedrockFamily::Ultramafic => 0.26,
        _ => 0.22,
    }
}

fn normalized_fractions(values: [f64; CLIMATE_SEASONS]) -> [u16; CLIMATE_SEASONS] {
    let total: f64 = values.iter().sum();
    if total <= f64::EPSILON {
        return [16_384, 16_384, 16_384, 16_383];
    }
    let mut out = [0u16; CLIMATE_SEASONS];
    let mut assigned = 0u32;
    for season in 0..CLIMATE_SEASONS - 1 {
        out[season] = ((values[season] / total) * 65_535.0)
            .round()
            .clamp(0.0, 65_535.0) as u16;
        assigned += u32::from(out[season]);
    }
    out[CLIMATE_SEASONS - 1] = (65_535u32.saturating_sub(assigned)).min(65_535) as u16;
    out
}

fn local_runoff(climate: ClimateCell, tectonics: TectonicCell) -> (f32, [f64; 4]) {
    let infiltration = infiltration_fraction(tectonics);
    let mut seasonal = [0.0f64; CLIMATE_SEASONS];
    let mut snow_store = 0.0f64;
    for (season, seasonal_runoff) in seasonal.iter_mut().enumerate() {
        let precipitation = f64::from(climate.seasonal_precipitation[season].max(0.0));
        let temperature = f64::from(climate.seasonal_temperature[season]);
        let seasonal_pet = f64::from(climate.potential_evapotranspiration.max(0.0))
            * ((temperature + 12.0) / 44.0).clamp(0.08, 0.48);
        if temperature <= 0.0 {
            snow_store += precipitation * f64::from(climate.snow_persistence.max(0.2));
            *seasonal_runoff = precipitation * 0.04;
        } else {
            let melt = snow_store * (temperature / 12.0).clamp(0.2, 1.0);
            snow_store -= melt;
            let available = (precipitation + melt - seasonal_pet * 0.42).max(0.0);
            *seasonal_runoff =
                available * f64::from(1.0 - infiltration * 0.62) + precipitation * 0.035;
        }
    }
    seasonal[CLIMATE_SEASONS - 1] += snow_store * 0.08;
    let annual = seasonal.iter().sum::<f64>().max(0.5) as f32;
    (annual, seasonal)
}

fn topological_order(receiver: &[u32]) -> Result<Vec<usize>, AtlasError> {
    let mut upstream = vec![0u32; receiver.len()];
    for &next in receiver {
        if next != u32::MAX {
            upstream[next as usize] += 1;
        }
    }
    let mut ready = BinaryHeap::new();
    for (index, count) in upstream.iter().enumerate() {
        if *count == 0 {
            ready.push(std::cmp::Reverse(index));
        }
    }
    let mut order = Vec::with_capacity(receiver.len());
    while let Some(std::cmp::Reverse(index)) = ready.pop() {
        order.push(index);
        let next = receiver[index];
        if next == u32::MAX {
            continue;
        }
        let slot = &mut upstream[next as usize];
        *slot -= 1;
        if *slot == 0 {
            ready.push(std::cmp::Reverse(next as usize));
        }
    }
    if order.len() != receiver.len() {
        return Err(AtlasError::Corrupt(
            "priority drainage contains an undeclared directed cycle".into(),
        ));
    }
    Ok(order)
}

struct FlowAccumulation {
    order: Vec<usize>,
    annual: Vec<f64>,
    seasonal: Vec<[f64; CLIMATE_SEASONS]>,
    area: Vec<f64>,
}

fn accumulate_flow(
    receiver: &[u32],
    geometry: &AtlasGrid<GeometryCell>,
    runoff: &[f32],
    seasonal_runoff: &[[f64; CLIMATE_SEASONS]],
) -> Result<FlowAccumulation, AtlasError> {
    let order = topological_order(receiver)?;
    let mut annual = vec![0.0f64; receiver.len()];
    let mut seasonal = vec![[0.0f64; CLIMATE_SEASONS]; receiver.len()];
    let mut area = vec![0.0f64; receiver.len()];
    for index in 0..receiver.len() {
        let cell_area = f64::from(geometry.values()[index].physical_area);
        area[index] = cell_area;
        annual[index] = f64::from(runoff[index]) * cell_area / 1000.0;
        for (accumulated, local) in seasonal[index].iter_mut().zip(seasonal_runoff[index]) {
            *accumulated = local * cell_area / 1000.0;
        }
    }
    for &index in &order {
        let next = receiver[index];
        if next == u32::MAX {
            continue;
        }
        let next = next as usize;
        annual[next] += annual[index];
        area[next] += area[index];
        let upstream = seasonal[index];
        for (downstream, upstream) in seasonal[next].iter_mut().zip(upstream) {
            *downstream += upstream;
        }
    }
    Ok(FlowAccumulation {
        order,
        annual,
        seasonal,
        area,
    })
}

fn edge_distance(side: u16, a: usize, b: usize) -> f64 {
    geodesic_distance(
        AtlasPos::from_index(a, side)
            .expect("atlas index")
            .center(side),
        AtlasPos::from_index(b, side)
            .expect("atlas index")
            .center(side),
    )
    .max(1.0)
}

fn label_oceans(side: u16, elevations: &[f32]) -> Result<(Vec<u16>, u16), AtlasError> {
    let mut labels = vec![0u16; elevations.len()];
    let mut next = 0u32;
    for start in 0..elevations.len() {
        if elevations[start] > SEA_LEVEL as f32 || labels[start] != 0 {
            continue;
        }
        next += 1;
        if next > u16::MAX as u32 {
            return Err(AtlasError::Corrupt(
                "ocean component count exceeds persisted identifier width".into(),
            ));
        }
        let label = next as u16;
        let mut queue = VecDeque::from([start]);
        labels[start] = label;
        while let Some(index) = queue.pop_front() {
            for neighbor in neighbors8_indices(index, side) {
                if labels[neighbor] == 0 && elevations[neighbor] <= SEA_LEVEL as f32 {
                    labels[neighbor] = label;
                    queue.push_back(neighbor);
                }
            }
        }
    }
    Ok((labels, next as u16))
}

fn storage_curve(
    side: u16,
    indices: &[usize],
    elevations: &[f32],
    maximum: f32,
) -> Vec<StoragePoint> {
    let column_area = f64::from(FACE_BLOCKS / side).powi(2);
    let minimum = indices
        .iter()
        .map(|index| elevations[*index])
        .min_by(f32::total_cmp)
        .unwrap_or(maximum)
        .min(maximum);
    (0..=4)
        .map(|step| {
            let t = step as f32 / 4.0;
            let elevation = minimum + (maximum - minimum) * t;
            let volume = indices
                .iter()
                .map(|index| {
                    let depth = f64::from((elevation - elevations[*index]).max(0.0));
                    depth * column_area * 8.0
                })
                .sum::<f64>()
                .round()
                .clamp(0.0, u64::MAX as f64) as u64;
            StoragePoint {
                elevation,
                volume_units: volume,
            }
        })
        .collect()
}

fn receiver_path_reaches(
    start: u32,
    receiver: &[u32],
    targets: &std::collections::BTreeSet<usize>,
) -> bool {
    let mut next = start;
    for _ in 0..receiver.len() {
        if next == u32::MAX {
            return false;
        }
        let index = next as usize;
        if targets.contains(&index) {
            return true;
        }
        next = receiver[index];
    }
    // An existing cycle is never a safe outlet. The normal priority-flood
    // graph is acyclic; this conservative answer also keeps lake solving
    // from hiding corruption if that invariant is ever broken earlier.
    true
}

fn basin_connection_sills(
    side: u16,
    elevations: &[f32],
    ocean_labels: &[u16],
    basin_count: u16,
) -> Vec<(u16, f32)> {
    let mut owner = ocean_labels.to_vec();
    let mut best = vec![f32::INFINITY; elevations.len()];
    let mut heap = BinaryHeap::new();
    for (index, label) in ocean_labels.iter().copied().enumerate() {
        if label != 0 {
            best[index] = elevations[index];
            heap.push(FloodEntry {
                elevation: elevations[index],
                index,
            });
        }
    }
    let mut connections = vec![(0u16, f32::INFINITY); usize::from(basin_count) + 1];
    while let Some(entry) = heap.pop() {
        if entry.elevation > best[entry.index] {
            continue;
        }
        for neighbor in neighbors8_indices(entry.index, side) {
            let candidate = entry.elevation.max(elevations[neighbor]);
            if owner[neighbor] == 0 || candidate < best[neighbor] {
                owner[neighbor] = owner[entry.index];
                best[neighbor] = candidate;
                heap.push(FloodEntry {
                    elevation: candidate,
                    index: neighbor,
                });
            } else if owner[neighbor] != owner[entry.index] {
                let a = owner[entry.index];
                let b = owner[neighbor];
                if a == 0 || b == 0 {
                    continue;
                }
                let sill = candidate.max(best[neighbor]);
                for (from, to) in [(a, b), (b, a)] {
                    let slot = &mut connections[usize::from(from)];
                    if sill < slot.1 || (sill == slot.1 && to < slot.0) {
                        *slot = (to, sill);
                    }
                }
            }
        }
    }
    connections
}

fn ocean_records(
    seed: u32,
    side: u16,
    elevations: &[f32],
    geometry: &AtlasGrid<GeometryCell>,
    labels: &[u16],
    basin_count: u16,
) -> (Vec<OceanBasinRecord>, u16) {
    let mut members = vec![Vec::<usize>::new(); usize::from(basin_count) + 1];
    for (index, label) in labels.iter().copied().enumerate() {
        if label != 0 {
            members[usize::from(label)].push(index);
        }
    }
    let dominant = (1..=basin_count)
        .max_by_key(|id| members[usize::from(*id)].len())
        .unwrap_or(1);
    let connections = basin_connection_sills(side, elevations, labels, basin_count);
    let mut records = Vec::new();
    for id in 1..=basin_count {
        let indices = &members[usize::from(id)];
        let area = indices
            .iter()
            .map(|index| f64::from(geometry.values()[*index].physical_area))
            .sum();
        let curve = storage_curve(side, indices, elevations, SEA_LEVEL as f32);
        let volume = curve.last().map_or(0, |point| point.volume_units);
        let enclosed = id != dominant;
        let hash = mix64(u64::from(seed) ^ u64::from(id) ^ 0x6f63_6561_6e73);
        let name = if id == dominant {
            "The World Ocean".to_string()
        } else {
            format!("{} Sea", generated_word(hash))
        };
        let (connection_basin_id, connection_sill_elevation) = if id == dominant {
            (0, SEA_LEVEL as f32)
        } else {
            connections[usize::from(id)]
        };
        records.push(OceanBasinRecord {
            id,
            name,
            cell_count: indices.len().try_into().unwrap_or(u32::MAX),
            area,
            baseline_volume_units: volume,
            salinity: if enclosed { 232 } else { 220 },
            connection_basin_id,
            connection_sill_elevation,
            volume_elevation_curve: curve,
        });
    }
    (records, dominant)
}

fn generated_word(hash: u64) -> String {
    const ONSETS: [&str; 16] = [
        "Al", "Bar", "Cor", "Dun", "Esh", "Fen", "Gal", "Har", "Is", "Kel", "Lor", "Mor", "Nor",
        "Or", "Sel", "Var",
    ];
    const CODAS: [&str; 16] = [
        "a", "en", "eth", "ia", "in", "or", "un", "ara", "mere", "vale", "esh", "os", "yr", "ain",
        "ora", "ith",
    ];
    format!(
        "{}{}",
        ONSETS[(hash & 15) as usize],
        CODAS[((hash >> 8) & 15) as usize]
    )
}

struct ErosionResult {
    elevations: Vec<f32>,
    iterations: u8,
    residual: f32,
    erosion: Vec<f32>,
    deposition: Vec<f32>,
}

fn erosion_loop(
    side: u16,
    geometry: &AtlasGrid<GeometryCell>,
    tectonics: &AtlasGrid<TectonicCell>,
    climate: &AtlasGrid<ClimateCell>,
    original: &[f32],
) -> Result<ErosionResult, AtlasError> {
    let mut elevations = original.to_vec();
    let mut total_erosion = vec![0.0f32; elevations.len()];
    let mut total_deposition = vec![0.0f32; elevations.len()];
    let runoff_data: Vec<_> = climate
        .values()
        .iter()
        .copied()
        .zip(tectonics.values().iter().copied())
        .map(|(climate, tectonics)| local_runoff(climate, tectonics))
        .collect();
    let runoff: Vec<f32> = runoff_data.iter().map(|value| value.0).collect();
    let seasonal: Vec<[f64; 4]> = runoff_data.iter().map(|value| value.1).collect();
    let mean_area = geometry
        .values()
        .iter()
        .map(|cell| f64::from(cell.physical_area))
        .sum::<f64>()
        / geometry.len() as f64;
    let mut iterations = 0;
    let mut residual = 0.0f32;
    for iteration in 0..MAX_EROSION_ITERATIONS {
        let (filled, receiver) = priority_flood(side, &elevations);
        let flow = accumulate_flow(&receiver, geometry, &runoff, &seasonal)?;
        let mut next = elevations.clone();
        residual = 0.0;
        let damping = 0.72f32.powi(i32::from(iteration));
        for index in 0..elevations.len() {
            let ocean = original[index] <= SEA_LEVEL as f32;
            if ocean {
                continue;
            }
            let depression = (filled[index] - elevations[index]).max(0.0);
            let receiver_index = receiver[index];
            let mut erosion = 0.0f32;
            let mut deposition = 0.0f32;
            if receiver_index != u32::MAX {
                let receiver_index = receiver_index as usize;
                let slope = ((filled[index] - filled[receiver_index])
                    / edge_distance(side, index, receiver_index) as f32)
                    .max(0.0);
                let discharge = (flow.annual[index] / mean_area).max(0.0) as f32;
                let power = discharge.ln_1p() * (slope * 48.0).sqrt();
                erosion = (power * 0.22 / bedrock_resistance(tectonics.values()[index])).min(0.72)
                    * damping;
                if slope < 0.0025 && discharge > 1.0 {
                    deposition = ((0.0025 - slope) * discharge.sqrt() * 12.0).min(0.22) * damping;
                }
            }
            if depression > 0.2 {
                deposition += depression.min(1.0) * 0.16 * damping;
            }
            let minimum = SEA_LEVEL as f32 + 0.35;
            let changed = (elevations[index] - erosion + deposition)
                .max(minimum)
                .min(CHANNEL_CEILING);
            let delta = changed - elevations[index];
            next[index] = changed;
            residual = residual.max(delta.abs());
            if delta < 0.0 {
                total_erosion[index] -= delta;
            } else {
                total_deposition[index] += delta;
            }
        }
        elevations = next;
        iterations = iteration + 1;
        if residual <= EROSION_CONVERGENCE_BLOCKS {
            break;
        }
    }
    Ok(ErosionResult {
        elevations,
        iterations,
        residual,
        erosion: total_erosion,
        deposition: total_deposition,
    })
}

const CHANNEL_CEILING: f32 = crate::chunk::CHUNK_Y as f32 - 2.0;

fn shape_supported_basins(
    seed: u32,
    side: u16,
    geometry: &AtlasGrid<GeometryCell>,
    tectonics: &AtlasGrid<TectonicCell>,
    climate: &AtlasGrid<ClimateCell>,
    elevations: &mut [f32],
    erosion: &mut [f32],
) -> Result<(), AtlasError> {
    let (filled, receiver) = priority_flood(side, elevations);
    let runoff_data: Vec<_> = climate
        .values()
        .iter()
        .copied()
        .zip(tectonics.values().iter().copied())
        .map(|(climate, tectonics)| local_runoff(climate, tectonics))
        .collect();
    let runoff: Vec<f32> = runoff_data.iter().map(|value| value.0).collect();
    let seasonal: Vec<[f64; CLIMATE_SEASONS]> = runoff_data.iter().map(|value| value.1).collect();
    let flow = accumulate_flow(&receiver, geometry, &runoff, &seasonal)?;
    let mean_area = geometry
        .values()
        .iter()
        .map(|cell| f64::from(cell.physical_area))
        .sum::<f64>()
        / geometry.len() as f64;
    // category: rift, crater, alpine/glacial, closed/arid, wet through-flow.
    let mut candidates = Vec::<(u8, f32, u64, usize)>::new();
    for index in 0..elevations.len() {
        if elevations[index] <= SEA_LEVEL as f32 + 5.0 || receiver[index] == u32::MAX {
            continue;
        }
        let tectonic = tectonics.values()[index];
        let local = climate.values()[index];
        let downstream = receiver[index] as usize;
        let slope = ((filled[index] - filled[downstream])
            / edge_distance(side, index, downstream) as f32)
            .max(0.0);
        let discharge = (flow.annual[index] / mean_area) as f32;
        let category = if tectonic.sediment_basin == BasinKind::Rift {
            Some((0, 8.0 + discharge.ln_1p()))
        } else if tectonic.volcanic_history != 0 && tectonic.boundary_distance > 1 {
            Some((1, 7.0 + tectonic.boundary_strength * 2.0))
        } else if elevations[index] > 112.0 && local.mean_temperature < 4.0 {
            Some((2, elevations[index] / 24.0 - local.mean_temperature * 0.1))
        } else if tectonic.sediment_basin == BasinKind::Closed
            || (local.aridity > 1.25 && discharge < 1.2)
        {
            Some((3, local.aridity * 2.0 + (1.2 - discharge).max(0.0)))
        } else if local.aridity < 0.82 && discharge > 2.0 && slope < 0.012 {
            Some((4, discharge.ln_1p() * 2.0 + (0.012 - slope) * 80.0))
        } else {
            None
        };
        if let Some((category, score)) = category {
            let pos = AtlasPos::from_index(index, side).expect("basin candidate");
            candidates.push((
                category,
                score,
                cell_hash(seed, pos, 0x6261_7369_6e5f_7368),
                index,
            ));
        }
    }
    candidates.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| b.1.total_cmp(&a.1))
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| a.3.cmp(&b.3))
    });
    let targets = [5usize, 5, 6, 8, 12];
    let mut accepted = Vec::<usize>::new();
    let mut counts = [0usize; 5];
    for (category, _, hash, index) in candidates {
        if counts[category as usize] >= targets[category as usize] {
            continue;
        }
        let pos = AtlasPos::from_index(index, side).expect("basin candidate");
        if accepted.iter().any(|other| {
            geodesic_distance(
                pos.center(side),
                AtlasPos::from_index(*other, side)
                    .expect("accepted basin")
                    .center(side),
            ) < 230.0
        }) {
            continue;
        }
        let depth = match category {
            0 => 5.5,
            1 => 4.2,
            2 => 3.2,
            3 => 3.8,
            _ => 2.8,
        } + (hash & 255) as f32 / 255.0 * 1.4;
        let floor = (elevations[index] - depth).max(SEA_LEVEL as f32 + 1.2);
        erosion[index] += elevations[index] - floor;
        elevations[index] = floor;
        for neighbor in neighbors8_indices(index, side) {
            if elevations[neighbor] <= SEA_LEVEL as f32 {
                continue;
            }
            let shoulder_depth = depth * 0.32;
            let shoulder = (elevations[neighbor] - shoulder_depth)
                .max(floor + depth * 0.16)
                .max(SEA_LEVEL as f32 + 1.2);
            erosion[neighbor] += (elevations[neighbor] - shoulder).max(0.0);
            elevations[neighbor] = elevations[neighbor].min(shoulder);
        }
        accepted.push(index);
        counts[category as usize] += 1;
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct LakeCandidate {
    members: Vec<usize>,
    sink: usize,
    spill: f32,
    depth: f32,
    score: f32,
}

fn lake_candidates(
    side: u16,
    elevations: &[f32],
    filled: &[f32],
    tectonics: &AtlasGrid<TectonicCell>,
) -> Vec<LakeCandidate> {
    let mut visited = vec![false; elevations.len()];
    let mut candidates = Vec::new();
    for start in 0..elevations.len() {
        if visited[start]
            || elevations[start] <= SEA_LEVEL as f32
            || filled[start] - elevations[start] <= 0.65
        {
            continue;
        }
        let target_fill = filled[start];
        let mut queue = VecDeque::from([start]);
        let mut members = Vec::new();
        visited[start] = true;
        while let Some(index) = queue.pop_front() {
            members.push(index);
            for neighbor in neighbors8_indices(index, side) {
                if !visited[neighbor]
                    && elevations[neighbor] > SEA_LEVEL as f32
                    && filled[neighbor] - elevations[neighbor] > 0.65
                    && (filled[neighbor] - target_fill).abs() <= 0.55
                {
                    visited[neighbor] = true;
                    queue.push_back(neighbor);
                }
            }
        }
        let sink = *members
            .iter()
            .min_by(|a, b| {
                elevations[**a]
                    .total_cmp(&elevations[**b])
                    .then_with(|| a.cmp(b))
            })
            .expect("depression has a member");
        let member_set: std::collections::BTreeSet<_> = members.iter().copied().collect();
        let mut outlet = sink;
        let mut spill = f32::INFINITY;
        for &index in &members {
            for neighbor in neighbors8_indices(index, side) {
                if member_set.contains(&neighbor) {
                    continue;
                }
                let candidate = filled[index]
                    .max(elevations[index])
                    .max(elevations[neighbor]);
                if candidate < spill || (candidate == spill && neighbor < outlet) {
                    spill = candidate;
                    outlet = neighbor;
                }
            }
        }
        if !spill.is_finite() {
            continue;
        }
        let depth = (spill - elevations[sink]).max(0.0);
        let tectonic_bonus = members
            .iter()
            .map(|index| match tectonics.values()[*index].sediment_basin {
                BasinKind::Rift | BasinKind::Closed => 2.0,
                BasinKind::Foreland => 0.8,
                _ => 0.0,
            })
            .fold(0.0f32, f32::max);
        let volcanic_bonus = members
            .iter()
            .any(|index| tectonics.values()[*index].volcanic_history != 0)
            as u8 as f32
            * 1.2;
        let score = depth + (members.len() as f32).ln_1p() * 0.7 + tectonic_bonus + volcanic_bonus;
        candidates.push(LakeCandidate {
            members,
            sink,
            spill,
            depth,
            score,
        });
    }
    candidates.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.sink.cmp(&b.sink))
    });
    candidates
}

fn lake_class(
    candidate: &LakeCandidate,
    terminal: bool,
    playa: bool,
    elevations: &[f32],
    climate: &AtlasGrid<ClimateCell>,
    tectonics: &AtlasGrid<TectonicCell>,
) -> LakeClass {
    if playa {
        return LakeClass::SeasonalPlaya;
    }
    if candidate
        .members
        .iter()
        .any(|index| tectonics.values()[*index].volcanic_history != 0)
        && candidate.members.len() <= 12
    {
        return LakeClass::VolcanicCrater;
    }
    if candidate
        .members
        .iter()
        .any(|index| tectonics.values()[*index].sediment_basin == BasinKind::Rift)
    {
        return LakeClass::Rift;
    }
    let sink_climate = climate.values()[candidate.sink];
    if elevations[candidate.sink] > 105.0 && sink_climate.mean_temperature < 5.0 {
        return LakeClass::GlacialAlpine;
    }
    if terminal {
        if sink_climate.aridity > 1.05 {
            LakeClass::SalineTerminal
        } else {
            LakeClass::TerminalFresh
        }
    } else {
        LakeClass::ThroughFlowFresh
    }
}

struct LakeSolution {
    records: Vec<LakeRecord>,
    membership: Vec<u32>,
    surface: Vec<f32>,
    salinity: Vec<u8>,
    terminal_sink: Vec<bool>,
}

struct LakeInput<'a> {
    seed: u32,
    side: u16,
    elevations: &'a [f32],
    filled: &'a [f32],
    receiver: &'a mut [u32],
    geometry: &'a AtlasGrid<GeometryCell>,
    climate: &'a AtlasGrid<ClimateCell>,
    tectonics: &'a AtlasGrid<TectonicCell>,
    flow: &'a FlowAccumulation,
}

fn solve_lakes(input: LakeInput<'_>) -> LakeSolution {
    let LakeInput {
        seed,
        side,
        elevations,
        filled,
        receiver,
        geometry,
        climate,
        tectonics,
        flow,
    } = input;
    let mut candidates = lake_candidates(side, elevations, filled, tectonics);
    // A real finite planet should expose multiple basins.  Tiny fixtures may
    // have only one; keeping the deepest is enough to exercise the contract.
    candidates.retain(|candidate| {
        candidate.depth >= 1.15
            && (candidate.members.len() >= 2
                || candidate.score >= 3.0
                || tectonics.values()[candidate.sink].sediment_basin != BasinKind::None)
    });
    candidates.truncate(4_096);
    let mut membership = vec![0u32; elevations.len()];
    let mut surface = vec![f32::NAN; elevations.len()];
    let mut salinity = vec![0u8; elevations.len()];
    let mut terminal_sink = vec![false; elevations.len()];
    let mut records = Vec::new();
    for candidate in candidates {
        if candidate
            .members
            .iter()
            .any(|index| membership[*index] != 0)
        {
            continue;
        }
        let potential_evaporation = candidate
            .members
            .iter()
            .map(|index| {
                f64::from(
                    climate.values()[*index]
                        .potential_evapotranspiration
                        .max(0.0),
                ) * f64::from(geometry.values()[*index].physical_area)
                    / 1000.0
            })
            .sum::<f64>();
        let inflow = flow.annual[candidate.sink].max(0.001);
        let aridity = candidate
            .members
            .iter()
            .map(|index| climate.values()[*index].aridity)
            .sum::<f32>()
            / candidate.members.len() as f32;
        let tectonically_closed = candidate
            .members
            .iter()
            .any(|index| tectonics.values()[*index].sediment_basin == BasinKind::Closed);
        let balance = (inflow / potential_evaporation.max(0.001)) as f32;
        let terminal = tectonically_closed || (aridity > 0.95 && balance < 1.08);
        let fill_fraction = if terminal {
            balance.clamp(0.03, 0.92).sqrt()
        } else {
            1.0
        };
        let lake_surface = elevations[candidate.sink]
            + (candidate.spill - elevations[candidate.sink]) * fill_fraction;
        let playa =
            terminal && (fill_fraction < 0.28 || lake_surface - elevations[candidate.sink] < 0.7);
        let class = lake_class(&candidate, terminal, playa, elevations, climate, tectonics);
        let id = records.len() as u32 + 1;
        let mut wet_members = Vec::new();
        for &index in &candidate.members {
            if elevations[index] < lake_surface - 0.04 || (playa && index == candidate.sink) {
                membership[index] = id;
                surface[index] = lake_surface;
                wet_members.push(index);
            }
        }
        if wet_members.is_empty() {
            continue;
        }
        let wet_set: std::collections::BTreeSet<_> = wet_members.iter().copied().collect();
        let root_and_outlet = if terminal {
            Some((candidate.sink, u32::MAX))
        } else {
            // Preserve a real edge from the acyclic priority-flood graph.
            // Merely choosing the geometrically lowest boundary neighbor can
            // choose a cell whose own receiver points back into the lake and
            // create a seed-dependent two-cycle.
            wet_members
                .iter()
                .copied()
                .filter_map(|index| {
                    let next = receiver[index];
                    (next != u32::MAX
                        && !wet_set.contains(&(next as usize))
                        && !receiver_path_reaches(next, receiver, &wet_set))
                    .then_some((index, next))
                })
                .min_by(|(a_index, a_next), (b_index, b_next)| {
                    filled[*a_index]
                        .total_cmp(&filled[*b_index])
                        .then_with(|| a_next.cmp(b_next))
                        .then_with(|| a_index.cmp(b_index))
                })
        };
        let Some((root, resolved_outlet)) = root_and_outlet else {
            for index in wet_members {
                membership[index] = 0;
                surface[index] = f32::NAN;
            }
            continue;
        };
        let mut routed = std::collections::BTreeSet::from([root]);
        let mut queue = VecDeque::from([root]);
        receiver[root] = resolved_outlet;
        while let Some(index) = queue.pop_front() {
            for neighbor in neighbors8_indices(index, side) {
                if wet_set.contains(&neighbor) && routed.insert(neighbor) {
                    receiver[neighbor] = index as u32;
                    queue.push_back(neighbor);
                }
            }
        }
        // A thresholded nested depression can leave a disconnected wet cell.
        // It is a separate pond, not part of this reservoir record.
        for index in wet_members.extract_if(.., |index| !routed.contains(index)) {
            membership[index] = 0;
            surface[index] = f32::NAN;
        }
        if terminal {
            terminal_sink[root] = true;
        }
        let concentration = if terminal {
            (64.0 + aridity.max(0.0) * 84.0 + (1.0 - fill_fraction) * 96.0)
                .round()
                .clamp(8.0, 255.0) as u8
        } else {
            (4.0 + aridity.max(0.0) * 12.0).round().clamp(0.0, 63.0) as u8
        };
        for &index in &wet_members {
            salinity[index] = concentration;
        }
        let curve = storage_curve(side, &candidate.members, elevations, candidate.spill);
        let column_area = f64::from(FACE_BLOCKS / side).powi(2);
        let volume = wet_members
            .iter()
            .map(|index| {
                f64::from((lake_surface - elevations[*index]).max(0.0)) * column_area * 8.0
            })
            .sum::<f64>()
            .round()
            .clamp(0.0, u64::MAX as f64) as u64;
        let seasonal_range = (climate.values()[candidate.sink]
            .precipitation_seasonality
            .abs()
            * 1.8
            / candidate.depth.max(0.5))
        .clamp(0.05, 3.5);
        let evaporation = if terminal {
            inflow
        } else {
            potential_evaporation.min(inflow * 0.85)
        };
        let outflow = if terminal {
            0.0
        } else {
            (inflow - evaporation).max(0.0)
        };
        let hash = mix64(u64::from(seed) ^ u64::from(id) ^ 0x006c_616b_6573);
        records.push(LakeRecord {
            id,
            name: if playa {
                format!("{} Playa", generated_word(hash))
            } else {
                format!("Lake {}", generated_word(hash))
            },
            class,
            sink: AtlasPos::from_index(candidate.sink, side).expect("lake sink"),
            outlet: (!terminal)
                .then(|| AtlasPos::from_index(resolved_outlet as usize, side))
                .flatten(),
            cell_count: wet_members.len().try_into().unwrap_or(u32::MAX),
            catchment_area: flow.area[candidate.sink],
            surface_elevation: lake_surface,
            spill_elevation: candidate.spill,
            baseline_inflow: inflow,
            baseline_evaporation: evaporation,
            baseline_outflow: outflow,
            groundwater_exchange_coefficient: (infiltration_fraction(
                tectonics.values()[candidate.sink],
            ) * 0.18)
                .clamp(0.01, 0.12),
            salinity: concentration,
            seasonal_level_range: seasonal_range,
            // A playa's curve records how much a wet season can hold, but its
            // genesis state is the dry salt flat materialized by the dense
            // atlas cells. Capacity is not baseline water mass.
            baseline_volume_units: if playa { 0 } else { volume },
            voxel_volume_residual: 0,
            volume_elevation_curve: curve,
        });
    }
    LakeSolution {
        records,
        membership,
        surface,
        salinity,
        terminal_sink,
    }
}

struct WatershedInput<'a> {
    side: u16,
    receiver: &'a [u32],
    ocean: &'a [u16],
    lake: &'a [u32],
    terminal_sink: &'a [bool],
    geometry: &'a AtlasGrid<GeometryCell>,
    runoff: &'a [f32],
    seed: u32,
}

fn assign_watersheds(
    input: WatershedInput<'_>,
) -> Result<(Vec<u32>, Vec<WatershedRecord>), AtlasError> {
    let WatershedInput {
        side,
        receiver,
        ocean,
        lake,
        terminal_sink,
        geometry,
        runoff,
        seed,
    } = input;
    let order = topological_order(receiver)?;
    let mut root = vec![u32::MAX; receiver.len()];
    for &index in order.iter().rev() {
        if ocean[index] != 0 || receiver[index] == u32::MAX {
            root[index] = index as u32;
        } else {
            root[index] = root[receiver[index] as usize];
        }
    }
    let mut ids = BTreeMap::<u32, u32>::new();
    let mut watershed = vec![0u32; receiver.len()];
    for index in 0..receiver.len() {
        if ocean[index] != 0 {
            continue;
        }
        let next = ids.len() as u32 + 1;
        let id = *ids.entry(root[index]).or_insert(next);
        watershed[index] = id;
    }
    let mut area = vec![0.0f64; ids.len() + 1];
    let mut weighted_runoff = vec![0.0f64; ids.len() + 1];
    for index in 0..receiver.len() {
        let id = watershed[index] as usize;
        if id == 0 {
            continue;
        }
        let cell_area = f64::from(geometry.values()[index].physical_area);
        area[id] += cell_area;
        weighted_runoff[id] += f64::from(runoff[index]) * cell_area;
    }
    let mut records = Vec::with_capacity(ids.len());
    for (outlet, id) in ids {
        let outlet_index = outlet as usize;
        let hash = mix64(u64::from(seed) ^ u64::from(id) ^ 0x7761_7465_7273_6864);
        records.push(WatershedRecord {
            id,
            name: format!("{} Watershed", generated_word(hash)),
            outlet: AtlasPos::from_index(outlet_index, side).expect("watershed outlet"),
            terminal: terminal_sink[outlet_index] || lake[outlet_index] != 0,
            area: area[id as usize],
            mean_runoff: if area[id as usize] > 0.0 {
                weighted_runoff[id as usize] / area[id as usize]
            } else {
                0.0
            },
        });
    }
    records.sort_by_key(|record| record.id);
    Ok((watershed, records))
}

fn stream_orders(receiver: &[u32], channel: &[bool]) -> Result<Vec<u8>, AtlasError> {
    let order = topological_order(receiver)?;
    let mut order_value = vec![0u8; receiver.len()];
    let mut largest_upstream = vec![0u8; receiver.len()];
    let mut largest_count = vec![0u8; receiver.len()];
    for &index in &order {
        if channel[index] {
            let inherited = largest_upstream[index];
            order_value[index] = if inherited == 0 {
                1
            } else if largest_count[index] >= 2 {
                inherited.saturating_add(1)
            } else {
                inherited
            };
        }
        let next = receiver[index];
        if next == u32::MAX || !channel[index] {
            continue;
        }
        let next = next as usize;
        let value = order_value[index];
        if value > largest_upstream[next] {
            largest_upstream[next] = value;
            largest_count[next] = 1;
        } else if value == largest_upstream[next] {
            largest_count[next] = largest_count[next].saturating_add(1);
        }
    }
    Ok(order_value)
}

struct RiverInput<'a> {
    seed: u32,
    side: u16,
    receiver: &'a [u32],
    watershed: &'a [u32],
    channel: &'a [bool],
    cells: &'a mut [HydrologyCell],
    model_lakes: &'a [LakeRecord],
    model_oceans: &'a [OceanBasinRecord],
}

fn river_records(input: RiverInput<'_>) -> Vec<RiverRecord> {
    let RiverInput {
        seed,
        side,
        receiver,
        watershed,
        channel,
        cells,
        model_lakes,
        model_oceans,
    } = input;
    let mut by_watershed = BTreeMap::<u32, Vec<usize>>::new();
    for (index, present) in channel.iter().copied().enumerate() {
        if present {
            by_watershed
                .entry(watershed[index])
                .or_default()
                .push(index);
        }
    }
    let mut records = Vec::new();
    for (watershed_id, members) in by_watershed {
        if watershed_id == 0 || members.len() < 2 {
            continue;
        }
        let member_set: std::collections::BTreeSet<_> = members.iter().copied().collect();
        let mouth = *members
            .iter()
            .filter(|index| {
                receiver[**index] == u32::MAX || !member_set.contains(&(receiver[**index] as usize))
            })
            .max_by(|a, b| {
                cells[**a]
                    .mean_discharge
                    .total_cmp(&cells[**b].mean_discharge)
                    .then_with(|| b.cmp(a))
            })
            .unwrap_or_else(|| {
                members
                    .iter()
                    .max_by(|a, b| {
                        cells[**a]
                            .mean_discharge
                            .total_cmp(&cells[**b].mean_discharge)
                    })
                    .expect("river has members")
            });
        let mut distance_to_mouth = BTreeMap::<usize, f64>::new();
        distance_to_mouth.insert(mouth, 0.0);
        let mut changed = true;
        while changed {
            changed = false;
            for &index in &members {
                let next = receiver[index];
                if next == u32::MAX {
                    continue;
                }
                let next = next as usize;
                if let Some(downstream) = distance_to_mouth.get(&next).copied() {
                    let candidate = downstream + edge_distance(side, index, next);
                    if distance_to_mouth
                        .get(&index)
                        .is_none_or(|old| candidate > *old)
                    {
                        distance_to_mouth.insert(index, candidate);
                        changed = true;
                    }
                }
            }
        }
        let source = members
            .iter()
            .copied()
            .max_by(|a, b| {
                distance_to_mouth
                    .get(a)
                    .copied()
                    .unwrap_or(0.0)
                    .total_cmp(&distance_to_mouth.get(b).copied().unwrap_or(0.0))
                    .then_with(|| b.cmp(a))
            })
            .unwrap_or(mouth);
        let mut path = Vec::new();
        let mut at = source;
        let mut seen = std::collections::BTreeSet::new();
        while seen.insert(at) {
            path.push(AtlasPos::from_index(at, side).expect("river path"));
            if at == mouth || receiver[at] == u32::MAX {
                break;
            }
            at = receiver[at] as usize;
        }
        let id = records.len() as u32 + 1;
        for &index in &members {
            cells[index].river_id = id;
        }
        let sink_index = receiver[mouth];
        let sink_name = if sink_index == u32::MAX {
            "an inland basin".to_string()
        } else {
            let sink = &cells[sink_index as usize];
            if sink.lake_basin_id != 0 {
                model_lakes
                    .iter()
                    .find(|lake| lake.id == sink.lake_basin_id)
                    .map_or_else(|| "a lake".to_string(), |lake| lake.name.clone())
            } else if sink.ocean_basin_id != 0 {
                model_oceans
                    .iter()
                    .find(|ocean| ocean.id == sink.ocean_basin_id)
                    .map_or_else(|| "the sea".to_string(), |ocean| ocean.name.clone())
            } else {
                "a downstream river".to_string()
            }
        };
        let hash = mix64(u64::from(seed) ^ u64::from(id) ^ 0x0000_7269_7665_7273);
        records.push(RiverRecord {
            id,
            name: format!("{} River", generated_word(hash)),
            watershed_id,
            source: AtlasPos::from_index(source, side).expect("river source"),
            mouth: AtlasPos::from_index(mouth, side).expect("river mouth"),
            sink_name,
            length_blocks: path
                .windows(2)
                .map(|edge| geodesic_distance(edge[0].center(side), edge[1].center(side)))
                .sum(),
            maximum_discharge: members
                .iter()
                .map(|index| cells[*index].mean_discharge)
                .fold(0.0, f32::max),
            maximum_width_blocks: members
                .iter()
                .map(|index| f32::from(cells[*index].channel_width_centiblocks) / 100.0)
                .fold(0.0, f32::max),
            stream_order: members
                .iter()
                .map(|index| cells[*index].stream_order)
                .max()
                .unwrap_or(1),
            path,
        });
    }
    records
}

struct ChannelInput<'a> {
    seed: u32,
    side: u16,
    elevations: &'a [f32],
    receiver: &'a [u32],
    geometry: &'a AtlasGrid<GeometryCell>,
    tectonics: &'a AtlasGrid<TectonicCell>,
    climate: &'a AtlasGrid<ClimateCell>,
    flow: &'a FlowAccumulation,
    watershed: &'a [u32],
    lake: &'a LakeSolution,
    ocean: &'a [u16],
    erosion: &'a [f32],
    deposition: &'a [f32],
    cells: &'a mut [HydrologyCell],
    oceans: &'a [OceanBasinRecord],
}

fn assign_channels(input: ChannelInput<'_>) -> Result<Vec<RiverRecord>, AtlasError> {
    let ChannelInput {
        seed,
        side,
        elevations,
        receiver,
        geometry,
        tectonics,
        climate,
        flow,
        watershed,
        lake,
        ocean,
        erosion,
        deposition,
        cells,
        oceans,
    } = input;
    let mean_area = geometry
        .values()
        .iter()
        .map(|cell| f64::from(cell.physical_area))
        .sum::<f64>()
        / geometry.len() as f64;
    let threshold = mean_area * 1.15;
    let mut channel = vec![false; cells.len()];
    for index in 0..cells.len() {
        if ocean[index] != 0 || lake.membership[index] != 0 {
            continue;
        }
        let runoff_equivalent = flow.annual[index] / threshold;
        channel[index] = runoff_equivalent >= 1.0 && receiver[index] != u32::MAX;
    }
    let orders = stream_orders(receiver, &channel)?;
    for index in 0..cells.len() {
        let runoff_data = local_runoff(climate.values()[index], tectonics.values()[index]);
        cells[index].drainage_receiver = receiver[index];
        cells[index].watershed_id = watershed[index];
        cells[index].mean_runoff = runoff_data.0;
        cells[index].seasonal_runoff_fraction = normalized_fractions(runoff_data.1);
        cells[index].mean_discharge = flow.annual[index].min(f32::MAX as f64) as f32;
        cells[index].seasonal_discharge_fraction = normalized_fractions(flow.seasonal[index]);
        cells[index].catchment_area = flow.area[index].min(f32::MAX as f64) as f32;
        cells[index].erosion_centiblocks =
            (erosion[index] * 100.0).round().clamp(0.0, i16::MAX as f32) as i16;
        cells[index].deposition_centiblocks = (deposition[index] * 100.0)
            .round()
            .clamp(0.0, i16::MAX as f32) as i16;
        if !channel[index] {
            continue;
        }
        let equivalent = (flow.annual[index] / mean_area).max(0.01) as f32;
        let resistance = bedrock_resistance(tectonics.values()[index]);
        let mut width = ((0.9 + equivalent.sqrt() * 1.45) / resistance.sqrt()).clamp(1.2, 28.0);
        let mut depth = ((0.75 + equivalent.powf(0.31) * 0.82) * resistance.sqrt()).clamp(1.0, 8.0);
        let next = receiver[index] as usize;
        let slope = ((cells[index].filled_elevation - cells[next].filled_elevation)
            / edge_distance(side, index, next) as f32)
            .max(0.0);
        // The receiver of a mouth is an ocean-floor cell. Measuring the
        // land-to-seafloor drop classified every coast as steep and made
        // deltas impossible even on broad low coastal plains. Delta relief
        // is the height of the last alluvial land above sea level; discharge
        // supplies the sediment. Higher/smaller mouths remain estuaries.
        let coastal_relief = (elevations[index] - SEA_LEVEL as f32).max(0.0);
        let delta_mouth = ocean[next] != 0
            && equivalent > 3.0
            && (coastal_relief <= 4.0 || slope < 0.006)
            && resistance <= 1.25;
        let estuary_mouth = ocean[next] != 0 && !delta_mouth;
        if delta_mouth {
            width = (width * 1.75).min(40.0);
            depth = (depth * 0.72).max(1.0);
            cells[index].deposition_centiblocks = cells[index]
                .deposition_centiblocks
                .saturating_add((equivalent.sqrt() * 18.0).round() as i16);
        } else if estuary_mouth {
            width = (width * 1.25).min(34.0);
        }
        let minimum_surface = if ocean[next] == 0 {
            SEA_LEVEL as f32 + 1.0
        } else {
            SEA_LEVEL as f32
        };
        let water_surface = (elevations[index] - 0.35).floor().max(minimum_surface);
        cells[index].channel_bed_elevation = (water_surface - depth).max(2.0);
        cells[index].water_surface_elevation = water_surface;
        cells[index].channel_width_centiblocks = (width * 100.0).round() as u16;
        cells[index].channel_depth_centiblocks = (depth * 100.0).round() as u16;
        cells[index].sediment_energy =
            ((slope * 3600.0 + equivalent.sqrt() * 900.0).clamp(0.0, 65_535.0)) as u16;
        cells[index].stream_order = orders[index];
        cells[index].flags |= HYDRO_RIVER;
        let min_share = cells[index]
            .seasonal_discharge_fraction
            .iter()
            .copied()
            .min()
            .unwrap_or(0);
        if min_share >= 3_000 || climate.values()[index].mean_precipitation > 1_000.0 {
            cells[index].flags |= HYDRO_PERENNIAL;
        } else {
            cells[index].flags |= HYDRO_INTERMITTENT;
        }
        if slope < 0.0045 && equivalent > 2.0 {
            cells[index].flags |= HYDRO_FLOODPLAIN;
        }
        if slope > 0.075 || elevations[index] - elevations[next] > 5.0 {
            cells[index].flags |= HYDRO_WATERFALL;
        }
        if BedrockFamily::from_id(tectonics.values()[index].bedrock_family)
            == BedrockFamily::Limestone
            && cells[index].flags & HYDRO_INTERMITTENT != 0
        {
            cells[index].flags |= HYDRO_KARST_LOSS;
        }
        let dissolved = match BedrockFamily::from_id(tectonics.values()[index].bedrock_family) {
            BedrockFamily::Limestone | BedrockFamily::Evaporite => 18.0,
            BedrockFamily::Shale | BedrockFamily::Sandstone => 8.0,
            _ => 3.0,
        };
        cells[index].salinity = (dissolved + climate.values()[index].aridity * 6.0)
            .round()
            .clamp(0.0, 63.0) as u8;
        cells[index].water_body = WaterBodyKind::River;
        if delta_mouth {
            cells[index].flags |= HYDRO_DELTA;
            cells[index].water_body = WaterBodyKind::Delta;
        } else if estuary_mouth {
            cells[index].flags |= HYDRO_ESTUARY;
            cells[index].water_body = WaterBodyKind::Estuary;
            cells[index].salinity = 96;
        }
        let length = edge_distance(side, index, next) as f32;
        let continuous = f64::from(length * width * depth * 8.0);
        let voxel = f64::from(length.floor() * width.floor().max(1.0) * depth.floor() * 8.0);
        let baseline_wet = cells[index].flags & HYDRO_PERENNIAL != 0 || equivalent >= 4.0;
        if baseline_wet {
            cells[index].baseline_water_units = continuous.round().max(0.0) as u64;
            cells[index].voxel_volume_residual = (continuous - voxel)
                .round()
                .clamp(i32::MIN as f64, i32::MAX as f64)
                as i32;
        }
    }
    // Dissolved load can accumulate or mix but cannot spontaneously vanish
    // at a lithological boundary. Estuaries are the explicit salt-mixing
    // exception and already carry their brackish genesis concentration.
    for &index in &flow.order {
        if !channel[index] || receiver[index] == u32::MAX {
            continue;
        }
        let next = receiver[index] as usize;
        if channel[next] && cells[next].flags & HYDRO_ESTUARY == 0 {
            cells[next].salinity = cells[next].salinity.max(cells[index].salinity).min(63);
        }
    }
    // Priority-flood routing is acyclic, so a source-to-sink pass can lower
    // any resistant flat step without ever revisiting an upstream bed.
    for &index in &flow.order {
        if !channel[index] || receiver[index] == u32::MAX {
            continue;
        }
        let next = receiver[index] as usize;
        if !channel[next] {
            continue;
        }
        cells[next].channel_bed_elevation = cells[next]
            .channel_bed_elevation
            .min((cells[index].channel_bed_elevation - 0.002).max(2.0));
        cells[next].water_surface_elevation = cells[next]
            .water_surface_elevation
            .min(cells[index].water_surface_elevation);
    }
    Ok(river_records(RiverInput {
        seed,
        side,
        receiver,
        watershed,
        channel: &channel,
        cells,
        model_lakes: &lake.records,
        model_oceans: oceans,
    }))
}

pub(super) fn generate_hydrology(
    seed: u32,
    side: u16,
    geometry: &AtlasGrid<GeometryCell>,
    tectonics: &AtlasGrid<TectonicCell>,
    preliminary_terrain: &AtlasGrid<TerrainCell>,
    climate: &AtlasGrid<ClimateCell>,
    cancel: &CancellationToken,
) -> Result<HydrologyOutput, AtlasError> {
    if cancel.is_cancelled() {
        return Err(AtlasError::Cancelled);
    }
    let original: Vec<f32> = preliminary_terrain
        .values()
        .iter()
        .map(|cell| cell.eroded_elevation)
        .collect();
    let ErosionResult {
        mut elevations,
        iterations,
        residual: erosion_residual,
        mut erosion,
        deposition,
    } = erosion_loop(side, geometry, tectonics, climate, &original)?;
    shape_supported_basins(
        seed,
        side,
        geometry,
        tectonics,
        climate,
        &mut elevations,
        &mut erosion,
    )?;
    if cancel.is_cancelled() {
        return Err(AtlasError::Cancelled);
    }
    let mut terrain_values = preliminary_terrain.values().to_vec();
    for (index, elevation) in elevations.iter().copied().enumerate() {
        terrain_values[index].eroded_elevation = elevation;
    }
    let terrain = AtlasGrid::from_values(side, terrain_values)?;
    let (ocean_labels, ocean_count) = label_oceans(side, &elevations)?;
    let (mut oceans, dominant_ocean_id) = ocean_records(
        seed,
        side,
        &elevations,
        geometry,
        &ocean_labels,
        ocean_count,
    );
    let (filled, mut receiver) = priority_flood(side, &elevations);
    // Build the two arrays directly. Keeping an intermediate tuple vector
    // alive beside both outputs cost roughly 15 MiB at production size and
    // pushed creation needlessly over the 512 MiB process envelope.
    let mut runoff = Vec::with_capacity(elevations.len());
    let mut seasonal_runoff = Vec::with_capacity(elevations.len());
    for (climate, tectonics) in climate
        .values()
        .iter()
        .copied()
        .zip(tectonics.values().iter().copied())
    {
        let (mean, seasonal) = local_runoff(climate, tectonics);
        runoff.push(mean);
        seasonal_runoff.push(seasonal);
    }
    let preliminary_flow = accumulate_flow(&receiver, geometry, &runoff, &seasonal_runoff)?;
    let mut lakes = solve_lakes(LakeInput {
        seed,
        side,
        elevations: &elevations,
        filled: &filled,
        receiver: &mut receiver,
        geometry,
        climate,
        tectonics,
        flow: &preliminary_flow,
    });
    let flow = accumulate_flow(&receiver, geometry, &runoff, &seasonal_runoff)?;
    let (watershed, watersheds) = assign_watersheds(WatershedInput {
        side,
        receiver: &receiver,
        ocean: &ocean_labels,
        lake: &lakes.membership,
        terminal_sink: &lakes.terminal_sink,
        geometry,
        runoff: &runoff,
        seed,
    })?;
    let mut cells = vec![HydrologyCell::default(); elevations.len()];
    let cell_columns = u64::from(FACE_BLOCKS / side).pow(2);
    for index in 0..cells.len() {
        cells[index].drainage_receiver = receiver[index];
        cells[index].watershed_id = watershed[index];
        cells[index].ocean_basin_id = ocean_labels[index];
        cells[index].lake_basin_id = lakes.membership[index];
        cells[index].spill_elevation = if lakes.membership[index] != 0 {
            lakes
                .records
                .iter()
                .find(|lake| lake.id == lakes.membership[index])
                .map_or(filled[index], |lake| lake.spill_elevation)
        } else {
            filled[index]
        };
        cells[index].filled_elevation = filled[index];
        cells[index].channel_bed_elevation = elevations[index];
        cells[index].water_surface_elevation = elevations[index];
        cells[index].erosion_centiblocks =
            (erosion[index] * 100.0).round().clamp(0.0, i16::MAX as f32) as i16;
        cells[index].deposition_centiblocks = (deposition[index] * 100.0)
            .round()
            .clamp(0.0, i16::MAX as f32) as i16;
        if ocean_labels[index] != 0 {
            let ocean = oceans
                .iter()
                .find(|ocean| ocean.id == ocean_labels[index])
                .expect("ocean label has a record");
            let depth = (SEA_LEVEL as f32 - elevations[index]).max(0.0);
            let continuous = f64::from(depth) * cell_columns as f64 * 8.0;
            let voxel = f64::from(depth.floor()) * cell_columns as f64 * 8.0;
            cells[index].water_surface_elevation = SEA_LEVEL as f32;
            cells[index].channel_depth_centiblocks =
                (depth * 100.0).round().clamp(0.0, u16::MAX as f32) as u16;
            cells[index].salinity = ocean.salinity;
            cells[index].flags = HYDRO_OCEAN;
            cells[index].water_body = WaterBodyKind::Ocean;
            cells[index].baseline_water_units = continuous.round() as u64;
            cells[index].voxel_volume_residual = (continuous - voxel)
                .round()
                .clamp(i32::MIN as f64, i32::MAX as f64)
                as i32;
        } else if lakes.membership[index] != 0 {
            let record = lakes
                .records
                .iter()
                .find(|record| record.id == lakes.membership[index])
                .expect("lake membership has a record");
            let depth = (lakes.surface[index] - elevations[index]).max(0.0);
            let continuous = f64::from(depth) * cell_columns as f64 * 8.0;
            let voxel = f64::from(depth.floor()) * cell_columns as f64 * 8.0;
            cells[index].water_surface_elevation = lakes.surface[index];
            cells[index].channel_depth_centiblocks =
                (depth * 100.0).round().clamp(0.0, u16::MAX as f32) as u16;
            cells[index].seasonal_level_range_centiblocks =
                (record.seasonal_level_range * 100.0).round() as u16;
            cells[index].salinity = lakes.salinity[index];
            cells[index].flags = HYDRO_LAKE;
            cells[index].water_body = if record.class == LakeClass::SeasonalPlaya {
                cells[index].flags |= HYDRO_PLAYA | HYDRO_TERMINAL;
                WaterBodyKind::Playa
            } else {
                if record.outlet.is_none() {
                    cells[index].flags |= HYDRO_TERMINAL;
                }
                WaterBodyKind::Lake
            };
            cells[index].baseline_water_units = if record.class == LakeClass::SeasonalPlaya {
                0
            } else {
                continuous.round() as u64
            };
            cells[index].voxel_volume_residual = if record.class == LakeClass::SeasonalPlaya {
                0
            } else {
                (continuous - voxel)
                    .round()
                    .clamp(i32::MIN as f64, i32::MAX as f64) as i32
            };
        }
    }
    let rivers = assign_channels(ChannelInput {
        seed,
        side,
        elevations: &elevations,
        receiver: &receiver,
        geometry,
        tectonics,
        climate,
        flow: &flow,
        watershed: &watershed,
        lake: &lakes,
        ocean: &ocean_labels,
        erosion: &erosion,
        deposition: &deposition,
        cells: &mut cells,
        oceans: &oceans,
    })?;

    // Low-gradient margins of lakes and channels are localized habitat
    // overlays, never province-wide biome assignments.
    for index in 0..cells.len() {
        if ocean_labels[index] != 0 || lakes.membership[index] != 0 {
            continue;
        }
        let adjacent_surface = neighbors8_indices(index, side)
            .into_iter()
            .filter(|neighbor| ocean_labels[*neighbor] != 0 || lakes.membership[*neighbor] != 0)
            .map(|neighbor| cells[neighbor].water_surface_elevation)
            .max_by(f32::total_cmp);
        if adjacent_surface.is_some_and(|water| (-0.5..=2.5).contains(&(elevations[index] - water)))
            && climate.values()[index].aridity < 0.95
        {
            cells[index].flags |= HYDRO_WETLAND;
            if cells[index].water_body == WaterBodyKind::Land {
                cells[index].water_body = WaterBodyKind::Wetland;
            }
        }
    }
    let baseline_surface_water_units = cells
        .iter()
        .map(|cell| u128::from(cell.baseline_water_units))
        .sum();
    let voxel_volume_residual = cells
        .iter()
        .map(|cell| i128::from(cell.voxel_volume_residual))
        .sum();
    for ocean in &mut oceans {
        ocean.baseline_volume_units = cells
            .iter()
            .filter(|cell| cell.ocean_basin_id == ocean.id)
            .fold(0u64, |total, cell| {
                total.saturating_add(cell.baseline_water_units)
            });
    }
    for lake in &mut lakes.records {
        lake.baseline_volume_units = cells
            .iter()
            .filter(|cell| cell.lake_basin_id == lake.id)
            .fold(0u64, |total, cell| {
                total.saturating_add(cell.baseline_water_units)
            });
        lake.voxel_volume_residual = cells
            .iter()
            .filter(|cell| cell.lake_basin_id == lake.id)
            .map(|cell| i64::from(cell.voxel_volume_residual))
            .sum();
    }
    let model = HydrologyModel {
        schema_version: HYDROLOGY_SCHEMA_VERSION,
        sea_level: SEA_LEVEL as f32,
        dominant_ocean_id,
        erosion_iterations: iterations,
        erosion_max_residual: erosion_residual,
        baseline_surface_water_units,
        voxel_volume_residual,
        oceans,
        lakes: lakes.records,
        rivers,
        watersheds,
    };
    let cells = AtlasGrid::from_values(side, cells)?;
    model.validate(side, &terrain, &cells)?;
    Ok(HydrologyOutput {
        terrain,
        cells,
        model,
    })
}

impl HydrologyModel {
    pub fn validate(
        &self,
        side: u16,
        terrain: &AtlasGrid<TerrainCell>,
        cells: &AtlasGrid<HydrologyCell>,
    ) -> Result<(), AtlasError> {
        if self.schema_version != HYDROLOGY_SCHEMA_VERSION
            || self.sea_level != SEA_LEVEL as f32
            || self.erosion_iterations == 0
            || self.erosion_iterations > MAX_EROSION_ITERATIONS
            || !self.erosion_max_residual.is_finite()
        {
            return Err(AtlasError::Corrupt(
                "hydrology model has invalid schema or erosion evidence".into(),
            ));
        }
        if !self
            .oceans
            .iter()
            .any(|ocean| ocean.id == self.dominant_ocean_id)
        {
            return Err(AtlasError::Corrupt(
                "hydrology model has no dominant ocean".into(),
            ));
        }
        for ocean in &self.oceans {
            validate_curve(&ocean.volume_elevation_curve)?;
            let dense_cells = cells
                .values()
                .iter()
                .filter(|cell| cell.ocean_basin_id == ocean.id);
            let dense_count = dense_cells.clone().count() as u32;
            let dense_volume = dense_cells
                .map(|cell| cell.baseline_water_units)
                .fold(0u64, u64::saturating_add);
            if ocean.cell_count == 0
                || ocean.cell_count != dense_count
                || ocean.baseline_volume_units != dense_volume
                || ocean.salinity < 192
            {
                return Err(AtlasError::Corrupt(
                    "ocean record disagrees with dense cells or has freshwater salinity".into(),
                ));
            }
        }
        for lake in &self.lakes {
            validate_curve(&lake.volume_elevation_curve)?;
            let dense_cells = cells
                .values()
                .iter()
                .filter(|cell| cell.lake_basin_id == lake.id);
            let dense_count = dense_cells.clone().count() as u32;
            let dense_volume = dense_cells
                .clone()
                .map(|cell| cell.baseline_water_units)
                .fold(0u64, u64::saturating_add);
            let dense_residual = dense_cells
                .map(|cell| i64::from(cell.voxel_volume_residual))
                .sum::<i64>();
            if lake.cell_count == 0
                || lake.cell_count != dense_count
                || lake.baseline_volume_units != dense_volume
                || lake.voxel_volume_residual != dense_residual
                || lake.surface_elevation > lake.spill_elevation + 0.001
                || (lake.baseline_inflow - lake.baseline_evaporation - lake.baseline_outflow).abs()
                    > lake.baseline_inflow.max(1.0) * 1.0e-8
            {
                return Err(AtlasError::Corrupt(
                    "lake record has invalid geometry or an open water budget".into(),
                ));
            }
        }
        if self.baseline_surface_water_units
            != cells
                .values()
                .iter()
                .map(|cell| u128::from(cell.baseline_water_units))
                .sum::<u128>()
        {
            return Err(AtlasError::Corrupt(
                "hydrology baseline volume disagrees with dense cells".into(),
            ));
        }
        let order = topological_order(
            &cells
                .values()
                .iter()
                .map(|cell| cell.drainage_receiver)
                .collect::<Vec<_>>(),
        )?;
        if order.len() != cells.len() || terrain.side() != side || cells.side() != side {
            return Err(AtlasError::Corrupt(
                "hydrology grids have invalid dimensions".into(),
            ));
        }
        for (index, cell) in cells.values().iter().enumerate() {
            if cell.drainage_receiver != u32::MAX {
                let receiver = cell.drainage_receiver as usize;
                if !neighbors8_indices(index, side).contains(&receiver) {
                    return Err(AtlasError::Corrupt(
                        "drainage receiver is not a seam-aware neighbor".into(),
                    ));
                }
                if cell.flags & HYDRO_RIVER != 0
                    && cells.values()[receiver].flags & HYDRO_RIVER != 0
                    && cell.channel_bed_elevation + 0.001
                        < cells.values()[receiver].channel_bed_elevation
                {
                    return Err(AtlasError::Corrupt(
                        "river bed climbs in the downstream direction".into(),
                    ));
                }
            }
            if cell.water_body == WaterBodyKind::Ocean
                && terrain.values()[index].eroded_elevation > SEA_LEVEL as f32
            {
                return Err(AtlasError::Corrupt(
                    "ocean classification lies above marine datum".into(),
                ));
            }
        }
        Ok(())
    }
}

fn validate_curve(curve: &[StoragePoint]) -> Result<(), AtlasError> {
    if curve.is_empty()
        || curve.iter().any(|point| !point.elevation.is_finite())
        || curve.windows(2).any(|pair| {
            pair[1].elevation < pair[0].elevation || pair[1].volume_units < pair[0].volume_units
        })
    {
        return Err(AtlasError::Corrupt(
            "reservoir volume/elevation curve is not monotonic".into(),
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtlasHydrologySample {
    pub water_body: WaterBodyKind,
    pub near_channel: bool,
    pub channel_distance_blocks: f32,
    pub channel_width_blocks: f32,
    pub channel_depth_blocks: f32,
    pub channel_bed_elevation: f32,
    pub water_surface_elevation: Option<f32>,
    pub discharge: f32,
    pub sediment_energy: f32,
    pub salinity: u8,
    pub flags: u16,
    pub river_id: u32,
    pub lake_basin_id: u32,
    pub ocean_basin_id: u16,
}

impl Default for AtlasHydrologySample {
    fn default() -> Self {
        Self {
            water_body: WaterBodyKind::Land,
            near_channel: false,
            channel_distance_blocks: f32::INFINITY,
            channel_width_blocks: 0.0,
            channel_depth_blocks: 0.0,
            channel_bed_elevation: SEA_LEVEL as f32,
            water_surface_elevation: None,
            discharge: 0.0,
            sediment_energy: 0.0,
            salinity: 0,
            flags: 0,
            river_id: 0,
            lake_basin_id: 0,
            ocean_basin_id: 0,
        }
    }
}

fn segment_distance(
    point: SurfacePoint,
    a: SurfacePoint,
    b: SurfacePoint,
    lateral_offset_blocks: f32,
) -> (f32, f32) {
    let q = surface_to_unit(point);
    let a = surface_to_unit(a);
    let b = surface_to_unit(b);
    let chord = b - a;
    let length_squared = chord.length_squared();
    let t = if length_squared <= f64::EPSILON {
        0.0
    } else {
        ((q - a).dot(chord) / length_squared).clamp(0.0, 1.0)
    };
    let nearest = (a + chord * t).normalize_or_zero();
    // Floodplain reaches bow away from the atlas chord but return exactly to
    // both declared endpoints. This gives sub-atlas meanders without moving a
    // confluence, mouth, lake outlet, chunk seam, or cube-face crossing.
    let tangent = (chord - nearest * chord.dot(nearest)).normalize_or_zero();
    let lateral = nearest.cross(tangent).normalize_or_zero();
    let bend = f64::from(lateral_offset_blocks) * (std::f64::consts::PI * t).sin() / PLANET_RADIUS;
    let curved = (nearest * bend.cos() + lateral * bend.sin()).normalize_or_zero();
    let angle = q.dot(curved).clamp(-1.0, 1.0).acos();
    ((angle * PLANET_RADIUS) as f32, t as f32)
}

impl PlanetAtlas {
    /// Exact generator-facing surface-water constraint.  The query considers
    /// nearby atlas edges in planet space, so one river centerline is shared
    /// by both chunks and both cube-face charts at a seam.
    pub fn hydrology_sample(&self, point: SurfacePoint) -> AtlasHydrologySample {
        let home = AtlasPos::from_surface(
            SurfacePos::new(
                point.face,
                point.u.floor().clamp(0.0, f64::from(FACE_BLOCKS - 1)) as u16,
                point.v.floor().clamp(0.0, f64::from(FACE_BLOCKS - 1)) as u16,
            )
            .expect("clamped hydrology query"),
            self.side(),
        );
        let home_cell = self
            .genesis
            .hydrology
            .get(home)
            .expect("validated hydrology query");
        if matches!(
            home_cell.water_body,
            WaterBodyKind::Ocean | WaterBodyKind::Lake | WaterBodyKind::Playa
        ) {
            let depth = f32::from(home_cell.channel_depth_centiblocks) / 100.0;
            return AtlasHydrologySample {
                water_body: home_cell.water_body,
                near_channel: true,
                channel_distance_blocks: 0.0,
                channel_width_blocks: f32::from(self.cell_blocks()),
                channel_depth_blocks: depth,
                channel_bed_elevation: home_cell.channel_bed_elevation,
                water_surface_elevation: (home_cell.baseline_water_units > 0)
                    .then_some(home_cell.water_surface_elevation),
                discharge: home_cell.mean_discharge,
                sediment_energy: f32::from(home_cell.sediment_energy) / 65_535.0,
                salinity: home_cell.salinity,
                flags: home_cell.flags,
                river_id: home_cell.river_id,
                lake_basin_id: home_cell.lake_basin_id,
                ocean_basin_id: home_cell.ocean_basin_id,
            };
        }
        let mut best = AtlasHydrologySample {
            flags: home_cell.flags,
            water_body: home_cell.water_body,
            ..AtlasHydrologySample::default()
        };
        let mut best_score = f32::INFINITY;
        for candidate in self.bounded_stencil(home, 2, 32) {
            let cell = self
                .genesis
                .hydrology
                .get(candidate)
                .expect("hydrology stencil");
            if cell.flags & HYDRO_RIVER == 0 || cell.drainage_receiver == u32::MAX {
                continue;
            }
            let Some(receiver) = AtlasPos::from_index(cell.drainage_receiver as usize, self.side())
            else {
                continue;
            };
            let width = f32::from(cell.channel_width_centiblocks) / 100.0;
            let segment_key = (candidate.index(self.side()) as u64).rotate_left(23)
                ^ receiver.index(self.side()) as u64;
            let direction = if mix64(segment_key) & 1 == 0 {
                -1.0
            } else {
                1.0
            };
            let meander = if cell.flags & HYDRO_FLOODPLAIN != 0 {
                direction * (width * 1.4).clamp(1.5, 24.0)
            } else {
                0.0
            };
            let (distance, _) = segment_distance(
                point,
                candidate.center(self.side()),
                receiver.center(self.side()),
                meander,
            );
            let score = distance / width.max(1.0);
            if score > 2.2
                || (score > best_score)
                || (score == best_score && cell.mean_discharge <= best.discharge)
            {
                continue;
            }
            best_score = score;
            let inside = distance <= width * 0.5;
            best = AtlasHydrologySample {
                water_body: if inside {
                    cell.water_body
                } else {
                    WaterBodyKind::Land
                },
                near_channel: true,
                channel_distance_blocks: distance,
                channel_width_blocks: width,
                channel_depth_blocks: f32::from(cell.channel_depth_centiblocks) / 100.0,
                channel_bed_elevation: cell.channel_bed_elevation,
                water_surface_elevation: (inside && cell.baseline_water_units > 0)
                    .then_some(cell.water_surface_elevation),
                discharge: cell.mean_discharge,
                sediment_energy: f32::from(cell.sediment_energy) / 65_535.0,
                salinity: cell.salinity,
                flags: cell.flags,
                river_id: cell.river_id,
                lake_basin_id: cell.lake_basin_id,
                ocean_basin_id: cell.ocean_basin_id,
            };
        }
        best
    }

    pub fn hydrological_name_at(&self, surface: SurfacePos) -> Option<&str> {
        let cell = self.genesis.hydrology.get(self.atlas_pos(surface))?;
        if cell.river_id != 0 {
            return self
                .hydrology
                .rivers
                .iter()
                .find(|river| river.id == cell.river_id)
                .map(|river| river.name.as_str());
        }
        if cell.lake_basin_id != 0 {
            return self
                .hydrology
                .lakes
                .iter()
                .find(|lake| lake.id == cell.lake_basin_id)
                .map(|lake| lake.name.as_str());
        }
        if cell.ocean_basin_id != 0 {
            return self
                .hydrology
                .oceans
                .iter()
                .find(|ocean| ocean.id == cell.ocean_basin_id)
                .map(|ocean| ocean.name.as_str());
        }
        self.hydrology
            .watersheds
            .iter()
            .find(|watershed| watershed.id == cell.watershed_id)
            .map(|watershed| watershed.name.as_str())
    }
}

pub(super) fn route_placer_deposits(
    side: u16,
    terrain: &AtlasGrid<TerrainCell>,
    tectonics: &AtlasGrid<TectonicCell>,
    hydrology: &AtlasGrid<HydrologyCell>,
    resources: &mut AtlasGrid<ResourceCell>,
    geology: &mut GeologyModel,
) {
    let sources: Vec<(usize, DepositRecord)> = geology
        .deposits
        .iter()
        .cloned()
        .enumerate()
        .filter(|(_, site)| matches!(site.mineral, MineralKind::Gold | MineralKind::RareEarth))
        .collect();
    for (source_index, source) in sources {
        // Divert one block of the source's per-chunk extraction ceiling into
        // a small downstream placer. Primary tonnage is generated exactly at
        // its declared ceiling, so looking only for nonexistent "excess"
        // would make this route unreachable.
        let retained_quota = source.max_blocks_per_chunk.saturating_sub(1);
        if retained_quota == 0 {
            continue;
        }
        let minimum_source_budget =
            u64::from(retained_quota) * u64::from(source.eligible_chunk_upper_bound);
        let available = source.tonnage_blocks.saturating_sub(minimum_source_budget);
        if available < 64 {
            continue;
        }
        let mut at = source.pos;
        let mut target = None;
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..usize::from(side).saturating_mul(6) {
            if !seen.insert(at) {
                break;
            }
            let cell = hydrology.get(at).expect("placer drainage");
            if at != source.pos
                && cell.flags & (HYDRO_FLOODPLAIN | HYDRO_DELTA) != 0
                && terrain
                    .get(at)
                    .is_some_and(|terrain| terrain.eroded_elevation > SEA_LEVEL as f32)
            {
                target = Some(at);
                break;
            }
            let Some(next) = (cell.drainage_receiver != u32::MAX)
                .then(|| AtlasPos::from_index(cell.drainage_receiver as usize, side))
                .flatten()
            else {
                break;
            };
            at = next;
        }
        let Some(target) = target else {
            continue;
        };
        let upper = 32u32;
        let per_chunk = 2u16;
        let placer_tonnage = available.min(u64::from(upper) * u64::from(per_chunk));
        if placer_tonnage < u64::from(upper) * u64::from(per_chunk) {
            continue;
        }
        geology.deposits[source_index].max_blocks_per_chunk = retained_quota;
        geology.deposits[source_index].tonnage_blocks -= placer_tonnage;
        let id = geology.deposits.len() as u32 + 1;
        let target_tectonics = tectonics.get(target).expect("placer host");
        geology.deposits.push(DepositRecord {
            id,
            mineral: source.mineral,
            pos: target,
            host: BedrockFamily::from_id(target_tectonics.bedrock_family),
            geological_province: target_tectonics.geological_province,
            landmass_id: terrain.get(target).expect("placer terrain").landmass_id,
            source_body_id: source.id,
            radius_blocks: 72,
            depth_min: SEA_LEVEL.max(1) as u16,
            depth_max: (SEA_LEVEL + 18) as u16,
            grade_ppm: source.grade_ppm.saturating_div(3).max(1),
            tonnage_blocks: placer_tonnage,
            max_blocks_per_chunk: per_chunk,
            eligible_chunk_upper_bound: upper,
        });
        let resource = resources.get_mut(target).expect("placer resource cell");
        if resource.deposit_site_ref == 0 {
            resource.deposit_site_ref = id;
        }
        resource.deposit_site_count = resource.deposit_site_count.saturating_add(1);
    }
}
