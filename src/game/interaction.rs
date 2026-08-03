//! Player interaction, combat, stations, and script-driven actions.

use super::*;

impl Game {
    pub(super) fn open_discovery_catalogue(
        &mut self,
        holder: net::RecordHolderSnap,
        records: Vec<crate::discovery::ObservationSummary>,
        capacity: u16,
        copy_target: Option<net::RecordHolderSnap>,
        writing_pos: Option<crate::planet::BlockPos>,
    ) {
        self.set_screen(Screen::Inventory);
        self.ui_state.discovery_holder = Some(holder);
        self.ui_state.discovery_copy_target = copy_target;
        self.ui_state.discovery_writing_pos = writing_pos;
        self.ui_state.discovery_records = records;
        self.ui_state.discovery_capacity = capacity;
        self.ui_state.discovery_page = 0;
        self.ui_state.discovery_selected = [None; 2];
        self.ui_state.inventory_status_open = false;
        self.ui_state.inventory_browser_open = false;
        self.ui_state.inventory_discovery_open = true;
        self.ui_state.search_focus = false;
    }

    pub(super) fn receive_discovery_catalogue(
        &mut self,
        holder: net::RecordHolderSnap,
        records: Vec<crate::discovery::ObservationSummary>,
        capacity: u16,
    ) {
        let copy_target = if self.ui_state.discovery_holder == Some(holder) {
            self.ui_state.discovery_copy_target
        } else if self.ui_state.discovery_copy_target == Some(holder) {
            self.ui_state.discovery_holder
        } else {
            None
        };
        let writing_pos = if copy_target.is_some() {
            self.ui_state.discovery_writing_pos
        } else {
            None
        };
        self.open_discovery_catalogue(holder, records, capacity, copy_target, writing_pos);
    }

    fn local_discovery_holder_id(&mut self, holder: net::RecordHolderSnap) -> Result<u64, String> {
        match holder {
            net::RecordHolderSnap::Inventory { slot } => {
                let index = usize::from(slot);
                let mut stack = self
                    .inventory
                    .slots
                    .get(index)
                    .copied()
                    .flatten()
                    .ok_or_else(|| "That record holder is no longer in your pack.".to_string())?;
                let at = self
                    .player
                    .pos
                    .block()
                    .ok_or_else(|| "Your position is outside the world.".to_string())?;
                self.server
                    .world
                    .bind_discovery_stack_at(at, &mut stack)
                    .map_err(|error| error.to_string())?;
                self.inventory.slots[index] = Some(stack);
                Ok(stack.arcane_id)
            }
            net::RecordHolderSnap::Folio { pos } => match self.server.world.block_entity_at(&pos) {
                Some(world::BlockEntity::SurveyFolio(folio)) if folio.object_id != 0 => {
                    Ok(folio.object_id)
                }
                _ => Err("That survey folio no longer has a record identity.".into()),
            },
        }
    }

    pub(super) fn copy_selected_discovery_record(&mut self) {
        let (Some(writing_pos), Some(source), Some(destination), Some(record_id)) = (
            self.ui_state.discovery_writing_pos,
            self.ui_state.discovery_holder,
            self.ui_state.discovery_copy_target,
            self.ui_state.discovery_selected[0],
        ) else {
            self.toast("Select a record at a writing surface with an adjacent folio.".into());
            return;
        };
        let at_surface = |holder: net::RecordHolderSnap| match holder {
            net::RecordHolderSnap::Inventory { .. } => true,
            net::RecordHolderSnap::Folio { pos } => [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .into_iter()
                .filter_map(|(du, dv)| writing_pos.offset(du, 0, dv))
                .any(|adjacent| adjacent == pos),
        };
        if !at_surface(source) || !at_surface(destination) {
            self.toast("The placed folio must remain beside the writing surface.".into());
            return;
        }
        if let Some(remote) = &self.multiplayer.remote {
            remote.client.send(&net::C2S::CopyObservation {
                writing_pos,
                source,
                record_id,
                destination,
                include_location: self.ui_state.discovery_include_location,
            });
            return;
        }
        let writing_valid = self
            .content
            .reg
            .block(self.server.world.get_block_at(writing_pos))
            .discovery_fixture
            .as_ref()
            .is_some_and(|fixture| fixture.kind == "writing_surface");
        if !writing_valid {
            self.toast("The writing surface is no longer there.".into());
            return;
        }
        let source_id = match self.local_discovery_holder_id(source) {
            Ok(id) => id,
            Err(error) => {
                self.toast(error);
                return;
            }
        };
        let destination_id = match self.local_discovery_holder_id(destination) {
            Ok(id) => id,
            Err(error) => {
                self.toast(error);
                return;
            }
        };
        match self.server.world.copy_discovery_record(
            source_id,
            record_id,
            destination_id,
            self.ui_state.discovery_include_location,
        ) {
            Ok(_) => {
                self.toast(if self.ui_state.discovery_include_location {
                    "Copied the signed observation with its location.".into()
                } else {
                    "Copied the signed observation with its location withheld.".into()
                });
                if let Err(error) = self.server.world.save_discovery() {
                    self.toast(format!("The copied record could not be saved: {error}"));
                }
            }
            Err(error) => self.toast(error.to_string()),
        }
    }

    pub(super) fn swap_discovery_copy_direction(&mut self) {
        let (Some(source), Some(destination)) = (
            self.ui_state.discovery_holder,
            self.ui_state.discovery_copy_target,
        ) else {
            self.toast("Use a writing surface beside a survey folio to exchange records.".into());
            return;
        };
        self.ui_state.discovery_holder = Some(destination);
        self.ui_state.discovery_copy_target = Some(source);
        self.ui_state.discovery_selected = [None; 2];
        self.ui_state.discovery_page = 0;
        if let Some(remote) = &self.multiplayer.remote {
            remote.client.send(&net::C2S::OpenDiscovery {
                holder: destination,
            });
            return;
        }
        let object_id = match self.local_discovery_holder_id(destination) {
            Ok(id) => id,
            Err(error) => {
                self.toast(error);
                return;
            }
        };
        match self.server.world.discovery_summaries(object_id, true) {
            Ok(records) => {
                self.ui_state.discovery_records = records;
                self.ui_state.discovery_capacity = match destination {
                    net::RecordHolderSnap::Inventory { .. } => {
                        crate::discovery::FIELD_LEDGER_RECORDS
                    }
                    net::RecordHolderSnap::Folio { .. } => crate::discovery::SURVEY_FOLIO_RECORDS,
                } as u16;
            }
            Err(error) => self.toast(error.to_string()),
        }
    }

    pub(super) fn read_held_knowledge(&mut self) {
        let slot = self.input.hotbar_sel;
        if let Some(remote) = &self.multiplayer.remote {
            self.ui_state.discovery_holder =
                Some(net::RecordHolderSnap::Inventory { slot: slot as u8 });
            self.ui_state.discovery_copy_target = None;
            self.ui_state.discovery_writing_pos = None;
            remote
                .client
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
        if let Some(text) = self.server.world.discovery_artifact_text(&mut stack, at) {
            self.inventory.slots[slot] = Some(stack);
            self.toast(text);
            self.sfx(Sfx::Click);
            return;
        }
        if self
            .server
            .world
            .bind_discovery_stack_at(at, &mut stack)
            .is_ok()
            && stack.arcane_id != 0
        {
            self.inventory.slots[slot] = Some(stack);
            match self.server.world.discovery_summaries(stack.arcane_id, true) {
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

    pub(super) fn present_discovery_record(
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

    pub(super) fn settle_discovery_reading(&mut self, aim: DiscoveryAim) {
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
            remote.client.send(&net::C2S::Observe {
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
        if let Err(error) = self.server.world.bind_discovery_stack_at(at, &mut ledger) {
            self.toast(error.to_string());
            return;
        }
        self.inventory.slots[ledger_slot] = Some(ledger);
        let calibration = calibration_slot
            .and_then(|slot| self.inventory.slots[slot])
            .and_then(|stack| self.server.world.calibration_grade_for(stack))
            .unwrap_or(crate::discovery::CalibrationGrade::Field);
        let player_id = identity::local_player_id(
            &self.server.world.save_dir_for_saving(),
            self.identity.device_id(),
        )
        .unwrap_or(identity::PlayerId([0; 16]));
        let target = match aim {
            DiscoveryAim::Region(pos) => world::ObservationTarget::Region(pos),
            DiscoveryAim::Block(pos) => world::ObservationTarget::Block(pos),
        };
        match self.server.world.record_observation(
            ledger.arcane_id,
            (player_id, &self.config.display_name),
            target,
            calibration,
            label,
            None,
        ) {
            Ok(record) => {
                let spent = self.server.world.wear_tuning_lens_at(
                    at,
                    &mut self.inventory,
                    self.input.hotbar_sel,
                );
                self.present_discovery_record(&record);
                if spent {
                    self.toast("The Wellglass clouds; the fitted frame and plate remain.".into());
                }
                if let Err(error) = self.server.world.save_discovery() {
                    self.toast(format!("The ledger could not be saved: {error}"));
                }
            }
            Err(error) => self.toast(error.to_string()),
        }
    }

    pub(super) fn settle_discovery_experiment(&mut self, pos: crate::planet::BlockPos) {
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
            remote.client.send(&net::C2S::RunExperiment {
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
        if let Err(error) = self.server.world.bind_discovery_stack_at(at, &mut ledger) {
            self.toast(error.to_string());
            return;
        }
        self.inventory.slots[ledger_slot] = Some(ledger);
        let sample = match self.server.world.experiment_sample_at(pos, kind) {
            Ok(sample) => sample,
            Err(error) => {
                self.toast(error);
                return;
            }
        };
        let calibration = calibration_slot
            .and_then(|slot| self.inventory.slots[slot])
            .and_then(|stack| self.server.world.calibration_grade_for(stack))
            .unwrap_or(crate::discovery::CalibrationGrade::Field);
        let player_id = identity::local_player_id(
            &self.server.world.save_dir_for_saving(),
            self.identity.device_id(),
        )
        .unwrap_or(identity::PlayerId([0; 16]));
        match self.server.world.record_observation(
            ledger.arcane_id,
            (player_id, &self.config.display_name),
            world::ObservationTarget::Item(sample, pos),
            calibration,
            Some(kind.label().into()),
            Some(kind),
        ) {
            Ok(record) => {
                let spent = self.server.world.wear_tuning_lens_at(
                    at,
                    &mut self.inventory,
                    self.input.hotbar_sel,
                );
                self.present_discovery_record(&record);
                if spent {
                    self.toast("The Wellglass clouds; the fitted frame and plate remain.".into());
                }
                if let Err(error) = self.server.world.save_discovery() {
                    self.toast(format!("The experiment record could not be saved: {error}"));
                }
            }
            Err(error) => self.toast(error.to_string()),
        }
    }

    pub(super) fn open_discovery_folio(&mut self, pos: crate::planet::BlockPos) {
        if let Some(remote) = &self.multiplayer.remote {
            self.ui_state.discovery_holder = Some(net::RecordHolderSnap::Folio { pos });
            self.ui_state.discovery_copy_target = None;
            self.ui_state.discovery_writing_pos = None;
            remote.client.send(&net::C2S::OpenDiscovery {
                holder: net::RecordHolderSnap::Folio { pos },
            });
            return;
        }
        let object_id = match self.server.world.block_entity_at(&pos) {
            Some(world::BlockEntity::SurveyFolio(folio)) if folio.object_id != 0 => folio.object_id,
            _ => {
                self.toast("This folio has no recoverable record identity.".into());
                return;
            }
        };
        match self.server.world.discovery_library_index(object_id, true) {
            Ok(index) => {
                self.open_discovery_catalogue(
                    net::RecordHolderSnap::Folio { pos },
                    index.records,
                    crate::discovery::SURVEY_FOLIO_RECORDS as u16,
                    None,
                    None,
                );
            }
            Err(error) => self.toast(error.to_string()),
        }
    }

    pub(super) fn assemble_tuning_lens(&mut self, pos: crate::planet::BlockPos) {
        if let Some(remote) = &self.multiplayer.remote {
            remote.client.send(&net::C2S::AssembleTuningLens { pos });
            return;
        }
        match self
            .server
            .world
            .assemble_tuning_lens_at(pos, &mut self.inventory)
        {
            Ok(_) => {
                self.toast("The Wellglass settles against the Echo Slate plate.".into());
                self.sfx(Sfx::Craft);
            }
            Err(error) => self.toast(error),
        }
    }

    pub(super) fn exchange_discovery_apparatus_item(&mut self, pos: crate::planet::BlockPos) {
        let slot = self.input.hotbar_sel;
        if let Some(kind) = self.inventory.slots[slot]
            .and_then(|stack| self.content.reg.item(stack.item).discovery.as_ref())
            .and_then(|definition| definition.experiment)
            && let Some(index) = crate::discovery::ExperimentKind::ALL
                .iter()
                .position(|candidate| *candidate == kind)
        {
            self.interaction.experiment_kind = index;
        }
        if let Some(remote) = &self.multiplayer.remote {
            remote.client.send(&net::C2S::SetExperimentItem {
                pos,
                slot: slot as u8,
            });
            return;
        }
        match self
            .server
            .world
            .exchange_experiment_item_at(pos, &mut self.inventory, slot)
        {
            Ok(message) => self.toast(message),
            Err(error) => self.toast(error),
        }
    }

    pub(super) fn operate_binding_frame(&mut self, pos: crate::planet::BlockPos) {
        let slot = self.input.hotbar_sel;
        let expected_revision = self.interaction.binding_revisions.get(&pos).copied();
        if let Some(remote) = &self.multiplayer.remote {
            remote.client.send(&net::C2S::OperateBindingFrame {
                pos,
                slot: slot as u8,
                action: crate::implements::FrameAction::Contextual,
                expected_revision,
            });
            return;
        }
        match self.server.world.operate_binding_frame(
            pos,
            &mut self.inventory,
            slot,
            crate::implements::FrameAction::Contextual,
            expected_revision,
            "local-player",
        ) {
            Ok(result) => {
                self.interaction
                    .binding_revisions
                    .insert(pos, result.revision);
                self.presentation.swing = 1.0;
                self.toast(result.message);
                for line in result.lines.into_iter().take(3) {
                    self.toast(line);
                }
                self.sfx(match result.cue {
                    crate::implements::ImplementCue::Use => Sfx::ImplementUse,
                    crate::implements::ImplementCue::Transfer => Sfx::ImplementTransfer,
                    crate::implements::ImplementCue::Strain => Sfx::ImplementStrain,
                    crate::implements::ImplementCue::Empty => Sfx::ImplementEmpty,
                    crate::implements::ImplementCue::Failure => Sfx::ImplementFailure,
                });
            }
            Err(error) => {
                self.sfx(match crate::implements::error_cue(&error) {
                    crate::implements::ImplementCue::Empty => Sfx::ImplementEmpty,
                    _ => Sfx::ImplementFailure,
                });
                self.toast(error);
            }
        }
    }

    pub(super) fn copy_at_writing_surface(&mut self, writing_pos: crate::planet::BlockPos) {
        let held_slot = self.input.hotbar_sel;
        let Some(mut held) = self.inventory.slots[held_slot] else {
            self.toast("Hold a field ledger at the writing surface.".into());
            return;
        };
        let held_is_holder = self
            .content
            .reg
            .item(held.item)
            .discovery
            .as_ref()
            .is_some_and(|definition| {
                matches!(definition.kind.as_str(), "field_ledger" | "survey_folio")
            });
        if !held_is_holder {
            self.toast("Hold a field ledger at the writing surface.".into());
            return;
        }
        let adjacent_folio = [(1, 0), (-1, 0), (0, 1), (0, -1)]
            .into_iter()
            .filter_map(|(du, dv)| writing_pos.offset(du, 0, dv))
            .find(|pos| {
                self.content
                    .reg
                    .block(self.server.world.get_block_at(*pos))
                    .discovery_fixture
                    .as_ref()
                    .is_some_and(|fixture| fixture.kind == "survey_folio")
            });
        let Some(folio_pos) = adjacent_folio else {
            self.toast("Place a survey folio beside the writing surface.".into());
            return;
        };
        let source = net::RecordHolderSnap::Inventory {
            slot: held_slot as u8,
        };
        let destination = net::RecordHolderSnap::Folio { pos: folio_pos };
        if let Some(remote) = &self.multiplayer.remote {
            self.ui_state.discovery_holder = Some(source);
            self.ui_state.discovery_copy_target = Some(destination);
            self.ui_state.discovery_writing_pos = Some(writing_pos);
            remote
                .client
                .send(&net::C2S::OpenDiscovery { holder: source });
            return;
        }
        let Some(at) = self.player.pos.block() else {
            return;
        };
        if let Err(error) = self.server.world.bind_discovery_stack_at(at, &mut held) {
            self.toast(error.to_string());
            return;
        }
        self.inventory.slots[held_slot] = Some(held);
        match self.server.world.block_entity_at(&folio_pos) {
            Some(world::BlockEntity::SurveyFolio(folio)) if folio.object_id != 0 => {}
            _ => {
                self.toast("The adjacent folio has no record identity.".into());
                return;
            }
        }
        let held_records = self
            .server
            .world
            .discovery_summaries(held.arcane_id, true)
            .unwrap_or_default();
        self.open_discovery_catalogue(
            source,
            held_records,
            crate::discovery::FIELD_LEDGER_RECORDS as u16,
            Some(destination),
            Some(writing_pos),
        );
    }

    /// The attunement sidecar for the current world (local knowledge —
    /// what this player's feet have actually touched).
    fn attune_path(&self) -> std::path::PathBuf {
        self.server.world.save_dir_for_saving().join("attuned.tsv")
    }

    pub(super) fn load_attunements(&mut self) {
        self.interaction.attuned.clear();
        if let Ok(text) = std::fs::read_to_string(self.attune_path()) {
            let mut lines = text.lines();
            if lines.next() != Some("version\t2") {
                return; // old planar knowledge is deliberately not reinterpreted
            }
            for line in lines {
                let mut parts = line.splitn(4, '\t');
                if let (Some(face), Some(u), Some(v), Some(name)) = (
                    parts.next().and_then(crate::planet::Face::from_name),
                    parts.next().and_then(|value| value.parse().ok()),
                    parts.next().and_then(|value| value.parse().ok()),
                    parts.next(),
                ) && let Ok(surface) = crate::planet::SurfacePos::new(face, u, v)
                {
                    self.interaction.attuned.push((name.to_string(), surface));
                }
            }
        }
    }

    fn save_attunements(&self) -> std::io::Result<()> {
        use std::fmt::Write as _;
        let mut out = String::from("version\t2\n");
        for (name, surface) in &self.interaction.attuned {
            let _ = writeln!(
                out,
                "{}\t{}\t{}\t{name}",
                surface.face(),
                surface.u(),
                surface.v()
            );
        }
        crate::persist::atomic_write(&self.attune_path(), out.as_bytes(), false)
    }

    /// Touch a waystone: learn it, then hear where the others stand.
    pub(super) fn read_waystone(&mut self, pos: crate::planet::BlockPos) {
        let surface = pos.surface();
        let name = match self.server.world.block_entity_at(&pos) {
            Some(world::BlockEntity::Sign(sg)) if !sg.lines[0].is_empty() => sg.lines[0].clone(),
            _ => {
                let atlas_name =
                    self.server.world.planet_atlas().and_then(|atlas| {
                        atlas.hydrological_name_at(surface).map(ToOwned::to_owned)
                    });
                let Some(atlas_name) = atlas_name else {
                    self.toast("The stone is unnamed. Write it first.".to_string());
                    return;
                };
                atlas_name
            }
        };
        let known = self
            .interaction
            .attuned
            .iter()
            .any(|(_, known)| *known == surface);
        if !known {
            self.interaction.attuned.push((name.clone(), surface));
            match self.save_attunements() {
                Ok(()) => self.toast(format!("The stone at {name} knows you now.")),
                Err(error) => {
                    self.interaction.attuned.pop();
                    self.toast(format!("The stone could not remember you: {error}"));
                }
            }
        }
        let mut lines: Vec<String> = Vec::new();
        for (other, other_surface) in &self.interaction.attuned {
            if *other_surface == surface {
                continue;
            }
            let from = surface.center();
            let to = other_surface.center();
            let distance = crate::planet::geodesic_distance(from, to).round() as i32;
            let direction = crate::planet::great_circle_bearing(from, to).map(|bearing| {
                const NAMES: [&str; 8] = [
                    "north",
                    "northeast",
                    "east",
                    "southeast",
                    "south",
                    "southwest",
                    "west",
                    "northwest",
                ];
                let octant = ((bearing.to_degrees() + 22.5).rem_euclid(360.0) / 45.0) as usize;
                NAMES[octant]
            });
            lines.push(match direction {
                Some(direction) => {
                    format!("{}: ~{distance} blocks {direction}", other.to_uppercase())
                }
                None => format!(
                    "{}: ~{distance} blocks; bearing uncertain",
                    other.to_uppercase()
                ),
            });
        }
        if lines.is_empty() {
            self.toast("It hums alone. Touch other stones.".to_string());
        }
        for l in lines {
            self.toast(l);
        }
    }

    /// Bedroll: sleep to dawn if it's night and the wild is far enough.
    /// In multiplayer, dawn waits for everyone (the sleep vote).
    pub(super) fn try_sleep(&mut self) {
        let sun = (self.server.time_of_day * std::f32::consts::TAU).sin();
        if sun > -0.05 {
            self.toast("You can only sleep at night.".to_string());
            return;
        }
        if let Some(r) = &mut self.multiplayer.remote {
            r.client.send(&net::C2S::SleepRequest);
            r.sleeping = true;
            self.toast("You settle in, waiting for the others... (move to get up)".to_string());
            return;
        }
        if self
            .multiplayer
            .host
            .as_ref()
            .is_some_and(|h| h.guests.values().any(|guest| guest.is_active()))
        {
            self.multiplayer.host_sleeping = true;
            self.survival.spawn_point = self.player.pos;
            self.toast("You settle in, waiting for the others... (move to get up)".to_string());
            return;
        }
        let reg = self.content.reg.clone();
        let near_warden = self.server.world.mobs().iter().any(|m| {
            reg.animals.get(m.species).is_some_and(|d| d.hostile)
                && (m.pos - self.player.pos).length_squared() < 24.0 * 24.0
        });
        if near_warden {
            self.toast("The wild is too close.".to_string());
            return;
        }
        // Time passes fairly: the skipped night still decays ire.
        let skipped = (1.0 + 0.3 - self.server.time_of_day) % 1.0;
        if self.server.world.tick_ire(skipped) {
            let r = self.server.world.accept_offerings();
            if r > 0.0 {
                self.toast("The wild has accepted your offering.".to_string());
            }
        }
        self.server.sleep_to_dawn();
        self.survival.spawn_point = self.player.pos;
        if !self.creative {
            self.inventory.wear_tool(&reg, self.input.hotbar_sel);
        }
        match self.save_session() {
            Ok(_) => self.toast("You camp until dawn. This is home now.".to_string()),
            Err(error) => {
                eprintln!("world: camp save incomplete: {error}");
                self.toast(format!("You wake, but the camp could not save: {error}"));
            }
        }
        self.sfx(Sfx::Craft);
    }
}
