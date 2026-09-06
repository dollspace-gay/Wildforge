//! Frame break implements transaction coordination.

use super::add_current;
use super::transaction_from_maps;
use crate::arcane::ArcaneOwner;
use crate::arcane::DrossMedium;
use crate::arcane::LinkedFileReplacement;
use crate::implements::ImplementAuditEvent;
use crate::planet::BlockPos;
use crate::world::BlockEntity;
use crate::world::World;
use std::collections::BTreeMap;

impl World {
    /// Settle every non-structural unit held by the fitted output before the
    /// work surface disappears. Ordinary mounted items are still spilled by
    /// the block-entity removal path, but a broken frame is not a portable
    /// charger: usable Current first fills its adjacent vessel, then returns
    /// to local Ambient custody, while retained and break-induced dross enters
    /// the environment. The fitted implement keeps one structural spark and
    /// therefore the same stable identity when it falls clear.
    pub(crate) fn settle_binding_frame_break_at(
        &mut self,
        pos: BlockPos,
        controlled: bool,
    ) -> Result<(), String> {
        let output = match self.installations.get(&pos) {
            Some(BlockEntity::BindingFrame(frame)) => frame.output,
            _ => None,
        };
        let Some(output) = output else {
            return Ok(());
        };
        if output.arcane_id == 0 {
            return Ok(());
        }

        let layout = self.binding_frame_layout(pos);
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        let adjacent_vessel = self.adjacent_vessel(pos).ok();
        let mut next_state = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let instance = next_state
            .instance(output.arcane_id)
            .cloned()
            .ok_or("The fitted implement has no construction record.")?;
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no Current ledger.")?;

        let clean_owner = ArcaneOwner::Item(output.arcane_id);
        let retained_owner = ArcaneOwner::ItemDross(output.arcane_id);
        let clean = ledger
            .account(&clean_owner)
            .map(|account| account.current.clone())
            .ok_or("The fitted implement has no finite Current custody.")?;
        let retained = ledger
            .account(&retained_owner)
            .map(|account| account.current.clone())
            .unwrap_or_default();
        let usable = crate::implements::usable_charge(clean.total());
        if usable == 0 && retained.is_empty() {
            return Ok(());
        }

        let mut clean_left = clean.clone();
        let clean_debit = clean_left
            .take_units(usable, std::iter::empty())
            .map_err(|error| error.to_string())?;
        let mut routed_clean = clean_debit.clone();
        let exposure = if controlled {
            1_000u64.saturating_sub(u64::from(layout.containment))
        } else {
            1_000
        };
        let dross_divisor = if controlled { 20_000 } else { 4_000 };
        let induced_units = routed_clean
            .total()
            .saturating_mul(exposure)
            .div_ceil(dross_divisor)
            .min(routed_clean.total());
        let induced_dross = routed_clean
            .take_units(induced_units, std::iter::empty())
            .map_err(|error| error.to_string())?;

        let mut debits = BTreeMap::new();
        if !clean_debit.is_empty() {
            add_current(&mut debits, clean_owner, &clean_debit)
                .map_err(|error| error.to_string())?;
        }
        if !retained.is_empty() {
            add_current(&mut debits, retained_owner, &retained)
                .map_err(|error| error.to_string())?;
        }

        let mut credits = BTreeMap::new();
        if let Some((_, vessel, _)) = adjacent_vessel
            && vessel.arcane_id != 0
            && vessel.arcane_id != output.arcane_id
            && let Some(vessel_instance) = next_state.instance(vessel.arcane_id)
        {
            let vessel_usable = ledger
                .item_clean_total(vessel.arcane_id)
                .map(crate::implements::usable_charge)
                .unwrap_or_default();
            let free = vessel_instance
                .usable_capacity()
                .saturating_sub(vessel_usable);
            let into_vessel_units = free.min(routed_clean.total());
            if into_vessel_units != 0 {
                let into_vessel = routed_clean
                    .take_units(into_vessel_units, std::iter::empty())
                    .map_err(|error| error.to_string())?;
                add_current(
                    &mut credits,
                    ArcaneOwner::Item(vessel.arcane_id),
                    &into_vessel,
                )
                .map_err(|error| error.to_string())?;
            }
        }
        if !routed_clean.is_empty() {
            add_current(&mut credits, ArcaneOwner::Ambient(region), &routed_clean)
                .map_err(|error| error.to_string())?;
        }
        let mut environmental_dross = retained.clone();
        environmental_dross
            .checked_add(&induced_dross)
            .map_err(|error| error.to_string())?;
        if !environmental_dross.is_empty() {
            add_current(
                &mut credits,
                ArcaneOwner::Dross {
                    region,
                    medium: if controlled {
                        DrossMedium::Soil
                    } else {
                        DrossMedium::Air
                    },
                },
                &environmental_dross,
            )
            .map_err(|error| error.to_string())?;
        }

        let operation_id = next_state
            .operation_id()
            .map_err(|error| error.to_string())?;
        next_state.record(ImplementAuditEvent {
            operation_id,
            kind: "frame_break".into(),
            instance_id: output.arcane_id,
            units: clean_debit.total(),
            dross: environmental_dross.total(),
            actor: if controlled {
                "controlled dismantle".into()
            } else {
                "destructive break".into()
            },
            note: format!(
                "containment {}; stable output identity spilled at {pos:?}",
                layout.containment
            ),
        });
        let transaction = transaction_from_maps(
            ledger,
            debits,
            credits,
            &instance.content_id,
            "binding-frame break disposition",
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
        Ok(())
    }
}
