//! Authenticated knowledge request adapter.

use super::{
    C2S, HostSession, S2C, Server, discovery_holder_at_writing_surface, discovery_holder_capacity,
    discovery_holder_id, discovery_reachable, net,
};

impl HostSession {
    pub(super) fn request_knowledge(&mut self, server: &mut Server, id: u32, msg: C2S) {
        let Some(guest) = self.guests.get_mut(&id) else {
            return;
        };
        match msg {
            C2S::ReadKnowledge { slot } => {
                let index = usize::from(slot);
                let Some(mut stack) = guest.inventory.slots.get(index).copied().flatten() else {
                    return;
                };
                let Some(at) = guest.pos.block() else {
                    return;
                };
                if let Err(error) = server.world.bind_discovery_stack_at(at, &mut stack) {
                    self.net.send(id, &S2C::Toast(error.to_string()));
                    return;
                }
                guest.inventory.slots[index] = Some(stack);
                if let Some(text) = server.world.discovery_artifact_text(&mut stack, at) {
                    guest.inventory.slots[index] = Some(stack);
                    self.net.send(
                        id,
                        &S2C::KnowledgeText {
                            instance_id: stack.arcane_id,
                            text,
                        },
                    );
                } else if let Ok(records) = server.world.discovery_summaries(stack.arcane_id, true)
                {
                    let capacity = if server
                        .world
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
                    self.net.send(
                        id,
                        &S2C::DiscoveryRecords {
                            holder: net::RecordHolderSnap::Inventory { slot },
                            records,
                            capacity: capacity as u16,
                        },
                    );
                }
                self.send_player_state(id);
            }
            C2S::OpenDiscovery { holder } => {
                match discovery_holder_id(&mut server.world, guest, holder) {
                    Ok(object_id) => match server.world.discovery_summaries(object_id, true) {
                        Ok(records) => {
                            let capacity =
                                discovery_holder_capacity(&server.world, guest, holder) as u16;
                            self.net.send(
                                id,
                                &S2C::DiscoveryRecords {
                                    holder,
                                    records,
                                    capacity,
                                },
                            );
                        }
                        Err(error) => self.net.send(id, &S2C::Toast(error.to_string())),
                    },
                    Err(error) => self.net.send(id, &S2C::Toast(error)),
                }
            }
            C2S::CopyObservation {
                writing_pos,
                source,
                record_id,
                destination,
                include_location,
            } => {
                let writing_surface = discovery_reachable(&server.world, guest, writing_pos)
                    && server
                        .world
                        .reg
                        .block(server.world.get_block_at(writing_pos))
                        .discovery_fixture
                        .as_ref()
                        .is_some_and(|fixture| fixture.kind == "writing_surface")
                    && discovery_holder_at_writing_surface(source, writing_pos)
                    && discovery_holder_at_writing_surface(destination, writing_pos);
                if !writing_surface {
                    self.net.send(
                        id,
                        &S2C::Toast(
                            "Signed records can only be copied at a writing surface; placed folios must be adjacent."
                                .into(),
                        ),
                    );
                    return;
                }
                let source_id = match discovery_holder_id(&mut server.world, guest, source) {
                    Ok(value) => value,
                    Err(error) => {
                        self.net.send(id, &S2C::Toast(error));
                        return;
                    }
                };
                let destination_id =
                    match discovery_holder_id(&mut server.world, guest, destination) {
                        Ok(value) => value,
                        Err(error) => {
                            self.net.send(id, &S2C::Toast(error));
                            return;
                        }
                    };
                match server.world.copy_discovery_record(
                    source_id,
                    record_id,
                    destination_id,
                    include_location,
                ) {
                    Ok(_) => {
                        let _ = server.world.save_discovery();
                        self.net.send(id, &S2C::Toast("Observation copied.".into()));
                        if let Ok(records) = server.world.discovery_summaries(destination_id, true)
                        {
                            let capacity =
                                discovery_holder_capacity(&server.world, guest, destination) as u16;
                            self.net.send(
                                id,
                                &S2C::DiscoveryRecords {
                                    holder: destination,
                                    records,
                                    capacity,
                                },
                            );
                        }
                    }
                    Err(error) => self.net.send(id, &S2C::Toast(error.to_string())),
                }
            }

            _ => {}
        }
    }
}
