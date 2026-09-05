//! Authenticated experiments request adapter.

use super::{C2S, HostSession, Instant, PendingDiscovery, PendingDiscoveryKind, S2C, Server, discovery_calibration, discovery_holder_id, discovery_reachable, net, refresh_held};

impl HostSession {
    pub(super) fn request_experiments(&mut self, server: &mut Server, id: u32, msg: C2S) {
        let Some(guest) = self.guests.get_mut(&id) else { return; };
        match msg {
            C2S::BeginExperiment { pos, kind } => {
                guest.pending_discovery = None;
                let fixture_ready = guest.action_cooldown <= 0.0
                    && discovery_reachable(&server.world, guest, pos)
                    && server
                        .world
                        .reg
                        .block(server.world.get_block_at(pos))
                        .discovery_fixture
                        .as_ref()
                        .is_some_and(|fixture| {
                            fixture.kind == "experiment_apparatus"
                                && fixture.experiments.contains(&kind)
                        });
                let lens_ready = guest.inventory.slots[guest.hotbar].is_some_and(|stack| {
                    server
                        .world
                        .reg
                        .item(stack.item)
                        .discovery
                        .as_ref()
                        .is_some_and(|definition| definition.kind == "tuning_lens")
                        && stack.arcane_id != 0
                });
                if fixture_ready && lens_ready {
                    guest.pending_discovery = Some(PendingDiscovery {
                        kind: PendingDiscoveryKind::Experiment(pos, kind),
                        began: Instant::now(),
                    });
                } else {
                    self.net.send(
                        id,
                        &S2C::Toast("The controlled trial cannot begin at that apparatus.".into()),
                    );
                }
            }
            C2S::SetExperimentItem { pos, slot } => {
                if guest.action_cooldown > 0.0 || !discovery_reachable(&server.world, guest, pos) {
                    return;
                }
                let index = usize::from(slot);
                match server
                    .world
                    .exchange_experiment_item_at(pos, &mut guest.inventory, index)
                {
                    Ok(message) => {
                        guest.action_cooldown = 0.25;
                        refresh_held(guest);
                        self.net.send(id, &S2C::Toast(message));
                        self.send_player_state(id);
                    }
                    Err(error) => self.net.send(id, &S2C::Toast(error)),
                }
            }
            C2S::RunExperiment {
                pos,
                kind,
                ledger_slot,
                calibration_slot,
            } => {
                let settled = guest.pending_discovery.is_some_and(|pending| {
                    pending.settled_for(PendingDiscoveryKind::Experiment(pos, kind), Instant::now())
                });
                guest.pending_discovery = None;
                if !settled
                    || guest.action_cooldown > 0.0
                    || !discovery_reachable(&server.world, guest, pos)
                    || !server
                        .world
                        .reg
                        .block(server.world.get_block_at(pos))
                        .discovery_fixture
                        .as_ref()
                        .is_some_and(|fixture| {
                            fixture.kind == "experiment_apparatus"
                                && fixture.experiments.contains(&kind)
                        })
                {
                    if !settled {
                        self.net.send(
                            id,
                            &S2C::Toast(
                                "The trial was refused: hold the apparatus steady until it settles."
                                    .into(),
                            ),
                        );
                    }
                    return;
                }
                let lens_slot = guest.hotbar;
                if !guest.inventory.slots[lens_slot].is_some_and(|stack| {
                    server
                        .world
                        .reg
                        .item(stack.item)
                        .discovery
                        .as_ref()
                        .is_some_and(|definition| definition.kind == "tuning_lens")
                        && stack.arcane_id != 0
                }) {
                    self.net
                        .send(id, &S2C::Toast("Hold a fitted tuning lens.".into()));
                    return;
                }
                let sample = match server.world.experiment_sample_at(pos, kind) {
                    Ok(sample) => sample,
                    Err(error) => {
                        self.net.send(id, &S2C::Toast(error));
                        return;
                    }
                };
                let holder = match discovery_holder_id(
                    &mut server.world,
                    guest,
                    net::RecordHolderSnap::Inventory { slot: ledger_slot },
                ) {
                    Ok(value) => value,
                    Err(error) => {
                        self.net.send(id, &S2C::Toast(error));
                        return;
                    }
                };
                let calibration =
                    match discovery_calibration(&mut server.world, guest, calibration_slot) {
                        Ok(value) => value,
                        Err(error) => {
                            self.net.send(id, &S2C::Toast(error));
                            return;
                        }
                    };
                match server.world.record_observation(
                    holder,
                    (guest.player_id, &guest.name),
                    crate::world::ObservationTarget::Item(sample, pos),
                    calibration,
                    Some(kind.label().into()),
                    Some(kind),
                ) {
                    Ok(summary) => {
                        guest.action_cooldown = 1.25;
                        let spent =
                            server
                                .world
                                .wear_tuning_lens_at(pos, &mut guest.inventory, lens_slot);
                        refresh_held(guest);
                        self.net.send(id, &S2C::DiscoveryReport(summary));
                        if spent {
                            self.net.send(
                                id,
                                &S2C::Toast(
                                    "The Wellglass element clouds; the fitted frame and plate remain."
                                        .into(),
                                ),
                            );
                        }
                        self.send_player_state(id);
                    }
                    Err(error) => self.net.send(id, &S2C::Toast(error.to_string())),
                }
            }
            C2S::AssembleTuningLens { pos } => {
                if !discovery_reachable(&server.world, guest, pos)
                    || server
                        .world
                        .reg
                        .block(server.world.get_block_at(pos))
                        .discovery_fixture
                        .as_ref()
                        .is_none_or(|fixture| fixture.kind != "lens_assembly")
                {
                    return;
                }
                match server
                    .world
                    .assemble_tuning_lens_at(pos, &mut guest.inventory)
                {
                    Ok(_) => {
                        refresh_held(guest);
                        self.net.send(
                            id,
                            &S2C::Toast(
                                "The Wellglass settles against the Echo Slate plate.".into(),
                            ),
                        );
                        self.send_player_state(id);
                    }
                    Err(error) => self.net.send(id, &S2C::Toast(error)),
                }
            }

            _ => {}
        }
    }
}
