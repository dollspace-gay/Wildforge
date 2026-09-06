//! The belly: hunger drives grazing, raids, dung, herds, compost.

use super::*;
use crate::world::soil;

/// A flat grass pad centered near the origin, cleared overhead.
fn pad(w: &mut World, reg: &Registry, x0: i32, x1: i32, z0: i32, z1: i32, h: i32) {
    let grass = b(reg, "base:grass");
    for x in x0..=x1 {
        for z in z0..=z1 {
            w.set_block(x, h, z, grass);
            for dy in 1..5 {
                if w.get_block(x, h + dy, z) != AIR {
                    w.set_block(x, h + dy, z, AIR);
                }
            }
        }
    }
}

fn hungry_deer(reg: &Registry, at: glam::Vec3) -> crate::mobs::Mob {
    let si = reg.animal_id("base:deer").expect("deer exists");
    let mut m = crate::mobs::Mob::new(si, at, 0.0);
    m.health = reg.animals[si].health;
    m.belly = -1.0;
    m
}

fn ctx(pos: glam::Vec3) -> crate::server::PlayerCtx {
    crate::server::PlayerCtx {
        id: 0,
        pos: ep(pos),
        spawn: ep(glam::Vec3::ZERO),
        attackable: true,
        aggro_mod: 0.0,
        quiet_charm: None,
    }
}

fn beast(reg: &Registry, name: &str, at: glam::Vec3) -> crate::mobs::Mob {
    let si = reg.animal_id(name).expect("species exists");
    let mut m = crate::mobs::Mob::new(si, at, 0.0);
    m.health = reg.animals[si].health;
    m
}

fn wolf_hunt_fixture(tag: &str, belly: f32) -> (World, crate::server::PlayerCtx) {
    let reg = base_reg();
    let mut world = World::new(42, tmp_dir(tag), reg.clone());
    world.insert_empty_chunks_for_test([tchunk(0, 0)]);
    pad(&mut world, &reg, 0, 15, 0, 15, 100);
    world.set_calendar_day(3 * crate::world::SEASON_DAYS);
    let mut wolf = beast(&reg, "base:wolf", Vec3::new(6.5, 101.0, 8.5));
    wolf.belly = belly;
    world.spawn_mob(wolf);
    (world, ctx(Vec3::new(7.0, 101.0, 8.5)))
}

mod aquatic;
mod disturbance;
mod fliers;
mod grazing;
mod habitats;
mod hunt_cancellation;
mod predators;
mod soil_cycles;
