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
    /// Days a planted seed has been taking, 0 when none is.
    pub rooting: f32,
    /// The biome a rooting seed was cut from. When it takes and that
    /// differs from this country's own, the country begins to BECOME
    /// what the donor was — restoration doubles as terraforming.
    pub graft: Option<crate::worldgen::Biome>,
    /// How far the country has drifted toward its graft, 0..1.
    pub drift: f32,
    /// Days before it will give another cutting. A living heart parts
    /// with one seed and then has nothing to spare for half a season —
    /// without this it was a dispenser you could stand at and click,
    /// trading ire for seeds as fast as the action cooldown allowed.
    pub regrow: f32,
}

impl Heart {
    pub fn alive(&self) -> bool {
        self.stage > 0
    }
}

/// Which shape a country's heart wears. Twelve countries, twelve
/// spirits: wooded ground grows a bole, dry ground keeps a spring that
/// should not be there, and cold or open ground raises a stone. The
/// forms are generated alongside the blocks and tiles from
/// tools/heart_table.py — that table is the source, not this match.
pub fn heart_form(biome: crate::worldgen::Biome) -> &'static str {
    use crate::worldgen::Biome as B;
    match biome {
        B::Forest => "base:heart_forest",
        B::Taiga => "base:heart_taiga",
        B::Jungle => "base:heart_jungle",
        B::Swamp => "base:heart_swamp",
        B::Desert => "base:heart_desert",
        B::Badlands => "base:heart_badlands",
        B::Savanna => "base:heart_savanna",
        B::Scrubland => "base:heart_scrubland",
        B::Plains => "base:heart_plains",
        B::Tundra => "base:heart_tundra",
        B::Arctic => "base:heart_arctic",
        B::Mountains => "base:heart_mountains",
        // Not a province culture; no country is ever labelled with it.
        B::Ocean => "base:heart_plains",
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

/// How tall the site stands. A bole is a landmark you can see across a
/// valley; a spring is a thing you nearly walk past.
pub fn heart_height(form: &str) -> i32 {
    match form {
        "base:heart_forest" => 5,
        "base:heart_taiga" => 5,
        "base:heart_jungle" => 5,
        "base:heart_swamp" => 5,
        "base:heart_desert" => 1,
        "base:heart_badlands" => 1,
        "base:heart_savanna" => 1,
        "base:heart_scrubland" => 1,
        "base:heart_plains" => 3,
        "base:heart_tundra" => 3,
        "base:heart_arctic" => 3,
        "base:heart_mountains" => 3,
        _ => 1,
    }
}

/// The seed a form gives.
pub fn seed_of_form(form: &str) -> &'static str {
    match form {
        "base:heart_forest" => "base:forest_seed",
        "base:heart_taiga" => "base:taiga_seed",
        "base:heart_jungle" => "base:jungle_seed",
        "base:heart_swamp" => "base:swamp_seed",
        "base:heart_desert" => "base:desert_seed",
        "base:heart_badlands" => "base:badlands_seed",
        "base:heart_savanna" => "base:savanna_seed",
        "base:heart_scrubland" => "base:scrubland_seed",
        "base:heart_plains" => "base:plains_seed",
        "base:heart_tundra" => "base:tundra_seed",
        "base:heart_arctic" => "base:arctic_seed",
        "base:heart_mountains" => "base:mountains_seed",
        _ => "base:plains_seed",
    }
}

/// What a seed wakes: the country a grafted heart drifts toward. This
/// is the terraforming lever — carry a jungle cutting into a dead
/// desert and the country becomes jungle.
pub fn seed_nature(item_name: &str) -> Option<crate::worldgen::Biome> {
    use crate::worldgen::Biome as B;
    Some(match item_name {
        "base:forest_seed" => B::Forest,
        "base:taiga_seed" => B::Taiga,
        "base:jungle_seed" => B::Jungle,
        "base:swamp_seed" => B::Swamp,
        "base:desert_seed" => B::Desert,
        "base:badlands_seed" => B::Badlands,
        "base:savanna_seed" => B::Savanna,
        "base:scrubland_seed" => B::Scrubland,
        "base:plains_seed" => B::Plains,
        "base:tundra_seed" => B::Tundra,
        "base:arctic_seed" => B::Arctic,
        "base:mountains_seed" => B::Mountains,
        _ => return None,
    })
}

/// How far around a dead site the ground must be made ready.
pub const ROOT_RADIUS: i32 = 6;
/// The share of that ground that must be living soil before a seed
/// will take: the whole nutrient cycle, spent as a key.
pub const ROOT_READY_FRAC: f32 = 0.55;
/// Fertility a cell must reach to count as made ready.
pub const ROOT_READY_FERT: u8 = 24;
/// A rooting takes a season, defended.
pub const ROOT_DAYS: f32 = SEASON_DAYS as f32;

/// A country held under this much resentment starts to strain.
pub const HEART_STRAIN_IRE: f32 = 8.0;
/// The most a country can strain in a day: regional standing caps at
/// 20, and strain accrues at `standing / HEART_STRAIN_IRE`.
const HEART_STRAIN_MAX_PER_DAY: f32 = 20.0 / HEART_STRAIN_IRE;
/// A season of unbroken grievance to show it, two and a half to die
/// of it. Tended country sheds 1.5 a day. A spirit takes SEASONS —
/// which is why these are written against the season and not as bare
/// day counts: the calendar can be retuned without quietly turning
/// the spirits' clock with it.
pub const HEART_SICKEN_STRAIN: f32 = HEART_STRAIN_MAX_PER_DAY * SEASON_DAYS as f32;
pub const HEART_DEATH_STRAIN: f32 = HEART_SICKEN_STRAIN * 2.5;

/// Days a living heart needs before it will give another cutting.
/// Half a season: a seed is an errand you make a journey for, not a
/// thing you farm by standing still. You only ever need one per dead
/// country, so this costs an honest restoration nothing.
pub const HEART_CUTTING_DAYS: f32 = SEASON_DAYS as f32 / 2.0;

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
                e.regrow = (e.regrow - day_frac).max(0.0);
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

    /// Ask a living heart for a cutting. It parts with one, then wants
    /// half a season before it will part with another. Returns false
    /// when it has nothing to spare — the caller says so, and charges
    /// no ire for the asking.
    pub fn take_heart_cutting(&mut self, x: i32, z: i32) -> bool {
        let key = self.generator.province(x, z).key;
        match self.hearts.get_mut(&key) {
            Some(h) if h.stage == 2 && h.regrow <= 0.0 => {
                h.regrow = HEART_CUTTING_DAYS;
                true
            }
            _ => false,
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
        self.orphan_wardens(h.pos.0, h.pos.2);
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
        let ancient = self.is_ancient_scar(pos.0, pos.2);
        self.hearts.entry(key).or_insert(Heart {
            pos,
            // The badlands were not always badlands. Their spirit went
            // out long before anyone alive walked there, which is why
            // nothing grows and why the takers' cities stand intact in
            // ground that stopped feeding them. Everywhere else, a
            // country met for the first time is well.
            stage: if ancient { 0 } else { 2 },
            strain: if ancient { HEART_DEATH_STRAIN } else { 0.0 },
            rooting: 0.0,
            graft: None,
            drift: 0.0,
            // A heart found for the first time has a cutting to spare.
            regrow: 0.0,
        });
        // Old saves carry the three archetype hearts, aliased into the
        // country whose shape they matched — a taiga bole came back as
        // heart_forest. Re-reading the site on load heals that, and any
        // future change to a country's form, without a migration pass.
        let stage = self.hearts.get(&key).map(|h| h.stage).unwrap_or(2);
        self.reface_heart_site(pos, stage);
    }

    /// Country whose spirit died before the world began. Read from the
    /// map rather than stored: badlands ARE the scar, so the answer
    /// cannot drift out of step with the terrain that shows it.
    pub fn is_ancient_scar(&self, x: i32, z: i32) -> bool {
        self.generator.province(x, z).biome == crate::worldgen::Biome::Badlands
    }

    /// Make the blocks at a site match the form and stage its country
    /// wears now.
    fn reface_heart_site(&mut self, pos: (i32, i32, i32), stage: u8) {
        let form = heart_form(self.generator.biome(pos.0, pos.2));
        let Some(want) = self.reg.block_id(&heart_block_name(form, stage)) else {
            return;
        };
        for dy in 0..heart_height(form) {
            let at = (pos.0, pos.1 + dy, pos.2);
            let here = self.get_block(at.0, at.1, at.2);
            if here != want && self.reg.block(here).name.starts_with("base:heart_") {
                self.set_block(at.0, at.1, at.2, want);
            }
        }
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
        if stage == 0 {
            self.orphan_wardens(h.pos.0, h.pos.2);
        }
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

    /// Is the ground around a dead site made ready? A heart will not
    /// root in dead dirt: the soil has to be raised by hand first —
    /// dung, compost, guano, litter, fallow seasons — which is the
    /// whole ecology arc spent as a key.
    pub fn root_ground_ready(&self, x: i32, z: i32) -> (u32, u32) {
        let mut ready = 0;
        let mut total = 0;
        for dx in -ROOT_RADIUS..=ROOT_RADIUS {
            for dz in -ROOT_RADIUS..=ROOT_RADIUS {
                if dx * dx + dz * dz > ROOT_RADIUS * ROOT_RADIUS {
                    continue;
                }
                let (cx, cz) = (x + dx, z + dz);
                let y = self.surface_height(cx, cz);
                total += 1;
                if self.fertility_at(cx, y, cz) >= ROOT_READY_FERT {
                    ready += 1;
                }
            }
        }
        (ready, total)
    }

    /// Plant a quickened seed at a dead site. `from` is the country
    /// the seed was cut in: its own means a reawakening, a stranger's
    /// means a replacement that brings its own nature with it.
    /// Returns the refusal to say out loud, or None when the rooting
    /// has begun.
    pub fn plant_heart_seed_from(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        from: Option<crate::worldgen::Biome>,
    ) -> Option<String> {
        let refusal = self.plant_heart_seed(x, y, z);
        if refusal.is_none() {
            let key = self.generator.province(x, z).key;
            let native = self.generator.province(x, z).biome;
            if let Some(e) = self.hearts.get_mut(&key) {
                // Same family: a reawakening, and the country keeps
                // its own nature. A stranger's: a replacement.
                e.graft = from.filter(|b| heart_form(*b) != heart_form(native));
            }
        }
        refusal
    }

    /// Plant a quickened seed at a dead site. Returns the refusal to
    /// say out loud, or None when the rooting has begun.
    pub fn plant_heart_seed(&mut self, x: i32, y: i32, z: i32) -> Option<String> {
        let key = self.generator.province(x, z).key;
        let Some(h) = self.hearts.get(&key).copied() else {
            return Some("No country's heart ever stood here.".into());
        };
        if h.stage != 0 {
            return Some("This country still has a spirit.".into());
        }
        if (h.pos.0 - x).abs() > 3 || (h.pos.2 - z).abs() > 3 {
            return Some("It must go where the old heart stood.".into());
        }
        let (ready, total) = self.root_ground_ready(h.pos.0, h.pos.2);
        if (ready as f32) < total as f32 * ROOT_READY_FRAC {
            return Some(format!(
                "The ground is not ready ({ready} of {total} plots living)."
            ));
        }
        if let Some(e) = self.hearts.get_mut(&key) {
            e.rooting = 0.01;
            e.pos = (h.pos.0, y, h.pos.2);
        }
        None
    }

    /// The rooting clock: a planted seed takes a season to take, and
    /// only in ground kept ready.
    pub(super) fn tick_rooting(&mut self, day_frac: f32) {
        let keys: Vec<(i32, i32)> = self
            .hearts
            .iter()
            .filter(|(_, h)| h.rooting > 0.0)
            .map(|(k, _)| *k)
            .collect();
        for key in keys {
            let Some(h) = self.hearts.get(&key).copied() else {
                continue;
            };
            let (ready, total) = self.root_ground_ready(h.pos.0, h.pos.2);
            if (ready as f32) < total as f32 * ROOT_READY_FRAC * 0.75 {
                // Let the ground go and the seed goes with it.
                if let Some(e) = self.hearts.get_mut(&key) {
                    e.rooting = 0.0;
                }
                continue;
            }
            let done = h.rooting + day_frac >= ROOT_DAYS;
            if let Some(e) = self.hearts.get_mut(&key) {
                e.rooting = if done { 0.0 } else { e.rooting + day_frac };
                if done {
                    e.strain = 0.0;
                }
            }
            if done {
                self.set_heart_stage(key, 2);
                // The country wakes: its wardens answer to it again.
                for m in &mut self.mobs {
                    m.masterless = false;
                }
                if let Some(e) = self.hearts.get_mut(&key) {
                    e.drift = 0.0;
                }
                // A reawakened country remembers who did it, and the
                // ground it stands on is blessed for good. A grafted
                // one wakes a stranger, and knows nothing of you.
                if self.hearts.get(&key).and_then(|e| e.graft).is_none() {
                    self.plant_ire_at(h.pos.0, h.pos.2, 6.0);
                }
            }
        }
    }

    /// A grafted country drifts toward the nature it was given, a
    /// season at a time. This is the terraforming: what grows, what
    /// spawns, what the ground is, all follow the heart.
    pub(super) fn tick_graft(&mut self, day_frac: f32) {
        for h in self.hearts.values_mut() {
            if h.stage == 2 && h.graft.is_some() && h.drift < 1.0 {
                h.drift = (h.drift + day_frac / (2.0 * SEASON_DAYS as f32)).min(1.0);
            }
        }
    }

    /// What a country counts as now: its own nature, or the one its
    /// heart was grafted from once the drift has carried far enough.
    pub fn country_biome(&self, x: i32, z: i32) -> crate::worldgen::Biome {
        let key = self.generator.province(x, z).key;
        match self.hearts.get(&key) {
            Some(h) if h.stage == 2 && h.drift >= 0.5 && h.graft.is_some() => h.graft.unwrap(),
            // Ungrafted country reads exactly as the map does — the
            // column's own label, fringe dither and terrain veto and
            // all. Only a graft overrides it.
            _ => self.generator.biome(x, z),
        }
    }

    /// What the place you are standing in *is*, which is not always what
    /// its country is: a column drowned to sea level is Ocean however
    /// the forest behind you is labelled. The country keeps its culture
    /// (a coastal province is still a forest province, and its heart
    /// still knows what it grows) — this is only the ground underfoot.
    pub fn biome_here(&self, x: i32, z: i32) -> crate::worldgen::Biome {
        if self.is_open_water(x, z) {
            return crate::worldgen::Biome::Ocean;
        }
        self.country_biome(x, z)
    }

    /// Sea, not puddle: the column's floor lies below sea level and the
    /// water over it reaches sea level. A dug pond on a hillside fails
    /// the first test; a one-deep tidal scrape fails the second.
    pub fn is_open_water(&self, x: i32, z: i32) -> bool {
        let floor = self.surface_height(x, z);
        floor < crate::chunk::SEA_LEVEL - 1
            && self
                .reg
                .is_water(self.get_block(x, crate::chunk::SEA_LEVEL - 1, z))
    }

    /// The wardens caught mid-existence when their heart died. They
    /// were never recalled and never will be.
    fn orphan_wardens(&mut self, x: i32, z: i32) {
        let key = self.generator.province(x, z).key;
        let reg = self.reg.clone();
        let g = &self.generator;
        for m in &mut self.mobs {
            if reg.animals.get(m.species).is_some_and(|d| d.hostile)
                && g.province(m.pos.x.floor() as i32, m.pos.z.floor() as i32)
                    .key
                    == key
            {
                m.masterless = true;
                m.watcher = false;
            }
        }
    }

    /// Where the nearest ground that would take a cutting lies, as a
    /// bearing you can walk. A seed knows where it is needed, which is
    /// the only navigation the game can offer at province range: 900
    /// blocks between countries and a view of a few hundred.
    ///
    /// Two kinds of site qualify and both are knowable without having
    /// been there — a country whose heart this world has watched die,
    /// and a badlands scar, which was dead before anyone walked it.
    pub fn seed_bearing(&self, from: glam::Vec3) -> String {
        let (fx, fz) = (from.x.floor() as i32, from.z.floor() as i32);
        let here = self.generator.province(fx, fz).key;
        let mut best: Option<(f32, i32, i32, bool)> = None;
        // Six provinces out is ~5000 blocks: further than anyone walks
        // in one errand, and cheap because a centre is pure arithmetic.
        for kx in -6..=6 {
            for kz in -6..=6 {
                let key = (here.0 + kx, here.1 + kz);
                let (sx, sz) = self.generator.province_center(key.0, key.1);
                let ancient = self.is_ancient_scar(sx, sz);
                let known_dead = self.hearts.get(&key).is_some_and(|h| h.stage == 0);
                if !ancient && !known_dead {
                    continue;
                }
                let d = (((sx - fx) as f32).powi(2) + ((sz - fz) as f32).powi(2)).sqrt();
                if best.is_none_or(|(b, _, _, _)| d < b) {
                    best = Some((d, sx, sz, ancient));
                }
            }
        }
        let Some((d, sx, sz, ancient)) = best else {
            return "It stirs, and finds nowhere that needs it.".into();
        };
        if d < 12.0 {
            return "It strains in your hand. The ground it wants is here.".into();
        }
        let dir = octant_of(sx - fx, sz - fz);
        let far = if ancient {
            "a country that died before your grandfathers"
        } else {
            "a country you watched go out"
        };
        format!("It leans {dir} — {far}, ~{} blocks.", d.round() as i32)
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
            // A scar is not a bereavement. The badlands lost their
            // spirit before anyone alive walked there, and a cairn
            // that says so is the first thread of the whole story.
            _ if self.is_ancient_scar(h.pos.0, h.pos.2) => {
                "and it died long before these stones were cut"
            }
            _ => "and it is dead",
        };
        // Name the shape, because they are no longer all alike: you
        // are looking for a particular thing now, not "a heart".
        let form = heart_form(self.generator.biome(h.pos.0, h.pos.2));
        let what = self
            .reg
            .block_id(&heart_block_name(form, h.stage))
            .map(|b| format!(" — a {}", self.reg.block(b).label))
            .unwrap_or_default();
        if dist <= 8 {
            format!("The heart of this country is here{what}, {state}.")
        } else {
            format!("The heart of this country lies ~{dist} blocks {dir}{what}, {state}.")
        }
    }
}
