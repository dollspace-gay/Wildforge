//! Held channels in the ordered graphical action pipeline.

use super::ActionFrame;
use crate::audio::Sfx;
use crate::game::Game;

impl Game {
    pub(in crate::game) fn interact_held_channels(&mut self, frame: &ActionFrame) -> bool {
        let reg = &frame.reg;
        let held = frame.held;
        let dt = frame.dt;
        // Bow: hold right to draw, release to loose (0.25 s minimum).
        let bow_def = held.and_then(|i| reg.item(i).bow.clone());
        if let Some(bow) = bow_def {
            if self.input.right_held && (self.creative || self.has_ammo("arrow")) {
                self.interaction.bow_draw += dt;
            } else {
                if self.interaction.bow_draw >= 0.25 {
                    let charge = ((self.interaction.bow_draw - 0.25) / 0.75).clamp(0.0, 1.0);
                    self.fire_bow(&bow, charge);
                }
                self.interaction.bow_draw = 0.0;
            }
        } else if self.interaction.bow_draw > 0.0 {
            self.interaction.bow_draw = 0.0; // switched away mid-draw
        }

        // The line in the water: the water decides when. A bite opens
        // a short window announced by a splash; miss it and the wait
        // begins again.
        let rod_held = held.is_some_and(|i| reg.item(i).name == "base:fishing_rod");
        if !rod_held {
            self.interaction.fishing = None;
        } else if let Some((bobber, mut wait, mut bite)) = self.interaction.fishing {
            if bite > 0.0 {
                bite -= dt;
                if bite <= 0.0 {
                    // Missed it: the water loses interest for a while.
                    wait = 4.0 + self.rand01() * 8.0;
                }
            } else {
                wait -= dt;
                if wait <= 0.0 {
                    bite = 1.4;
                    let tile = reg.block(reg.water_block(0)).tiles[0];
                    self.presentation.burst(bobber.render_pos(), tile, 8, 1.4);
                    self.sfx(Sfx::Splash);
                }
            }
            self.interaction.fishing = Some((bobber, wait, bite));
        }

        false
    }
}
