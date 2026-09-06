//! Watcher grading, hostile budgets, and nest-spawn coordination.

use crate::chunk::ChunkPos;
use crate::chunk::SEA_LEVEL;
use crate::mobs::Mob;
use crate::planet::BlockPos;
use crate::planet::EntityPos;
use crate::registry::AIR;
use crate::world::MOB_CAP;
use crate::world::World;

impl World {
    /// Ire-driven warden spawner: territorial lurkers roll into the dark
    /// ring around the player. Never near the world spawn, never in light.
    /// Grade the vigils: a watcher stands down when its ground is
    /// mended (fading without a corpse), and graduates to the hunt
    /// when the grievance stands too long ignored.
    pub(crate) fn grade_watchers(&mut self) {
        let mut i = 0;
        while i < self.population.mobs().len() {
            let m = &self.population.mobs()[i];
            if !m.watcher {
                i += 1;
                continue;
            }
            let Some(surface) = m.pos.block().map(BlockPos::surface) else {
                let retired = self.population.remove_mob(i);
                let disposition = self
                    .reg
                    .animals
                    .get(retired.species)
                    .and_then(|definition| definition.arcane.as_ref())
                    .map(|arcane| arcane.on_destroy);
                if let Some(disposition) = disposition {
                    self.retire_warden_current(
                        retired.id,
                        retired.pos,
                        disposition,
                        "watcher left valid terrain",
                    );
                }
                continue;
            };
            let cell = self.regional_ire_at_surface(surface);
            if cell < m.watch_baseline - 1.5 || self.ire_tier_at_surface(surface) == 0 {
                // The land was answered while it watched.
                self.whispers
                    .push("The watcher melts back into the trees.".to_string());
                let retired = self.population.remove_mob(i);
                let disposition = self
                    .reg
                    .animals
                    .get(retired.species)
                    .and_then(|definition| definition.arcane.as_ref())
                    .map(|arcane| arcane.on_destroy);
                if let Some(disposition) = disposition {
                    self.retire_warden_current(
                        retired.id,
                        retired.pos,
                        disposition,
                        "watcher stood down",
                    );
                }
                continue;
            }
            if m.watch_timer > 45.0 {
                self.whispers.push("The watching is over.".to_string());
                self.population.mobs_mut()[i].watcher = false;
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
        if !self.ruleset().hostile_spawns {
            return;
        }
        if !self.population.hostile_cycle(dt) {
            return;
        }
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
            .population
            .mobs()
            .iter()
            .any(|m| m.watcher && m.pos.distance_to(player) < 96.0);
        if watcher_near {
            return;
        }
        if self.ruleset().weather_extremes
            && self.weather_at_surface(player_surface).kind
                == crate::planet_atlas::LocalWeather::Storm
            && tier >= 2
        {
            budget += 1; // dark skies are cover
        }
        let near_hostiles = self
            .population
            .mobs()
            .iter()
            .filter(|m| {
                reg.animals.get(m.species).is_some_and(|d| d.hostile)
                    && m.pos.distance_to(player) < 96.0
            })
            .count();
        if near_hostiles >= budget || self.population.mobs().len() >= MOB_CAP {
            return;
        }
        let roll = |rng: &mut u32| {
            *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
            *rng >> 8
        };
        // Nest-bound species never ride the ring: their dens are their only
        // source (that is what makes clearing a den stop the respawns).
        let nest_species: std::collections::HashSet<usize> =
            reg.nests.iter().map(|nest| nest.species).collect();
        let _ = &nest_species;
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
                .filter(|(i, d)| {
                    let local =
                        (self.ire + self.regional_ire_at_surface(surface) * 3.0).clamp(0.0, 100.0);
                    // A nest-bound species spawns only from its nests, never
                    // from the ire ring.
                    d.hostile && local >= d.ire_min && !nest_species.contains(i)
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
                    } else if self.animal_habitat_suitable(d, surface, false, &biome)
                        && surface_y > SEA_LEVEL
                    {
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
                && self.population.mobs().iter().any(|m| {
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
            self.spawn_mob(m);
            return; // one spawn per cycle
        }
    }

    /// Nest spawns (capability E9, split from the hostile ring during the
    /// belt-quest content pass): species bound to a live nest spawn near
    /// their den — clearing the nest block stops those respawns entirely.
    /// Deliberately INDEPENDENT of `hostile_spawns` and of any country
    /// heart: a mod mode can silence the ring yet keep its dens alive, and
    /// the Deep has no hearts but its dungeons still need populations.
    pub fn tick_nest_spawns(&mut self, player: EntityPos, daylight: f32, dt: f32, rng: &mut u32) {
        if !self.ruleset().nest_spawns {
            return;
        }
        if !self.population.nest_cycle(dt) {
            return;
        }
        if self.population.mobs().len() >= MOB_CAP {
            return;
        }
        let reg = self.reg.clone();
        if reg.nests.is_empty() || self.nests.is_empty() {
            return;
        }
        let roll = |rng: &mut u32| {
            *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
            *rng >> 8
        };
        let nest_positions: Vec<(BlockPos, usize)> = self
            .nests
            .iter()
            .filter(|(pos, _)| pos.entity_center().distance_to(player) < 96.0)
            .map(|(pos, index)| (*pos, *index))
            .collect();
        for (pos, nest_index) in nest_positions {
            let Some(nest) = reg.nest(nest_index) else {
                continue;
            };
            // A stale record (marker block broken while its chunk was
            // unloaded) self-heals once the chunk loads: no block, no nest.
            if self.get_block_at(pos) != nest.block {
                self.nests.remove(&pos);
                self.nest_spawn_cd.remove(&pos);
                continue;
            }
            let Some(def) = reg.animals.get(nest.species) else {
                continue;
            };
            let local =
                (self.ire + self.regional_ire_at_surface(pos.surface()) * 3.0).clamp(0.0, 100.0);
            if !def.hostile || local < def.ire_min {
                continue;
            }
            let mut cd = *self.nest_spawn_cd.entry(pos).or_insert(0.0);
            cd -= dt;
            if cd > 0.0 {
                continue;
            }
            let living = self
                .population
                .mobs()
                .iter()
                .filter(|m| {
                    m.species == nest.species
                        && m.pos.horizontal_distance_to(pos.entity_center()) <= nest.radius
                })
                .count();
            if living >= nest.cap as usize {
                self.nest_spawn_cd.insert(pos, nest.interval);
                continue;
            }
            // The spawn cell is the nest's own surface, light-gated like any
            // warden manifestation.
            let surface = pos.surface();
            if !self.chunks.contains_key(&ChunkPos::from_surface(surface)) {
                continue;
            }
            let surface_y = self.surface_height_at(surface);
            let at = BlockPos::new(
                surface.face(),
                surface.u(),
                (surface_y + 1) as u8,
                surface.v(),
            );
            let spawned = if let Ok(at) = at {
                let (bl, sl) = self.light_at_pos(at);
                let eff = (bl as f32).max(sl as f32 * daylight);
                if eff < def.spawn_light_max as f32 {
                    let entity = EntityPos::new(
                        surface.face(),
                        f32::from(surface.u()) + 0.5,
                        surface_y as f32 + 1.0,
                        f32::from(surface.v()) + 0.5,
                    )
                    .expect("nest spawn is canonical");
                    let mut m = crate::mobs::Mob::new_at(
                        nest.species,
                        entity,
                        (roll(rng) % 1024) as f32 / 1024.0 * std::f32::consts::TAU,
                    );
                    m.health = def.health;
                    self.spawn_mob(m);
                    true
                } else {
                    false
                }
            } else {
                false
            };
            // A successful nest spawn is one spawn this cycle; a light- or
            // terrain-blocked nest retries soon.
            self.nest_spawn_cd
                .insert(pos, if spawned { nest.interval } else { 2.0 });
            if spawned {
                return;
            }
        }
    }
}
