//! Reading interaction adapter.

use crate::world::TerrainRead;
use crate::audio::Sfx;
use crate::identity;
use crate::net;
use crate::world;
use crate::game::DiscoveryAim;
use crate::game::Game;

impl Game {

    pub(in crate::game) fn read_held_knowledge(&mut self) {
        let slot = self.input.hotbar_sel;
        if let Some(remote) = &self.multiplayer.remote {
            self.ui_state.discovery_holder =
                Some(net::RecordHolderSnap::Inventory { slot: slot as u8 });
            self.ui_state.discovery_copy_target = None;
            self.ui_state.discovery_writing_pos = None;
            remote
                .session
                .send(&net::C2S::ReadKnowledge { slot: slot as u8 });
            self.sfx(Sfx::Click);
            return;
        }
        let Some(mut stack) = self.inventory.slots[slot] else {
            return;
        };
        let Some(at) = self.player.pos.block() else {
            return;
        };
        if let Some(text) = self.runtime.local_mut().world.discovery_artifact_text(&mut stack, at) {
            self.inventory.slots[slot] = Some(stack);
            self.toast(text);
            self.sfx(Sfx::Click);
            return;
        }
        if self.runtime.local_mut().world.bind_discovery_stack_at(at, &mut stack)
            .is_ok()
            && stack.arcane_id != 0
        {
            self.inventory.slots[slot] = Some(stack);
            match self.runtime.local().world.discovery_summaries(stack.arcane_id, true) {
                Ok(records) if records.is_empty() => {
                    self.open_discovery_catalogue(
                        net::RecordHolderSnap::Inventory { slot: slot as u8 },
                        records,
                        crate::discovery::FIELD_LEDGER_RECORDS as u16,
                        None,
                        None,
                    );
                }
                Ok(records) => {
                    let capacity = if self
                        .content
                        .reg
                        .item(stack.item)
                        .discovery
                        .as_ref()
                        .is_some_and(|definition| definition.kind == "survey_folio")
                    {
                        crate::discovery::SURVEY_FOLIO_RECORDS
                    } else {
                        crate::discovery::FIELD_LEDGER_RECORDS
                    };
                    self.open_discovery_catalogue(
                        net::RecordHolderSnap::Inventory { slot: slot as u8 },
                        records,
                        capacity as u16,
                        None,
                        None,
                    );
                }
                Err(error) => self.toast(error.to_string()),
            }
        }
        self.sfx(Sfx::Click);
    }

    pub(in crate::game) fn present_discovery_record(
        &mut self,
        record: &crate::discovery::ObservationSummary,
    ) {
        let unstable = record.reading.contains("strained")
            || record.reading.contains("fouled")
            || record.reading.contains("dangerous")
            || record.reading.contains("breaking");
        self.sfx(Sfx::Lens(if unstable { 0.55 } else { 1.18 }));
        let label = record
            .label
            .as_deref()
            .map_or(String::new(), |label| format!(" — {label}"));
        self.toast(format!(
            "{}{}: {}",
            record.category.to_uppercase(),
            label,
            record.reading
        ));
        if !record.properties.is_empty() {
            self.toast(
                record
                    .properties
                    .iter()
                    .map(|(property, value)| format!("{property}: {value}"))
                    .collect::<Vec<_>>()
                    .join("; "),
            );
        }
        let place = record
            .provenance
            .place
            .as_deref()
            .unwrap_or(&record.provenance.biome);
        let location = record.location.map_or_else(
            || "location withheld".to_string(),
            |position| {
                format!(
                    "{} {},{},{}",
                    position.face(),
                    position.u(),
                    position.y(),
                    position.v()
                )
            },
        );
        self.toast(format!(
            "Recorded by {} · day {} {} · {} · {}{}",
            record.observer_name,
            record.day,
            record.season,
            place,
            location,
            if record.obsolete_content {
                " · OBSOLETE CONTENT VERSION"
            } else {
                ""
            }
        ));
    }

    pub(in crate::game) fn settle_discovery_reading(&mut self, aim: DiscoveryAim) {
        let Some(at) = self.player.pos.block() else {
            return;
        };
        let ledger_slot = self.inventory.slots.iter().position(|stack| {
            stack.is_some_and(|stack| {
                self.content
                    .reg
                    .item(stack.item)
                    .discovery
                    .as_ref()
                    .is_some_and(|definition| definition.kind == "field_ledger")
            })
        });
        let Some(ledger_slot) = ledger_slot else {
            self.toast("Carry a field ledger to record the settled reading.".into());
            return;
        };
        let calibration_slot = self.inventory.slots.iter().position(|stack| {
            stack.is_some_and(|stack| {
                self.content
                    .reg
                    .item(stack.item)
                    .discovery
                    .as_ref()
                    .is_some_and(|definition| definition.kind == "calibration_plate")
            })
        });
        let label = (!self.ui_state.discovery_label.trim().is_empty())
            .then(|| self.ui_state.discovery_label.trim().to_string());
        if let Some(remote) = &self.multiplayer.remote {
            remote.session.send(&net::C2S::Observe {
                target: match aim {
                    DiscoveryAim::Region(_) => net::DiscoveryTargetSnap::Region,
                    DiscoveryAim::Block(pos) => net::DiscoveryTargetSnap::Block(pos),
                },
                ledger_slot: ledger_slot as u8,
                calibration_slot: calibration_slot.map(|slot| slot as u8),
                label,
            });
            return;
        }
        let mut ledger = self.inventory.slots[ledger_slot].expect("located ledger");
        if let Err(error) = self.runtime.local_mut().world.bind_discovery_stack_at(at, &mut ledger) {
            self.toast(error.to_string());
            return;
        }
        self.inventory.slots[ledger_slot] = Some(ledger);
        let calibration = calibration_slot
            .and_then(|slot| self.inventory.slots[slot])
            .and_then(|stack| self.runtime.local().world.calibration_grade_for(stack))
            .unwrap_or(crate::discovery::CalibrationGrade::Field);
        let player_id = identity::local_player_id(
            &self.runtime.local().world.save_dir_for_saving(),
            self.identity.device_id(),
        )
        .unwrap_or(identity::PlayerId([0; 16]));
        let target = match aim {
            DiscoveryAim::Region(pos) => world::ObservationTarget::Region(pos),
            DiscoveryAim::Block(pos) => world::ObservationTarget::Block(pos),
        };
        match self.runtime.local_mut().world.record_observation(
            ledger.arcane_id,
            (player_id, &self.config.display_name),
            target,
            calibration,
            label,
            None,
        ) {
            Ok(record) => {
                let spent = self.runtime.local_mut().world.wear_tuning_lens_at(
                    at,
                    &mut self.inventory,
                    self.input.hotbar_sel,
                );
                self.present_discovery_record(&record);
                if spent {
                    self.toast("The Wellglass clouds; the fitted frame and plate remain.".into());
                }
                if let Err(error) = self.runtime.local_mut().world.save_discovery() {
                    self.toast(format!("The ledger could not be saved: {error}"));
                }
            }
            Err(error) => self.toast(error.to_string()),
        }
    }

    pub(in crate::game) fn settle_discovery_experiment(&mut self, pos: crate::planet::BlockPos) {
        let kind = crate::discovery::ExperimentKind::ALL
            [self.interaction.experiment_kind % crate::discovery::ExperimentKind::ALL.len()];
        self.interaction.experiment_kind =
            (self.interaction.experiment_kind + 1) % crate::discovery::ExperimentKind::ALL.len();
        let ledger_slot = self.inventory.slots.iter().position(|stack| {
            stack.is_some_and(|stack| {
                self.content
                    .reg
                    .item(stack.item)
                    .discovery
                    .as_ref()
                    .is_some_and(|definition| definition.kind == "field_ledger")
            })
        });
        let Some(ledger_slot) = ledger_slot else {
            self.toast("Carry a field ledger for the apparatus record.".into());
            return;
        };
        let calibration_slot = self.inventory.slots.iter().position(|stack| {
            stack.is_some_and(|stack| {
                self.content
                    .reg
                    .item(stack.item)
                    .discovery
                    .as_ref()
                    .is_some_and(|definition| definition.kind == "calibration_plate")
            })
        });
        if let Some(remote) = &self.multiplayer.remote {
            remote.session.send(&net::C2S::RunExperiment {
                pos,
                kind,
                ledger_slot: ledger_slot as u8,
                calibration_slot: calibration_slot.map(|slot| slot as u8),
            });
            self.toast(format!("Settling {}...", kind.label()));
            return;
        }
        let Some(at) = self.player.pos.block() else {
            return;
        };
        let mut ledger = self.inventory.slots[ledger_slot].expect("located ledger");
        if let Err(error) = self.runtime.local_mut().world.bind_discovery_stack_at(at, &mut ledger) {
            self.toast(error.to_string());
            return;
        }
        self.inventory.slots[ledger_slot] = Some(ledger);
        let sample = match self.runtime.local().world.experiment_sample_at(pos, kind) {
            Ok(sample) => sample,
            Err(error) => {
                self.toast(error);
                return;
            }
        };
        let calibration = calibration_slot
            .and_then(|slot| self.inventory.slots[slot])
            .and_then(|stack| self.runtime.local().world.calibration_grade_for(stack))
            .unwrap_or(crate::discovery::CalibrationGrade::Field);
        let player_id = identity::local_player_id(
            &self.runtime.local().world.save_dir_for_saving(),
            self.identity.device_id(),
        )
        .unwrap_or(identity::PlayerId([0; 16]));
        match self.runtime.local_mut().world.record_observation(
            ledger.arcane_id,
            (player_id, &self.config.display_name),
            world::ObservationTarget::Item(sample, pos),
            calibration,
            Some(kind.label().into()),
            Some(kind),
        ) {
            Ok(record) => {
                let spent = self.runtime.local_mut().world.wear_tuning_lens_at(
                    at,
                    &mut self.inventory,
                    self.input.hotbar_sel,
                );
                self.present_discovery_record(&record);
                if spent {
                    self.toast("The Wellglass clouds; the fitted frame and plate remain.".into());
                }
                if let Err(error) = self.runtime.local_mut().world.save_discovery() {
                    self.toast(format!("The experiment record could not be saved: {error}"));
                }
            }
            Err(error) => self.toast(error.to_string()),
        }
    }
}
