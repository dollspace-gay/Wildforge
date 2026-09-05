//! Authenticated field observation request adapter.

use super::{C2S, HostSession, Instant, PendingDiscovery, PendingDiscoveryKind, REACH, S2C, Server, discovery_calibration, discovery_holder_id, discovery_reachable, net, refresh_held};

impl HostSession {
    pub(super) fn request_field_observation(&mut self, server: &mut Server, id: u32, msg: C2S) {
        let Some(guest) = self.guests.get_mut(&id) else { return; };
        match msg {
            C2S::BrushBlock { pos } => {
                if guest.pos.distance_to(pos.entity_center()) > REACH {
                    return;
                }
                let b = server.world.get_block_at(pos);
                let archaeology = server.world.reg.block(b).brush.is_some();
                if !archaeology && !server.world.can_sift_salvage_at(pos) {
                    return;
                }
                if !guest.inventory.slots[guest.hotbar]
                    .is_some_and(|stack| server.world.reg.item(stack.item).brush_tool)
                {
                    return;
                }
                let found = if archaeology {
                    let mut r = server.rng;
                    let found = server.world.brush_block_at(pos, &mut r);
                    server.rng = r;
                    found
                } else {
                    match server.world.sift_salvage_at(pos) {
                        Ok(found) => found,
                        Err(error) => {
                            eprintln!("materials: guest regional salvage recovery failed: {error}");
                            None
                        }
                    }
                };
                if let Some(stack) = found {
                    server.world.queue_give(id, stack);
                    if !archaeology {
                        self.net.send(
                            id,
                            &S2C::Toast("The brush turns up usable buried stock.".into()),
                        );
                    }
                } else if !archaeology {
                    self.net.send(
                        id,
                        &S2C::Toast("Nothing recoverable gathers in this ground yet.".into()),
                    );
                }
                if server.world.mode != "creative" {
                    guest.inventory.wear_tool(&server.world.reg, guest.hotbar);
                    refresh_held(guest);
                    self.send_player_state(id);
                }
            }
            C2S::BeginObserve { target } => {
                guest.pending_discovery = None;
                let lens_ready = guest.action_cooldown <= 0.0
                    && guest.inventory.slots[guest.hotbar].is_some_and(|stack| {
                        server
                            .world
                            .reg
                            .item(stack.item)
                            .discovery
                            .as_ref()
                            .is_some_and(|definition| definition.kind == "tuning_lens")
                            && stack.arcane_id != 0
                    });
                let target_ready = match target {
                    net::DiscoveryTargetSnap::Region => guest.pos.block().is_some(),
                    net::DiscoveryTargetSnap::Block(pos) => {
                        discovery_reachable(&server.world, guest, pos)
                    }
                    net::DiscoveryTargetSnap::Held { slot } => guest
                        .inventory
                        .slots
                        .get(usize::from(slot))
                        .is_some_and(Option::is_some),
                };
                if lens_ready && target_ready {
                    guest.pending_discovery = Some(PendingDiscovery {
                        kind: PendingDiscoveryKind::Observation(target),
                        began: Instant::now(),
                    });
                } else {
                    self.net.send(
                        id,
                        &S2C::Toast("The tuning lens cannot begin settling on that target.".into()),
                    );
                }
            }
            C2S::Observe {
                target,
                ledger_slot,
                calibration_slot,
                label,
            } => {
                let settled = guest.pending_discovery.is_some_and(|pending| {
                    pending.settled_for(PendingDiscoveryKind::Observation(target), Instant::now())
                });
                guest.pending_discovery = None;
                if guest.action_cooldown > 0.0 || !settled {
                    self.net.send(
                        id,
                        &S2C::Toast(
                            "The reading was refused: hold the lens steady until it settles."
                                .into(),
                        ),
                    );
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
                let holder = match discovery_holder_id(
                    &mut server.world,
                    guest,
                    net::RecordHolderSnap::Inventory { slot: ledger_slot },
                ) {
                    Ok(holder) => holder,
                    Err(error) => {
                        self.net.send(id, &S2C::Toast(error));
                        return;
                    }
                };
                let calibration =
                    match discovery_calibration(&mut server.world, guest, calibration_slot) {
                        Ok(calibration) => calibration,
                        Err(error) => {
                            self.net.send(id, &S2C::Toast(error));
                            return;
                        }
                    };
                let measured = match target {
                    net::DiscoveryTargetSnap::Region => guest
                        .pos
                        .block()
                        .map(crate::world::ObservationTarget::Region),
                    net::DiscoveryTargetSnap::Block(pos) => {
                        discovery_reachable(&server.world, guest, pos)
                            .then_some(crate::world::ObservationTarget::Block(pos))
                    }
                    net::DiscoveryTargetSnap::Held { slot } => guest
                        .inventory
                        .slots
                        .get(usize::from(slot))
                        .copied()
                        .flatten()
                        .zip(guest.pos.block())
                        .map(|(stack, at)| crate::world::ObservationTarget::Item(stack, at)),
                };
                let Some(measured) = measured else {
                    self.net.send(
                        id,
                        &S2C::Toast("The target is not physically measurable from here.".into()),
                    );
                    return;
                };
                match server.world.record_observation(
                    holder,
                    (guest.player_id, &guest.name),
                    measured,
                    calibration,
                    label,
                    None,
                ) {
                    Ok(summary) => {
                        guest.action_cooldown = 1.25;
                        let at = guest.pos.block().unwrap_or(measured.position());
                        let spent =
                            server
                                .world
                                .wear_tuning_lens_at(at, &mut guest.inventory, lens_slot);
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
                    Err(error) => self
                        .net
                        .send(id, &S2C::Toast(format!("Observation refused: {error}"))),
                }
            }

            _ => {}
        }
    }
}
