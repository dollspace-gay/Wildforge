//! Authenticated movement request adapter.

use super::{C2S, HOTBAR_SLOTS, HostSession, PlayerRuntime, S2C, Server, Vec3, refresh_held};

impl HostSession {
    pub(super) fn request_movement(&mut self, server: &mut Server, id: u32, msg: C2S) {
        let Some(guest) = self.guests.get_mut(&id) else { return; };
        match msg {
            C2S::Move {
                pos,
                yaw,
                hotbar,
                sprint,
            } => {
                let elapsed = guest.net_age.clamp(0.03, 0.3);
                let delta = guest.pos.local_delta_to(pos);
                let horizontal = Vec3::new(delta.x, 0.0, delta.z).length();
                let probe = crate::physics::Player::new_at(pos);
                let grounded = pos
                    .translated(Vec3::new(0.0, -0.05, 0.0))
                    .ok()
                    .and_then(|p| p.pos.block())
                    .is_some_and(|p| server.world.reg.is_solid(server.world.get_block_at(p)));
                let in_water = pos
                    .translated(Vec3::new(0.0, 0.6, 0.0))
                    .ok()
                    .and_then(|p| p.pos.block())
                    .is_some_and(|p| server.world.reg.is_water(server.world.get_block_at(p)));
                let airborne_rise = if grounded || in_water {
                    0.0
                } else {
                    guest.airborne_rise + delta.y.max(0.0)
                };
                let valid = pos.is_canonical()
                    && yaw.is_finite()
                    && hotbar < HOTBAR_SLOTS as u8
                    && !probe.collides(&server.world, probe.pos)
                    && airborne_rise <= 2.4
                    && (guest.has_moved
                        && horizontal <= 8.0 * elapsed + 0.35
                        && delta.y.abs() <= 14.0 * elapsed + 0.75
                        || !guest.has_moved && delta.length() <= 3.0);
                if !valid {
                    self.net.send(
                        id,
                        &S2C::PlayerState(PlayerRuntime::from_guest(guest).to_snap()),
                    );
                    return;
                }
                guest.render_from = guest.render_pos();
                guest.net_interval = guest.net_age.clamp(0.03, 0.3);
                guest.net_age = 0.0;
                guest.pos = pos;
                guest.yaw = yaw;
                guest.hotbar = hotbar as usize;
                guest.sprinting = sprint && guest.hunger >= 6.0;
                guest.airborne_rise = airborne_rise;
                guest.has_moved = true;
                refresh_held(guest);
                // Guests leave footprints too; the edit echoes to all.
                if let Some(at) = pos.block() {
                    server.world.tread_at(at);
                }
            }
            C2S::SleepRequest => {
                guest.sleeping = true;
            }
            C2S::SleepCancel => {
                guest.sleeping = false;
            }
            C2S::Respawn => {
                if guest.health > 0.0 {
                    return;
                }
                // A dungeon death (capability E10) wakes at the party's
                // checkpoint with belongings intact; the host never
                // scattered them.
                if let Some(cp) = server.world.dungeon_checkpoint_for(guest.pos) {
                    guest.pos = cp;
                    guest.health = 14.0;
                    guest.hunger = 20.0;
                    guest.since_damage = 100.0;
                    self.send_player_state(id);
                    return;
                }
                // The saved spawn may be buried or dug out by now.
                guest.pos = server.world.settle_spawn_at(guest.spawn);
                guest.health = 14.0;
                guest.hunger = 20.0;
                guest.since_damage = 100.0;
                self.send_player_state(id);
            }

            _ => {}
        }
    }
}
