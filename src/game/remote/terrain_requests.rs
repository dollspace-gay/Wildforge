//! Terrain requests graphical guest adapter.

use crate::chunk::CHUNK_X;
use crate::net;
use crate::game::Remote;

impl Game {
    pub(in crate::game) fn request_missing_chunks(&mut self, r: &mut Remote) {
        const ASK_PER_FRAME: usize = 4;
        let vd = r.granted_view_dist.min(self.config.view_dist);
        let Some(center) = self.player.pos.chunk() else {
            return;
        };
        let mut asked = 0;
        for ring in 0..=vd {
            for dx in -ring..=ring {
                for dz in -ring..=ring {
                    if dx.abs().max(dz.abs()) != ring {
                        continue;
                    }
                    let pos = center.offset(dx, dz);
                    if pos.distance(center) > f64::from(vd * CHUNK_X as i32) + 1.0 {
                        continue;
                    }
                    if self.runtime.view().has_chunk(pos) || !r.wants.insert(pos) {
                        continue;
                    }
                    r.session.send(&net::C2S::RequestChunk {
                        face: pos.face() as u8,
                        u: pos.u(),
                        v: pos.v(),
                    });
                    asked += 1;
                    if asked >= ASK_PER_FRAME {
                        return;
                    }
                }
            }
        }
    }
}
