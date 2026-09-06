//! Containers graphical guest adapter.

use super::RemoteFlow;
use crate::inventory::ItemStack;
use crate::net;
use crate::world;
use crate::game::Remote;
use crate::game::navigation::Screen;

impl Game {
    pub(in crate::game) fn remote_containers_message(&mut self, r: &mut Remote, message: net::S2C) -> RemoteFlow {
        match message {
                net::S2C::MobCargo { id, slots } => {
                    // The host's pack truth: mirror it onto the local
                    // snapshot mob and open the screen if we asked.
                    let mut cargo: Box<[Option<ItemStack>; 12]> = Default::default();
                    for (i, sl) in slots.iter().enumerate().take(12) {
                        cargo[i] = sl.as_ref().map(|sn| ItemStack {
                            item: crate::registry::ItemId(sn.item),
                            count: sn.count,
                            durability: sn.durability,
                            arcane_id: sn.arcane_id,
                        });
                    }
                    if let Some((world, _)) = self.runtime.guest_mut()
                        && let Some(m) = world.mob_by_id_mut(id) {
                        m.cargo = Some(cargo);
                    }
                    if matches!(self.ui_state.screen, Screen::Playing) {
                        self.set_screen(Screen::MobCargo(id));
                    }
                }
                net::S2C::Container {
                    pos,
                    kind,
                    slots,
                    aux,
                } => {
                    let conv = |s: &Option<net::StackSnap>| -> Option<ItemStack> {
                        s.as_ref()
                            .and_then(|stack| r.session.content().stack(stack))
                    };
                    let entity = match kind {
                        0 => {
                            let mut c = world::ChestState::default();
                            for (i, s) in slots.iter().enumerate().take(world::CHEST_SLOTS) {
                                c.slots[i] = conv(s);
                            }
                            world::BlockEntity::Chest(c)
                        }
                        1 => {
                            let f = world::FurnaceState {
                                input: slots.first().and_then(&conv),
                                fuel: slots.get(1).and_then(&conv),
                                output: slots.get(2).and_then(&conv),
                                progress: aux.first().copied().unwrap_or(0.0),
                                burn_left: aux.get(1).copied().unwrap_or(0.0),
                                burn_total: aux.get(2).copied().unwrap_or(0.0),
                                ..Default::default()
                            };
                            world::BlockEntity::Furnace(f)
                        }
                        6 => {
                            let mut st = world::StallState::default();
                            for (i, sl) in slots.iter().enumerate().take(13) {
                                let stk = conv(sl);
                                match i {
                                    0..=5 => st.goods[i] = stk,
                                    6 => st.price = stk,
                                    _ => st.till[i - 7] = stk,
                                }
                            }
                            // aux[0] carries "you own this" — marked
                            // with a sentinel owner so the UI knows.
                            if aux.first().copied().unwrap_or(0.0) > 0.5 {
                                st.owner = [1; 16];
                            }
                            world::BlockEntity::Stall(st)
                        }
                        _ => {
                            let mut o = world::OfferingState::default();
                            for (i, s) in slots.iter().enumerate().take(3) {
                                o.slots[i] = conv(s);
                            }
                            world::BlockEntity::Offering(o)
                        }
                    };
                    if let Some((world, _)) = self.runtime.guest_mut() {
                        world.receive_block_entity(pos, entity);
                    }
                    if matches!(self.ui_state.screen, Screen::Playing) {
                        self.set_screen(match kind {
                            0 => Screen::Chest(pos),
                            1 => Screen::Furnace(pos),
                            6 => Screen::Stall(pos),
                            _ => Screen::Offering(pos),
                        });
                    }
                }
                net::S2C::MachineContainer {
                    pos,
                    machine,
                    slots,
                    aux,
                } => {
                    // Capability E7: the machine id remaps like the item
                    // palette; the handler's layout rebuilds the instance.
                    let reg = self.content.reg.clone();
                    let conv = |s: &Option<net::StackSnap>| -> Option<ItemStack> {
                        s.as_ref()
                            .and_then(|stack| r.session.content().stack(stack))
                    };
                    let Some(kind) = reg.machine_kind(&machine) else {
                        return RemoteFlow::Abort;
                    };
                    let Some(def) = reg.machine(kind) else {
                        return RemoteFlow::Abort;
                    };
                    let handler = def.handler;
                    let mut m = world::MachineInstance {
                        kind,
                        lit: aux.first().copied().unwrap_or(0.0) > 0.5,
                        progress: aux.get(1).copied().unwrap_or(0.0) * def.fire_secs,
                        ..Default::default()
                    };
                    match handler {
                        crate::machines::MachineHandler::Kiln => {
                            for (i, sl) in slots.iter().enumerate().take(9) {
                                let st = conv(sl);
                                match i {
                                    0..=3 => m.charge[i] = st,
                                    4 => m.reagent = st,
                                    _ => m.fuel[i - 5] = st,
                                }
                            }
                        }
                        crate::machines::MachineHandler::Workbench => {}
                        _ => {
                            for (i, s) in slots.iter().enumerate().take(8) {
                                if i < 4 {
                                    m.charge[i] = conv(s);
                                } else {
                                    m.fuel[i - 4] = conv(s);
                                }
                            }
                        }
                    }
                    if let Some((world, _)) = self.runtime.guest_mut() {
                        world.receive_block_entity(pos, world::BlockEntity::Multiblock(m));
                    }
                    if matches!(self.ui_state.screen, Screen::Playing) {
                        self.set_screen(match handler {
                            crate::machines::MachineHandler::Kiln => Screen::Kiln(pos),
                            crate::machines::MachineHandler::Workbench => Screen::Workbench(pos),
                            _ => Screen::Bloomery(pos),
                        });
                    }
                }
                net::S2C::HeldResult(held) => {
                    // The authoritative cursor after our click replaces
                    // the local prediction (identical on agreement).
                    self.ui_state.held_stack = held
                        .as_ref()
                        .and_then(|stack| r.session.content().stack(stack));
                }
            _ => {}
        }
        RemoteFlow::Continue
    }
}
