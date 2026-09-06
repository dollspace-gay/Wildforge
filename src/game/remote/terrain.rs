//! Terrain graphical guest adapter.

use super::RemoteFlow;
use crate::chunk::ChunkPos;
use crate::net;
use crate::world;
use glam::Vec3;
use crate::game::Remote;

impl Game {
    pub(in crate::game) fn remote_terrain_message(&mut self, r: &mut Remote, message: net::S2C) -> RemoteFlow {
        match message {
                net::S2C::Chunk { face, u, v, rle } => {
                    let Some(face) = crate::planet::Face::from_u8(face) else {
                        return RemoteFlow::Continue;
                    };
                    let Ok(pos) = ChunkPos::new(face, u, v) else {
                        return RemoteFlow::Continue;
                    };
                    // Never decode an arbitrarily large host burst inline.
                    // A prepared host can encode the whole view faster than a
                    // software-rendered client presents frames; inserting all
                    // of those chunks here froze the UI immediately after
                    // EntryAccepted. The paced adoption stage below is shared
                    // by admission and ordinary view expansion.
                    if !self.runtime.view().has_chunk(pos) && !r.session.has_queued_chunk(pos) {
                        // Proactively pushed chunks are pending too; marking
                        // them wanted prevents the repair scan from asking for
                        // duplicates before paced adoption reaches them.
                        r.wants.insert(pos);
                        r.session.queue_chunk(pos, rle);
                    }
                }
                net::S2C::BlockSet {
                    pos,
                    id,
                    meta,
                    salt_mass,
                    soil_salinity,
                } => {
                    let local = r
                        .session
                        .queue_block(pos, id, meta, salt_mass, soil_salinity);
                    let old = self.runtime.view().get_block_at(pos);
                    // Someone broke something: the world crumbles for
                    // everyone watching.
                    if local == crate::registry::AIR
                        && old != crate::registry::AIR
                        && self.content.reg.block(old).hardness.is_some()
                    {
                        let center = Vec3::new(
                            pos.surface().centered_u() as f32 + 0.5,
                            pos.y() as f32 + 0.5,
                            pos.surface().centered_v() as f32 + 0.5,
                        );
                        if (center - self.camera.pos).length() < 40.0 {
                            self.presentation.burst(center, self.content.reg.block(old).tiles[0], 8, 2.0);
                        }
                    }
                }
                net::S2C::ViewDistance { chunks } => {
                    r.granted_view_dist = chunks.max(1) as i32;
                }
            _ => {}
        }
        RemoteFlow::Continue
    }
}
