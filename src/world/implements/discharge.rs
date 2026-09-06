//! Discharge implements transaction coordination.

use super::add_current;
use super::transaction_from_maps;
use crate::arcane::ArcaneOwner;
use crate::arcane::Current;
use crate::arcane::DrossMedium;
use crate::arcane::LinkedFileReplacement;
use crate::implements::FrameResult;
use crate::implements::ImplementAuditEvent;
use crate::implements::ImplementCue;
use crate::planet::BlockPos;
use crate::world::BlockEntity;
use crate::world::World;
use std::collections::BTreeMap;

impl World {
    pub(super) fn discharge_at_frame(
        &mut self,
        pos: BlockPos,
        actor: &str,
    ) -> Result<FrameResult, String> {
        let layout = self.binding_frame_layout(pos);
        if !layout.valid {
            return Err(layout.problems.join(" "));
        }
        let (output, frame_revision) = match self.installations.get(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => (
                frame
                    .output
                    .ok_or("Fit an implement in the finished cradle.")?,
                frame.revision,
            ),
            _ => return Err("The frame has no mounts.".into()),
        };
        if output.arcane_id == 0 {
            return Err("The fitted object carries no bound Current.".into());
        }
        let mut next_state = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let instance = next_state
            .instance(output.arcane_id)
            .cloned()
            .ok_or("The fitted implement has no construction record.")?;
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no Current ledger.")?;
        let source_owner = ArcaneOwner::Item(output.arcane_id);
        let source = ledger
            .account(&source_owner)
            .cloned()
            .ok_or("The fitted implement is dormant.")?;
        let usable = crate::implements::usable_charge(source.current.total());
        if usable == 0 {
            return Err("The fitted implement is already dormant.".into());
        }
        let mut remaining = source.current.clone();
        let mut discharged = remaining
            .take_units(usable, std::iter::empty())
            .map_err(|error| error.to_string())?;
        let dross_units = usable.div_ceil(100).min(usable.saturating_sub(1));
        let dross = if dross_units == 0 {
            Current::default()
        } else {
            discharged
                .take_units(dross_units, std::iter::empty())
                .map_err(|error| error.to_string())?
        };
        let ambient = ArcaneOwner::Ambient(region);
        let environmental_dross = ArcaneOwner::Dross {
            region,
            medium: DrossMedium::Air,
        };
        let mut debits = BTreeMap::new();
        let mut total = discharged.clone();
        total
            .checked_add(&dross)
            .map_err(|error| error.to_string())?;
        add_current(&mut debits, source_owner, &total).map_err(|error| error.to_string())?;
        let mut credits = BTreeMap::new();
        add_current(&mut credits, ambient, &discharged).map_err(|error| error.to_string())?;
        if !dross.is_empty() {
            add_current(&mut credits, environmental_dross, &dross)
                .map_err(|error| error.to_string())?;
        }
        let operation_id = next_state
            .operation_id()
            .map_err(|error| error.to_string())?;
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: "safe_discharge".into(),
            instance_id: output.arcane_id,
            units: discharged.total(),
            dross: dross.total(),
            actor: actor.into(),
            note: format!(
                "frame revision {frame_revision}; containment {}",
                layout.containment
            ),
        });
        let transaction = transaction_from_maps(
            ledger,
            debits,
            credits,
            &instance.content_id,
            "binding-frame safe environmental discharge",
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
        let revision =
            if let Some(BlockEntity::BindingFrame(frame)) = self.installations.get_mut(&pos) {
                frame.revision = frame.revision.saturating_add(1);
                frame.revision
            } else {
                frame_revision
            };
        self.save_entities().map_err(|error| error.to_string())?;
        Ok(FrameResult {
            success: true,
            revision,
            cue: ImplementCue::Transfer,
            message: format!(
                "Safely discharged {} usable Current into the local environment.",
                usable
            ),
            preview: self.frame_preview(pos).ok(),
            lines: vec![format!(
                "{} units became measured airborne dross.",
                dross.total()
            )],
        })
    }
}
