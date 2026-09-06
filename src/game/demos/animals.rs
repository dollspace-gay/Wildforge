//! Animals capture scene construction.

use crate::world::TerrainRead;
use crate::world;
use glam::Vec3;
use crate::game::Game;
use crate::planet::{BlockPos, EntityPos, Face, SurfacePos};
use super::DemoChart;

impl Game {
    pub(in crate::game) fn stage_capture_wardens(&mut self, spawn: EntityPos, chart: DemoChart) {
        // Dev: a row of wardens near spawn (rendering/combat verification).
        if std::env::var("WILDFORGE_DEMO_WARDENS").is_ok() {
            for (i, name) in [
                "base:thornling",
                "base:dryad",
                "base:emberkin",
                "base:gravelurk",
                "base:wrathwood",
            ]
            .iter()
            .enumerate()
            {
                if let Some(si) = self.content.reg.animal_id(name) {
                    let x = spawn.x as i32 - 4 + i as i32 * 3;
                    let z = spawn.z as i32 - 7;
                    let y = demo_height!(self.runtime.local().world, chart, x, z) + 1;
                    let mut m = demo_mob!(
                        chart,
                        si,
                        Vec3::new(x as f32 + 0.5, y as f32 + 0.05, z as f32 + 0.5),
                        0.0,
                    );
                    m.health = self.content.reg.animals[si].health;
                    self.runtime.local_mut().world.spawn_mob(m);
                }
            }
        }
    }

    pub(in crate::game) fn stage_capture_mobs(&mut self, spawn: EntityPos, chart: DemoChart) {
        // Dev: a small menagerie near spawn (rendering/combat verification).
        if std::env::var("WILDFORGE_DEMO_MOBS").is_ok() {
            for (i, name) in [
                "base:deer",
                "base:boar",
                "base:goat",
                "base:grouse",
                "base:rabbit",
            ]
            .iter()
            .enumerate()
            {
                if let Some(si) = self.content.reg.animal_id(name) {
                    let x = spawn.x as i32 - 3 + i as i32 * 2;
                    let z = spawn.z as i32 - 6;
                    let y = demo_height!(self.runtime.local().world, chart, x, z) + 1;
                    let mut m = demo_mob!(
                        chart,
                        si,
                        Vec3::new(x as f32 + 0.5, y as f32 + 0.05, z as f32 + 0.5),
                        i as f32 * 1.3,
                    );
                    m.health = self.content.reg.animals[si].health;
                    self.runtime.local_mut().world.spawn_mob(m);
                }
            }
        }
    }

    pub(in crate::game) fn stage_capture_flight(&mut self, spawn: EntityPos, chart: DemoChart) {
        // Dev: a line of fliers at eye level ahead, wings mid-beat
        // (flight and wingbeat verification — the one thing you cannot
        // judge from a still of a bird standing on the ground).
        if std::env::var("WILDFORGE_DEMO_FLIGHT").is_ok() {
            for (i, name) in ["base:gull", "base:eagle", "base:vulture", "base:bat"]
                .iter()
                .enumerate()
            {
                if let Some(si) = self.content.reg.animal_id(name) {
                    let x = spawn.x as i32 - 4 + i as i32 * 3;
                    let z = spawn.z as i32 - 9;
                    let y = demo_height!(self.runtime.local().world, chart, x, z) + 4;
                    let mut m = demo_mob!(
                        chart,
                        si,
                        Vec3::new(x as f32 + 0.5, y as f32, z as f32 + 0.5),
                        std::f32::consts::FRAC_PI_2,
                    );
                    m.health = self.content.reg.animals[si].health;
                    // Staggered so one still shows the whole stroke.
                    m.anim_phase = i as f32 * 0.9;
                    self.runtime.local_mut().world.spawn_mob(m);
                }
            }
        }
    }
}
