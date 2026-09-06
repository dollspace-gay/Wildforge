//! Loose-item custody coordination, motion, and loss settlement.

use crate::inventory::ItemStack;
use crate::world::World;

impl World {
    pub fn loose_items(&self) -> &[crate::entity::ItemEntity] {
        self.population.loose_items()
    }

    pub fn spawn_loose_item(&mut self, item: crate::entity::ItemEntity) -> u64 {
        self.population.spawn_loose_item(item)
    }

    pub fn replace_loose_items(&mut self, items: Vec<crate::entity::ItemEntity>) {
        self.population.replace_loose_items(items)
    }

    pub fn take_loose_items(&mut self) -> Vec<crate::entity::ItemEntity> {
        self.population.take_loose_items()
    }

    pub fn clear_loose_items(&mut self) {
        self.population.clear_loose_items()
    }

    pub fn for_each_loose_item_mut(&mut self, update: impl FnMut(&mut crate::entity::ItemEntity)) {
        self.population.for_each_loose_item_mut(update)
    }

    /// Advance ordinary dropped-item physics and settle material/arcane loss
    /// at the same host authority that owns Nudge and pickup.
    pub(in crate::world) fn tick_loose_items(&mut self, dt: f32) {
        let pending = std::mem::take(&mut self.pending_drops);
        for (pos, stack) in pending {
            let id = self.population.next_loose_item_id();
            let angle = ((id ^ (id >> 31)) as u32) as f32 / u32::MAX as f32 * std::f32::consts::TAU;
            let mut item = crate::entity::ItemEntity::new(
                pos.entity_center(),
                glam::Vec3::new(angle.cos() * 1.5, 2.5, angle.sin() * 1.5),
                stack.item,
                stack.count,
            );
            item.durability = stack.durability;
            item.arcane_id = stack.arcane_id;
            self.spawn_loose_item(item);
        }

        let mut kept = Vec::with_capacity(self.population.loose_items().len());
        let mut lost = Vec::new();
        for mut item in self.population.take_loose_items() {
            if item.update(self, dt) {
                kept.push(item);
            } else {
                lost.push(item);
            }
        }
        self.population.restore_loose_items(kept);
        let reg = self.reg.clone();
        for item in lost {
            let reason = item.loss_reason(self);
            let Some(pos) = item.pos.block() else {
                continue;
            };
            let mut stack = ItemStack::new(&reg, item.item, item.count);
            stack.durability = item.durability;
            stack.arcane_id = item.arcane_id;
            let implement_materials_handled = self.retire_arcane_stack_at(pos, stack, reason);
            if !implement_materials_handled
                && let Some(ledger) = &mut self.material_ledger
                && let Err(error) = ledger.bury_stack(&reg, pos, stack, reason)
            {
                eprintln!("materials: dropped-item salvage failed: {error}");
            }
        }
    }
}
