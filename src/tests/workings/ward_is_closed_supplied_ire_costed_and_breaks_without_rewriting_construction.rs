//! Ward is closed supplied ire costed and breaks without rewriting construction scenarios.

use super::*;

#[test]
fn ward_is_closed_supplied_ire_costed_and_breaks_without_rewriting_construction() {
    let (mut world, controller, first_pos, _, _) = ritual_world("workings-ward");
    let source_vessel = vessel_stack_at(&world, first_pos);
    charge_fixture_vessel(&mut world, source_vessel.arcane_id, 768);
    let conductor = b(&world.reg, "base:arcane_conductor");
    let mut boundary = Vec::new();
    for du in -3i32..=3 {
        for dv in -3i32..=3 {
            if du.abs().max(dv.abs()) == 3 {
                let pos = controller.offset(du, 0, dv).unwrap();
                world.set_block_authored_at(pos, conductor, "ward fixture boundary");
                boundary.push(pos);
            }
        }
    }
    world.add_ire(80.0);
    let ire_before = world.ire;
    let current_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let started = world
        .begin_ward_boundary_ritual([32; 16], "ward-worker", controller)
        .unwrap();
    settle_channel(&mut world, started.stable_id);
    assert!(world.resist_supernatural_pressure_at(controller, "warden", 1));
    let transaction = &world.workings_state.as_ref().unwrap().active[&started.stable_id];
    assert!(matches!(
        transaction.effect,
        crate::workings::WorkingEffect::Ward {
            pressure_units: 5,
            ire_before_millipoints: 80_000,
            ire_after_millipoints: 80_000,
            ..
        }
    ));
    assert_eq!(world.ire, ire_before, "a ward never lowers stored Ire");

    let inside = controller.offset(0, 2, 0).unwrap();
    world.set_block_at(inside, AIR);
    world.spawn_projectile(crate::mobs::Projectile {
        stable_id: 88_001,
        pos: inside.entity_center(),
        vel: glam::Vec3::ZERO,
        tile: 0,
        damage: 2.0,
        damage_type: None,
        age: 0.0,
        from_player: false,
        drop_item: None,
        preparation_payload: None,
        owner: 0,
    });
    let players = [crate::server::PlayerCtx {
        id: 7,
        pos: inside.entity_center(),
        spawn: inside.entity_center(),
        attackable: true,
        aggro_mod: 0.0,
        quiet_charm: None,
    }];
    assert!(world.tick_projectiles(&players, 0.05).is_empty());
    assert!(world.projectiles().is_empty());
    let projectile_pressure =
        match &world.workings_state.as_ref().unwrap().active[&started.stable_id].effect {
            crate::workings::WorkingEffect::Ward {
                pressure_kind,
                pressure_units,
                ..
            } => {
                assert_eq!(pressure_kind, "projectile");
                *pressure_units
            }
            _ => unreachable!(),
        };
    assert!(projectile_pressure > 5);
    world.spawn_projectile(crate::mobs::Projectile {
        stable_id: 88_002,
        pos: inside.entity_center(),
        vel: glam::Vec3::ZERO,
        tile: 0,
        damage: 2.0,
        damage_type: None,
        age: 0.0,
        from_player: true,
        drop_item: None,
        preparation_payload: None,
        owner: 7,
    });
    assert!(world.tick_projectiles(&players, 0.05).is_empty());
    assert_eq!(
        world.projectiles().len(),
        1,
        "a ward may not stop an ordinary player's projectile"
    );
    world.replace_projectiles(Vec::new());

    // Real planetary wake and dross state applies pressure once the next
    // authoritative Current transport step lands. Neither environmental
    // reservoir is consumed by the ward.
    let region = world
        .planet_atlas()
        .unwrap()
        .atlas_pos(controller.surface());
    let geography = world.arcane_geography.as_mut().unwrap();
    let geographic_total_before = geography.audit().unwrap().accounted_total;
    assert!(geography.foul_ambient(region, 128).unwrap() > 0);
    geography.begin_wake(vec![region], 64, 64, 1).unwrap();
    let next_step = geography.dynamic.completed_steps + 1;
    world.set_simulation_clock(
        (next_step * crate::arcane_geography::ARCANE_GEOGRAPHY_SIMULATION_SECONDS) as f64,
    );
    for _ in 0..8 {
        world.tick_arcane_geography(4_096).unwrap();
        if world
            .arcane_geography
            .as_ref()
            .unwrap()
            .dynamic
            .completed_steps
            >= next_step
        {
            break;
        }
    }
    let transaction = &world.workings_state.as_ref().unwrap().active[&started.stable_id];
    assert!(matches!(
        transaction.effect,
        crate::workings::WorkingEffect::Ward {
            ref pressure_kind,
            pressure_units,
            ..
        } if pressure_kind == "dross" && pressure_units > projectile_pressure
    ));
    assert_eq!(
        world
            .arcane_geography
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .accounted_total,
        geographic_total_before
    );
    assert_eq!(world.ire, ire_before);

    let broken = boundary[0];
    let untouched = boundary[1];
    world.set_block_at(broken, AIR);
    assert!(!world.resist_supernatural_pressure_at(controller, "warden", 1));
    let outcomes = world.tick_workings();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].0.cue, crate::workings::WorkingCueKind::Strain);
    assert_eq!(world.get_block_at(broken), AIR);
    assert_eq!(world.get_block_at(untouched), conductor);
    assert_eq!(world.ire, ire_before);
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        current_before
    );
}
