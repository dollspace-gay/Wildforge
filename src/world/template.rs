//! Capture & stamp tooling (spec Part 1.4).
//!
//! A player selects a built region, captures it as a reusable structural
//! template, and stamps a copy elsewhere — either instantly (materials
//! permitting, all-or-nothing on shortfall) or as a fill-in-yourself ghost
//! overlay.
//!
//! ## Scope decision: structure only
//!
//! Templates record block layout (offset + block name) and nothing else.
//! [`BlockEntity`] contents are deliberately not copied: a captured chest is
//! an empty chest block, a captured forge an empty unlit shell. That closes
//! the duplication-exploit surface (copying chest goods, machine charge/fuel,
//! binding-frame mounts, charge-vessel reservoirs) without an allowlist of
//! which entity fields are "safe" to copy. Since Phase 3 resolved slot state
//! as fully re-derivable from a live world scan, a captured machine template
//! with an installed module *is* simply that block at that position — the
//! module's block identity rides along with the raw cells, no special-casing.
//!
//! ## Storage: shared, world-wide
//!
//! Templates are persisted to `templates.toml` on the world save directory
//! and are shared: any player can stamp any saved template. This mirrors the
//! codebase having no per-player land-claim/permission concept to respect.

use std::collections::{BTreeMap, HashMap};

use super::multiblock::{MachineKind, Rotation};
use super::*;
use crate::inventory::Inventory;
use crate::planet::BlockPos;
use crate::registry::{BlockId, ItemId, Registry};

/// Upper bound on a single capture, guarding a slip-of-the-hand megastructure
/// from ballooning into an unbounded cell list (and an unchecked save file).
pub const MAX_CAPTURE_CELLS: usize = 4096;

/// One stored voxel of a template: an offset from the template origin plus
/// the named block. Names survive registry remaps where runtime ids do not.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TemplateCell {
    pub du: i32,
    pub dy: i32,
    pub dv: i32,
    pub block: String,
}

/// A captured region, persisted by name. Cells are offsets relative to the
/// capture's first corner (offset `0,0,0`), which becomes the default stamp
/// anchor.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Template {
    pub name: String,
    pub cells: Vec<TemplateCell>,
}

impl Template {
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// Bounding box (`du`, `dy`, `dv`) of the captured cells.
    pub fn dims(&self) -> (i32, i32, i32) {
        let (mut du, mut dy, mut dv) = (0, 0, 0);
        for c in &self.cells {
            du = du.max(c.du);
            dy = dy.max(c.dy);
            dv = dv.max(c.dv);
        }
        (du, dy, dv)
    }

    /// Aggregate block-name → count cost table, used for the instant-stamp
    /// affordability pre-check and for reporting what a stamp needs.
    pub fn cost(&self) -> BTreeMap<String, u32> {
        let mut cost: BTreeMap<String, u32> = BTreeMap::new();
        for cell in &self.cells {
            *cost.entry(cell.block.clone()).or_default() += 1;
        }
        cost
    }

    fn cost_text(&self) -> String {
        self.cost()
            .into_iter()
            .map(|(block, n)| format!("{n} {block}"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// One active ghost overlay: the world cells a player still has to place,
/// keyed by absolute position, mapped to the required block name. Pure
/// server-side world state — a separate presentation task renders it.
#[derive(Clone, Debug)]
pub struct PendingFill {
    pub anchor: BlockPos,
    pub remaining: HashMap<BlockPos, String>,
}

impl PendingFill {
    pub fn remaining_count(&self) -> usize {
        self.remaining.len()
    }
}

/// Versioned on-disk shape of the template library. The version field is
/// read on load so a future format change can be detected rather than
/// misread as an empty library.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct TemplateFile {
    version: u32,
    templates: Vec<Template>,
}

const TEMPLATE_FILE_VERSION: u32 = 1;

/// Resolve a block to the item a player would hold to place it: the block's
/// own item when it has one, else the auto-generated `/place` item. `None`
/// means the block has nothing a player can carry.
fn block_to_item(reg: &Registry, block: BlockId) -> Option<ItemId> {
    let name = reg.block(block).name.as_str();
    reg.item_id(name)
        .or_else(|| reg.item_id(&format!("{name}/place")))
}

/// Build a reusable template from the inclusive `corner_a..=corner_b` box.
/// Both corners must share a planet face. Air, fluids, the unknown
/// placeholder, and any block with no placeable item are dropped — a
/// template must be reproducible, and an unplaceable cell would guarantee an
/// asymmetric round-trip.
pub fn capture_region(
    world: &World,
    corner_a: BlockPos,
    corner_b: BlockPos,
    name: &str,
) -> Option<Template> {
    if corner_a.face() != corner_b.face() {
        return None;
    }
    let u0 = corner_a.u().min(corner_b.u());
    let u1 = corner_a.u().max(corner_b.u());
    let v0 = corner_a.v().min(corner_b.v());
    let v1 = corner_a.v().max(corner_b.v());
    let y0 = corner_a.y().min(corner_b.y());
    let y1 = corner_a.y().max(corner_b.y());
    let mut cells = Vec::new();
    for u in u0..=u1 {
        for v in v0..=v1 {
            for y in y0..=y1 {
                let Ok(pos) = BlockPos::new(corner_a.face(), u, y, v) else {
                    continue;
                };
                let block = world.get_block_at(pos);
                if block == AIR
                    || world.reg.is_water(block)
                    || world.reg.is_lava(block)
                    || block == world.reg.unknown_block
                {
                    continue;
                }
                let block_name = world.reg.block(block).name.clone();
                if block_to_item(&world.reg, block).is_none() {
                    continue;
                }
                cells.push(TemplateCell {
                    du: i32::from(u) - i32::from(corner_a.u()),
                    dy: i32::from(y) - i32::from(corner_a.y()),
                    dv: i32::from(v) - i32::from(corner_a.v()),
                    block: block_name,
                });
                if cells.len() > MAX_CAPTURE_CELLS {
                    return None;
                }
            }
        }
    }
    Some(Template {
        name: name.to_string(),
        cells,
    })
}

/// Which `MachineKind`, if any, a placed block's interaction makes it the
/// mouth of (capability E7: the interaction names the machine id).
fn machine_kind_for(reg: &Registry, interaction: &Option<String>) -> Option<MachineKind> {
    reg.machine_by_interaction(interaction.as_deref()?)
}

impl World {
    /// Persist the world-shared template library to `templates.toml`.
    pub(super) fn save_templates(&self) -> std::io::Result<()> {
        let text = toml::to_string_pretty(&TemplateFile {
            version: TEMPLATE_FILE_VERSION,
            templates: self.templates.clone(),
        })
        .map_err(std::io::Error::other)?;
        crate::identity::atomic_write(
            &self.save_dir.join("templates.toml"),
            text.as_bytes(),
            false,
        )
    }

    /// Load the world-shared template library from `templates.toml`.
    pub(super) fn load_templates(&mut self) {
        let Ok(text) = std::fs::read_to_string(self.save_dir.join("templates.toml")) else {
            return;
        };
        let Ok(file) = toml::from_str::<TemplateFile>(&text) else {
            return;
        };
        if file.version != TEMPLATE_FILE_VERSION {
            return;
        }
        self.templates = file.templates;
    }

    /// Capture `corner_a..=corner_b` as a named, world-shared template and
    /// persist it. Returns the cell count, or an error naming the problem.
    pub fn capture_and_save(
        &mut self,
        corner_a: BlockPos,
        corner_b: BlockPos,
        name: &str,
    ) -> Result<usize, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("name a template first".into());
        }
        if self.templates.iter().any(|t| t.name == name) {
            return Err(format!("a template named {name} already exists"));
        }
        let Some(template) = capture_region(self, corner_a, corner_b, name) else {
            return Err(
                "the two corners must share a face — pick both on the same side of the world"
                    .into(),
            );
        };
        if template.cells.is_empty() {
            return Err("nothing capturable in that region".into());
        }
        if template.cells.len() > MAX_CAPTURE_CELLS {
            return Err(format!("capture too large (>{MAX_CAPTURE_CELLS} blocks)"));
        }
        let n = template.cells.len();
        self.templates.push(template);
        self.save_templates()
            .map_err(|error| format!("saved in memory but not to disk: {error}"))?;
        Ok(n)
    }

    pub fn templates(&self) -> &[Template] {
        &self.templates
    }

    pub fn template(&self, name: &str) -> Option<&Template> {
        self.templates.iter().find(|t| t.name == name)
    }

    /// Remove a template by name (and persist). Returns whether one was
    /// removed.
    pub fn remove_template(&mut self, name: &str) -> bool {
        let before = self.templates.len();
        self.templates.retain(|t| t.name != name);
        let removed = self.templates.len() < before;
        if removed {
            let _ = self.save_templates();
        }
        removed
    }

    /// Map a template's cell offsets through `rot` and resolve them to live
    /// world positions anchored at `anchor`.
    fn rotated_cells(
        &self,
        template: &Template,
        anchor: BlockPos,
        rot: Rotation,
    ) -> Vec<(BlockPos, BlockId)> {
        let mut out = Vec::with_capacity(template.cells.len());
        for cell in &template.cells {
            let (du, dy, dv) = rot.apply((cell.du, cell.dy, cell.dv));
            let Some(pos) = anchor.offset(du, dy, dv) else {
                continue;
            };
            let Some(block) = self.reg.block_id(&cell.block) else {
                continue;
            };
            out.push((pos, block));
        }
        out
    }

    /// Instant stamp: check the whole template against `inv`, then place
    /// every cell. Nothing is placed and nothing is consumed when the
    /// inventory is short of any required block; the error names the gap.
    pub fn stamp_instant(
        &mut self,
        template: &Template,
        anchor: BlockPos,
        rot: Rotation,
        inv: &mut Inventory,
        creative: bool,
    ) -> Result<String, String> {
        let cells = self.rotated_cells(template, anchor, rot);
        if cells.is_empty() {
            return Err("nothing placeable at that anchor".into());
        }
        // Aggregate the whole template to one item → count table, resolved
        // to the item the player holds. Check first; only then place.
        let mut need: HashMap<ItemId, u32> = HashMap::new();
        for (_, block) in &cells {
            let Some(item) = block_to_item(&self.reg, *block) else {
                continue;
            };
            *need.entry(item).or_default() += 1;
        }
        let cost: Vec<(ItemId, u32)> = need.into_iter().collect();
        if !creative {
            let short: Vec<String> = cost
                .iter()
                .filter(|(item, n)| inv.count_of(*item) < *n)
                .map(|(item, n)| {
                    format!(
                        "{} (need {}, have {})",
                        self.reg.item(*item).name,
                        n,
                        inv.count_of(*item)
                    )
                })
                .collect();
            if !short.is_empty() {
                return Err(format!("short on: {}", short.join(", ")));
            }
        }
        // Place everything, consuming only what actually lands.
        let mut placed = 0usize;
        let mut consumed: HashMap<ItemId, u32> = HashMap::new();
        for (pos, block) in &cells {
            if self.place_block_at(*pos, *block) {
                placed += 1;
                if let Some(item) = block_to_item(&self.reg, *block) {
                    *consumed.entry(item).or_default() += 1;
                }
            }
        }
        if !creative && !consumed.is_empty() {
            let cost: Vec<(ItemId, u32)> = consumed.into_iter().collect();
            if !inv.try_consume(&cost) {
                return Err("could not consume materials from inventory".into());
            }
        }
        self.finalize_stamp_shells(&cells);
        Ok(format!("placed {placed} of {} blocks", cells.len()))
    }

    /// A builder's stamp (spec 3.6): place every cell through the ordinary
    /// block path with no inventory — the mob builds for free. Cells that
    /// cannot land are skipped. Returns the number of blocks placed.
    pub fn stamp_mob(&mut self, template: &Template, anchor: BlockPos, rot: Rotation) -> usize {
        let cells = self.rotated_cells(template, anchor, rot);
        let mut placed = 0usize;
        for (pos, block) in &cells {
            if self.place_block_at(*pos, *block) {
                placed += 1;
            }
        }
        self.finalize_stamp_shells(&cells);
        placed
    }

    /// Ghost stamp: register a pending fill at `anchor` — every template
    /// cell the world does not already hold. No blocks are placed; ordinary
    /// placement of the correct block at a pending cell clears it (see
    /// [`World::clear_pending_fill_at`]), and once the last cell lands the
    /// fill disappears with no activation step — Phase 1–3 revalidation has
    /// been following along on every edit.
    pub fn stamp_ghost(
        &mut self,
        template: &Template,
        anchor: BlockPos,
        rot: Rotation,
    ) -> Result<String, String> {
        let cells = self.rotated_cells(template, anchor, rot);
        if cells.is_empty() {
            return Err("nothing placeable at that anchor".into());
        }
        let mut remaining = HashMap::new();
        for (pos, block) in &cells {
            let name = self.reg.block(*block).name.clone();
            if self.get_block_at(*pos) == *block {
                continue;
            }
            remaining.insert(*pos, name);
        }
        if remaining.is_empty() {
            return Err("nothing left to fill — the structure is already there".into());
        }
        let fill = PendingFill { anchor, remaining };
        let n = fill.remaining_count();
        self.pending_fills.retain(|fill| fill.anchor != anchor);
        self.pending_fills.push(fill);
        Ok(format!("ghost overlay marked {n} cells to fill"))
    }

    pub fn pending_fill_at(&self, anchor: BlockPos) -> Option<&PendingFill> {
        self.pending_fills.iter().find(|fill| fill.anchor == anchor)
    }

    /// Cancel the ghost overlay anchored at `pos`. Returns whether one was
    /// removed.
    pub fn cancel_fill(&mut self, anchor: BlockPos) -> bool {
        let before = self.pending_fills.len();
        self.pending_fills.retain(|fill| fill.anchor != anchor);
        self.pending_fills.len() < before
    }

    /// Hook called from the authoritative block-edit path: a pending cell is
    /// cleared exactly when the voxel holds the required block. Any other
    /// write is an ordinary edit and leaves the fill as-is (the player can
    /// still brute-force-fill by eye).
    pub(super) fn clear_pending_fill_at(&mut self, pos: BlockPos, block: BlockId) {
        let name = self.reg.block(block).name.clone();
        self.pending_fills.retain_mut(|fill| {
            if fill
                .remaining
                .get(&pos)
                .is_some_and(|required| *required == name)
            {
                fill.remaining.remove(&pos);
            }
            !fill.remaining.is_empty()
        });
    }

    /// After a stamp's blocks are all placed, register a shell entity at each
    /// machine mouth and re-fold the instance so a stamped forge is a *live*
    /// empty unlit shell (spec Part 1.4, acceptance: no `BlockEntity`
    /// contents restored). Revalidation emitters fire on every cell edit
    /// during the stamp; this last pass catches the shells born only after
    /// their full extent was already on the floor.
    fn finalize_stamp_shells(&mut self, cells: &[(BlockPos, BlockId)]) {
        let mut mouths: Vec<(BlockPos, MachineKind)> = Vec::new();
        for (pos, block) in cells {
            let kind = self.reg.block(*block).interaction.clone();
            if let Some(kind) = machine_kind_for(&self.reg, &kind) {
                mouths.push((*pos, kind));
            }
        }
        for (pos, kind) in &mouths {
            self.ensure_block_entity_at(
                *pos,
                BlockEntity::Multiblock(MachineInstance {
                    kind: *kind,
                    ..Default::default()
                }),
            );
        }
        for (pos, _) in &mouths {
            self.revalidate_multiblocks_around(*pos);
        }
    }

    /// Minimal whole-world command surface for capture/stamp operations that
    /// need no player inventory. Positions are `face u y v` (the coordinates
    /// the world actually uses); the game layer passes the player's inventory
    /// in separately for `stamp` via [`World::stamp_instant`].
    ///
    /// Commands: `capture <name> <face> <u> <y> <v> <face2> <u2> <y2> <v2>`,
    /// `ghost <name> <face> <u> <y> <v> [rot]`, `cost <name>`,
    /// `list`, `drop <name>`, `cancel <face> <u> <y> <v>`,
    /// `spawn <template_name> <face> <u> <y> <v> [rot]`,
    /// `despawn <id>`. `rot` is one of `r0|r90|r180|r270`.
    pub fn template_command(&mut self, line: &str) -> Vec<String> {
        let mut tokens = line.split_whitespace();
        let Some(cmd) = tokens.next().map(str::to_ascii_lowercase) else {
            return vec![
                "templates: capture | ghost | cost | list | drop | cancel | stamp | spawn | despawn"
                    .into(),
            ];
        };
        let reply = |msg: String| vec![msg];
        match cmd.as_str() {
            "list" => {
                let library = self.templates();
                if library.is_empty() {
                    return reply("no saved templates".into());
                }
                let names: Vec<String> = library
                    .iter()
                    .map(|t| {
                        let (du, dy, dv) = t.dims();
                        format!(
                            "{} ({} blocks, {}x{}x{})",
                            t.name,
                            t.cell_count(),
                            du + 1,
                            dy + 1,
                            dv + 1
                        )
                    })
                    .collect();
                reply(format!("templates: {}", names.join(", ")))
            }
            "cost" => {
                let Some(name) = tokens.next() else {
                    return reply("usage: cost <name>".into());
                };
                match self.template(name) {
                    Some(t) => reply(format!("{name} needs: {}", t.cost_text())),
                    None => reply(format!("no template named {name}")),
                }
            }
            "drop" => {
                let Some(name) = tokens.next() else {
                    return reply("usage: drop <name>".into());
                };
                if self.remove_template(name) {
                    reply(format!("dropped template {name}"))
                } else {
                    reply(format!("no template named {name}"))
                }
            }
            "despawn" => {
                let Some(token) = tokens.next() else {
                    return reply("usage: despawn <id>".into());
                };
                let Ok(id) = token.parse::<u64>() else {
                    return reply(format!("bad structure id {token}"));
                };
                let id = crate::world::local_structure::LocalStructureId(id);
                if self.remove_structure(id) {
                    reply(format!("despawned structure #{}", id.0))
                } else {
                    reply(format!("no structure with id {}", id.0))
                }
            }
            "cancel" => {
                let Some(pos) = parse_block_pos(&mut tokens) else {
                    return reply("usage: cancel <face> <u> <y> <v>".into());
                };
                if self.pending_fill_at(pos).is_none() {
                    return reply("no ghost overlay at that anchor".into());
                }
                self.cancel_fill(pos);
                reply("cancelled ghost overlay".into())
            }
            "capture" => {
                let Some(name) = tokens.next() else {
                    return reply(
                        "usage: capture <name> <face> <u> <y> <v> <face2> <u2> <y2> <v2>".into(),
                    );
                };
                let Some(a) = parse_block_pos(&mut tokens) else {
                    return reply("capture: bad first corner — <face> <u> <y> <v>".into());
                };
                let Some(b) = parse_block_pos(&mut tokens) else {
                    return reply("capture: bad second corner — <face> <u> <y> <v>".into());
                };
                match self.capture_and_save(a, b, name) {
                    Ok(n) => reply(format!("captured {name}: {n} blocks")),
                    Err(e) => reply(format!("capture {name}: {e}")),
                }
            }
            "ghost" => {
                let Some(name) = tokens.next() else {
                    return reply("usage: ghost <name> <face> <u> <y> <v> [rot]".into());
                };
                let Some(pos) = parse_block_pos(&mut tokens) else {
                    return reply("ghost: bad anchor — <face> <u> <y> <v> [rot]".into());
                };
                let rot = parse_rot(tokens.next()).unwrap_or(Rotation::R0);
                let Some(t) = self.template(name).cloned() else {
                    return reply(format!("no template named {name}"));
                };
                match self.stamp_ghost(&t, pos, rot) {
                    Ok(msg) => reply(msg),
                    Err(e) => reply(format!("ghost {name}: {e}")),
                }
            }
            "spawn" => {
                let Some(name) = tokens.next() else {
                    return reply("usage: spawn <template_name> <face> <u> <y> <v> [rot]".into());
                };
                let Some(pos) = parse_block_pos(&mut tokens) else {
                    return reply("spawn: bad anchor — <face> <u> <y> <v> [rot]".into());
                };
                let rot = parse_rot(tokens.next()).unwrap_or(Rotation::R0);
                let Some(t) = self.template(name).cloned() else {
                    return reply(format!("no template named {name}"));
                };
                match self.spawn_structure(&t, pos, rot) {
                    Ok(id) => reply(format!("spawned {name} as structure #{} at {pos:?}", id.0)),
                    Err(e) => reply(format!("spawn {name}: {e}")),
                }
            }
            _ => reply(format!("templates: unknown command {cmd}")),
        }
    }
}

pub(crate) fn parse_rot(token: Option<&str>) -> Option<Rotation> {
    match token?.to_ascii_lowercase().as_str() {
        "r0" | "0" => Some(Rotation::R0),
        "r90" | "90" => Some(Rotation::R90),
        "r180" | "180" => Some(Rotation::R180),
        "r270" | "270" => Some(Rotation::R270),
        _ => None,
    }
}

/// Parse `face u y v` into a [`BlockPos`], consuming exactly four tokens
/// from the iterator.
pub(crate) fn parse_block_pos<'a, I: Iterator<Item = &'a str>>(tokens: &mut I) -> Option<BlockPos> {
    let face = crate::planet::Face::from_name(tokens.next()?)?;
    let u: u16 = tokens.next()?.parse().ok()?;
    let y: u8 = tokens.next()?.parse().ok()?;
    let v: u16 = tokens.next()?.parse().ok()?;
    BlockPos::new(face, u, y, v).ok()
}
