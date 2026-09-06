//! Authenticated containers request adapter.

use super::{
    BlockEntity, C2S, HostSession, ItemStack, MachineInstance, REACH, S2C, Server, refresh_held,
};

impl HostSession {
    pub(super) fn request_containers(&mut self, server: &mut Server, id: u32, msg: C2S) {
        let Some(guest) = self.guests.get_mut(&id) else {
            return;
        };
        match msg {
            C2S::OpenContainer { pos } => {
                if guest.pos.distance_to(pos.entity_center()) > REACH {
                    return;
                }
                let b = server.world.get_block_at(pos);
                // Capability E7: machine interactions resolve to a
                // `MachineDef` and ride `S2C::MachineContainer`; the
                // classic containers keep their `S2C::Container` kind code.
                let machine =
                    server
                        .world
                        .reg
                        .block(b)
                        .interaction
                        .as_deref()
                        .and_then(|i| server.world.reg.machine_by_interaction(i))
                        .filter(|kind| {
                            server.world.reg.machine(*kind).is_some_and(|def| {
                                def.handler.has_fire() || def.handler.is_station()
                            })
                        });
                let kind = match (machine, server.world.reg.block(b).interaction.as_deref()) {
                    (Some(_), _) => 7u8,
                    (None, Some("chest")) => 0,
                    (None, Some("furnace")) => 1,
                    (None, Some("offering")) => 2,
                    (None, Some("stall")) => 6,
                    _ => return,
                };
                let default = if let Some(mkind) = machine {
                    BlockEntity::Multiblock(MachineInstance {
                        kind: mkind,
                        ..Default::default()
                    })
                } else {
                    match kind {
                        0 => BlockEntity::Chest(Default::default()),
                        1 => BlockEntity::Furnace(Default::default()),
                        6 => BlockEntity::Stall(Default::default()),
                        _ => BlockEntity::Offering(Default::default()),
                    }
                };
                let entry = server.world.ensure_block_entity_at(pos, default);
                // A fresh counter belongs to whoever opens it first.
                if let BlockEntity::Stall(st) = entry
                    && st.owner == [0; 16]
                {
                    st.owner = guest.player_id.0;
                    st.owner_name = guest.name.clone();
                }
                if let BlockEntity::Chest(c) = entry
                    && c.wild_owned
                {
                    c.wild_owned = false;
                    server.world.add_ire_at_surface(pos.surface(), 1.0);
                    self.net
                        .send(id, &S2C::Toast("The wild keeps its trophies.".into()));
                }
                if let Some(g) = self.guests.get_mut(&id) {
                    g.container = Some(pos);
                }
                self.send_container(server, id, pos);
            }
            C2S::ContainerClick { pos, slot, right } => {
                self.container_click(server, id, pos, slot as usize, right);
            }
            C2S::CloseContainer => {
                guest.container = None;
                guest.mob_cargo = None;
            }
            C2S::LightBloomery { pos } => {
                if guest.pos.distance_to(pos.entity_center()) > REACH {
                    return;
                }
                let ember = server.world.reg.item_id("base:ember");
                if server.world.mode != "creative"
                    && !guest
                        .inventory
                        .slots
                        .iter()
                        .flatten()
                        .any(|stack| Some(stack.item) == ember)
                {
                    return;
                }
                let b = server.world.get_block_at(pos);
                // Capability E7: light any fire handler's machine by its
                // interaction, not a hardcoded three-way match.
                let res = match server
                    .world
                    .reg
                    .block(b)
                    .interaction
                    .as_deref()
                    .and_then(|interaction| server.world.reg.machine_by_interaction(interaction))
                    .filter(|kind| {
                        server
                            .world
                            .reg
                            .machine(*kind)
                            .is_some_and(|def| def.handler.has_fire())
                    }) {
                    Some(kind) => {
                        let matched = match kind.validate(&server.world, pos) {
                            Some(matched) => matched,
                            None => {
                                self.net
                                    .send(id, &S2C::Toast("the stack is breached".into()));
                                return;
                            }
                        };
                        crate::world::machines::light_machine_at(
                            &mut server.world,
                            pos,
                            kind,
                            matched,
                        )
                    }
                    None => return,
                };
                match res {
                    Ok(()) => {
                        if server.world.mode != "creative"
                            && let Some(ember) = server.world.reg.item_id("base:ember")
                            && let Some(slot) =
                                guest.inventory.slots.iter().position(|stack| {
                                    stack.is_some_and(|stack| stack.item == ember)
                                })
                            && let Some(stack) = guest.inventory.slots[slot]
                        {
                            guest.inventory.take_one(slot);
                            server.world.retire_arcane_stack_at(
                                pos,
                                ItemStack { count: 1, ..stack },
                                "high-heat station ignition",
                            );
                            refresh_held(guest);
                            self.send_player_state(id);
                        }
                        self.send_container(server, id, pos);
                    }
                    Err(e) => self.net.send(id, &S2C::Toast(e.into())),
                }
            }
            C2S::LightClamp { pos } => {
                if guest.pos.distance_to(pos.entity_center()) > REACH {
                    return;
                }
                if server.world.mode != "creative"
                    && guest.inventory.slots[guest.hotbar].map(|stack| stack.item)
                        != server.world.reg.item_id("base:ember")
                {
                    return;
                }
                match server.world.try_light_clamp_at(pos) {
                    Ok(n) => {
                        if server.world.mode != "creative" {
                            let consumed = guest.inventory.slots[guest.hotbar];
                            guest.inventory.take_one(guest.hotbar);
                            if let Some(stack) = consumed {
                                server.world.retire_arcane_stack_at(
                                    pos,
                                    ItemStack { count: 1, ..stack },
                                    "clamp ignition",
                                );
                            }
                            refresh_held(guest);
                            self.send_player_state(id);
                        }
                        self.net
                            .send(id, &S2C::Toast(format!("The clamp smolders ({n} logs).")));
                    }
                    Err(e) => self.net.send(id, &S2C::Toast(e.into())),
                }
            }
            C2S::AnvilPut { pos } => {
                if guest.pos.distance_to(pos.entity_center()) > REACH {
                    return;
                }
                let Some(stack) = guest.inventory.slots[guest.hotbar] else {
                    return;
                };
                let one = ItemStack { count: 1, ..stack };
                if server.world.anvil_put_at(pos, one) && server.world.mode != "creative" {
                    guest.inventory.take_one(guest.hotbar);
                    refresh_held(guest);
                    self.send_player_state(id);
                }
            }
            C2S::AnvilStrike { pos } => {
                if guest.pos.distance_to(pos.entity_center()) > REACH {
                    return;
                }
                if guest.action_cooldown > 0.0 {
                    return;
                }
                guest.action_cooldown = 0.35;
                let has_hammer = guest.inventory.slots[guest.hotbar]
                    .is_some_and(|stack| server.world.reg.item(stack.item).hammer);
                if !has_hammer && server.world.mode != "creative" {
                    return;
                }
                if has_hammer && server.world.mode != "creative" {
                    guest.inventory.wear_tool(&server.world.reg, guest.hotbar);
                    refresh_held(guest);
                }
                if let Some(out) = server.world.anvil_strike_at(pos) {
                    server.world.queue_give(id, out);
                }
                self.send_player_state(id);
            }
            C2S::AnvilTake { pos } => {
                if guest.pos.distance_to(pos.entity_center()) > REACH {
                    return;
                }
                if let Some(b) = server.world.anvil_take_at(pos) {
                    server.world.queue_give(id, b);
                }
            }

            _ => {}
        }
    }
}
