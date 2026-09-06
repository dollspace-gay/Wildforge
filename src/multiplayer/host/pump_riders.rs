//! Pump riders for the authoritative host session.

use super::{EntityPos, HostSession, Server, Vec3};

impl HostSession {
    pub(super) fn pump_riders(&mut self, server: &mut Server) {
        // Vehicles follow their riders exactly (the rider's client
        // owns their motion; the boat is presentation that floats).
        {
            let riders: Vec<(u32, EntityPos)> = self
                .guests
                .iter()
                .filter(|(_, guest)| guest.entry_ready)
                .map(|(gid, g)| (*gid, g.pos))
                .collect();
            for m in server.world.mobs_mut() {
                if let Some(rid) = m.ridden_by
                    && rid != 0
                {
                    match riders.iter().find(|(gid, _)| *gid == rid) {
                        Some((_, at)) => {
                            m.pos = at
                                .translated(Vec3::new(0.0, -0.35, 0.0))
                                .expect("vehicle remains below rider")
                                .pos;
                            m.vel = Vec3::ZERO;
                        }
                        None => m.ridden_by = None,
                    }
                }
            }
        }
    }
}
