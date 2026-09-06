//! Frame graphical guest adapter.

use super::RemoteFlow;
use crate::game::Game;
use crate::game::Remote;
use crate::mesher;
use crate::net;
use crate::world::TerrainRead;
use glam::Vec3;

impl Game {
    pub(in crate::game) fn remote_entry_terrain(&mut self, r: &mut Remote) -> RemoteFlow {
        // Decode terrain at a fixed cadence. During admission this brings in
        // the exact safety set; afterward it prevents a fast host's full-view
        // burst from monopolizing the render/input thread. Once all nine entry
        // chunks are resident, build the spawn chunk's first visible mesh
        // before claiming readiness; Welcome by itself never exposes a blank
        // world.
        const REMOTE_CHUNKS_PER_FRAME: usize = 8;
        if let Some((world, _)) = self.runtime.guest_mut() {
            for position in r.session.apply_terrain(world, REMOTE_CHUNKS_PER_FRAME) {
                r.wants.remove(&position);
            }
        }
        if let Some(center) = r.session.admission().frame_needed()
            && [(-1, 0), (1, 0), (0, -1), (0, 1)]
                .iter()
                .all(|(du, dv)| self.runtime.view().has_chunk(center.offset(*du, *dv)))
        {
            let mesh =
                mesher::mesh_chunk(&self.runtime.view(), center, &self.content.tile_variants);
            self.renderer.upload_chunk(center, &mesh);
            self.presentation.lights.chunk_meshed(center, mesh.emitters);
            self.runtime.mark_chunk_meshed(center);
            if let Err(error) = r.session.frame_ready() {
                self.multiplayer.join_status = format!("FAILED: {error}").to_uppercase();
                return RemoteFlow::Abort;
            }
        }
        if r.session.take_ready() {
            r.session.send(&net::C2S::EntryReady);
        }
        RemoteFlow::Continue
    }
    pub(in crate::game) fn remote_presentation(&mut self, r: &mut Remote, dt: f32) {
        // Snapshot smoothing: glide players and mobs along their spans,
        // dead-reckon bolts, advance walk cycles from apparent speed.
        r.player_age += dt;
        r.mob_age += dt;
        let t = (r.player_age / r.player_interval.max(0.001)).clamp(0.0, 1.0);
        for (id, entry) in r.players.iter_mut() {
            if let Some(l) = r.player_lerp.get(id) {
                let (p, y) = l.at(t);
                entry.1 = p;
                entry.2 = y;
            }
        }
        let t = (r.mob_age / r.mob_interval.max(0.001)).clamp(0.0, 1.0);
        if let Some((world, _)) = self.runtime.guest_mut() {
            world.for_each_mob_mut(|m| {
                if let Some(l) = r.mob_lerp.get_mut(&m.id) {
                    let (_, y) = l.at(t);
                    m.yaw = y;
                    let d = l.to - l.from;
                    let hspeed = Vec3::new(d.x, 0.0, d.z).length() / r.mob_interval.max(0.03);
                    l.phase += hspeed * dt * 3.2; // same feel as the local tick
                    m.anim_phase = l.phase;
                    m.hurt_flash = (m.hurt_flash - dt).max(0.0);
                }
            });
            world.for_each_projectile_mut(|p| {
                if let Ok(moved) = p.pos.translated(p.vel * dt) {
                    p.pos = moved.pos;
                    p.vel = moved.rotation.rotate_vec3(p.vel);
                }
                p.age += dt;
            });
        }
    }
    pub(in crate::game) fn remote_upstream(&mut self, r: &mut Remote, dt: f32) {
        // Our movement upstream at 20 Hz.
        if self.in_world {
            self.multiplayer.move_timer += dt;
            if self.multiplayer.move_timer >= 0.05 {
                self.multiplayer.move_timer = 0.0;
                r.session.send_datagram(&net::C2S::Move {
                    pos: self.player.pos,
                    yaw: self.camera.yaw,
                    hotbar: self.input.hotbar_sel as u8,
                    sprint: self.input.keys.sprint,
                });
            }
            // Tell the host how far we want to see, whenever that changes.
            // The host clamps and answers; until it does we keep the ring
            // we were given.
            let want = self.config.view_dist;
            if want != r.asked_view_dist {
                r.asked_view_dist = want;
                r.session
                    .send(&net::C2S::SetViewDistance { chunks: want as u8 });
            }
            // Ask for terrain we are missing inside the granted radius.
            // The host pushes a ring as we walk, but ground we evicted and
            // came back to is ours to ask for — it believes we still have it.
            self.request_missing_chunks(r);
        }
    }
}
