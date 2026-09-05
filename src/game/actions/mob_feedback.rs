//! Mob feedback in the ordered graphical action pipeline.

use crate::game::Game;
use crate::world::TerrainRead;
use crate::audio::Sfx;
use crate::raycast;
use glam::Vec3;

impl Game {

    /// Nearest mob under the crosshair within reach, unless a solid block
    /// sits in front of it.
    pub(in crate::game) fn mob_in_crosshair(&self, hit: &Option<raycast::PlanetHit>) -> Option<usize> {
        let origin = self.player.eye();
        let dir = self.camera.tangent_forward();
        let reach = self.reach();
        // A wall in the way shields the mob behind it (approximate the
        // wall distance by its block center).
        let wall_t = hit
            .as_ref()
            .map(|h| origin.distance_to(h.block.entity_center()) + 0.5)
            .unwrap_or(reach);
        let mut best: Option<(usize, f32)> = None;
        for (i, m) in self.runtime.view().mobs().iter().enumerate() {
            let def = &self.content.reg.animals[m.species];
            if let Some(t) = m.ray_hit_from(def, origin, dir, reach.min(wall_t))
                && best.is_none_or(|(_, bt)| t < bt)
            {
                best = Some((i, t));
            }
        }
        best.map(|(i, _)| i)
    }

    /// Remove dead mobs: roll their drop table, spill items, notify mods.
    pub(in crate::game) fn present_settled_mob_death(&mut self, death: crate::world::SettledMobDeath) {
        let reg = self.content.reg.clone();
        let Some(def) = reg.animals.get(death.species) else {
            return;
        };
        self.sfx(Sfx::MobDeath(def.sound_pitch));
        let (tile, at) = (
            def.tile,
            death
                .pos
                .translated(Vec3::new(0.0, 0.5, 0.0))
                .expect("death effect stays beside the mob")
                .pos
                .render_pos(),
        );
        self.presentation.burst(at, tile, 12, 2.0);
        if def.hostile && self.content.scripts.wants("on_enemy_destroyed") {
            self.content.scripts.dispatch_view(
                &self.runtime.view(),
                "on_enemy_destroyed",
                (
                    def.name.clone(),
                    death.pos.face().name().to_string(),
                    death.pos.u().floor() as i64,
                    death.pos.y().floor() as i64,
                    death.pos.v().floor() as i64,
                ),
            );
            self.apply_script_cmds();
        }
        if self.content.scripts.wants("on_animal_killed") {
            self.content.scripts.dispatch_view(
                &self.runtime.view(),
                "on_animal_killed",
                (
                    def.name.clone(),
                    death.pos.face().name().to_string(),
                    death.pos.u().floor() as i64,
                    death.pos.y().floor() as i64,
                    death.pos.v().floor() as i64,
                ),
            );
            self.apply_script_cmds();
        }
    }
}
