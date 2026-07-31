//! Seasons, weather locality, ire, offerings, and renewable growth.

use super::*;

impl World {
    /// 0 spring, 1 summer, 2 autumn, 3 winter.
    pub fn season(&self) -> usize {
        // The Long Winter: enough countries dead and the year stops
        // turning. Everything winter already means — crops at zero,
        // no breeding, halved repopulation, water freezing — arrives
        // for free, because it IS winter, world-wide, until enough
        // hearts are relit.
        if self.long_winter {
            return 3;
        }
        ((self.day / SEASON_DAYS) % 4) as usize
    }

    /// How many known countries have lost their spirit, and how many
    /// are known at all.
    pub fn dead_countries(&self) -> (usize, usize) {
        // Ancient scars do not count, on either side of the ratio. The
        // badlands died before anyone alive walked there; letting them
        // into the tally would stop the world's year over history the
        // player never touched — walk through three of them early and
        // the Long Winter would fall on a world you had done nothing
        // to. Relight one and it becomes a living country like any
        // other, which is the right way for it to help lift a winter.
        let counted = self
            .hearts
            .values()
            .filter(|h| !self.is_ancient_scar_at(h.pos.surface()));
        let (mut dead, mut known) = (0, 0);
        for h in counted {
            known += 1;
            if h.stage == 0 {
                dead += 1;
            }
        }
        (dead, known)
    }

    /// Re-read whether the world's year has stopped. Returns Some(true)
    /// when the Long Winter falls and Some(false) when it lifts.
    pub(super) fn refresh_long_winter(&mut self) -> Option<bool> {
        let (dead, known) = self.dead_countries();
        // A handful of dead countries is a tragedy, not a winter; it
        // takes both a real count and a real share of the known world.
        let falls = dead >= LONG_WINTER_MIN_DEAD
            && known > 0
            && dead as f32 >= known as f32 * LONG_WINTER_FRAC;
        if falls == self.long_winter {
            return None;
        }
        self.long_winter = falls;
        Some(falls)
    }

    /// 0..1 through the current season.
    pub fn season_progress(&self) -> f32 {
        (self.day % SEASON_DAYS) as f32 / SEASON_DAYS as f32
    }

    /// Does precipitation fall as snow in this column? The threshold
    /// relaxes in winter so taiga and cold-temperate lands whiten.
    pub fn snows_at_surface(&self, pos: crate::planet::SurfacePos) -> bool {
        let t = self.generator.climate_at(pos).t;
        t < if self.season() == 3 { -0.05 } else { -0.35 }
    }

    /// Deserts stay dry: overcast skies, nothing falls.
    pub fn rains_at_surface(&self, pos: crate::planet::SurfacePos) -> bool {
        let c = self.generator.climate_at(pos);
        !(c.t > 0.6 && c.h < -0.5)
    }

    // ---------------- ire (reciprocity) ----------------

    /// The land's local standing at a canonical planetary surface cell.
    pub fn regional_ire_at_surface(&self, pos: crate::planet::SurfacePos) -> f32 {
        self.regional_ire
            .get(&RegionCell::from_surface(pos))
            .copied()
            .unwrap_or(0.0)
    }

    /// The land's local standing: negative is tended, positive is
    /// aggrieved, clamped to a grudge the wild can actually hold.
    #[cfg(test)]
    pub fn regional_ire_at(&self, x: i32, z: i32) -> f32 {
        self.regional_ire_at_surface(
            crate::planet::SurfacePos::from_centered(crate::planet::Face::PosZ, x, z)
                .expect("legacy regional coordinate is within the bounded porting window"),
        )
    }

    fn charge_cell(&mut self, cell: RegionCell, amt: f32) {
        let e = self.regional_ire.entry(cell).or_insert(0.0);
        *e = (*e + amt).clamp(-20.0, 20.0);
        if e.abs() < 0.01 {
            self.regional_ire.remove(&cell);
        }
    }

    /// Taking, placed: the world remembers, and so does the valley.
    #[cfg(test)]
    pub fn add_ire_at(&mut self, x: i32, z: i32, amt: f32) {
        let pos = crate::planet::SurfacePos::from_centered(crate::planet::Face::PosZ, x, z)
            .expect("legacy regional coordinate is within the bounded porting window");
        self.add_ire_at_surface(pos, amt);
    }

    pub fn add_ire_at_surface(&mut self, pos: crate::planet::SurfacePos, amt: f32) {
        self.add_ire(amt);
        self.charge_cell(RegionCell::from_surface(pos), amt);
    }

    /// Mending, placed: the global refund keeps its daily cap, but the
    /// valley always notices the hands that tend it.
    #[cfg(test)]
    pub fn plant_ire_at(&mut self, x: i32, z: i32, amt: f32) {
        let pos = crate::planet::SurfacePos::from_centered(crate::planet::Face::PosZ, x, z)
            .expect("legacy regional coordinate is within the bounded porting window");
        self.plant_ire_at_surface(pos, amt);
    }

    pub fn plant_ire_at_surface(&mut self, pos: crate::planet::SurfacePos, amt: f32) {
        self.plant_ire(amt);
        self.charge_cell(RegionCell::from_surface(pos), -amt);
        // Tending is also how a cell earns back its bloom.
        self.ease_bloom_debt_at_surface(pos, amt);
    }

    // ---------------- the bloom (wrath as renewal) ----------------

    /// Days of bloom left in a cell: lightning strikes and fallen
    /// wardens charge it; charged country erupts — the green tide
    /// runs hot, flowers and fungi sprout, bushes refruit. The titan
    /// levels the valley and the jungle follows it home.
    #[cfg(test)]
    pub fn bloom_at(&self, x: i32, z: i32) -> f32 {
        let pos = crate::planet::SurfacePos::from_centered(crate::planet::Face::PosZ, x, z)
            .expect("legacy regional coordinate is within the bounded porting window");
        self.bloom_at_surface(pos)
    }

    pub fn bloom_at_surface(&self, pos: crate::planet::SurfacePos) -> f32 {
        self.bloom
            .get(&RegionCell::from_surface(pos))
            .copied()
            .unwrap_or(0.0)
    }

    /// Bank a bloom — but the ground's willingness is finite. A cell
    /// bloomed over and over and never tended gives less each time,
    /// and finally nothing: the storm's gift is not a faucet, and
    /// farming the wild's rage spends something real.
    #[cfg(test)]
    pub fn add_bloom(&mut self, x: i32, z: i32, days: f32) {
        let pos = crate::planet::SurfacePos::from_centered(crate::planet::Face::PosZ, x, z)
            .expect("legacy regional coordinate is within the bounded porting window");
        self.add_bloom_at_surface(pos, days);
    }

    pub fn add_bloom_at_surface(&mut self, pos: crate::planet::SurfacePos, days: f32) {
        let cell = RegionCell::from_surface(pos);
        let spent = self.bloom_spent.get(&cell).copied().unwrap_or(0.0);
        let yield_frac = (1.0 - spent / BLOOM_EXHAUSTION).clamp(0.0, 1.0);
        let given = days * yield_frac;
        if given <= 0.01 {
            return;
        }
        *self.bloom_spent.entry(cell).or_insert(0.0) += given;
        let e = self.bloom.entry(cell).or_insert(0.0);
        *e = (*e + given).min(9.0);
    }

    /// Tending pays the ground back its willingness to bloom.
    pub fn ease_bloom_debt_at_surface(&mut self, pos: crate::planet::SurfacePos, amount: f32) {
        let cell = RegionCell::from_surface(pos);
        if let Some(v) = self.bloom_spent.get_mut(&cell) {
            *v = (*v - amount).max(0.0);
            if *v <= 0.01 {
                self.bloom_spent.remove(&cell);
            }
        }
    }

    /// A hostile fell here: the wild reclaims its own, extravagantly.
    /// Dryads put up a sapling where they stood.
    #[cfg(test)]
    pub fn wild_falls(&mut self, species_name: &str, x: i32, y: i32, z: i32) {
        if let Some(pos) = crate::planet::BlockPos::of_world(x, y, z) {
            self.wild_falls_at(species_name, pos);
        }
    }

    pub fn wild_falls_at(&mut self, species_name: &str, pos: crate::planet::BlockPos) {
        self.add_bloom_at_surface(pos.surface(), 1.0);
        if species_name.contains("dryad")
            && self.get_block_at(pos) == AIR
            && pos.offset(0, -1, 0).is_some_and(|below| {
                self.reg
                    .block(self.get_block_at(below))
                    .name
                    .contains("grass")
            })
            && let Some(sap) = self.reg.block_id("base:oak_sapling")
        {
            self.set_block_at(pos, sap);
        }
    }

    /// The wild's own hand: a bolt out of an ire storm. Strikes only
    /// natural, untouched country; chars grass or dirt to max-fertile
    /// scorch and banks bloom in the cell. Returns the struck cell.
    pub fn lightning_strike_at(
        &mut self,
        surface: crate::planet::SurfacePos,
    ) -> Option<crate::planet::BlockPos> {
        let cp = crate::planet::ChunkPos::from_surface(surface);
        // The invariant, absolute: the wild never touches what
        // players BUILT — a touched chunk is off the target list.
        if self.player_touched.contains(&cp) {
            return None;
        }
        let y = self.surface_height_at(surface);
        if y <= 2 {
            return None;
        }
        let struck =
            crate::planet::BlockPos::new(surface.face(), surface.u(), y as u8, surface.v()).ok()?;
        let name = self.reg.block(self.get_block_at(struck)).name.clone();
        self.add_bloom_at_surface(surface, 3.0);
        if (name == "base:grass" || name == "base:dirt")
            && let Some(ch) = self.reg.block_id("base:charred_soil")
        {
            self.set_block_at(struck, ch);
        }
        // And it starts a fire, which is the wild's to own: it pays
        // bloom where it burns and will not cross onto worked ground.
        if let Some(above) = struck.offset(0, 1, 0) {
            self.light_fire_at(above, false);
        }
        Some(struck)
    }

    #[cfg(test)]
    pub fn lightning_strike(&mut self, x: i32, z: i32) -> Option<(i32, i32, i32)> {
        let surface =
            crate::planet::SurfacePos::from_centered(crate::planet::Face::PosZ, x, z).ok()?;
        self.lightning_strike_at(surface)
            .map(crate::planet::BlockPos::centered)
    }

    pub fn ire_tier(&self) -> usize {
        Self::tier_of(self.ire)
    }

    fn tier_of(ire: f32) -> usize {
        match ire {
            x if x < 20.0 => 0,
            x if x < 50.0 => 1,
            x if x < 80.0 => 2,
            _ => 3,
        }
    }

    /// The tier as this ground feels it: the world's mood shifted by
    /// the local ledger (±20 regional ≈ ±2 tiers — an angry forest is
    /// menacing, not lethal; a tended valley forgives a lot).
    #[cfg(test)]
    pub fn ire_tier_at(&self, x: i32, z: i32) -> usize {
        Self::tier_of((self.ire + self.regional_ire_at(x, z) * 3.0).clamp(0.0, 100.0))
    }

    pub fn ire_tier_at_surface(&self, pos: crate::planet::SurfacePos) -> usize {
        Self::tier_of((self.ire + self.regional_ire_at_surface(pos) * 3.0).clamp(0.0, 100.0))
    }

    pub fn add_ire(&mut self, amt: f32) {
        self.ire = (self.ire + amt).clamp(0.0, 100.0);
    }

    /// Planting refunds ire, capped per day — mending stays slower than
    /// taking; a clearcut can't be laundered with a seed drawer.
    pub fn plant_ire(&mut self, amt: f32) {
        let room = (8.0 - self.plant_ire_today).max(0.0);
        let refund = amt.min(room);
        if refund > 0.0 {
            self.plant_ire_today += refund;
            self.add_ire(-refund);
        }
    }

    /// Advance ire time by a fraction of a day: passive decay (-4/day)
    /// and the daily reset of the planting cap. Returns true at dawn
    /// (day rollover) — the moment offerings are accepted.
    pub fn tick_ire(&mut self, day_frac: f32) -> bool {
        // The wild breathes easier when the land drinks.
        let decay = if self.weather.precipitating() {
            5.0
        } else {
            4.0
        };
        self.add_ire(-decay * day_frac);
        // Grudges and gratitude both fade (2 per day toward zero).
        self.regional_ire.retain(|_, v| {
            *v -= v.signum() * (2.0 * day_frac).min(v.abs());
            v.abs() >= 0.01
        });
        self.tick_hearts(day_frac);
        self.refresh_long_winter();
        self.tick_rooting(day_frac);
        self.tick_graft(day_frac);
        // Blooms burn down day by day.
        self.bloom.retain(|_, v| {
            *v -= day_frac;
            *v > 0.0
        });
        self.day_progress += day_frac;
        if self.day_progress >= 1.0 {
            self.day_progress -= 1.0;
            self.plant_ire_today = 0.0;
            // The wild forgives, slowly: a cell held deeply blessed
            // for a full season earns ONE wildlife reseed — its
            // hunted-out chunks roll again when next visited.
            let blessed: Vec<RegionCell> = self
                .regional_ire
                .iter()
                .filter(|(_, v)| **v < -10.0)
                .map(|(c, _)| *c)
                .collect();
            for cell in blessed {
                let streak = self.blessed_streak.entry(cell).or_insert(0);
                *streak += 1;
                if *streak >= SEASON_DAYS {
                    self.blessed_streak.remove(&cell);
                    let (cu0, cv0) = (u16::from(cell.u) * 16, u16::from(cell.v) * 16);
                    for du in 0..16 {
                        for dv in 0..16 {
                            let pos = ChunkPos::new(cell.face, cu0 + du, cv0 + dv)
                                .expect("regional ledger cells partition each face");
                            self.mob_seeded.remove(&pos);
                        }
                    }
                    self.whispers
                        .push("The land breathes. Something returns.".to_string());
                }
            }
            self.blessed_streak
                .retain(|c, _| self.regional_ire.get(c).is_some_and(|&v| v < -10.0));
            return true;
        }
        false
    }

    /// The season's appetite: what the wild wants brought this time
    /// of year, and how it says so. Fixed to the calendar — players
    /// learn the year, not a dice roll.
    pub fn season_want(&self) -> (usize, &'static str) {
        match self.season() {
            0 => (0, "The wild stirs. Seeds and saplings are welcome."),
            1 => (1, "The wild thirsts. Carried water is welcome."),
            2 => (2, "The wild gathers. First fruits are welcome."),
            _ => (3, "The wild hungers. Food is welcome."),
        }
    }

    /// Does a stack satisfy the season's want?
    pub fn satisfies_want(&self, want: usize, s: &ItemStack) -> bool {
        let d = self.reg.item(s.item);
        match want {
            // Spring: things that grow — saplings and plantables.
            0 => {
                d.name.ends_with("_sapling")
                    || d.places
                        .is_some_and(|b| self.reg.block(b).crop_next.is_some())
            }
            // Summer: water, carried by hand.
            1 => d.name == "base:bucket_water",
            // Autumn: the harvest's produce (plant nutrition).
            2 => d
                .food
                .as_ref()
                .is_some_and(|f| f.nutrition[..4].iter().any(|&n| n > 0.0)),
            // Winter: anything that feeds.
            _ => d.food.is_some(),
        }
    }

    /// What the wild values: its own materials most, then life given.
    pub fn offering_value(&self, s: &ItemStack) -> f32 {
        let d = self.reg.item(s.item);
        let per = if d.name == "base:diamond" {
            // The wild prizes what the deep earth surrenders rarest.
            6.0
        } else if [
            "base:amethyst_shard",
            "base:gold_ingot",
            "base:silver_ingot",
        ]
        .contains(&d.name.as_str())
        {
            3.0
        } else if [
            "base:heartwood",
            "base:living_wood",
            "base:ember",
            "base:frost_shard",
        ]
        .contains(&d.name.as_str())
        {
            2.0
        } else if d.name.ends_with("_sapling")
            || d.name.contains("raw_")
            || d.name.contains("cooked_")
        {
            1.0
        } else if let Some(f) = &d.food {
            f.hunger * 0.25
        } else {
            0.25
        };
        per * s.count as f32
    }

    /// Dawn: the wild takes everything left on offering stones. Items are
    /// consumed regardless; the refund is capped at 10 per dawn.
    pub fn accept_offerings(&mut self) -> f32 {
        let (want, _) = self.season_want();
        // The ire cells whose country has no spirit left to hear.
        let dead_country: std::collections::HashSet<RegionCell> = self
            .hearts
            .values()
            .filter(|h| h.stage == 0)
            .map(|h| RegionCell::from_surface(h.pos.surface()))
            .collect();
        let mut taken: Vec<(RegionCell, ItemStack)> = Vec::new();
        for (&pos, e) in self.block_entities.iter_mut() {
            let BlockEntity::Offering(o) = e else {
                continue;
            };
            let cell = RegionCell::from_surface(pos.surface());
            // In a country whose heart is dead the stone accepts
            // nothing. Not refused — unreceived. Nobody is home.
            if dead_country.contains(&cell) {
                continue;
            }
            for slot in o.slots.iter_mut() {
                if let Some(s) = slot.take() {
                    taken.push((cell, s));
                }
            }
        }
        if taken.is_empty() {
            return 0.0;
        }
        // The season's want counts double — a bonus for listening,
        // never a penalty — and every stone credits its own valley.
        let mut value = 0.0f32;
        for (cell, s) in &taken {
            let mut v = self.offering_value(s);
            if self.satisfies_want(want, s) {
                v *= 2.0;
            }
            value += v;
            self.charge_cell(*cell, -v.min(6.0));
        }
        let refund = value.min(10.0);
        self.add_ire(-refund);
        refund
    }

    /// Grow a planted sapling into a full tree, mirroring the worldgen
    /// shapes. Returns false (sapling stays) if the trunk is blocked.
    pub fn grow_tree_at(&mut self, pos: crate::planet::BlockPos, species: &str, rnd: u32) -> bool {
        let reg = self.reg.clone();
        let ids = |l: &str, f: &str| Some((reg.block_id(l)?, reg.block_id(f)?));
        let Some((log, leaf)) = (match species {
            "birch" => ids("base:birch_log", "base:birch_leaves"),
            "spruce" => ids("base:spruce_log", "base:spruce_leaves"),
            "jungle" => ids("base:jungle_log", "base:jungle_leaves"),
            "acacia" => ids("base:acacia_log", "base:acacia_leaves"),
            _ => ids("base:log", "base:leaves"),
        }) else {
            return false;
        };
        let trunk_h = match species {
            "acacia" => 1,
            "spruce" => 5 + (rnd % 3) as i32,
            "jungle" => 6 + (rnd % 3) as i32,
            _ => 4 + (rnd % 3) as i32,
        };
        // Clearance: the trunk column (above the sapling cell) must be open.
        for dy in 1..=trunk_h + 1 {
            if pos
                .offset(0, dy, 0)
                .is_none_or(|at| self.get_block_at(at) != AIR)
            {
                return false;
            }
        }
        let leaf_at = |w: &mut World, dx: i32, dy: i32, dz: i32| {
            if let Some(at) = pos.offset(dx, dy, dz)
                && at.y() > 0
                && w.get_block_at(at) == AIR
            {
                w.set_block_at(at, leaf);
            }
        };
        for dy in 0..trunk_h {
            if let Some(at) = pos.offset(0, dy, 0) {
                self.set_block_at(at, log);
            }
        }
        match species {
            "acacia" => {
                for dx in -1..=1 {
                    for dz in -1..=1 {
                        leaf_at(self, dx, trunk_h, dz);
                    }
                }
            }
            "spruce" => {
                for (dy, r) in [(-3i32, 2i32), (-2, 1), (-1, 2), (0, 1), (1, 1)] {
                    for dx in -r..=r {
                        for dz in -r..=r {
                            if dx.abs() == r && dz.abs() == r && r > 1 {
                                continue;
                            }
                            if dx == 0 && dz == 0 && dy < 0 {
                                continue;
                            }
                            leaf_at(self, dx, trunk_h + dy, dz);
                        }
                    }
                }
                leaf_at(self, 0, trunk_h + 2, 0);
            }
            _ => {
                let big: i32 = if species == "jungle" { 3 } else { 2 };
                for (dy, r) in [(-2i32, big), (-1, big), (0, 1), (1, 1)] {
                    for dx in -r..=r {
                        for dz in -r..=r {
                            if dx == 0 && dz == 0 && dy < 0 {
                                continue;
                            }
                            leaf_at(self, dx, trunk_h + dy, dz);
                        }
                    }
                }
            }
        }
        true
    }

    /// Ire cost of breaking a block, by what it is.
    pub fn ire_for_block(&self, b: BlockId) -> f32 {
        let name = &self.reg.block(b).name;
        if name.ends_with("_log") || name.ends_with(":log") {
            0.3
        } else if name.contains("ore") {
            0.4
        } else if name.ends_with("stone") && !name.contains("cobble") {
            0.05
        } else if name.contains("leaves") || name.ends_with("dirt") || name.ends_with("grass") {
            0.02
        } else {
            0.0
        }
    }
}
