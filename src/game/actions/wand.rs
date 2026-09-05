//! Wand in the ordered graphical action pipeline.

use crate::game::Game;
use crate::world::TerrainRead;
use crate::audio::Sfx;
use crate::identity;
use crate::net;
use crate::raycast;
use crate::game::LocalWorkingChannel;

impl Game {

    pub(in crate::game) fn interact_wand(&mut self, dt: f32, hit: Option<&raycast::PlanetHit>) -> bool {
        use crate::workings::{WorkingIntent, WorkingTargetIntent};

        let left_range = if self.multiplayer.remote.is_none() {
            self.interaction.working.as_ref().and_then(|channel| {
                let source = self.player.pos.block()?;
                (channel.stable_id != 0
                    && !self.runtime.local().world.wand_working_reachable_from(channel.stable_id, source))
                .then_some(channel.stable_id)
            })
        } else {
            None
        };
        if let Some(stable_id) = left_range {
            self.interaction.working = None;
            let mut cue = self.runtime.local().world.working_cues()
                .into_iter()
                .find(|cue| cue.stable_id == stable_id);
            match self.runtime.local_mut().world.interrupt_working(stable_id) {
                Ok(result) => {
                    if let Some(cue) = cue.as_mut() {
                        cue.kind = result.cue;
                        cue.warning_band = result.warning_band;
                        cue.completion_permille = 1_000;
                    }
                    if let Some(cue) = cue {
                        self.present_working_cue(cue);
                    }
                    self.toast("The wand path leaves its bounded reach and breaks cleanly.".into());
                }
                Err(error) => self.toast(error),
            }
            return true;
        }

        let held_stack = self.inventory.slots[self.input.hotbar_sel];
        let held_wand = held_stack.filter(|stack| {
            stack.arcane_id != 0
                && self
                    .content
                    .reg
                    .item(stack.item)
                    .implement
                    .as_ref()
                    .is_some_and(|definition| {
                        definition.kind == crate::implements::ImplementItemKind::Wand
                    })
        });
        if let Some(channel) = self.interaction.working.as_ref()
            && held_wand.is_none_or(|wand| wand.arcane_id != channel.wand_id)
        {
            let channel = self.interaction.working.take().unwrap();
            if let Some(remote) = &self.multiplayer.remote {
                remote.session.send(&net::C2S::OperateWorking {
                    working_id: channel.working_id,
                    held_instance: channel.wand_id,
                    target: channel.target,
                    intent: WorkingIntent::Cancel,
                });
            } else if channel.stable_id != 0
                && let Err(error) = self.runtime.local_mut().world.interrupt_working(channel.stable_id)
            {
                self.toast(error);
            }
            return true;
        }
        let Some(wand) = held_wand else {
            return false;
        };
        if let Some(channel) = self.interaction.working.as_mut() {
            if self.input.right_held {
                channel.held_secs += dt;
                if channel.held_secs >= crate::workings::MIN_WAND_SETTLE_SECONDS
                    && !channel.hold_sent
                {
                    channel.hold_sent = true;
                    let working_id = channel.working_id.clone();
                    let wand_id = channel.wand_id;
                    let target = channel.target;
                    let stable_id = channel.stable_id;
                    if let Some(remote) = &self.multiplayer.remote {
                        remote.session.send(&net::C2S::OperateWorking {
                            working_id,
                            held_instance: wand_id,
                            target,
                            intent: WorkingIntent::Hold,
                        });
                    } else if stable_id != 0 {
                        match self.runtime.local_mut().world.activate_working(stable_id) {
                            Ok(result) => self.toast(result.message),
                            Err(error) if !error.contains("cannot move") => self.toast(error),
                            Err(_) => {}
                        }
                    }
                }
                return true;
            }
            let channel = self.interaction.working.take().unwrap();
            if let Some(remote) = &self.multiplayer.remote {
                remote.session.send(&net::C2S::OperateWorking {
                    working_id: channel.working_id,
                    held_instance: channel.wand_id,
                    target: channel.target,
                    intent: WorkingIntent::Release,
                });
            } else if channel.stable_id != 0 {
                let completion = if channel.working_id == "base:fieldmend" {
                    self.runtime.local_mut().world.complete_inventory_working(channel.stable_id, &mut self.inventory)
                } else {
                    self.runtime.local_mut().world.release_working(channel.stable_id)
                };
                match completion {
                    Ok(result) => {
                        if result.phase == Some(crate::workings::WorkingPhase::PendingApply) {
                            match self.save_player() {
                                Ok(()) => match self.runtime.local_mut().world.finish_inventory_working(channel.stable_id)
                                {
                                    Ok(finished) => self.toast(finished.message),
                                    Err(error) => self.toast(error),
                                },
                                Err(error) => self.toast(format!(
                                    "Fieldmend landed, but its profile checkpoint failed: {error}"
                                )),
                            }
                        } else {
                            self.toast(result.message);
                        }
                        self.sfx(Sfx::ImplementUse);
                    }
                    Err(error) => {
                        self.toast(error);
                        self.sfx(Sfx::ImplementFailure);
                    }
                }
            }
            self.input.action_cooldown = crate::workings::WAND_RECOVERY_SECONDS;
            return true;
        }
        if !self.input.right_held || self.input.action_cooldown > 0.0 {
            return false;
        }
        let source = match self.player.pos.block() {
            Some(source) => source,
            None => return true,
        };
        let water_hit = raycast::raycast_water_at(
            &self.runtime.view(),
            self.player.eye(),
            self.camera.local_forward(),
            self.reach(),
        )
        .filter(|water| {
            self.content
                .reg
                .is_water(self.runtime.view().get_block_at(water.block))
        });
        let eye = self.player.eye();
        let forward = self.camera.local_forward().normalize_or_zero();
        let reach = self.reach();
        let entity_target = self.runtime.view().projectiles()
            .iter()
            .map(|projectile| (projectile.pos, projectile.stable_id, 0.45))
            .chain(
                self.runtime.view().loose_items()
                    .iter()
                    .map(|item| (item.pos, item.stable_id, 0.35)),
            )
            .filter_map(|(pos, stable_id, radius)| {
                if stable_id == 0 { return None; }
                let delta = eye.local_delta_to(pos);
                let along = delta.dot(forward);
                (along > 0.0 && along <= reach && (delta - forward * along).length() <= radius)
                    .then_some((along, stable_id))
            })
            .min_by(|left, right| left.0.total_cmp(&right.0))
            .map(|(_, stable_id)| stable_id);
        let (working_id, target) = if let Some(stable_id) = entity_target {
            (
                "base:nudge".to_string(),
                WorkingTargetIntent::Entity { stable_id },
            )
        } else if let Some(water) = water_hit
            && (self
                .content
                .reg
                .is_air(self.runtime.view().get_block_at(water.adjacent))
                || self
                    .content
                    .reg
                    .is_water(self.runtime.view().get_block_at(water.adjacent)))
        {
            (
                "base:draw".to_string(),
                WorkingTargetIntent::Water {
                    from: water.block,
                    to: water.adjacent,
                    water_hu: crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL,
                },
            )
        } else if let Some(hit) = hit {
            let block = self.runtime.view().get_block_at(hit.block);
            let definition = self.content.reg.block(block);
            if definition.interaction.as_deref() == Some("discovery_lab")
                && (self.multiplayer.remote.is_some()
                    || self.runtime.local().world.holdfast_mounted_target_at(hit.block))
            {
                // The client identifies only the physical mount. Its hidden
                // sample contents remain host-owned and are validated by the
                // Holdfast handler before any Current is reserved.
                (
                    "base:holdfast".to_string(),
                    WorkingTargetIntent::Block {
                        pos: hit.block,
                        adjacent: None,
                    },
                )
            } else if self.runtime.view().is_nudge_mechanism_at(hit.block) {
                (
                    "base:nudge".to_string(),
                    WorkingTargetIntent::Block {
                        pos: hit.block,
                        adjacent: None,
                    },
                )
            } else if definition.crop_next.is_some() || definition.sapling.is_some() {
                (
                    "base:rootwake".to_string(),
                    WorkingTargetIntent::Block {
                        pos: hit.block,
                        adjacent: None,
                    },
                )
            } else if definition.burns != 0 {
                (
                    "base:kindle".to_string(),
                    WorkingTargetIntent::Block {
                        pos: hit.block,
                        adjacent: Some(hit.adjacent),
                    },
                )
            } else {
                (
                    "base:trace".to_string(),
                    WorkingTargetIntent::Block {
                        pos: hit.block,
                        adjacent: None,
                    },
                )
            }
        } else {
            // Inventory workings use a physical little tableau instead of a
            // spell hotbar: target immediately right of the wand, matching
            // stock one slot farther right. With no valid tableau, empty-air
            // use remains Gleam.
            let target_slot = (self.input.hotbar_sel + 1) % crate::inventory::HOTBAR_SLOTS;
            let material_slot = (self.input.hotbar_sel + 2) % crate::inventory::HOTBAR_SLOTS;
            let staged = self.inventory.slots[target_slot];
            let matching = staged.and_then(|target| {
                let definition = self.content.reg.item(target.item);
                let repair = self
                    .content
                    .reg
                    .item_id(&format!("{}/forge_scrap", definition.name))?;
                self.inventory.slots[material_slot]
                    .is_some_and(|stock| stock.item == repair && stock.count == 1)
                    .then_some(())
            });
            let fragile = staged.is_some_and(|stack| {
                let definition = self.content.reg.item(stack.item);
                stack.count == 1
                    && definition.durability != 0
                    && (definition.food.is_some()
                        || definition.name.ends_with("_seed")
                        || (definition.arcane.is_some() && definition.places.is_some()))
            });
            if matching.is_some() {
                (
                    "base:fieldmend".to_string(),
                    WorkingTargetIntent::Inventory {
                        target_slot: target_slot as u8,
                        material_slot: Some(material_slot as u8),
                        magnitude: 16,
                    },
                )
            } else if fragile {
                (
                    "base:holdfast".to_string(),
                    WorkingTargetIntent::Inventory {
                        target_slot: target_slot as u8,
                        material_slot: None,
                        magnitude: 1,
                    },
                )
            } else {
                ("base:gleam".to_string(), WorkingTargetIntent::None)
            }
        };
        // Ctrl + use is an explicit unsafe choice. It never changes the
        // effect requested by the client; it only authorizes the host to draw
        // below the measured safe floor with visible, deterministic cost.
        let forced = self.input.keys.sprint;
        let start_intent = if forced {
            WorkingIntent::StartForced
        } else {
            WorkingIntent::Start
        };
        if let Some(remote) = &self.multiplayer.remote {
            remote.session.send(&net::C2S::OperateWorking {
                working_id: working_id.clone(),
                held_instance: wand.arcane_id,
                target,
                intent: start_intent,
            });
            self.interaction.working = Some(LocalWorkingChannel {
                stable_id: 0,
                working_id,
                wand_id: wand.arcane_id,
                target,
                held_secs: 0.0,
                hold_sent: false,
            });
        } else {
            let player_id = identity::local_player_id(
                &self.runtime.local().world.save_dir_for_saving(),
                self.identity.device_id(),
            )
            .unwrap_or(identity::PlayerId([0; 16]));
            let result = self.runtime.local_mut().world.begin_wand_working(
                player_id.0,
                &self.config.display_name,
                source,
                wand.arcane_id,
                &working_id,
                target,
                Some(&self.inventory),
                forced,
            );
            match result {
                Ok(result) => {
                    if let Some(cue) = self.runtime.local().world.working_cues()
                        .into_iter()
                        .find(|cue| cue.stable_id == result.stable_id)
                    {
                        self.present_working_cue(cue);
                    }
                    self.toast(result.message);
                    self.interaction.working = Some(LocalWorkingChannel {
                        stable_id: result.stable_id,
                        working_id,
                        wand_id: wand.arcane_id,
                        target,
                        held_secs: 0.0,
                        hold_sent: false,
                    });
                }
                Err(error) => {
                    self.toast(error);
                    self.sfx(Sfx::ImplementFailure);
                }
            }
        }
        self.input.action_cooldown = 0.1;
        true
    }
}
