//! Guest terrain delivery and perception-bounded world snapshots.

use glam::Vec3;

use super::HostSession;
use crate::chunk::ChunkPos;
use crate::net::{self, MobSnap, S2C, batch_snapshot};
use crate::server::Server;

/// Chunks pushed per guest per pump. The ring is paced so a guest asking for
/// a wide view does not make the host generate hundreds of chunks in one
/// frame; `RequestChunk` fills anything the pacing misses.
const CHUNKS_PER_PUMP: usize = 8;

/// How far, in blocks, a guest is told about mobs, bolts and tumbling sand.
/// A little inside its terrain horizon: things it cannot see are things it
/// does not need, and this keeps snapshots bounded by perception.
fn snapshot_reach(view_dist: i32) -> f32 {
    (view_dist.max(1) * crate::chunk::CHUNK_X as i32) as f32
}

impl HostSession {
    /// Stream the nearest missing chunks for each guest, paced per pump.
    pub(super) fn stream_chunks(&mut self, server: &mut Server) {
        let guest_ids: Vec<u32> = self.guests.keys().copied().collect();
        for id in guest_ids {
            let (gpos, vd) = {
                let g = &self.guests[&id];
                (g.pos, g.view_dist)
            };
            let center = ChunkPos::of_world(gpos.x as i32, gpos.z as i32);
            // A guest that walked away has dropped these; forget that we sent
            // them so walking back re-streams instead of leaving a hole.
            if let Some(g) = self.guests.get_mut(&id) {
                let keep = vd + 2;
                g.sent_chunks
                    .retain(|(cx, cz)| (cx - center.x).abs().max((cz - center.z).abs()) <= keep);
            }
            let mut needed = Vec::new();
            'scan: for r in 0..=vd {
                for dx in -r..=r {
                    for dz in -r..=r {
                        if dx.abs().max(dz.abs()) != r {
                            continue;
                        }
                        let cp = (center.x + dx, center.z + dz);
                        if !self.guests[&id].sent_chunks.contains(&cp) {
                            needed.push(cp);
                            if needed.len() >= CHUNKS_PER_PUMP {
                                break 'scan;
                            }
                        }
                    }
                }
            }
            for (cx, cz) in needed {
                self.stream_chunk(server, id, cx, cz);
            }
        }
    }

    /// Send players and authoritative world actors at 20 Hz, culled to each
    /// guest's negotiated perception horizon.
    pub(super) fn stream_snapshots(
        &mut self,
        server: &Server,
        host: Option<(Vec3, f32, u16, u32)>,
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
        if let Some((pos, yaw, held, style)) = host {
            everyone.push((0u32, pos, yaw, held, style));
        }
        for (id, g) in &self.guests {
            everyone.push((*id, g.pos, g.yaw, g.held, g.style));
        }
        let ids: Vec<u32> = self.guests.keys().copied().collect();
        for id in ids {
            let Some(g) = self.guests.get(&id) else {
                continue;
            };
            let (eye, reach) = (g.pos, snapshot_reach(g.view_dist));
            let near = |p: Vec3| {
                let (dx, dz) = (p.x - eye.x, p.z - eye.z);
                dx * dx + dz * dz <= reach * reach
            };
            let budget = self.net.datagram_budget(id);

            // Other players stay visible past the mob horizon: a person on
            // the skyline is the point of playing together.
            let players: Vec<_> = everyone
                .iter()
                .copied()
                .filter(|(pid, p, ..)| *pid == id || near(*p))
                .collect();
            self.send_snapshot(id, batch_snapshot(seq, players, budget, S2C::Players));

            let mobs: Vec<MobSnap> = server
                .world
                .mobs()
                .iter()
                .filter(|m| near(m.pos))
                .map(|m| MobSnap {
                    id: m.id,
                    species: m.species as u16,
                    pos: m.pos,
                    yaw: m.yaw,
                    growth: m.growth,
                    hurt: m.hurt_flash,
                    fed: m.fed || m.breed_cd > 0.0 || m.growth < 1.0,
                })
                .collect();
            self.send_snapshot(id, batch_snapshot(seq, mobs, budget, S2C::Mobs));

            let bolts: Vec<net::BoltSnap> = server
                .world
                .projectiles()
                .iter()
                .filter(|p| near(p.pos))
                .map(|p| net::BoltSnap {
                    pos: p.pos,
                    vel: p.vel,
                    tile: p.tile,
                    age: p.age,
                })
                .collect();
            self.send_snapshot(id, batch_snapshot(seq, bolts, budget, S2C::Bolts));

            // Sent even when empty so guests clear their last tumble.
            let falls: Vec<net::FallSnap> = server
                .world
                .falling_blocks()
                .iter()
                .filter(|f| near(f.pos))
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
        let centers = self
            .guests
            .values()
            .map(|g| ChunkPos::of_world(g.pos.x as i32, g.pos.z as i32))
            .collect();
        let radius = self.guests.values().map(|g| g.view_dist).max().unwrap_or(0);
        (centers, radius)
    }

    /// Generate, encode, and send one chunk, remembering that the guest has
    /// it. The single path for both the streaming ring and `RequestChunk`.
    pub(super) fn stream_chunk(&mut self, server: &mut Server, id: u32, cx: i32, cz: i32) {
        let cp = ChunkPos { x: cx, z: cz };
        server.world.ensure_chunk(cp);
        let Some(rle) = server.world.chunk_rle(cp) else {
            // Nothing to send — do not record it as sent or the ring skips
            // this chunk forever and the guest cannot repair the hole.
            return;
        };
        self.net.send(id, &S2C::Chunk { x: cx, z: cz, rle });
        if let Some(g) = self.guests.get_mut(&id) {
            g.sent_chunks.insert((cx, cz));
        }
    }

    fn send_snapshot(&self, id: u32, parts: Vec<Vec<u8>>) {
        for bytes in parts {
            self.net.send_datagram(id, bytes);
        }
    }
}
