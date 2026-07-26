//! The hearts of the land: one spirit per province, living in a
//! findable site.
//!
//! The regional ire ledger says how a country feels; the heart is
//! WHERE that feeling lives. It can be found, read at a glance,
//! sickened, killed — and the wardens are its immune response, so
//! its death is the quietest catastrophe in the game.

use super::*;

/// Compass octant of an offset ("north" is -z).
fn octant_of(dx: i32, dz: i32) -> &'static str {
    if dx == 0 && dz == 0 {
        return "here";
    }
    let a = (dx as f32).atan2(-(dz as f32)).to_degrees();
    [
        "north",
        "northeast",
        "east",
        "southeast",
        "south",
        "southwest",
        "west",
        "northwest",
    ][(((a + 382.5) / 45.0) as usize) % 8]
}

/// A country's spirit, keyed on its province.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Heart {
    pub pos: (i32, i32, i32),
    /// 2 = alive, 1 = sickening, 0 = dead.
    pub stage: u8,
    /// Accumulated grievance, in days held under resentment. The
    /// sickening is slow on purpose: a country takes seasons to die.
    pub strain: f32,
}

impl Heart {
    pub fn alive(&self) -> bool {
        self.stage > 0
    }
}

/// Which form a country's heart wears. Wooded country grows a bole,
/// dry country keeps a spring that should not be there, and the cold
/// and open ground raises a stone.
pub fn heart_form(biome: crate::worldgen::Biome) -> &'static str {
    use crate::worldgen::Biome as B;
    match biome {
        B::Forest | B::Taiga | B::Jungle | B::Swamp => "base:heart_tree",
        B::Desert | B::Badlands | B::Savanna | B::Scrubland => "base:heart_spring",
        _ => "base:heart_stone",
    }
}

/// The block a form wears at a given stage.
pub fn heart_block_name(form: &str, stage: u8) -> String {
    match stage {
        2 => form.to_string(),
        1 => format!("{form}_sick"),
        _ => format!("{form}_dead"),
    }
}

/// How tall the site stands (a bole is a landmark; a spring is not).
pub fn heart_height(form: &str) -> i32 {
    match form {
        "base:heart_tree" => 5,
        "base:heart_stone" => 3,
        _ => 1,
    }
}

/// A country held under this much resentment starts to strain.
pub const HEART_STRAIN_IRE: f32 = 8.0;
/// Strain accrues at `standing / HEART_STRAIN_IRE` per day, so the
/// angriest country (regional ire caps at 20) gains 2.5 a day: a
/// season of unbroken grievance to show it, two and a half to die
/// of it. Tended country sheds 1.5 a day. A spirit takes SEASONS.
pub const HEART_SICKEN_STRAIN: f32 = 30.0;
pub const HEART_DEATH_STRAIN: f32 = 75.0;

impl World {
    /// The slow clock of the spirits: a country held in resentment
    /// strains, and strain shows before it kills. Tended country
    /// bleeds it off. Death is not on this clock's way back — a dead
    /// heart is restored by hand, never by waiting.
    pub(super) fn tick_hearts(&mut self, day_frac: f32) {
        let keys: Vec<(i32, i32)> = self.hearts.keys().copied().collect();
        for key in keys {
            let Some(h) = self.hearts.get(&key).copied() else {
                continue;
            };
            if h.stage == 0 {
                continue;
            }
            let standing = self.regional_ire_at(h.pos.0, h.pos.2);
            let strain = if standing >= HEART_STRAIN_IRE {
                // Deeper grievance sickens faster, but never fast.
                h.strain + day_frac * (standing / HEART_STRAIN_IRE)
            } else {
                (h.strain - day_frac * 1.5).max(0.0)
            };
            if let Some(e) = self.hearts.get_mut(&key) {
                e.strain = strain;
            }
            let want = if strain >= HEART_DEATH_STRAIN {
                0
            } else if strain >= HEART_SICKEN_STRAIN {
                1
            } else {
                2
            };
            if want != h.stage {
                self.set_heart_stage(key, want);
            }
        }
    }

    /// A heart cut down. The raids stop that night — which is the
    /// whole trap: it WORKS, and the country never gives again.
    pub(super) fn heart_struck(&mut self, pos: (i32, i32, i32)) {
        let key = self.generator.province(pos.0, pos.2).key;
        let Some(h) = self.hearts.get(&key).copied() else {
            return;
        };
        if h.stage == 0 {
            return;
        }
        if let Some(e) = self.hearts.get_mut(&key) {
            e.stage = 0;
            e.strain = HEART_DEATH_STRAIN;
        }
        // Take the rest of the site down with it: a half-cut heart is
        // not a thing, and the husk is what the country wears now.
        let form = heart_form(self.generator.biome(h.pos.0, h.pos.2));
        if let Some(dead) = self.reg.block_id(&heart_block_name(form, 0)) {
            for dy in 0..heart_height(form) {
                let at = (h.pos.0, h.pos.1 + dy, h.pos.2);
                let name = self
                    .reg
                    .block(self.get_block(at.0, at.1, at.2))
                    .name
                    .clone();
                if name.starts_with("base:heart_") {
                    self.set_block(at.0, at.1, at.2, dead);
                }
            }
        }
    }
    /// The heart of the country a position stands in, if its site has
    /// been generated yet.
    pub fn heart_at(&self, x: i32, z: i32) -> Option<Heart> {
        let key = self.generator.province(x, z).key;
        self.hearts.get(&key).copied()
    }

    /// Does the country here still have a spirit? Unvisited country
    /// counts as living — the wild is presumed well until seen
    /// otherwise, and a heart registers the moment its chunk loads.
    pub fn heart_alive_at(&self, x: i32, z: i32) -> bool {
        self.heart_at(x, z).is_none_or(|h| h.alive())
    }

    /// Record a heart the generator has just laid down.
    pub(crate) fn register_heart(&mut self, key: (i32, i32), pos: (i32, i32, i32)) {
        self.hearts.entry(key).or_insert(Heart {
            pos,
            stage: 2,
            strain: 0.0,
        });
    }

    /// Set a heart's stage and swap the blocks at its site to match.
    pub fn set_heart_stage(&mut self, key: (i32, i32), stage: u8) {
        let Some(mut h) = self.hearts.get(&key).copied() else {
            return;
        };
        if h.stage == stage {
            return;
        }
        h.stage = stage;
        self.hearts.insert(key, h);
        let biome = self.generator.biome(h.pos.0, h.pos.2);
        let form = heart_form(biome);
        let Some(want) = self.reg.block_id(&heart_block_name(form, stage)) else {
            return;
        };
        for dy in 0..heart_height(form) {
            let at = (h.pos.0, h.pos.1 + dy, h.pos.2);
            let here = self.get_block(at.0, at.1, at.2);
            let name = self.reg.block(here).name.clone();
            // Only rewrite the heart's own blocks: whatever a player
            // has built around the site is theirs.
            if name.starts_with("base:heart_") {
                self.set_block(at.0, at.1, at.2, want);
            }
        }
    }

    /// The compass reading a survey cairn gives for the country's
    /// heart: where it stands and how it fares.
    pub fn heart_report(&self, x: i32, z: i32) -> String {
        let Some(h) = self.heart_at(x, z) else {
            return "The heart of this country lies beyond your maps.".into();
        };
        let (dx, dz) = (h.pos.0 - x, h.pos.2 - z);
        let dist = ((dx * dx + dz * dz) as f32).sqrt().round() as i32;
        let dir = octant_of(dx, dz);
        let state = match h.stage {
            2 if h.strain > 4.0 => "and it is uneasy",
            2 => "and it is well",
            1 => "and it is FAILING",
            _ => "and it is dead",
        };
        if dist <= 8 {
            format!("The heart of this country is here — {state}.")
        } else {
            format!("The heart of this country lies ~{dist} blocks {dir}, {state}.")
        }
    }
}
