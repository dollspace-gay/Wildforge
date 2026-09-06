//! Authenticated world use request adapter.

use super::{
    BlockEntity, C2S, HostFx, HostSession, PlayerRuntime, REACH, S2C, Server, refresh_held,
};

impl HostSession {
    pub(super) fn request_world_use(
        &mut self,
        server: &mut Server,
        id: u32,
        msg: C2S,
        fx: &mut Vec<HostFx>,
    ) {
        let Some(guest) = self.guests.get_mut(&id) else {
            return;
        };
        match msg {
            C2S::StallBuy { pos } => {
                if guest.pos.distance_to(pos.entity_center()) > REACH
                    || !server.world.check_stall_at(pos)
                {
                    return;
                }
                let reg = server.world.reg.clone();
                // A banned seller's stall stops trading (their goods
                // stay theirs).
                let banned_owner = |owner: [u8; 16]| {
                    self.moderation
                        .as_ref()
                        .is_some_and(|m| m.player_banned(crate::identity::PlayerId(owner)))
                };
                let Some(BlockEntity::Stall(stall)) = server.world.block_entity_mut_at(&pos) else {
                    return;
                };
                if stall.owner == [0; 16] || banned_owner(stall.owner) {
                    return;
                }
                let Ok(purchase) =
                    crate::player_ops::trade::purchase(&reg, stall, &mut guest.inventory)
                else {
                    return;
                };
                if let Some(stack) = purchase.overflow
                    && let Some(at) = guest.pos.block()
                {
                    server.world.push_drop_at(at, stack);
                }
                refresh_held(guest);
                self.send_player_state(id);
                self.send_container(server, id, pos);
            }
            C2S::SetSign { pos, lines } => {
                if guest.pos.distance_to(pos.entity_center()) > REACH {
                    return;
                }
                let b = server.world.get_block_at(pos);
                let station = server.world.reg.block(b).interaction.as_deref();
                if !matches!(station, Some("sign") | Some("waystone")) {
                    return;
                }
                let mut lines = lines;
                for l in lines.iter_mut() {
                    l.truncate(14);
                    l.retain(|c| c.is_ascii_alphanumeric() || " :_-'".contains(c));
                }
                server.world.insert_block_entity_at(
                    pos,
                    BlockEntity::Sign(crate::world::SignState {
                        lines: lines.clone(),
                    }),
                );
                self.broadcast_ready(&S2C::SignText { pos, lines });
            }
            C2S::DepotDeposit { pos } => {
                // Capability E13: a guest delivers held goods to a depot.
                // The host validates the need against its own registry,
                // consumes from the guest's inventory, and pays the
                // reputation through HostFx (the KV namespace lives in the
                // windowed host).
                if guest.pos.distance_to(pos.entity_center()) > REACH {
                    return;
                }
                let interaction = server
                    .world
                    .reg
                    .block(server.world.get_block_at(pos))
                    .interaction
                    .clone();
                let Some(interaction) = interaction else {
                    return;
                };
                if !interaction.starts_with("depot:") {
                    return;
                }
                let Some(held) = guest.inventory.slots[guest.hotbar] else {
                    return;
                };
                let Some((settlement, item, units, rep_per_unit)) =
                    server
                        .world
                        .deliver_to_depot(pos, &mut guest.inventory, guest.hotbar)
                else {
                    self.net.send(
                        id,
                        &S2C::Toast(format!(
                            "The depot has no appetite for {} right now.",
                            server.world.reg.item(held.item).label
                        )),
                    );
                    return;
                };
                refresh_held(guest);
                self.send_player_state(id);
                self.net.send(
                    id,
                    &S2C::SettlementDelivery {
                        settlement,
                        item: server.world.reg.item(item).name.clone(),
                        units,
                        rep_per_unit,
                    },
                );
            }
            C2S::ScreenClick { screen, action } => {
                // Capability E11: a mod-screen button click. Both ids are
                // validated against the host's own registry, so a tampered
                // client can only ever name buttons that exist. Scripts
                // live on the windowed host, so the click rides HostFx.
                let Some(def) = server.world.reg.screens.iter().find(|s| s.id == screen) else {
                    return;
                };
                if !def.rows().iter().any(
                    |row| matches!(row, crate::screens::ScreenWidget::Button { action: a, .. } if a == &action),
                ) {
                    return;
                }
                fx.push(HostFx::ScreenClick { screen, action });
            }
            C2S::ToggleSwitch { pos } => {
                if guest.pos.distance_to(pos.entity_center()) > REACH {
                    return;
                }
                let b = server.world.get_block_at(pos);
                let interaction = server.world.reg.block(b).interaction.as_deref();
                if !matches!(interaction, Some("rail_switch") | Some("belt_switch")) {
                    return;
                }
                server.world.toggle_switch(pos);
                let selected = server
                    .world
                    .switch_selected(pos)
                    .unwrap_or(crate::planet::Direction4::North);
                self.broadcast_ready(&S2C::SwitchState {
                    pos,
                    selected: selected as u8,
                });
            }
            C2S::DungeonUse { pos, kind } => {
                if guest.pos.distance_to(pos.entity_center()) > REACH {
                    return;
                }
                let interaction = server
                    .world
                    .reg
                    .block(server.world.get_block_at(pos))
                    .interaction
                    .clone();
                match (kind, interaction.as_deref()) {
                    // Entry: stand the guest at their run's spawn point.
                    (0, Some(s)) if s.starts_with("dungeon_entry:") => {
                        let name = s.trim_start_matches("dungeon_entry:").to_string();
                        if let Some(spawn) = server.world.enter_dungeon(id, guest.pos, &name) {
                            if let Some(g) = self.guests.get_mut(&id) {
                                g.pos = spawn;
                                self.net.send(
                                    id,
                                    &S2C::PlayerState(PlayerRuntime::from_guest(g).to_snap()),
                                );
                            }
                            self.net.send(
                                id,
                                &S2C::Toast("The dark takes you. The door is behind you.".into()),
                            );
                        }
                    }
                    // Exit: route back to the participant's own door.
                    (1, Some("dungeon_exit")) => {
                        if let Some(back) = server.world.exit_dungeon(id, guest.pos) {
                            if let Some(g) = self.guests.get_mut(&id) {
                                g.pos = back;
                                self.net.send(
                                    id,
                                    &S2C::PlayerState(PlayerRuntime::from_guest(g).to_snap()),
                                );
                            }
                            self.net.send(
                                id,
                                &S2C::Toast("Daylight again. The deep forgets you.".into()),
                            );
                        }
                    }
                    // Checkpoint: party-shared, host-owned.
                    (2, Some("dungeon_checkpoint")) => {
                        server.world.set_dungeon_checkpoint(guest.pos);
                        self.net
                            .send(id, &S2C::Toast("The shrine remembers you.".into()));
                    }
                    _ => {}
                }
            }

            _ => {}
        }
    }
}
