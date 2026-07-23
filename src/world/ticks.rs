//! Random ticks, offline reconciliation, crops, snow, rain, and saplings.

use super::*;

impl World {
    /// Random ticks: crops advance a stage when conditions hold.
    /// Random ticks at constant cost: visit the K oldest-stamped
    /// chunks with a sample burst scaled by how long each waited —
    /// one mechanism for "far corner of a big view distance" and
    /// "just came back". Returns samples drawn (for tests).
    pub fn random_tick(&mut self, rng: &mut u32) -> usize {
        const K: usize = 64;
        let reg = self.reg.clone();
        let farmland = reg.block_id("base:farmland");
        let ice = reg.block_id("base:ice");
        let snow_layer = reg.block_id("base:snow_layer");
        let snow_trod = reg.block_id("base:snow_layer_trod");
        let season = self.season();
        let mut order: Vec<(f64, ChunkPos)> = self
            .chunks
            .keys()
            .map(|p| {
                (
                    self.last_random
                        .get(&(p.x, p.z))
                        .copied()
                        .unwrap_or(self.clock),
                    *p,
                )
            })
            .collect();
        order.sort_unstable_by(|a, b| {
            a.0.total_cmp(&b.0)
                .then((a.1.x, a.1.z).cmp(&(b.1.x, b.1.z)))
        });
        order.truncate(K);
        let mut samples = 0;
        let mut changes = Vec::new();
        let mut saplings: Vec<(i32, i32, i32, String, u32)> = Vec::new();
        for (stamp, pos) in order {
            let elapsed = (self.clock - stamp).max(0.0);
            // 8 samples per half-second of waiting, floor 8, cap 256.
            let n = ((elapsed * 16.0) as usize).clamp(8, 256);
            self.last_random.insert((pos.x, pos.z), self.clock);
            samples += n;
            for _ in 0..n {
                *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                let r = *rng >> 8;
                let (lx, lz) = ((r % 16) as i32, ((r >> 4) % 16) as i32);
                let y = ((r >> 8) % CHUNK_Y as u32) as i32;
                let (wx, wz) = (pos.x * 16 + lx, pos.z * 16 + lz);
                let b = self.get_block(wx, y, wz);
                let d = reg.block(b);
                if let Some(species) = &d.sapling {
                    *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                    if ((*rng >> 8) as f32 / (1 << 24) as f32) < 0.02 {
                        saplings.push((wx, y, wz, species.clone(), *rng));
                    }
                    continue;
                }
                if let Some(next) = d.crop_next {
                    let soil_ok =
                        d.crop_any_soil || farmland == Some(self.get_block(wx, y - 1, wz));
                    // The calendar gates growth. Bushes fruit in summer
                    // and autumn; crops slow through the year and stop
                    // in winter - unless roofed and torchlit (a
                    // greenhouse, emergent from the light rules).
                    let mult = if d.crop_any_soil {
                        if season == 1 || season == 2 { 1.0 } else { 0.0 }
                    } else {
                        match season {
                            0 => 1.25,
                            1 => 1.0,
                            2 => 0.75,
                            _ => 0.0,
                        }
                    };
                    let mult = if mult == 0.0 && !d.crop_any_soil {
                        let (bl, sl) = self.light_at(wx, y, wz);
                        if sl < 15 && bl >= 10 {
                            0.5 // dark roof + torchlight
                        } else if sl == 15
                            && (1..=16)
                                .any(|dy| self.reg.block(self.get_block(wx, y + dy, wz)).glass)
                        {
                            0.75 // a glass roof is a greenhouse
                        } else {
                            0.0
                        }
                    } else {
                        mult
                    };
                    *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                    if soil_ok
                        && mult > 0.0
                        && ((*rng >> 8) as f32 / (1 << 24) as f32) < d.crop_chance * mult
                    {
                        changes.push((wx, y, wz, next));
                    }
                    continue;
                }
                // Winter freezes exposed still water outside the warm
                // belts; spring gives the lakes back.
                let sky_open = self.light_at(wx, y + 1, wz).1 == 15;
                if d.water_level == Some(0)
                    && season == 3
                    && sky_open
                    && self.get_block(wx, y + 1, wz) == AIR
                    && self.generator.climate(wx, wz).t < 0.35
                {
                    if let Some(ice) = ice {
                        changes.push((wx, y, wz, ice));
                    }
                    continue;
                }
                if Some(b) == ice
                    && (season == 0 || season == 1)
                    && sky_open
                    && self.generator.climate(wx, wz).t > -0.35
                {
                    changes.push((wx, y, wz, self.reg.water_block(0)));
                    continue;
                }
                // Snow layers melt under bright light or a warm season
                // (footprints melt with them).
                if Some(b) == snow_layer || Some(b) == snow_trod {
                    let (bl, _) = self.light_at(wx, y, wz);
                    let warm = season != 3 && self.generator.climate(wx, wz).t > -0.35;
                    if bl >= 12 || warm {
                        changes.push((wx, y, wz, AIR));
                    }
                    continue;
                }
                // Summer sun dries shallow water: a hit drops the cell
                // to a marshy film in a basin, or clears an open spill
                // entirely. The cell and every water neighbor must be
                // shallow — deep bodies are safe, and the sea can't be
                // siphoned out through its beaches.
                if season == 1
                    && sky_open
                    && let Some(v) = reg.water_volume(b)
                    && self.get_block(wx, y + 1, wz) == AIR
                    && self.generator.climate(wx, wz).t > -0.35
                    && self.water_depth_at_most(wx, y, wz, 2)
                    && [(1, 0), (-1, 0), (0, 1), (0, -1)]
                        .iter()
                        .all(|&(dx, dz)| self.water_depth_at_most(wx + dx, y, wz + dz, 2))
                {
                    let contained = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                        .iter()
                        .filter(|&&(dx, dz)| {
                            let n = self.get_block(wx + dx, y, wz + dz);
                            self.reg.is_solid(n) || self.reg.is_water(n)
                        })
                        .count()
                        >= 3;
                    if contained {
                        if v > 1 {
                            changes.push((wx, y, wz, reg.water_for_volume(1)));
                        }
                    } else {
                        changes.push((wx, y, wz, AIR));
                    }
                }
            }
        }
        for (x, y, z, b) in changes {
            // A crop reaching its final stage refunds ire (capped daily).
            if self.reg.block(b).crop_next.is_none() {
                self.plant_ire(0.5);
            }
            self.set_block(x, y, z, b);
        }
        for (x, y, z, _species, rnd) in saplings {
            self.try_grow_sapling(x, y, z, rnd);
        }
        samples
    }

    /// The last random-tick stamp for a chunk.
    #[cfg(test)]
    pub fn chunk_stamp(&self, x: i32, z: i32) -> Option<f64> {
        self.last_random.get(&(x, z)).copied()
    }

    /// A chunk returning after an absence catches up in one sweep:
    /// phase rules apply wholesale — it's winter, so the pond you
    /// left liquid is simply frozen when you arrive — and crops
    /// advance by a Poisson draw over the random ticks they missed,
    /// integrated across the seasons the absence spanned. The water
    /// cycle is deliberately not reconciled (it nets roughly zero
    /// over a season); light-gated cases judge the plot as it stands
    /// today — an accepted approximation.
    pub(super) fn reconcile_chunk(&mut self, pos: ChunkPos, elapsed: f64) {
        let reg = self.reg.clone();
        let ice = reg.block_id("base:ice");
        let snow_layer = reg.block_id("base:snow_layer");
        let snow_trod = reg.block_id("base:snow_layer_trod");
        let farmland = reg.block_id("base:farmland");
        let season = self.season();
        let day_len = crate::server::DAY_LENGTH as f64;
        // Sudden wholesale phase changes want a real absence behind
        // them; short gaps stay with the gradual burst mechanism.
        let phase = elapsed >= 2.0 * day_len;
        // Expected random-tick visits per block per in-game day.
        let ticks_per_day = 16.0 * day_len / (CHUNK_X * CHUNK_Z * CHUNK_Y) as f64;
        // Deterministic per-(world, chunk, day) randomness.
        let mut r = self
            .seed
            .wrapping_mul(31)
            .wrapping_add(pos.x as u32)
            .wrapping_mul(31)
            .wrapping_add(pos.z as u32)
            .wrapping_mul(31)
            .wrapping_add(self.day);
        // Seasons of the missed days, capped at two years back —
        // beyond that the expectations saturate anyway.
        let end_day = (self.clock / day_len) as i64;
        let start_day = ((self.clock - elapsed) / day_len) as i64;
        let days: Vec<usize> = (start_day..end_day)
            .rev()
            .take(96)
            .map(|d| ((d.max(0) as u32 / SEASON_DAYS) % 4) as usize)
            .collect();

        // One pass over the chunk collects the cells the rules touch.
        let mut interesting: Vec<(i32, i32, i32, BlockId)> = Vec::new();
        if let Some(c) = self.chunks.get(&pos) {
            for lx in 0..CHUNK_X {
                for lz in 0..CHUNK_Z {
                    for y in 1..CHUNK_Y {
                        let b = c.get(lx, y, lz);
                        if b == AIR {
                            continue;
                        }
                        let d = reg.block(b);
                        if d.crop_next.is_some()
                            || d.sapling.is_some()
                            || d.water_level == Some(0)
                            || Some(b) == ice
                            || Some(b) == snow_layer
                            || Some(b) == snow_trod
                        {
                            interesting.push((
                                pos.x * CHUNK_X as i32 + lx as i32,
                                y as i32,
                                pos.z * CHUNK_Z as i32 + lz as i32,
                                b,
                            ));
                        }
                    }
                }
            }
        }

        let mut changes = Vec::new();
        let mut grow: Vec<(i32, i32, i32, u32)> = Vec::new();
        let mut refunds = 0u32;
        for (wx, y, wz, b) in interesting {
            let d = reg.block(b);
            if d.sapling.is_some() {
                let e = days.len() as f64 * ticks_per_day * 0.02;
                if poisson(e, &mut r) > 0 {
                    r = r.wrapping_mul(1664525).wrapping_add(1013904223);
                    grow.push((wx, y, wz, r));
                }
                continue;
            }
            if d.crop_next.is_some() {
                let soil_ok = d.crop_any_soil || farmland == Some(self.get_block(wx, y - 1, wz));
                if !soil_ok {
                    continue;
                }
                let mut sum = 0.0;
                for &s in &days {
                    let mult = if d.crop_any_soil {
                        if s == 1 || s == 2 { 1.0 } else { 0.0 }
                    } else {
                        match s {
                            0 => 1.25,
                            1 => 1.0,
                            2 => 0.75,
                            _ => 0.0,
                        }
                    };
                    let mult = if mult == 0.0 && !d.crop_any_soil {
                        let (bl, sl) = self.light_at(wx, y, wz);
                        if sl < 15 && bl >= 10 {
                            0.5
                        } else if sl == 15
                            && (1..=16).any(|dy| reg.block(self.get_block(wx, y + dy, wz)).glass)
                        {
                            0.75
                        } else {
                            0.0
                        }
                    } else {
                        mult
                    };
                    sum += mult;
                }
                let k = poisson(ticks_per_day * d.crop_chance as f64 * sum, &mut r);
                if k > 0 {
                    let mut cur = b;
                    for _ in 0..k {
                        match reg.block(cur).crop_next {
                            Some(n) => cur = n,
                            None => break,
                        }
                    }
                    if cur != b {
                        // Reaching the final stage refunds ire, as a
                        // live random tick would have.
                        if reg.block(cur).crop_next.is_none() {
                            refunds += 1;
                        }
                        changes.push((wx, y, wz, cur));
                    }
                }
                continue;
            }
            if !phase {
                continue;
            }
            let sky_open = self.light_at(wx, y + 1, wz).1 == 15;
            if d.water_level == Some(0)
                && season == 3
                && sky_open
                && self.get_block(wx, y + 1, wz) == AIR
                && self.generator.climate(wx, wz).t < 0.35
            {
                if let Some(ice) = ice {
                    changes.push((wx, y, wz, ice));
                }
                continue;
            }
            if Some(b) == ice
                && (season == 0 || season == 1)
                && sky_open
                && self.generator.climate(wx, wz).t > -0.35
            {
                changes.push((wx, y, wz, reg.water_block(0)));
                continue;
            }
            if Some(b) == snow_layer || Some(b) == snow_trod {
                let (bl, _) = self.light_at(wx, y, wz);
                let warm = season != 3 && self.generator.climate(wx, wz).t > -0.35;
                if bl >= 12 || warm {
                    changes.push((wx, y, wz, AIR));
                }
            }
        }
        // Batched apply: a frozen lake is many cells — one relight,
        // not one per cell.
        let any = !changes.is_empty();
        for (x, y, z, nb) in changes {
            let (lx, lz) = (
                x.rem_euclid(CHUNK_X as i32) as usize,
                z.rem_euclid(CHUNK_Z as i32) as usize,
            );
            if let Some(c) = self.chunks.get_mut(&pos) {
                c.set(lx, y as usize, lz, nb);
                c.dirty = true;
                c.modified = true;
                if self.log_edits {
                    self.edit_log.push((x, y, z, nb, 0));
                }
            }
            self.wake_water(x, y, z);
        }
        if any {
            self.relight_and_cascade(pos);
        }
        for _ in 0..refunds {
            self.plant_ire(0.5);
        }
        for (x, y, z, rnd) in grow {
            self.try_grow_sapling(x, y, z, rnd);
        }
    }

    /// A footstep through a snow layer presses it into a trodden
    /// print — a real edit: logged, broadcast, persisted, and it melts
    /// like any layer. History written in the ground.
    pub fn tread(&mut self, x: i32, y: i32, z: i32) {
        if self.remote {
            return; // guests' prints are stamped by the host
        }
        let (Some(layer), Some(trod)) = (
            self.reg.block_id("base:snow_layer"),
            self.reg.block_id("base:snow_layer_trod"),
        ) else {
            return;
        };
        if self.get_block(x, y, z) == layer {
            self.set_block(x, y, z, trod);
        }
    }

    /// One flake of consequence: lay a snow layer on this column's
    /// surface if the storm is cold here and the sky can reach it.
    pub fn settle_snow(&mut self, x: i32, z: i32) {
        if !self.snows_at(x, z) {
            return;
        }
        let Some(layer) = self.reg.block_id("base:snow_layer") else {
            return;
        };
        let y = self.surface_height(x, z);
        if y <= SEA_LEVEL || y + 1 >= CHUNK_Y as i32 - 1 {
            return;
        }
        if self.get_block(x, y + 1, z) != AIR || self.light_at(x, y + 1, z).1 != 15 {
            return;
        }
        self.set_block(x, y + 1, z, layer);
    }

    /// Is the water column under (x, y, z) at most `d` cells deep?
    /// Non-water counts as depth zero (air and solid never block).
    pub(super) fn water_depth_at_most(&self, x: i32, y: i32, z: i32, d: i32) -> bool {
        let mut depth = 0;
        let mut yy = y;
        while self.reg.is_water(self.get_block(x, yy, z)) {
            depth += 1;
            if depth > d {
                return false;
            }
            yy -= 1;
        }
        true
    }

    /// Rain refills the water it lands on: the first surface the
    /// column offers, if partial water or a film, gains one unit —
    /// ponds creep back toward full through a wet autumn.
    pub fn rain_fill(&mut self, x: i32, z: i32) {
        if self.snows_at(x, z) || !self.rains_at(x, z) {
            return;
        }
        for y in (1..CHUNK_Y as i32).rev() {
            let b = self.get_block(x, y, z);
            if b == AIR {
                continue;
            }
            if let Some(v) = self.reg.water_volume(b)
                && v < 8
            {
                self.set_block(x, y, z, self.reg.water_for_volume(v + 1));
            }
            return;
        }
    }

    /// Attempt to mature the sapling at this position. On success the
    /// tree is built and the wild refunds -2 ire, bypassing the daily
    /// planting cap (it took days — it IS the slow path).
    pub fn try_grow_sapling(&mut self, x: i32, y: i32, z: i32, rnd: u32) -> bool {
        let b = self.get_block(x, y, z);
        let Some(species) = self.reg.block(b).sapling.clone() else {
            return false;
        };
        if self.grow_tree(x, y, z, &species, rnd) {
            self.add_ire(-2.0);
            true
        } else {
            false
        }
    }
}
