//! Transfer ritual workings transaction coordination.

use crate::arcane::ArcaneOwner;
use crate::planet::BlockPos;
use crate::arcane::Current;
use crate::workings::PhysicalDebit;
use crate::workings::PhysicalDebitKind;
use crate::workings::WorkingEffect;
use crate::workings::WorkingResult;
use crate::workings::WorkingTargetSnapshot;
use crate::world::World;

impl World {
    /// Discover two physically mounted adjacent vessels and reserve an exact
    /// payload plus the circle's lower transfer cost. Payload Current remains
    /// distinguishable from conversion Current all the way through settlement.
    #[cfg(test)]
    pub fn begin_transfer_circle_ritual(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        controller: BlockPos,
        requested_units: u64,
    ) -> Result<WorkingResult, String> {
        self.begin_transfer_circle_ritual_definition(
            actor,
            actor_label,
            "base:transfer_circle",
            controller,
            requested_units,
        )
    }

    pub fn begin_transfer_circle_ritual_definition(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        working_id: &str,
        controller: BlockPos,
        requested_units: u64,
    ) -> Result<WorkingResult, String> {
        let layout = self.binding_frame_layout(controller);
        if !layout.valid || layout.conductor_endpoints == 0 {
            return Err(format!(
                "The transfer circle is not physically complete: {}",
                layout.problems.join(" ")
            ));
        }
        let vessels = self.ritual_vessels(controller, 2);
        if vessels.len() != 2 {
            return Err(
                "A transfer circle needs exactly two mounted charge vessels within two cells."
                    .into(),
            );
        }
        let ledger = self
            .arcane_ledger
            .as_ref()
            .ok_or("The finite Current ledger is unavailable.")?;
        let mut ranked = vessels
            .into_iter()
            .map(|(pos, stack, revision, damage)| {
                let total = ledger
                    .account(&ArcaneOwner::Item(stack.arcane_id))
                    .map_or(0, |account| account.current.total());
                (total, pos, stack, revision, damage)
            })
            .collect::<Vec<_>>();
        ranked.sort_by_key(|entry| (entry.0, entry.1));
        let destination = ranked.remove(0);
        let source = ranked.remove(0);
        if source.0 <= destination.0 {
            return Err("The two vessels are already at the same charge level.".into());
        }
        let destination_instance = self
            .implements_state
            .as_ref()
            .and_then(|state| state.instance(destination.2.arcane_id))
            .ok_or("The destination vessel has no embodied implement record.")?;
        let destination_current = ledger
            .account(&ArcaneOwner::Item(destination.2.arcane_id))
            .map_or(0, |account| account.current.total());
        let free = destination_instance
            .kind
            .capacity()
            .saturating_sub(destination_current);
        let definition = self
            .reg
            .workings
            .get(working_id)
            .cloned()
            .ok_or("That transfer-circle definition is not registered.")?;
        let maximum = requested_units
            .clamp(1, u64::from(definition.max_magnitude))
            .min(free)
            .min(source.0);
        let mut amount = maximum;
        while amount != 0 {
            let quote = definition
                .quote(amount as u32, 1, 20 * 4)
                .map_err(|error| error.to_string())?;
            if amount.saturating_add(quote.charge) <= source.0 {
                break;
            }
            amount -= 1;
        }
        if amount == 0 {
            return Err("The source cannot pay both payload and circle conversion cost.".into());
        }
        let source_owner = ArcaneOwner::Item(source.2.arcane_id);
        let destination_owner = ArcaneOwner::Item(destination.2.arcane_id);
        let mut source_current = ledger
            .account(&source_owner)
            .ok_or("The source vessel is empty.")?
            .current
            .clone();
        let payload = source_current
            .take_units(amount, [definition.focus.clone()])
            .map_err(|error| error.to_string())?;
        let targets = vec![
            WorkingTargetSnapshot::Area {
                controller,
                revision: self.binding_frame_revision(controller)?,
                cells: vec![source.1, destination.1],
            },
            WorkingTargetSnapshot::Item {
                stable_id: source.2.arcane_id,
                item_name: self.reg.item(source.2.item).name.clone(),
                durability: source.2.durability,
                age_ticks: 0,
                version: source.3,
            },
            WorkingTargetSnapshot::Item {
                stable_id: destination.2.arcane_id,
                item_name: self.reg.item(destination.2.item).name.clone(),
                durability: destination.2.durability,
                age_ticks: 0,
                version: destination.3,
            },
        ];
        let physical = vec![
            PhysicalDebit {
                kind: PhysicalDebitKind::Item,
                source: format!("vessel:{:?}", source.1),
                content_id: self.reg.item(source.2.item).name.clone(),
                units: amount,
                expected_version: source.3,
            },
            PhysicalDebit {
                kind: PhysicalDebitKind::Item,
                source: format!("vessel:{:?}", destination.1),
                content_id: self.reg.item(destination.2.item).name.clone(),
                units: amount,
                expected_version: destination.3,
            },
        ];
        self.reserve_ritual_effect(
            actor,
            actor_label,
            controller,
            source.2.arcane_id,
            self.binding_frame_revision(controller)?,
            source.4,
            working_id,
            targets,
            physical,
            WorkingEffect::TransferCurrent {
                from: source_owner,
                to: destination_owner,
                current: payload.clone(),
            },
            amount as u32,
            1,
            20 * 4,
            payload,
        )
    }
}
