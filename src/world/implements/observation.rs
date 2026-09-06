//! Observation implements transaction coordination.

use crate::world::BlockEntity;
use crate::world::ItemStack;
use crate::world::World;

impl World {
    /// Bounded qualitative apparatus state for local presentation or nearby
    /// guest interest management. Exact amounts remain in item custody and
    /// tuning-lens responses.
    pub fn apparatus_cues_near(
        &self,
        observer: crate::planet::EntityPos,
        radius: f32,
    ) -> Vec<crate::implements::ApparatusCue> {
        let radius = radius.clamp(1.0, 96.0);
        let Some(state) = self.implements_state.as_ref() else {
            return Vec::new();
        };
        let Some(ledger) = self.arcane_ledger.as_ref() else {
            return Vec::new();
        };
        self.installations
            .iter()
            .filter_map(|(&pos, entity)| {
                if observer.distance_to(pos.entity_center()) > radius {
                    return None;
                }
                let BlockEntity::ChargeVessel(vessel) = entity else {
                    return None;
                };
                let stack = vessel.vessel?;
                let instance = state.instance(stack.arcane_id)?;
                let total = ledger.item_clean_total(stack.arcane_id).unwrap_or(0);
                let strain_band = match vessel.damage {
                    0..=199 => 0,
                    200..=499 => 1,
                    500..=799 => 2,
                    _ => 3,
                };
                Some(crate::implements::ApparatusCue {
                    pos,
                    charge_band: crate::implements::charge_band(total, instance.usable_capacity()),
                    strain_band,
                })
            })
            .take(128)
            .collect()
    }

    pub fn implement_visual(&self, stack: ItemStack) -> Option<crate::implements::ImplementVisual> {
        let kind = self
            .implements_state
            .as_ref()
            .and_then(|state| state.instance(stack.arcane_id))
            .map(|instance| &instance.kind)?;
        crate::world::item_presentation::implement_visual(
            &self.reg, kind, self.inspectable_item_current(stack.arcane_id),
        )
    }
}
