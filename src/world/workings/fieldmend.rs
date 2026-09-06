//! Fieldmend workings transaction coordination.

use crate::planet::BlockPos;
use crate::workings::PhysicalDebit;
use crate::workings::PhysicalDebitKind;
use crate::workings::WorkingEffect;
use crate::workings::WorkingResult;
use crate::workings::WorkingTargetSnapshot;
use crate::world::World;
use super::inventory_target_id;

impl World {
    /// Reserve a wasteful portable repair against two exact inventory slots.
    /// The actual stack edit is a write-ahead external adapter completed by
    /// the authoritative local/host profile owner.
    #[allow(clippy::too_many_arguments)]
    #[cfg(test)]
    pub fn begin_fieldmend_working(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        inventory: &crate::inventory::Inventory,
        target_slot: usize,
        material_slot: usize,
        magnitude: u32,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        self.begin_fieldmend_working_definition(
            actor,
            actor_label,
            source,
            wand_id,
            "base:fieldmend",
            inventory,
            target_slot,
            material_slot,
            magnitude,
            forced,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn begin_fieldmend_working_definition(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        working_id: &str,
        inventory: &crate::inventory::Inventory,
        target_slot: usize,
        material_slot: usize,
        magnitude: u32,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        if target_slot >= crate::inventory::TOTAL_SLOTS
            || material_slot >= crate::inventory::TOTAL_SLOTS
            || target_slot == material_slot
        {
            return Err("Fieldmend needs two distinct authoritative inventory slots.".into());
        }
        let target_stack =
            inventory.slots[target_slot].ok_or("Fieldmend's target slot is empty.")?;
        let material_stack =
            inventory.slots[material_slot].ok_or("Fieldmend's matching-stock slot is empty.")?;
        let target = self.reg.item(target_stack.item);
        if target_stack.count != 1
            || target_stack.arcane_id != 0
            || target.food.is_some()
            || target.durability == 0
            || target_stack.durability == 0
            || target_stack.durability >= target.durability
        {
            return Err("Fieldmend supports one damaged, still-serviceable portable item.".into());
        }
        let repair_name = format!("{}/forge_scrap", target.name);
        let residue_name = format!("{}/primitive_scale", target.name);
        let repair_item = self
            .reg
            .item_id(&repair_name)
            .ok_or("This portable item has no recipe-declared matching repair stock.")?;
        let residue_item = self
            .reg
            .item_id(&residue_name)
            .ok_or("This repair family has no ordinary residue definition.")?;
        if material_stack.item != repair_item
            || material_stack.count != 1
            || material_stack.arcane_id != 0
        {
            return Err(format!(
                "Fieldmend needs one separated stack of {}.",
                self.reg.item(repair_item).label
            ));
        }
        let restored = magnitude
            .clamp(1, 16)
            .min(target.durability - target_stack.durability);
        let after = target_stack.durability.saturating_add(restored);
        let item_id = inventory_target_id(actor, target_slot, target_stack.item.0);
        let material_id = inventory_target_id(actor, material_slot, material_stack.item.0);
        let material_units = self
            .reg
            .item(repair_item)
            .materials
            .values()
            .try_fold(0u64, |sum, units| sum.checked_add(*units))
            .ok_or("Fieldmend matching material quantity overflowed.")?;
        if material_units == 0 {
            return Err("Fieldmend refuses unaccounted or massless repair stock.".into());
        }
        let targets = vec![
            WorkingTargetSnapshot::Item {
                stable_id: item_id,
                item_name: target.name.clone(),
                durability: target_stack.durability,
                age_ticks: 0,
                version: target_slot as u64,
            },
            WorkingTargetSnapshot::Item {
                stable_id: material_id,
                item_name: repair_name.clone(),
                durability: material_stack.durability,
                age_ticks: 0,
                version: material_slot as u64,
            },
        ];
        let physical = vec![
            PhysicalDebit {
                kind: PhysicalDebitKind::Material,
                source: format!("inventory:{material_slot}"),
                content_id: repair_name.clone(),
                units: material_units,
                expected_version: material_slot as u64,
            },
            PhysicalDebit {
                kind: PhysicalDebitKind::Item,
                source: format!("inventory:{target_slot}"),
                content_id: target.name.clone(),
                units: u64::from(restored),
                expected_version: target_slot as u64,
            },
        ];
        self.reserve_wand_effect(
            actor,
            actor_label,
            source,
            wand_id,
            working_id,
            targets,
            physical,
            WorkingEffect::RepairItem {
                item_id,
                item_name: target.name.clone(),
                before_durability: target_stack.durability,
                after_durability: after,
                repair_material: repair_name,
                material_units,
                residue_item: self.reg.item(residue_item).name.clone(),
                residue_units: 1,
            },
            restored,
            0,
            0,
            forced,
        )
    }
}
