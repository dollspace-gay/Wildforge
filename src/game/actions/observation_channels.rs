//! Observation channels in the ordered graphical action pipeline.

use super::ActionFrame;
use crate::audio::Sfx;
use crate::entity::ItemEntity;
use crate::game::DiscoveryAim;
use crate::game::Game;
use crate::net;
use crate::world::TerrainRead;
use glam::Vec3;

impl Game {
    pub(in crate::game) fn interact_observation_channels(&mut self, frame: &ActionFrame) -> bool {
        let reg = &frame.reg;
        let hit = &frame.hit;
        let held = frame.held;
        let dt = frame.dt;
        // A tuning lens is deliberately slow and local. Holding the aim still
        // for the full settle period produces one qualitative, signed record;
        // moving off the target or releasing use starts the reading over.
        let lens_held = self.inventory.slots[self.input.hotbar_sel].is_some_and(|stack| {
            reg.item(stack.item)
                .discovery
                .as_ref()
                .is_some_and(|definition| definition.kind == "tuning_lens")
                && stack.arcane_id != 0
        });
        if self.input.right_held && lens_held {
            let aim = hit.as_ref().map_or_else(
                || self.player.pos.block().map(DiscoveryAim::Region),
                |hit| Some(DiscoveryAim::Block(hit.block)),
            );
            let Some(aim) = aim else {
                return true;
            };
            if self.interaction.lens_target != Some(aim) {
                self.interaction.lens_target = Some(aim);
                self.interaction.lens_settle = 0.0;
                if let Some(remote) = &self.multiplayer.remote {
                    if let DiscoveryAim::Block(pos) = aim
                        && reg
                            .block(self.runtime.view().get_block_at(pos))
                            .discovery_fixture
                            .as_ref()
                            .is_some_and(|fixture| fixture.kind == "experiment_apparatus")
                    {
                        let kind =
                            crate::discovery::ExperimentKind::ALL[self.interaction.experiment_kind
                                % crate::discovery::ExperimentKind::ALL.len()];
                        remote
                            .session
                            .send(&net::C2S::BeginExperiment { pos, kind });
                    } else {
                        remote.session.send(&net::C2S::BeginObserve {
                            target: match aim {
                                DiscoveryAim::Region(_) => net::DiscoveryTargetSnap::Region,
                                DiscoveryAim::Block(pos) => net::DiscoveryTargetSnap::Block(pos),
                            },
                        });
                    }
                }
                self.sfx(Sfx::Lens(0.78));
            }
            let prior_settle = self.interaction.lens_settle;
            self.interaction.lens_settle += dt;
            if prior_settle < 0.62 && self.interaction.lens_settle >= 0.62 {
                self.sfx(Sfx::Lens(0.96));
            }
            if self.interaction.lens_settle >= 1.25 {
                self.interaction.lens_settle = 0.0;
                self.interaction.lens_target = None;
                self.input.right_held = false;
                self.input.action_cooldown = 0.25;
                if let DiscoveryAim::Block(pos) = aim
                    && reg
                        .block(self.runtime.view().get_block_at(pos))
                        .discovery_fixture
                        .as_ref()
                        .is_some_and(|fixture| fixture.kind == "experiment_apparatus")
                {
                    self.settle_discovery_experiment(pos);
                } else {
                    self.settle_discovery_reading(aim);
                }
                self.sfx(Sfx::Click);
            }
            return true;
        } else {
            self.interaction.lens_settle = 0.0;
            self.interaction.lens_target = None;
        }

        // Archaeology and regional salvage: sweeping a remnant or sifting
        // ordinary ground is a slow, careful channel.
        let brush_held = held.is_some_and(|i| reg.item(i).brush_tool);
        let brush_target = hit.as_ref().map(|h| h.block).filter(|t| {
            brush_held
                && (reg
                    .block(self.runtime.view().get_block_at(*t))
                    .brush
                    .is_some()
                    || self.runtime.view().can_sift_salvage_at(*t))
        });
        if let (true, Some(target)) = (self.input.right_held, brush_target) {
            if self.interaction.brush_target != Some(target) {
                self.interaction.brush_target = Some(target);
                self.interaction.brushing = 0.0;
            }
            self.interaction.brushing += dt;
            if self.interaction.brushing >= 1.5 {
                self.interaction.brushing = 0.0;
                self.interaction.brush_target = None;
                if let Some(rc) = &self.multiplayer.remote {
                    // The host rolls the find and Gives it straight to
                    // us; the BlockSet echo swaps the remnant out.
                    rc.session.send(&net::C2S::BrushBlock { pos: target });
                    if !self.creative {
                        self.inventory.wear_tool(reg, self.input.hotbar_sel);
                    }
                    return true;
                }
                let archaeology = reg
                    .block(self.runtime.view().get_block_at(target))
                    .brush
                    .is_some();
                let found = if archaeology {
                    let mut r = self.rng;
                    let found = self
                        .runtime
                        .local_mut()
                        .world
                        .brush_block_at(target, &mut r);
                    self.rng = r;
                    found
                } else {
                    match self.runtime.local_mut().world.sift_salvage_at(target) {
                        Ok(found) => found,
                        Err(error) => {
                            eprintln!("materials: regional salvage recovery failed: {error}");
                            None
                        }
                    }
                };
                if let Some(stack) = found {
                    let center = crate::planet::EntityPos::new(
                        target.face(),
                        f32::from(target.u()) + 0.5,
                        f32::from(target.y()) + 0.6,
                        f32::from(target.v()) + 0.5,
                    )
                    .expect("brushed item begins inside its source cell");
                    let mut ent =
                        ItemEntity::new(center, Vec3::new(0.0, 2.0, 0.0), stack.item, stack.count);
                    // Old tools surface as worn as they were buried.
                    if stack.durability < reg.item(stack.item).durability {
                        ent.durability = stack.durability;
                    }
                    ent.arcane_id = stack.arcane_id;
                    self.runtime.local_mut().world.spawn_loose_item(ent);
                    self.sfx(Sfx::Pickup);
                    if !archaeology {
                        self.toast("The brush turns up usable buried stock.".into());
                    }
                } else if !archaeology {
                    self.toast("Nothing recoverable gathers in this ground yet.".into());
                }
                if !self.creative {
                    self.inventory.wear_tool(reg, self.input.hotbar_sel);
                }
            }
            return true;
        } else {
            self.interaction.brushing = 0.0;
            self.interaction.brush_target = None;
        }

        false
    }
}
