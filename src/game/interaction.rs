//! Player interaction, combat, stations, and script-driven actions.

use super::*;

impl Game {
    /// A line from the lost takers, chosen at random.
    pub(super) fn read_tablet(&mut self) {
        const LINES: [&str; 18] = [
            "We burned the south wood. Something started coming at night.",
            "The forge ran hot for a year. Then the trees walked.",
            "Plant after you cut. My father knew. I forgot.",
            "The wisps came the week we cleared the ridge. Nobody said it out loud.",
            "We left the stone offerings too late.",
            "The deep ones never slept. We dug anyway.",
            "Third winter: the wisps crossed the river.",
            "Feed the land. We stopped, and it came for the rest.",
            "My daughter planted a row of oaks. They spared her field.",
            "If you read this: the wild forgives. Slowly.",
            // The takers' confession, in the order it happened.
            "The walls held. Every night, the walls held.",
            "We farmed their rage. Heartwood pays better than wheat.",
            "Kellen says the raids come from the old tree. Kellen is right.",
            "We took axes to the bole at dawn. It took all morning.",
            "No wardens tonight. None the next. The children slept.",
            "We did the same in the east valley, and the north.",
            "Fourth summer: the burning ground gives us nothing.",
            "The stone will not take the offering. It just sits there.",
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
            let mut lines = text.lines();
            if lines.next() != Some("version\t2") {
                return; // old planar knowledge is deliberately not reinterpreted
            }
            for line in lines {
                let mut parts = line.splitn(4, '\t');
                if let (Some(face), Some(u), Some(v), Some(name)) = (
                    parts.next().and_then(crate::planet::Face::from_name),
                    parts.next().and_then(|value| value.parse().ok()),
                    parts.next().and_then(|value| value.parse().ok()),
                    parts.next(),
                ) && let Ok(surface) = crate::planet::SurfacePos::new(face, u, v)
                {
                    self.interaction.attuned.push((name.to_string(), surface));
                }
            }
        }
    }

    fn save_attunements(&self) -> std::io::Result<()> {
        use std::fmt::Write as _;
        let mut out = String::from("version\t2\n");
        for (name, surface) in &self.interaction.attuned {
            let _ = writeln!(
                out,
                "{}\t{}\t{}\t{name}",
                surface.face(),
                surface.u(),
                surface.v()
            );
        }
        crate::persist::atomic_write(&self.attune_path(), out.as_bytes(), false)
    }

    /// Touch a waystone: learn it, then hear where the others stand.
    pub(super) fn read_waystone(&mut self, pos: crate::planet::BlockPos) {
        let name = match self.server.world.block_entity_at(&pos) {
            Some(world::BlockEntity::Sign(sg)) if !sg.lines[0].is_empty() => sg.lines[0].clone(),
            _ => {
                self.toast("The stone is unnamed. Write it first.".to_string());
                return;
            }
        };
        let surface = pos.surface();
        let known = self
            .interaction
            .attuned
            .iter()
            .any(|(_, known)| *known == surface);
        if !known {
            self.interaction.attuned.push((name.clone(), surface));
            match self.save_attunements() {
                Ok(()) => self.toast(format!("The stone at {name} knows you now.")),
                Err(error) => {
                    self.interaction.attuned.pop();
                    self.toast(format!("The stone could not remember you: {error}"));
                }
            }
        }
        let mut lines: Vec<String> = Vec::new();
        for (other, other_surface) in &self.interaction.attuned {
            if *other_surface == surface {
                continue;
            }
            let from = surface.center();
            let to = other_surface.center();
            let distance = crate::planet::geodesic_distance(from, to).round() as i32;
            let direction = crate::planet::great_circle_bearing(from, to).map(|bearing| {
                const NAMES: [&str; 8] = [
                    "north",
                    "northeast",
                    "east",
                    "southeast",
                    "south",
                    "southwest",
                    "west",
                    "northwest",
                ];
                let octant = ((bearing.to_degrees() + 22.5).rem_euclid(360.0) / 45.0) as usize;
                NAMES[octant]
            });
            lines.push(match direction {
                Some(direction) => {
                    format!("{}: ~{distance} blocks {direction}", other.to_uppercase())
                }
                None => format!(
                    "{}: ~{distance} blocks; bearing uncertain",
                    other.to_uppercase()
                ),
            });
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
