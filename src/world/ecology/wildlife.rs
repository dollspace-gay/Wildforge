//! Deterministic wildlife admission and detailed placement.

use crate::chunk::CHUNK_X;
use crate::chunk::CHUNK_Z;
use crate::chunk::ChunkPos;
use crate::chunk::SEA_LEVEL;
use crate::mobs::Mob;
use crate::planet::BlockPos;
use crate::planet::EntityPos;
use crate::planet::SurfacePos;
use crate::registry::AIR;
use crate::world::MOB_CAP;
use crate::world::World;

impl World {
    /// Deterministic per-chunk wildlife roll: at most one species' group.
    pub(in crate::world) fn seed_wildlife(&mut self, pos: ChunkPos) {
        if self.population.mobs().len() >= MOB_CAP {
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
        // Salt sea keeps its own roster; fresh water inherits the climate
        // and habitat around its watershed.
        let here = self.animal_biome_name(center, true);
        for (si, def) in reg.animals.iter().enumerate() {
            // Wildlife only — wardens come and go with the spawner.
            // Swimmers roll in the water pass below; letting them share
            // this slot meant a fish only ever spawned in a chunk where
            // every land animal of the biome had already failed its
            // rarity roll, which is why the sea looked empty.
            if def.hostile
                || def.movement_swim
                || !self.animal_habitat_suitable(def, center, false, &here)
            {
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
                (!def.hostile
                    && def.movement_swim
                    && self.animal_habitat_suitable(def, center, true, &here))
                .then_some(index)
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
                    && self.population.mobs().len() < MOB_CAP
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
                    self.population.push_mob(m);
                }
            }
        }
    }

    /// Spawn on dry solid ground at the surface — or, for swimmers,
    /// submerged in a water column at least two deep. Skips bad spots.
    pub(in crate::world) fn try_spawn_at(
        &mut self,
        species: usize,
        surface: SurfacePos,
        yaw01: f32,
    ) -> bool {
        if self.population.mobs().len() >= MOB_CAP {
            return false;
        }
        let Some(definition) = self.reg.animals.get(species) else {
            return false;
        };
        let swim = definition.movement_swim;
        if swim {
            let Some(habitat) = self.aquatic_habitat_at(surface) else {
                return false;
            };
            let preference = definition.aquatic.unwrap_or_default();
            if !(preference.depth_blocks[0]..=preference.depth_blocks[1])
                .contains(&habitat.depth_blocks)
                || (self.planet_atlas.is_some()
                    && (!(preference.temperature_c[0]..=preference.temperature_c[1])
                        .contains(&habitat.temperature_c)
                        || !(preference.discharge[0]..=preference.discharge[1])
                            .contains(&habitat.discharge)
                        || !(preference.salinity[0]..=preference.salinity[1])
                            .contains(&habitat.salinity)))
            {
                return false;
            }
        }
        // Category budget: a full lake never starves the land spawns.
        if swim {
            let reg = self.reg.clone();
            let fish = self
                .population
                .mobs()
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
        self.population.push_mob(m);
        true
    }
}
