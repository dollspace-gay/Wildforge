//! Seasons, weather locality, ire, offerings, and renewable growth.

use super::*;

impl World {
    /// 0 spring, 1 summer, 2 autumn, 3 winter.
    pub fn season(&self) -> usize {
        ((self.day / SEASON_DAYS) % 4) as usize
    }

    /// 0..1 through the current season.
    pub fn season_progress(&self) -> f32 {
        (self.day % SEASON_DAYS) as f32 / SEASON_DAYS as f32
    }

    /// Does precipitation fall as snow in this column? The threshold
    /// relaxes in winter so taiga and cold-temperate lands whiten.
    pub fn snows_at(&self, x: i32, z: i32) -> bool {
        let t = self.generator.climate(x, z).t;
        t < if self.season() == 3 { -0.05 } else { -0.35 }
    }

    /// Deserts stay dry: overcast skies, nothing falls.
    pub fn rains_at(&self, x: i32, z: i32) -> bool {
        let c = self.generator.climate(x, z);
        !(c.t > 0.6 && c.h < -0.5)
    }

    // ---------------- ire (reciprocity) ----------------

    /// The regional ledger's cell for a position (~256-block country).
    pub fn ire_cell(x: i32, z: i32) -> (i32, i32) {
        (x >> 8, z >> 8)
    }

    /// The land's local standing: negative is tended, positive is
    /// aggrieved, clamped to a grudge the wild can actually hold.
    pub fn regional_ire_at(&self, x: i32, z: i32) -> f32 {
        self.regional_ire
            .get(&Self::ire_cell(x, z))
            .copied()
            .unwrap_or(0.0)
    }

    fn charge_cell(&mut self, x: i32, z: i32, amt: f32) {
        let cell = Self::ire_cell(x, z);
        let e = self.regional_ire.entry(cell).or_insert(0.0);
        *e = (*e + amt).clamp(-20.0, 20.0);
        if e.abs() < 0.01 {
            self.regional_ire.remove(&cell);
        }
    }

    /// Taking, placed: the world remembers, and so does the valley.
    pub fn add_ire_at(&mut self, x: i32, z: i32, amt: f32) {
        self.add_ire(amt);
        self.charge_cell(x, z, amt);
    }

    /// Mending, placed: the global refund keeps its daily cap, but the
    /// valley always notices the hands that tend it.
    pub fn plant_ire_at(&mut self, x: i32, z: i32, amt: f32) {
        self.plant_ire(amt);
        self.charge_cell(x, z, -amt);
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
    pub fn ire_tier_at(&self, x: i32, z: i32) -> usize {
        Self::tier_of((self.ire + self.regional_ire_at(x, z) * 3.0).clamp(0.0, 100.0))
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
        self.day_progress += day_frac;
        if self.day_progress >= 1.0 {
            self.day_progress -= 1.0;
            self.plant_ire_today = 0.0;
            return true;
        }
        false
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
        let mut taken: Vec<ItemStack> = Vec::new();
        for e in self.block_entities.values_mut() {
            let BlockEntity::Offering(o) = e else {
                continue;
            };
            for slot in o.slots.iter_mut() {
                if let Some(s) = slot.take() {
                    taken.push(s);
                }
            }
        }
        if taken.is_empty() {
            return 0.0;
        }
        let value: f32 = taken.iter().map(|s| self.offering_value(s)).sum();
        let refund = value.min(10.0);
        self.add_ire(-refund);
        refund
    }

    /// Grow a planted sapling into a full tree, mirroring the worldgen
    /// shapes. Returns false (sapling stays) if the trunk is blocked.
    pub fn grow_tree(&mut self, x: i32, y: i32, z: i32, species: &str, rnd: u32) -> bool {
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
            if self.get_block(x, y + dy, z) != AIR {
                return false;
            }
        }
        let leaf_at = |w: &mut World, lx: i32, ly: i32, lz: i32| {
            if ly > 0 && ly < CHUNK_Y as i32 && w.get_block(lx, ly, lz) == AIR {
                w.set_block(lx, ly, lz, leaf);
            }
        };
        for dy in 0..trunk_h {
            self.set_block(x, y + dy, z, log);
        }
        let top = y + trunk_h;
        match species {
            "acacia" => {
                for dx in -1..=1 {
                    for dz in -1..=1 {
                        leaf_at(self, x + dx, top, z + dz);
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
                            leaf_at(self, x + dx, top + dy, z + dz);
                        }
                    }
                }
                leaf_at(self, x, top + 2, z);
            }
            _ => {
                let big: i32 = if species == "jungle" { 3 } else { 2 };
                for (dy, r) in [(-2i32, big), (-1, big), (0, 1), (1, 1)] {
                    for dx in -r..=r {
                        for dz in -r..=r {
                            if dx == 0 && dz == 0 && dy < 0 {
                                continue;
                            }
                            leaf_at(self, x + dx, top + dy, z + dz);
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
