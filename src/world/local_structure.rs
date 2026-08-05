//! Bounded local-structure entities (spec Part 1.1, scoped).
//!
//! A [`LocalStructure`] is a small, self-contained block store built from a
//! saved [`Template`] (Phase 4): it holds its cells in local-offset space
//! (`HashMap<(du, dy, dv), BlockId>`), independent of the main world's chunk
//! grid, and carries a static world [`LocalTransform`]. This phase is the
//! representation primitive only — no movement, rendering, collision, or
//! block behavior runs on these blocks yet.
//!
//! ## Why reuse `Template`?
//!
//! A template is already coordinate-space-agnostic: its cells are relative
//! offsets resolved against a world anchor only at use time. A local
//! structure is the same shape resolved against a *moving* transform
//! instead of a fixed anchor; building it straight from `Template` means no
//! second capture format. Block names are resolved to ids exactly the way
//! `template.rs`'s stamp path resolves them, so nothing re-derives a
//! name→id lookup.
//!
//! ## Deferred, not implemented here
//!
//! Path-following motion (the transform is static), rendering and collision,
//! block ticking / `BlockEntity` behavior inside a structure, player
//! interaction, and multiblock recognition running *inside* a structure are
//! all explicitly out of scope. The tick-coupling question that block
//! behavior will pose (`World`'s per-kind tick functions are signed against
//! `World`/`self.block_entities`, not any block-store abstraction) is
//! flagged in the task brief for the phase that takes it on.

use std::collections::HashMap;

use super::multiblock::Rotation;
use super::template::Template;
use super::*;
use crate::planet::{BlockPos, Face};
use crate::registry::{BlockId, Registry};

/// Uniquely identifies a spawned [`LocalStructure`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LocalStructureId(pub u64);

/// Where a [`LocalStructure`] sits in the main world and how it is oriented.
/// Static in this phase; path-following motion is explicitly deferred.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalTransform {
    pub anchor: BlockPos,
    pub rotation: Rotation,
}

/// A self-contained, non-chunk-grid block store, born from a [`Template`].
#[derive(Clone, Debug)]
pub struct LocalStructure {
    pub id: LocalStructureId,
    /// Which template this was spawned from (save/debug label).
    pub name: String,
    /// Local-offset cells. Keys are `(du, dy, dv)` relative to the
    /// structure's own origin, stored in canonical, as-captured
    /// orientation. Never rotated in place — consult `transform.rotation`
    /// and apply it at resolution time (see
    /// [`LocalStructure::world_position`]). Mirrors
    /// `Template`/`rotated_cells`.
    pub blocks: HashMap<(i32, i32, i32), BlockId>,
    pub transform: LocalTransform,
}

/// Build a [`LocalStructure`] from a saved template, resolving block names
/// to ids exactly the way `rotated_cells` does for stamps. The result is
/// oriented to the template (`Rotation::R0`) and parked at the world origin;
/// [`World::spawn_structure`] assigns the real id, anchor, and rotation.
pub fn from_template(template: &Template, reg: &Registry) -> LocalStructure {
    let mut blocks = HashMap::with_capacity(template.cells.len());
    for cell in &template.cells {
        if let Some(block) = reg.block_id(&cell.block) {
            blocks.insert((cell.du, cell.dy, cell.dv), block);
        }
    }
    LocalStructure {
        id: LocalStructureId(0),
        name: template.name.clone(),
        blocks,
        transform: LocalTransform {
            anchor: BlockPos::new(Face::PosZ, 0, 0, 0)
                .expect("the world origin is a valid block position"),
            rotation: Rotation::R0,
        },
    }
}

/// Forward-looking API for Phase 6 (rendering/movement will call
/// `world_position`/`rotate`); only tests touch it today.
#[allow(dead_code)]
impl LocalStructure {
    /// Number of stored cells.
    pub fn cell_count(&self) -> usize {
        self.blocks.len()
    }

    /// The block at a local offset, or `AIR` when the cell is empty.
    pub fn get_block(&self, offset: (i32, i32, i32)) -> BlockId {
        self.blocks.get(&offset).copied().unwrap_or(AIR)
    }

    /// Set (or clear, with `AIR`) the block at a local offset.
    pub fn set_block(&mut self, offset: (i32, i32, i32), block: BlockId) {
        if block == AIR {
            self.blocks.remove(&offset);
        } else {
            self.blocks.insert(offset, block);
        }
    }

    /// Rotate the structure: the transform records the composed orientation
    /// (O(1), no cell remapping). Blocks stay canonical — the rotation is
    /// applied only at resolution time, exactly like
    /// `Template`/`rotated_cells`.
    pub fn rotate(&mut self, rot: Rotation) {
        self.transform.rotation = compose_rotation(self.transform.rotation, rot);
    }

    /// The world position of a local cell, applying the current rotation
    /// exactly once. The only correct way to resolve a `blocks` key to a
    /// world position — mirrors `template.rs::rotated_cells`.
    pub fn world_position(&self, local_offset: (i32, i32, i32)) -> Option<BlockPos> {
        let rotated = self.transform.rotation.apply(local_offset);
        self.transform
            .anchor
            .offset(rotated.0, rotated.1, rotated.2)
    }
}

/// Compose two quarter-turns around the vertical axis (apply `a`, then `b`).
#[allow(dead_code)]
fn compose_rotation(a: Rotation, b: Rotation) -> Rotation {
    use Rotation::{R0, R90, R180, R270};
    match (a, b) {
        (R0, r) => r,
        (r, R0) => r,
        (R90, R90) => R180,
        (R90, R180) => R270,
        (R90, R270) => R0,
        (R180, R90) => R270,
        (R180, R180) => R0,
        (R180, R270) => R90,
        (R270, R90) => R0,
        (R270, R180) => R90,
        (R270, R270) => R180,
    }
}

/// Versioned on-disk shape of the local-structure collection, mirroring
/// `TemplateFile` in `template.rs` exactly.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct StructureFile {
    version: u32,
    structures: Vec<SavedStructure>,
}

const STRUCTURE_FILE_VERSION: u32 = 1;

/// One persisted structure. Cells are stored as named offsets (like
/// [`TemplateCell`]) so the library survives registry remaps; the in-memory
/// `BlockId` store is rebuilt on load.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct SavedStructure {
    id: u64,
    name: String,
    cells: Vec<super::template::TemplateCell>,
    anchor: (Face, u16, u8, u16),
    rotation: Rotation,
}

impl World {
    /// Persist the spawned local structures to `local_structures.toml`.
    pub(super) fn save_local_structures(&self) -> std::io::Result<()> {
        let structures: Vec<SavedStructure> = self
            .local_structures
            .iter()
            .map(|structure| {
                let mut cells: Vec<super::template::TemplateCell> = structure
                    .blocks
                    .iter()
                    .map(|((du, dy, dv), block)| super::template::TemplateCell {
                        du: *du,
                        dy: *dy,
                        dv: *dv,
                        block: self.reg.block(*block).name.clone(),
                    })
                    .collect();
                cells.sort_by_key(|cell| (cell.du, cell.dy, cell.dv));
                SavedStructure {
                    id: structure.id.0,
                    name: structure.name.clone(),
                    cells,
                    anchor: (
                        structure.transform.anchor.face(),
                        structure.transform.anchor.u(),
                        structure.transform.anchor.y(),
                        structure.transform.anchor.v(),
                    ),
                    rotation: structure.transform.rotation,
                }
            })
            .collect();
        let text = toml::to_string_pretty(&StructureFile {
            version: STRUCTURE_FILE_VERSION,
            structures,
        })
        .map_err(std::io::Error::other)?;
        crate::identity::atomic_write(
            &self.save_dir.join("local_structures.toml"),
            text.as_bytes(),
            false,
        )
    }

    /// Load spawned local structures from `local_structures.toml`. A
    /// missing or mismatched-version file is a silent no-op, exactly like
    /// the template library.
    pub(super) fn load_local_structures(&mut self) {
        let Ok(text) = std::fs::read_to_string(self.save_dir.join("local_structures.toml")) else {
            return;
        };
        let Ok(file) = toml::from_str::<StructureFile>(&text) else {
            return;
        };
        if file.version != STRUCTURE_FILE_VERSION {
            return;
        }
        let mut loaded = Vec::new();
        let mut next = 0u64;
        for saved in file.structures {
            let Some(anchor) = BlockPos::new(
                saved.anchor.0,
                saved.anchor.1,
                saved.anchor.2,
                saved.anchor.3,
            )
            .ok() else {
                continue;
            };
            let mut blocks = HashMap::new();
            for cell in saved.cells {
                if let Some(block) = self.reg.block_id(&cell.block) {
                    blocks.insert((cell.du, cell.dy, cell.dv), block);
                }
            }
            next = next.max(saved.id + 1);
            loaded.push(LocalStructure {
                id: LocalStructureId(saved.id),
                name: saved.name,
                blocks,
                transform: LocalTransform {
                    anchor,
                    rotation: saved.rotation,
                },
            });
        }
        self.local_structures = loaded;
        self.next_local_structure_id = self.next_local_structure_id.max(next);
    }

    #[allow(dead_code)]
    pub fn local_structures(&self) -> &[LocalStructure] {
        &self.local_structures
    }

    #[allow(dead_code)]
    pub fn local_structure(&self, id: LocalStructureId) -> Option<&LocalStructure> {
        self.local_structures
            .iter()
            .find(|structure| structure.id == id)
    }

    /// Remove a spawned structure by id (and persist). Returns whether one
    /// was removed; a nonexistent id is a clean no-op.
    pub fn remove_structure(&mut self, id: LocalStructureId) -> bool {
        let before = self.local_structures.len();
        self.local_structures.retain(|structure| structure.id != id);
        let removed = self.local_structures.len() < before;
        if removed {
            let _ = self.save_local_structures();
        }
        removed
    }

    /// Spawn a [`LocalStructure`] from a saved template at `anchor`,
    /// oriented by `rot`, without touching the main chunk grid at all — the
    /// key behavioral difference from `stamp_instant`/`stamp_ghost`, both of
    /// which write into `World`'s real block storage. Persists the new
    /// structure immediately.
    pub fn spawn_structure(
        &mut self,
        template: &Template,
        anchor: BlockPos,
        rot: Rotation,
    ) -> Result<LocalStructureId, String> {
        if template.cells.is_empty() {
            return Err("template has no cells".into());
        }
        let mut structure = from_template(template, &self.reg);
        structure.id = LocalStructureId(self.next_local_structure_id);
        structure.transform.anchor = anchor;
        structure.transform.rotation = rot;
        self.next_local_structure_id += 1;
        let id = structure.id;
        self.local_structures.push(structure);
        self.save_local_structures()
            .map_err(|error| format!("saved in memory but not to disk: {error}"))?;
        Ok(id)
    }
}
