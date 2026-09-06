//! Mining in the ordered graphical action pipeline.

use super::ActionFrame;
use crate::audio::Sfx;
use crate::entity::ItemEntity;
use crate::game::Game;
use crate::net;
use crate::raycast;
use crate::registry::AIR;
use crate::world::TerrainRead;
use glam::Vec3;

impl Game {
    pub(in crate::game) fn interact_mining(&mut self, frame: &ActionFrame) -> bool {
        let reg = &frame.reg;
        let hit = &frame.hit;
        let aim = &frame.aim;
        let held = frame.held;
        let dt = frame.dt;
        // Hold-to-mine; tools speed up matching blocks and wear down.
        if self.input.left_held {
            // Structure mining (takes priority over world mining).
            if let Some(raycast::TargetHit::Structure {
                id,
                block,
                adjacent: _adj,
            }) = &aim
            {
                let (sid, soff) = (*id, *block);
                let s_block_id = self
                    .runtime
                    .view()
                    .local_structure(sid)
                    .map(|s| s.get_block(soff))
                    .unwrap_or(AIR);
                let hardness = if self.creative {
                    reg.block(s_block_id).hardness.map(|_| 0.0001)
                } else {
                    reg.effective_hardness(s_block_id, held)
                };
                if let Some(hardness) = hardness {
                    let target_break = crate::game::BreakTarget::Structure(sid, soff);
                    let progress = match self.interaction.breaking {
                        Some((t, p)) if t == target_break => p + dt / hardness.max(0.0001),
                        _ => dt / hardness.max(0.0001),
                    };
                    if progress >= 1.0 {
                        // No `on_block_break` script hook for structure
                        // blocks (they have no world BlockPos).
                        self.interaction.breaking = None;
                        let drop = self
                            .runtime
                            .local_mut()
                            .world
                            .local_structure_mut(sid)
                            .and_then(|s| s.break_block(soff, held));
                        // 8c: drop to player inventory directly.
                        if !self.creative {
                            if let Some(stack) = drop {
                                let item = stack.item;
                                let remaining = self.inventory.add_stack(reg, stack);
                                if remaining > 0
                                    && let Some(wp) = self
                                        .runtime
                                        .view()
                                        .local_structure(sid)
                                        .and_then(|s| s.world_position(soff))
                                {
                                    self.runtime.local_mut().world.spawn_loose_item(
                                        ItemEntity::new(
                                            wp.entity_at_height(0.3),
                                            Vec3::new(0.0, 2.2, 0.0),
                                            item,
                                            remaining,
                                        ),
                                    );
                                }
                            }
                            self.inventory.wear_tool(reg, self.input.hotbar_sel);
                        }
                        self.survival.hunger = (self.survival.hunger - 0.008).max(0.0);
                        self.sfx(Sfx::Break(self.break_mat(s_block_id)));
                        if let Some(wp) = self
                            .runtime
                            .view()
                            .local_structure(sid)
                            .and_then(|s| s.world_position(soff))
                        {
                            self.presentation.burst(
                                wp.entity_center().render_pos(),
                                self.content.reg.block(s_block_id).tiles[0],
                                10,
                                2.2,
                            );
                        }
                    } else {
                        self.interaction.breaking = Some((target_break, progress));
                    }
                } else {
                    self.interaction.breaking = None;
                }
            } else if let Some(h) = &hit {
                let target = h.block;
                let b = self.runtime.view().get_block_at(target);
                let hardness = if self.creative {
                    // Creative breaks anything instantly — except the
                    // unbreakable (the world's floor stays a floor).
                    reg.block(b).hardness.map(|_| 0.0001)
                } else {
                    reg.effective_hardness(b, held)
                };
                if let Some(hardness) = hardness {
                    let progress = match self.interaction.breaking {
                        Some((t, p)) if t == crate::game::BreakTarget::World(target) => {
                            p + dt / hardness.max(0.0001)
                        }
                        _ => dt / hardness.max(0.0001),
                    };
                    if progress >= 1.0 {
                        // Flag-gated features (spec 2.5): a sealed gate cannot
                        // be mined open. The world refuses anyway (backstop);
                        // here we surface the reason as a toast instead of
                        // letting the swing hit the None path.
                        let gate_blocked = if let Some(gate) = self.runtime.view().gate_at(target)
                            && self
                                .content
                                .reg
                                .gates
                                .get(gate)
                                .is_some_and(|g| g.unbreakable_when_locked)
                        {
                            let definition = self.content.reg.gates[gate].clone();
                            let unlocked = self
                                .read_player_kv(&definition.flag)
                                .is_some_and(|v| v == definition.value);
                            if !unlocked {
                                self.toast(definition.message.clone());
                            } else {
                                self.toast("Right-click to open the sealed gate.".to_string());
                            }
                            true
                        } else {
                            false
                        };
                        self.interaction.breaking = None;
                        if gate_blocked {
                            return true;
                        }
                        // Cancellable mod event.
                        let allow = if self.content.scripts.wants("on_block_break") {
                            let name = reg.block(b).name.clone();
                            let ok = self.content.scripts.dispatch_view(
                                &self.runtime.view(),
                                "on_block_break",
                                (
                                    target.face().name().to_string(),
                                    target.u() as i64,
                                    target.y() as i64,
                                    target.v() as i64,
                                    name,
                                ),
                            );
                            self.apply_script_cmds();
                            ok
                        } else {
                            true
                        };
                        self.interaction.breaking = None;
                        if allow && self.multiplayer.remote.is_some() {
                            // Guests request; the echo applies the change.
                            if let Some(r) = &self.multiplayer.remote {
                                r.session.send(&net::C2S::Break { pos: target });
                            }
                            self.survival.hunger = (self.survival.hunger - 0.008).max(0.0);
                            self.sfx(Sfx::Break(self.break_mat(b)));
                            self.presentation.burst(
                                target.entity_center().render_pos(),
                                self.content.reg.block(b).tiles[0],
                                10,
                                2.2,
                            );
                            if !self.creative {
                                self.inventory.wear_tool(reg, self.input.hotbar_sel);
                            }
                            return true;
                        }
                        if allow {
                            let Some(mined) = crate::player_ops::terrain::mine(
                                &mut self.runtime.local_mut().world,
                                target,
                                held,
                                self.creative,
                            ) else {
                                return true;
                            };
                            self.survival.hunger = (self.survival.hunger - 0.008).max(0.0);
                            let (result, sheared) = (mined.result, mined.sheared);
                            let b = result.block;
                            self.sfx(Sfx::Break(self.break_mat(b)));
                            self.presentation.burst(
                                target.entity_center().render_pos(),
                                self.content.reg.block(b).tiles[0],
                                10,
                                2.2,
                            );
                            self.grant_xp("mine");
                            if !self.creative {
                                self.inventory.wear_tool(reg, self.input.hotbar_sel);
                            }
                            // Shears: leaves come off whole.
                            if sheared
                                && !self.creative
                                && let Some(item) = reg.item_id(&reg.block(b).name)
                            {
                                let center = target.entity_at_height(0.3);
                                self.runtime
                                    .local_mut()
                                    .world
                                    .spawn_loose_item(ItemEntity::new(
                                        center,
                                        Vec3::new(0.0, 2.2, 0.0),
                                        item,
                                        1,
                                    ));
                            }
                            if let Some(drop) = result.drop {
                                let center = target.entity_at_height(0.3);
                                let a = self.rand01() * std::f32::consts::TAU;
                                let v = Vec3::new(a.cos() * 1.2, 2.2, a.sin() * 1.2);
                                let mut entity = ItemEntity::new(center, v, drop.item, drop.count);
                                entity.durability = drop.durability;
                                entity.arcane_id = drop.arcane_id;
                                self.runtime.local_mut().world.spawn_loose_item(entity);
                            }
                            // Chance extras (leaves drop saplings).
                            if !self.creative
                                && let Some(stack) = self
                                    .runtime
                                    .local_mut()
                                    .world
                                    .roll_bonus_drop_at(target, b, &mut self.rng)
                            {
                                let center = target.entity_at_height(0.3);
                                let a = self.rand01() * std::f32::consts::TAU;
                                let v = Vec3::new(a.cos() * 1.2, 2.2, a.sin() * 1.2);
                                let mut entity =
                                    ItemEntity::new(center, v, stack.item, stack.count);
                                entity.arcane_id = stack.arcane_id;
                                self.runtime.local_mut().world.spawn_loose_item(entity);
                            }
                        }
                    } else {
                        let stage_before =
                            (self.interaction.breaking.map(|(_, p)| p).unwrap_or(0.0) * 4.0) as i32;
                        self.interaction.breaking =
                            Some((crate::game::BreakTarget::World(target), progress));
                        // Chips fly as each crack stage lands.
                        if (progress * 4.0) as i32 > stage_before {
                            self.presentation.burst(
                                target.entity_center().render_pos(),
                                self.content.reg.block(b).tiles[0],
                                2,
                                1.2,
                            );
                        }
                        // Keep the arm swinging while we chip away.
                        if self.presentation.swing <= 0.0 {
                            self.presentation.swing = 1.0;
                        }
                    }
                } else {
                    self.interaction.breaking = None;
                }
            } else {
                self.interaction.breaking = None;
            }
        } else {
            self.interaction.breaking = None;
        }

        false
    }
}
