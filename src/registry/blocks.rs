//! Block identities, runtime definitions, and emission color resolution.

use super::{ArcaneContentDef, ArcaneEcologyDef, DiscoveryFixtureDef, ItemId, MaterialClass, MaterialVector, ObservationDef, ToolKind};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct BlockId(pub u16);
pub const AIR: BlockId = BlockId(0);

#[derive(Clone, Debug)]
pub struct BlockDef {
    pub name: String,  // "base:stone"
    pub label: String, // "Stone"
    /// Atlas slots per face: +X -X +Y -Y +Z -Z.
    pub tiles: [u16; 6],
    pub hardness: Option<f32>,
    pub tool: Option<ToolKind>,
    pub requires_tool: bool,
    /// Resolved drop (item, count); None = drops nothing.
    pub drops: Option<(ItemId, u32)>,
    pub solid: bool,
    pub opaque: bool,
    /// Right-click behavior: "crafting", "furnace", ...
    pub interaction: Option<String>,
    /// Minimum tool tier for drops when requires_tool is set.
    pub min_tier: u8,
    /// 0 = fluid source, 1..=7 flowing levels. None = not a fluid.
    pub water_level: Option<u8>,
    /// True for the lava chain (water_level then means lava volume).
    pub lava: bool,
    /// Render as two crossed quads instead of a cube (plants).
    pub cross: bool,
    /// How readily this block takes fire, 0 = never. Higher catches
    /// sooner. Wood, leaf and stalk burn; stone, earth and glass do not.
    pub burns: u8,
    /// Stands with nothing beneath it. Fire is the one cross block
    /// that is not a plant: it climbs, and the support rule that keeps
    /// torches honest was knocking out every flame the one below it
    /// had just lit.
    pub floats: bool,
    /// Custom mesh: "obelisk" (tapered pillar) or "signboard"
    /// (board on a post). Render-only; collision stays the cube.
    pub shape: Option<String>,
    /// Crop: (final stage block advances no further). tick advances stages.
    pub crop_next: Option<BlockId>,
    pub crop_chance: f32,
    pub crop_any_soil: bool,
    /// Right-click harvest: (item, count, block it becomes).
    pub harvest: Option<(ItemId, u32, BlockId)>,
    /// Emitted light 0..15 (torches, glowing mod blocks).
    pub light_emit: u8,
    /// Grows into this tree species on random ticks (saplings).
    pub sapling: Option<String>,
    /// Extra chance drop on break: (item, probability).
    pub bonus_drop: Option<(ItemId, f32)>,
    /// Archaeology: (loot table, block it becomes) when brushed.
    pub brush: Option<(String, BlockId)>,
    /// Rendered height 0..1; None = full cube (snow layers are 0.125).
    pub height: Option<f32>,
    /// Falls when unsupported.
    pub falls: bool,
    /// Glazing: a glass roof is a greenhouse.
    pub glass: bool,
    /// Per-channel block-light pass-through (stained glass).
    pub light_filter: [bool; 3],
    /// Per-channel emission (r,g,b), each 0..15. The brightest channel equals
    /// `light_emit`, so a colored light keeps its intensity; the dimmer
    /// channels fall off sooner, warming/cooling the glow with distance.
    pub light_rgb: [u8; 3],
    /// Soil: top-face tiles by fertility quartile (dust, poor, normal,
    /// rich) — the mesher reads the block's meta byte to pick one.
    pub fert_tiles: Option<[u16; 4]>,
    /// Crop rotation family (1..=3); 0 = not a crop. Stamped into the
    /// soil at maturation so monoculture drains harder than rotation.
    pub crop_family: u8,
    pub material_class: MaterialClass,
    pub materials: MaterialVector,
    pub dismantles_to: Option<ItemId>,
    /// Pattern A stat contribution this block lends a matched multiblock
    /// shell. Base firebrick is 1; an "advanced" tier raises it. Folding
    /// is pure data: the sum of the matched shell's cells (see
    /// [`crate::world::multiblock::fold_stats`]), never dispatched per
    /// machine.
    pub heat_retention: u32,
    pub arcane: Option<ArcaneContentDef>,
    pub arcane_ecology: Option<ArcaneEcologyDef>,
    pub observation: Option<ObservationDef>,
    pub discovery_fixture: Option<DiscoveryFixtureDef>,
}

/// Resolve a block's per-channel emission from its level and optional color.
/// The color is hue-normalized so the brightest channel always reaches the
/// full level (preserving the scalar light contract the world/tests rely on).
pub(super) fn resolve_light_rgb(level: u8, color: Option<[f32; 3]>) -> [u8; 3] {
    match color {
        None => [level, level, level],
        Some(c) => {
            let m = c[0].max(c[1]).max(c[2]).max(1e-3);
            let ch = |v: f32| ((level as f32) * (v / m).clamp(0.0, 1.0)).round() as u8;
            [ch(c[0]), ch(c[1]), ch(c[2])]
        }
    }
}

