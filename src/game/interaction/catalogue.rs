//! Catalogue interaction adapter.

use crate::world::TerrainRead;
use crate::identity;
use crate::net;
use crate::world;
use crate::game::Game;
use crate::game::navigation::Screen;

impl Game {
    pub(in crate::game) fn open_discovery_catalogue(
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

    pub(in crate::game) fn receive_discovery_catalogue(
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

    pub(in crate::game) fn local_discovery_holder_id(&mut self, holder: net::RecordHolderSnap) -> Result<u64, String> {
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
                self.runtime.local_mut().world.bind_discovery_stack_at(at, &mut stack)
                    .map_err(|error| error.to_string())?;
                self.inventory.slots[index] = Some(stack);
                Ok(stack.arcane_id)
            }
            net::RecordHolderSnap::Folio { pos } => match self.runtime.view().block_entity_at(&pos) {
                Some(world::BlockEntity::SurveyFolio(folio)) if folio.object_id != 0 => {
                    Ok(folio.object_id)
                }
                _ => Err("That survey folio no longer has a record identity.".into()),
            },
        }
    }

    pub(in crate::game) fn copy_selected_discovery_record(&mut self) {
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
            remote.session.send(&net::C2S::CopyObservation {
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
            .block(self.runtime.view().get_block_at(writing_pos))
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
        match self.runtime.local_mut().world.copy_discovery_record(
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
                if let Err(error) = self.runtime.local_mut().world.save_discovery() {
                    self.toast(format!("The copied record could not be saved: {error}"));
                }
            }
            Err(error) => self.toast(error.to_string()),
        }
    }

    pub(in crate::game) fn swap_discovery_copy_direction(&mut self) {
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
            remote.session.send(&net::C2S::OpenDiscovery {
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
        match self.runtime.local().world.discovery_summaries(object_id, true) {
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

    pub(in crate::game) fn copy_at_writing_surface(&mut self, writing_pos: crate::planet::BlockPos) {
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
                    .block(self.runtime.view().get_block_at(*pos))
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
                .session
                .send(&net::C2S::OpenDiscovery { holder: source });
            return;
        }
        let Some(at) = self.player.pos.block() else {
            return;
        };
        if let Err(error) = self.runtime.local_mut().world.bind_discovery_stack_at(at, &mut held) {
            self.toast(error.to_string());
            return;
        }
        self.inventory.slots[held_slot] = Some(held);
        match self.runtime.view().block_entity_at(&folio_pos) {
            Some(world::BlockEntity::SurveyFolio(folio)) if folio.object_id != 0 => {}
            _ => {
                self.toast("The adjacent folio has no record identity.".into());
                return;
            }
        }
        let held_records = self.runtime.local().world.discovery_summaries(held.arcane_id, true)
            .unwrap_or_default();
        self.open_discovery_catalogue(
            source,
            held_records,
            crate::discovery::FIELD_LEDGER_RECORDS as u16,
            Some(destination),
            Some(writing_pos),
        );
    }
}
