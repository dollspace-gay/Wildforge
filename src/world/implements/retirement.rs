//! Retirement implements transaction coordination.

use crate::arcane::ArcaneOwner;
use std::collections::BTreeMap;
use crate::world::BlockPos;
use crate::arcane::Current;
use crate::arcane::DrossMedium;
use crate::implements::ImplementAuditEvent;
use crate::implements::ImplementKind;
use crate::world::ItemStack;
use crate::arcane::LinkedFileReplacement;
use crate::world::World;
use super::add_current;
use super::transaction_from_maps;

impl World {
    /// Remove a destroyed/despawned implement and its sidecar identity in the
    /// same durable commit that settles every clean and retained-dross unit.
    /// This is deliberately separate from safe frame disassembly: an unsafe
    /// path yields fragments only where a physical catastrophe leaves them.
    pub(in crate::world) fn retire_implement_at(
        &mut self,
        pos: BlockPos,
        stack: ItemStack,
        reason: &str,
    ) -> Result<(), String> {
        let heat_fracture = reason.contains("lava") || reason.contains("burned in fire");
        if heat_fracture
            && self
                .implements_state
                .as_ref()
                .and_then(|state| state.instance(stack.arcane_id))
                .is_some_and(|instance| {
                    matches!(
                        instance.kind,
                        ImplementKind::Wand { .. } | ImplementKind::Vessel { .. }
                    )
                })
        {
            let fragments = self.fracture_implement_at(pos, stack, reason)?;
            self.push_drop_at(pos, fragments);
            return Ok(());
        }
        let mut next_state = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let instance = next_state
            .instances
            .remove(&stack.arcane_id)
            .ok_or("The destroyed implement has no construction record.")?;
        let tracked_materials = instance
            .tracked_materials()
            .map_err(|error| error.to_string())?;
        let material_consumed = [
            "consumed",
            "transformed",
            "fitted",
            "composted",
            "ignition",
            "placed into the environment",
        ]
        .iter()
        .any(|needle| reason.contains(needle));
        let staged_material = if tracked_materials.is_empty() {
            None
        } else {
            let material_ledger = self
                .material_ledger
                .as_ref()
                .ok_or("The world has no finite material ledger.")?;
            if material_consumed {
                material_ledger.stage_linked_recipe_loss(&tracked_materials)
            } else {
                material_ledger.stage_linked_bury_materials(pos, &tracked_materials, reason)
            }
            .map_err(|error| error.to_string())?
        };
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        let disposition = self
            .reg
            .items
            .get(stack.item.0 as usize)
            .and_then(|definition| definition.arcane.as_ref())
            .map_or(crate::registry::ArcaneDisposition::Dross, |arcane| {
                arcane.on_destroy
            });
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no Current ledger.")?;
        let clean_owner = ArcaneOwner::Item(stack.arcane_id);
        let dross_owner = ArcaneOwner::ItemDross(stack.arcane_id);
        let clean = ledger
            .account(&clean_owner)
            .map(|account| account.current.clone())
            .unwrap_or_default();
        let retained = ledger
            .account(&dross_owner)
            .map(|account| account.current.clone())
            .unwrap_or_default();
        if clean.is_empty() && retained.is_empty() {
            return Err("The destroyed implement names no finite Current custody.".into());
        }
        let clean_destination = match disposition {
            crate::registry::ArcaneDisposition::Ambient => ArcaneOwner::Ambient(region),
            crate::registry::ArcaneDisposition::Dross
            | crate::registry::ArcaneDisposition::Scar => ArcaneOwner::Dross {
                region,
                medium: if heat_fracture {
                    DrossMedium::Air
                } else {
                    DrossMedium::Soil
                },
            },
        };
        let dross_destination = ArcaneOwner::Dross {
            region,
            medium: if heat_fracture {
                DrossMedium::Air
            } else {
                DrossMedium::Soil
            },
        };
        let mut debits = BTreeMap::new();
        let mut credits = BTreeMap::new();
        if !clean.is_empty() {
            add_current(&mut debits, clean_owner, &clean).map_err(|error| error.to_string())?;
            add_current(&mut credits, clean_destination, &clean)
                .map_err(|error| error.to_string())?;
        }
        if !retained.is_empty() {
            add_current(&mut debits, dross_owner, &retained).map_err(|error| error.to_string())?;
            add_current(&mut credits, dross_destination, &retained)
                .map_err(|error| error.to_string())?;
        }
        let operation_id = next_state
            .operation_id()
            .map_err(|error| error.to_string())?;
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: "destructive_retirement".into(),
            instance_id: stack.arcane_id,
            units: clean.total(),
            dross: retained.total(),
            actor: "world".into(),
            note: reason.into(),
        });
        let transaction =
            transaction_from_maps(ledger, debits, credits, &instance.content_id, reason)?;
        let replacement = LinkedFileReplacement {
            subsystem: "implements".into(),
            operation_id,
            relative_path: crate::implements::IMPLEMENTS_FILE.into(),
            after: Some(next_state.encode().map_err(|error| error.to_string())?),
        };
        let mut replacements = vec![replacement];
        if let Some((_, bytes)) = &staged_material {
            replacements.push(LinkedFileReplacement {
                subsystem: "materials".into(),
                operation_id,
                relative_path: crate::materials::MaterialLedger::linked_delta_path().into(),
                after: Some(bytes.clone()),
            });
        }
        ledger
            .commit_linked_files(transaction, replacements)
            .map_err(|error| error.to_string())?;
        self.implements_state = Some(next_state);
        if let Some((next_material, _)) = staged_material {
            self.material_ledger = Some(next_material);
        }
        Ok(())
    }
}
