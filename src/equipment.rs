//! Modular equipment (belt-quest capability E6): frames with typed slots
//! and components that slot in/out intact, derived loadout stats, and
//! repairable durability. Frames declare their slot layout on the item
//! graph (`ItemDef.frame`); components declare the slot type they fill
//! (`ItemDef.component`). This module holds the pure loadout mechanics and
//! preset model; the game wires it to the E4 stat surface and the world.

use crate::inventory::ItemStack;
use crate::stats::StatBlock;

/// One typed slot a frame offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameSlotDef {
    pub slot_type: String,
    /// How many components of this type may be slotted at once.
    pub max: u8,
}

/// The slot layout an equipment frame declares in `items.toml`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct FrameDef {
    pub slots: Vec<FrameSlotDef>,
}

impl FrameDef {
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// How many components of `slot_type` the frame accepts (0 = none).
    pub fn max_for(&self, slot_type: &str) -> usize {
        self.slots
            .iter()
            .find(|slot| slot.slot_type == slot_type)
            .map(|slot| slot.max as usize)
            .unwrap_or(0)
    }

    /// Validate a frame declaration. Each returned string is a player-facing
    /// error: empty or duplicate slot types and zero-capacity slots.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for slot in &self.slots {
            if slot.slot_type.is_empty() {
                errors.push("frame slot type must not be empty".into());
            } else if !seen.insert(slot.slot_type.clone()) {
                errors.push(format!("frame declares slot type {} twice", slot.slot_type));
            }
            if slot.max == 0 {
                errors.push(format!("frame slot {} has max 0", slot.slot_type));
            }
        }
        errors
    }
}

/// A component item slotted into a worn frame. The stack is the component's
/// own physical instance, so unslotting returns it intact (durability and
/// any stable instance state ride along).
#[derive(Clone, Debug)]
pub struct SlottedComponent {
    pub slot_type: String,
    pub stack: ItemStack,
}

/// The per-worn-slot loadout: every component currently slotted into a
/// frame, keyed by the frame's declared slot types.
#[derive(Clone, Debug, Default)]
pub struct Loadout {
    pub components: Vec<SlottedComponent>,
}

impl Loadout {
    pub fn clear(&mut self) {
        self.components.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }

    /// How many components of `slot_type` are currently slotted.
    pub fn used_for(&self, slot_type: &str) -> usize {
        self.components
            .iter()
            .filter(|component| component.slot_type == slot_type)
            .count()
    }

    /// Slot a component stack into `frame`. The caller resolves `slot_type`
    /// from the component item's declaration; this method only enforces the
    /// frame's capacity and returns a player-facing error otherwise.
    pub fn slot(
        &mut self,
        frame: &FrameDef,
        slot_type: &str,
        stack: ItemStack,
    ) -> Result<(), String> {
        let max = frame.max_for(slot_type);
        if max == 0 {
            return Err(format!("this frame has no {slot_type} component slot"));
        }
        let used = self.used_for(slot_type);
        if used >= max {
            return Err(format!(
                "this frame holds at most {max} {slot_type} components"
            ));
        }
        self.components.push(SlottedComponent {
            slot_type: slot_type.into(),
            stack,
        });
        Ok(())
    }

    /// Remove the component at `index`, returning it intact.
    pub fn unslot(&mut self, index: usize) -> Option<ItemStack> {
        if index < self.components.len() {
            Some(self.components.remove(index).stack)
        } else {
            None
        }
    }

    /// Aggregated stat modifiers from every slotted component. Frames add
    /// their own base stats separately; components only ever contribute
    /// while slotted.
    pub fn stats(&self, reg: &crate::registry::Registry) -> StatBlock {
        let mut block = StatBlock::default();
        for component in &self.components {
            block.add_all(reg.item(component.stack.item).stats.iter().copied());
        }
        block
    }
}

/// One slot of a saved loadout preset: the frame item id plus the component
/// item ids that were slotted into it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresetSlot {
    pub frame: String,
    pub components: Vec<String>,
}

/// A named, saved equipment configuration. `slots[0..4]` mirror the four
/// armor slots (head/chest/legs/feet); the charm slot is not part of a
/// loadout.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoadoutPreset {
    pub name: String,
    pub slots: [Option<PresetSlot>; 4],
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(slots: &[(&str, u8)]) -> FrameDef {
        FrameDef {
            slots: slots
                .iter()
                .map(|(slot_type, max)| FrameSlotDef {
                    slot_type: (*slot_type).into(),
                    max: *max,
                })
                .collect(),
        }
    }

    fn stack(item: u16) -> ItemStack {
        ItemStack {
            item: crate::registry::ItemId(item),
            count: 1,
            durability: 0,
            arcane_id: 0,
        }
    }

    #[test]
    fn frame_validation_catches_bad_slots() {
        let good = frame(&[("gems", 2), ("stabilizer", 1)]);
        assert!(good.validate().is_empty());

        let empty_type = FrameDef {
            slots: vec![FrameSlotDef {
                slot_type: String::new(),
                max: 1,
            }],
        };
        assert!(empty_type.validate().iter().any(|e| e.contains("empty")));

        let zero_max = frame(&[("gems", 0)]);
        assert!(zero_max.validate().iter().any(|e| e.contains("max 0")));

        let duplicate = frame(&[("gems", 1), ("gems", 2)]);
        assert!(duplicate.validate().iter().any(|e| e.contains("twice")));
    }

    #[test]
    fn slot_respects_frame_capacity_and_types() {
        let frame = frame(&[("gems", 2)]);
        let mut loadout = Loadout::default();

        assert!(loadout.slot(&frame, "gems", stack(1)).is_ok());
        assert!(loadout.slot(&frame, "gems", stack(2)).is_ok());
        // Third of the same type exceeds the frame's capacity.
        let err = loadout
            .slot(&frame, "gems", stack(3))
            .expect_err("over capacity");
        assert!(err.contains("at most 2 gems"), "{err}");

        // An undeclared slot type is refused outright.
        let err = loadout
            .slot(&frame, "stabilizer", stack(4))
            .expect_err("no slot");
        assert!(err.contains("no stabilizer"), "{err}");
    }

    #[test]
    fn unslot_returns_component_intact() {
        let frame = frame(&[("gems", 2)]);
        let mut loadout = Loadout::default();
        let mut original = stack(7);
        original.durability = 3;
        original.arcane_id = 42;
        loadout.slot(&frame, "gems", original).unwrap();
        loadout.slot(&frame, "gems", stack(8)).unwrap();

        let back = loadout.unslot(0).expect("slotted component");
        assert_eq!(back, original, "unslotted component comes back intact");
        assert_eq!(
            loadout.components.len(),
            1,
            "only the other component remains"
        );
        assert!(loadout.unslot(5).is_none());
    }

    #[test]
    fn empty_frame_accepts_nothing() {
        let mut loadout = Loadout::default();
        let err = loadout
            .slot(&FrameDef::default(), "gems", stack(1))
            .expect_err("no slots");
        assert!(err.contains("no gems"));
    }

    #[test]
    fn registry_loads_frames_and_components_from_a_mod_dir() {
        let dir = std::env::temp_dir().join(format!("wildforge-equipment-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mod_dir = dir.join("gear");
        std::fs::create_dir_all(&mod_dir).unwrap();
        std::fs::write(
            mod_dir.join("mod.toml"),
            "id = \"gear\"\nname = \"Gear\"\nversion = \"1.0.0\"\nworld_api = 2\ndepends = [\"base\"]\n",
        )
        .unwrap();
        std::fs::write(
            mod_dir.join("items.toml"),
            r#"
[[item]]
id = "shell_chest"
name = "Shell Chest"
texture = "@leather_chestplate"
durability = 50
armor = { slot = "chest", points = 2 }
frame = { slots = [ { type = "gems", max = 2 } ] }

[[item]]
id = "cut_ruby"
name = "Cut Ruby"
texture = "@ruby"
component = "gems"
[[item.stats]]
kind = "health"
flat = 2

[[item]]
id = "bad_frame"
name = "Bad Frame"
texture = "@leather_helmet"
frame = { slots = [ { type = "gems", max = 0 } ] }
"#,
        )
        .unwrap();
        let reg = crate::registry::load(&dir);
        let frame_id = reg.item_id("gear:shell_chest").expect("frame item");
        let component_id = reg.item_id("gear:cut_ruby").expect("component item");
        let frame_def = reg.item(frame_id).frame.clone().expect("frame def");
        assert_eq!(frame_def.max_for("gems"), 2);
        assert_eq!(reg.item(component_id).component.as_deref(), Some("gems"));
        assert_eq!(reg.item(frame_id).max_stack, 1, "frames are one_only");
        assert_eq!(
            reg.item(component_id).max_stack,
            1,
            "components are one_only"
        );
        assert!(
            reg.mods
                .iter()
                .filter_map(|m| m.error.as_deref())
                .any(|e| e.contains("max 0")),
            "bad frame reported: {:?}",
            reg.mods.iter().map(|m| &m.error).collect::<Vec<_>>()
        );

        let mut loadout = Loadout::default();
        loadout
            .slot(&frame_def, "gems", ItemStack::new(&reg, component_id, 1))
            .expect("slot");
        let stats = loadout.stats(&reg);
        assert_eq!(
            stats.effective(crate::stats::StatKind::Health, 14.0),
            16.0,
            "slotted component grants its declared stat"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
