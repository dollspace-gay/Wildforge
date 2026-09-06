//! Pump observations for the authoritative host session.

use super::{HostSession, S2C, Server, inspectable_arcane_items};

impl HostSession {
    pub(super) fn pump_observations(&mut self, server: &Server, dt: f32) {
        self.state_timer += dt;
        if self.state_timer >= 1.0 {
            self.state_timer = 0.0;
            self.broadcast_ready(&S2C::TimeIre {
                time: server.time_of_day,
                ire: server.world.ire,
                day: server.world.day(),
            });
            // Active effects are durable host state, not one-shot animation
            // packets. Refreshing these small qualitative cues lets late
            // joiners and packet-delayed guests see Gleam, ritual paths, and
            // persistent warning bands without exposing private accounting.
            for cue in server.world.working_cues() {
                self.broadcast_ready(&S2C::WorkingEvent(cue));
            }
            if let Some(atlas) = server.world.planet_atlas() {
                let side = atlas.side();
                let updates: Vec<_> =
                    self.guests
                        .iter()
                        .filter(|(_, guest)| guest.entry_ready)
                        .map(|(id, guest)| {
                            let center = atlas.atlas_pos(guest.pos.surface());
                            let mut positions = vec![center];
                            positions.extend(center.neighbors8(side));
                            positions.sort();
                            positions.dedup();
                            let cells = positions
                                .into_iter()
                                .map(|pos| {
                                    let center = pos.center(side);
                                    let surface =
                                        crate::planet::SurfacePos::new(
                                            center.face,
                                            center.u.floor().clamp(
                                                0.0,
                                                f64::from(crate::planet::FACE_BLOCKS - 1),
                                            ) as u16,
                                            center.v.floor().clamp(
                                                0.0,
                                                f64::from(crate::planet::FACE_BLOCKS - 1),
                                            ) as u16,
                                        )
                                        .expect("atlas weather center is canonical");
                                    (pos, server.world.weather_at_surface(surface))
                                })
                                .collect();
                            (*id, S2C::WeatherCells { side, cells })
                        })
                        .collect();
                for (id, update) in updates {
                    self.net.send(id, &update);
                }
                for (id, guest) in self.guests.iter().filter(|(_, guest)| guest.entry_ready) {
                    let region = atlas.atlas_pos(guest.pos.surface());
                    let (bands, dominant) = server.world.arcane_sensory_cue_at(region);
                    let ecology = server
                        .world
                        .arcane_ecology_observation_at(guest.pos.surface(), 72.0)
                        .map(|observation| (observation.text, observation.damped));
                    self.net.send(
                        *id,
                        &S2C::ArcaneCue {
                            bands,
                            dominant,
                            ecology,
                        },
                    );
                }
                let item_updates = self
                    .guests
                    .iter()
                    .filter(|(_, guest)| guest.entry_ready)
                    .map(|(id, guest)| (*id, inspectable_arcane_items(&server.world, guest)))
                    .collect::<Vec<_>>();
                for (id, (charges, implements, apparatus)) in item_updates {
                    let Some(guest) = self.guests.get_mut(&id) else {
                        continue;
                    };
                    if guest.last_arcane_items != charges
                        || guest.last_implements != implements
                        || guest.last_apparatus != apparatus
                    {
                        guest.last_arcane_items.clone_from(&charges);
                        guest.last_implements.clone_from(&implements);
                        guest.last_apparatus.clone_from(&apparatus);
                        // Public implement metadata is bounded to 1 KiB per
                        // identity, but a legitimately open chest/cargo pack
                        // can expose many identities at once. Split the
                        // reliable replacement snapshot so no mod-valid
                        // inventory can exceed the transport frame budget.
                        const IMPLEMENTS_PER_FRAME: usize = 16;
                        let batches = implements.len().div_ceil(IMPLEMENTS_PER_FRAME).max(1);
                        for batch in 0..batches {
                            let start = batch * IMPLEMENTS_PER_FRAME;
                            let end = (start + IMPLEMENTS_PER_FRAME).min(implements.len());
                            self.net.send(
                                id,
                                &S2C::ArcaneItems {
                                    reset: batch == 0,
                                    charges: if batch == 0 {
                                        charges.clone()
                                    } else {
                                        Vec::new()
                                    },
                                    implements: implements[start..end].to_vec(),
                                    apparatus: if batch == 0 {
                                        apparatus.clone()
                                    } else {
                                        Vec::new()
                                    },
                                },
                            );
                        }
                    }
                }
            }
        }
    }
}
