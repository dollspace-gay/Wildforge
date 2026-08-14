//! Mass-driven power draw (spec §2.2, scoped).
//!
//! WildForge's shaft power is a *boolean with a rate* ([`power_at_pos`]),
//! never a number with a cable. A train car or belt still needs to cost
//! something against that rate, so we meet the model halfway with the
//! spec's quantized load tiers: mass is summed from the structure's actual
//! placed blocks (or a belt's riding items) each tick, quantized into
//! Empty/Light/Medium/Heavy/Overloaded, and each tier maps to a fixed draw
//! rate. No continuous wattage accounting, no wire network — two cheap
//! lookups per structure per tick, even over arbitrarily long rail lines.
//!
//! ## Mass
//!
//! A block's mass comes from its declared `materials` (`MaterialVector`,
//! a `BTreeMap<material, units>`): `Σ material_unit_mass(name) × units`.
//! Blocks that declare no materials fall back to a per-class nominal mass,
//! so a train built from ordinary stone is never free. Item stacks (belt
//! cargo) use the same two rules against `ItemDef.materials`.
//!
//! ## Draw and speed
//!
//! `draw = incline_multiplier × load_tier_rate(tier)`. A structure runs at
//! its tier's speed step while `delivered (power_at_pos) >= draw`; a
//! shortfall steps it one tier heavier (slower), and an Overloaded load is
//! a real stall (speed 0), resuming when the load leaves the tier or power
//! returns. Overloaded is deliberately a jam, not just a cost tier.

use crate::inventory::ItemStack;
use crate::registry::{BlockId, MaterialClass, Registry};

/// Nominal belt speed, cells per second.
pub const BELT_SPEED: f32 = 2.0;

/// The quantized load classes. Heavier is later in the enum, so the
/// "step heavier under a shortfall" walk is a simple successor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LoadTier {
    Empty,
    Light,
    Medium,
    Heavy,
    Overloaded,
}

/// Mass thresholds for the tier boundaries, in the same units `block_mass`
/// / `item_stack_mass` produce. `[Light, Medium, Heavy, Overloaded]`.
pub const TIER_THRESHOLDS: [f32; 4] = [4.0, 12.0, 32.0, 128.0];

/// Fixed draw each tier demands of the shaft line, in `power_at_pos`
/// units (water wheel 1.0, windmill 0.35..=1.6, steam 1.4).
///
/// Empty deliberately draws nothing: a bare car or lightly-loaded belt
/// coasts at nominal speed even on an unpowered line, which keeps ordinary
/// cargo (and the phase-6 motion tests) honest while loaded work has to
/// earn its power.
pub const TIER_DRAW_RATE: [f32; 5] = [0.0, 0.4, 0.8, 1.2, 1.8];

/// Speed step each tier runs at when its draw is met, as a fraction of the
/// structure's nominal speed. Heavier is slower; Overloaded is 0 (stall).
pub const TIER_SPEED_STEP: [f32; 5] = [1.0, 0.85, 0.7, 0.55, 0.0];

/// Per-unit mass by material name. A typical declaration is 1200 units per
/// block (one ingot's worth), so a copper block weighs 1200 × 0.001 = 1.2.
const MATERIAL_UNIT_MASS: &[(&str, f32)] = &[
    ("choirstone", 0.0012),
    ("still_salt", 0.0008),
    ("wake_iron", 0.0011),
    ("echo_slate", 0.0012),
    ("copper", 0.0010),
    ("tin", 0.0010),
    ("bronze", 0.0011),
    ("iron", 0.0011),
    ("cobalt", 0.0011),
    ("chromium", 0.0011),
    ("manganese", 0.0011),
    ("mercury", 0.0017),
    ("sulfur", 0.0009),
    ("quartz", 0.0010),
    ("amethyst", 0.0018),
    ("coal", 0.0006),
    ("gold", 0.0020),
    ("silver", 0.0014),
    ("lead", 0.0014),
    ("diamond", 0.0022),
    ("uranium", 0.0024),
    ("rare_earth", 0.0016),
    ("salt", 0.0008),
    ("wood", 0.0005),
    ("stone", 0.0010),
];

/// Fallback mass for content that declares no materials, by class. The
/// classes are too coarse for per-material weights, but they keep an
/// un-declared firebrick from weighing nothing while a choirstone still
/// outsizes a plank.
const CLASS_MASS: [(MaterialClass, f32); 5] = [
    (MaterialClass::Renewable, 0.4),
    (MaterialClass::GeologicallyFinite, 1.0),
    (MaterialClass::TransformativeFinite, 0.8),
    (MaterialClass::Consumptive, 0.3),
    (MaterialClass::Exceptional, 1.5),
];

impl LoadTier {
    /// The next-heavier tier (Empty → Light → ... → Overloaded). Under a
    /// power shortfall a structure steps one step heavier, which is also
    /// one step slower — the physical reading of "the load exceeds what the
    /// line delivers".
    pub fn heavier(self) -> LoadTier {
        match self {
            LoadTier::Empty => LoadTier::Light,
            LoadTier::Light => LoadTier::Medium,
            LoadTier::Medium => LoadTier::Heavy,
            LoadTier::Heavy => LoadTier::Overloaded,
            LoadTier::Overloaded => LoadTier::Overloaded,
        }
    }
}

/// Quantize a mass into a load tier.
pub fn load_tier_for_mass(mass: f32) -> LoadTier {
    if mass < TIER_THRESHOLDS[0] {
        LoadTier::Empty
    } else if mass < TIER_THRESHOLDS[1] {
        LoadTier::Light
    } else if mass < TIER_THRESHOLDS[2] {
        LoadTier::Medium
    } else if mass < TIER_THRESHOLDS[3] {
        LoadTier::Heavy
    } else {
        LoadTier::Overloaded
    }
}

/// The fixed draw rate a tier demands, in `power_at_pos` units.
pub fn load_tier_rate(tier: LoadTier) -> f32 {
    TIER_DRAW_RATE[tier as usize]
}

/// The speed step a tier runs at, as a fraction of nominal speed.
pub fn tier_speed_step(tier: LoadTier) -> f32 {
    TIER_SPEED_STEP[tier as usize]
}

/// The speed a structure or belt may roll: nominal × its tier's step when
/// the draw is met, else the next-heavier (slower) step, with an
/// Overloaded load always stalling.
pub fn effective_speed(base: LoadTier, delivered: f32, draw: f32, nominal: f32) -> f32 {
    if base == LoadTier::Overloaded {
        return 0.0;
    }
    let tier = if delivered >= draw {
        base
    } else {
        base.heavier()
    };
    nominal * tier_speed_step(tier)
}

/// Incline multiplier for a rail segment's draw. Flat/curve/switch pieces
/// cost the base draw; climbing a ramp (the North-only incline piece) costs
/// half again as much. Descending is not cheaper — the base draw already
/// counts the line's friction, and we deliberately do not hand out free
/// energy for rolling downhill.
pub fn incline_multiplier(kind: Option<super::rail::RailKind>, climbing: bool) -> f32 {
    if matches!(kind, Some(super::rail::RailKind::Incline)) && climbing {
        1.5
    } else {
        1.0
    }
}

/// Per-unit mass of one material; unknown material names use a middle
/// default so mod content is never free.
pub fn material_unit_mass(material: &str) -> f32 {
    MATERIAL_UNIT_MASS
        .iter()
        .find(|(name, _)| *name == material)
        .map_or(0.001, |(_, mass)| *mass)
}

fn class_mass(class: MaterialClass) -> f32 {
    CLASS_MASS
        .iter()
        .find(|(c, _)| *c == class)
        .map_or(0.5, |(_, mass)| *mass)
}

/// Mass of one block: its declared materials against the unit table, or a
/// per-class fallback when it declares none.
pub fn block_mass(reg: &Registry, block: BlockId) -> f32 {
    let def = reg.block(block);
    if def.materials.is_empty() {
        class_mass(def.material_class)
    } else {
        def.materials
            .iter()
            .map(|(material, units)| material_unit_mass(material) * *units as f32)
            .sum()
    }
}

/// Mass of one item: declared materials per unit, or a per-class fallback.
pub fn item_mass(reg: &Registry, item: crate::registry::ItemId) -> f32 {
    let def = reg.item(item);
    if def.materials.is_empty() {
        class_mass(def.material_class)
    } else {
        def.materials
            .iter()
            .map(|(material, units)| material_unit_mass(material) * *units as f32)
            .sum()
    }
}

/// Mass of an item stack (belt cargo): per-item mass × stack size.
pub fn item_stack_mass(reg: &Registry, stack: ItemStack) -> f32 {
    item_mass(reg, stack.item) * stack.count as f32
}
