//! Pump events for the authoritative host session.

use super::{AuthenticatedJoin, HostEvent, HostFx, HostSession, PlayerRuntime, S2C, Server};

impl HostSession {
    pub(super) fn pump_events(&mut self, server: &mut Server, dt: f32, fx: &mut Vec<HostFx>) {
        for ev in self.net.poll() {
            match ev {
                HostEvent::Joined {
                    id,
                    display_name,
                    principal,
                    principals,
                    verification_cached,
                    verified_handle,
                    public_handle,
                    content_hash,
                    style,
                } => {
                    self.on_join(
                        server,
                        AuthenticatedJoin {
                            id,
                            display_name,
                            principal,
                            principals,
                            verification_cached,
                            verified_handle,
                            public_handle,
                            content_hash,
                            style,
                        },
                        fx,
                    );
                }
                HostEvent::Left { id } => {
                    self.pending_guests.remove(&id);
                    if let Some(g) = self.guests.remove(&id) {
                        if let Err(error) = server.world.interrupt_actor_workings(g.player_id.0) {
                            eprintln!(
                                "workings: disconnect settlement for {} failed: {error}",
                                g.player_id
                            );
                        }
                        if let Some(profiles) = &self.profiles
                            && let Err(e) =
                                profiles.save(&PlayerRuntime::from_guest(&g), &server.world.reg)
                        {
                            eprintln!("profiles: save {} failed: {e}", g.name);
                        }
                        if g.entry_ready {
                            self.broadcast_ready(&S2C::Left { id });
                            fx.push(HostFx::Left(g.name));
                        }
                    }
                }
                HostEvent::Msg { id, msg } => {
                    self.on_msg(server, id, msg, fx);
                }
            }
        }

        let mut progress = Vec::new();
        for (id, pending) in &mut self.pending_guests {
            pending.progress_age += dt;
            if pending.progress_age >= 1.0 {
                pending.progress_age -= 1.0;
                let resident = pending
                    .required
                    .iter()
                    .filter(|position| server.world.has_chunk(**position))
                    .count();
                progress.push((*id, resident as u16, pending.required.len() as u16));
            }
        }
        for (id, resident, total) in progress {
            self.net.send(id, &S2C::EntryProgress { resident, total });
        }

    }
}
