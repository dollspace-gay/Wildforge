//! Derived player stats (belt-quest capability E4): the modifier surface
//! plumbing on top of `crate::stats`. Equipment and active preparations
//! feed one `StatBlock`; derived values route health, stamina, carry,
//! build reach, scan radius, and move speed.

use super::*;
use crate::stats::{StatBlock, StatKind, StatModifier};

/// Default carry capacity in weight units (one default item weighs one).
pub(crate) const BASE_CARRY_UNITS: f32 = 512.0;
/// Default arcane ecology observation radius in blocks.
pub(crate) const BASE_SCAN_UNITS: f32 = 72.0;

impl Game {
    /// Aggregated stat modifiers from worn equipment.
    pub(super) fn equipment_stats(&self) -> StatBlock {
        let reg = &self.content.reg;
        let mut block = StatBlock::default();
        for slot in self.survival.armor.iter().flatten() {
            block.add_all(reg.item(slot.item).stats.iter().copied());
        }
        block
    }

    /// Aggregated stat modifiers from active preparations.
    pub(super) fn preparation_stats(&self) -> StatBlock {
        let p = self.survival.preparation_modifiers;
        let mut block = StatBlock::default();
        block.add(StatModifier::new(StatKind::Health, 0.0, p.health_permille));
        block.add(StatModifier::new(StatKind::Stamina, 0.0, p.stamina_permille));
        block.add(StatModifier::new(
            StatKind::StaminaRegen,
            0.0,
            p.stamina_regen_permille,
        ));
        block.add(StatModifier::new(StatKind::Carry, 0.0, p.carry_permille));
        block.add(StatModifier::new(StatKind::BuildRange, 0.0, p.reach_permille));
        block.add(StatModifier::new(StatKind::ScanRange, 0.0, p.scan_permille));
        block.add(StatModifier::new(
            StatKind::MoveSpeed,
            0.0,
            p.move_speed_permille,
        ));
        block
    }

    /// The player's complete derived-stat surface.
    pub(super) fn stat_block(&self) -> StatBlock {
        let mut block = self.equipment_stats();
        block.merge(&self.preparation_stats());
        block.merge(&self.skills_stats());
        block
    }

    pub(super) fn max_health(&self) -> f32 {
        let base = MAX_HEALTH
            + self
                .survival
                .nutrition
                .iter()
                .filter(|&&n| n >= 40.0)
                .count() as f32
                * 2.0;
        self.stat_block().effective(StatKind::Health, base)
    }

    pub(super) fn stamina_max(&self) -> f32 {
        self.stat_block()
            .effective(StatKind::Stamina, combat::STAMINA_MAX)
    }

    pub(super) fn stamina_regen(&self) -> f32 {
        self.stat_block()
            .effective(StatKind::StaminaRegen, combat::STAMINA_REGEN)
    }

    pub(super) fn carry_capacity(&self) -> f32 {
        self.stat_block()
            .effective(StatKind::Carry, BASE_CARRY_UNITS)
    }

    pub(super) fn reach(&self) -> f32 {
        self.stat_block()
            .effective(StatKind::BuildRange, REACH)
    }

    pub(super) fn scan_range(&self) -> f32 {
        self.stat_block()
            .effective(StatKind::ScanRange, BASE_SCAN_UNITS)
    }

    pub(super) fn move_speed(&self) -> f32 {
        self.stat_block()
            .effective(StatKind::MoveSpeed, 1.0)
    }

    /// Current carried weight in units (sum of stack counts weighted by
    /// each item's carry weight).
    pub(super) fn carried_weight(&self) -> f32 {
        self.inventory.total_weight(&self.content.reg) as f32
    }
}
