//! Player interaction, combat, stations, and script-driven actions.

use super::*;

impl Game {
    /// A line from the lost takers, chosen at random.
    pub(super) fn read_tablet(&mut self) {
        const LINES: [&str; 10] = [
            "We burned the south wood. The nights grew teeth.",
            "The forge ran hot for a year. Then the trees walked.",
            "Plant after you cut. My father knew. I forgot.",
            "It does not hate. It answers.",
            "We left the stone offerings too late.",
            "The deep ones never slept. We dug anyway.",
            "Third winter: the wisps crossed the river.",
            "Feed the land and it feeds you. Starve it and it comes.",
            "My daughter planted a row of oaks. They spared her field.",
            "If you read this: the wild forgives. Slowly.",
        ];
        let i = (self.rand01() * LINES.len() as f32) as usize % LINES.len();
        self.toast(LINES[i].to_string());
        self.sfx(Sfx::Click);
    }

    /// Bedroll: sleep to dawn if it's night and the wild is far enough.
    /// In multiplayer, dawn waits for everyone (the sleep vote).
    pub(super) fn try_sleep(&mut self) {
        let sun = (self.server.time_of_day * std::f32::consts::TAU).sin();
        if sun > -0.05 {
            self.toast("You can only sleep at night.".to_string());
            return;
        }
        if let Some(r) = &mut self.multiplayer.remote {
            r.client.send(&net::C2S::SleepRequest);
            r.sleeping = true;
            self.toast("You settle in, waiting for the others... (move to get up)".to_string());
            return;
        }
        if self
            .multiplayer
            .host
            .as_ref()
            .is_some_and(|h| !h.guests.is_empty())
        {
            self.multiplayer.host_sleeping = true;
            self.survival.spawn_point = self.player.pos;
            self.toast("You settle in, waiting for the others... (move to get up)".to_string());
            return;
        }
        let reg = self.content.reg.clone();
        let near_warden = self.server.world.mobs().iter().any(|m| {
            reg.animals.get(m.species).is_some_and(|d| d.hostile)
                && (m.pos - self.player.pos).length_squared() < 24.0 * 24.0
        });
        if near_warden {
            self.toast("The wild is too close.".to_string());
            return;
        }
        // Time passes fairly: the skipped night still decays ire.
        let skipped = (1.0 + 0.3 - self.server.time_of_day) % 1.0;
        if self.server.world.tick_ire(skipped) {
            let r = self.server.world.accept_offerings();
            if r > 0.0 {
                self.toast("The wild has accepted your offering.".to_string());
            }
        }
        self.server.sleep_to_dawn();
        self.survival.spawn_point = self.player.pos;
        if !self.creative {
            self.inventory.wear_tool(&reg, self.input.hotbar_sel);
        }
        self.save_player();
        self.server.world.settle_falling();
        self.server.world.save_modified();
        self.toast("You camp until dawn. This is home now.".to_string());
        self.sfx(Sfx::Craft);
    }
}
