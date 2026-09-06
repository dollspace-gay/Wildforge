//! Discovery sites for the common spawn contract.

use super::entry_chunks;
use crate::chunk::CHUNK_X;
use crate::chunk::CHUNK_Y;
use crate::chunk::CHUNK_Z;
use crate::chunk::ChunkPos;
use crate::chunk::SEA_LEVEL;
use crate::planet::BlockPos;
use crate::planet::SurfacePos;
use crate::registry::AIR;
use crate::world::World;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;

impl World {
    /// Census-backed discovery sites. New planets receive three independent
    /// observational ruins in the prepared homeland. Existing planets use an
    /// untouched prepared chunk when possible; if every candidate was already
    /// worked, a single surface remnant is added without replacing authored
    /// voxels. The persisted site keys make this retrogen idempotent.
    pub(super) fn ensure_spawn_discovery_sites(
        &mut self,
        spawn: SurfacePos,
    ) -> std::io::Result<Vec<ChunkPos>> {
        let structure = self
            .reg
            .structures
            .iter()
            .position(|structure| structure.name == "base:observational_outpost")
            .ok_or_else(|| {
                std::io::Error::other("base observational outpost content is missing")
            })?;
        let prepared_chunks = entry_chunks(spawn);
        let prepared = prepared_chunks.iter().copied().collect::<HashSet<_>>();
        // Site placement is a consequence of the already-qualified homeland,
        // not another reason to reject it. Build the dry component a player
        // can actually walk from the accepted doorstep, then choose origins
        // nearest the three desired bearings. Fixed offsets failed valid
        // coastal homelands whenever one bearing happened to land in water.
        let mut dry_columns = HashMap::<SurfacePos, i32>::new();
        for position in &prepared_chunks {
            let origin = position.block_origin();
            for x in 0..CHUNK_X {
                for z in 0..CHUNK_Z {
                    let surface = SurfacePos::new(
                        origin.face(),
                        origin.u() + x as u16,
                        origin.v() + z as u16,
                    )
                    .expect("prepared chunks contain canonical surface cells");
                    let y = self.surface_height_at(surface);
                    let clear = |y: i32| {
                        BlockPos::new(surface.face(), surface.u(), y as u8, surface.v()).is_ok_and(
                            |pos| {
                                let block = self.get_block_at(pos);
                                !self.reg.is_solid(block) && !self.reg.is_fluid(block)
                            },
                        )
                    };
                    if y > SEA_LEVEL + 1 && y < CHUNK_Y as i32 - 8 && clear(y + 1) && clear(y + 2) {
                        dry_columns.insert(surface, y);
                    }
                }
            }
        }
        let mut reachable = HashSet::new();
        let mut queue = VecDeque::new();
        if dry_columns.contains_key(&spawn) {
            reachable.insert(spawn);
            queue.push_back(spawn);
        }
        while let Some(surface) = queue.pop_front() {
            let height = dry_columns[&surface];
            for neighbor in crate::planet::neighbors4(surface) {
                if reachable.contains(&neighbor)
                    || !prepared.contains(&ChunkPos::from_surface(neighbor))
                {
                    continue;
                }
                let Some(next_height) = dry_columns.get(&neighbor) else {
                    continue;
                };
                if (height - next_height).abs() <= 1 {
                    reachable.insert(neighbor);
                    queue.push_back(neighbor);
                }
            }
        }
        if reachable.is_empty() {
            return Err(std::io::Error::other(
                "prepared homeland discovery census cannot reach its accepted doorstep",
            ));
        }
        let desired = [(24, 0), (-24, 0), (0, 24)];
        let mut changed = Vec::new();
        let mut chosen = Vec::<SurfacePos>::new();
        for (index, (base_u, base_v)) in desired.into_iter().enumerate() {
            let key = format!("spawn-observational-site-v1-{index}");
            if self.discovery_site_installed(&key) {
                continue;
            }
            let target = SurfacePos::canonicalized(
                spawn.face(),
                i32::from(spawn.u()) + base_u,
                i32::from(spawn.v()) + base_v,
            )
            .map_err(|error| std::io::Error::other(error.to_string()))?;
            let mut dry = reachable
                .iter()
                .copied()
                .filter(|surface| {
                    chosen.iter().all(|prior| {
                        crate::planet::geodesic_distance(prior.center(), surface.center()) >= 8.0
                    })
                })
                .map(|surface| (surface, dry_columns[&surface]))
                .collect::<Vec<_>>();
            dry.sort_by(|(left, _), (right, _)| {
                self.player_touched
                    .contains(&ChunkPos::from_surface(*left))
                    .cmp(
                        &self
                            .player_touched
                            .contains(&ChunkPos::from_surface(*right)),
                    )
                    .then_with(|| {
                        crate::planet::geodesic_distance(target.center(), left.center()).total_cmp(
                            &crate::planet::geodesic_distance(target.center(), right.center()),
                        )
                    })
                    .then_with(|| left.cmp(right))
            });
            let Some((origin_surface, surface_y)) = dry.first().copied() else {
                return Err(std::io::Error::other(format!(
                    "prepared homeland has fewer than three distinct reachable observational sites (stopped at {index})"
                )));
            };
            chosen.push(origin_surface);
            let origin = BlockPos::new(
                origin_surface.face(),
                origin_surface.u(),
                surface_y as u8,
                origin_surface.v(),
            )
            .map_err(|error| std::io::Error::other(error.to_string()))?;
            if !self.player_touched.contains(&origin.chunk()) {
                let seed = self.seed ^ 0xd15c_0000 ^ index as u32;
                self.place_structure_at(structure, origin, seed);
            } else {
                // Explicit remnant fallback for an evolved homeland: add one
                // brushable clue in the air above existing terrain. Never
                // rewrite a player-authored cell to fake an untouched ruin.
                let remnant = origin
                    .offset(0, 1, 0)
                    .ok_or_else(|| std::io::Error::other("discovery remnant exceeds world"))?;
                if self.get_block_at(remnant) != AIR {
                    return Err(std::io::Error::other(format!(
                        "worked homeland has no non-destructive remnant cell for site {index}"
                    )));
                }
                let cracked = self
                    .reg
                    .block_id("base:cracked_masonry")
                    .ok_or_else(|| std::io::Error::other("cracked masonry content is missing"))?;
                self.set_block_authored_at(
                    remnant,
                    cracked,
                    "discovery remnant retrogen into worked homeland",
                );
            }
            self.mark_discovery_site_installed(&key);
            changed.push(origin.chunk());
        }
        Ok(changed)
    }
}
