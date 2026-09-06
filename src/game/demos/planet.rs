//! Planet capture scene construction.

use crate::world::TerrainRead;
use crate::chunk::SEA_LEVEL;
use crate::registry::AIR;
use crate::world;
use glam::Vec3;
use crate::game::Game;
use crate::planet::{BlockPos, EntityPos, Face, SurfacePos};
use super::DemoChart;

impl Game {
    /// Deterministic scenes used by the finite-planet visual gate.
    pub(in crate::game) fn stage_planet_qualification(&mut self, scene: &str) {
        if matches!(scene, "sea" | "mountain") {
            let anchor = self.qualification_ocean();
            self.config.view_dist = 14;
            let y = if scene == "sea" {
                (SEA_LEVEL + 1) as f32
            } else {
                (SEA_LEVEL + 42) as f32
            };
            self.player.pos = EntityPos::new(
                anchor.face(),
                f32::from(anchor.u()) + 0.5,
                y,
                f32::from(anchor.v()) + 0.5,
            )
            .expect("qualification altitude is inside the voxel shell");
            self.player.vel = Vec3::ZERO;
            self.survival.spawn_point = self.player.pos;
            self.flying = true;
            self.camera.yaw = 0.18;
            self.camera.pitch = -0.035;
            self.camera.follow_planet(self.player.eye());
            eprintln!(
                "planet qualification {scene}: {:?} {},{} y={y}",
                anchor.face(),
                anchor.u(),
                anchor.v()
            );
            return;
        }

        let chart = DemoChart::new(Face::PosZ);
        let (center_x, center_z, radius) = if scene == "corner" {
            (4095, 4095, 3)
        } else {
            (4095, 0, 3)
        };
        let center = chart.chunk(center_x, center_z);
        for du in -radius..=radius {
            for dv in -radius..=radius {
                self.runtime.local_mut().world.ensure_chunk(center.offset(du, dv));
            }
        }
        self.config.view_dist = 7;

        let stone = self.content.reg.block_id("base:cobblestone").unwrap_or(AIR);
        let planks = self.content.reg.block_id("base:planks").unwrap_or(stone);
        let red = self
            .content
            .reg
            .block_id("base:red_glass")
            .unwrap_or(planks);
        let blue = self
            .content
            .reg
            .block_id("base:blue_glass")
            .unwrap_or(planks);
        let amber = self
            .content
            .reg
            .block_id("base:amber_glass")
            .unwrap_or(planks);
        let water = self.content.reg.water_for_volume(8);
        let mut edits = Vec::new();

        if scene == "corner" {
            for x in 4088..=4103 {
                for z in 4088..=4103 {
                    let surface = chart.surface(x, z);
                    let floor = match surface.face() {
                        Face::PosZ => stone,
                        Face::PosX => red,
                        Face::PosY => blue,
                        _ => amber,
                    };
                    edits.push((chart.block(x, 108, z), floor));
                    for y in 109..=124 {
                        edits.push((chart.block(x, y, z), AIR));
                    }
                }
            }
            for (x, z, block) in [
                (4093, 4093, stone),
                (4098, 4093, red),
                (4093, 4098, blue),
                (4098, 4098, amber),
            ] {
                for y in 109..=115 {
                    edits.push((chart.block(x, y, z), block));
                }
            }
            self.player.pos = chart.entity(Vec3::new(4088.5, 109.0, 4088.5));
            self.camera.yaw = std::f32::consts::FRAC_PI_4;
            self.camera.pitch = -0.14;
        } else {
            for x in 4083..=4107 {
                for z in -13..=13 {
                    edits.push((chart.block(x, 108, z), stone));
                    for y in 109..=122 {
                        edits.push((chart.block(x, y, z), AIR));
                    }
                }
            }
            match scene {
                "building" => {
                    for x in 4091..=4100 {
                        for z in -6..=6 {
                            edits.push((chart.block(x, 109, z), planks));
                            for y in 110..=115 {
                                let wall = x == 4091 || x == 4100 || z == -6 || z == 6;
                                let doorway = x == 4091 && (-1..=1).contains(&z) && y <= 112;
                                if wall && !doorway {
                                    let block = if y == 112 && (z == -6 || z == 6) {
                                        if x < 4096 { blue } else { amber }
                                    } else {
                                        planks
                                    };
                                    edits.push((chart.block(x, y, z), block));
                                }
                            }
                            edits.push((chart.block(x, 116, z), planks));
                        }
                    }
                    self.player.pos = chart.entity(Vec3::new(4084.5, 110.0, 0.5));
                    self.camera.yaw = 0.0;
                    self.camera.pitch = -0.08;
                }
                "water" => {
                    for x in 4087..=4104i32 {
                        for z in -4..=4i32 {
                            if z.abs() == 4 || x == 4087 || x == 4104 {
                                edits.push((chart.block(x, 109, z), stone));
                            } else {
                                edits.push((chart.block(x, 109, z), water));
                            }
                        }
                    }
                    self.player.pos = chart.entity(Vec3::new(4084.5, 111.0, 0.5));
                    self.camera.yaw = 0.0;
                    self.camera.pitch = -0.28;
                }
                "players" => {
                    self.player.pos = chart.entity(Vec3::new(4095.5, 109.0, -5.0));
                    self.camera.yaw = std::f32::consts::FRAC_PI_2;
                    self.camera.pitch = -0.08;
                }
                other => {
                    eprintln!("unknown WILDFORGE_PLANET_SHOT={other:?}");
                    return;
                }
            }
        }

        self.runtime.local_mut().world.edit_batch(|world| {
            for (pos, block) in edits {
                world.set_block_authored_at(pos, block, "development qualification scene");
            }
        });
        self.player.vel = Vec3::ZERO;
        self.survival.spawn_point = self.player.pos;
        self.flying = true;
        self.camera.follow_planet(self.player.eye());
        eprintln!("planet qualification {scene}: staged at the PosZ east seam");
    }

    pub(in crate::game) fn qualification_ocean(&self) -> SurfacePos {
        for face in Face::ALL {
            for u in (128..crate::planet::FACE_BLOCKS).step_by(256) {
                for v in (128..crate::planet::FACE_BLOCKS).step_by(256) {
                    let center = SurfacePos::new(face, u, v).unwrap();
                    let deep = [(0, 0), (-160, 0), (160, 0), (0, -160), (0, 160)]
                        .into_iter()
                        .all(|(du, dv)| {
                            let sample = SurfacePos::canonicalized(
                                face,
                                i32::from(u) + du,
                                i32::from(v) + dv,
                            )
                            .unwrap();
                            self.runtime.local().world.generator.biome_at(sample)
                                == crate::worldgen::Biome::Ocean
                                && self.runtime.local().world.generator.surface_estimate_at(sample)
                                    < SEA_LEVEL - 4
                        });
                    if deep {
                        return center;
                    }
                }
            }
        }
        SurfacePos::new(Face::PosZ, 4096, 4096).unwrap()
    }
}
