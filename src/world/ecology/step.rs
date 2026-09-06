//! Ordered mob/NPC simulation, event application, and repopulation.

use crate::chunk::CHUNK_Y;
use crate::chunk::ChunkPos;
use crate::inventory::ItemStack;
use crate::mobs::Mob;
use crate::mobs::MobEvent;
use crate::planet::BlockPos;
use crate::planet::EntityPos;
use crate::world::MOB_CAP;
use crate::world::World;
use std::collections::HashMap;

impl World {
    /// Tick AI/physics for all mobs, plus the slow repopulation roll.
    /// Returns events (player hits, projectile casts) for the game loop.
    pub fn tick_mobs(
        &mut self,
        players: &[crate::server::PlayerCtx],
        fallback_daylight: f32,
        dt: f32,
        rng: &mut u32,
    ) -> Vec<MobEvent> {
        let fallback_player = players.first().map(|p| p.pos);
        let reg = self.reg.clone();
        let mut events = Vec::new();
        let fallback_season = fallback_player
            .map(|player| self.season_at_surface(player.surface()))
            .unwrap_or_else(|| crate::planet_atlas::local_season(self.calendar_state.day(), 0.0));
        // Stamp stable ids on anything new (spawns, births, loaded saves).
        self.population.stamp_unassigned_mobs();
        // Herd pulls are averaged in each animal's local tangent frame.
        // This costs little at the mob cap and lets a herd straddle a face
        // seam without splitting into two coordinate buckets.
        let herd_members: Vec<(usize, EntityPos)> = self
            .population
            .mobs()
            .iter()
            .filter_map(|m| {
                let d = reg.animals.get(m.species)?;
                (!d.hostile && !d.vehicle && d.group[1] >= 2 && m.growth >= 1.0)
                    .then_some((m.species, m.pos))
            })
            .collect();
        let local_conditions: HashMap<u32, (usize, f32)> = self
            .population
            .mobs()
            .iter()
            .map(|mob| {
                let surface = mob.pos.surface();
                (
                    mob.id,
                    (
                        self.season_at_surface(surface),
                        self.daylight_at_surface(surface),
                    ),
                )
            })
            .collect();
        // The trophic pre-pass: hungry predators pick their quarry,
        // desperation is graded (deep hunger plus night or winter),
        // and prey with a stalker on top of it bolts.
        let snapshot: Vec<(u32, usize, crate::planet::EntityPos)> = self
            .population
            .mobs()
            .iter()
            .map(|m| (m.id, m.species, m.pos))
            .collect();
        let mut spooked: Vec<(u32, crate::planet::EntityPos)> = Vec::new();
        // Capability E12: the global tier once per pass — a region's
        // industrial temper is shared by every animal in it.
        let ire_tier = self.ire_tier();
        for m in self.population.mobs_mut() {
            let Some(d) = reg.animals.get(m.species) else {
                continue;
            };
            let (season, daylight) = local_conditions
                .get(&m.id)
                .copied()
                .unwrap_or((fallback_season, fallback_daylight));
            m.bold = false;
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
            // A player is a last resort: even a starving predator hunts
            // available animal prey first. Industrial pressure changes when
            // desperation is possible, never the hunger or food requirements.
            m.bold = !d.hostile
                && !d.fierce
                && d.attack > 0.0
                && m.quarry.is_none()
                && m.belly < crate::mobs::BELLY_DESPERATE
                && (season == 3 || daylight < 0.35 || ire_tier >= 2);
            if m.state == crate::mobs::MobState::Stalk
                && let Some((id, _, dist)) = best
                && dist < 7.0
            {
                spooked.push((id, m.pos));
            }
            // Capability E15: guards scan for hostile mobs regardless of
            // hunger or prey lists — they defend their post.
            if d.guards && !d.hostile {
                let mut best_hostile: Option<(u32, EntityPos, f32)> = None;
                for &(id, sp, pos) in &snapshot {
                    if id == m.id {
                        continue;
                    }
                    let Some(sd) = reg.animals.get(sp) else {
                        continue;
                    };
                    if !sd.hostile {
                        continue;
                    }
                    let delta = m.pos.local_delta_to(pos);
                    let dist = delta.length();
                    let range = d.aggro_range.max(crate::mobs::HUNT_RANGE);
                    if dist < range && best_hostile.is_none_or(|(_, _, bd)| dist < bd) {
                        best_hostile = Some((id, pos, dist));
                    }
                }
                m.quarry = best_hostile.map(|(id, pos, _)| (id, pos));
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
        let mut mobs = self.population.take_mobs();
        // NPC walkers drive their companion mobs before the ordinary AI
        // pass: fixed NPCs idle, patrol NPCs walk their loop (spec 3.1).
        let mut npcs = self.population.take_npcs();
        for npc in &mut npcs {
            let Some(m) = mobs.iter_mut().find(|m| m.id == npc.mob_id) else {
                continue;
            };
            if let Some(cp) = m.pos.chunk()
                && self.chunks.contains_key(&cp)
            {
                npc.tick(m, dt);
            }
        }
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
                let before = m.pos;
                m.tick(self, def, players, dt, rng, &mut events);
                if def.name.contains(":warden")
                    && let Some(cell) = m.pos.block()
                    && self.resist_supernatural_pressure_at(
                        cell,
                        "warden",
                        def.attack.max(1.0).ceil() as u64,
                    )
                {
                    // A supplied ward resists rather than destroys the Wild's
                    // creature. Overload returns false and lets the crossing
                    // stand; successful resistance restores the exact ordinary
                    // pre-step position and leaves collision/AI authoritative.
                    m.pos = before;
                    m.vel = glam::Vec3::ZERO;
                    m.state_timer = m.state_timer.max(0.2);
                }
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
        let mut retired_cargo: Vec<(BlockPos, ItemStack)> = Vec::new();
        mobs.retain_mut(|m| {
            if reg
                .animals
                .get(m.species)
                .is_some_and(|d| d.name.ends_with(":carcass"))
            {
                m.rot -= dt;
                if m.rot <= 0.0 {
                    rotted.push(m.pos);
                    if let Some(cargo) = m.cargo.take() {
                        let at = m.pos.block().unwrap_or_else(|| {
                            let surface = m.pos.surface();
                            BlockPos::new(surface.face(), surface.u(), 1, surface.v())
                                .expect("canonical mob surface has a shell floor")
                        });
                        retired_cargo.extend(cargo.into_iter().flatten().map(|stack| (at, stack)));
                    }
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
        let mut retired_current = Vec::new();
        mobs.retain_mut(|m| {
            let Some(def) = reg.animals.get(m.species) else {
                if let Some(cargo) = m.cargo.take() {
                    let at = m.pos.block().unwrap_or_else(|| {
                        let surface = m.pos.surface();
                        BlockPos::new(surface.face(), surface.u(), 1, surface.v())
                            .expect("canonical mob surface has a shell floor")
                    });
                    retired_cargo.extend(cargo.into_iter().flatten().map(|stack| (at, stack)));
                }
                return false;
            };
            let keep = if m.pos.y() < -20.0 {
                false // fell out of the world somehow
            } else if def.hostile && m.masterless {
                // Left over when the heart died and never recalled:
                // no daylight dissolves them, nothing sends them, and
                // they do not stop. Only distance retires them.
                let near = players
                    .iter()
                    .map(|p| m.pos.local_delta_to(p.pos).length_squared())
                    .fold(f32::INFINITY, f32::min);
                near <= 120.0 * 120.0
            } else if !def.hostile {
                // Fish are ambience-plus-resource: the water has
                // fish while someone's there to see it.
                if def.movement_swim {
                    let near = players
                        .iter()
                        .map(|p| m.pos.local_delta_to(p.pos).length_squared())
                        .fold(f32::INFINITY, f32::min);
                    near <= 96.0 * 96.0
                } else {
                    true
                }
            } else {
                let near = players
                    .iter()
                    .map(|p| m.pos.local_delta_to(p.pos).length_squared())
                    .fold(f32::INFINITY, f32::min);
                if near > 80.0 * 80.0 {
                    false
                } else if let Some(light_pos) = m
                    .pos
                    .translated(glam::Vec3::new(0.0, 0.5, 0.0))
                    .ok()
                    .and_then(|p| p.pos.block())
                {
                    let (_, sl) = self.light_at_pos(light_pos);
                    let local_daylight = local_conditions
                        .get(&m.id)
                        .map_or(fallback_daylight, |(_, daylight)| *daylight);
                    sl as f32 * local_daylight < 7.0
                } else {
                    false
                }
            };
            if !keep {
                if def.hostile
                    && let Some(arcane) = def.arcane.as_ref()
                {
                    retired_current.push((m.id, m.pos, arcane.on_destroy));
                }
                if let Some(cargo) = m.cargo.take() {
                    let at = m.pos.block().unwrap_or_else(|| {
                        let surface = m.pos.surface();
                        BlockPos::new(surface.face(), surface.u(), 1, surface.v())
                            .expect("canonical mob surface has a shell floor")
                    });
                    retired_cargo.extend(cargo.into_iter().flatten().map(|stack| (at, stack)));
                }
            }
            keep
        });
        for (id, pos, disposition) in retired_current {
            self.retire_warden_current(id, pos, disposition, "warden dissolved");
        }
        for (at, stack) in retired_cargo {
            self.push_drop_at(at, stack);
        }
        // Husbandry: two fed adults of a species near each other bear
        // young - but not in winter; spring is the birthing season.
        let mut births: Vec<(usize, usize)> = Vec::new();
        for i in 0..mobs.len() {
            let winter = local_conditions
                .get(&mobs[i].id)
                .is_some_and(|(season, _)| *season == 3);
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
        self.population.replace_mobs(mobs);
        self.population.restore_npcs(npcs);

        // Repopulation: overhunted wildlife slowly recovers, away from the
        // player and only under the local cap.
        // Spring teems, winter starves: the repop clock runs at double
        // or half speed with the season.
        let repop_season = fallback_player
            .map(|player| self.season_at_surface(player.surface()))
            .unwrap_or(fallback_season);
        if self.population.repopulation_cycle(dt, repop_season) {
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
            // The Deep stocks no wildlife (capability E10): a restock ring
            // around a player below would fill dungeons with deer.
            if player.face().is_deep() {
                return events;
            }
            let near = self
                .population
                .mobs()
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
                    let biome = self.animal_biome_name(surface, wet);
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
                            !d.hostile
                                && placeable
                                && self.animal_habitat_suitable(d, surface, wet, &biome)
                                && self.animal_habitat_network_connected(d, surface)
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
}
