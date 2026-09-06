//! Point lights in the graphical frame pipeline.

use crate::audio::Sfx;
use crate::game::Game;
use crate::lights;
use crate::registry::ItemId;
use glam::Vec3;

impl Game {
    pub(in crate::game) fn prepare_frame_point_lights(
        &mut self,
        dt: f32,
    ) -> Vec<crate::renderer::PointLight> {
        // Point lights: promote nearby emitters + the dynamic set.
        let mut dyn_lights = self.presentation.demo_lights.clone();
        self.presentation
            .working_cues
            .retain(|_, (_, seen)| self.time_abs - *seen <= 2.5);
        let active_workings = if self.multiplayer.remote.is_some() {
            self.presentation
                .working_cues
                .values()
                .map(|(cue, _)| cue.clone())
                .collect::<Vec<_>>()
        } else {
            self.runtime.local().world.working_cues()
        };
        for cue in active_workings
            .iter()
            .filter(|cue| cue.handler == crate::workings::WorkingHandler::Gleam)
        {
            let Some(target) = cue.path.last().copied() else {
                continue;
            };
            let warning = f32::from(cue.warning_band.min(3)) / 3.0;
            dyn_lights.push(lights::DynLight {
                key: lights::Key::Working(cue.stable_id),
                pos: target.entity_center().render_pos(),
                color: Vec3::new(
                    0.56 + warning * 0.25,
                    0.74 - warning * 0.18,
                    1.0 - warning * 0.25,
                ),
                range: 5.0,
            });
        }
        // The held torch: your own body of light, real shadows and all.
        // Anchored to the body center, never the facing — a camera-
        // relative offset made the light orbit the head when turning,
        // so shadows stuck then snapped with every look-around (and
        // thrashed the cube cache). Remote helds anchor the same way.
        if self.in_world
            && let Some(stack) = self.inventory.slots[self.input.hotbar_sel]
        {
            let glow = self
                .runtime
                .view()
                .implement_visual(stack)
                .and_then(|visual| self.implement_glow(visual))
                .or_else(|| self.held_glow(stack.item));
            if let Some((color, range)) = glow {
                let held_pos = if self.camera.mode == crate::camera::CameraMode::First {
                    // First person: the light rides the camera, near the hand.
                    self.camera.pos - self.camera.up() * 0.15
                } else {
                    // Chase / orbit: the body carries the held item at its
                    // right hand, chest-high and a little ahead of the feet.
                    let f = self.camera.forward();
                    let r = f.cross(self.camera.up()).normalize_or_zero();
                    self.player.pos.render_pos() + self.camera.up() * 1.2 + r * 0.35 + f * 0.35
                };
                dyn_lights.push(lights::DynLight {
                    key: lights::Key::Held,
                    pos: held_pos,
                    color,
                    range,
                });
            }
        }
        // Placed charge vessels are not unconditional glowing blocks. Their
        // restrained light follows the authoritative qualitative charge band;
        // guests receive only these nearby bands, never exact custody. Damage
        // warms the hue and a nearby strained vessel gives a sparse warning
        // envelope even when nobody has a frame screen open.
        let apparatus_cues = if self.in_world {
            self.runtime
                .view()
                .apparatus_cues_near(self.player.pos, 48.0)
        } else {
            Vec::new()
        };
        for cue in &apparatus_cues {
            if cue.charge_band == 0 {
                continue;
            }
            let strength = 0.35 + f32::from(cue.charge_band.min(3)) * 0.24;
            let strain = f32::from(cue.strain_band.min(3)) / 3.0;
            let color = Vec3::new(
                0.35 + strain * 0.35,
                0.62 - strain * 0.12,
                0.95 - strain * 0.22,
            ) * strength;
            dyn_lights.push(lights::DynLight {
                key: lights::Key::Block(cue.pos),
                pos: cue.pos.entity_center().render_pos(),
                color,
                range: 2.5 + f32::from(cue.charge_band.min(3)) * 1.7,
            });
        }
        if self.total_frames.is_multiple_of(300)
            && apparatus_cues.iter().any(|cue| {
                cue.strain_band >= 2 && self.player.pos.distance_to(cue.pos.entity_center()) <= 12.0
            })
        {
            self.sfx(Sfx::ImplementStrain);
        }
        // The remaining dynamic slots go to whatever is closest: other
        // players' torches or glowing wardens.
        if self.in_world {
            let cam = self.camera.pos;
            let mut tail: Vec<(f32, lights::DynLight)> = Vec::new();
            if let Some(r) = &self.multiplayer.remote {
                for id in r.players.keys() {
                    let Some(&held) = r.player_held.get(id) else {
                        continue;
                    };
                    let local = r.session.content().item(held);
                    let glow = r
                        .player_implement
                        .get(id)
                        .copied()
                        .and_then(|visual| self.implement_glow(visual))
                        .or_else(|| local.and_then(|item| self.held_glow(item)));
                    if let Some((color, range)) = glow
                        && let Some(logical) = r.player_positions.get(id).copied()
                    {
                        let pos = logical
                            .translated(Vec3::new(0.0, 1.4, 0.0))
                            .expect("held light stays in the voxel shell")
                            .pos
                            .render_pos();
                        tail.push((
                            pos.distance(cam),
                            lights::DynLight {
                                key: lights::Key::RemoteHeld(*id),
                                pos,
                                color,
                                range,
                            },
                        ));
                    }
                }
            }
            if let Some(sess) = &self.multiplayer.host {
                for (id, g) in &sess.guests {
                    if !g.is_active() {
                        continue;
                    }
                    let stack = g.inventory.slots[g.hotbar];
                    let glow = stack
                        .and_then(|stack| self.runtime.view().implement_visual(stack))
                        .and_then(|visual| self.implement_glow(visual))
                        .or_else(|| {
                            (g.held != u16::MAX)
                                .then_some(ItemId(g.held))
                                .and_then(|item| self.held_glow(item))
                        });
                    if let Some((color, range)) = glow {
                        let p = g
                            .render_entity_pos()
                            .translated(Vec3::new(0.0, 1.4, 0.0))
                            .expect("guest held light stays in the voxel shell")
                            .pos
                            .render_pos();
                        tail.push((
                            p.distance(cam),
                            lights::DynLight {
                                key: lights::Key::RemoteHeld(*id),
                                pos: p,
                                color,
                                range,
                            },
                        ));
                    }
                }
            }
            for m in self.runtime.view().mobs().iter().filter(|m| m.id != 0) {
                let Some(g) = self.content.reg.animals[m.species].glow else {
                    continue;
                };
                let d = (m.pos.render_pos() - cam).length();
                if d < 32.0 {
                    let pos = m
                        .pos
                        .translated(Vec3::new(0.0, 0.7, 0.0))
                        .expect("mob light stays in the voxel shell")
                        .pos
                        .render_pos();
                    tail.push((
                        d,
                        lights::DynLight {
                            key: lights::Key::Mob(m.id),
                            pos,
                            color: Vec3::from(g),
                            range: 12.0,
                        },
                    ));
                }
            }
            tail.sort_by(|a, b| a.0.total_cmp(&b.0));
            let spare = lights::MAX_DYNAMIC.saturating_sub(dyn_lights.len());
            dyn_lights.extend(tail.into_iter().take(spare).map(|(_, l)| l));
        }

        if self.in_world && self.config.lights > 0 {
            self.presentation.lights.frame(
                self.camera.pos,
                &dyn_lights,
                dt,
                self.config.lights >= 2,
            )
        } else {
            Vec::new()
        }
    }
}
