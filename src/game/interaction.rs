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

    /// The attunement sidecar for the current world (local knowledge —
    /// what this player's feet have actually touched).
    fn attune_path(&self) -> std::path::PathBuf {
        self.server.world.save_dir_for_saving().join("attuned.tsv")
    }

    pub(super) fn load_attunements(&mut self) {
        self.interaction.attuned.clear();
        if let Ok(text) = std::fs::read_to_string(self.attune_path()) {
            for l in text.lines() {
                let mut parts = l.splitn(3, '\t');
                if let (Some(x), Some(z), Some(name)) = (
                    parts.next().and_then(|v| v.parse().ok()),
                    parts.next().and_then(|v| v.parse().ok()),
                    parts.next(),
                ) {
                    self.interaction.attuned.push((name.to_string(), x, z));
                }
            }
        }
    }

    fn save_attunements(&self) {
        use std::fmt::Write as _;
        let mut out = String::new();
        for (name, x, z) in &self.interaction.attuned {
            let _ = writeln!(out, "{x}\t{z}\t{name}");
        }
        let _ = std::fs::create_dir_all("saves");
        let _ = std::fs::write(self.attune_path(), out);
    }

    /// Touch a waystone: learn it, then hear where the others stand.
    pub(super) fn read_waystone(&mut self, pos: (i32, i32, i32)) {
        let name = match self.server.world.block_entity(&pos) {
            Some(world::BlockEntity::Sign(sg)) if !sg.lines[0].is_empty() => sg.lines[0].clone(),
            _ => {
                self.toast("The stone is unnamed. Write it first.".to_string());
                return;
            }
        };
        let known = self
            .interaction
            .attuned
            .iter()
            .any(|(_, x, z)| (*x, *z) == (pos.0, pos.2));
        if !known {
            self.interaction.attuned.push((name.clone(), pos.0, pos.2));
            self.save_attunements();
            self.toast(format!("The stone at {name} knows you now."));
        }
        let mut lines: Vec<String> = Vec::new();
        for (other, x, z) in &self.interaction.attuned {
            if (*x, *z) == (pos.0, pos.2) {
                continue;
            }
            let d = (((x - pos.0).pow(2) + (z - pos.2).pow(2)) as f32).sqrt() as i32;
            lines.push(format!(
                "{}: ~{d} blocks {}",
                other.to_uppercase(),
                Self::octant((x - pos.0, z - pos.2))
            ));
        }
        if lines.is_empty() {
            self.toast("It hums alone. Touch other stones.".to_string());
        }
        for l in lines {
            self.toast(l);
        }
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
