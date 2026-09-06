//! Lenses discovery transaction coordination.

use crate::discovery::CalibrationGrade;
use crate::inventory::ItemStack;
use crate::planet::BlockPos;
use crate::world::World;

impl World {
    pub fn discovery_artifact_text(
        &mut self,
        stack: &mut ItemStack,
        at: BlockPos,
    ) -> Option<String> {
        if let Err(error) = self.bind_discovery_stack_at(at, stack) {
            eprintln!("discovery: could not bind artifact: {error}");
            return None;
        }
        self.discovery_state
            .as_ref()
            .and_then(|state| state.artifact_text(stack.arcane_id))
            .map(ToOwned::to_owned)
    }

    pub fn calibration_grade_for(&self, stack: ItemStack) -> Option<CalibrationGrade> {
        self.discovery_state
            .as_ref()
            .and_then(|state| state.calibration_of(stack.arcane_id))
            .or_else(|| self.reg.item(stack.item).discovery.as_ref()?.calibration)
    }

    /// Fit an industrial frame with one Echo Slate plate and one replaceable
    /// Wellglass element. The Wellglass item identity becomes the lens
    /// identity; the slate's Current returns locally while its physical
    /// mineral remains in the composite material vector.
    pub fn assemble_tuning_lens_at(
        &mut self,
        pos: BlockPos,
        inventory: &mut crate::inventory::Inventory,
    ) -> Result<ItemStack, String> {
        let frame = self
            .reg
            .item_id("base:tuning_lens_frame")
            .ok_or("content has no tuning lens frame")?;
        let mount = self
            .reg
            .item_id("base:tuning_lens_mount")
            .ok_or("content has no fitted tuning lens mount")?;
        let slate = self
            .reg
            .item_id("base:echo_slate")
            .ok_or("content has no Echo Slate plate")?;
        let wellglass = self
            .reg
            .item_id("base:wellglass_shard")
            .ok_or("content has no Wellglass element")?;
        let lens = self
            .reg
            .item_id("base:tuning_lens")
            .ok_or("content has no tuning lens")?;
        let find = |item| {
            inventory
                .slots
                .iter()
                .position(|slot| slot.is_some_and(|stack| stack.item == item))
        };
        let mount_slot = find(mount);
        let frame_slot = mount_slot
            .or_else(|| find(frame))
            .ok_or("Bring a fitted mount, or an unfitted frame with one Echo Slate plate.")?;
        let slate_slot = if mount_slot.is_none() {
            Some(find(slate).ok_or("Bring one Echo Slate plate for the unfitted frame.")?)
        } else {
            None
        };
        let wellglass_slot = find(wellglass).ok_or("Bring one charged Wellglass shard.")?;
        let mut wellglass_stack = inventory.slots[wellglass_slot].unwrap();
        let mut slate_stack = slate_slot.map(|slot| inventory.slots[slot].unwrap());
        if let Some(stack) = &mut slate_stack {
            self.bind_arcane_stack_at(pos, stack, "lens assembly slate")
                .map_err(|error| error.to_string())?;
        }
        self.bind_arcane_stack_at(pos, &mut wellglass_stack, "lens assembly element")
            .map_err(|error| error.to_string())?;
        if let (Some(slot), Some(stack)) = (slate_slot, slate_stack) {
            inventory.slots[slot] = Some(stack);
        }
        inventory.slots[wellglass_slot] = Some(wellglass_stack);
        let slate_stack = slate_slot
            .map(|slot| {
                inventory
                    .take_one_stack(slot)
                    .ok_or("Echo Slate moved during assembly.")
            })
            .transpose()?;
        let wellglass_stack = inventory
            .take_one_stack(wellglass_slot)
            .ok_or("Wellglass moved during assembly.")?;
        let frame_stack = inventory
            .take_one_stack(frame_slot)
            .ok_or("The lens mount moved during assembly.")?;
        let output = ItemStack {
            item: lens,
            count: 1,
            durability: self.reg.item(lens).durability,
            arcane_id: wellglass_stack.arcane_id,
        };
        if inventory.slots[frame_slot].is_some() {
            // A counted frame stack is forbidden by content, but preserve the
            // operation atomically if a mod or old save violates that rule.
            let left = inventory.add_stack(&self.reg, output);
            if left != 0 {
                if let Some(slate_stack) = slate_stack {
                    inventory.add_stack(&self.reg, slate_stack);
                }
                inventory.add_stack(&self.reg, frame_stack);
                inventory.add_stack(&self.reg, wellglass_stack);
                return Err("No room for the completed lens.".into());
            }
        } else {
            inventory.slots[frame_slot] = Some(output);
        }
        if let Some(slate_stack) = slate_stack {
            self.retire_arcane_stack_at(pos, slate_stack, "Echo Slate fitted into tuning lens");
        }
        Ok(output)
    }

    pub fn wear_tuning_lens_at(
        &mut self,
        pos: BlockPos,
        inventory: &mut crate::inventory::Inventory,
        slot: usize,
    ) -> bool {
        let Some(lens) = self.reg.item_id("base:tuning_lens") else {
            return false;
        };
        let Some(mut stack) = inventory.slots.get(slot).copied().flatten() else {
            return false;
        };
        if stack.item != lens {
            return false;
        }
        stack.durability = stack.durability.saturating_sub(1);
        if stack.durability != 0 {
            inventory.slots[slot] = Some(stack);
            return false;
        }
        self.retire_arcane_stack_at(pos, stack, "spent Wellglass lens element returned locally");
        inventory.slots[slot] = self
            .reg
            .item_id("base:tuning_lens_mount")
            .map(|mount| ItemStack::new(&self.reg, mount, 1));
        true
    }
}
