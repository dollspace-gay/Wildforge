//! Guest terrain delivery and perception-bounded world snapshots.

use std::collections::HashSet;

use super::HostSession;
use crate::chunk::ChunkPos;
use crate::net::{self, MobSnap, S2C, batch_snapshot};
use crate::planet::EntityPos;
use crate::server::Server;

/// Chunks pushed per guest per pump. The ring is paced so a guest asking for
/// a wide view does not make the host generate hundreds of chunks in one
/// frame; `RequestChunk` fills anything the pacing misses.
const CHUNKS_PER_PUMP: usize = 2;

/// How far, in blocks, a guest is told about mobs, bolts and tumbling sand.
/// A little inside its terrain horizon: things it cannot see are things it
/// does not need, and this keeps snapshots bounded by perception.
fn snapshot_reach(view_dist: i32) -> f32 {
    (view_dist.max(1) * crate::chunk::CHUNK_X as i32) as f32
}

impl HostSession {
    /// Stream the nearest missing chunks for each guest, paced per pump.
    pub(super) fn stream_chunks(&mut self, server: &mut Server) {
        self.ensure_chunk_jobs(server);
        if self.refuse_failed_workers() {
            return;
        }
        let entry_required: HashSet<ChunkPos> = self
            .pending_guests
            .values()
            .flat_map(|pending| pending.required.iter().copied())
            .chain(
                self.guests
                    .values()
                    .filter(|guest| !guest.entry_ready)
                    .flat_map(|guest| guest.entry_required.iter().copied()),
            )
            .collect();
        let active_regions: Vec<(ChunkPos, i32)> = self
            .guests
            .values()
            .filter(|guest| guest.entry_ready)
            .filter_map(|guest| guest.pos.chunk().map(|center| (center, guest.view_dist)))
            .collect();
        let wanted = |position: ChunkPos| {
            entry_required.contains(&position)
                || active_regions.iter().any(|(center, radius)| {
                    position.distance(*center) <= f64::from(*radius * 16) + 1.0
                })
        };
        if let Some(jobs) = self.chunk_jobs.as_mut() {
            jobs.cancel_queued(&wanted);
            for pending in self.pending_guests.values() {
                for &pos in &pending.required {
                    if !server.world.has_chunk(pos) {
                        jobs.enqueue(pos, true);
                    }
                }
            }
        }
        if let Some(jobs) = self.chunk_jobs.as_mut() {
            jobs.drain_into(server, &wanted);
        }
        self.refuse_failed_terrain();
        let prepared: Vec<u32> = self
            .pending_guests
            .iter()
            .filter(|(_, pending)| {
                pending
                    .required
                    .iter()
                    .all(|position| server.world.has_chunk(*position))
            })
            .map(|(id, _)| *id)
            .collect();
        for id in prepared {
            self.try_finish_pending_entry(server, id);
        }
        let guest_ids: Vec<u32> = self.guests.keys().copied().collect();
        for id in guest_ids {
            let (gpos, vd, entry_ready, entry_required) = {
                let g = &self.guests[&id];
                (g.pos, g.view_dist, g.entry_ready, g.entry_required.clone())
            };
            if !entry_ready {
                let needed: Vec<_> = entry_required
                    .into_iter()
                    .filter(|pos| !self.guests[&id].sent_chunks.contains(pos))
                    .take(CHUNKS_PER_PUMP)
                    .collect();
                for pos in needed {
                    self.stream_chunk(server, id, pos);
                }
                continue;
            }
            let Some(center) = gpos.chunk() else {
                continue;
            };
            // A guest that walked away has dropped these; forget that we sent
            // them so walking back re-streams instead of leaving a hole.
            if let Some(g) = self.guests.get_mut(&id) {
                let keep = vd + 2;
                g.sent_chunks
                    .retain(|pos| pos.distance(center) <= f64::from(keep * 16));
            }
            let mut needed = Vec::new();
            'scan: for r in 0..=vd {
                for dx in -r..=r {
                    for dz in -r..=r {
                        if dx.abs().max(dz.abs()) != r {
                            continue;
                        }
                        let cp = center.offset(dx, dz);
                        if cp.distance(center) > f64::from(vd * 16) + 1.0 {
                            continue;
                        }
                        if !self.guests[&id].sent_chunks.contains(&cp) {
                            needed.push(cp);
                            if needed.len() >= CHUNKS_PER_PUMP {
                                break 'scan;
                            }
                        }
                    }
                }
            }
            for pos in needed {
                self.stream_chunk(server, id, pos);
            }
        }
    }

    /// Send players and authoritative world actors at 20 Hz, culled to each
    /// guest's negotiated perception horizon.
    pub(super) fn stream_snapshots(
        &mut self,
        server: &Server,
        host: Option<(
            EntityPos,
            f32,
            u16,
            u32,
            Option<crate::implements::ImplementVisual>,
        )>,
        dt: f32,
    ) {
        self.snapshot_timer += dt;
        if self.snapshot_timer < 0.05 {
            return;
        }
        self.snapshot_timer = 0.0;
        self.snapshot_seq = self.snapshot_seq.wrapping_add(1);
        let seq = self.snapshot_seq;
        let mut everyone = Vec::new();
        if let Some((pos, yaw, held, style, implement)) = host {
            everyone.push((0u32, pos, yaw, held, style, implement));
        }
        for (id, g) in self.guests.iter().filter(|(_, guest)| guest.entry_ready) {
            let implement =
                g.inventory.slots[g.hotbar].and_then(|stack| server.world.implement_visual(stack));
            everyone.push((*id, g.pos, g.yaw, g.held, g.style, implement));
        }
        let ids: Vec<u32> = self
            .guests
            .iter()
            .filter(|(_, guest)| guest.entry_ready)
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            let Some(g) = self.guests.get(&id) else {
                continue;
            };
            let (eye, reach) = (g.pos, snapshot_reach(g.view_dist));
            let near_entity = |p: EntityPos| eye.horizontal_distance_to(p) <= reach;
            let budget = self.net.datagram_budget(id);

            // Other players stay visible past the mob horizon: a person on
            // the skyline is the point of playing together.
            let players: Vec<_> = everyone
                .iter()
                .copied()
                .filter(|(pid, p, ..)| *pid == id || near_entity(*p))
                .collect();
            self.send_snapshot(id, batch_snapshot(seq, players, budget, S2C::Players));

            let mobs: Vec<MobSnap> = server
                .world
                .mobs()
                .iter()
                .filter(|m| near_entity(m.pos))
                .map(|m| MobSnap {
                    id: m.id,
                    species: m.species as u16,
                    pos: m.pos,
                    yaw: m.yaw,
                    growth: m.growth,
                    hurt: m.hurt_flash,
                    health: m.health,
                    fed: m.fed || m.breed_cd > 0.0 || m.growth < 1.0,
                })
                .collect();
            self.send_snapshot(id, batch_snapshot(seq, mobs, budget, S2C::Mobs));

            let bolts: Vec<net::BoltSnap> = server
                .world
                .projectiles()
                .iter()
                .filter(|p| near_entity(p.pos))
                .map(|p| net::BoltSnap {
                    id: p.stable_id,
                    pos: p.pos,
                    vel: p.vel,
                    tile: p.tile,
                    age: p.age,
                })
                .collect();
            self.send_snapshot(id, batch_snapshot(seq, bolts, budget, S2C::Bolts));

            let loose_items: Vec<net::LooseItemSnap> = server
                .world
                .loose_items()
                .iter()
                .filter(|item| near_entity(item.pos))
                .map(|item| net::LooseItemSnap {
                    id: item.stable_id,
                    pos: item.pos,
                    vel: item.vel,
                    item: item.item.0,
                    count: item.count,
                    age: item.age,
                    durability: item.durability,
                    arcane_id: item.arcane_id,
                })
                .collect();
            self.send_snapshot(
                id,
                batch_snapshot(seq, loose_items, budget, S2C::LooseItems),
            );

            // Sent even when empty so guests clear their last tumble.
            let falls: Vec<net::FallSnap> = server
                .world
                .falling_blocks()
                .iter()
                .filter(|f| near_entity(f.pos))
                .map(|f| net::FallSnap {
                    pos: f.pos,
                    block: f.block.0,
                })
                .collect();
            self.send_snapshot(id, batch_snapshot(seq, falls, budget, S2C::Falling));
        }
    }

    /// Resident centers and radius requested by active guests.
    pub fn residency(&self) -> (Vec<ChunkPos>, i32) {
        let mut centers = self
            .guests
            .values()
            .filter_map(|g| g.pos.chunk())
            .collect::<Vec<_>>();
        centers.extend(
            self.pending_guests
                .values()
                .flat_map(|pending| pending.required.iter().copied()),
        );
        if let Some(spawn) = self.fresh_spawn.and_then(|spawn| spawn.chunk())
            && !centers.contains(&spawn)
        {
            centers.push(spawn);
        }
        centers.sort();
        centers.dedup();
        // Dedicated residency adds a two-chunk simulation apron. A one-chunk
        // base radius keeps the corners of the square 5x5 prepared homeland
        // (distance sqrt(8), rounded up to three) resident even with no
        // guests; zero retained only its inscribed 17 chunks.
        let radius = self.guests.values().map(|g| g.view_dist).max().unwrap_or(1);
        (centers, radius)
    }

    /// Generate, encode, and send one chunk, remembering that the guest has
    /// it. The single path for both the streaming ring and `RequestChunk`.
    pub(super) fn stream_chunk(&mut self, server: &mut Server, id: u32, pos: ChunkPos) {
        if !server.world.has_chunk(pos) {
            self.ensure_chunk_jobs(server);
            if let Some(jobs) = self.chunk_jobs.as_mut() {
                jobs.enqueue(pos, false);
            }
            return;
        }
        let Some(chunk) = server.world.chunk(pos) else {
            // Nothing to send — do not record it as sent or the ring skips
            // this chunk forever and the guest cannot repair the hole.
            return;
        };
        self.ensure_chunk_jobs(server);
        let Some(rle) = self
            .chunk_jobs
            .as_mut()
            .and_then(|jobs| jobs.encoded_or_enqueue(pos, chunk))
        else {
            return;
        };
        self.net.send(
            id,
            &S2C::Chunk {
                face: pos.face() as u8,
                u: pos.u(),
                v: pos.v(),
                rle: (*rle).clone(),
            },
        );
        if let Some(g) = self.guests.get_mut(&id) {
            g.sent_chunks.insert(pos);
        }
    }

    #[cfg(test)]
    pub(crate) fn panic_terrain_worker_for_test(&mut self, server: &Server) {
        self.ensure_chunk_jobs(server);
        self.chunk_jobs.as_mut().unwrap().panic_worker_for_test();
    }

    fn ensure_chunk_jobs(&mut self, server: &Server) {
        self.chunk_jobs
            .ensure_context(crate::terrain_jobs::TerrainContext::new(
                server.world.seed,
                server.world.planet_atlas(),
                server.world.chunk_loader(),
            ));
    }

    fn send_snapshot(&self, id: u32, parts: Vec<Vec<u8>>) {
        for bytes in parts {
            self.net.send_datagram(id, bytes);
        }
    }
}
