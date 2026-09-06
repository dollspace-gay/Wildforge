//! Replies for the authoritative host session.

use super::{BlockEntity, BlockPos, HostSession, ItemStack, MachineHandler, PlayerRuntime, S2C, Server, StackSnap};

impl HostSession {

    /// Gameplay broadcasts exclude authenticated connections that are still
    /// decoding their entry terrain. They do not yet have a coherent world
    /// mirror and are not members of the active roster.
    pub(super) fn broadcast_ready(&self, msg: &S2C) {
        for (id, guest) in &self.guests {
            if guest.entry_ready {
                self.net.send(*id, msg);
            }
        }
    }

    pub fn broadcast_working_cue(&self, cue: crate::workings::WorkingCue) {
        self.broadcast_ready(&S2C::WorkingEvent(cue));
    }

    pub fn broadcast_alchemy_cue(&self, cue: crate::alchemy::AlchemyCue) {
        self.broadcast_ready(&S2C::AlchemyEvent(cue));
    }

    pub fn broadcast_dross_cue(&self, world: &crate::world::World, cue: crate::dross::DrossCue) {
        let Some(atlas) = world.planet_atlas() else {
            return;
        };
        for (id, guest) in &self.guests {
            if guest.entry_ready && atlas.atlas_pos(guest.pos.surface()) == cue.region {
                self.net.send(*id, &S2C::DrossEvent(cue));
            }
        }
    }

    pub(super) fn send_player_state(&self, id: u32) {
        if let Some(guest) = self.guests.get(&id) {
            self.net.send(
                id,
                &S2C::PlayerState(PlayerRuntime::from_guest(guest).to_snap()),
            );
        }
    }

    /// Apply one guest click with the cursor stack it sent, exactly as
    /// local play would, then echo the container and the new cursor.
    pub(super) fn container_click(
        &mut self,
        server: &mut Server,
        id: u32,
        pos: BlockPos,
        slot: usize,
        right: bool,
    ) {
        let reg = server.world.reg.clone();
        let Some(guest) = self.guests.get(&id).filter(|guest| guest.container == Some(pos)) else {
            return;
        };
        let actor = guest.player_id.0;
        let mut held = guest.cursor;
        let Some(entity) = server.world.block_entity_mut_at(&pos) else { return; };
        let result = crate::player_ops::container::click(
            &reg, entity, &mut held,
            crate::player_ops::container::Click { slot, right, actor: Some(actor) },
        );
        // Depots retain their deposit-only request path. Other rejected clicks
        // still receive the unchanged cursor/container echo, as before.
        if result == Err(crate::player_ops::container::Rejected::DepositOnly) { return; }
        let snap = held.map(|s| StackSnap {
            item: s.item.0,
            count: s.count,
            durability: s.durability,
            arcane_id: s.arcane_id,
            current_units: 0,
        });
        if let Some(guest) = self.guests.get_mut(&id) {
            guest.cursor = held;
        }
        self.net.send(id, &S2C::HeldResult(snap));
        self.send_player_state(id);
        self.send_container(server, id, pos);
    }

    pub fn broadcast_sign_at(&mut self, pos: BlockPos, lines: &[String; 3]) {
        self.broadcast_ready(&S2C::SignText {
            pos,
            lines: lines.clone(),
        });
    }

    pub(super) fn send_mob_cargo(&mut self, server: &Server, id: u32, mob_id: u32) {
        let Some(slots) = server
            .world
            .mob_by_id(mob_id)
            .and_then(|m| m.cargo.as_deref())
        else {
            return;
        };
        let slots = slots
            .iter()
            .map(|s| {
                s.map(|s| StackSnap {
                    item: s.item.0,
                    count: s.count,
                    durability: s.durability,
                    arcane_id: s.arcane_id,
                    current_units: server
                        .world
                        .inspectable_item_current(s.arcane_id)
                        .unwrap_or(0),
                })
            })
            .collect();
        self.net.send(id, &S2C::MobCargo { id: mob_id, slots });
    }

    pub(super) fn send_container(&mut self, server: &Server, id: u32, pos: BlockPos) {
        let Some(entity) = server.world.block_entity_at(&pos) else {
            return;
        };
        let snap = |s: &Option<ItemStack>| {
            s.map(|s| StackSnap {
                item: s.item.0,
                count: s.count,
                durability: s.durability,
                arcane_id: s.arcane_id,
                current_units: server
                    .world
                    .inspectable_item_current(s.arcane_id)
                    .unwrap_or(0),
            })
        };
        let reg = server.world.reg.clone();
        // Capability E7: machine kinds ride `S2C::MachineContainer`, keyed
        // by the machine id (host and guest remap by id like the palette).
        // The layout comes from the def, so any data-driven kind works.
        if let BlockEntity::Multiblock(b) = entity {
            let Some(def) = reg.machine(b.kind) else {
                return;
            };
            let fire = def.fire_secs.max(1.0);
            let (slots, aux) = match def.handler {
                MachineHandler::Kiln => (
                    b.charge
                        .iter()
                        .chain([&b.reagent])
                        .chain(b.fuel.iter())
                        .map(snap)
                        .collect(),
                    vec![if b.lit { 1.0 } else { 0.0 }, b.progress / fire],
                ),
                MachineHandler::Workbench => (Vec::new(), Vec::new()),
                _ => (
                    b.charge.iter().chain(b.fuel.iter()).map(snap).collect(),
                    vec![if b.lit { 1.0 } else { 0.0 }, b.progress / fire],
                ),
            };
            self.net.send(
                id,
                &S2C::MachineContainer {
                    pos,
                    machine: def.id.clone(),
                    slots,
                    aux,
                },
            );
            return;
        }
        let (kind, slots, aux): (u8, Vec<Option<StackSnap>>, Vec<f32>) = match entity {
            // Depots have no container screen; deposits go through the
            // interaction arm and C2S::DepotDeposit.
            BlockEntity::Depot(_) => return,
            BlockEntity::Chest(c) => (0, c.slots.iter().map(snap).collect(), Vec::new()),
            BlockEntity::Furnace(f) => (
                1,
                vec![snap(&f.input), snap(&f.fuel), snap(&f.output)],
                vec![f.progress, f.burn_left, f.burn_total],
            ),
            BlockEntity::Offering(o) => (2, o.slots.iter().map(snap).collect(), Vec::new()),
            BlockEntity::Stall(st) => {
                let owner = self
                    .guests
                    .get(&id)
                    .is_some_and(|g| g.player_id.0 == st.owner);
                let mut slots: Vec<Option<StackSnap>> = st.goods.iter().map(snap).collect();
                slots.push(snap(&st.price));
                for t in st.till.iter() {
                    slots.push(if owner { snap(t) } else { None });
                }
                (6, slots, vec![if owner { 1.0 } else { 0.0 }])
            }
            BlockEntity::Clamp(_)
            | BlockEntity::Anvil(_)
            | BlockEntity::Sign(_)
            | BlockEntity::Smoker(_)
            | BlockEntity::Steam(_)
            | BlockEntity::Multiblock(_)
            | BlockEntity::SurveyFolio(_)
            | BlockEntity::DiscoveryApparatus(_)
            | BlockEntity::BindingFrame(_)
            | BlockEntity::ChargeVessel(_)
            | BlockEntity::Switch(_) => return,
        };
        self.net.send(
            id,
            &S2C::Container {
                pos,
                kind,
                slots,
                aux,
            },
        );
    }
}
