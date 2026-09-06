//! Charm binding implements transaction coordination.

use super::add_current;
use super::charm_resonance_preference;
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
    // Every frame verb below shares this host-authoritative surface and the
    // same finite-ledger transaction rules.
    pub(super) fn bind_charm_at_frame(
        &mut self,
        pos: BlockPos,
        actor: &str,
    ) -> Result<FrameResult, String> {
        let layout = self.binding_frame_layout(pos);
        if !layout.valid {
            return Err(layout.problems.join(" "));
        }
        let (blank, mounts, frame_revision) = match self.installations.get(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => (
                frame
                    .output
                    .ok_or("Place an unbound charm in the finished cradle.")?,
                frame.mounts(),
                frame.revision,
            ),
            _ => return Err("The frame has no physical mounts.".into()),
        };
        if blank.arcane_id != 0 {
            return Err("That finished cradle already holds a bound object.".into());
        }
        let blank_name = self.reg.item(blank.item).name.as_str();
        let (target_name, effect, reagent_names): (&str, crate::implements::CharmEffect, &[&str]) =
            match blank_name {
                "base:quiet_charm_blank" => (
                    "base:charm_quiet",
                    crate::implements::CharmEffect::Quiet,
                    &["base:nightglass_pod", "base:echo_slate"],
                ),
                "base:bark_charm_blank" => (
                    "base:charm_bark",
                    crate::implements::CharmEffect::Bark,
                    &["base:hushwood_switch", "base:pilgrim_root_cutting"],
                ),
                "base:hunger_charm_blank" => (
                    "base:charm_hunger",
                    crate::implements::CharmEffect::Hunger,
                    &["base:ashlace_tissue", "base:wellglass_shard"],
                ),
                _ => {
                    return Err("Place one of the three unbound charm bodies in the cradle.".into());
                }
            };
        let reagent = mounts
            .into_iter()
            .flatten()
            .find(|stack| reagent_names.contains(&self.reg.item(stack.item).name.as_str()))
            .ok_or_else(|| {
                format!(
                    "The {} binding needs a matching living or mineral reagent in a mount.",
                    effect.id()
                )
            })?;
        let target_item = self
            .reg
            .item_id(target_name)
            .ok_or("The bound charm content definition is missing.")?;
        let definition = self
            .reg
            .item(target_item)
            .charm_def
            .clone()
            .ok_or("The bound charm has no authoritative effect definition.")?;
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        let ambient = ArcaneOwner::Ambient(region);
        self.ensure_regional_ambient_units(
            region,
            definition.capacity.min(64) + STRUCTURAL_SPARK_UNITS,
            "binding-frame charm binding reserve",
        )?;
        let mut next_state = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no Current ledger.")?;
        let mut clean = Current::default();
        let mut carried_dross = Current::default();
        let mut debits = BTreeMap::new();
        if reagent.arcane_id != 0 {
            for (owner, is_dross) in [
                (ArcaneOwner::Item(reagent.arcane_id), false),
                (ArcaneOwner::ItemDross(reagent.arcane_id), true),
            ] {
                if let Some(account) = ledger.account(&owner) {
                    add_current(&mut debits, owner, &account.current)
                        .map_err(|error| error.to_string())?;
                    if is_dross {
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
        let desired_initial = definition.capacity.min(64) + STRUCTURAL_SPARK_UNITS;
        if clean.total() < desired_initial {
            let missing = desired_initial - clean.total();
            let ambient_account = ledger
                .account(&ambient)
                .ok_or("This region has no measurable Ambient Current.")?;
            let mut available = ambient_account.current.clone();
            let selected = available
                .take_units(missing, charm_resonance_preference(effect))
                .map_err(|_| "The regional Ambient reserve cannot settle this charm.")?;
            add_current(&mut debits, ambient.clone(), &selected)
                .map_err(|error| error.to_string())?;
            clean
                .checked_add(&selected)
                .map_err(|error| error.to_string())?;
        }
        let keep = clean
            .total()
            .min(definition.capacity.saturating_add(STRUCTURAL_SPARK_UNITS));
        let mut source = clean;
        let mut item_clean = source
            .take_units(keep, charm_resonance_preference(effect))
            .map_err(|error| error.to_string())?;
        let overflow = source;
        let usable = item_clean.total().saturating_sub(STRUCTURAL_SPARK_UNITS);
        let generated_units = usable
            .saturating_mul(u64::from(definition.dross_per_transfer))
            .div_ceil(1_000)
            .min(usable);
        if generated_units != 0 {
            let generated = item_clean
                .take_units(generated_units, std::iter::empty())
                .map_err(|error| error.to_string())?;
            carried_dross
                .checked_add(&generated)
                .map_err(|error| error.to_string())?;
        }
        if item_clean.total() < STRUCTURAL_SPARK_UNITS {
            return Err("The binding would consume the charm's structural spark.".into());
        }
        let instance_id = ledger
            .allocate_item_id()
            .map_err(|error| error.to_string())?;
        let target = ArcaneOwner::Item(instance_id);
        let dross_target = ArcaneOwner::ItemDross(instance_id);
        let mut credits = BTreeMap::new();
        add_current(&mut credits, target, &item_clean).map_err(|error| error.to_string())?;
        if !carried_dross.is_empty() {
            add_current(&mut credits, dross_target, &carried_dross)
                .map_err(|error| error.to_string())?;
        }
        if !overflow.is_empty() {
            add_current(&mut credits, ambient, &overflow).map_err(|error| error.to_string())?;
        }
        let operation_id = next_state
            .operation_id()
            .map_err(|error| error.to_string())?;
        next_state
            .charm_manifests
            .insert(target_name.into(), definition.clone());
        next_state
            .insert(ImplementInstance {
                instance_id,
                content_id: target_name.into(),
                kind: ImplementKind::Charm {
                    effect,
                    capacity: definition.capacity,
                    charge_per_trigger: definition.charge_per_trigger,
                    stability: definition.stability,
                    dross_per_transfer: definition.dross_per_transfer,
                },
                construction: vec![
                    physical_component(&self.reg, blank),
                    physical_component(&self.reg, reagent),
                ],
                wear: 0,
                strain: 0,
                provenance: format!("charm_binding:{pos:?}"),
                created_day: self.calendar_state.day(),
                format_version: crate::implements::IMPLEMENT_RESOLVER_VERSION,
                creative: self.mode == "creative",
            })
            .map_err(|error| error.to_string())?;
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: "bind_charm".into(),
            instance_id,
            units: crate::implements::usable_charge(item_clean.total()),
            dross: carried_dross.total(),
            actor: actor.into(),
            note: format!("{} binding at frame revision {frame_revision}", effect.id()),
        });
        let transaction = transaction_from_maps(
            ledger,
            debits,
            credits,
            target_name,
            "binding-frame charm binding",
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
            return Err("The frame vanished after binding committed.".into());
        };
        for mount in [
            &mut frame.body,
            &mut frame.reservoir,
            &mut frame.focus,
            &mut frame.binding,
        ] {
            if mount.is_some_and(|stack| stack == reagent) {
                *mount = None;
                break;
            }
        }
        frame.output = Some(ItemStack {
            item: target_item,
            count: 1,
            durability: self.reg.item(target_item).durability,
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
                "The {} charm settles into a reproducible charged implement.",
                effect.id()
            ),
            preview: None,
            lines: vec![format!(
                "Usable charge {}; retained dross {}.",
                crate::implements::usable_charge(item_clean.total()),
                carried_dross.total()
            )],
        })
    }
}
