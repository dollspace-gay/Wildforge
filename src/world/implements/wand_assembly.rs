//! Wand assembly implements transaction coordination.

use super::ASSEMBLY_CALIBRATION_UNITS;
use super::add_current;
use super::physical_component;
use super::transaction_from_maps;
use crate::arcane::ArcaneOwner;
use crate::arcane::Current;
use crate::arcane::LinkedFileReplacement;
use crate::implements::FrameResult;
use crate::implements::ImplementAuditEvent;
use crate::implements::ImplementCue;
use crate::implements::ImplementInstance;
use crate::implements::ImplementKind;
use crate::implements::STRUCTURAL_SPARK_UNITS;
use crate::inventory::ItemStack;
use crate::planet::BlockPos;
use crate::world::BlockEntity;
use crate::world::World;
use std::collections::BTreeMap;

impl World {
    pub(super) fn assemble_wand(
        &mut self,
        pos: BlockPos,
        actor: &str,
    ) -> Result<FrameResult, String> {
        let layout = self.binding_frame_layout(pos);
        if !layout.valid {
            return Err(layout.problems.join(" "));
        }
        let (mounts, revision, output_occupied) = match self.installations.get(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => {
                (frame.mounts(), frame.revision, frame.output.is_some())
            }
            _ => return Err("Install the four components in the frame.".into()),
        };
        if output_occupied {
            return Err("Retrieve the object in the finished cradle first.".into());
        }
        let mut declared = Vec::new();
        for stack in mounts {
            let stack = stack.ok_or("Install one body, reservoir, focus, and binding.")?;
            let definition = self.reg.item(stack.item);
            let component = definition
                .wand_component
                .clone()
                .ok_or("A mounted item is not a declared wand component.")?;
            declared.push((definition.name.clone(), component));
        }
        let declared: [(String, crate::implements::WandComponentDef); 4] = declared
            .try_into()
            .map_err(|_| "The frame needs exactly four component roles.".to_string())?;
        let (parts, resolved) =
            crate::implements::resolve_wand(&declared).map_err(|error| error.to_string())?;

        let output_item = self
            .reg
            .item_id("base:bound_wand")
            .ok_or("The bound-wand content definition is missing.")?;
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        self.ensure_regional_ambient_units(
            region,
            ASSEMBLY_CALIBRATION_UNITS.saturating_add(STRUCTURAL_SPARK_UNITS),
            "binding-frame wand calibration reserve",
        )?;
        let ambient_owner = ArcaneOwner::Ambient(region);
        let content_id = self.reg.item(output_item).name.clone();

        let mut next_state = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no Current ledger.")?;
        let instance_id = ledger
            .allocate_item_id()
            .map_err(|error| error.to_string())?;
        let operation_id = next_state
            .operation_id()
            .map_err(|error| error.to_string())?;

        let mut debits = BTreeMap::<ArcaneOwner, Current>::new();
        let mut clean = Current::default();
        let mut carried_dross = Current::default();
        for stack in mounts.into_iter().flatten() {
            if stack.arcane_id == 0 {
                continue;
            }
            for (owner, dross) in [
                (ArcaneOwner::Item(stack.arcane_id), false),
                (ArcaneOwner::ItemDross(stack.arcane_id), true),
            ] {
                if let Some(account) = ledger.account(&owner) {
                    add_current(&mut debits, owner, &account.current)
                        .map_err(|error| error.to_string())?;
                    if dross {
                        carried_dross
                            .checked_add(&account.current)
                            .map_err(|error| error.to_string())?;
                    } else {
                        clean
                            .checked_add(&account.current)
                            .map_err(|error| error.to_string())?;
                    }
                }
            }
        }
        let calibration_needed = ASSEMBLY_CALIBRATION_UNITS
            .saturating_add(STRUCTURAL_SPARK_UNITS)
            .saturating_sub(clean.total());
        if calibration_needed != 0 {
            let account = ledger
                .account(&ambient_owner)
                .ok_or("This region has no measurable Ambient Current.")?;
            let mut available = account.current.clone();
            let selected = available
                .take_units(calibration_needed, resolved.resonance.keys().cloned())
                .map_err(|_| "The regional Ambient reserve cannot calibrate this wand.")?;
            add_current(&mut debits, ambient_owner.clone(), &selected)
                .map_err(|error| error.to_string())?;
            clean
                .checked_add(&selected)
                .map_err(|error| error.to_string())?;
        }
        if clean.total() < STRUCTURAL_SPARK_UNITS {
            return Err("The frame cannot establish the wand's structural spark.".into());
        }

        let keep = clean
            .total()
            .min(resolved.capacity.saturating_add(STRUCTURAL_SPARK_UNITS));
        let mut source_for_keep = clean.clone();
        let mut item_clean = source_for_keep
            .take_units(keep, resolved.resonance.keys().cloned())
            .map_err(|error| error.to_string())?;
        let overflow = source_for_keep;

        let usable_before_dross = item_clean.total().saturating_sub(STRUCTURAL_SPARK_UNITS);
        let generated_units = usable_before_dross
            .saturating_mul(u64::from(resolved.dross_per_thousand))
            .div_ceil(1_000)
            .min(usable_before_dross);
        let generated_dross = if generated_units == 0 {
            Current::default()
        } else {
            item_clean
                .take_units(generated_units, std::iter::empty())
                .map_err(|error| error.to_string())?
        };
        carried_dross
            .checked_add(&generated_dross)
            .map_err(|error| error.to_string())?;
        if item_clean.total() < STRUCTURAL_SPARK_UNITS {
            return Err("Assembly dross consumed the structural spark.".into());
        }

        let target = ArcaneOwner::Item(instance_id);
        let dross_target = ArcaneOwner::ItemDross(instance_id);
        let mut credits = BTreeMap::<ArcaneOwner, Current>::new();
        add_current(&mut credits, target.clone(), &item_clean)
            .map_err(|error| error.to_string())?;
        if !carried_dross.is_empty() {
            add_current(&mut credits, dross_target.clone(), &carried_dross)
                .map_err(|error| error.to_string())?;
        }
        if !overflow.is_empty() {
            add_current(&mut credits, ambient_owner.clone(), &overflow)
                .map_err(|error| error.to_string())?;
        }

        for (id, component) in &declared {
            next_state
                .component_manifests
                .insert(id.clone(), component.clone());
        }
        next_state
            .insert(ImplementInstance {
                instance_id,
                content_id: content_id.clone(),
                kind: ImplementKind::Wand {
                    parts,
                    resolved: resolved.clone(),
                },
                construction: mounts
                    .into_iter()
                    .flatten()
                    .map(|stack| physical_component(&self.reg, stack))
                    .collect(),
                wear: 0,
                strain: 0,
                provenance: format!("binding_frame:{pos:?}"),
                created_day: self.calendar_state.day(),
                format_version: crate::implements::IMPLEMENT_RESOLVER_VERSION,
                creative: self.mode == "creative",
            })
            .map_err(|error| error.to_string())?;
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: "assemble_wand".into(),
            instance_id,
            units: item_clean.total().saturating_sub(STRUCTURAL_SPARK_UNITS),
            dross: carried_dross.total(),
            actor: actor.into(),
            note: format!(
                "frame revision {revision}; containment {}",
                layout.containment
            ),
        });

        let transaction = transaction_from_maps(
            ledger,
            debits,
            credits,
            &content_id,
            "binding-frame wand assembly",
        )?;
        let replacement = LinkedFileReplacement {
            subsystem: "implements".into(),
            operation_id,
            relative_path: crate::implements::IMPLEMENTS_FILE.into(),
            after: Some(next_state.encode().map_err(|error| error.to_string())?),
        };
        ledger
            .commit_linked_files(transaction, vec![replacement])
            .map_err(|error| error.to_string())?;
        self.implements_state = Some(next_state);
        let Some(BlockEntity::BindingFrame(frame)) = self.installations.get_mut(&pos) else {
            return Err("The frame vanished after assembly committed.".into());
        };
        frame.body = None;
        frame.reservoir = None;
        frame.focus = None;
        frame.binding = None;
        frame.output = Some(ItemStack {
            item: output_item,
            count: 1,
            durability: self.reg.item(output_item).durability,
            arcane_id: instance_id,
        });
        frame.revision = frame.revision.saturating_add(1);
        let revision = frame.revision;
        self.save_entities().map_err(|error| error.to_string())?;
        Ok(FrameResult {
            success: true,
            revision,
            cue: ImplementCue::Use,
            message: format!(
                "The four parts settle into a bound wand: capacity {}, safe transfer {}, stability {}.",
                resolved.capacity, resolved.safe_transfer, resolved.stability
            ),
            preview: Some(resolved),
            lines: vec![format!(
                "Assembly retained {} dross units; no Current was created.",
                carried_dross.total()
            )],
        })
    }
}
