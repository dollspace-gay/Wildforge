//! Settling rite workings transaction coordination.

use super::working_distance;
use crate::arcane::ArcaneOwner;
use crate::arcane::Current;
use crate::planet::BlockPos;
use crate::workings::PhysicalDebit;
use crate::workings::PhysicalDebitKind;
use crate::workings::WorkingEffect;
use crate::workings::WorkingHandler;
use crate::workings::WorkingPhase;
use crate::workings::WorkingResult;
use crate::workings::WorkingTargetSnapshot;
use crate::world::World;

impl World {
    /// Stabilize one already-running adjacent magical process. The rite uses
    /// a distinct charge source and dross vessel, deliberately lengthens the
    /// target interval, and can later route (not delete) its reduced dross.
    #[cfg(test)]
    pub fn begin_settling_rite(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        controller: BlockPos,
    ) -> Result<WorkingResult, String> {
        self.begin_settling_rite_definition(actor, actor_label, "base:settling_rite", controller)
    }

    pub fn begin_settling_rite_definition(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        working_id: &str,
        controller: BlockPos,
    ) -> Result<WorkingResult, String> {
        let layout = self.binding_frame_layout(controller);
        if !layout.valid {
            return Err(format!(
                "The settling frame is incomplete: {}",
                layout.problems.join(" ")
            ));
        }
        let tick = self.working_tick();
        let process = self
            .workings_state
            .as_ref()
            .into_iter()
            .flat_map(|state| state.active.values())
            .filter(|transaction| {
                transaction.definition.handler != WorkingHandler::SettlingRite
                    && transaction.phase != WorkingPhase::PendingApply
                    && controller
                        .entity_center()
                        .distance_to(transaction.source.entity_center())
                        <= 3.5
            })
            .min_by_key(|transaction| transaction.id)
            .cloned()
            .ok_or("The rite needs one running magical process within three blocks.")?;
        let vessels = self.ritual_vessels(controller, 2);
        if vessels.len() < 2 {
            return Err(
                "The settling rite needs separate mounted charge and dross vessels.".into(),
            );
        }
        let ledger = self
            .arcane_ledger
            .as_ref()
            .ok_or("The finite Current ledger is unavailable.")?;
        let source = vessels
            .iter()
            .max_by_key(|(_, stack, _, _)| {
                ledger
                    .account(&ArcaneOwner::Item(stack.arcane_id))
                    .map_or(0, |account| account.current.total())
            })
            .cloned()
            .ok_or("The settling rite has no mounted charge source.")?;
        let dross_vessel = vessels
            .iter()
            .filter(|(_, stack, _, _)| stack.arcane_id != source.1.arcane_id)
            .min_by_key(|(_, stack, _, _)| {
                ledger
                    .account(&ArcaneOwner::ItemDross(stack.arcane_id))
                    .map_or(0, |account| account.current.total())
            })
            .cloned()
            .ok_or("The settling rite needs a distinct dross vessel.")?;
        let vessel_capacity = self
            .implements_state
            .as_ref()
            .and_then(|state| state.instance(dross_vessel.1.arcane_id))
            .map(|instance| instance.kind.capacity())
            .ok_or("The dross vessel has no authoritative capacity.")?;
        let vessel_load = ledger
            .account(&ArcaneOwner::Item(dross_vessel.1.arcane_id))
            .map_or(0, |account| account.current.total())
            .saturating_add(
                ledger
                    .account(&ArcaneOwner::ItemDross(dross_vessel.1.arcane_id))
                    .map_or(0, |account| account.current.total()),
            );
        if vessel_load.saturating_add(process.dross_current.total()) > vessel_capacity {
            return Err("The physical dross vessel cannot contain the target process load.".into());
        }
        let cells = vec![source.0, dross_vessel.0];
        let mut targets = vec![WorkingTargetSnapshot::Area {
            controller,
            revision: self.binding_frame_revision(controller)?,
            cells: cells.clone(),
        }];
        for (_, stack, revision, _) in [&source, &dross_vessel] {
            targets.push(WorkingTargetSnapshot::Item {
                stable_id: stack.arcane_id,
                item_name: self.reg.item(stack.item).name.clone(),
                durability: stack.durability,
                age_ticks: 0,
                version: *revision,
            });
        }
        targets.push(WorkingTargetSnapshot::Entity {
            stable_id: process.id,
            kind: "magical_process".into(),
            version: process.completion_nonce,
        });
        let remaining = process.due_tick.saturating_sub(tick).max(20);
        self.reserve_ritual_effect(
            actor,
            actor_label,
            controller,
            source.1.arcane_id,
            self.binding_frame_revision(controller)?,
            source.3.max(dross_vessel.3),
            working_id,
            targets,
            vec![
                PhysicalDebit {
                    kind: PhysicalDebitKind::Durability,
                    source: format!("settling_frame:{controller:?}"),
                    content_id: "frame_conductor_containment".into(),
                    units: u64::from(layout.network_size.max(1)),
                    expected_version: self.binding_frame_revision(controller)?,
                },
                PhysicalDebit {
                    kind: PhysicalDebitKind::Item,
                    source: format!("dross_vessel:{:?}", dross_vessel.0),
                    content_id: self.reg.item(dross_vessel.1.item).name.clone(),
                    units: 1,
                    expected_version: dross_vessel.2,
                },
            ],
            WorkingEffect::Settle {
                controller,
                process_id: process.id,
                dross_vessel_id: dross_vessel.1.arcane_id,
                dross_routed: 0,
                stabilizer_wear: 0,
            },
            u32::try_from(process.dross_current.total().clamp(1, 64)).unwrap_or(64),
            working_distance(controller, process.source),
            remaining.min(20 * 60),
            Current::default(),
        )
    }
}
