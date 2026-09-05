//! Waystones interaction adapter.

use crate::world::TerrainRead;
use crate::world;
use crate::game::Game;

impl Game {

    /// The attunement sidecar for the current world (local knowledge —
    /// what this player's feet have actually touched).
    pub(in crate::game) fn attune_path(&self) -> std::path::PathBuf {
        self.runtime.player_sidecar_dir().join("attuned.tsv")
    }

    pub(in crate::game) fn load_attunements(&mut self) {
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

    pub(in crate::game) fn save_attunements(&self) -> std::io::Result<()> {
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
    pub(in crate::game) fn read_waystone(&mut self, pos: crate::planet::BlockPos) {
        let surface = pos.surface();
        let name = match self.runtime.view().block_entity_at(&pos) {
            Some(world::BlockEntity::Sign(sg)) if !sg.lines[0].is_empty() => sg.lines[0].clone(),
            _ => {
                let atlas_name =
                    self.runtime.view().planet_atlas().and_then(|atlas| {
                        atlas.hydrological_name_at(surface).map(ToOwned::to_owned)
                    });
                let Some(atlas_name) = atlas_name else {
                    self.toast("The stone is unnamed. Write it first.".to_string());
                    return;
                };
                atlas_name
            }
        };
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
}
