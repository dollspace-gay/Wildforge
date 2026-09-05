//! Random ticks, offline reconciliation, crops, snow, rain, and saplings.

use super::*;
use crate::planet::{BlockPos, SurfacePos};

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
        let grass_id = reg.block_id("base:grass");
        let dirt_id = reg.block_id("base:dirt");
        let heap = reg.block_id("base:compost_heap");
        let heap_ready = reg.block_id("base:compost_heap_ready");
        let litter_id = reg.block_id("base:leaf_litter");
        let mut order: Vec<(f64, ChunkPos)> = self
            .chunks
            .keys()
            .map(|p| (self.last_random.get(p).copied().unwrap_or(self.calendar_state.clock()), *p))
            .collect();
        order.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        order.truncate(K);
        let mut samples = 0;
        let mut changes = Vec::new();
        let mut freezes: Vec<BlockPos> = Vec::new();
        let mut melts: Vec<BlockPos> = Vec::new();
        let mut snow_melts: Vec<BlockPos> = Vec::new();
        let mut evaporates: Vec<BlockPos> = Vec::new();
        // Fallow soil recovering (position, gain).
        let mut fed: Vec<(BlockPos, u8)> = Vec::new();
        // Plain swaps that earn no plant-ire credit (compost ripening,
        // fungus creep, settling litter).
        let mut swaps: Vec<(BlockPos, BlockId)> = Vec::new();
        // Items shed where a change happened (sapling from rot).
        let mut drops: Vec<(BlockPos, ItemId)> = Vec::new();
        let mut saplings: Vec<(BlockPos, String, u32)> = Vec::new();
        for (stamp, pos) in order {
            let elapsed = (self.calendar_state.clock() - stamp).max(0.0);
            // Samples proportional to the wait, floor 8, cap 256.
            let n = ((elapsed * RANDOM_TICKS_PER_CHUNK_SEC) as usize).clamp(8, 256);
            self.last_random.insert(pos, self.calendar_state.clock());
            samples += n;
            for _ in 0..n {
                *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                let r = *rng >> 8;
                let (lx, lz) = ((r % 16) as i32, ((r >> 4) % 16) as i32);
                let y = ((r >> 8) % CHUNK_Y as u32) as u8;
                let at = BlockPos::new(
                    pos.face(),
                    pos.u() * CHUNK_X as u16 + lx as u16,
                    y,
                    pos.v() * CHUNK_Z as u16 + lz as u16,
                )
                .expect("a sampled chunk cell is canonical");
                let season = self.season_at_surface(at.surface());
                let local_weather = self.weather_at_surface(at.surface());
                let b = self.get_block_at(at);
                let d = reg.block(b);
                // An arc lamp whose generator stopped (or left) goes
                // dark on its own clock — self-healing, no scan.
                if let Some(stripped) = d.name.strip_suffix("_lit")
                    && d.name.contains("arc_lamp")
                    && !self.generator_near_at(at, ELEC_RADIUS)
                    && let Some(off) = reg.block_id(stripped)
                {
                    changes.push((at, off));
                    continue;
                }
                if let Some(species) = &d.sapling {
                    *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                    if ((*rng >> 8) as f32 / (1 << 24) as f32) < 0.02 {
                        saplings.push((at, species.clone(), *rng));
                    }
                    continue;
                }
                if let Some(next) = d.crop_next {
                    let below = at.offset(0, -1, 0);
                    let soil_ok = d.crop_any_soil
                        || below.is_some_and(|pos| farmland == Some(self.get_block_at(pos)));
                    // The calendar gates growth. Bushes fruit in summer
                    // and autumn; crops slow through the year and stop
                    // in winter - unless roofed and torchlit (a
                    // greenhouse, emergent from the light rules).
                    let mult = if d.crop_any_soil {
                        // Wild fruit is the country's gift, not yours.
                        let base = if !self.heart_alive_at_surface(at.surface()) {
                            0.0
                        } else if season == 1 || season == 2 {
                            1.0
                        } else {
                            0.0
                        };
                        // Blessed country feeds back — and bloomed
                        // country (post-wrath) erupts the same way.
                        if self.regional_ire_at_surface(at.surface()) < -8.0
                            || self.bloom_at_surface(at.surface()) > 0.0
                        {
                            base * 2.0
                        } else {
                            base
                        }
                    } else {
                        match season {
                            0 => 1.25,
                            1 => 1.0,
                            2 => 0.75,
                            _ => 0.0,
                        }
                    };
                    let mult = if mult == 0.0 && !d.crop_any_soil {
                        let (bl, sl) = self.light_at_pos(at);
                        if sl < 15 && bl >= 10 {
                            0.5 // dark roof + torchlight
                        } else if sl == 15
                            && (1..=16)
                                .filter_map(|dy| at.offset(0, dy, 0))
                                .any(|pos| self.reg.block(self.get_block_at(pos)).glass)
                        {
                            0.75 // a glass roof is a greenhouse
                        } else {
                            0.0
                        }
                    } else {
                        mult
                    };
                    let (block_light, sky_light) = self.light_at_pos(at);
                    let protected = block_light >= 10
                        && (sky_light < 15
                            || (1..=16)
                                .filter_map(|dy| at.offset(0, dy, 0))
                                .any(|pos| self.reg.block(self.get_block_at(pos)).glass));
                    let effective_temperature =
                        local_weather.temperature_c + if protected { 10.0 } else { 0.0 };
                    let temperature_mult = if effective_temperature <= 0.0 {
                        0.0
                    } else if effective_temperature < 14.0 {
                        effective_temperature / 14.0
                    } else if effective_temperature <= 29.0 {
                        1.0
                    } else {
                        ((42.0 - effective_temperature) / 13.0).clamp(0.0, 1.0)
                    };
                    let light_mult = (f32::from(block_light.max(sky_light)) / 12.0).clamp(0.0, 1.0);
                    let moisture_mult = below
                        .map_or_else(
                            || self.soil_moisture_at_surface(at.surface()),
                            |soil_pos| self.managed_soil_moisture_at(soil_pos),
                        )
                        .clamp(0.0, 1.25);
                    let mult = mult
                        * temperature_mult
                        * light_mult
                        * moisture_mult
                        * self.root_uptake_multiplier_at(at)
                        * if self.environmental_dross_band_at(at) >= crate::dross::DrossBand::Scar {
                            // Severe burden stalls a cultivated block. Its
                            // identity and metadata remain byte-identical.
                            0.0
                        } else {
                            1.0
                        };
                    // Fertile loam runs half again over baseline;
                    // exhausted dust crawls (soil.rs).
                    let fmult = if d.crop_any_soil {
                        1.0
                    } else {
                        below.map_or(0.0, |soil_pos| {
                            soil::fert_mult(soil::fert_of(self.get_meta_at(soil_pos)))
                                * self.crop_soil_multiplier_at(soil_pos)
                        })
                    };
                    *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                    if soil_ok
                        && mult > 0.0
                        && ((*rng >> 8) as f32 / (1 << 24) as f32) < d.crop_chance * mult * fmult
                    {
                        changes.push((at, next));
                    }
                    continue;
                }
                // A country whose heart is dead gives nothing: soil
                // stops resting, the tide stops seeding, bushes stop
                // fruiting. Crops still grow on ground you feed
                // yourself — farms work here, wilderness does not.
                let living = self.heart_alive_at_surface(at.surface());
                // Fallow farmland recovers, twice as fast under winter
                // (or snow) — the off season is the soil's turn.
                if Some(b) == farmland {
                    let soil_profile = self
                        .planet_atlas
                        .as_ref()
                        .map(|atlas| atlas.biome_sample(at.surface()));
                    let current_salt = self.get_soil_salinity_at(at);
                    let irrigation_salt = [
                        (1, 0, 0),
                        (-1, 0, 0),
                        (0, 1, 0),
                        (0, -1, 0),
                        (0, 0, 1),
                        (0, 0, -1),
                    ]
                    .into_iter()
                    .filter_map(|(du, dy, dv)| at.offset(du, dy, dv))
                    .filter_map(|neighbor| self.water_mass_at(neighbor))
                    .filter(|mass| mass.water_hu > 0)
                    .map(|mass| mass.salinity())
                    .max();
                    let next_salt = if irrigation_salt.is_some_and(|salt| salt >= 48)
                        && soil_profile.is_none_or(|soil| soil.drainage < 175)
                    {
                        current_salt
                            .saturating_add(irrigation_salt.map_or(1, |salt| (salt / 32).max(1)))
                    } else if local_weather.precipitation
                        == crate::planet_atlas::PrecipitationForm::Rain
                        && soil_profile.is_none_or(|soil| soil.drainage >= 72)
                    {
                        current_salt.saturating_sub(3)
                    } else if irrigation_salt.is_some_and(|salt| salt < 24)
                        && soil_profile.is_none_or(|soil| soil.drainage >= 96)
                    {
                        current_salt.saturating_sub(1)
                    } else {
                        current_salt
                    };
                    if next_salt != current_salt {
                        self.set_soil_salinity_at(at, next_salt);
                    }
                    // A dead heart can stop supernatural renewal, never
                    // rainfall, drainage, or salt transport.
                    if !living {
                        continue;
                    }
                    let above = at.offset(0, 1, 0).map_or(AIR, |pos| self.get_block_at(pos));
                    let resting =
                        above == AIR || Some(above) == snow_layer || Some(above) == snow_trod;
                    if resting {
                        *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                        let moisture = self.managed_soil_moisture_at(at).clamp(0.0, 1.0);
                        let warmth = ((local_weather.temperature_c + 4.0) / 22.0).clamp(0.0, 1.0);
                        let organic = soil_profile.map_or(0.7, |soil| {
                            (f32::from(soil.organic) / 160.0).clamp(0.2, 1.2)
                        });
                        if ((*rng >> 8) as f32 / (1 << 24) as f32)
                            < 0.5 * moisture * warmth.max(0.25) * organic
                        {
                            let winterish = season == 3 || Some(above) == snow_layer;
                            let gain = if winterish {
                                soil::FERT_FALLOW * 2
                            } else {
                                soil::FERT_FALLOW
                            };
                            fed.push((at, gain));
                        }
                    }
                    continue;
                }
                // A full compost heap turns the next time the world
                // looks at it — random-tick visits are days apart for
                // any single cell, so the wait is already real.
                if Some(b) == heap
                    && self.get_meta_at(at) >= soil::COMPOST_FULL
                    && let Some(ready) = heap_ready
                {
                    swaps.push((at, ready));
                    continue;
                }
                // Severed leaves rot: a canopy with no trunk within
                // reach (BFS through leaves, four steps) sheds its
                // sapling chance and sometimes drops litter on the
                // ground below. Felled forests finally fall.
                if d.name.contains("leaves") {
                    let mut seen = vec![at];
                    let mut queue = vec![(at, 0u8)];
                    let mut anchored = false;
                    'bfs: while let Some((cell, depth)) = queue.pop() {
                        for (dx, dy, dz) in [
                            (1, 0, 0),
                            (-1, 0, 0),
                            (0, 1, 0),
                            (0, -1, 0),
                            (0, 0, 1),
                            (0, 0, -1),
                        ] {
                            let Some(n) = cell.offset(dx, dy, dz) else {
                                continue;
                            };
                            let nb = self.get_block_at(n);
                            let name = &reg.block(nb).name;
                            if name.ends_with("log") {
                                anchored = true;
                                break 'bfs;
                            }
                            if depth < 4 && name.contains("leaves") && !seen.contains(&n) {
                                seen.push(n);
                                queue.push((n, depth + 1));
                            }
                        }
                    }
                    if !anchored {
                        *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                        let roll = (*rng >> 8) as f32 / (1 << 24) as f32;
                        if roll < 0.35 {
                            changes.push((at, AIR));
                            if let Some((item, chance)) = d.bonus_drop
                                && roll < 0.35 * chance
                            {
                                drops.push((at, item));
                            }
                            // Litter settles on the first floor below.
                            if roll > 0.12
                                && let Some(litter) = litter_id
                                && let Some(floor) = (1..=8)
                                    .filter_map(|dy| at.offset(0, -dy, 0))
                                    .find(|&pos| reg.is_solid(self.get_block_at(pos)))
                                && let Some(above) = floor.offset(0, 1, 0)
                                && self.get_block_at(above) == AIR
                            {
                                swaps.push((above, litter));
                            }
                        }
                    }
                    continue;
                }
                // Litter fades into the ground that holds it.
                if Some(b) == litter_id {
                    *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                    if ((*rng >> 8) as f32 / (1 << 24) as f32) < 0.4 {
                        changes.push((at, AIR));
                        if let Some(below) = at.offset(0, -1, 0) {
                            fed.push((below, 4));
                        }
                    }
                    continue;
                }
                // Fungi creep where it is dark and damp: mushrooms
                // spread cell to cell underground or near water, the
                // lantern fungus glows its way along deep stone.
                if d.name.contains("mushroom") || d.name.contains("lantern_fungus") {
                    // No extra odds: any single cell's random-tick
                    // visits are days apart already — the visit rate
                    // IS the creep's pace.
                    *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                    let r1 = *rng;
                    {
                        let dx = (r1 % 5) as i32 - 2;
                        let dz = ((r1 >> 3) % 5) as i32 - 2;
                        let dy = ((r1 >> 6) % 3) as i32 - 1;
                        let Some(target) = at.offset(dx, dy, dz) else {
                            continue;
                        };
                        let (bl, sl) = self.light_at_pos(target);
                        let dark = bl < 6 && sl < 6;
                        let damp = sl == 0
                            || (-3..=3i32).any(|ax| {
                                (-2..=2i32).any(|ay| {
                                    (-3..=3i32).any(|az| {
                                        target
                                            .offset(ax, ay, az)
                                            .is_some_and(|pos| reg.is_water(self.get_block_at(pos)))
                                    })
                                })
                            });
                        let crowd = (-2..=2i32)
                            .flat_map(|ax| {
                                (-1..=1i32)
                                    .flat_map(move |ay| (-2..=2i32).map(move |az| (ax, ay, az)))
                            })
                            .filter(|&(ax, ay, az)| {
                                let n = target.offset(ax, ay, az).map_or("", |pos| {
                                    reg.block(self.get_block_at(pos)).name.as_str()
                                });
                                n.contains("mushroom") || n.contains("lantern_fungus")
                            })
                            .count();
                        let solid_below = target
                            .offset(0, -1, 0)
                            .is_some_and(|below| reg.is_solid(self.get_block_at(below)));
                        if dark
                            && damp
                            && crowd < 3
                            && self.get_block_at(target) == AIR
                            && solid_below
                        {
                            swaps.push((target, b));
                        }
                    }
                    continue;
                }
                // Bare dirt under the sky heals over beside grass —
                // the world stops keeping scars nobody meant to leave.
                if Some(b) == dirt_id
                    && living
                    && at.offset(0, 1, 0).is_some_and(|above| {
                        self.get_block_at(above) == AIR && self.light_at_pos(above).1 >= 9
                    })
                {
                    let near_grass = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|&(dx, dz)| {
                        (-1..=1).any(|dy| {
                            at.offset(dx, dy, dz)
                                .is_some_and(|pos| Some(self.get_block_at(pos)) == grass_id)
                        })
                    });
                    if near_grass {
                        *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                        if ((*rng >> 8) as f32 / (1 << 24) as f32) < 0.05
                            && let Some(g) = grass_id
                        {
                            changes.push((at, g));
                        }
                    }
                    continue;
                }
                // The green tide: in country the wild doesn't resent,
                // mature trees seed the grass at their feet — only on
                // natural ground (untouched chunks), only where the
                // forest isn't already thick, and never in winter.
                let above = at.offset(0, 1, 0);
                let sky_open = above.is_some_and(|pos| self.light_at_pos(pos).1 == 15);
                let blooming = self.bloom_at_surface(at.surface()) > 0.0;
                if Some(b) == grass_id
                    && living
                    && season != 3
                    && sky_open
                    && above.is_some_and(|pos| self.get_block_at(pos) == AIR)
                    // A bloom is the wild's OWN doing: it ignores the
                    // resentment gate (never the built-country one).
                    && (self.regional_ire_at_surface(at.surface()) <= 2.0 || blooming)
                    && !self.player_touched.contains(&pos)
                {
                    // Post-wrath country erupts: flowers first.
                    if blooming {
                        *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                        let fr = *rng;
                        if ((fr >> 8) as f32 / (1 << 24) as f32) < 0.35 {
                            let flower = if fr.is_multiple_of(2) {
                                reg.block_id("base:meadow_bloom")
                            } else {
                                reg.block_id("base:ember_poppy")
                            };
                            if let (Some(f), Some(above)) = (flower, above) {
                                swaps.push((above, f));
                                continue;
                            }
                        }
                    }
                    *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                    let tide = if blooming { 0.24 } else { 0.06 };
                    if ((*rng >> 8) as f32 / (1 << 24) as f32) < tide {
                        // A parent within reach, and room to breathe.
                        let mut parent: Option<BlockId> = None;
                        let mut crowd = 0;
                        for dx in -5i32..=5 {
                            for dz in -5i32..=5 {
                                for dy in 0..=6 {
                                    let Some(sample) = at.offset(dx, dy, dz) else {
                                        continue;
                                    };
                                    let nb = self.get_block_at(sample);
                                    let name = &reg.block(nb).name;
                                    if name.ends_with("log") {
                                        parent.get_or_insert(nb);
                                        if dy == 0 || dy == 1 {
                                            crowd += 1;
                                        }
                                    }
                                    if reg.block(nb).sapling.is_some() {
                                        crowd += 1;
                                    }
                                }
                            }
                        }
                        if let Some(log) = parent
                            && crowd < 4
                        {
                            let ln = reg.block(log).name.clone();
                            let sap = match ln.as_str() {
                                "base:birch_log" => "base:birch_sapling",
                                "base:spruce_log" => "base:spruce_sapling",
                                "base:jungle_log" => "base:jungle_sapling",
                                "base:acacia_log" => "base:acacia_sapling",
                                _ => "base:oak_sapling",
                            };
                            if let (Some(sb), Some(above)) = (reg.block_id(sap), above) {
                                changes.push((above, sb));
                            }
                        }
                    }
                    continue;
                }
                // Winter freezes exposed still water outside the warm
                // belts; spring gives the lakes back.
                if d.water_level == Some(0)
                    && !d.lava
                    && season == 3
                    && sky_open
                    && above.is_some_and(|pos| self.get_block_at(pos) == AIR)
                    && local_weather.temperature_c <= 0.0
                {
                    if ice.is_some() {
                        freezes.push(at);
                    }
                    continue;
                }
                if Some(b) == ice
                    && (season == 0 || season == 1)
                    && sky_open
                    && local_weather.temperature_c > 1.0
                {
                    melts.push(at);
                    continue;
                }
                // Snow layers melt under bright light or a warm season
                // (footprints melt with them).
                if Some(b) == snow_layer || Some(b) == snow_trod {
                    let (bl, _) = self.light_at_pos(at);
                    let warm = season != 3 && local_weather.temperature_c > 1.0;
                    if bl >= 12 || warm {
                        snow_melts.push(at);
                    }
                    continue;
                }
                // Exposed detailed water evaporates into the same atlas
                // atmosphere. Depth is not an exemption: large bodies last
                // because their committed volume is large.
                if reg.water_volume(b).is_some()
                    && season != 3
                    && sky_open
                    && above.is_some_and(|pos| self.get_block_at(pos) == AIR)
                    && local_weather.temperature_c > 5.0
                {
                    evaporates.push(at);
                }
            }
        }
        for (pos, b) in changes {
            // A crop reaching its final stage refunds ire (capped daily).
            let (family, final_stage, any_soil) = {
                let d = self.reg.block(b);
                (d.crop_family, d.crop_next.is_none(), d.crop_any_soil)
            };
            if final_stage {
                self.plant_ire_at_surface(pos.surface(), 0.5);
            }
            // A maturing crop drew its meal from the soil below —
            // and stamped its family there for the rotation ledger.
            if final_stage
                && family != 0
                && !any_soil
                && let Some(below) = pos.offset(0, -1, 0)
            {
                let sb = self.get_block_at(below);
                if self.reg.block(sb).fert_tiles.is_some() {
                    let meta = soil::soil_after_harvest(self.get_meta_at(below), family);
                    self.set_block_meta_at(below, sb, meta);
                }
            }
            self.set_block_at(pos, b);
        }
        for (pos, gain) in fed {
            self.feed_soil_at(pos, gain);
        }
        for (pos, b) in swaps {
            self.set_block_at(pos, b);
        }
        for (at, item) in drops {
            let reg = self.reg.clone();
            self.push_drop_at(at, crate::inventory::ItemStack::new(&reg, item, 1));
        }
        if let Some(ice) = ice {
            for pos in freezes {
                self.freeze_water_at(pos, ice);
            }
        }
        for pos in melts {
            self.melt_ice_at(pos);
        }
        for pos in snow_melts {
            self.melt_snow_at(pos);
        }
        for pos in evaporates {
            self.evaporate_water_hu_at(pos, crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL);
        }
        for (pos, _species, rnd) in saplings {
            self.try_grow_sapling_at(pos, rnd);
        }
        samples
    }

    /// The last random-tick stamp for a chunk.
    #[cfg(test)]
    pub fn chunk_stamp(&self, x: i32, z: i32) -> Option<f64> {
        self.last_random.get(&ChunkPos::of_world(x, z)).copied()
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
        let day_len = crate::server::DAY_LENGTH as f64;
        // Sudden wholesale phase changes want a real absence behind
        // them; short gaps stay with the gradual burst mechanism.
        let phase = elapsed >= 2.0 * day_len;
        // Expected random-tick visits per block per in-game day.
        let ticks_per_day =
            RANDOM_TICKS_PER_CHUNK_SEC * day_len / (CHUNK_X * CHUNK_Z * CHUNK_Y) as f64;
        // Deterministic per-(world, chunk, day) randomness.
        let mut r = self
            .seed
            .wrapping_mul(31)
            .wrapping_add(pos.u() as u32)
            .wrapping_mul(31)
            .wrapping_add(pos.v() as u32)
            .wrapping_add((pos.face() as u32).wrapping_mul(0x9e37_79b9))
            .wrapping_mul(31)
            .wrapping_add(self.calendar_state.day());
        // Seasons of the missed days, capped at two years back —
        // beyond that the expectations saturate anyway.
        let end_day = (self.calendar_state.clock() / day_len) as i64;
        let start_day = ((self.calendar_state.clock() - elapsed) / day_len) as i64;
        let chunk_center = crate::planet::SurfacePos::new(
            pos.face(),
            pos.u() * CHUNK_X as u16 + CHUNK_X as u16 / 2,
            pos.v() * CHUNK_Z as u16 + CHUNK_Z as u16 / 2,
        )
        .expect("chunk center is canonical");
        let latitude = self.latitude_at_surface(chunk_center);
        let missed_days: Vec<u32> = (start_day..end_day)
            .rev()
            .take(96)
            .map(|d| d.max(0) as u32)
            .collect();
        let days: Vec<usize> = missed_days
            .iter()
            .map(|&d| {
                if self.calendar_state.long_winter() {
                    3
                } else {
                    crate::planet_atlas::local_season(d, latitude)
                }
            })
            .collect();

        // One pass over the chunk collects the cells the rules touch.
        let mut interesting: Vec<(BlockPos, BlockId)> = Vec::new();
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
                            || (d.water_level == Some(0) && !d.lava)
                            || Some(b) == ice
                            || Some(b) == snow_layer
                            || Some(b) == snow_trod
                            || Some(b) == farmland
                        {
                            interesting.push((
                                BlockPos::new(
                                    pos.face(),
                                    pos.u() * CHUNK_X as u16 + lx as u16,
                                    y as u8,
                                    pos.v() * CHUNK_Z as u16 + lz as u16,
                                )
                                .expect("a chunk cell is canonical"),
                                b,
                            ));
                        }
                    }
                }
            }
        }

        let mut changes = Vec::new();
        let mut water_freezes: Vec<BlockPos> = Vec::new();
        let mut ice_melts: Vec<BlockPos> = Vec::new();
        let mut offline_snow_melts: Vec<BlockPos> = Vec::new();
        let mut grow: Vec<(BlockPos, u32)> = Vec::new();
        let mut refunds = 0u32;
        let mut drains: Vec<(BlockPos, u8)> = Vec::new();
        let mut rested: Vec<(BlockPos, u8)> = Vec::new();
        for (at, b) in interesting {
            let d = reg.block(b);
            let season = self.season_at_surface(at.surface());
            let local_weather = self.weather_at_surface(at.surface());
            if Some(b) == farmland {
                // An absent field rests: recovery integrated over the
                // missed days (winter days restore double), only when
                // nothing grows on it.
                let above = at.offset(0, 1, 0).map_or(AIR, |pos| self.get_block_at(pos));
                let resting = above == AIR || Some(above) == snow_layer || Some(above) == snow_trod;
                if resting {
                    let sample = self
                        .planet_atlas
                        .as_ref()
                        .map(|atlas| atlas.biome_sample(at.surface()));
                    let moisture = self.managed_soil_moisture_at(at).clamp(0.0, 1.0);
                    let warmth = ((local_weather.temperature_c + 4.0) / 22.0).clamp(0.1, 1.0);
                    let organic = sample.map_or(0.7, |soil| {
                        (f64::from(soil.organic) / 160.0).clamp(0.2, 1.2)
                    });
                    let weight: f64 = days
                        .iter()
                        .map(|&s| if s == 3 { 2.0 } else { 1.0 })
                        .sum::<f64>();
                    let e = ticks_per_day
                        * 0.5
                        * weight
                        * soil::FERT_FALLOW as f64
                        * f64::from(moisture * warmth)
                        * organic;
                    let k = poisson(e, &mut r).min(soil::FERT_MAX as u32) as u8;
                    if k > 0 {
                        rested.push((at, k));
                    }
                }
                continue;
            }
            if d.sapling.is_some() {
                let e = days.len() as f64 * ticks_per_day * 0.02;
                if poisson(e, &mut r) > 0 {
                    r = r.wrapping_mul(1664525).wrapping_add(1013904223);
                    grow.push((at, r));
                }
                continue;
            }
            if d.crop_next.is_some() {
                let below = at.offset(0, -1, 0);
                let soil_ok = d.crop_any_soil
                    || below.is_some_and(|pos| farmland == Some(self.get_block_at(pos)));
                if !soil_ok {
                    continue;
                }
                let mut sum = 0.0;
                let (block_light, sky_light) = self.light_at_pos(at);
                let protected = block_light >= 10
                    && (sky_light < 15
                        || (1..=16)
                            .filter_map(|dy| at.offset(0, dy, 0))
                            .any(|pos| reg.block(self.get_block_at(pos)).glass));
                let light_mult = (f32::from(block_light.max(sky_light)) / 12.0).clamp(0.0, 1.0);
                let moisture_mult = below
                    .map_or_else(
                        || self.soil_moisture_at_surface(at.surface()),
                        |soil_pos| self.managed_soil_moisture_at(soil_pos),
                    )
                    .clamp(0.0, 1.25);
                let soil_mult = if d.crop_any_soil {
                    1.0
                } else {
                    below.map_or(0.0, |soil_pos| {
                        soil::fert_mult(soil::fert_of(self.get_meta_at(soil_pos)))
                            * self.crop_soil_multiplier_at(soil_pos)
                    })
                };
                for (&day, &s) in missed_days.iter().zip(&days) {
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
                        let (bl, sl) = self.light_at_pos(at);
                        if sl < 15 && bl >= 10 {
                            0.5
                        } else if sl == 15
                            && (1..=16)
                                .filter_map(|dy| at.offset(0, dy, 0))
                                .any(|pos| reg.block(self.get_block_at(pos)).glass)
                        {
                            0.75
                        } else {
                            0.0
                        }
                    } else {
                        mult
                    };
                    let effective_temperature = self
                        .temperature_at_surface_on_day(at.surface(), f64::from(day))
                        + if protected { 10.0 } else { 0.0 };
                    let temperature_mult = if effective_temperature <= 0.0 {
                        0.0
                    } else if effective_temperature < 14.0 {
                        effective_temperature / 14.0
                    } else if effective_temperature <= 29.0 {
                        1.0
                    } else {
                        ((42.0 - effective_temperature) / 13.0).clamp(0.0, 1.0)
                    };
                    sum += mult * temperature_mult * light_mult * moisture_mult * soil_mult;
                }
                let k = poisson(
                    ticks_per_day * d.crop_chance as f64 * f64::from(sum),
                    &mut r,
                );
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
                        // live random tick would have — and drains the
                        // soil it grew from, stamping the rotation.
                        let fd = reg.block(cur);
                        if fd.crop_next.is_none() {
                            refunds += 1;
                            if fd.crop_family != 0
                                && !fd.crop_any_soil
                                && let Some(below) = below
                            {
                                drains.push((below, fd.crop_family));
                            }
                        }
                        changes.push((at, cur));
                    }
                }
                continue;
            }
            if !phase {
                continue;
            }
            let above = at.offset(0, 1, 0);
            let sky_open = above.is_some_and(|pos| self.light_at_pos(pos).1 == 15);
            if d.water_level == Some(0)
                && !d.lava
                && season == 3
                && sky_open
                && above.is_some_and(|pos| self.get_block_at(pos) == AIR)
                && local_weather.temperature_c <= 0.0
            {
                if ice.is_some() {
                    water_freezes.push(at);
                }
                continue;
            }
            if Some(b) == ice
                && (season == 0 || season == 1)
                && sky_open
                && local_weather.temperature_c > 1.0
            {
                ice_melts.push(at);
                continue;
            }
            if Some(b) == snow_layer || Some(b) == snow_trod {
                let (bl, _) = self.light_at_pos(at);
                let warm = season != 3 && local_weather.temperature_c > 1.0;
                if bl >= 12 || warm {
                    offline_snow_melts.push(at);
                }
            }
        }
        // Batched apply: a frozen lake is many cells — one relight,
        // not one per cell.
        let any = !changes.is_empty();
        for (at, nb) in changes {
            let (lx, y, lz) = at.local();
            if let Some(c) = self.chunks.get_mut(&at.chunk()) {
                c.set(lx, y, lz, nb);
                c.dirty = true;
                c.modified = true;
                if self.log_edits {
                    self.edit_log.push((at, nb, 0, 0, 0));
                }
            }
            self.wake_water_at(at);
        }
        if let Some(ice) = ice {
            for at in water_freezes {
                self.freeze_water_at(at, ice);
            }
        }
        for at in ice_melts {
            self.melt_ice_at(at);
        }
        for at in offline_snow_melts {
            self.melt_snow_at(at);
        }
        if any {
            self.relight_and_cascade(pos);
        }
        for _ in 0..refunds {
            // Reconciled growth credits the chunk's own country.
            let center = SurfacePos::new(
                pos.face(),
                pos.u() * CHUNK_X as u16 + CHUNK_X as u16 / 2,
                pos.v() * CHUNK_Z as u16 + CHUNK_Z as u16 / 2,
            )
            .expect("chunk center is canonical");
            self.plant_ire_at_surface(center, 0.5);
        }
        for (at, family) in drains {
            let sb = self.get_block_at(at);
            if self.reg.block(sb).fert_tiles.is_some() {
                let meta = soil::soil_after_harvest(self.get_meta_at(at), family);
                self.set_block_meta_at(at, sb, meta);
            }
        }
        for (at, gain) in rested {
            self.feed_soil_at(at, gain);
        }
        for (at, rnd) in grow {
            self.try_grow_sapling_at(at, rnd);
        }
    }

    /// A footstep through a snow layer presses it into a trodden
    /// print — a real edit: logged, broadcast, persisted, and it melts
    /// like any layer. History written in the ground.
    pub fn tread_at(&mut self, pos: BlockPos) {
        let (Some(layer), Some(trod)) = (
            self.reg.block_id("base:snow_layer"),
            self.reg.block_id("base:snow_layer_trod"),
        ) else {
            return;
        };
        if self.get_block_at(pos) == layer {
            self.set_block_at(pos, trod);
        }
    }

    /// One flake of consequence: lay a snow layer on this column's
    /// surface if the storm is cold here and the sky can reach it.
    pub fn settle_snow_at(&mut self, surface: SurfacePos) {
        if !self.snows_at_surface(surface) {
            return;
        }
        let Some(layer) = self.reg.block_id("base:snow_layer") else {
            return;
        };
        let y = self.surface_height_at(surface);
        if y <= SEA_LEVEL || y + 1 >= CHUNK_Y as i32 - 1 {
            return;
        }
        let Ok(pos) = BlockPos::new(surface.face(), surface.u(), (y + 1) as u8, surface.v()) else {
            return;
        };
        if self.get_block_at(pos) != AIR || self.light_at_pos(pos).1 != 15 {
            return;
        }
        if self.claim_precipitation_transfer(
            surface,
            crate::planet_atlas::PrecipitationForm::Snow,
            crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL as u32,
        ) != crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL as u32
        {
            return;
        }
        self.set_block_water_at(pos, layer, 0, 0);
    }

    /// Rain refills the water it lands on: the first surface the
    /// column offers, if partial water or a film, gains one unit —
    /// ponds creep back toward full through a wet autumn. A dry
    /// pothole catches the rain too: a solid floor whose open cell
    /// has 3+ solid walls seeds a fresh film, and a drained basin
    /// rebuilds from its corners, rain by rain.
    pub fn rain_fill_at(&mut self, surface: SurfacePos) {
        if self.snows_at_surface(surface) || !self.rains_at_surface(surface) {
            return;
        }
        for y in (1..CHUNK_Y as i32).rev() {
            let pos = BlockPos::new(surface.face(), surface.u(), y as u8, surface.v())
                .expect("rain scan height is inside the world");
            let b = self.get_block_at(pos);
            if b == AIR {
                continue;
            }
            if let Some(v) = self.reg.water_volume(b)
                && v < 8
            {
                if self.claim_precipitation_transfer(
                    surface,
                    crate::planet_atlas::PrecipitationForm::Rain,
                    crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL as u32,
                ) == crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL as u32
                {
                    let mut mass = self.water_mass_at(pos).unwrap_or_default();
                    mass.add_assign(crate::planet_atlas::ReservoirMass::fresh(
                        crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL,
                    ))
                    .expect("one voxel water parcel fits");
                    self.write_water_mass_at(pos, mass);
                }
            } else if self.reg.is_solid(b)
                && y + 1 < CHUNK_Y as i32
                && [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .filter(|&&(dx, dz)| {
                        pos.offset(dx, 1, dz)
                            .is_some_and(|at| self.reg.is_solid(self.get_block_at(at)))
                    })
                    .count()
                    >= 3
                && let Some(above) = pos.offset(0, 1, 0)
                && self.claim_precipitation_transfer(
                    surface,
                    crate::planet_atlas::PrecipitationForm::Rain,
                    crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL as u32,
                ) == crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL as u32
            {
                self.write_water_mass_at(
                    above,
                    crate::planet_atlas::ReservoirMass::fresh(
                        crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL,
                    ),
                );
            }
            return;
        }
    }

    #[cfg(test)]
    pub fn rain_fill(&mut self, x: i32, z: i32) {
        if let Ok(surface) = SurfacePos::from_centered(crate::planet::Face::PosZ, x, z) {
            self.rain_fill_at(surface);
        }
    }

    /// Attempt to mature the sapling at this position. On success the
    /// tree is built and the wild refunds -2 ire, bypassing the daily
    /// planting cap (it took days — it IS the slow path).
    pub fn try_grow_sapling_at(&mut self, pos: BlockPos, rnd: u32) -> bool {
        let b = self.get_block_at(pos);
        let Some(species) = self.reg.block(b).sapling.clone() else {
            return false;
        };
        let target = match species.as_str() {
            "spruce" => crate::worldgen::Biome::Taiga,
            "jungle" => crate::worldgen::Biome::Jungle,
            "acacia" => crate::worldgen::Biome::Savanna,
            _ => crate::worldgen::Biome::Forest,
        };
        let compatibility = self.generator.graft_compatibility_at(pos.surface(), target);
        let (block_light, sky_light) = self.light_at_pos(pos);
        let sheltered = block_light >= 9
            || sky_light < 15
            || (1..=16)
                .filter_map(|dy| pos.offset(0, dy, 0))
                .any(|above| self.reg.block(self.get_block_at(above)).glass);
        let watered = self.managed_soil_moisture_at(pos) >= 0.85;
        if matches!(
            compatibility,
            crate::planet_atlas::GraftCompatibility::Marginal
        ) && !watered
            || matches!(
                compatibility,
                crate::planet_atlas::GraftCompatibility::Incompatible
            ) && !(watered && sheltered)
        {
            return false;
        }
        if self.grow_tree_at(pos, &species, rnd) {
            self.add_ire_at_surface(pos.surface(), -2.0);
            true
        } else {
            false
        }
    }

    #[cfg(test)]
    pub fn try_grow_sapling(&mut self, x: i32, y: i32, z: i32, rnd: u32) -> bool {
        BlockPos::of_world(x, y, z).is_some_and(|pos| self.try_grow_sapling_at(pos, rnd))
    }
}
