//! Player stat modifier surface (belt-quest capability E4).
//!
//! Derived stats are computed as `(base + flat) * permille / 1000`. A
//! modifier block aggregates flat bonuses additively and multiplier
//! permilles multiplicatively across every source the player carries:
//! worn equipment (`ItemDef.stats`) and active preparations
//! (`PreparationModifiers`). Skills (E5) and equipment loadouts (E6) plug
//! into the same `StatBlock` later.

use serde::{Deserialize, Serialize};

/// The derived stats the player surface can modify. Order is the array
/// index convention used by `StatBlock`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StatKind {
    /// Maximum health in half-hearts (hearts in the HUD).
    Health,
    /// Maximum stamina pool for combat exertion.
    Stamina,
    /// Stamina recovered per second while resting.
    StaminaRegen,
    /// Carry capacity in weight units (one default item = one unit).
    Carry,
    /// Block placement/breaking reach in blocks.
    BuildRange,
    /// Radius in blocks within which arcane ecology is observed.
    ScanRange,
    /// Ground/water movement speed multiplier (1.0 = base).
    MoveSpeed,
}

impl StatKind {
    pub const COUNT: usize = 7;

    pub const ALL: [StatKind; StatKind::COUNT] = [
        StatKind::Health,
        StatKind::Stamina,
        StatKind::StaminaRegen,
        StatKind::Carry,
        StatKind::BuildRange,
        StatKind::ScanRange,
        StatKind::MoveSpeed,
    ];

    pub fn index(self) -> usize {
        match self {
            StatKind::Health => 0,
            StatKind::Stamina => 1,
            StatKind::StaminaRegen => 2,
            StatKind::Carry => 3,
            StatKind::BuildRange => 4,
            StatKind::ScanRange => 5,
            StatKind::MoveSpeed => 6,
        }
    }
}

impl std::str::FromStr for StatKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "health" => Ok(StatKind::Health),
            "stamina" => Ok(StatKind::Stamina),
            "stamina_regen" => Ok(StatKind::StaminaRegen),
            "carry" => Ok(StatKind::Carry),
            "build_range" => Ok(StatKind::BuildRange),
            "scan_range" => Ok(StatKind::ScanRange),
            "move_speed" => Ok(StatKind::MoveSpeed),
            other => Err(format!("unknown stat kind {other:?}")),
        }
    }
}

/// One contribution to a stat: a flat bonus and/or a percentage multiplier
/// (1000 permille = no change). Declared in item content (`[[item.stats]]`).
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
pub struct StatModifier {
    pub kind: StatKind,
    #[serde(default)]
    pub flat: f32,
    #[serde(default = "default_permille")]
    pub mult_permille: u16,
}

fn default_permille() -> u16 {
    1_000
}

/// Chain two multiplier permilles: `(a * b) / 1000`, clamped to u16. The
/// intermediate product uses u64 so a pair of large permilles (or a long
/// chain) cannot overflow before the /1000 normalizes back to permille
/// space.
fn combine_permille(a: u16, b: u16) -> u16 {
    (u64::from(a) * u64::from(b) / 1_000).min(u64::from(u16::MAX)) as u16
}

impl StatModifier {
    pub const fn new(kind: StatKind, flat: f32, mult_permille: u16) -> StatModifier {
        StatModifier {
            kind,
            flat,
            mult_permille,
        }
    }
}

/// Aggregated per-kind flat sum and permille product. Effective value is
/// `(base + flat) * mult / 1000`, with a floor so negative equipment cannot
/// invert a derived stat below its resting floor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StatBlock {
    flat: [f32; StatKind::COUNT],
    mult: [u16; StatKind::COUNT],
}

impl Default for StatBlock {
    fn default() -> Self {
        StatBlock {
            flat: [0.0; StatKind::COUNT],
            mult: [1_000; StatKind::COUNT],
        }
    }
}

impl StatBlock {
    pub fn add(&mut self, m: StatModifier) {
        self.flat[m.kind.index()] += m.flat;
        self.mult[m.kind.index()] = combine_permille(self.mult[m.kind.index()], m.mult_permille);
    }

    pub fn add_all(&mut self, modifiers: impl IntoIterator<Item = StatModifier>) {
        for m in modifiers {
            self.add(m);
        }
    }

    pub fn merge(&mut self, other: &StatBlock) {
        for kind in StatKind::ALL {
            let i = kind.index();
            self.flat[i] += other.flat[i];
            self.mult[i] = combine_permille(self.mult[i], other.mult[i]);
        }
    }

    /// `(base + flat) * mult / 1000`, floored at zero.
    pub fn effective(&self, kind: StatKind, base: f32) -> f32 {
        let i = kind.index();
        ((base + self.flat[i]) * f32::from(self.mult[i]) / 1_000.0).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_block_is_identity() {
        let block = StatBlock::default();
        assert_eq!(block.effective(StatKind::Health, 14.0), 14.0);
        assert_eq!(block.effective(StatKind::MoveSpeed, 1.0), 1.0);
    }

    #[test]
    fn flat_and_permille_combine() {
        let mut block = StatBlock::default();
        block.add(StatModifier::new(StatKind::Health, 4.0, 1_000));
        block.add(StatModifier::new(StatKind::Health, 0.0, 800));
        assert_eq!(block.effective(StatKind::Health, 14.0), 14.4);
    }

    #[test]
    fn permille_multiplies_across_sources() {
        let mut block = StatBlock::default();
        block.add(StatModifier::new(StatKind::StaminaRegen, 0.0, 1_250));
        block.add(StatModifier::new(StatKind::StaminaRegen, 0.0, 800));
        assert_eq!(block.effective(StatKind::StaminaRegen, 3.5), 3.5);
    }

    #[test]
    fn merge_accumulates_both_channels() {
        let mut a = StatBlock::default();
        a.add(StatModifier::new(StatKind::Carry, 100.0, 1_000));
        let mut b = StatBlock::default();
        b.add(StatModifier::new(StatKind::Carry, 50.0, 1_500));
        a.merge(&b);
        assert_eq!(a.effective(StatKind::Carry, 512.0), 993.0);
    }

    #[test]
    fn stat_kind_parses_content_names() {
        for (text, kind) in [
            ("health", StatKind::Health),
            ("stamina", StatKind::Stamina),
            ("stamina_regen", StatKind::StaminaRegen),
            ("carry", StatKind::Carry),
            ("build_range", StatKind::BuildRange),
            ("scan_range", StatKind::ScanRange),
            ("move_speed", StatKind::MoveSpeed),
        ] {
            assert_eq!(text.parse::<StatKind>(), Ok(kind));
        }
        assert!("poke".parse::<StatKind>().is_err());
    }
}
