//! Wildlife seeding, mob/projectile ticking, and hostile spawning.

use super::*;

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

    pub(super) fn mob_hash(&self, x: i32, z: i32, salt: u32) -> u32 {
        let mut h = (x as u32).wrapping_mul(0x85eb_ca6b)
            ^ (z as u32).wrapping_mul(0xc2b2_ae35)
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
        let reg = self.reg.clone();
        let (cx, cz) = (pos.x * CHUNK_X as i32, pos.z * CHUNK_Z as i32);
        let biome = self.generator.biome(cx + 8, cz + 8).name().to_lowercase();
        for (si, def) in reg.animals.iter().enumerate() {
            // Wildlife only — wardens come and go with the spawner.
            if def.hostile || !def.biomes.contains(&biome) {
                continue;
            }
            let roll = self.mob_hash(pos.x, pos.z, 7000 + si as u32);
            if !roll.is_multiple_of(def.rarity) {
                continue;
            }
            let span = def.group[1].saturating_sub(def.group[0]) + 1;
            let n = def.group[0] + (roll >> 8) % span;
            for i in 0..n {
                let h = self.mob_hash(pos.x, pos.z, 7100 + si as u32 * 31 + i);
                let lx = (h % CHUNK_X as u32) as i32;
                let lz = ((h >> 8) % CHUNK_Z as u32) as i32;
                self.try_spawn(si, cx + lx, cz + lz, (h >> 16) as f32 / 65535.0);
            }
            break; // one species per chunk keeps groups readable
        }
    }

    /// Spawn on dry solid ground at the surface; silently skips bad spots.
    pub(super) fn try_spawn(&mut self, species: usize, x: i32, z: i32, yaw01: f32) -> bool {
        if self.mobs.len() >= MOB_CAP {
            return false;
        }
        let y = self.surface_height(x, z);
        if y <= SEA_LEVEL {
            return false;
        }
        let ground = self.get_block(x, y, z);
        if !self.reg.is_solid(ground) {
            return false;
        }
        let mut m = Mob::new(
            species,
            glam::Vec3::new(x as f32 + 0.5, y as f32 + 1.05, z as f32 + 0.5),
            yaw01 * std::f32::consts::TAU,
        );
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
        let player = players.first().map(|p| p.pos).unwrap_or(glam::Vec3::ZERO);
        let reg = self.reg.clone();
        let mut events = Vec::new();
        // Stamp stable ids on anything new (spawns, births, loaded saves).
        for m in &mut self.mobs {
            if m.id == 0 {
                m.id = self.next_mob_id;
                self.next_mob_id += 1;
            }
        }
        let mut mobs = std::mem::take(&mut self.mobs);
        for m in &mut mobs {
            // Frozen until its chunk streams in: an unloaded chunk reads as
            // air, and ticking against it drops the mob through the world.
            let cp = ChunkPos::of_world(m.pos.x.floor() as i32, m.pos.z.floor() as i32);
            if !self.chunks.contains_key(&cp) {
                continue;
            }
            if let Some(def) = reg.animals.get(m.species) {
                m.unstick(self, def);
                m.tick(self, def, players, dt, rng, &mut events);
            }
        }
        // Wardens are expressions of the wild, not creatures: they dissolve
        // in daylight (sky-lit cells only — torchlight never banishes them)
        // and when the player leaves them far behind.
        mobs.retain(|m| {
            let Some(def) = reg.animals.get(m.species) else {
                return false;
            };
            if m.pos.y < -20.0 {
                return false; // fell out of the world somehow
            }
            if !def.hostile {
                return true;
            }
            let near = players
                .iter()
                .map(|p| (m.pos - p.pos).length_squared())
                .fold(f32::INFINITY, f32::min);
            if near > 80.0 * 80.0 {
                return false;
            }
            let (_, sl) = self.light_at(
                m.pos.x.floor() as i32,
                (m.pos.y + 0.5).floor() as i32,
                m.pos.z.floor() as i32,
            );
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
                    && (mobs[i].pos - mobs[j].pos).length_squared() < 16.0
                {
                    births.push((i, j));
                    break;
                }
            }
        }
        for (i, j) in births {
            let mid = (mobs[i].pos + mobs[j].pos) * 0.5;
            mobs[i].fed = false;
            mobs[j].fed = false;
            mobs[i].breed_cd = 300.0;
            mobs[j].breed_cd = 300.0;
            if mobs.len() < MOB_CAP {
                let mut baby = Mob::new(mobs[i].species, mid, 0.0);
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
        if self.repop_timer >= 8.0 {
            self.repop_timer = 0.0;
            let near = self
                .mobs
                .iter()
                .filter(|m| (m.pos - player).length_squared() < 96.0 * 96.0)
                .count();
            if near < 40 && !reg.animals.is_empty() {
                *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                let r = *rng;
                // A ring 32-72 blocks out at a random angle.
                let ang = (r % 1024) as f32 / 1024.0 * std::f32::consts::TAU;
                let dist = 32.0 + ((r >> 10) % 40) as f32;
                let x = (player.x + ang.sin() * dist).floor() as i32;
                let z = (player.z + ang.cos() * dist).floor() as i32;
                let cp = ChunkPos::of_world(x, z);
                if self.chunks.contains_key(&cp) {
                    let biome = self.generator.biome(x, z).name().to_lowercase();
                    // Wildlife only — wardens have their own spawner.
                    let eligible: Vec<usize> = reg
                        .animals
                        .iter()
                        .enumerate()
                        .filter(|(_, d)| !d.hostile && d.biomes.contains(&biome))
                        .map(|(i, _)| i)
                        .collect();
                    if let Some(&si) = eligible.get(((r >> 20) as usize) % eligible.len().max(1)) {
                        self.try_spawn(si, x, z, (r >> 8) as f32 / (u32::MAX >> 8) as f32);
                    }
                }
            }
        }
        events
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
        let mut mob_hits: Vec<(usize, f32, glam::Vec3)> = Vec::new();
        let mut drops: Vec<((i32, i32, i32), crate::registry::ItemId)> = Vec::new();
        let mut projectiles = std::mem::take(&mut self.projectiles);
        projectiles.retain_mut(|p| match p.tick(self, players, dt) {
            ProjHit::None => true,
            ProjHit::Expired => false,
            ProjHit::Player(i) => {
                dmg.push((i, p.damage));
                false
            }
            ProjHit::Mob(i) => {
                mob_hits.push((i, p.damage, p.pos - p.vel * dt));
                false
            }
            ProjHit::Block => {
                if let Some(it) = p.drop_item {
                    if p.owner != 0 {
                        // A guest's arrow: hand it back over the wire.
                        let stack = ItemStack::new(&self.reg, it, 1);
                        self.pending_gives.push((p.owner, stack));
                    } else {
                        let back = p.pos - p.vel * dt * 2.0;
                        drops.push((
                            (
                                back.x.floor() as i32,
                                back.y.floor() as i32,
                                back.z.floor() as i32,
                            ),
                            it,
                        ));
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
            self.pending_drops.push((pos, ItemStack::new(&reg, it, 1)));
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
            let (mx, mz) = (m.pos.x.floor() as i32, m.pos.z.floor() as i32);
            let cell = self.regional_ire_at(mx, mz);
            if cell < m.watch_baseline - 1.5 || self.ire_tier_at(mx, mz) == 0 {
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
        player: glam::Vec3,
        world_spawn: glam::Vec3,
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
        let reg = self.reg.clone();
        // The tier as THIS ground feels it: an angry forest hunts
        // harder, a tended valley softer, wherever the world's mood.
        let (px, pz) = (player.x.floor() as i32, player.z.floor() as i32);
        let tier = self.ire_tier_at(px, pz);
        let mut budget = [2usize, 6, 10, 14][tier];
        // While a watcher watches, nothing else comes: the warning IS
        // the encounter until it's answered or it graduates.
        let watcher_near = self
            .mobs
            .iter()
            .any(|m| m.watcher && (m.pos - player).length_squared() < 96.0 * 96.0);
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
                    && (m.pos - player).length_squared() < 96.0 * 96.0
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
            let x = (player.x + ang.sin() * dist).floor() as i32;
            let z = (player.z + ang.cos() * dist).floor() as i32;
            if !self.chunks.contains_key(&ChunkPos::of_world(x, z)) {
                continue;
            }
            let dxs = x as f32 - world_spawn.x;
            let dzs = z as f32 - world_spawn.z;
            if dxs * dxs + dzs * dzs < 16.0 * 16.0 {
                continue;
            }
            let biome = self.generator.biome(x, z).name().to_lowercase();
            // Split the roster: surface wardens spawn at the surface, the
            // deep's own ("underground" biome tag) in caves below.
            let surface_y = self.surface_height(x, z);
            let candidates: Vec<(usize, i32)> = reg
                .animals
                .iter()
                .enumerate()
                .filter(|(_, d)| {
                    let local = (self.ire + self.regional_ire_at(x, z) * 3.0).clamp(0.0, 100.0);
                    d.hostile && local >= d.ire_min
                })
                .filter_map(|(i, d)| {
                    if d.biomes.iter().any(|b| b == "underground") {
                        // A random depth with a 2-tall air pocket.
                        let y = 6 + (roll(rng) % (surface_y.max(12) as u32 - 6)) as i32;
                        let ground = self.get_block(x, y - 1, z);
                        let a1 = self.get_block(x, y, z);
                        let a2 = self.get_block(x, y + 1, z);
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
            let (bl, sl) = self.light_at(x, y, z);
            let eff = (bl as f32).max(sl as f32 * daylight);
            if eff >= def.spawn_light_max as f32 {
                continue;
            }
            let mut m = Mob::new(
                si,
                glam::Vec3::new(x as f32 + 0.5, y as f32 + 0.05, z as f32 + 0.5),
                (roll(rng) % 1024) as f32 / 1024.0 * std::f32::consts::TAU,
            );
            m.health = def.health;
            // The first surface warden into aggrieved country arrives
            // as a WATCHER: one warning at the treeline before any
            // hunt. (The deep gives no warnings.)
            let cell_ire = self.regional_ire_at(x, z);
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
