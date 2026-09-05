//! Station work in the ordered graphical action pipeline.

use crate::game::Game;
use crate::world::TerrainRead;
use crate::atlas;
use crate::audio::BreakMat;
use crate::audio::Sfx;
use crate::entity::ItemEntity;
use crate::net;
use crate::world;
use glam::Vec3;
use super::ActionFrame;

impl Game {
    pub(in crate::game) fn interact_station_work(&mut self, frame: &ActionFrame) -> bool {
        let reg = &frame.reg;
        let hit = &frame.hit;
        let held = frame.held;
        let dt = frame.dt;
        // Station work is a held channel: hammer strikes at the anvil,
        // bare-hand turns at the quern. The def decides the tool.
        let anvil_target = hit.as_ref().map(|h| h.block).filter(|t| {
            let station = reg
                .block(self.runtime.view().get_block_at(*t))
                .interaction
                .clone();
            let Some(station) = station else { return false };
            // Powered stations take their strikes from the shaft
            // line; hands only load and unload them.
            if world::station_powered(&station) {
                return false;
            }
            let rested = match self.runtime.view().block_entity_at(t) {
                Some(world::BlockEntity::Anvil(a)) => a.bloom,
                _ => None,
            };
            let Some(rested) = rested else { return false };
            let Some(def) = reg
                .worked
                .iter()
                .find(|w| w.input == rested.item && w.station == station)
            else {
                return false;
            };
            if def.needs_hammer {
                held.is_some_and(|i| reg.item(i).hammer)
            } else {
                held.is_none()
            }
        });
        if let (true, Some(target)) = (self.input.right_held, anvil_target) {
            if self.interaction.anvil_pos != Some(target) {
                self.interaction.anvil_pos = Some(target);
                self.interaction.anvil_work = 0.0;
            }
            self.interaction.anvil_work += dt;
            if self.interaction.anvil_work >= 2.0 {
                self.interaction.anvil_work = 0.0;
                self.sfx(Sfx::Break(BreakMat::Stone));
                let top_pos = crate::planet::EntityPos::new(
                    target.face(),
                    f32::from(target.u()) + 0.5,
                    f32::from(target.y()) + 1.05,
                    f32::from(target.v()) + 0.5,
                )
                .expect("station effects remain above their source");
                let top = top_pos.render_pos();
                if self.presentation.juice {
                    if held.is_some_and(|i| reg.item(i).hammer) {
                        // The promised sparks: embers ring off the bloom.
                        let v = self.presentation.vary();
                        self.sfx_vol(Sfx::Spark, v.min(1.0));
                        let ember = *atlas::builtin_slots().get("ember").unwrap_or(&0);
                        self.presentation.burst(top, ember, 8, 1.8);
                    } else {
                        let v = self.presentation.vary();
                        self.sfx_vol(Sfx::Grind, v.min(1.0));
                        let b = self.runtime.view().get_block_at(target);
                        let tile = reg.block(b).tiles[2];
                        self.presentation.puff(top, tile, 3);
                    }
                }
                if !self.creative && held.is_some() {
                    self.inventory.wear_tool(&reg, self.input.hotbar_sel);
                }
                if let Some(rc) = &self.multiplayer.remote {
                    // The host counts strikes and Gives the bar.
                    rc.session.send(&net::C2S::AnvilStrike { pos: target });
                } else {
                    let Some(out) = self.runtime.local_mut().world.anvil_strike_at(target) else {
                        return true;
                    };
                    let center = crate::planet::EntityPos::new(
                        target.face(),
                        f32::from(target.u()) + 0.5,
                        f32::from(target.y()) + 1.0,
                        f32::from(target.v()) + 0.5,
                    )
                    .expect("worked item remains above its station");
                    self.runtime.local_mut().world.spawn_loose_item(ItemEntity::new(
                        center,
                        Vec3::new(0.0, 2.0, 0.0),
                        out.item,
                        out.count,
                    ));
                    self.sfx(Sfx::Craft);
                }
            }
            return true;
        } else {
            self.interaction.anvil_work = 0.0;
            self.interaction.anvil_pos = None;
        }


        false
    }
}
