//! Held use in the ordered graphical action pipeline.

use super::ActionFrame;
use crate::audio::Sfx;
use crate::game::Game;
use crate::game::navigation::Screen;
use crate::inventory::ItemStack;
use crate::mobs;
use crate::net;
use crate::raycast;
use crate::world;
use crate::world::TerrainRead;

impl Game {
    pub(in crate::game) fn interact_held_use(&mut self, frame: &ActionFrame) -> bool {
        let reg = &frame.reg;
        let hit = &frame.hit;
        let aim = &frame.aim;
        let held = frame.held;
        // Right click: interact with the targeted block (crafting table),
        // otherwise place the selected block.
        // Feeding wildlife: right-click an adult with its favorite food.
        if self.input.right_held && self.input.action_cooldown <= 0.0 {
            // Structure hit: place the held block at the adjacent cell.
            // In-structure interaction (machines, containers) is deferred
            // this phase — right-clicking a structure always places.
            if let Some(raycast::TargetHit::Structure { id, adjacent, .. }) = &aim {
                let sid = *id;
                let off = *adjacent;
                let place = self.inventory.slots[self.input.hotbar_sel]
                    .and_then(|s| reg.item(s.item).places);
                if let Some(block) = place
                    && (self.creative || self.inventory.slots[self.input.hotbar_sel].is_some())
                {
                    let placed = self
                        .runtime
                        .local_mut()
                        .world
                        .local_structure_mut(sid)
                        .map(|s| s.place_block(off, block))
                        .unwrap_or(false);
                    if placed {
                        if !self.creative {
                            self.inventory.take_one(self.input.hotbar_sel);
                        }
                        self.input.action_cooldown = 0.22;
                        self.sfx(Sfx::Place);
                    }
                }
                return true;
            }
            if let Some(mi) = self.mob_in_crosshair(hit) {
                let Some(mob) = self.runtime.view().mob(mi) else {
                    return true;
                };
                let (sp, mob_id) = (mob.species, mob.id);
                let (tamed, led_by, has_cargo) = (mob.tamed, mob.led_by, mob.cargo.is_some());
                let def = &reg.animals[sp];
                let def_carrier = def.carrier;
                let def_label = def.label.clone();
                // Talking: right-clicking a friendly NPC opens its dialogue
                // tree (spec 3.2) instead of the animal interactions below.
                // NPCs never feed/tame/cargo/ride.
                if reg.is_npc_species(sp) {
                    let root = self
                        .runtime
                        .view()
                        .npc_by_mob(mob_id)
                        .and_then(|npc| npc.dialogue.clone())
                        .and_then(|d| reg.dialogues.iter().find(|dd| dd.id == d).cloned())
                        .map(|dd| dd.root)
                        .unwrap_or_default();
                    self.input.action_cooldown = 0.3;
                    self.set_screen(Screen::Dialog {
                        npc: mob_id,
                        node_id: root,
                        choice_sel: 0,
                    });
                    return true;
                }
                // Hacking: right-clicking a construct with a tagged tool
                // disables it instead of destroying it (spec 3.6).
                // Destroying one yields its scrap `drops`; hacking it
                // yields the core. The hack arm sits next to dialogue,
                // before feeding/taming ever runs.
                if def.hack.is_some()
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
                    if let Some(rc) = &self.multiplayer.remote {
                        rc.session.send(&net::C2S::HackMob { id: mob_id });
                    } else {
                        let server = self.runtime.local_mut();
                        server.world.hack_mob(mi, &mut server.rng);
                    }
                    self.input.action_cooldown = 0.5;
                    self.sfx(Sfx::Place);
                    self.toast(format!("The {def_label} goes still."));
                    return true;
                }
                // Feeding: breeds as ever, and repeated meals TAME —
                // a tamed animal never flees people and takes a lead.
                let feeding =
                    self.runtime.view().mob_by_id(mob_id).and_then(|mob| {
                        crate::player_ops::feeding::FeedPlan::prepare(def, mob, held)
                    });
                if let Some(feeding) = feeding
                    && (self.creative || self.inventory.take_one(self.input.hotbar_sel).is_some())
                {
                    // Guests request; local change is the
                    // prediction until the snapshot echoes it.
                    if let Some(rc) = &self.multiplayer.remote {
                        rc.session.send(&net::C2S::FeedMob { id: mob_id });
                    } else if !self.creative
                        && let Err(error) = self
                            .runtime
                            .local_mut()
                            .world
                            .record_consumed_stacks([ItemStack::new(reg, feeding.food(), 1)])
                    {
                        eprintln!("materials: animal feed accounting failed: {error}");
                    }
                    let now_tamed = self.runtime.present_mob_feeding(mob_id, feeding);
                    if now_tamed {
                        self.toast(format!("The {def_label} trusts you now."));
                    }
                    self.input.action_cooldown = 0.4;
                    self.sfx(Sfx::Pickup);
                    return true;
                }
                // The lead: attach to a tamed animal, click again to
                // release (the strip comes back).
                if tamed && led_by == Some(0) && self.multiplayer.remote.is_none() {
                    if let Some(mob) = self.runtime.local_mut().world.mob_mut(mi) {
                        mob.led_by = None;
                    }
                    if let Some(lead) = reg.item_id("base:lead") {
                        let left = self.inventory.add(reg, lead, 1);
                        if left > 0 {
                            self.drop_stack(ItemStack::new(reg, lead, 1));
                        }
                    }
                    self.input.action_cooldown = 0.4;
                    self.sfx(Sfx::Click);
                    return true;
                }
                if tamed
                    && led_by.is_none()
                    && held == reg.item_id("base:lead")
                    && (self.creative || self.inventory.take_one(self.input.hotbar_sel).is_some())
                {
                    if let Some(rc) = &self.multiplayer.remote {
                        rc.session.send(&net::C2S::LeadMob { id: mob_id });
                    } else if let Some(mob) = self.runtime.local_mut().world.mob_mut(mi) {
                        mob.led_by = Some(0);
                    }
                    self.input.action_cooldown = 0.4;
                    self.sfx(Sfx::Click);
                    return true;
                }
                // Saddlebags: a tamed carrier takes a pack.
                if tamed
                    && def_carrier
                    && !has_cargo
                    && held == reg.item_id("base:saddlebags")
                    && (self.creative || self.inventory.take_one(self.input.hotbar_sel).is_some())
                {
                    if let Some(rc) = &self.multiplayer.remote {
                        rc.session.send(&net::C2S::SaddleMob { id: mob_id });
                    } else if let Some(mob) = self.runtime.local_mut().world.mob_mut(mi) {
                        mob.cargo = Some(Default::default());
                    }
                    self.input.action_cooldown = 0.4;
                    self.sfx(Sfx::Place);
                    return true;
                }
                // Step aboard a vehicle (empty-handed).
                if def.vehicle && held.is_none() {
                    let free = self
                        .runtime
                        .view()
                        .mob_by_id(mob_id)
                        .is_some_and(|m| m.ridden_by.is_none());
                    if free {
                        if let Some(rc) = &self.multiplayer.remote {
                            rc.session.send(&net::C2S::RideMob {
                                id: mob_id,
                                mount: true,
                            });
                        } else if let Some(m) = self.runtime.local_mut().world.mob_by_id_mut(mob_id)
                        {
                            m.ridden_by = Some(0);
                        }
                        self.interaction.riding = Some(mob_id);
                        self.toast("Aboard. Jump to step off.".to_string());
                        self.input.action_cooldown = 0.4;
                        return true;
                    }
                }
                // Open the pack.
                if tamed && has_cargo {
                    if let Some(rc) = &self.multiplayer.remote {
                        rc.session.send(&net::C2S::OpenMobCargo { id: mob_id });
                    } else {
                        self.set_screen(Screen::MobCargo(mob_id));
                    }
                    self.input.action_cooldown = 0.3;
                    return true;
                }
            }
            // A covered log pile takes a warden's ember: the clamp.
            let ember = reg.item_id("base:ember");
            if held == ember
                && let Some(hb) = &hit
            {
                let pos = hb.block;
                let tb = self.runtime.view().get_block_at(pos);
                let is_log = reg.tags.get("base:logs").is_some_and(|l| {
                    reg.item_id(&reg.block(tb).name)
                        .is_some_and(|i| l.contains(&i))
                });
                if is_log {
                    if let Some(rc) = &self.multiplayer.remote {
                        self.inventory.take_one(self.input.hotbar_sel);
                        rc.session.send(&net::C2S::LightClamp { pos });
                    } else {
                        match self.runtime.local_mut().world.try_light_clamp_at(pos) {
                            Ok(n) => {
                                let consumed = self.inventory.slots[self.input.hotbar_sel];
                                self.inventory.take_one(self.input.hotbar_sel);
                                if !self.creative
                                    && let Some(stack) = consumed
                                {
                                    self.runtime.local_mut().world.retire_arcane_stack_at(
                                        pos,
                                        ItemStack { count: 1, ..stack },
                                        "clamp ignition",
                                    );
                                }
                                self.sfx(Sfx::Bolt(0.8));
                                self.toast(format!(
                                    "The clamp smolders - {n} logs, {:.0} minutes.",
                                    n as f32 * world::CLAMP_SECS_PER_LOG / 60.0
                                ));
                            }
                            Err(e) => self.toast(e.to_string()),
                        }
                    }
                    self.input.action_cooldown = 0.5;
                    return true;
                }
            }
            // The prospector's pick: strike bare rock, read the country.
            if held == reg.item_id("base:prospect_pick")
                && let Some(hb) = &hit
            {
                let pos = hb.block;
                let tb = self.runtime.view().get_block_at(pos);
                if reg.is_solid(tb) {
                    self.toast_prospect(pos.surface());
                    if !self.creative {
                        self.inventory.wear_tool(reg, self.input.hotbar_sel);
                    }
                    self.sfx(Sfx::Bolt(1.2));
                    self.input.action_cooldown = 0.8;
                    return true;
                }
            }
            // Throwables are loosed from the hand. A preparation remains a
            // stable physical vessel in flight even in creative mode; it may
            // never be cloned or discarded as a cosmetic projectile.
            if let Some(speed) = held.and_then(|i| reg.item(i).throw_speed) {
                let item = held.unwrap();
                let selected = self.inventory.slots[self.input.hotbar_sel];
                let state_bearing = selected.is_some_and(|stack| stack.arcane_id != 0);
                let removed = if self.creative && !state_bearing {
                    None
                } else {
                    self.inventory.take_one_stack(self.input.hotbar_sel)
                };
                if state_bearing && removed.is_none() {
                    return true;
                }
                let dir = self.camera.local_forward();
                if let Some(rc) = &self.multiplayer.remote {
                    rc.session.send(&net::C2S::FireProjectile {
                        direction: dir,
                        charge: 1.0,
                    });
                } else {
                    let pos = self
                        .player
                        .eye()
                        .translated(dir * 0.4)
                        .expect("throwing muzzle stays near the player")
                        .pos;
                    let vel = dir * speed;
                    let tile = reg.item(item).icon;
                    self.runtime
                        .local_mut()
                        .world
                        .spawn_projectile(mobs::Projectile {
                            stable_id: 0,
                            pos,
                            vel,
                            tile,
                            damage: 0.0,
                            damage_type: None,
                            age: 0.0,
                            from_player: true,
                            drop_item: None,
                            preparation_payload: removed.filter(|stack| stack.arcane_id != 0),
                            owner: 0,
                        });
                }
                self.sfx(Sfx::Bolt(1.6));
                self.input.action_cooldown = 0.35;
                return true;
            }
            // A cutting knows where it is needed: held up, it gives a
            // bearing to the nearest ground that would take it. That is
            // the only navigation that works at province range, where
            // countries are 900 blocks apart and you see a few hundred.
            //
            // NOT while pointing at a heart. This arm runs before the
            // block-interaction pass, so without that guard reading the
            // bearing would shadow PLANTING the thing — the whole
            // restoration verb, silently gone.
            let at_heart = hit.as_ref().is_some_and(|h| {
                reg.block(self.runtime.view().get_block_at(h.block))
                    .interaction
                    .as_deref()
                    == Some("heart")
            });
            if !at_heart && held.is_some_and(|i| world::seed_nature(&reg.item(i).name).is_some()) {
                let line = self.runtime.view().seed_bearing_at(self.player.pos);
                self.toast(line);
                self.sfx(Sfx::Click);
                self.input.action_cooldown = 0.6;
                return true;
            }
            // Knowledge is physical: artifacts retain their generated words,
            // while ledgers and folios expose only the signed records inside.
            if held.is_some_and(|item| reg.item(item).discovery.is_some()) {
                self.read_held_knowledge();
                self.input.action_cooldown = 0.6;
                return true;
            }
            // Bedroll: camp until dawn.
            if held.is_some_and(|i| reg.item(i).bedroll) {
                self.try_sleep();
                self.input.action_cooldown = 0.5;
                return true;
            }
        }

        false
    }
}
