//! Calibration implements transaction coordination.

use super::VESSEL_INITIAL_CHARGE;
use super::horizontal_neighbors;
use super::physical_component;
use crate::arcane::AccountRead;
use crate::arcane::ArcaneAuthority;
use crate::arcane::ArcaneMove;
use crate::arcane::ArcaneOwner;
use crate::arcane::ArcaneTransaction;
use crate::arcane::LinkedFileReplacement;
use crate::implements::FrameResult;
use crate::implements::ImplementAuditEvent;
use crate::implements::ImplementCue;
use crate::implements::ImplementInstance;
use crate::implements::ImplementKind;
use crate::implements::STRUCTURAL_SPARK_UNITS;
use crate::implements::VESSEL_CAPACITY;
use crate::implements::VESSEL_SAFE_TRANSFER;
use crate::planet::BlockPos;
use crate::world::BlockEntity;
use crate::world::World;

impl World {
    pub(super) fn calibrate_frame_or_vessel(
        &mut self,
        pos: BlockPos,
        actor: &str,
    ) -> Result<FrameResult, String> {
        let layout = self.binding_frame_layout(pos);
        if !layout.valid {
            return Err(layout.problems.join(" "));
        }
        let vessel_pos = horizontal_neighbors(pos)
            .into_iter()
            .find(|at| {
                matches!(
                    self.installations.get(at),
                    Some(BlockEntity::ChargeVessel(_))
                )
            })
            .ok_or("The adjacent vessel has no physical state.")?;
        let (stack, damage, vessel_revision) = match self.installations.get(&vessel_pos) {
            Some(BlockEntity::ChargeVessel(vessel)) => (
                vessel.vessel.ok_or("The vessel shell is missing.")?,
                vessel.damage,
                vessel.revision,
            ),
            _ => return Err("The adjacent vessel has no physical state.".into()),
        };
        if damage >= 900 {
            return Err("The vessel is too damaged to accept a calibration pulse.".into());
        }
        if stack.arcane_id != 0 {
            return self.calibration_pulse(pos, actor);
        }
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        let source = ArcaneOwner::Ambient(region);
        self.ensure_regional_ambient_units(
            region,
            VESSEL_INITIAL_CHARGE + STRUCTURAL_SPARK_UNITS + 1,
            "binding-frame vessel calibration reserve",
        )?;
        let mut next_state = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no Current ledger.")?;
        let source_account = ledger
            .account(&source)
            .ok_or("This region has no measurable Ambient Current.")?;
        let requested = VESSEL_INITIAL_CHARGE + STRUCTURAL_SPARK_UNITS + 1;
        let mut available = source_account.current.clone();
        let selected = available
            .take_units(
                requested,
                crate::arcane::BASE_RESONANCES
                    .into_iter()
                    .map(str::to_string),
            )
            .map_err(|_| "The regional Ambient reserve cannot calibrate the vessel.")?;
        let mut clean = selected.clone();
        let dross = clean
            .take_units(1, std::iter::empty())
            .map_err(|error| error.to_string())?;
        let instance_id = ledger
            .allocate_item_id()
            .map_err(|error| error.to_string())?;
        let operation_id = next_state
            .operation_id()
            .map_err(|error| error.to_string())?;
        next_state
            .insert(ImplementInstance {
                instance_id,
                content_id: self.reg.item(stack.item).name.clone(),
                kind: ImplementKind::Vessel {
                    capacity: VESSEL_CAPACITY,
                    safe_transfer: VESSEL_SAFE_TRANSFER,
                    containment: layout.containment,
                },
                construction: vec![physical_component(&self.reg, stack)],
                wear: 0,
                strain: u32::from(damage) * 5,
                provenance: format!("calibrated_vessel:{vessel_pos:?}"),
                created_day: self.calendar_state.day(),
                format_version: crate::implements::IMPLEMENT_RESOLVER_VERSION,
                creative: self.mode == "creative",
            })
            .map_err(|error| error.to_string())?;
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: "calibrate_vessel".into(),
            instance_id,
            units: clean.total().saturating_sub(STRUCTURAL_SPARK_UNITS),
            dross: dross.total(),
            actor: actor.into(),
            note: format!("vessel revision {vessel_revision}"),
        });
        let target = ArcaneOwner::Item(instance_id);
        let dross_target = ArcaneOwner::ItemDross(instance_id);
        let mut transaction = ArcaneTransaction {
            id: ledger
                .system_transaction_id()
                .map_err(|error| error.to_string())?,
            reads: vec![
                AccountRead {
                    owner: source.clone(),
                    expected_version: ledger.version_of(&source),
                },
                AccountRead {
                    owner: target.clone(),
                    expected_version: 0,
                },
                AccountRead {
                    owner: dross_target.clone(),
                    expected_version: 0,
                },
            ],
            debits: vec![ArcaneMove {
                owner: source,
                current: selected,
                content_id: None,
            }],
            credits: vec![
                ArcaneMove {
                    owner: target,
                    current: clean,
                    content_id: Some(self.reg.item(stack.item).name.clone()),
                },
                ArcaneMove {
                    owner: dross_target,
                    current: dross,
                    content_id: Some(self.reg.item(stack.item).name.clone()),
                },
            ],
            transforms: Vec::new(),
            authority: ArcaneAuthority::System,
            reason: "binding-frame vessel calibration".into(),
            content_id: self.reg.item(stack.item).name.clone(),
            linked: Vec::new(),
        };
        transaction.reads.sort_by(|a, b| a.owner.cmp(&b.owner));
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
        let Some(BlockEntity::ChargeVessel(vessel)) = self.installations.get_mut(&vessel_pos)
        else {
            return Err("The vessel vanished after calibration committed.".into());
        };
        if vessel.revision != vessel_revision {
            return Err("The vessel changed during calibration.".into());
        }
        let Some(physical) = vessel.vessel.as_mut() else {
            return Err("The vessel shell vanished during calibration.".into());
        };
        physical.arcane_id = instance_id;
        vessel.revision = vessel.revision.saturating_add(1);
        self.save_entities().map_err(|error| error.to_string())?;
        let frame_revision = match self.installations.get(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => frame.revision,
            _ => 0,
        };
        Ok(FrameResult {
            success: true,
            revision: frame_revision,
            cue: ImplementCue::Transfer,
            message: "The vessel accepts a measured calibration pulse and begins to glow.".into(),
            preview: self.frame_preview(pos).ok(),
            lines: vec![
                "One unit settled as contained dross; the rest remains finite Current.".into(),
            ],
        })
    }

    pub(super) fn calibration_pulse(
        &mut self,
        pos: BlockPos,
        actor: &str,
    ) -> Result<FrameResult, String> {
        self.transfer_at_frame_with_limit(pos, actor, Some(2), true)
    }
}
