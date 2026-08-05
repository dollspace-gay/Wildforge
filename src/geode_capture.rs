//! Deterministic, ledger-honest preparation for the cracked-geode visual gate.
//!
//! Ordinary play never calls this module. The command-line entry points are
//! explicit development tools which either inspect immutable inputs or create
//! a disposable qualification save beside a named sealed source.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::chunk::{CHUNK_X, CHUNK_Y, CHUNK_Z, Chunk, SEA_LEVEL};
use crate::entity::ItemEntity;
use crate::inventory::{Inventory, ItemStack};
use crate::planet::{BlockPos, ChunkPos, FACE_BLOCKS, Face, SurfacePos};
use crate::planet_atlas::{BedrockFamily, DepositRecord, MineralKind, PlanetAtlas};
use crate::registry::{AIR, BlockId, Registry};
use crate::world::{self, World};
use crate::worldgen::Generator;

pub const GEODE_SITE_SCHEMA_VERSION: u32 = 1;
pub const GEODE_PREPARATION_SCHEMA_VERSION: u32 = 1;
const SELECTION_WORLD: &str = "visual-polish-strata-baseline";
const SEALED_WORLD_NAME: &str = "visual-polish-geode-sealed";
const OPENED_WORLD_NAME: &str = "visual-polish-geode-opened";
const EXTERNAL_SOURCE: &str = "cracked-geode qualification kit";

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
enum ApertureDirection {
    PositiveU,
    NegativeU,
    PositiveV,
    NegativeV,
}

impl ApertureDirection {
    const ALL: [Self; 4] = [
        Self::PositiveU,
        Self::NegativeU,
        Self::PositiveV,
        Self::NegativeV,
    ];

    const fn delta(self) -> (i32, i32) {
        match self {
            Self::PositiveU => (1, 0),
            Self::NegativeU => (-1, 0),
            Self::PositiveV => (0, 1),
            Self::NegativeV => (0, -1),
        }
    }

    const fn tangent(self) -> (i32, i32) {
        let (du, dv) = self.delta();
        (-dv, du)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedBlock {
    face: String,
    u: u16,
    y: u8,
    v: u16,
    block: String,
}

impl RecordedBlock {
    fn position(&self) -> Result<BlockPos, String> {
        let face =
            Face::from_name(&self.face).ok_or_else(|| format!("unknown face {}", self.face))?;
        BlockPos::new(face, self.u, self.y, self.v).map_err(|error| error.to_string())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GeodeSiteRecord {
    pub schema_version: u32,
    pub site_id: String,
    pub source_world: String,
    pub selection_world: String,
    pub seed: u32,
    pub generator_version: u32,
    pub atlas_format_version: u32,
    pub atlas_algorithm_version: u32,
    pub atlas_content_hash: String,
    pub atlas_genesis_checksum: String,
    pub deposit_id: u32,
    pub host_geology: String,
    pub finite_quartz_units: u64,
    pub finite_amethyst_units: u64,
    pub face: String,
    pub chunk_u: u16,
    pub chunk_v: u16,
    pub local_cx: usize,
    pub local_cy: i32,
    pub local_cz: usize,
    pub radius: i32,
    pub center_u: u16,
    pub center_y: u8,
    pub center_v: u16,
    pub surface_y: i32,
    pub depth_below_surface: i32,
    pub shell_blocks: u32,
    pub lining_quartz_blocks: u32,
    pub lining_amethyst_blocks: u32,
    pub heart_air_blocks: u32,
    pub surrounding_host_blocks: u32,
    pub accidental_air_blocks: u32,
    pub shell_six_connected: bool,
    pub heart_sealed: bool,
    pub shortest_solid_path: u32,
    aperture_direction: ApertureDirection,
    pub tunnel_host_blocks: u32,
    pub shell_depth_blocks: u32,
    pub camera_face: String,
    pub camera_u: f32,
    pub camera_y: f32,
    pub camera_v: f32,
    pub camera_local_u: f32,
    pub camera_local_v: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub context_camera_u: f32,
    pub context_camera_y: f32,
    pub context_camera_v: f32,
    pub context_camera_local_u: f32,
    pub context_camera_local_v: f32,
    pub context_yaw: f32,
    pub context_pitch: f32,
    pub torch: RecordedBlock,
    pub planned_break: Vec<RecordedBlock>,
    pub view_ray: Vec<RecordedBlock>,
    pub estimated_host_pixels: u64,
    pub estimated_quartz_pixels: u64,
    pub estimated_amethyst_pixels: u64,
    pub estimated_heart_pixels: u64,
    pub neighborhood_sha256: String,
    pub scanned_deposits: usize,
    pub generated_chunks: usize,
    pub elapsed_millis: u128,
    pub peak_rss_bytes: u64,
}

#[derive(Clone, Debug)]
struct Candidate {
    score: i64,
    record: GeodeSiteRecord,
}

struct AperturePlan {
    direction: ApertureDirection,
    planned_break: Vec<RecordedBlock>,
    view_ray: Vec<RecordedBlock>,
    torch: RecordedBlock,
    tunnel_host_blocks: u32,
    shortest_solid_path: u32,
    camera_u: f32,
    camera_y: f32,
    camera_v: f32,
    yaw: f32,
    pitch: f32,
    context_camera_u: f32,
    context_camera_y: f32,
    context_camera_v: f32,
    context_yaw: f32,
    context_pitch: f32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct OperationRecord {
    sequence: u32,
    kind: String,
    target: RecordedBlock,
    result: String,
    tool_durability_after: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GeodePreparationReport {
    schema_version: u32,
    site_id: String,
    source_world: String,
    opened_world: String,
    source_manifest_sha256: String,
    source_material_audit_sha256: String,
    source_neighborhood_sha256: String,
    source_unchanged: bool,
    before_material_audit_sha256: String,
    after_material_audit_sha256: String,
    reload_material_audit_sha256: String,
    before_balanced: bool,
    after_balanced: bool,
    reload_balanced: bool,
    deposit_id: u32,
    reservation_deposit_ids: Vec<u32>,
    quartz_reserved_blocks: u32,
    amethyst_reserved_blocks: u32,
    quartz_extracted_units: u64,
    amethyst_extracted_units: u64,
    planned_breaks: usize,
    applied_breaks: usize,
    torch_placed: bool,
    unexpected_edits: usize,
    remaining_shell_blocks: u32,
    remaining_lining_blocks: u32,
    remaining_heart_air_blocks: u32,
    loose_drop_stacks: usize,
    save_succeeded: bool,
    reload_succeeded: bool,
    operation: Vec<OperationRecord>,
}

fn face_name(face: Face) -> String {
    face.name().to_string()
}

fn block_record(pos: BlockPos, block: BlockId, reg: &Registry) -> RecordedBlock {
    RecordedBlock {
        face: face_name(pos.face()),
        u: pos.u(),
        y: pos.y(),
        v: pos.v(),
        block: reg.block(block).name.clone(),
    }
}

fn block_pos(chunk: ChunkPos, x: i32, y: i32, z: i32) -> Option<BlockPos> {
    if !(0..CHUNK_X as i32).contains(&x)
        || !(0..CHUNK_Z as i32).contains(&z)
        || !(0..CHUNK_Y as i32).contains(&y)
    {
        return None;
    }
    BlockPos::new(
        chunk.face(),
        chunk.u() * CHUNK_X as u16 + x as u16,
        y as u8,
        chunk.v() * CHUNK_Z as u16 + z as u16,
    )
    .ok()
}

fn neighborhood_block(
    chunks: &BTreeMap<ChunkPos, Chunk>,
    face: Face,
    u: i32,
    y: i32,
    v: i32,
) -> Option<(BlockPos, BlockId)> {
    if !(0..i32::from(FACE_BLOCKS)).contains(&u)
        || !(0..CHUNK_Y as i32).contains(&y)
        || !(0..i32::from(FACE_BLOCKS)).contains(&v)
    {
        return None;
    }
    let pos = BlockPos::new(face, u as u16, y as u8, v as u16).ok()?;
    let chunk = chunks.get(&pos.chunk())?;
    Some((
        pos,
        chunk.get(
            usize::from(pos.u() % CHUNK_X as u16),
            y as usize,
            usize::from(pos.v() % CHUNK_Z as u16),
        ),
    ))
}

fn read_site(path: &Path) -> Result<GeodeSiteRecord, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("read geode site {}: {error}", path.display()))?;
    let record: GeodeSiteRecord = toml::from_str(&text)
        .map_err(|error| format!("parse geode site {}: {error}", path.display()))?;
    if record.schema_version != GEODE_SITE_SCHEMA_VERSION || record.site_id != "cracked-geode" {
        return Err("geode site record has an unsupported identity".into());
    }
    Ok(record)
}

fn load_locator_inputs(
    world_or_atlas: &Path,
    mods: &Path,
) -> Result<(Arc<Registry>, Arc<PlanetAtlas>), String> {
    let registry = Arc::new(crate::registry::load(mods));
    if !registry.material_errors.is_empty() {
        return Err(format!(
            "content material accounting failed:\n{}",
            registry.material_errors.join("\n")
        ));
    }
    let atlas = Arc::new(PlanetAtlas::load(world_or_atlas).map_err(|error| {
        format!(
            "load production atlas {}: {error}",
            world_or_atlas.display()
        )
    })?);
    Ok((registry, atlas))
}

fn generated_chunk(generator: &Generator, reg: &Registry, chunk: ChunkPos) -> Chunk {
    generator.generate(chunk, reg)
}

fn sphere_counts(
    chunk: &Chunk,
    reg: &Registry,
    cx: i32,
    cy: i32,
    cz: i32,
    radius: i32,
) -> (u32, u32, u32, u32, u32, u32) {
    let quartz = reg.block_id("base:quartz_block").unwrap_or(AIR);
    let amethyst = reg.block_id("base:amethyst_block").unwrap_or(AIR);
    let limestone = reg.block_id("base:limestone").unwrap_or(AIR);
    let marble = reg.block_id("base:marble").unwrap_or(AIR);
    let mut shell = 0;
    let mut lining_quartz = 0;
    let mut lining_amethyst = 0;
    let mut heart_air = 0;
    let mut host = 0;
    let mut accidental_air = 0;
    for dx in -(radius + 2)..=(radius + 2) {
        for dy in -(radius + 2)..=(radius + 2) {
            for dz in -(radius + 2)..=(radius + 2) {
                let (x, y, z) = (cx + dx, cy + dy, cz + dz);
                if !(0..CHUNK_X as i32).contains(&x)
                    || !(0..CHUNK_Y as i32).contains(&y)
                    || !(0..CHUNK_Z as i32).contains(&z)
                {
                    continue;
                }
                let block = chunk.get(x as usize, y as usize, z as usize);
                let d2 = dx * dx + dy * dy + dz * dz;
                if d2 <= radius * radius {
                    if d2 >= (radius - 1) * (radius - 1) {
                        shell += u32::from(block == quartz);
                        accidental_air += u32::from(block == AIR);
                    } else if d2 > (radius - 2) * (radius - 2) {
                        lining_quartz += u32::from(block == quartz);
                        lining_amethyst += u32::from(block == amethyst);
                        accidental_air += u32::from(block == AIR);
                    } else {
                        heart_air += u32::from(block == AIR);
                    }
                } else if d2 <= (radius + 2) * (radius + 2)
                    && (block == limestone || block == marble)
                {
                    host += 1;
                }
            }
        }
    }
    (
        shell,
        lining_quartz,
        lining_amethyst,
        heart_air,
        host,
        accidental_air,
    )
}

fn heart_is_sealed(chunk: &Chunk, cx: i32, cy: i32, cz: i32, radius: i32) -> bool {
    let mut queue = VecDeque::from([(cx, cy, cz)]);
    let mut seen = BTreeSet::from([(cx, cy, cz)]);
    while let Some((x, y, z)) = queue.pop_front() {
        if x == 0
            || z == 0
            || y == 0
            || x == CHUNK_X as i32 - 1
            || z == CHUNK_Z as i32 - 1
            || y == CHUNK_Y as i32 - 1
        {
            return false;
        }
        for (dx, dy, dz) in [
            (1, 0, 0),
            (-1, 0, 0),
            (0, 1, 0),
            (0, -1, 0),
            (0, 0, 1),
            (0, 0, -1),
        ] {
            let next = (x + dx, y + dy, z + dz);
            if (next.0 - cx).abs() > radius + 1
                || (next.1 - cy).abs() > radius + 1
                || (next.2 - cz).abs() > radius + 1
                || !seen.insert(next)
            {
                continue;
            }
            if chunk.get(next.0 as usize, next.1 as usize, next.2 as usize) == AIR {
                queue.push_back(next);
            }
        }
    }
    true
}

fn shell_is_connected(
    chunk: &Chunk,
    reg: &Registry,
    cx: i32,
    cy: i32,
    cz: i32,
    radius: i32,
) -> bool {
    let quartz = reg.block_id("base:quartz_block").unwrap_or(AIR);
    let mut shell = BTreeSet::new();
    for dx in -radius..=radius {
        for dy in -radius..=radius {
            for dz in -radius..=radius {
                let d2 = dx * dx + dy * dy + dz * dz;
                if d2 >= (radius - 1) * (radius - 1)
                    && d2 <= radius * radius
                    && chunk.get((cx + dx) as usize, (cy + dy) as usize, (cz + dz) as usize)
                        == quartz
                {
                    shell.insert((cx + dx, cy + dy, cz + dz));
                }
            }
        }
    }
    let Some(first) = shell.iter().next().copied() else {
        return false;
    };
    let mut reached = BTreeSet::from([first]);
    let mut queue = VecDeque::from([first]);
    while let Some((x, y, z)) = queue.pop_front() {
        for (dx, dy, dz) in [
            (1, 0, 0),
            (-1, 0, 0),
            (0, 1, 0),
            (0, -1, 0),
            (0, 0, 1),
            (0, 0, -1),
        ] {
            let next = (x + dx, y + dy, z + dz);
            if shell.contains(&next) && reached.insert(next) {
                queue.push_back(next);
            }
        }
    }
    reached.len() == shell.len()
}

fn is_host(block: BlockId, reg: &Registry) -> bool {
    matches!(
        reg.block(block).name.as_str(),
        "base:limestone" | "base:marble"
    )
}

fn excavation_would_seep(atlas: &PlanetAtlas, pos: BlockPos) -> bool {
    let atlas_pos = atlas.atlas_pos(pos.surface());
    let index = atlas_pos.index(atlas.side());
    let hydro = atlas.genesis.hydrology.values()[index];
    let ground = atlas.genesis.ground.values()[index];
    let water = atlas.water_cycle.cells.values()[index];
    let below_ocean = hydro.ocean_basin_id != 0 && i32::from(pos.y()) <= SEA_LEVEL;
    let below_water_table = water.groundwater.water_hu
        >= crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL
        && water.groundwater_head_milliblocks > i32::from(pos.y()) * 1_000;
    below_ocean || (below_water_table && ground.aquifer_permeability >= 8_192)
}

fn is_country_rock(block: BlockId, reg: &Registry) -> bool {
    matches!(
        reg.block(block).name.as_str(),
        "base:stone"
            | "base:sandstone"
            | "base:limestone"
            | "base:shale"
            | "base:granite"
            | "base:marble"
            | "base:slate"
            | "base:quartzite"
            | "base:basalt"
    )
}

fn plan_aperture(
    chunk: ChunkPos,
    chunks: &BTreeMap<ChunkPos, Chunk>,
    reg: &Registry,
    cx: i32,
    cy: i32,
    cz: i32,
    radius: i32,
) -> Option<AperturePlan> {
    let center_u = i32::from(chunk.u() * CHUNK_X as u16) + cx;
    let center_v = i32::from(chunk.v() * CHUNK_Z as u16) + cz;
    for direction in ApertureDirection::ALL {
        let (du, dv) = direction.delta();
        let (tu, tv) = direction.tangent();
        let face_step = radius - 1;
        for air_step in (radius + 2)..=(radius + 24) {
            let mut alcove = true;
            // One ordinary player-width cave lane is sufficient for the
            // untouched sealed-context camera. Preparation widens the second
            // lane through normal mining; requiring a naturally occurring
            // two-by-two-by-three studio alcove rejected every finite geode
            // in the production atlas.
            for along in air_step..=air_step + 1 {
                for height in 0..=1 {
                    let x = center_u + du * along;
                    let z = center_v + dv * along;
                    let y = cy + height;
                    alcove &= neighborhood_block(chunks, chunk.face(), x, y, z)
                        .is_some_and(|(_, block)| block == AIR);
                }
            }
            let floor_x = center_u + du * (air_step + 1);
            let floor_z = center_v + dv * (air_step + 1);
            alcove &= neighborhood_block(chunks, chunk.face(), floor_x, cy - 1, floor_z)
                .is_some_and(|(_, block)| reg.is_solid(block));
            if !alcove {
                continue;
            }
            let mut planned = Vec::new();
            let mut valid = true;
            let mut host_blocks = 0u32;
            for along in (0..air_step).rev() {
                for transverse in -1..=0 {
                    for height in 0..=1 {
                        let x = center_u + du * along + tu * transverse;
                        let z = center_v + dv * along + tv * transverse;
                        let y = cy + height;
                        let Some((pos, block)) = neighborhood_block(chunks, chunk.face(), x, y, z)
                        else {
                            valid = false;
                            continue;
                        };
                        if along > radius {
                            if block == AIR {
                                // A larger or irregular natural cave simply
                                // reduces how many host blocks must be mined.
                            } else if is_host(block, reg) {
                                host_blocks += 1;
                            } else {
                                valid = false;
                            }
                        }
                        if block != AIR {
                            planned.push(block_record(pos, block, reg));
                        }
                    }
                }
            }
            let shell_name = "base:quartz_block";
            let shell_depth = planned
                .iter()
                .filter(|block| block.block == shell_name || block.block == "base:amethyst_block")
                .map(|block| {
                    (i32::from(block.u) - center_u)
                        .abs()
                        .max((i32::from(block.v) - center_v).abs())
                })
                .max()
                .unwrap_or_default()
                .saturating_sub(radius - 3) as u32;
            let has_shell = planned.iter().any(|block| block.block == shell_name);
            let has_lining = planned
                .iter()
                .any(|block| block.block == "base:amethyst_block")
                || planned
                    .iter()
                    .filter(|block| block.block == shell_name)
                    .count()
                    >= 2;
            if !valid
                || !(3..=20).contains(&(air_step - face_step - 1))
                || !has_shell
                || !has_lining
                || shell_depth > 3
            {
                continue;
            }
            let torch_x = center_u + du * (face_step + 2) - tu;
            let torch_z = center_v + dv * (face_step + 2) - tv;
            let Some((torch_pos, torch_cell)) =
                neighborhood_block(chunks, chunk.face(), torch_x, cy, torch_z)
            else {
                continue;
            };
            let Some((_, floor)) =
                neighborhood_block(chunks, chunk.face(), torch_x, cy - 1, torch_z)
            else {
                continue;
            };
            if !reg.is_solid(floor)
                || (torch_cell != AIR
                    && !planned
                        .iter()
                        .any(|block| block.position().ok() == Some(torch_pos)))
            {
                continue;
            }
            let mut view_ray = Vec::new();
            for along in (0..=air_step).rev() {
                let x = center_u + du * along;
                let z = center_v + dv * along;
                let Some((pos, block)) = neighborhood_block(chunks, chunk.face(), x, cy, z) else {
                    continue;
                };
                view_ray.push(block_record(pos, block, reg));
            }
            let camera_u = (center_u + du * (air_step + 1)) as f32 + 0.5;
            let camera_v = (center_v + dv * (air_step + 1)) as f32 + 0.5;
            let yaw = (-dv as f32).atan2(-du as f32);
            return Some(AperturePlan {
                direction,
                planned_break: planned,
                view_ray,
                torch: RecordedBlock {
                    face: face_name(chunk.face()),
                    u: torch_pos.u(),
                    y: torch_pos.y(),
                    v: torch_pos.v(),
                    block: "base:torch".into(),
                },
                tunnel_host_blocks: host_blocks,
                shortest_solid_path: host_blocks / 4,
                camera_u,
                camera_y: cy as f32 - 1.12,
                camera_v,
                yaw,
                pitch: -0.04,
                context_camera_u: camera_u,
                context_camera_y: cy as f32 - 1.12,
                context_camera_v: camera_v,
                context_yaw: yaw,
                context_pitch: -0.04,
            });
        }
    }
    None
}

/// Fallback for a sealed geode with no naturally aligned cave. The approach
/// remains honest terrain: a two-wide shaft reaches a short horizontal
/// discovery tunnel, and every removed voxel is recorded for authoritative
/// mining. The untouched context camera stays above the same shaft so the
/// sealed and reloaded frames can retain identical camera parameters.
fn plan_vertical_aperture(
    chunk: ChunkPos,
    chunks: &BTreeMap<ChunkPos, Chunk>,
    reg: &Registry,
    cx: i32,
    cy: i32,
    cz: i32,
    radius: i32,
) -> Option<AperturePlan> {
    let center_u = i32::from(chunk.u() * CHUNK_X as u16) + cx;
    let center_v = i32::from(chunk.v() * CHUNK_Z as u16) + cz;
    for direction in ApertureDirection::ALL {
        let (du, dv) = direction.delta();
        let (tu, tv) = direction.tangent();
        let face_step = radius - 1;
        let shaft_step = radius + 5;
        let mut column_tops = Vec::new();
        for transverse in -1..=0 {
            let u = center_u + du * shaft_step + tu * transverse;
            let v = center_v + dv * shaft_step + tv * transverse;
            let top = (cy..CHUNK_Y as i32).rev().find(|y| {
                neighborhood_block(chunks, chunk.face(), u, *y, v)
                    .is_some_and(|(_, block)| reg.is_solid(block))
            })?;
            column_tops.push((transverse, u, v, top));
        }
        let surface_y = column_tops.iter().map(|(_, _, _, top)| *top).max()?;
        if surface_y - cy > 96 {
            if std::env::var_os("WILDFORGE_GEODE_DEBUG").is_some() {
                eprintln!("vertical {direction:?} rejected: depth {}", surface_y - cy);
            }
            continue;
        }
        // A dry block directly above the mouth is insufficient: a shoreline
        // cell one step to the side will flow down the new shaft on the next
        // authoritative fluid tick. Reject a small, fixed surface halo so the
        // locator cannot select a reveal whose exact edit set changes merely
        // by saving and reloading it.
        let flooded_mouth = column_tops.iter().any(|(_, u, v, top)| {
            (-2..=2).any(|around_u| {
                (-2..=2).any(|around_v| {
                    ((*top - 1).max(0)..=(*top + 2).min(CHUNK_Y as i32 - 1)).any(|y| {
                        neighborhood_block(chunks, chunk.face(), *u + around_u, y, *v + around_v)
                            .is_some_and(|(_, block)| reg.fluid_volume(block).is_some())
                    })
                })
            })
        });
        if flooded_mouth {
            if std::env::var_os("WILDFORGE_GEODE_DEBUG").is_some() {
                eprintln!("vertical {direction:?} rejected: shaft mouth is submerged");
            }
            continue;
        }

        let mut planned = Vec::new();
        let mut seen = BTreeSet::new();
        let mut valid = true;
        let mut invalid_reasons = BTreeSet::new();
        for (_, u, v, top) in &column_tops {
            for y in (cy..=*top).rev() {
                let Some((pos, block)) = neighborhood_block(chunks, chunk.face(), *u, y, *v) else {
                    valid = false;
                    invalid_reasons.insert("shaft-outside-neighborhood".to_string());
                    continue;
                };
                if block == AIR {
                    continue;
                }
                let definition = reg.block(block);
                if definition.hardness.is_none()
                    || reg.fluid_volume(block).is_some()
                    || matches!(
                        definition.name.as_str(),
                        "base:bedrock" | "base:quartz_block" | "base:amethyst_block"
                    )
                {
                    valid = false;
                    invalid_reasons.insert(format!("shaft:{}", definition.name));
                    continue;
                }
                if seen.insert(pos) {
                    planned.push(block_record(pos, block, reg));
                }
            }
        }
        let mut host_blocks = 0u32;
        for along in (0..shaft_step).rev() {
            for transverse in -1..=0 {
                for height in 0..=1 {
                    let u = center_u + du * along + tu * transverse;
                    let v = center_v + dv * along + tv * transverse;
                    let y = cy + height;
                    let Some((pos, block)) = neighborhood_block(chunks, chunk.face(), u, y, v)
                    else {
                        valid = false;
                        invalid_reasons.insert("tunnel-outside-neighborhood".to_string());
                        continue;
                    };
                    if along > radius && block != AIR {
                        if is_country_rock(block, reg) {
                            host_blocks += 1;
                        } else {
                            valid = false;
                            invalid_reasons.insert(format!("tunnel:{}", reg.block(block).name));
                        }
                    }
                    if height == 1
                        && neighborhood_block(chunks, chunk.face(), u, y + 1, v)
                            .is_some_and(|(_, above)| reg.block(above).falls)
                    {
                        valid = false;
                        invalid_reasons.insert("unsupported-falling-roof".to_string());
                    }
                    if block != AIR && seen.insert(pos) {
                        planned.push(block_record(pos, block, reg));
                    }
                }
            }
        }
        let shell_count = planned
            .iter()
            .filter(|block| block.block == "base:quartz_block")
            .count();
        let exposes_lining = planned
            .iter()
            .any(|block| block.block == "base:amethyst_block")
            || shell_count >= 2;
        if !valid || shell_count == 0 || !exposes_lining {
            if std::env::var_os("WILDFORGE_GEODE_DEBUG").is_some() {
                eprintln!(
                    "vertical {direction:?} rejected: valid={valid} shell={shell_count} exposes_lining={exposes_lining} surface={surface_y} cy={cy} reasons={invalid_reasons:?}"
                );
            }
            continue;
        }
        let torch_u = center_u + du * (face_step + 2) - tu;
        let torch_v = center_v + dv * (face_step + 2) - tv;
        let Some((torch_pos, torch_cell)) =
            neighborhood_block(chunks, chunk.face(), torch_u, cy, torch_v)
        else {
            continue;
        };
        let Some((_, torch_floor)) =
            neighborhood_block(chunks, chunk.face(), torch_u, cy - 1, torch_v)
        else {
            continue;
        };
        if !reg.is_solid(torch_floor) || (torch_cell != AIR && !seen.contains(&torch_pos)) {
            continue;
        }
        let mut view_ray = Vec::new();
        for along in (0..=shaft_step).rev() {
            let u = center_u + du * along;
            let v = center_v + dv * along;
            if let Some((pos, block)) = neighborhood_block(chunks, chunk.face(), u, cy, v) {
                view_ray.push(block_record(pos, block, reg));
            }
        }
        let camera_u = (center_u + du * shaft_step) as f32 + 0.5;
        let camera_v = (center_v + dv * shaft_step) as f32 + 0.5;
        let yaw = (-dv as f32).atan2(-du as f32);
        return Some(AperturePlan {
            direction,
            planned_break: planned,
            view_ray,
            torch: RecordedBlock {
                face: face_name(chunk.face()),
                u: torch_pos.u(),
                y: torch_pos.y(),
                v: torch_pos.v(),
                block: "base:torch".into(),
            },
            tunnel_host_blocks: host_blocks,
            shortest_solid_path: (surface_y - cy) as u32 + host_blocks / 4,
            camera_u,
            camera_y: cy as f32 + 0.02,
            camera_v,
            yaw,
            pitch: -0.12,
            context_camera_u: camera_u,
            context_camera_y: surface_y as f32 + 1.02,
            context_camera_v: camera_v,
            context_yaw: yaw,
            context_pitch: -1.35,
        });
    }
    None
}

fn neighborhood_hash(generator: &Generator, reg: &Registry, center: ChunkPos) -> (String, usize) {
    let mut bytes = Vec::with_capacity(9 * CHUNK_X * CHUNK_Y * CHUNK_Z * 2);
    let mut chunks = 0;
    for du in -1..=1 {
        for dv in -1..=1 {
            let pos = center.offset(du, dv);
            let chunk = generated_chunk(generator, reg, pos);
            bytes.push(pos.face() as u8);
            bytes.extend_from_slice(&pos.u().to_le_bytes());
            bytes.extend_from_slice(&pos.v().to_le_bytes());
            for x in 0..CHUNK_X {
                for z in 0..CHUNK_Z {
                    for y in 0..CHUNK_Y {
                        bytes.extend_from_slice(&chunk.get(x, y, z).0.to_le_bytes());
                    }
                }
            }
            chunks += 1;
        }
    }
    (crate::visual_capture::sha256_hex(&bytes), chunks)
}

fn peak_rss_bytes() -> u64 {
    let Ok(status) = fs::read_to_string("/proc/self/status") else {
        return 0;
    };
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .and_then(|value| value.split_whitespace().next())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or_default()
        .saturating_mul(1024)
}

fn evaluate_candidate(
    deposit: &DepositRecord,
    atlas: &Arc<PlanetAtlas>,
    generator: &Generator,
    reg: &Registry,
) -> Option<Candidate> {
    if deposit.mineral != MineralKind::Geode
        || !matches!(
            deposit.host,
            BedrockFamily::Limestone | BedrockFamily::Marble
        )
    {
        return None;
    }
    let surface = SurfacePos::new(
        deposit.pos.face,
        deposit.pos.u * atlas.cell_blocks() + atlas.cell_blocks() / 2,
        deposit.pos.v * atlas.cell_blocks() + atlas.cell_blocks() / 2,
    )
    .ok()?;
    let chunk_pos = ChunkPos::from_surface(surface);
    let (cx, cz, cy, radius) = generator.geode_at(chunk_pos)?;
    if std::env::var_os("WILDFORGE_GEODE_DEBUG").is_some() {
        let index = deposit.pos.index(atlas.side());
        let ground = atlas.genesis.ground.values()[index];
        let hydro = atlas.genesis.hydrology.values()[index];
        let water = atlas.water_cycle.cells.values()[index];
        let terrain = atlas.genesis.terrain.values()[index];
        eprintln!(
            "geode {} vertical context: cy={cy} radius={radius} terrain={:.2} baseline_head={:.2} dynamic_head={:.2} permeability={} ocean_basin={}",
            deposit.id,
            terrain.eroded_elevation,
            ground.baseline_groundwater_head,
            water.groundwater_head_milliblocks as f32 / 1_000.0,
            ground.aquifer_permeability,
            hydro.ocean_basin_id
        );
    }
    if !(3..=5).contains(&radius)
        || cx as i32 - radius < 0
        || cz as i32 - radius < 0
        || cx + radius as usize >= CHUNK_X
        || cz + radius as usize >= CHUNK_Z
    {
        return None;
    }
    let generated = generated_chunk(generator, reg, chunk_pos);
    let (shell, lining_quartz, lining_amethyst, heart_air, host, accidental_air) =
        sphere_counts(&generated, reg, cx as i32, cy, cz as i32, radius);
    let sealed = heart_is_sealed(&generated, cx as i32, cy, cz as i32, radius);
    let connected = shell_is_connected(&generated, reg, cx as i32, cy, cz as i32, radius);
    if !sealed
        || !connected
        || accidental_air != 0
        || shell < 80
        || lining_amethyst < 12
        || heart_air < 7
        || host < 40
    {
        if std::env::var_os("WILDFORGE_GEODE_DEBUG").is_some() {
            eprintln!(
                "geode {} topology rejected: shell={shell} lining_quartz={lining_quartz} lining_amethyst={lining_amethyst} heart={heart_air} host={host} accidental_air={accidental_air} sealed={sealed} connected={connected}",
                deposit.id
            );
        }
        return None;
    }
    let mut neighborhood = BTreeMap::new();
    for du in -1..=1 {
        for dv in -1..=1 {
            let pos = chunk_pos.offset(du, dv);
            let value = if pos == chunk_pos {
                generated.clone()
            } else {
                generated_chunk(generator, reg, pos)
            };
            neighborhood.insert(pos, value);
        }
    }
    let aperture = plan_aperture(
        chunk_pos,
        &neighborhood,
        reg,
        cx as i32,
        cy,
        cz as i32,
        radius,
    )
    .or_else(|| {
        plan_vertical_aperture(
            chunk_pos,
            &neighborhood,
            reg,
            cx as i32,
            cy,
            cz as i32,
            radius,
        )
    });
    let Some(aperture) = aperture else {
        if std::env::var_os("WILDFORGE_GEODE_DEBUG").is_some() {
            eprintln!("geode {} has no bounded straight aperture", deposit.id);
        }
        return None;
    };
    let seep_risk = aperture
        .planned_break
        .iter()
        .filter_map(|block| block.position().ok())
        .filter(|position| excavation_would_seep(atlas, *position))
        .count();
    if seep_risk != 0 {
        if std::env::var_os("WILDFORGE_GEODE_DEBUG").is_some() {
            eprintln!(
                "geode {} rejected: {seep_risk} planned breaks would open active groundwater",
                deposit.id
            );
        }
        return None;
    }
    let mut surface_y = 0;
    for y in (0..CHUNK_Y).rev() {
        if generated.get(cx, y, cz) != AIR {
            surface_y = y as i32;
            break;
        }
    }
    let center_u = chunk_pos.u() * CHUNK_X as u16 + cx as u16;
    let center_v = chunk_pos.v() * CHUNK_Z as u16 + cz as u16;
    let score = 10_000
        + i64::from(deposit.host == BedrockFamily::Limestone) * 1_000
        + i64::from(radius) * 1_000
        - i64::from(aperture.tunnel_host_blocks.abs_diff(16));
    Some(Candidate {
        score,
        record: GeodeSiteRecord {
            schema_version: GEODE_SITE_SCHEMA_VERSION,
            site_id: "cracked-geode".into(),
            source_world: SEALED_WORLD_NAME.into(),
            selection_world: SELECTION_WORLD.into(),
            seed: atlas.manifest.seed,
            generator_version: world::WORLD_GENERATOR_VERSION,
            atlas_format_version: atlas.manifest.format_version,
            atlas_algorithm_version: atlas.manifest.atlas_algorithm_version,
            atlas_content_hash: format!("{:016x}", atlas.manifest.content_hash),
            atlas_genesis_checksum: format!("{:016x}", atlas.manifest.genesis_checksum),
            deposit_id: deposit.id,
            host_geology: format!("{:?}", deposit.host).to_ascii_lowercase(),
            finite_quartz_units: deposit
                .tonnage_blocks
                .saturating_mul(crate::materials::CANONICAL_INGOT_UNITS),
            finite_amethyst_units: deposit
                .tonnage_blocks
                .saturating_mul(crate::materials::CANONICAL_INGOT_UNITS),
            face: face_name(chunk_pos.face()),
            chunk_u: chunk_pos.u(),
            chunk_v: chunk_pos.v(),
            local_cx: cx,
            local_cy: cy,
            local_cz: cz,
            radius,
            center_u,
            center_y: cy as u8,
            center_v,
            surface_y,
            depth_below_surface: surface_y - cy,
            shell_blocks: shell,
            lining_quartz_blocks: lining_quartz,
            lining_amethyst_blocks: lining_amethyst,
            heart_air_blocks: heart_air,
            surrounding_host_blocks: host,
            accidental_air_blocks: accidental_air,
            shell_six_connected: connected,
            heart_sealed: sealed,
            shortest_solid_path: aperture.shortest_solid_path,
            aperture_direction: aperture.direction,
            tunnel_host_blocks: aperture.tunnel_host_blocks,
            shell_depth_blocks: 3,
            camera_face: face_name(chunk_pos.face()),
            camera_u: aperture.camera_u,
            camera_y: aperture.camera_y,
            camera_v: aperture.camera_v,
            camera_local_u: aperture.camera_u - f32::from(FACE_BLOCKS) * 0.5,
            camera_local_v: aperture.camera_v - f32::from(FACE_BLOCKS) * 0.5,
            yaw: aperture.yaw,
            pitch: aperture.pitch,
            context_camera_u: aperture.context_camera_u,
            context_camera_y: aperture.context_camera_y,
            context_camera_v: aperture.context_camera_v,
            context_camera_local_u: aperture.context_camera_u - f32::from(FACE_BLOCKS) * 0.5,
            context_camera_local_v: aperture.context_camera_v - f32::from(FACE_BLOCKS) * 0.5,
            context_yaw: aperture.context_yaw,
            context_pitch: aperture.context_pitch,
            torch: aperture.torch,
            planned_break: aperture.planned_break,
            view_ray: aperture.view_ray,
            estimated_host_pixels: 760_000,
            estimated_quartz_pixels: 180_000,
            estimated_amethyst_pixels: 120_000,
            estimated_heart_pixels: 80_000,
            neighborhood_sha256: String::new(),
            scanned_deposits: 0,
            generated_chunks: 0,
            elapsed_millis: 0,
            peak_rss_bytes: 0,
        },
    })
}

pub fn locate(world_or_atlas: &Path, mods: &Path) -> Result<GeodeSiteRecord, String> {
    let started = Instant::now();
    let (reg, atlas) = load_locator_inputs(world_or_atlas, mods)?;
    let generator = Generator::with_atlas(atlas.manifest.seed, &reg, Arc::clone(&atlas));
    let mut deposits = atlas
        .geology
        .deposits
        .iter()
        .filter(|deposit| deposit.mineral == MineralKind::Geode)
        .collect::<Vec<_>>();
    deposits.sort_by_key(|deposit| deposit.id);
    let scanned_deposits = deposits.len();
    let mut generated_candidates = 0usize;
    let mut best: Option<Candidate> = None;
    for deposit in deposits {
        if generated_candidates >= 384 {
            break;
        }
        let surface = SurfacePos::new(
            deposit.pos.face,
            deposit.pos.u * atlas.cell_blocks() + atlas.cell_blocks() / 2,
            deposit.pos.v * atlas.cell_blocks() + atlas.cell_blocks() / 2,
        )
        .map_err(|error| error.to_string())?;
        let chunk = ChunkPos::from_surface(surface);
        let Some((cx, cz, _, radius)) = generator.geode_at(chunk) else {
            continue;
        };
        if std::env::var_os("WILDFORGE_GEODE_DEBUG").is_some() {
            eprintln!(
                "geode {} at {chunk:?}: cx={cx} cz={cz} radius={radius} host={:?}",
                deposit.id, deposit.host
            );
        }
        if !(3..=5).contains(&radius)
            || cx as i32 - radius < 0
            || cz as i32 - radius < 0
            || cx + radius as usize >= CHUNK_X
            || cz + radius as usize >= CHUNK_Z
        {
            continue;
        }
        generated_candidates += 1;
        let Some(candidate) = evaluate_candidate(deposit, &atlas, &generator, &reg) else {
            continue;
        };
        let replace = best.as_ref().is_none_or(|current| {
            candidate.score > current.score
                || candidate.score == current.score
                    && candidate.record.deposit_id < current.record.deposit_id
        });
        if replace {
            best = Some(candidate);
        }
    }
    let mut record = best
        .ok_or_else(|| {
            format!(
                "no sealed production geode met the aperture and camera constraints ({scanned_deposits} deposits, {generated_candidates} radius/center candidates)"
            )
        })?
        .record;
    let center = ChunkPos::new(
        Face::from_name(&record.face).ok_or("selected geode face is invalid")?,
        record.chunk_u,
        record.chunk_v,
    )
    .map_err(|error| error.to_string())?;
    let (hash, neighborhood_chunks) = neighborhood_hash(&generator, &reg, center);
    record.neighborhood_sha256 = hash;
    record.scanned_deposits = scanned_deposits;
    record.generated_chunks = generated_candidates * 9 + neighborhood_chunks;
    record.elapsed_millis = started.elapsed().as_millis();
    record.peak_rss_bytes = peak_rss_bytes();
    if record.elapsed_millis >= 60_000 {
        return Err(format!(
            "locator exceeded 60 seconds: {} ms",
            record.elapsed_millis
        ));
    }
    if record.peak_rss_bytes != 0 && record.peak_rss_bytes > 512 * 1024 * 1024 {
        return Err(format!(
            "locator exceeded 512 MiB peak RSS: {} bytes",
            record.peak_rss_bytes
        ));
    }
    Ok(record)
}

fn manifest_hash(world: &Path) -> Result<String, String> {
    let mut bytes = Vec::new();
    for relative in ["world.toml", "planet/manifest.toml"] {
        let value = fs::read(world.join(relative))
            .map_err(|error| format!("read source manifest {relative}: {error}"))?;
        bytes.extend_from_slice(relative.as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
        bytes.extend_from_slice(&value);
    }
    Ok(crate::visual_capture::sha256_hex(&bytes))
}

fn audit_hash(audit: &crate::materials::MaterialAudit) -> String {
    crate::visual_capture::sha256_hex(audit.render().as_bytes())
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir(destination).map_err(|error| {
        format!(
            "create qualification copy {}: {error}",
            destination.display()
        )
    })?;
    for entry in
        fs::read_dir(source).map_err(|error| format!("read {}: {error}", source.display()))?
    {
        let entry = entry.map_err(|error| format!("read source entry: {error}"))?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|error| format!("inspect {}: {error}", from.display()))?;
        if file_type.is_dir() {
            copy_tree(&from, &to)?;
        } else if file_type.is_file() {
            fs::copy(&from, &to).map_err(|error| format!("copy {}: {error}", from.display()))?;
        } else {
            return Err(format!(
                "qualification source contains a non-file entry: {}",
                from.display()
            ));
        }
    }
    Ok(())
}

fn validate_qualification_paths(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.is_dir() || destination.exists() {
        return Err("sealed source must exist and opened destination must not exist".into());
    }
    if source.file_name().and_then(|name| name.to_str()) != Some(SEALED_WORLD_NAME)
        || destination.file_name().and_then(|name| name.to_str()) != Some(OPENED_WORLD_NAME)
        || source.parent() != destination.parent()
    {
        return Err(format!(
            "qualification saves must be sibling {SEALED_WORLD_NAME} and {OPENED_WORLD_NAME} directories"
        ));
    }
    Ok(())
}

fn create_sealed_source(
    selection_world: &Path,
    destination: &Path,
    site_path: &Path,
    mods: &Path,
) -> Result<(), String> {
    let site = read_site(site_path)?;
    if !selection_world.is_dir()
        || destination.exists()
        || selection_world.file_name().and_then(|name| name.to_str())
            != Some(site.selection_world.as_str())
        || destination.file_name().and_then(|name| name.to_str()) != Some(SEALED_WORLD_NAME)
        || selection_world.parent() != destination.parent()
    {
        return Err(format!(
            "sealed qualification source must be a new sibling {SEALED_WORLD_NAME} copy of {}",
            site.selection_world
        ));
    }
    let source_manifest = manifest_hash(selection_world)?;
    let source_audit = crate::materials::audit_world(selection_world)
        .map_err(|error| format!("audit selection world: {error}"))?;
    if !source_audit.is_balanced() {
        return Err("selection world material audit is not balanced".into());
    }
    let source_audit_hash = audit_hash(&source_audit);
    let parent = destination
        .parent()
        .ok_or("sealed destination has no parent")?;
    let temporary = parent.join(format!(
        ".{SEALED_WORLD_NAME}.cloning.{}",
        std::process::id()
    ));
    if temporary.exists() {
        fs::remove_dir_all(&temporary)
            .map_err(|error| format!("remove stale sealed temp: {error}"))?;
    }
    let result = (|| {
        copy_tree(selection_world, &temporary)?;
        let reg = Arc::new(crate::registry::load(mods));
        let mut created = World::load_or_create(temporary.clone(), Arc::clone(&reg))
            .map_err(|error| format!("load sealed qualification source: {error}"))?;
        if created.seed != site.seed || world::WORLD_GENERATOR_VERSION != site.generator_version {
            return Err("sealed source does not match selected generator identity".into());
        }
        let atlas = created
            .planet_atlas()
            .ok_or("sealed source has no production atlas")?;
        if atlas.manifest.format_version != site.atlas_format_version
            || atlas.manifest.atlas_algorithm_version != site.atlas_algorithm_version
            || format!("{:016x}", atlas.manifest.content_hash) != site.atlas_content_hash
            || format!("{:016x}", atlas.manifest.genesis_checksum) != site.atlas_genesis_checksum
        {
            return Err("sealed source atlas identity differs from the locator record".into());
        }
        ensure_site_neighborhood(&mut created, &site)?;
        verify_planned_state(&created, &site)?;
        let (ids, quartz, amethyst) = reservation_evidence(&created, &site)?;
        if ids != [site.deposit_id] || quartz == 0 || amethyst == 0 {
            return Err("sealed source did not reserve both finite geode bands".into());
        }
        let save = created.save_modified();
        if !save.is_ok() {
            return Err(save.summary());
        }
        drop(created);
        fs::rename(&temporary, destination)
            .map_err(|error| format!("publish sealed qualification source: {error}"))?;
        Ok(())
    })();
    if result.is_err() && temporary.exists() {
        let _ = fs::remove_dir_all(&temporary);
    }
    result?;
    if manifest_hash(selection_world)? != source_manifest
        || audit_hash(
            &crate::materials::audit_world(selection_world)
                .map_err(|error| format!("re-audit selection world: {error}"))?,
        ) != source_audit_hash
    {
        return Err("selection world changed while producing sealed source".into());
    }
    Ok(())
}

fn site_chunk(site: &GeodeSiteRecord) -> Result<ChunkPos, String> {
    ChunkPos::new(
        Face::from_name(&site.face).ok_or("site face is invalid")?,
        site.chunk_u,
        site.chunk_v,
    )
    .map_err(|error| error.to_string())
}

fn ensure_site_neighborhood(world: &mut World, site: &GeodeSiteRecord) -> Result<(), String> {
    let center = site_chunk(site)?;
    for du in -1..=1 {
        for dv in -1..=1 {
            world.ensure_chunk(center.offset(du, dv));
        }
    }
    Ok(())
}

fn reservation_evidence(
    world: &World,
    site: &GeodeSiteRecord,
) -> Result<(Vec<u32>, u32, u32), String> {
    let ledger = world
        .material_ledger
        .as_ref()
        .ok_or("qualification world has no material ledger")?;
    let reservation = ledger
        .chunk_reservations
        .get(&site_chunk(site)?)
        .ok_or("selected geode chunk has no finite reservation")?;
    let mut ids = BTreeSet::new();
    let mut quartz = 0u32;
    let mut amethyst = 0u32;
    for (block, slices) in &reservation.blocks {
        for slice in slices {
            if matches!(block.as_str(), "base:quartz_block" | "base:amethyst_block") {
                ids.insert(slice.deposit_id);
                if block == "base:quartz_block" {
                    quartz = quartz.saturating_add(slice.count);
                } else {
                    amethyst = amethyst.saturating_add(slice.count);
                }
            }
        }
    }
    Ok((ids.into_iter().collect(), quartz, amethyst))
}

fn verify_planned_state(world: &World, site: &GeodeSiteRecord) -> Result<(), String> {
    let mut planned_positions = BTreeSet::new();
    for expected in &site.planned_break {
        let pos = expected.position()?;
        planned_positions.insert(pos);
        let actual = &world.reg.block(world.get_block_at(pos)).name;
        if actual != &expected.block {
            return Err(format!(
                "stale geode plan at {pos:?}: expected {}, found {actual}",
                expected.block
            ));
        }
    }
    let torch = site.torch.position()?;
    if !planned_positions.contains(&torch) {
        return Err(format!(
            "planned torch cell {torch:?} is not one of the authoritative breaks"
        ));
    }
    Ok(())
}

fn operation_drop_position(site: &GeodeSiteRecord) -> Result<crate::planet::EntityPos, String> {
    let surface = SurfacePos::new(
        Face::from_name(&site.camera_face).ok_or("camera face is invalid")?,
        site.camera_u.round().clamp(0.0, f32::from(FACE_BLOCKS - 1)) as u16,
        site.camera_v.round().clamp(0.0, f32::from(FACE_BLOCKS - 1)) as u16,
    )
    .map_err(|error| error.to_string())?;
    crate::planet::EntityPos::new(
        surface.face(),
        f32::from(surface.u()) + 1.5,
        site.camera_y + 0.3,
        f32::from(surface.v()) + 1.5,
    )
    .map_err(|error| error.to_string())
}

fn snapshot_neighborhood(
    world: &World,
    site: &GeodeSiteRecord,
) -> Result<(Vec<BlockId>, String), String> {
    let center = site_chunk(site)?;
    let mut blocks = Vec::with_capacity(9 * CHUNK_X * CHUNK_Y * CHUNK_Z);
    let mut bytes = Vec::with_capacity(blocks.capacity() * 2);
    for du in -1..=1 {
        for dv in -1..=1 {
            let chunk = center.offset(du, dv);
            for x in 0..CHUNK_X {
                for z in 0..CHUNK_Z {
                    for y in 0..CHUNK_Y {
                        let pos = block_pos(chunk, x as i32, y as i32, z as i32)
                            .expect("qualification neighborhood is canonical");
                        let block = world.get_block_at(pos);
                        blocks.push(block);
                        bytes.extend_from_slice(&block.0.to_le_bytes());
                    }
                }
            }
        }
    }
    Ok((blocks, crate::visual_capture::sha256_hex(&bytes)))
}

fn compare_local_edits(
    world: &World,
    site: &GeodeSiteRecord,
    reg: &Registry,
    sealed: &[BlockId],
) -> Result<usize, String> {
    let center = site_chunk(site)?;
    if sealed.len() != 9 * CHUNK_X * CHUNK_Y * CHUNK_Z {
        return Err("sealed geode snapshot has the wrong voxel count".into());
    }
    let planned: BTreeSet<BlockPos> = site
        .planned_break
        .iter()
        .map(RecordedBlock::position)
        .collect::<Result<_, _>>()?;
    let torch = site.torch.position()?;
    let mut unexpected = 0usize;
    let torch_block = reg.block_id("base:torch").unwrap_or(AIR);
    let mut sealed_blocks = sealed.iter().copied();
    let mut unexpected_examples = Vec::new();
    for du in -1..=1 {
        for dv in -1..=1 {
            let chunk = center.offset(du, dv);
            for x in 0..CHUNK_X {
                for z in 0..CHUNK_Z {
                    for y in 0..CHUNK_Y {
                        let pos = block_pos(chunk, x as i32, y as i32, z as i32)
                            .expect("qualification neighborhood is canonical");
                        let expected = sealed_blocks
                            .next()
                            .expect("sealed snapshot length was prechecked");
                        let actual = world.get_block_at(pos);
                        let allowed = (planned.contains(&pos) && actual == AIR)
                            || (pos == torch && actual == torch_block);
                        if actual != expected && !allowed {
                            unexpected += 1;
                            if unexpected_examples.len() < 12 {
                                unexpected_examples.push(format!(
                                    "{pos:?}: {} -> {}",
                                    reg.block(expected).name,
                                    reg.block(actual).name
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    if let Some(position) = planned
        .iter()
        .find(|position| **position != torch && world.get_block_at(**position) != AIR)
    {
        return Err(format!(
            "planned geode reveal block {position:?} reloaded as {}",
            reg.block(world.get_block_at(*position)).name
        ));
    }
    if world.get_block_at(torch) != torch_block {
        return Err("the qualification torch did not survive reload".into());
    }
    if !unexpected_examples.is_empty() {
        eprintln!(
            "geode reveal unexpected edit samples:\n{}",
            unexpected_examples.join("\n")
        );
    }
    Ok(unexpected)
}

fn remaining_counts(world: &World, site: &GeodeSiteRecord) -> Result<(u32, u32, u32), String> {
    let quartz = world
        .reg
        .block_id("base:quartz_block")
        .ok_or("missing quartz block")?;
    let amethyst = world
        .reg
        .block_id("base:amethyst_block")
        .ok_or("missing amethyst block")?;
    let mut shell = 0;
    let mut lining = 0;
    let mut heart = 0;
    for dx in -site.radius..=site.radius {
        for dy in -site.radius..=site.radius {
            for dz in -site.radius..=site.radius {
                let d2 = dx * dx + dy * dy + dz * dz;
                if d2 > site.radius * site.radius {
                    continue;
                }
                let pos = BlockPos::new(
                    Face::from_name(&site.face).ok_or("invalid site face")?,
                    (i32::from(site.center_u) + dx) as u16,
                    (i32::from(site.center_y) + dy) as u8,
                    (i32::from(site.center_v) + dz) as u16,
                )
                .map_err(|error| error.to_string())?;
                let block = world.get_block_at(pos);
                if d2 >= (site.radius - 1) * (site.radius - 1) {
                    shell += u32::from(block == quartz);
                } else if d2 > (site.radius - 2) * (site.radius - 2) {
                    lining += u32::from(block == quartz || block == amethyst);
                } else {
                    heart += u32::from(block == AIR);
                }
            }
        }
    }
    Ok((shell, lining, heart))
}

pub fn prepare(
    source: &Path,
    destination: &Path,
    site_path: &Path,
    mods: &Path,
) -> Result<GeodePreparationReport, String> {
    validate_qualification_paths(source, destination)?;
    let site = read_site(site_path)?;
    let source_manifest = manifest_hash(source)?;
    let source_audit = crate::materials::audit_world(source).map_err(|error| error.to_string())?;
    if !source_audit.is_balanced() {
        return Err("sealed source material audit is not balanced".into());
    }
    let source_audit_hash = audit_hash(&source_audit);
    let parent = destination
        .parent()
        .ok_or("opened destination has no parent")?;
    let temporary = parent.join(format!(
        ".{OPENED_WORLD_NAME}.preparing.{}",
        std::process::id()
    ));
    if temporary.exists() {
        fs::remove_dir_all(&temporary)
            .map_err(|error| format!("remove stale qualification temp: {error}"))?;
    }
    let result = (|| {
        copy_tree(source, &temporary)?;
        let reg = Arc::new(crate::registry::load(mods));
        let mut world = World::load_or_create(temporary.clone(), Arc::clone(&reg))
            .map_err(|error| format!("load qualification copy: {error}"))?;
        if world.seed != site.seed || world::WORLD_GENERATOR_VERSION != site.generator_version {
            return Err("qualification copy does not match selected world identity".into());
        }
        ensure_site_neighborhood(&mut world, &site)?;
        verify_planned_state(&world, &site)?;
        let (sealed_neighborhood, sealed_neighborhood_hash) = snapshot_neighborhood(&world, &site)?;
        let before_audit = world
            .material_ledger
            .as_ref()
            .ok_or("missing material ledger")?
            .audit();
        let (reservation_ids, quartz_reserved, amethyst_reserved) =
            reservation_evidence(&world, &site)?;
        if reservation_ids != [site.deposit_id] || quartz_reserved == 0 || amethyst_reserved == 0 {
            return Err(format!(
                "geode bands do not share deposit {}: ids {reservation_ids:?}, quartz {quartz_reserved}, amethyst {amethyst_reserved}",
                site.deposit_id
            ));
        }
        let pick = reg
            .item_id("base:iron_pickaxe")
            .ok_or("missing iron pickaxe")?;
        let torch_item = reg.item_id("base:torch").ok_or("missing torch item")?;
        let pick_stack = ItemStack::new(&reg, pick, 1);
        let torch_stack = ItemStack::new(&reg, torch_item, 1);
        world
            .record_external_stack(pick_stack, EXTERNAL_SOURCE)
            .map_err(|error| error.to_string())?;
        world
            .record_external_stack(torch_stack, EXTERNAL_SOURCE)
            .map_err(|error| error.to_string())?;
        let mut inventory = Inventory::new();
        inventory.slots[0] = Some(pick_stack);
        inventory.slots[1] = Some(torch_stack);
        let drop_pos = operation_drop_position(&site)?;
        let mut operations = Vec::new();
        for (index, expected) in site.planned_break.iter().enumerate() {
            let pos = expected.position()?;
            let block = world.get_block_at(pos);
            if reg.block(block).name != expected.block
                || reg.effective_hardness(block, Some(pick)).is_none()
            {
                return Err(format!("planned authoritative break refused at {pos:?}"));
            }
            let result = world
                .break_block_at(pos, Some(pick), true, true)
                .ok_or_else(|| format!("authoritative break failed at {pos:?}"))?;
            inventory.wear_tool(&reg, 0);
            if let Some(drop) = result.drop {
                world.spawn_loose_item(ItemEntity::new(
                    drop_pos,
                    Vec3::ZERO,
                    drop.item,
                    drop.count,
                ));
            }
            operations.push(OperationRecord {
                sequence: index as u32 + 1,
                kind: "authoritative-break".into(),
                target: expected.clone(),
                result: "air-plus-retained-drop".into(),
                tool_durability_after: inventory.slots[0].map_or(0, |stack| stack.durability),
            });
        }
        let torch_pos = site.torch.position()?;
        let placed =
            inventory.slots[1].is_some_and(|stack| world.place_item_block_at(torch_pos, stack));
        if !placed {
            return Err(format!(
                "authoritative torch placement failed at {torch_pos:?}"
            ));
        }
        inventory.take_one(1);
        operations.push(OperationRecord {
            sequence: operations.len() as u32 + 1,
            kind: "authoritative-place".into(),
            target: site.torch.clone(),
            result: "placed-and-inventory-spent".into(),
            tool_durability_after: inventory.slots[0].map_or(0, |stack| stack.durability),
        });
        if let Some(stack) = inventory.slots[0] {
            let mut entity = ItemEntity::new(drop_pos, Vec3::ZERO, stack.item, stack.count);
            entity.durability = stack.durability;
            world.spawn_loose_item(entity);
        }
        let after_audit = world
            .material_ledger
            .as_ref()
            .ok_or("missing material ledger after reveal")?
            .audit();
        if !before_audit.is_balanced() || !after_audit.is_balanced() {
            return Err("material conservation failed during reveal".into());
        }
        let before_account = world
            .material_ledger
            .as_ref()
            .and_then(|ledger| ledger.deposits.get(&site.deposit_id))
            .ok_or("selected deposit account disappeared")?;
        let quartz_extracted = *before_account.extracted.get("quartz").unwrap_or(&0);
        let amethyst_extracted = *before_account.extracted.get("amethyst").unwrap_or(&0);
        let save = world.save_modified();
        if !save.is_ok() {
            return Err(format!("qualification save failed: {}", save.summary()));
        }
        drop(world);
        let mut reloaded = World::load_or_create(temporary.clone(), Arc::clone(&reg))
            .map_err(|error| format!("reload qualification copy: {error}"))?;
        ensure_site_neighborhood(&mut reloaded, &site)?;
        let reload_audit = reloaded
            .material_ledger
            .as_ref()
            .ok_or("missing reloaded ledger")?
            .audit();
        let unexpected = compare_local_edits(&reloaded, &site, &reg, &sealed_neighborhood)?;
        let (remaining_shell, remaining_lining, remaining_heart) =
            remaining_counts(&reloaded, &site)?;
        if !reload_audit.is_balanced()
            || unexpected != 0
            || remaining_shell < site.shell_blocks.saturating_sub(8)
            || remaining_lining < 12
            || remaining_heart < site.heart_air_blocks
        {
            return Err(format!(
                "reloaded reveal failed: balanced={} unexpected_edits={unexpected} shell={remaining_shell}/{} lining={remaining_lining} heart={remaining_heart}/{}",
                reload_audit.is_balanced(),
                site.shell_blocks.saturating_sub(8),
                site.heart_air_blocks
            ));
        }
        let report = GeodePreparationReport {
            schema_version: GEODE_PREPARATION_SCHEMA_VERSION,
            site_id: site.site_id.clone(),
            source_world: SEALED_WORLD_NAME.into(),
            opened_world: OPENED_WORLD_NAME.into(),
            source_manifest_sha256: source_manifest.clone(),
            source_material_audit_sha256: source_audit_hash.clone(),
            source_neighborhood_sha256: sealed_neighborhood_hash,
            source_unchanged: false,
            before_material_audit_sha256: audit_hash(&before_audit),
            after_material_audit_sha256: audit_hash(&after_audit),
            reload_material_audit_sha256: audit_hash(&reload_audit),
            before_balanced: before_audit.is_balanced(),
            after_balanced: after_audit.is_balanced(),
            reload_balanced: reload_audit.is_balanced(),
            deposit_id: site.deposit_id,
            reservation_deposit_ids: reservation_ids,
            quartz_reserved_blocks: quartz_reserved,
            amethyst_reserved_blocks: amethyst_reserved,
            quartz_extracted_units: quartz_extracted,
            amethyst_extracted_units: amethyst_extracted,
            planned_breaks: site.planned_break.len(),
            applied_breaks: operations
                .iter()
                .filter(|operation| operation.kind == "authoritative-break")
                .count(),
            torch_placed: true,
            unexpected_edits: unexpected,
            remaining_shell_blocks: remaining_shell,
            remaining_lining_blocks: remaining_lining,
            remaining_heart_air_blocks: remaining_heart,
            loose_drop_stacks: reloaded.loose_items().len(),
            save_succeeded: true,
            reload_succeeded: true,
            operation: operations,
        };
        drop(reloaded);
        fs::rename(&temporary, destination)
            .map_err(|error| format!("publish opened qualification save: {error}"))?;
        Ok(report)
    })();
    if result.is_err() && temporary.exists() {
        let _ = fs::remove_dir_all(&temporary);
    }
    let mut report = result?;
    report.source_unchanged = manifest_hash(source)? == source_manifest
        && audit_hash(&crate::materials::audit_world(source).map_err(|error| error.to_string())?)
            == source_audit_hash;
    if !report.source_unchanged {
        return Err("sealed source changed during preparation".into());
    }
    Ok(report)
}

fn argument_after(args: &[String], name: &str) -> Option<PathBuf> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .map(PathBuf::from)
}

pub fn run_cli(args: &[String]) -> Option<Result<(), String>> {
    let mods = argument_after(args, "--mods").unwrap_or_else(|| PathBuf::from("mods"));
    if let Some(input) = argument_after(args, "--locate-cracked-geode") {
        return Some((|| {
            let record = locate(&input, &mods)?;
            let text = toml::to_string_pretty(&record)
                .map_err(|error| format!("serialize geode site: {error}"))?;
            if let Some(output) = argument_after(args, "--output") {
                crate::persist::atomic_write(&output, text.as_bytes(), false)
                    .map_err(|error| format!("write geode site {}: {error}", output.display()))?;
            } else {
                print!("{text}");
            }
            Ok(())
        })());
    }
    if let Some(atlas_path) = argument_after(args, "--create-cracked-geode-source") {
        return Some((|| {
            let destination = argument_after(args, "--destination")
                .ok_or("--create-cracked-geode-source requires --destination")?;
            if destination.file_name().and_then(|name| name.to_str()) != Some(SEALED_WORLD_NAME) {
                return Err(format!(
                    "source destination must be named {SEALED_WORLD_NAME}"
                ));
            }
            let site_path =
                argument_after(args, "--site").ok_or("source creation requires --site")?;
            let site = read_site(&site_path)?;
            let reg = Arc::new(crate::registry::load(&mods));
            let atlas = PlanetAtlas::load(&atlas_path).map_err(|error| error.to_string())?;
            if atlas.manifest.seed != site.seed
                || format!("{:016x}", atlas.manifest.genesis_checksum)
                    != site.atlas_genesis_checksum
            {
                return Err("source atlas does not match the selected geode".into());
            }
            world::create_qualification_world_from_atlas(
                &destination,
                atlas,
                Arc::clone(&reg),
                &crate::planet_atlas::CancellationToken::default(),
                |progress| match progress {
                    world::WorldCreationProgress::Atlas(atlas) => {
                        eprintln!("atlas: {}", atlas.stage.label())
                    }
                    world::WorldCreationProgress::Arcane(arcane) => {
                        eprintln!("arcane: {}", arcane.stage.label())
                    }
                    world::WorldCreationProgress::Homeland {
                        stage,
                        completed,
                        total,
                    } => {
                        eprintln!("homeland: {stage} {completed}/{total}")
                    }
                },
            )
            .map_err(|error| error.to_string())?;
            let mut created = World::load_or_create(destination.clone(), reg)
                .map_err(|error| error.to_string())?;
            ensure_site_neighborhood(&mut created, &site)?;
            verify_planned_state(&created, &site)?;
            let (ids, quartz, amethyst) = reservation_evidence(&created, &site)?;
            if ids != [site.deposit_id] || quartz == 0 || amethyst == 0 {
                return Err("new sealed source did not reserve both geode bands".into());
            }
            let save = created.save_modified();
            if !save.is_ok() {
                return Err(save.summary());
            }
            Ok(())
        })());
    }
    if let Some(selection_world) = argument_after(args, "--clone-cracked-geode-source") {
        return Some((|| {
            let destination = argument_after(args, "--destination")
                .ok_or("--clone-cracked-geode-source requires --destination")?;
            let site = argument_after(args, "--site").ok_or("source clone requires --site")?;
            create_sealed_source(&selection_world, &destination, &site, &mods)
        })());
    }
    if let Some(source) = argument_after(args, "--prepare-cracked-geode") {
        return Some((|| {
            let destination = argument_after(args, "--destination")
                .ok_or("--prepare-cracked-geode requires --destination")?;
            let site = argument_after(args, "--site").ok_or("preparation requires --site")?;
            let report = prepare(&source, &destination, &site, &mods)?;
            let text = toml::to_string_pretty(&report)
                .map_err(|error| format!("serialize geode preparation: {error}"))?;
            if let Some(output) = argument_after(args, "--output") {
                crate::persist::atomic_write(&output, text.as_bytes(), false).map_err(|error| {
                    format!("write preparation report {}: {error}", output.display())
                })?;
            } else {
                print!("{text}");
            }
            Ok(())
        })());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
    }

    fn selected() -> GeodeSiteRecord {
        read_site(&root().join("screenshots/visual-polish/cracked-geode-site.toml")).unwrap()
    }

    #[test]
    fn quartz_blocks_are_part_of_the_finite_geode_band() {
        assert_eq!(
            MineralKind::from_block_name("base:quartz_block"),
            MineralKind::Geode
        );
        assert_eq!(
            MineralKind::from_block_name("base:amethyst_block"),
            MineralKind::Geode
        );
    }

    #[test]
    fn selected_capture_geode_is_a_sealed_production_structure() {
        let site = selected();
        assert_eq!(site.source_world, SEALED_WORLD_NAME);
        assert_eq!(site.selection_world, SELECTION_WORLD);
        assert_eq!(site.generator_version, world::WORLD_GENERATOR_VERSION);
        assert!(site.heart_sealed && site.shell_six_connected);
        assert_eq!(site.accidental_air_blocks, 0);
        assert!((3..=5).contains(&site.radius));
        assert_eq!(site.neighborhood_sha256.len(), 64);

        let mut shell = BTreeSet::new();
        let mut lining = 0u32;
        let mut heart = 0u32;
        for x in -site.radius..=site.radius {
            for y in -site.radius..=site.radius {
                for z in -site.radius..=site.radius {
                    match crate::worldgen::geode_band_at(x * x + y * y + z * z, site.radius) {
                        Some(crate::worldgen::GeodeBand::Shell) => {
                            shell.insert((x, y, z));
                        }
                        Some(crate::worldgen::GeodeBand::Lining) => lining += 1,
                        Some(crate::worldgen::GeodeBand::Heart) => heart += 1,
                        None => {}
                    }
                }
            }
        }
        let first = *shell.iter().next().unwrap();
        let mut reached = BTreeSet::from([first]);
        let mut queue = VecDeque::from([first]);
        while let Some((x, y, z)) = queue.pop_front() {
            for delta in [
                (1, 0, 0),
                (-1, 0, 0),
                (0, 1, 0),
                (0, -1, 0),
                (0, 0, 1),
                (0, 0, -1),
            ] {
                let next = (x + delta.0, y + delta.1, z + delta.2);
                if shell.contains(&next) && reached.insert(next) {
                    queue.push_back(next);
                }
            }
        }
        assert_eq!(reached, shell);
        assert!(site.shell_blocks >= 80 && site.shell_blocks <= shell.len() as u32);
        assert!(
            site.lining_quartz_blocks >= 12
                && site.lining_amethyst_blocks >= 12
                && site.lining_quartz_blocks + site.lining_amethyst_blocks <= lining
        );
        assert!(site.heart_air_blocks >= 7 && site.heart_air_blocks <= heart);
        let names = site
            .view_ray
            .iter()
            .map(|block| block.block.as_str())
            .collect::<Vec<_>>();
        let host = names
            .iter()
            .position(|name| matches!(*name, "base:limestone" | "base:marble"))
            .unwrap();
        let shell = names
            .iter()
            .skip(host + 1)
            .position(|name| *name == "base:quartz_block")
            .unwrap()
            + host
            + 1;
        let lining = names
            .iter()
            .skip(shell + 1)
            .position(|name| matches!(*name, "base:quartz_block" | "base:amethyst_block"))
            .unwrap()
            + shell
            + 1;
        let heart = names
            .iter()
            .skip(lining + 1)
            .position(|name| *name == "base:air")
            .unwrap()
            + lining
            + 1;
        assert!(host < shell && shell < lining && lining < heart);
    }

    #[test]
    fn production_geode_shell_radii_are_six_connected() {
        for radius in 3..=5 {
            let mut shell = BTreeSet::new();
            for x in -radius..=radius {
                for y in -radius..=radius {
                    for z in -radius..=radius {
                        if crate::worldgen::geode_band_at(x * x + y * y + z * z, radius)
                            == Some(crate::worldgen::GeodeBand::Shell)
                        {
                            shell.insert((x, y, z));
                        }
                    }
                }
            }
            let first = *shell.iter().next().unwrap();
            let mut reached = BTreeSet::from([first]);
            let mut queue = VecDeque::from([first]);
            while let Some((x, y, z)) = queue.pop_front() {
                for (dx, dy, dz) in [
                    (1, 0, 0),
                    (-1, 0, 0),
                    (0, 1, 0),
                    (0, -1, 0),
                    (0, 0, 1),
                    (0, 0, -1),
                ] {
                    let next = (x + dx, y + dy, z + dz);
                    if shell.contains(&next) && reached.insert(next) {
                        queue.push_back(next);
                    }
                }
            }
            assert_eq!(reached, shell, "radius {radius} shell disconnected");
        }
    }

    #[test]
    fn geode_reveal_uses_only_authoritative_break_and_place_operations() {
        let text = fs::read_to_string(
            root().join("screenshots/visual-polish/geode-preparation.report.toml"),
        )
        .unwrap();
        let report: GeodePreparationReport = toml::from_str(&text).unwrap();
        assert_eq!(report.planned_breaks, report.applied_breaks);
        assert!(report.torch_placed && report.source_unchanged && report.unexpected_edits == 0);
        assert_eq!(report.operation.len(), report.planned_breaks + 1);
        assert!(
            report.operation[..report.planned_breaks]
                .iter()
                .all(|operation| operation.kind == "authoritative-break")
        );
        assert_eq!(report.operation.last().unwrap().kind, "authoritative-place");
    }

    #[test]
    fn geode_reveal_materials_balance_before_and_after_reload() {
        let site = selected();
        let text = fs::read_to_string(
            root().join("screenshots/visual-polish/geode-preparation.report.toml"),
        )
        .unwrap();
        let report: GeodePreparationReport = toml::from_str(&text).unwrap();
        assert!(report.before_balanced && report.after_balanced && report.reload_balanced);
        assert!(report.save_succeeded && report.reload_succeeded);
        assert_eq!(report.reservation_deposit_ids, [site.deposit_id]);
        assert!(report.quartz_reserved_blocks > 0 && report.amethyst_reserved_blocks > 0);
        assert!(report.remaining_shell_blocks > 70);
        assert!(report.remaining_lining_blocks > 12);
        assert!(report.remaining_heart_air_blocks >= site.heart_air_blocks);
        assert!(report.loose_drop_stacks >= report.applied_breaks);
    }
}
