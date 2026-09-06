//! Exposure scenarios.

use super::*;

#[test]
fn scar_burden_stalls_cultivation_without_deleting_block_or_metadata() {
    let (mut world, _) = dross_world("dross-crop-stall", 0xd205_5202);
    let region = AtlasPos {
        face: Face::PosZ,
        u: 8,
        v: 8,
    };
    let surface = surface_atlas_center(&world, region);
    world.ensure_chunk(ChunkPos::from_surface(surface));
    let soil = crate::planet::BlockPos::new(
        surface.face(),
        surface.u(),
        (world.surface_height_at(surface) + 1) as u8,
        surface.v(),
    )
    .unwrap();
    let crop = soil.offset(0, 1, 0).unwrap();
    world.set_block_at(soil, b(&world.reg, "base:farmland"));
    let wheat = b(&world.reg, "base:wheat_seeds");
    world.set_block_meta_at(crop, wheat, 117);
    let index = region.index(world.planet_atlas().unwrap().side());
    world
        .arcane_geography
        .as_mut()
        .unwrap()
        .dynamic
        .dross_state
        .cells[index]
        .band = DrossBand::Scar;
    let mut rng = 7;
    for _ in 0..1_000 {
        world.random_tick(&mut rng);
    }
    assert_eq!(world.get_block_at(crop), wheat);
    assert_eq!(world.get_meta_at(crop), 117);
}

#[test]
fn environmental_exposure_is_bounded_reversible_and_does_not_change_ire() {
    let (mut world, _) = dross_world("dross-reversible-exposure", 0xd205_5203);
    let polluted = AtlasPos {
        face: Face::PosZ,
        u: 8,
        v: 8,
    };
    let clean = polluted
        .step(
            crate::planet::Direction4::East,
            world.planet_atlas().unwrap().side(),
        )
        .pos;
    let side = world.planet_atlas().unwrap().side();
    world
        .arcane_geography
        .as_mut()
        .unwrap()
        .dynamic
        .dross_state
        .cells[polluted.index(side)]
    .band = DrossBand::Scar;
    let polluted_surface = surface_atlas_center(&world, polluted);
    let clean_surface = surface_atlas_center(&world, clean);
    let polluted_pos = crate::planet::BlockPos::new(
        polluted_surface.face(),
        polluted_surface.u(),
        100,
        polluted_surface.v(),
    )
    .unwrap();
    let clean_pos = crate::planet::BlockPos::new(
        clean_surface.face(),
        clean_surface.u(),
        100,
        clean_surface.v(),
    )
    .unwrap();
    let actor = [42; 16];
    let ire = world.ire;
    let initial = PreparationPhysiology {
        health: 20.0,
        max_health: 20.0,
        hunger: 20.0,
        nutrition: [1.0; 5],
        strain: 0.0,
        bodily_dross: 0,
    };
    let _ = world
        .tick_preparation_statuses(actor, polluted_pos, initial)
        .unwrap();
    world.set_simulation_clock(world.clock() + 20.0);
    let exposed = world
        .tick_preparation_statuses(actor, polluted_pos, initial)
        .unwrap();
    assert!(exposed.physiology.bodily_dross > 0);
    assert_eq!(exposed.modifiers.dross_band, DrossBand::Scar.ordinal());
    assert!(exposed.modifiers.recovery_permille < 1_000);
    assert!(exposed.modifiers.perception_permille < 1_000);
    assert!(exposed.modifiers.stamina_permille < 1_000);
    assert_eq!(world.ire, ire);

    world.set_simulation_clock(world.clock() + 120.0);
    let recovered = world
        .tick_preparation_statuses(actor, clean_pos, exposed.physiology)
        .unwrap();
    assert_eq!(recovered.physiology.bodily_dross, 0);
    assert_eq!(recovered.modifiers, PreparationModifiers::default());
    assert_eq!(world.ire, ire);
}
