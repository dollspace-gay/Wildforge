//! Piece-based procedural structures (spec Part 2.3).
//!
//! A generation walk assembles a coherent structure from an entry piece
//! outward: each placed piece exposes typed connector points, connectors are
//! followed through per-kind weighted *pools*, and the walk honours `max_depth`
//! (steps from the entry) and `max_pieces` budgets. Terrain adaptation modes
//! (none / bury / encapsulate) pick the entry anchor's height; every later
//! piece derives its Y from the parent connector it attaches to, so the whole
//! assembly keeps floor continuity instead of re-sampling terrain per piece.
//!
//! Pieces stamp into the ordinary chunk grid like ruins do (they are normal
//! world blocks — breakable, lootable) and reuse the existing loot/chest/
//! arcane/ledger machinery rather than a second stamp path. Assemblies may
//! span chunks: every touched chunk is `ensure_chunk`ed and reserved in
//! `structure_chunks` *before* being written, which is what makes the
//! `seed_structures` reservation guard safe.
//!
//! Spawn/feature [`Marker`]s are resolved to world positions and returned with
//! the assembly result; no phase consumes them yet (spec 2.4/2.5 plug in
//! later).

use std::collections::{HashSet, VecDeque};

use super::multiblock::Rotation;
use super::*;
use crate::chunk::{CHUNK_X, CHUNK_Y, CHUNK_Z};
use crate::inventory::ItemStack;
use crate::planet::{BlockPos, Direction4, SurfacePos};
use crate::registry::{AssemblyDef, BlockId, MaterialVector, PieceDef, PoolDef};

/// Horizontal offset a [`Direction4`] takes in (du, dv) step space, matching
/// `step4`'s convention (East = +u, North = +v, ...).
fn dir_offset(dir: Direction4) -> (i32, i32) {
    match dir {
        Direction4::East => (1, 0),
        Direction4::North => (0, 1),
        Direction4::West => (-1, 0),
        Direction4::South => (0, -1),
    }
}

/// Rotate a cardinal facing through a multiblock [`Rotation`]. Rotation keeps
/// `dy` unchanged, so only the horizontal components matter here.
fn rotate_dir(rot: Rotation, dir: Direction4) -> Direction4 {
    let (du, dv) = dir_offset(dir);
    let (rdu, _, rdv) = rot.apply((du, 0, dv));
    if rdu == 1 {
        Direction4::East
    } else if rdu == -1 {
        Direction4::West
    } else if rdv == 1 {
        Direction4::North
    } else {
        Direction4::South
    }
}

/// The single [`Rotation`] that maps `from` onto `to` (the four cardinal turns
/// act transitively and freely on the four facings, so it is unique).
fn align_rotation(from: Direction4, to: Direction4) -> Rotation {
    Rotation::CARDINAL
        .into_iter()
        .find(|&rot| rotate_dir(rot, from) == to)
        .expect("cardinal rotations act transitively on cardinal facings")
}

/// One open connector in the walk queue: where the connector cell sits, which
/// way it points out of the placed piece, its kind, and its depth from the
/// entry piece.
struct OpenConnector {
    pos: BlockPos,
    facing: Direction4,
    kind: String,
    depth: u32,
}

/// A resolved spawn/feature marker (spec 2.4/2.5 seam): a typed tag plus the
/// world position it lands on once its piece is placed.
///
/// Marker `kind` conventions (spec 2.4/2.5):
/// - `"spawn:npc:<npc_id>"` places that NPC (`mod:npc` qualified) at the
///   resolved position when chunkgen consumes the marker — see `chunks.rs`.
///   The NPC is clamped to the surface if `at` is not walkable, and an
///   unknown id is silently skipped (the piece stays).
/// - `"feature:<gate_id>"` places a flag-gated sealed block (`mod:gate`
///   qualified): `gate.block` at `at`, locked until the player's KV flag
///   reads the gate's `value`, then opened by right-click or mined. An
///   unknown id is silently skipped so a missing def cannot leave a
///   permanent unbreakable wall.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssemblyMarker {
    pub kind: String,
    pub at: BlockPos,
}

/// Rolls one piece from a pool by weight using the codebase's integer LCG
/// (the same shape `roll_loot` uses). Returns the chosen piece name.
fn roll_piece<'a>(pool: &'a PoolDef, rng: &mut u32) -> Option<&'a str> {
    let total: u32 = pool.entries.iter().map(|e| e.weight.max(1)).sum();
    if total == 0 {
        return None;
    }
    *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
    let mut pick = (*rng >> 8) % total;
    for entry in &pool.entries {
        let weight = entry.weight.max(1);
        if pick < weight {
            return Some(&entry.piece);
        }
        pick -= weight;
    }
    None
}

/// The cells of `piece` (rotated by `rot`) as `(du, dv)` extents, used to size
/// the entry-placement window inside its source chunk.
fn piece_extent(piece: &PieceDef, rot: Rotation) -> (i32, i32, i32, i32) {
    let (mut min_du, mut max_du, mut min_dv, mut max_dv) = (0, 0, 0, 0);
    for cell in &piece.cells {
        let (du, _, dv) = rot.apply((cell.du, cell.dy, cell.dv));
        min_du = min_du.min(du);
        max_du = max_du.max(du);
        min_dv = min_dv.min(dv);
        max_dv = max_dv.max(dv);
    }
    (min_du, max_du, min_dv, max_dv)
}

/// One fully-placed connector attachment: the world anchor of the child piece,
/// the rotation it was placed in, and which of its connectors (if any) was the
/// attachment connector (already matched, so it must not be re-enqueued).
struct Attachment {
    anchor: BlockPos,
    rotation: Rotation,
    consumed: Option<usize>,
}

impl World {
    /// Run one deterministic piece assembly rooted in `pos` (generated by
    /// `seed_structures`; `seed` is that call's per-chunk hash so regeneration
    /// reproduces the exact same layout). Returns the resolved
    /// [`AssemblyMarker`]s for the placed pieces (consumed by later phases).
    pub(crate) fn place_assembly(
        &mut self,
        asm: AssemblyDef,
        pos: crate::planet::ChunkPos,
        seed: u32,
    ) -> (Vec<AssemblyMarker>, u32) {
        let reg = self.reg.clone();
        let Some(entry) = reg.pieces.iter().find(|p| p.name == asm.entry_piece) else {
            return (Vec::new(), 0);
        };
        if entry.cells.is_empty() {
            return (Vec::new(), 0);
        }
        let center = SurfacePos::new(
            pos.face(),
            pos.u() * CHUNK_X as u16 + CHUNK_X as u16 / 2,
            pos.v() * CHUNK_Z as u16 + CHUNK_Z as u16 / 2,
        )
        .expect("chunk center is canonical");
        let biome = self.generator.biome_at(center).name().to_lowercase();
        if !asm.biomes.contains(&biome) {
            return (Vec::new(), 0);
        }
        let (min_du, max_du, min_dv, max_dv) = piece_extent(entry, Rotation::R0);
        let (w, d) = (max_du - min_du + 1, max_dv - min_dv + 1);
        let entry_max_dy = entry.cells.iter().map(|cell| cell.dy).max().unwrap_or(0);
        let height = entry_max_dy.max(1);
        // Keep the entry piece's origin jittered inside its chunk the way
        // ruins are; the walk may still extend it across chunk borders.
        let origin_surface = SurfacePos::new(
            pos.face(),
            pos.u() * CHUNK_X as u16
                + (1 + ((seed >> 8) as i32).rem_euclid((15 - w.clamp(1, 14)).max(1))) as u16,
            pos.v() * CHUNK_Z as u16
                + (1 + ((seed >> 16) as i32).rem_euclid((15 - d.clamp(1, 14)).max(1))) as u16,
        )
        .expect("piece origin is inside its chunk");
        let sample = SurfacePos::canonicalized(
            origin_surface.face(),
            i32::from(origin_surface.u()) + w / 2,
            i32::from(origin_surface.v()) + d / 2,
        )
        .expect("piece center canonicalizes");
        let surface_y = self.surface_height_at(sample);
        if surface_y <= crate::chunk::SEA_LEVEL + 1 || surface_y >= CHUNK_Y as i32 - 24 {
            return (Vec::new(), 0);
        }
        let mut rng = seed ^ 0xa55e_b1e3;
        let y0 = match asm.terrain {
            crate::registry::TerrainAdaptation::None => surface_y,
            crate::registry::TerrainAdaptation::Bury => {
                let depth = 4 + ((rng >> 4) % 9) as i32;
                (surface_y - depth - height).max(6)
            }
            crate::registry::TerrainAdaptation::Encapsulate => {
                let depth = 8 + ((rng >> 4) % 12) as i32;
                (surface_y - depth - height).max(6)
            }
        };
        let entry_anchor = BlockPos::new(
            origin_surface.face(),
            origin_surface.u(),
            y0 as u8,
            origin_surface.v(),
        )
        .expect("entry base is inside the world");

        let mut occupied = HashSet::new();
        let mut reserved: HashSet<crate::planet::ChunkPos> = HashSet::new();
        let mut markers = Vec::new();
        let mut inherited_stacks = Vec::new();
        let mut inherited_placements = Vec::new();
        let mut queue = VecDeque::new();
        let mut placed_count = 0u32;

        if let Some(attach) = self.place_piece(
            entry,
            entry_anchor,
            Rotation::R0,
            None,
            0,
            &mut occupied,
            &mut reserved,
            &mut markers,
            &mut inherited_stacks,
            &mut inherited_placements,
            &mut rng,
        ) {
            for (i, conn) in entry.connectors.iter().enumerate() {
                if Some(i) == attach.consumed {
                    continue;
                }
                let Some(world) = attach.anchor.offset(conn.du, conn.dy, conn.dv) else {
                    continue;
                };
                queue.push_back(OpenConnector {
                    pos: world,
                    facing: rotate_dir(attach.rotation, conn.facing),
                    kind: conn.kind.clone(),
                    depth: 1,
                });
            }
            placed_count += 1;
        }

        while let Some(open) = queue.pop_front() {
            if placed_count >= asm.max_pieces {
                break;
            }
            if open.depth > asm.max_depth {
                continue;
            }
            let Some(pool_id) = asm.pools.get(&open.kind) else {
                continue; // no pool for this kind: dead end, valid
            };
            let Some(pool) = reg.pools.iter().find(|p| &p.id == pool_id) else {
                continue;
            };
            let Some(chosen) = roll_piece(pool, &mut rng).map(|name| name.to_string()) else {
                continue;
            };
            let Some(piece) = reg.pieces.iter().find(|p| p.name == chosen) else {
                continue;
            };
            let target = open.facing.opposite();
            let mut attached = None;
            for (i, conn) in piece.connectors.iter().enumerate() {
                if conn.kind != open.kind {
                    continue;
                }
                let rot = align_rotation(conn.facing, target);
                let (sdu, sdv) = dir_offset(open.facing);
                let Some(connector_cell) = open.pos.offset(sdu, 0, sdv) else {
                    continue;
                };
                let (cdu, _, cdv) = rot.apply((conn.du, conn.dy, conn.dv));
                let Some(anchor) = connector_cell.offset(-cdu, -conn.dy, -cdv) else {
                    continue;
                };
                if !self.piece_fits(piece, anchor, rot, &occupied, &reserved) {
                    continue;
                }
                if let Some(attach) = self.place_piece(
                    piece,
                    anchor,
                    rot,
                    Some(i),
                    open.depth.saturating_add(1),
                    &mut occupied,
                    &mut reserved,
                    &mut markers,
                    &mut inherited_stacks,
                    &mut inherited_placements,
                    &mut rng,
                ) {
                    attached = Some(attach);
                    break;
                }
            }
            if let Some(attach) = attached {
                placed_count += 1;
                for (i, conn) in piece.connectors.iter().enumerate() {
                    if Some(i) == attach.consumed {
                        continue;
                    }
                    let Some(world) = attach.anchor.offset(conn.du, conn.dy, conn.dv) else {
                        continue;
                    };
                    queue.push_back(OpenConnector {
                        pos: world,
                        facing: rotate_dir(attach.rotation, conn.facing),
                        kind: conn.kind.clone(),
                        depth: open.depth.saturating_add(1),
                    });
                }
            }
            // No pool entry fit: the connector dead-ends; that's fine.
        }

        if let Some(ledger) = &mut self.material_ledger
            && let Err(error) = ledger.record_external_world_content(
                &reg,
                &inherited_stacks,
                &inherited_placements,
                "pre-genesis piece inheritance",
            )
        {
            eprintln!("materials: piece inheritance accounting failed: {error}");
        }
        (markers, placed_count)
    }

    /// Whether every solid cell of `piece` (at `anchor`/`rot`) lands in-world
    /// and off the colliding occupied set. A piece may add to chunks we have
    /// reserved this walk or to untouched chunks, but may not cross into a
    /// chunk another structure already reserved.
    fn piece_fits(
        &self,
        piece: &PieceDef,
        anchor: BlockPos,
        rot: Rotation,
        occupied: &HashSet<BlockPos>,
        reserved: &HashSet<crate::planet::ChunkPos>,
    ) -> bool {
        for cell in &piece.cells {
            let (du, dy, dv) = rot.apply((cell.du, cell.dy, cell.dv));
            let Some(world) = anchor.offset(du, dy, dv) else {
                return false;
            };
            if occupied.contains(&world) {
                return false;
            }
            if self.structure_chunks.contains(&world.chunk()) && !reserved.contains(&world.chunk())
            {
                return false;
            }
        }
        true
    }

    /// Stamp `piece` at `anchor`/`rot`, reserving every chunk it touches
    /// before its first write. Returns the [`Attachment`] (anchor, rotation,
    /// which connector of the piece was used for the attachment, if any).
    /// `None` means nothing was placeable (empty cells or a fully-skipped
    /// piece).
    #[allow(clippy::too_many_arguments)]
    fn place_piece(
        &mut self,
        piece: &PieceDef,
        anchor: BlockPos,
        rot: Rotation,
        consumed: Option<usize>,
        _depth: u32,
        occupied: &mut HashSet<BlockPos>,
        reserved: &mut HashSet<crate::planet::ChunkPos>,
        markers: &mut Vec<AssemblyMarker>,
        inherited_stacks: &mut Vec<ItemStack>,
        inherited_placements: &mut Vec<(BlockPos, MaterialVector)>,
        rng: &mut u32,
    ) -> Option<Attachment> {
        let reg = self.reg.clone();
        let mut placements: Vec<(BlockPos, BlockId)> = Vec::new();
        for cell in &piece.cells {
            let (du, dy, dv) = rot.apply((cell.du, cell.dy, cell.dv));
            let Some(world) = anchor.offset(du, dy, dv) else {
                continue;
            };
            let Some(block) = reg.block_id(&cell.block) else {
                continue;
            };
            placements.push((world, block));
        }
        if placements.is_empty() {
            return None;
        }
        // Reserve every touched chunk first so a chunk `ensure_chunk`ed mid
        // walk (which runs its own `seed_structures`) early-returns, and so
        // retrogen leaves the whole assembly alone.
        for (world, _) in &placements {
            let chunk = world.chunk();
            if reserved.insert(chunk) {
                self.structure_chunks.insert(chunk);
                self.ensure_chunk(chunk);
            }
        }
        for (world, block) in &placements {
            self.set_block_at(*world, *block);
            occupied.insert(*world);
            let materials = reg.block(*block).materials.clone();
            if !materials.is_empty() {
                inherited_placements.push((*world, materials));
            }
        }
        // Chests get rolled, wild-owned loot, bound exactly like ruins.
        if let Some(chest_block) = reg.block_id("base:chest") {
            for chest in &piece.chests {
                let (du, dy, dv) = rot.apply((chest.du, chest.dy, chest.dv));
                let Some(world) = anchor.offset(du, dy, dv) else {
                    continue;
                };
                self.set_block_at(world, chest_block);
                let mut state = ChestState {
                    wild_owned: true,
                    ..Default::default()
                };
                let n = 3 + (*rng % 3) as usize;
                let mut loot = self.roll_loot(&chest.loot, n as u32, rng);
                for (i, mut stack) in loot.drain(..).enumerate() {
                    if i >= CHEST_SLOTS {
                        break;
                    }
                    if self.arcane_ledger.is_some()
                        && let Err(error) =
                            self.bind_arcane_stack_at(world, &mut stack, "piece inheritance")
                    {
                        eprintln!("arcane: piece loot could not bind: {error}");
                        continue;
                    }
                    if self.discovery_state.is_some()
                        && let Err(error) = self.bind_discovery_stack_at(world, &mut stack)
                    {
                        eprintln!("discovery: piece artifact could not bind: {error}");
                        continue;
                    }
                    let preferred = (i * 7 + (*rng % 5) as usize) % CHEST_SLOTS;
                    let slot = (0..CHEST_SLOTS)
                        .map(|offset| (preferred + offset) % CHEST_SLOTS)
                        .find(|slot| state.slots[*slot].is_none())
                        .unwrap_or(preferred);
                    inherited_stacks.push(stack);
                    state.slots[slot] = Some(stack);
                }
                self.block_entities.insert(world, BlockEntity::Chest(state));
            }
        }
        for marker in &piece.markers {
            let (du, dy, dv) = rot.apply((marker.du, marker.dy, marker.dv));
            if let Some(world) = anchor.offset(du, dy, dv) {
                markers.push(AssemblyMarker {
                    kind: marker.kind.clone(),
                    at: world,
                });
            }
        }
        Some(Attachment {
            anchor,
            rotation: rot,
            consumed,
        })
    }
}
