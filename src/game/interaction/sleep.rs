//! Sleep interaction adapter.

use crate::audio::Sfx;
use crate::game::Game;
use crate::net;

impl Game {
    /// Bedroll: sleep to dawn if it's night and the wild is far enough.
    /// In multiplayer, dawn waits for everyone (the sleep vote).
    pub(in crate::game) fn try_sleep(&mut self) {
        let sun = (self.runtime.time_of_day() * std::f32::consts::TAU).sin();
        if sun > -0.05 {
            self.toast("You can only sleep at night.".to_string());
            return;
        }
        if let Some(r) = &mut self.multiplayer.remote {
            r.session.send(&net::C2S::SleepRequest);
            r.sleeping = true;
            self.toast("You settle in, waiting for the others... (move to get up)".to_string());
            return;
        }
        if self
            .multiplayer
            .host
            .as_ref()
            .is_some_and(|h| h.guests.values().any(|guest| guest.is_active()))
        {
            self.multiplayer.host_sleeping = true;
            self.survival.spawn_point = self.player.pos;
            self.toast("You settle in, waiting for the others... (move to get up)".to_string());
            return;
        }
        let reg = self.content.reg.clone();
        let near_warden = self.runtime.view().mobs().iter().any(|m| {
            reg.animals.get(m.species).is_some_and(|d| d.hostile)
                && (m.pos - self.player.pos).length_squared() < 24.0 * 24.0
        });
        if near_warden {
            self.toast("The wild is too close.".to_string());
            return;
        }
        // Time passes fairly: the skipped night still decays ire.
        let skipped = (1.0 + 0.3 - self.runtime.time_of_day()) % 1.0;
        if self.runtime.local_mut().world.tick_ire(skipped) {
            let r = self.runtime.local_mut().world.accept_offerings();
            if r > 0.0 {
                self.toast("The wild has accepted your offering.".to_string());
            }
        }
        self.runtime.local_mut().sleep_to_dawn();
        self.survival.spawn_point = self.player.pos;
        if !self.creative {
            self.inventory.wear_tool(&reg, self.input.hotbar_sel);
        }
        match self.save_session() {
            Ok(_) => self.toast("You camp until dawn. This is home now.".to_string()),
            Err(error) => {
                eprintln!("world: camp save incomplete: {error}");
                self.toast(format!("You wake, but the camp could not save: {error}"));
            }
        }
        self.sfx(Sfx::Craft);
    }
}
