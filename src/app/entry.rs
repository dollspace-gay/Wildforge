//! Offline spawn preparation followed by durable water/material qualification.

use crate::world::World;
use crate::{materials, planet_atlas, registry};
use std::{path::PathBuf, sync::Arc};

pub(super) fn validate(args: &[String], i: usize) {
    let Some(world_arg) = args.get(i + 1).map(PathBuf::from) else {
        eprintln!("usage: wildforge --validate-entry <world>");
        std::process::exit(2);
    };
    let world_path = if world_arg.components().count() == 1 {
        PathBuf::from("saves").join(world_arg)
    } else {
        world_arg
    };
    let reg = match registry::load_validated(std::path::Path::new("mods")) {
        Ok(registry) => Arc::new(registry),
        Err(error) => {
            eprintln!("entry validation failed: {error}");
            std::process::exit(1);
        }
    };
    let mut world = match World::load_or_create(world_path.clone(), reg) {
        Ok(world) => world,
        Err(error) => {
            eprintln!("entry validation failed: {error}");
            std::process::exit(1);
        }
    };
    let spawn = match world.prepare_common_spawn(|stage, completed, total| {
        eprintln!("entry: {stage} {completed}/{total}");
    }) {
        Ok(spawn) => spawn,
        Err(error) => {
            eprintln!("entry validation failed: {error}");
            std::process::exit(1);
        }
    };
    let save = world.save_modified();
    if !save.is_ok() {
        eprintln!("entry validation save failed: {}", save.summary());
        std::process::exit(1);
    }
    let atlas = match planet_atlas::PlanetAtlas::load(&world_path) {
        Ok(atlas) => atlas,
        Err(error) => {
            eprintln!("entry validation atlas reload failed: {error}");
            std::process::exit(1);
        }
    };
    let water = atlas.water_audit();
    let material = match materials::audit_world(&world_path) {
        Ok(audit) => audit,
        Err(error) => {
            eprintln!("entry validation material audit failed: {error}");
            std::process::exit(1);
        }
    };
    println!(
        "entry qualified at {} {:.1},{:.1},{:.1}; {}",
        spawn.face().name(),
        spawn.u(),
        spawn.y,
        spawn.v(),
        save.summary()
    );
    println!(
        "water delta: {} HU; salt delta: {}; material: {}",
        water.unexplained_water_delta_hu,
        water.unexplained_salt_delta,
        if material.is_balanced() && material.is_qualified() {
            "balanced and qualified"
        } else {
            "FAILED"
        }
    );
    if water.unexplained_water_delta_hu != 0
        || water.unexplained_salt_delta != 0
        || !material.is_balanced()
        || !material.is_qualified()
    {
        std::process::exit(1);
    }
}
