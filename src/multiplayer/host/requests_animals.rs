//! Authenticated animals request adapter.

use super::{
    C2S, HostSession, ItemStack, REACH, S2C, Server, StackSnap, Vec3, click_stack, net,
    refresh_held, take_item,
};

impl HostSession {
    pub(super) fn request_animals(&mut self, server: &mut Server, id: u32, msg: C2S) {
        let Some(guest) = self.guests.get_mut(&id) else {
            return;
        };
        match msg {
            C2S::AttackMob { id: mob_id, heavy } => {
                // Stable ids: snapshots lag the sim, so an index would
                // race deaths/spawns and strike the wrong creature.
                if guest.action_cooldown > 0.0 {
                    return;
                }
                guest.action_cooldown = 0.35;
                let held = guest.inventory.slots[guest.hotbar];
                let dmg = held
                    .map(|stack| server.world.reg.item(stack.item).damage)
                    .unwrap_or(1.0)
                    .clamp(0.0, 16.0);
                let dmg_type =
                    held.and_then(|stack| server.world.reg.item(stack.item).damage_type.clone());
                let from = guest
                    .pos
                    .translated(Vec3::new(0.0, 1.6, 0.0))
                    .expect("guest attack origin stays in the shell")
                    .pos;
                let gpos = guest.pos;
                let reg = server.world.reg.clone();
                if let Some(m) = server.world.mob_by_id_mut(mob_id)
                    && m.pos.distance_to(gpos) <= REACH
                {
                    let def = reg.animals[m.species].clone();
                    let surface = m.pos.surface();
                    let backstab = crate::player_ops::combat::mob_facing_away(m.yaw, m.pos, gpos);
                    let damage = crate::player_ops::combat::melee_damage(dmg, heavy, backstab);
                    let (final_dmg, crit) = (damage.amount, damage.critical);
                    m.hurt(&def, final_dmg, dmg_type.as_deref(), from);
                    m.last_hit_by = id;
                    if !def.hostile {
                        server.world.add_ire_at_surface(surface, 2.0);
                    }
                    if server.world.mode != "creative" {
                        guest.hunger = (guest.hunger - 0.01).max(0.0);
                        guest.inventory.wear_tool(&reg, guest.hotbar);
                        refresh_held(guest);
                        self.send_player_state(id);
                    }
                    // Report the authoritative number for the guest's
                    // floating damage feedback.
                    self.net.send(
                        id,
                        &net::S2C::MobHit {
                            id: mob_id,
                            dmg: final_dmg,
                            crit,
                        },
                    );
                }
            }
            C2S::FeedMob { id: mob_id } => {
                let gpos = guest.pos;
                let reg = server.world.reg.clone();
                if let Some(m) = server.world.mob_by_id_mut(mob_id) {
                    let def = &reg.animals[m.species];
                    if (m.pos - gpos).length() <= REACH
                        && let Some(feeding) = crate::player_ops::feeding::FeedPlan::prepare(
                            def,
                            m,
                            guest.inventory.slots[guest.hotbar].map(|stack| stack.item),
                        )
                    {
                        feeding.apply(m);
                        if server.world.mode != "creative" {
                            let consumed = guest.inventory.slots[guest.hotbar]
                                .map(|stack| ItemStack::new(&reg, stack.item, 1));
                            if let Some(stack) = consumed
                                && let Err(error) = server.world.record_consumed_stacks([stack])
                            {
                                eprintln!(
                                    "materials: guest animal feed accounting failed: {error}"
                                );
                            }
                            guest.inventory.take_one(guest.hotbar);
                            refresh_held(guest);
                            self.send_player_state(id);
                        }
                    }
                }
            }
            C2S::HackMob { id: mob_id } => {
                let gpos = guest.pos;
                let reg = server.world.reg.clone();
                let held = guest.inventory.slots[guest.hotbar].map(|s| s.item);
                if let Some(m) = server.world.mob_by_id(mob_id)
                    && (m.pos - gpos).length() <= REACH
                    && let Some(def) = reg.animals.get(m.species)
                    && let Some(hack) = &def.hack
                    && let Some(tool) = hack.tool.as_deref()
                    && held.is_some_and(|i| {
                        let item = reg.item(i);
                        match tool {
                            "hack" => item.hack,
                            other => item.name.ends_with(&format!(":{other}")),
                        }
                    })
                {
                    if let Some(index) = server.world.mobs().iter().position(|x| x.id == mob_id) {
                        server.world.hack_mob(index, &mut server.rng);
                    }
                    guest.action_cooldown = 0.5;
                }
            }
            C2S::LeadMob { id: mob_id } => {
                let gpos = guest.pos;
                let lead = server.world.reg.item_id("base:lead");
                let holding = guest.inventory.slots[guest.hotbar].map(|s| s.item);
                if let Some(m) = server.world.mob_by_id_mut(mob_id)
                    && m.tamed
                    && m.led_by.is_none()
                    && (m.pos - gpos).length() <= REACH
                    && holding == lead
                {
                    m.led_by = Some(id);
                    if server.world.mode != "creative"
                        && let Some(lead) = lead
                    {
                        take_item(&mut guest.inventory, lead);
                        refresh_held(guest);
                    }
                } else if let Some(m) = server.world.mob_by_id_mut(mob_id)
                    && m.led_by == Some(id)
                {
                    // Second use releases; the strip comes back.
                    m.led_by = None;
                    if let Some(lead) = lead {
                        let reg = server.world.reg.clone();
                        let _ = guest.inventory.add(&reg, lead, 1);
                        refresh_held(guest);
                    }
                }
            }
            C2S::SaddleMob { id: mob_id } => {
                let gpos = guest.pos;
                let reg = server.world.reg.clone();
                let bags = reg.item_id("base:saddlebags");
                let holding = guest.inventory.slots[guest.hotbar].map(|s| s.item);
                if let Some(m) = server.world.mob_by_id_mut(mob_id)
                    && m.tamed
                    && reg.animals[m.species].carrier
                    && m.cargo.is_none()
                    && (m.pos - gpos).length() <= REACH
                    && holding == bags
                {
                    m.cargo = Some(Default::default());
                    if server.world.mode != "creative"
                        && let Some(bags) = bags
                    {
                        take_item(&mut guest.inventory, bags);
                        refresh_held(guest);
                    }
                }
            }
            C2S::OpenMobCargo { id: mob_id } => {
                let gpos = guest.pos;
                if let Some(m) = server.world.mob_by_id(mob_id)
                    && m.tamed
                    && m.cargo.is_some()
                    && (m.pos - gpos).length() <= REACH
                {
                    guest.mob_cargo = Some(mob_id);
                    self.send_mob_cargo(server, id, mob_id);
                }
            }
            C2S::MobCargoClick {
                id: mob_id,
                slot,
                right,
            } => {
                if guest.mob_cargo != Some(mob_id) || slot >= 12 {
                    return;
                }
                let reg = server.world.reg.clone();
                let mut held = guest.cursor;
                if let Some(m) = server.world.mob_by_id_mut(mob_id)
                    && let Some(cargo) = m.cargo.as_mut()
                {
                    let (ns, nh) = click_stack(&reg, cargo[slot as usize], held, right);
                    cargo[slot as usize] = ns;
                    held = nh;
                }
                let snap = held.map(|s| StackSnap {
                    item: s.item.0,
                    count: s.count,
                    durability: s.durability,
                    arcane_id: s.arcane_id,
                    current_units: 0,
                });
                if let Some(g) = self.guests.get_mut(&id) {
                    g.cursor = held;
                }
                self.net.send(id, &S2C::HeldResult(snap));
                self.send_mob_cargo(server, id, mob_id);
            }
            C2S::RideMob { id: mob_id, mount } => {
                let gpos = guest.pos;
                let reg = server.world.reg.clone();
                if let Some(m) = server.world.mob_by_id_mut(mob_id)
                    && reg.animals[m.species].vehicle
                {
                    if mount && m.ridden_by.is_none() && (m.pos - gpos).length() <= REACH {
                        m.ridden_by = Some(id);
                    } else if !mount && m.ridden_by == Some(id) {
                        m.ridden_by = None;
                    }
                }
            }

            _ => {}
        }
    }
}
