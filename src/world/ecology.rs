//! Wildlife seeding, mob/projectile ticking, and hostile spawning.

use super::*;
use crate::planet::{BlockPos, EntityPos, SurfacePos};

impl World {
    pub fn mobs(&self) -> &[Mob] {
        &self.mobs
    }

    pub(crate) fn mobs_mut(&mut self) -> &mut Vec<Mob> {
        &mut self.mobs
    }

    pub fn mob(&self, index: usize) -> Option<&Mob> {
        self.mobs.get(index)
    }

    pub fn mob_mut(&mut self, index: usize) -> Option<&mut Mob> {
        self.mobs.get_mut(index)
    }

    pub fn mob_by_id(&self, id: u32) -> Option<&Mob> {
        self.mobs.iter().find(|m| m.id == id)
    }

    pub fn mob_by_id_mut(&mut self, id: u32) -> Option<&mut Mob> {
        self.mobs.iter_mut().find(|mob| mob.id == id)
    }

    pub fn mob_count(&self) -> usize {
        self.mobs.len()
    }

    pub fn spawn_mob(&mut self, mob: Mob) {
        self.mobs.push(mob);
    }

    pub fn remove_mob(&mut self, index: usize) -> Mob {
        self.mobs.swap_remove(index)
    }

    pub fn replace_mobs(&mut self, mobs: Vec<Mob>) {
        self.mobs = mobs;
    }

    pub fn for_each_mob_mut(&mut self, mut update: impl FnMut(&mut Mob)) {
        for mob in &mut self.mobs {
            update(mob);
        }
    }

    pub fn projectiles(&self) -> &[Projectile] {
        &self.projectiles
    }

    pub fn spawn_projectile(&mut self, projectile: Projectile) {
        self.projectiles.push(projectile);
    }

    pub fn replace_projectiles(&mut self, projectiles: Vec<Projectile>) {
        self.projectiles = projectiles;
    }

    pub fn for_each_projectile_mut(&mut self, mut update: impl FnMut(&mut Projectile)) {
        for projectile in &mut self.projectiles {
            update(projectile);
        }
    }

    pub(super) fn mob_hash_at(&self, pos: SurfacePos, salt: u32) -> u32 {
        let mut h = u32::from(pos.u()).wrapping_mul(0x85eb_ca6b)
            ^ u32::from(pos.v()).wrapping_mul(0xc2b2_ae35)
            ^ (pos.face() as u32).wrapping_mul(0x27d4_eb2d)
            ^ self.seed.wrapping_mul(0x9e37_79b9)
            ^ salt.wrapping_mul(0x2708_92cd);
        h ^= h >> 15;
        h = h.wrapping_mul(0x2c1b_3c6d);
        h ^= h >> 12;
        h
    }

    /// Deterministic per-chunk wildlife roll: at most one species' group.
    pub(super) fn seed_wildlife(&mut self, pos: ChunkPos) {
        if self.mobs.len() >= MOB_CAP {
            return;
        }
        // Dead country restocks nothing.
        let center = SurfacePos::new(
            pos.face(),
            pos.u() * CHUNK_X as u16 + CHUNK_X as u16 / 2,
            pos.v() * CHUNK_Z as u16 + CHUNK_Z as u16 / 2,
        )
        .expect("chunk center is canonical");
        if !self.heart_alive_at_surface(center) {
            return;
        }
        let reg = self.reg.clone();
        // What a country IS, not what the map first called it: a
        // grafted heart drags its country's life after it.
        let biome = self.country_biome_at(center).name().to_lowercase();
        // Open sea keeps its own roster. A country's culture is a fact
        // about its land, and the water over a drowned shelf belongs to
        // neither the forest behind it nor the deer in that forest.
        let here = if self.is_open_water_at(center) {
            "ocean".to_string()
        } else {
            biome.clone()
        };
        for (si, def) in reg.animals.iter().enumerate() {
            // Wildlife only — wardens come and go with the spawner.
            // Swimmers roll in the water pass below; letting them share
            // this slot meant a fish only ever spawned in a chunk where
            // every land animal of the biome had already failed its
            // rarity roll, which is why the sea looked empty.
            if def.hostile || def.movement_swim || !def.biomes.contains(&here) {
                continue;
            }
            let roll = self.mob_hash_at(center, 7000 + si as u32);
            if !roll.is_multiple_of(def.rarity) {
                continue;
            }
            let span = def.group[1].saturating_sub(def.group[0]) + 1;
            let n = def.group[0] + (roll >> 8) % span;
            for i in 0..n {
                let h = self.mob_hash_at(center, 7100 + si as u32 * 31 + i);
                let surface = SurfacePos::new(
                    pos.face(),
                    pos.u() * CHUNK_X as u16 + (h % CHUNK_X as u32) as u16,
                    pos.v() * CHUNK_Z as u16 + ((h >> 8) % CHUNK_Z as u32) as u16,
                )
                .expect("sampled wildlife column is canonical");
                self.try_spawn_at(si, surface, (h >> 16) as f32 / 65535.0);
            }
            break; // one species per chunk keeps groups readable
        }
        // The water has its own roster, rolled independently of the land
        // above it — a chunk can carry deer on the bank and trout in the
        // river. Fresh water stocks the country's fish; salt water its
        // own.
        let swimmers: Vec<usize> = reg
            .animals
            .iter()
            .enumerate()
            .filter_map(|(index, def)| {
                (!def.hostile && def.movement_swim && def.biomes.contains(&here)).then_some(index)
            })
            .collect();
        let swimmer_start = if swimmers.is_empty() {
            0
        } else {
            // Use the high half of the mixed hash. Taking `% 2` here
            // accidentally made the low-bit quality of the coordinate
            // hash decide between the two ocean fish, and one species
            // could win every early chunk before the aquatic budget
            // filled.
            ((u64::from(self.mob_hash_at(center, 9_101)) * swimmers.len() as u64) >> 32) as usize
        };
        for step in 0..swimmers.len() {
            let si = swimmers[(swimmer_start + step) % swimmers.len()];
            let def = &reg.animals[si];
            let roll = self.mob_hash_at(center, 9200 + si as u32);
            if !roll.is_multiple_of(def.rarity) {
                continue;
            }
            let span = def.group[1].saturating_sub(def.group[0]) + 1;
            let n = def.group[0] + (roll >> 8) % span;
            let mut spawned = false;
            for i in 0..n {
                let h = self.mob_hash_at(center, 9300 + si as u32 * 31 + i);
                let surface = SurfacePos::new(
                    pos.face(),
                    pos.u() * CHUNK_X as u16 + (h % CHUNK_X as u32) as u16,
                    pos.v() * CHUNK_Z as u16 + ((h >> 8) % CHUNK_Z as u32) as u16,
                )
                .expect("sampled fish column is canonical");
                // Dry chunks simply fail every attempt: try_spawn wants a
                // water cell two deep and finds none.
                spawned |= self.try_spawn_at(si, surface, (h >> 16) as f32 / 65535.0);
            }
            // One shoal per chunk. Candidate order rotates by canonical
            // address so the first fish in the data file cannot fill the
            // global mob cap before later ocean natives ever get a turn.
            if spawned {
                break;
            }
        }
        // The dark has its own roster: underground species roll
        // independently of the surface (a chunk can carry deer above
        // and a bat colony below).
        for (si, def) in reg.animals.iter().enumerate() {
            if def.hostile || def.biomes.iter().all(|b| b != "underground") {
                continue;
            }
            let roll = self.mob_hash_at(center, 8600 + si as u32);
            if !roll.is_multiple_of(def.rarity) {
                continue;
            }
            let span = def.group[1].saturating_sub(def.group[0]) + 1;
            let n = def.group[0] + (roll >> 8) % span;
            for i in 0..n {
                let h = self.mob_hash_at(center, 8700 + si as u32 * 31 + i);
                let surface = SurfacePos::new(
                    pos.face(),
                    pos.u() * CHUNK_X as u16 + (h % CHUNK_X as u32) as u16,
                    pos.v() * CHUNK_Z as u16 + ((h >> 8) % CHUNK_Z as u32) as u16,
                )
                .expect("sampled cave column is canonical");
                // A pocket of cave: two air cells under a solid roof.
                let base = 8 + (h >> 16) % 32;
                let spot = (base as i32..(base as i32 + 24).min(52)).find(|&y| {
                    let at =
                        BlockPos::new(surface.face(), surface.u(), y as u8, surface.v()).unwrap();
                    self.get_block_at(at) == AIR
                        && at
                            .offset(0, 1, 0)
                            .is_some_and(|p| self.get_block_at(p) == AIR)
                        && at
                            .offset(0, 2, 0)
                            .is_some_and(|p| self.reg.is_solid(self.get_block_at(p)))
                });
                if let Some(y) = spot
                    && self.mobs.len() < MOB_CAP
                {
                    let at = EntityPos::new(
                        surface.face(),
                        f32::from(surface.u()) + 0.5,
                        y as f32 + 0.4,
                        f32::from(surface.v()) + 0.5,
                    )
                    .expect("cave spawn is canonical");
                    let mut m = Mob::new_at(si, at, (h >> 12) as f32);
                    m.health = reg.animals[si].health;
                    self.mobs.push(m);
                }
            }
        }
    }

    /// Spawn on dry solid ground at the surface — or, for swimmers,
    /// submerged in a water column at least two deep. Skips bad spots.
    pub(super) fn try_spawn_at(&mut self, species: usize, surface: SurfacePos, yaw01: f32) -> bool {
        if self.mobs.len() >= MOB_CAP {
            return false;
        }
        let swim = self
            .reg
            .animals
            .get(species)
            .is_some_and(|d| d.movement_swim);
        // Category budget: a full lake never starves the land spawns.
        if swim {
            let reg = self.reg.clone();
            let fish = self
                .mobs
                .iter()
                .filter(|m| reg.animals.get(m.species).is_some_and(|d| d.movement_swim))
                .count();
            if fish >= 90 {
                return false;
            }
        }
        let spawn_at = if swim {
            // The first water cell from the sky down, needing depth.
            (4..=96)
                .rev()
                .filter_map(|y| {
                    BlockPos::new(surface.face(), surface.u(), y, surface.v())
                        .ok()
                        .map(|pos| (pos, self.get_block_at(pos)))
                })
                .find(|&(_, b)| self.reg.is_water(b))
                .filter(|&(pos, _)| {
                    pos.offset(0, -1, 0)
                        .is_some_and(|below| self.reg.is_water(self.get_block_at(below)))
                })
                .map(|(pos, _)| f32::from(pos.y()) - 0.6)
        } else {
            let y = self.surface_height_at(surface);
            let ground = BlockPos::new(surface.face(), surface.u(), y as u8, surface.v()).unwrap();
            let dry = y > SEA_LEVEL && self.reg.is_solid(self.get_block_at(ground));
            // A seabird has nowhere to stand, and the whole point of it
            // is that it is over the water. Wings only need air.
            let airborne = self.reg.animals[species].movement_float
                && BlockPos::new(
                    surface.face(),
                    surface.u(),
                    (SEA_LEVEL - 1) as u8,
                    surface.v(),
                )
                .is_ok_and(|pos| self.reg.is_water(self.get_block_at(pos)));
            if dry {
                Some(y as f32 + 1.05)
            } else if airborne {
                Some(SEA_LEVEL as f32 + 4.0)
            } else {
                None
            }
        };
        let Some(sy) = spawn_at else {
            return false;
        };
        let pos = EntityPos::new(
            surface.face(),
            f32::from(surface.u()) + 0.5,
            sy,
            f32::from(surface.v()) + 0.5,
        )
        .expect("wildlife spawn is canonical");
        let mut m = Mob::new_at(species, pos, yaw01 * std::f32::consts::TAU);
        m.health = self.reg.animals[species].health;
        self.mobs.push(m);
        true
    }

    /// Tick AI/physics for all mobs, plus the slow repopulation roll.
    /// Returns events (player hits, projectile casts) for the game loop.
    pub fn tick_mobs(
        &mut self,
        players: &[crate::server::PlayerCtx],
        daylight: f32,
        dt: f32,
        rng: &mut u32,
    ) -> Vec<MobEvent> {
        let fallback_player = players.first().map(|p| p.pos);
        let reg = self.reg.clone();
        let mut events = Vec::new();
        // Stamp stable ids on anything new (spawns, births, loaded saves).
        for m in &mut self.mobs {
            if m.id == 0 {
                m.id = self.next_mob_id;
                self.next_mob_id += 1;
            }
        }
        // Herd pulls are averaged in each animal's local tangent frame.
        // This costs little at the mob cap and lets a herd straddle a face
        // seam without splitting into two coordinate buckets.
        let herd_members: Vec<(usize, EntityPos)> = self
            .mobs
            .iter()
            .filter_map(|m| {
                let d = reg.animals.get(m.species)?;
                (!d.hostile && !d.vehicle && d.group[1] >= 2 && m.growth >= 1.0)
                    .then_some((m.species, m.pos))
            })
            .collect();
        // The trophic pre-pass: hungry predators pick their quarry,
        // desperation is graded (deep hunger plus night or winter),
        // and prey with a stalker on top of it bolts.
        let winter = self.season() == 3;
        let night = daylight < 0.35;
        let snapshot: Vec<(u32, usize, crate::planet::EntityPos)> =
            self.mobs.iter().map(|m| (m.id, m.species, m.pos)).collect();
        let mut spooked: Vec<(u32, crate::planet::EntityPos)> = Vec::new();
        for m in &mut self.mobs {
            let Some(d) = reg.animals.get(m.species) else {
                continue;
            };
            m.bold = !d.hostile
                && !d.fierce
                && !d.prey.is_empty()
                && d.attack > 0.0
                && m.belly < crate::mobs::BELLY_DESPERATE
                && (winter || night);
            if d.prey.is_empty() || d.belly_secs <= 0.0 || m.belly > 0.0 || m.growth < 1.0 {
                m.quarry = None;
                continue;
            }
            let mut best: Option<(u32, crate::planet::EntityPos, f32)> = None;
            for &(id, sp, pos) in &snapshot {
                if id == m.id || !d.prey.contains(&sp) {
                    continue;
                }
                let delta = m.pos.local_delta_to(pos);
                let dist = delta.length();
                if dist < crate::mobs::HUNT_RANGE && best.is_none_or(|(_, _, bd)| dist < bd) {
                    best = Some((id, pos, dist));
                }
            }
            m.quarry = best.map(|(id, pos, _)| (id, pos));
            if m.state == crate::mobs::MobState::Stalk
                && let Some((id, _, dist)) = best
                && dist < 7.0
            {
                spooked.push((id, m.pos));
            }
        }
        for (id, from) in spooked {
            if let Some(p) = self.mob_by_id_mut(id)
                && p.state != crate::mobs::MobState::Flee
            {
                p.state = crate::mobs::MobState::Flee;
                p.state_timer = 4.0;
                p.target = from;
            }
        }
        let mut mobs = std::mem::take(&mut self.mobs);
        for m in &mut mobs {
            // Frozen until its chunk streams in: an unloaded chunk reads as
            // air, and ticking against it drops the mob through the world.
            let Some(cp) = m.pos.chunk() else {
                continue;
            };
            if !self.chunks.contains_key(&cp) {
                continue;
            }
            if let Some(def) = reg.animals.get(m.species) {
                let mut sum = glam::Vec3::ZERO;
                let mut count = 0.0;
                for &(species, other) in &herd_members {
                    if species == m.species {
                        let delta = m.pos.local_delta_to(other);
                        if delta.length_squared() <= 32.0 * 32.0 {
                            sum += delta;
                            count += 1.0;
                        }
                    }
                }
                m.herd_pull = (count >= 2.0)
                    .then(|| m.pos.translated(sum / count).ok().map(|moved| moved.pos))
                    .flatten();
                m.unstick(self, def);
                m.tick(self, def, players, dt, rng, &mut events);
            }
        }
        // The kill lands: the prey leaves a carcass where it fell
        // (a scavenged carcass just goes — never a carcass's
        // carcass), and a laden pack spills. Predation moves the
        // ire meter not at all: the wild's own violence is its own.
        let killed: Vec<u32> = events
            .iter()
            .filter_map(|e| match e {
                MobEvent::Killed(id) => Some(*id),
                _ => None,
            })
            .collect();
        events.retain(|e| !matches!(e, MobEvent::Killed(_)));
        for id in killed {
            if let Some(i) = mobs.iter().position(|m| m.id == id) {
                let prey = mobs.swap_remove(i);
                let at = prey.pos.block();
                if let Some(cargo) = prey.cargo {
                    for st in cargo.into_iter().flatten() {
                        if let Some(at) = at {
                            self.push_drop_at(at, st);
                        }
                    }
                }
                let was_carcass = reg
                    .animals
                    .get(prey.species)
                    .is_some_and(|d| d.name.ends_with(":carcass"));
                if !was_carcass && let Some(ci) = reg.animal_id("base:carcass") {
                    let mut c = Mob::new_at(ci, prey.pos, prey.yaw);
                    c.health = reg.animals[ci].health;
                    c.rot = crate::mobs::CARCASS_ROT_SECS;
                    mobs.push(c);
                }
            }
        }
        // Rot: the ground takes whatever the vultures leave.
        let mut rotted: Vec<crate::planet::EntityPos> = Vec::new();
        mobs.retain_mut(|m| {
            if reg
                .animals
                .get(m.species)
                .is_some_and(|d| d.name.ends_with(":carcass"))
            {
                m.rot -= dt;
                if m.rot <= 0.0 {
                    rotted.push(m.pos);
                    return false;
                }
            }
            true
        });
        for p in rotted {
            if let Some(pos) = p
                .translated(glam::Vec3::new(0.0, -0.5, 0.0))
                .ok()
                .and_then(|moved| moved.pos.block())
            {
                self.feed_soil_at(pos, 8);
            }
        }
        // Wardens are expressions of the wild, not creatures: they dissolve
        // in daylight (sky-lit cells only — torchlight never banishes them)
        // and when the player leaves them far behind.
        mobs.retain(|m| {
            let Some(def) = reg.animals.get(m.species) else {
                return false;
            };
            if m.pos.y() < -20.0 {
                return false; // fell out of the world somehow
            }
            if def.hostile && m.masterless {
                // Left over when the heart died and never recalled:
                // no daylight dissolves them, nothing sends them, and
                // they do not stop. Only distance retires them.
                let near = players
                    .iter()
                    .map(|p| m.pos.local_delta_to(p.pos).length_squared())
                    .fold(f32::INFINITY, f32::min);
                return near <= 120.0 * 120.0;
            }
            if !def.hostile {
                // Fish are ambience-plus-resource: the water has
                // fish while someone's there to see it.
                if def.movement_swim {
                    let near = players
                        .iter()
                        .map(|p| m.pos.local_delta_to(p.pos).length_squared())
                        .fold(f32::INFINITY, f32::min);
                    return near <= 96.0 * 96.0;
                }
                return true;
            }
            let near = players
                .iter()
                .map(|p| m.pos.local_delta_to(p.pos).length_squared())
                .fold(f32::INFINITY, f32::min);
            if near > 80.0 * 80.0 {
                return false;
            }
            let Some(light_pos) = m
                .pos
                .translated(glam::Vec3::new(0.0, 0.5, 0.0))
                .ok()
                .and_then(|p| p.pos.block())
            else {
                return false;
            };
            let (_, sl) = self.light_at_pos(light_pos);
            sl as f32 * daylight < 7.0
        });
        // Husbandry: two fed adults of a species near each other bear
        // young - but not in winter; spring is the birthing season.
        let winter = self.season() == 3;
        let mut births: Vec<(usize, usize)> = Vec::new();
        for i in 0..mobs.len() {
            if winter || births.iter().any(|&(a, b)| a == i || b == i) {
                continue;
            }
            if !mobs[i].fed || mobs[i].growth < 1.0 {
                continue;
            }
            for j in (i + 1)..mobs.len() {
                if mobs[j].species == mobs[i].species
                    && mobs[j].fed
                    && mobs[j].growth >= 1.0
                    && mobs[i].pos.distance_to(mobs[j].pos).powi(2) < 16.0
                {
                    births.push((i, j));
                    break;
                }
            }
        }
        for (i, j) in births {
            let mid = mobs[i]
                .pos
                .translated(mobs[i].pos.local_delta_to(mobs[j].pos) * 0.5)
                .map(|p| p.pos)
                .unwrap_or(mobs[i].pos);
            mobs[i].fed = false;
            mobs[j].fed = false;
            mobs[i].breed_cd = 300.0;
            mobs[j].breed_cd = 300.0;
            if mobs.len() < MOB_CAP {
                let mut baby = Mob::new_at(mobs[i].species, mid, 0.0);
                baby.health = reg.animals[mobs[i].species].health;
                baby.growth = 0.05;
                mobs.push(baby);
                // Life returned to the world.
                self.ire = (self.ire - 1.0).max(0.0);
                events.push(MobEvent::Bred);
            }
        }
        self.mobs = mobs;

        // Repopulation: overhunted wildlife slowly recovers, away from the
        // player and only under the local cap.
        // Spring teems, winter starves: the repop clock runs at double
        // or half speed with the season.
        self.repop_timer += dt
            * match self.season() {
                0 => 2.0,
                3 => 0.5,
                _ => 1.0,
            };
        if self.repop_timer >= 16.0 {
            self.repop_timer = 0.0;
            // One player's ring per cycle, chosen at random — the same shape
            // the warden spawner already uses. Restocking only ever followed
            // players.first(), so on a shared world every guest but one lived
            // in a country that never recovered from being hunted.
            *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
            let player = players
                .get((*rng >> 8) as usize % players.len().max(1))
                .map(|p| p.pos)
                .or(fallback_player);
            let Some(player) = player else {
                return events;
            };
            let near = self
                .mobs
                .iter()
                .filter(|m| m.pos.distance_to(player) < 96.0)
                .count();
            if near < 40 && !reg.animals.is_empty() {
                *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                let r = *rng;
                // A ring 32-72 blocks out at a random angle.
                let ang = (r % 1024) as f32 / 1024.0 * std::f32::consts::TAU;
                let dist = 32.0 + ((r >> 10) % 40) as f32;
                let Some(surface) = player
                    .translated(glam::Vec3::new(ang.sin() * dist, 0.0, ang.cos() * dist))
                    .ok()
                    .and_then(|moved| moved.pos.block())
                    .map(BlockPos::surface)
                else {
                    return events;
                };
                let cp = ChunkPos::from_surface(surface);
                if self.chunks.contains_key(&cp) && self.heart_alive_at_surface(surface) {
                    // Restock what the spot can actually hold: a column
                    // of water gets swimmers, dry ground gets landfolk.
                    // Drawing both from one pool wasted most rolls out at
                    // sea, where every land pick fails to place.
                    let surface_y = self.surface_height_at(surface);
                    let wet = BlockPos::new(
                        surface.face(),
                        surface.u(),
                        (surface_y + 1).clamp(0, CHUNK_Y as i32 - 1) as u8,
                        surface.v(),
                    )
                    .is_ok_and(|pos| self.reg.is_water(self.get_block_at(pos)));
                    let biome = if wet && self.is_open_water_at(surface) {
                        "ocean".to_string()
                    } else {
                        self.country_biome_at(surface).name().to_lowercase()
                    };
                    // Wildlife only — wardens have their own spawner.
                    let eligible: Vec<usize> = reg
                        .animals
                        .iter()
                        .enumerate()
                        .filter(|(_, d)| {
                            let placeable = if wet {
                                // Over water: fish below it, wings above it.
                                d.movement_swim || d.movement_float
                            } else {
                                !d.movement_swim
                            };
                            !d.hostile && placeable && d.biomes.contains(&biome)
                        })
                        .map(|(i, _)| i)
                        .collect();
                    if let Some(&si) = eligible.get(((r >> 20) as usize) % eligible.len().max(1)) {
                        self.try_spawn_at(si, surface, (r >> 8) as f32 / (u32::MAX >> 8) as f32);
                    }
                }
            }
        }
        events
    }

    /// The strike connects: the nearest swimmer within reach of the
    /// bobber leaves the water. Returns its species — real fish get
    /// caught before any luck table gets a say.
    #[cfg(test)]
    pub fn catch_fish_near(&mut self, at: glam::Vec3, radius: f32) -> Option<usize> {
        let at = crate::planet::EntityPos::from_local(crate::planet::Face::PosZ, at).ok()?;
        self.catch_fish_near_at(at, radius)
    }

    pub fn catch_fish_near_at(
        &mut self,
        at: crate::planet::EntityPos,
        radius: f32,
    ) -> Option<usize> {
        let reg = self.reg.clone();
        let idx = self
            .mobs
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                reg.animals.get(m.species).is_some_and(|d| d.movement_swim)
                    && m.pos.distance_to(at) < radius
            })
            .min_by(|(_, a), (_, b)| a.pos.distance_to(at).total_cmp(&b.pos.distance_to(at)))
            .map(|(i, _)| i)?;
        let fish = self.mobs.swap_remove(idx);
        Some(fish.species)
    }

    /// A grazer's bite lands: a grown crop reverts to its planted
    /// base, grass to bare dirt (which heals). The animal never
    /// breaks a placed block — it eats what the plant grew, not what
    /// the farmer built.
    pub fn apply_bite_at(&mut self, pos: crate::planet::BlockPos) {
        let b = self.get_block_at(pos);
        let d = self.reg.block(b);
        if d.crop_family != 0 && d.name.contains("/stage") {
            let base = d.name.split("/stage").next().unwrap_or("").to_string();
            if let Some(base_id) = self.reg.block_id(&base) {
                self.set_block_at(pos, base_id);
            }
            return;
        }
        if d.name == "base:grass"
            && let Some(dirt) = self.reg.block_id("base:dirt")
        {
            self.set_block_at(pos, dirt);
        }
    }

    /// Advance all bolts and arrows; returns (player index, damage) hits.
    /// Player arrows strike mobs through the normal hurt path and stick
    /// into blocks as recoverable item drops.
    pub fn tick_projectiles(
        &mut self,
        players: &[crate::server::PlayerCtx],
        dt: f32,
    ) -> Vec<(usize, f32)> {
        let mut dmg: Vec<(usize, f32)> = Vec::new();
        let mut mob_hits: Vec<(usize, f32, crate::planet::EntityPos)> = Vec::new();
        let mut drops: Vec<(crate::planet::BlockPos, crate::registry::ItemId)> = Vec::new();
        let mut projectiles = std::mem::take(&mut self.projectiles);
        projectiles.retain_mut(|p| match p.tick(self, players, dt) {
            ProjHit::None => true,
            ProjHit::Expired => false,
            ProjHit::Player(i) => {
                dmg.push((i, p.damage));
                false
            }
            ProjHit::Mob(i) => {
                let from = p
                    .pos
                    .translated(-p.vel * dt)
                    .map(|moved| moved.pos)
                    .unwrap_or(p.pos);
                mob_hits.push((i, p.damage, from));
                false
            }
            ProjHit::Block => {
                if let Some(it) = p.drop_item {
                    if p.owner != 0 {
                        // A guest's arrow: hand it back over the wire.
                        let stack = ItemStack::new(&self.reg, it, 1);
                        self.pending_gives.push((p.owner, stack));
                    } else {
                        let back = p
                            .pos
                            .translated(-p.vel * dt * 2.0)
                            .map(|moved| moved.pos)
                            .unwrap_or(p.pos);
                        if let Some(back) = back.block() {
                            drops.push((back, it));
                        }
                    }
                }
                false
            }
        });
        self.projectiles = projectiles;
        let reg = self.reg.clone();
        for (i, d, from) in mob_hits {
            if let Some(m) = self.mobs.get_mut(i)
                && let Some(def) = reg.animals.get(m.species)
            {
                m.hurt(def, d, from);
            }
        }
        for (pos, it) in drops {
            self.push_drop_at(pos, ItemStack::new(&reg, it, 1));
        }
        dmg
    }

    /// Ire-driven warden spawner: territorial lurkers roll into the dark
    /// ring around the player. Never near the world spawn, never in light.
    /// Grade the vigils: a watcher stands down when its ground is
    /// mended (fading without a corpse), and graduates to the hunt
    /// when the grievance stands too long ignored.
    pub(crate) fn grade_watchers(&mut self) {
        let mut i = 0;
        while i < self.mobs.len() {
            let m = &self.mobs[i];
            if !m.watcher {
                i += 1;
                continue;
            }
            let Some(surface) = m.pos.block().map(BlockPos::surface) else {
                self.mobs.swap_remove(i);
                continue;
            };
            let cell = self.regional_ire_at_surface(surface);
            if cell < m.watch_baseline - 1.5 || self.ire_tier_at_surface(surface) == 0 {
                // The land was answered while it watched.
                self.whispers
                    .push("The watcher melts back into the trees.".to_string());
                self.mobs.swap_remove(i);
                continue;
            }
            if m.watch_timer > 45.0 {
                self.whispers.push("The watching is over.".to_string());
                self.mobs[i].watcher = false;
            }
            i += 1;
        }
    }

    pub fn tick_hostile_spawns(
        &mut self,
        player: EntityPos,
        world_spawn: EntityPos,
        daylight: f32,
        dt: f32,
        rng: &mut u32,
    ) {
        self.hostile_spawn_timer += dt;
        if self.hostile_spawn_timer < 4.0 {
            return;
        }
        self.hostile_spawn_timer = 0.0;
        self.grade_watchers();
        // The wardens are the spirit's immune response. Where the
        // heart is dead they simply stop coming — and the silence is
        // the loudest thing this game ever does, because the player
        // has spent the whole game reading warden pressure as danger.
        let Some(player_surface) = player.block().map(BlockPos::surface) else {
            return;
        };
        if !self.heart_alive_at_surface(player_surface) {
            return;
        }
        let reg = self.reg.clone();
        // The tier as THIS ground feels it: an angry forest hunts
        // harder, a tended valley softer, wherever the world's mood.
        let tier = self.ire_tier_at_surface(player_surface);
        let mut budget = [2usize, 6, 10, 14][tier];
        // While a watcher watches, nothing else comes: the warning IS
        // the encounter until it's answered or it graduates.
        let watcher_near = self
            .mobs
            .iter()
            .any(|m| m.watcher && m.pos.distance_to(player) < 96.0);
        if watcher_near {
            return;
        }
        if self.weather == Weather::Storm && tier >= 2 {
            budget += 1; // dark skies are cover
        }
        let near_hostiles = self
            .mobs
            .iter()
            .filter(|m| {
                reg.animals.get(m.species).is_some_and(|d| d.hostile)
                    && m.pos.distance_to(player) < 96.0
            })
            .count();
        if near_hostiles >= budget || self.mobs.len() >= MOB_CAP {
            return;
        }
        let roll = |rng: &mut u32| {
            *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
            *rng >> 8
        };
        for _ in 0..6 {
            let r = roll(rng);
            let ang = (r % 1024) as f32 / 1024.0 * std::f32::consts::TAU;
            let dist = 24.0 + ((r >> 10) % 32) as f32;
            let Some(surface) = player
                .translated(glam::Vec3::new(ang.sin() * dist, 0.0, ang.cos() * dist))
                .ok()
                .and_then(|moved| moved.pos.block())
                .map(BlockPos::surface)
            else {
                continue;
            };
            if !self.chunks.contains_key(&ChunkPos::from_surface(surface)) {
                continue;
            }
            let spawn_probe = EntityPos::new(
                surface.face(),
                f32::from(surface.u()) + 0.5,
                player.y(),
                f32::from(surface.v()) + 0.5,
            )
            .expect("hostile spawn probe is canonical");
            if spawn_probe.horizontal_distance_to(world_spawn) < 16.0 {
                continue;
            }
            // The wardens a country fields follow its heart too.
            let biome = self.country_biome_at(surface).name().to_lowercase();
            // Split the roster: surface wardens spawn at the surface, the
            // deep's own ("underground" biome tag) in caves below.
            let surface_y = self.surface_height_at(surface);
            let candidates: Vec<(usize, i32)> = reg
                .animals
                .iter()
                .enumerate()
                .filter(|(_, d)| {
                    let local =
                        (self.ire + self.regional_ire_at_surface(surface) * 3.0).clamp(0.0, 100.0);
                    d.hostile && local >= d.ire_min
                })
                .filter_map(|(i, d)| {
                    if d.biomes.iter().any(|b| b == "underground") {
                        // A random depth with a 2-tall air pocket.
                        let y = 6 + (roll(rng) % (surface_y.max(12) as u32 - 6)) as i32;
                        let at = BlockPos::new(surface.face(), surface.u(), y as u8, surface.v())
                            .ok()?;
                        let ground = at
                            .offset(0, -1, 0)
                            .map_or(AIR, |pos| self.get_block_at(pos));
                        let a1 = self.get_block_at(at);
                        let a2 = at.offset(0, 1, 0).map_or(AIR, |pos| self.get_block_at(pos));
                        (self.reg.is_solid(ground) && a1 == AIR && a2 == AIR).then_some((i, y))
                    } else if d.biomes.contains(&biome) && surface_y > SEA_LEVEL {
                        Some((i, surface_y + 1))
                    } else {
                        None
                    }
                })
                .collect();
            if candidates.is_empty() {
                continue;
            }
            let (si, y) = candidates[(roll(rng) as usize) % candidates.len()];
            let def = &reg.animals[si];
            // Only one wrathwood walks at a time.
            if def.name.ends_with("wrathwood")
                && self.mobs.iter().any(|m| {
                    reg.animals
                        .get(m.species)
                        .is_some_and(|d| d.name.ends_with("wrathwood"))
                })
            {
                continue;
            }
            let Some(at) = BlockPos::new(surface.face(), surface.u(), y as u8, surface.v()).ok()
            else {
                continue;
            };
            let (bl, sl) = self.light_at_pos(at);
            let eff = (bl as f32).max(sl as f32 * daylight);
            if eff >= def.spawn_light_max as f32 {
                continue;
            }
            let entity = EntityPos::new(
                surface.face(),
                f32::from(surface.u()) + 0.5,
                y as f32 + 0.05,
                f32::from(surface.v()) + 0.5,
            )
            .expect("hostile spawn is canonical");
            let mut m = Mob::new_at(
                si,
                entity,
                (roll(rng) % 1024) as f32 / 1024.0 * std::f32::consts::TAU,
            );
            m.health = def.health;
            // The first surface warden into aggrieved country arrives
            // as a WATCHER: one warning at the treeline before any
            // hunt. (The deep gives no warnings.)
            let cell_ire = self.regional_ire_at_surface(surface);
            let is_surface = y == surface_y + 1;
            if is_surface && near_hostiles == 0 && cell_ire > 4.0 {
                m.watcher = true;
                m.watch_baseline = cell_ire;
                self.whispers
                    .push("Something watches from the treeline.".to_string());
            }
            self.mobs.push(m);
            return; // one spawn per cycle
        }
    }
}
