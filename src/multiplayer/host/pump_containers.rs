//! Pump containers for the authoritative host session.

use super::{BlockPos, HostSession, Server};

impl HostSession {
    pub(super) fn pump_containers(&mut self, server: &Server, dt: f32) {
        // Open containers stay live: furnaces smelt and other players
        // shuffle stacks while a guest is looking at them.
        self.container_timer += dt;
        if self.container_timer >= 0.5 {
            self.container_timer = 0.0;
            let open: Vec<(u32, BlockPos)> = self
                .guests
                .iter()
                .filter(|(_, guest)| guest.entry_ready)
                .filter_map(|(id, g)| g.container.map(|c| (*id, c)))
                .collect();
            for (id, pos) in open {
                self.send_container(server, id, pos);
            }
        }
    }
}
