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
use std::sync::Arc;

use super::multiblock::{BlockRead, BlockStore, MachineKind, Rotation};
use super::template::Template;
use super::*;
use crate::inventory::ItemStack;
use crate::planet::{BlockPos, Face};
use crate::planet_atlas::LocalWeatherSample;
use crate::registry::{AIR, BlockId, ItemId, Registry};

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

/// Optional rail-following motion (spec Part 2.2, scoped). A structure with
/// `None` here is fully static, exactly as Phase 5 left it; with `Some(_)`
/// the per-tick rail step advances it along connected rail cells. Motion
/// state is transient runtime data — it is not persisted with the structure
/// (a reloaded structure is static until re-railed).
#[derive(Clone, Debug, PartialEq)]
pub struct RailState {
    /// The rail cell the structure is currently departing.
    pub current_cell: BlockPos,
    /// The rail cell it is traveling toward.
    pub next_cell: BlockPos,
    /// 0.0 at `current_cell`, 1.0 at `next_cell`.
    pub progress: f32,
    /// Cells per second. The deferred mass-driven power-draw phase must
    /// agree with this unit.
    pub speed: f32,
}

/// A self-contained, non-chunk-grid block store, born from a [`Template`].
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
    /// The registry this structure's block names resolve against, shared
    /// with the host world. The multiblock layer needs it: shape matching
    /// resolves `Tag`/`Module` constraints through the registry.
    pub reg: Arc<Registry>,
    /// Block-entity state hosted inside the structure, keyed by local
    /// offset (7b-1) — the machine-state counterpart to `blocks`. Without
    /// it a full forge shape inside a structure would stay inert raw
    /// blocks with nowhere to hold its `MachineInstance`.
    pub block_entities: HashMap<(i32, i32, i32), BlockEntity>,
    /// Produced outputs from structure-hosted machines collect here: a
    /// structure has no loose-item world of its own, so completion stays
    /// observable (and testable) without inventing a drop system.
    pub outbox: Vec<ItemStack>,
    pub transform: LocalTransform,
    /// Where this structure is going, if it is riding rails. `None` = static.
    pub rail: Option<RailState>,
}

/// Build a [`LocalStructure`] from a saved template, resolving block names
/// to ids exactly the way `rotated_cells` does for stamps. The result is
/// oriented to the template (`Rotation::R0`) and parked at the world origin;
/// [`World::spawn_structure`] assigns the real id, anchor, and rotation.
pub fn from_template(template: &Template, reg: &Arc<Registry>) -> LocalStructure {
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
        reg: reg.clone(),
        block_entities: HashMap::new(),
        outbox: Vec::new(),
        transform: LocalTransform {
            anchor: BlockPos::new(Face::PosZ, 0, 0, 0)
                .expect("the world origin is a valid block position"),
            rotation: Rotation::R0,
        },
        rail: None,
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

    /// Set (or clear, with `AIR`) the block at a local offset. A plain
    /// edit inside any registered machine's shell revalidates the
    /// structure-hosted instances the same way the world's 2c edit hook
    /// does — scoped to this structure only, never the main world.
    pub fn set_block(&mut self, offset: (i32, i32, i32), block: BlockId) {
        if block == AIR {
            self.blocks.remove(&offset);
        } else {
            self.blocks.insert(offset, block);
        }
        crate::world::machines::revalidate_machines_around(self, offset);
    }

    /// Total mass of the structure's placed blocks (spec §2.2 power draw).
    /// Folded from actual content each call: an empty car is Empty-tier,
    /// a full freight train climbs the load tiers. `AIR` cells are never
    /// stored, so the empty offset naturally weighs nothing.
    pub fn mass(&self) -> f32 {
        let mut mass = 0.0;
        for block in self.blocks.values() {
            mass += crate::world::power_draw::block_mass(&self.reg, *block);
        }
        mass
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

    /// Break the block at `offset`.  Returns the drop that should be given
    /// to the breaking player (hardness/unbreakable-gated), or `None` if
    /// the cell was already air or cannot be broken.
    ///
    /// The underlying cell is set to AIR and any `BlockEntity` at this
    /// exact offset is removed; the edit revalidates nearby machines via
    /// the same `set_block` hook the world's edit cascade uses.
    pub fn break_block(
        &mut self,
        offset: (i32, i32, i32),
        tool: Option<ItemId>,
    ) -> Option<ItemStack> {
        let block = self.get_block(offset);
        if block == AIR || self.reg.block(block).hardness.is_none() {
            return None;
        }
        let drop = self.reg.drops_for(block, tool).map(|(item, count)| {
            let item = if tool.is_some() {
                self.reg.block(block).dismantles_to.unwrap_or(item)
            } else {
                item
            };
            ItemStack::new(&self.reg, item, count)
        });
        self.block_entities.remove(&offset);
        self.set_block(offset, AIR);
        drop
    }

    /// Place `block` at `offset`.  Returns whether the placement was
    /// accepted (the cell was replaceable).  The edit revalidates nearby
    /// machines exactly as `set_block` already does.
    pub fn place_block(&mut self, offset: (i32, i32, i32), block: BlockId) -> bool {
        if self.reg.block(block).hardness.is_none() {
            return false;
        }
        let current = self.get_block(offset);
        if !self.reg.is_replaceable(current) {
            return false;
        }
        self.set_block(offset, block);
        true
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

impl BlockRead for LocalStructure {
    type Pos = (i32, i32, i32);

    fn get_block(&self, pos: Self::Pos) -> BlockId {
        self.blocks.get(&pos).copied().unwrap_or(AIR)
    }

    /// Plain tuple addition: a local store has no shell, so an offset can
    /// never leave the world and never fails (unlike `BlockPos::offset`).
    fn offset(&self, pos: Self::Pos, d: (i32, i32, i32)) -> Option<Self::Pos> {
        Some((pos.0 + d.0, pos.1 + d.1, pos.2 + d.2))
    }

    fn cell_delta(&self, from: Self::Pos, to: Self::Pos) -> Option<(i32, i32, i32)> {
        Some((to.0 - from.0, to.1 - from.1, to.2 - from.2))
    }

    fn block_entities(&self) -> &HashMap<Self::Pos, BlockEntity> {
        &self.block_entities
    }

    fn reg(&self) -> &Arc<Registry> {
        &self.reg
    }

    fn to_world(&self, pos: Self::Pos) -> Option<BlockPos> {
        self.world_position(pos)
    }

    /// A chunkless structure has no light model, so its machines read as
    /// permanently unroofed — the same `(0, 15)` open-sky sample a
    /// structure-local `light_at_pos` would return for any cell. (Rain
    /// never douses an in-structure fire anyway: `weather_at` reports
    /// fair weather.) Consistent by design with the structure being
    /// enclosed, not weather-exposed.
    fn open_sky_above(&self, _core: BlockPos) -> bool {
        true
    }

    /// Structures report fair weather: a structure-hosted machine is
    /// exempt from the world's storm dousing.
    fn weather_at(&self, _at: BlockPos) -> LocalWeatherSample {
        LocalWeatherSample::default()
    }
}

impl BlockStore for LocalStructure {
    fn block_entities_mut(&mut self) -> &mut HashMap<Self::Pos, BlockEntity> {
        &mut self.block_entities
    }

    fn swap_block_keep_entity(&mut self, pos: Self::Pos, block_name: &str) {
        let Some(to) = self.reg.block_id(block_name) else {
            return;
        };
        let e = self.block_entities.remove(&pos);
        self.blocks.insert(pos, to);
        if let Some(e) = e {
            self.block_entities.insert(pos, e);
        }
    }

    /// Structure-hosted machines are exempt from the main world's economy
    /// accounting by design: there is no structure-local ledger.
    fn material_ledger(&mut self) -> Option<&mut crate::materials::MaterialLedger> {
        None
    }

    /// A structure has no loose-item world; produced outputs collect in
    /// its `outbox`, keeping completion observable without a drop system.
    fn push_drop_at(&mut self, _at: BlockPos, stack: ItemStack) {
        self.outbox.push(stack);
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
/// `BlockId` store is rebuilt on load. `machines` carries the structure's
/// block-entity state (7b-1) — a machine's charge/fuel/progress survives a
/// save/reload round-trip, exactly as it does for world-hosted machines.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct SavedStructure {
    id: u64,
    name: String,
    cells: Vec<super::template::TemplateCell>,
    anchor: (Face, u16, u8, u16),
    rotation: Rotation,
    #[serde(default)]
    machines: Vec<SavedMachine>,
}

/// One persisted structure-hosted machine, mirroring `entities.toml`'s
/// `[[machine]]` record but keyed by local offset instead of `BlockPos`.
/// The `core` is stored in world coordinates, the same value the live
/// entity holds (structures are static in this phase, so it stays valid).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct SavedMachine {
    offset: (i32, i32, i32),
    kind: String,
    lit: bool,
    #[serde(default)]
    progress: f32,
    #[serde(default)]
    core: Option<crate::planet::BlockPos>,
    #[serde(default)]
    powder: u32,
    #[serde(default)]
    separator_fuel: u32,
    #[serde(default)]
    neodymium: u32,
    #[serde(default)]
    cerium: u32,
    #[serde(default)]
    slot: Vec<SavedMachineSlot>,
    #[serde(default)]
    reclaim: Vec<SavedReclaim>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct SavedMachineSlot {
    index: usize,
    item: String,
    count: u32,
    durability: u32,
    #[serde(default)]
    arcane_id: u64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct SavedReclaim {
    material: String,
    units: u64,
}

/// Build the persisted form of one structure-hosted machine entity.
fn save_machine(reg: &Registry, offset: (i32, i32, i32), m: &MachineInstance) -> SavedMachine {
    let slot = |index: usize, s: &Option<ItemStack>| {
        s.as_ref().map(|s| SavedMachineSlot {
            index,
            item: reg.item(s.item).name.clone(),
            count: s.count,
            durability: s.durability,
            arcane_id: s.arcane_id,
        })
    };
    let mut slots = Vec::new();
    for (i, s) in m.charge.iter().enumerate() {
        if let Some(s) = slot(i, s) {
            slots.push(s);
        }
    }
    if let Some(s) = slot(4, &m.reagent) {
        slots.push(s);
    }
    for (i, s) in m.fuel.iter().enumerate() {
        if let Some(s) = slot(i + 5, s) {
            slots.push(s);
        }
    }
    slots.sort_by_key(|s| s.index);
    SavedMachine {
        offset,
        kind: reg
            .machine(m.kind)
            .map(|def| def.id.clone())
            .unwrap_or_default(),
        lit: m.lit,
        progress: m.progress,
        core: m.core,
        powder: m.powder,
        separator_fuel: m.separator_fuel,
        neodymium: m.neodymium,
        cerium: m.cerium,
        slot: slots,
        reclaim: m
            .reclaim
            .iter()
            .map(|(material, units)| SavedReclaim {
                material: material.clone(),
                units: *units,
            })
            .collect(),
    }
}

impl World {
    /// Persist the spawned local structures to `local_structures.toml`.
    pub(super) fn save_local_structures(&self) -> std::io::Result<()> {
        let structures: Vec<SavedStructure> = self
            .construction
            .structures()
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
                let mut machines: Vec<SavedMachine> = structure
                    .block_entities
                    .iter()
                    .filter_map(|(&offset, entity)| match entity {
                        BlockEntity::Multiblock(m) => Some(save_machine(&self.reg, offset, m)),
                        _ => None,
                    })
                    .collect();
                machines.sort_by_key(|m| m.offset);
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
                    machines,
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
            let mut machines = HashMap::new();
            for sm in saved.machines {
                let Some(kind) = MachineKind::from_name(&self.reg, &sm.kind) else {
                    continue;
                };
                let mut state = MachineInstance {
                    kind,
                    lit: sm.lit,
                    progress: sm.progress,
                    core: sm.core,
                    powder: sm.powder,
                    separator_fuel: sm.separator_fuel,
                    neodymium: sm.neodymium,
                    cerium: sm.cerium,
                    ..Default::default()
                };
                for material in sm.reclaim {
                    if material.units != 0 {
                        *state.reclaim.entry(material.material).or_default() += material.units;
                    }
                }
                for sl in sm.slot {
                    if let Some(item) = self.reg.item_id(&sl.item) {
                        let st = Some(ItemStack {
                            item,
                            count: sl.count,
                            durability: sl.durability,
                            arcane_id: sl.arcane_id,
                        });
                        match sl.index {
                            0..=3 => state.charge[sl.index] = st,
                            4 => state.reagent = st,
                            5..=8 => state.fuel[sl.index - 5] = st,
                            _ => {}
                        }
                    }
                }
                machines.insert(sm.offset, BlockEntity::Multiblock(state));
            }
            next = next.max(saved.id + 1);
            let mut structure = LocalStructure {
                id: LocalStructureId(saved.id),
                name: saved.name,
                blocks,
                reg: self.reg.clone(),
                block_entities: machines,
                outbox: Vec::new(),
                transform: LocalTransform {
                    anchor,
                    rotation: saved.rotation,
                },
                rail: None,
            };
            // Revalidate on load: fold stats and douse any machine whose
            // shell broke while it was saved (mirrors `entities.toml` load).
            let offsets: Vec<(i32, i32, i32)> = structure.block_entities.keys().copied().collect();
            for offset in offsets {
                crate::world::machines::revalidate_machine_at(&mut structure, offset);
            }
            loaded.push(structure);
        }
        self.construction.restore_structures(loaded, next);
    }

    #[allow(dead_code)]
    pub fn local_structures(&self) -> &[LocalStructure] {
        self.construction.structures()
    }

    #[allow(dead_code)]
    pub fn local_structure(&self, id: LocalStructureId) -> Option<&LocalStructure> {
        self.construction.structure(id)
    }

    #[allow(dead_code)]
    pub fn local_structure_mut(&mut self, id: LocalStructureId) -> Option<&mut LocalStructure> {
        self.construction.structure_mut(id)
    }

    /// Remove a spawned structure by id (and persist). Returns whether one
    /// was removed; a nonexistent id is a clean no-op.
    pub fn remove_structure(&mut self, id: LocalStructureId) -> bool {
        let removed = self.construction.remove_structure(id);
        if removed {
            let _ = self.save_local_structures();
        }
        removed
    }

    /// Set (or clear) a structure's rail-following state. Returns whether a
    /// structure with that id exists. Used by tests and by the future
    /// player-triggered "set onto track" path; the step itself is transient.
    #[allow(dead_code)]
    pub fn set_rail(&mut self, id: LocalStructureId, rail: Option<RailState>) -> bool {
        self.construction.set_rail(id, rail)
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
        let id = self
            .construction
            .spawn_structure(&self.reg, template, anchor, rot)?;
        self.save_local_structures()
            .map_err(|error| format!("saved in memory but not to disk: {error}"))?;
        Ok(id)
    }
}
