//! Pump sleep for the authoritative host session.

use super::{HostFx, HostSession, S2C, Server};

impl HostSession {
    pub(super) fn pump_sleep(
        &mut self,
        server: &mut Server,
        dt: f32,
        host_present: bool,
        host_sleeping: bool,
        fx: &mut Vec<HostFx>,
    ) {
        // Sleep vote.
        if !host_sleeping && !self.guests.values().any(|g| g.entry_ready && g.sleeping) {
            self.sleep_settle = 0.0;
        }
        if host_sleeping || self.guests.values().any(|g| g.entry_ready && g.sleeping) {
            let present =
                self.guests.values().filter(|g| g.entry_ready).count() as u32 + host_present as u32;
            let sleeping = self
                .guests
                .values()
                .filter(|g| g.entry_ready && g.sleeping)
                .count() as u32
                + host_sleeping as u32;
            self.broadcast_ready(&S2C::Sleep { sleeping, present });
            self.sleep_settle = if sleeping == present {
                self.sleep_settle + dt
            } else {
                0.0
            };
            if sleeping == present && self.sleep_settle >= 0.75 {
                self.sleep_settle = 0.0;
                let skipped = (1.0 + 0.3 - server.time_of_day) % 1.0;
                if server.world.tick_ire(skipped) {
                    server.world.accept_offerings();
                }
                server.sleep_to_dawn();
                for g in self.guests.values_mut().filter(|guest| guest.entry_ready) {
                    g.sleeping = false;
                }
                self.broadcast_ready(&S2C::TimeIre {
                    time: server.time_of_day,
                    ire: server.world.ire,
                    day: server.world.day(),
                });
                self.broadcast_ready(&S2C::Toast("Dawn. The camp wakes.".into()));
                fx.push(HostFx::AllSlept);
            }
        }
    }
}
