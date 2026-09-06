//! Nudge scenarios.

use super::*;

#[test]
fn nudge_uses_one_locked_projectile_and_ordinary_velocity_without_teleporting() {
    let (mut world, source, wand) = workings_world("workings-nudge");
    let current_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let wand_source = source.offset(0, 2, 0).unwrap();
    world.set_block_at(wand_source, AIR);
    let projectile_pos = wand_source
        .entity_center()
        .translated(glam::Vec3::new(2.0, 0.25, 0.0))
        .unwrap()
        .pos;
    world.spawn_projectile(crate::mobs::Projectile {
        stable_id: 777,
        pos: projectile_pos,
        vel: glam::Vec3::new(0.0, 0.0, 1.0),
        tile: 0,
        damage: 1.0,
        damage_type: None,
        age: 0.0,
        from_player: true,
        drop_item: None,
        preparation_payload: None,
        owner: 0,
    });
    let position_before = world.projectiles()[0].pos;
    let velocity_before = world.projectiles()[0].vel;
    let started = world
        .begin_nudge_working(
            [23; 16],
            "nudge-worker",
            wand_source,
            wand.arcane_id,
            777,
            2,
            false,
        )
        .unwrap();
    assert!(
        world
            .begin_nudge_working(
                [24; 16],
                "competing-worker",
                wand_source,
                wand.arcane_id,
                777,
                1,
                false,
            )
            .unwrap_err()
            .contains("already reserved")
    );
    world.complete_working(started.stable_id).unwrap();
    let projectile = &world.projectiles()[0];
    assert_eq!(
        projectile.pos, position_before,
        "Nudge is not teleportation"
    );
    assert_ne!(projectile.vel, velocity_before);
    assert!(projectile.vel.length() <= 40.0);
    assert_eq!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .unexplained_delta,
        0
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        current_before
    );
}

#[test]
fn nudge_operates_one_real_firebox_latch_and_machine_simulation_obeys_it() {
    let (mut world, source, wand) = workings_world("workings-nudge-mechanism");
    let firebox = source.offset(2, 0, 0).unwrap();
    let boiler = firebox.offset(0, 1, 0).unwrap();
    assert!(world.place_block_at(firebox, b(&world.reg, "base:firebox")));
    assert!(world.place_block_at(boiler, b(&world.reg, "base:boiler")));
    let crate::world::BlockEntity::Steam(steam) = world.block_entity_mut_at(&firebox).unwrap()
    else {
        panic!("placed firebox did not receive ordinary machine authority");
    };
    steam.fuel = 20.0;
    steam.water.water_hu = crate::planet_atlas::HYDRO_UNITS_PER_BLOCK;

    let closed = world
        .begin_nudge_mechanism_working(
            [33; 16],
            "nudge-latch",
            source,
            wand.arcane_id,
            firebox,
            false,
        )
        .unwrap();
    world.complete_working(closed.stable_id).unwrap();
    assert!(matches!(
        world.block_entity_at(&firebox),
        Some(crate::world::BlockEntity::Steam(steam)) if steam.draft_closed
    ));
    let fuel_before = match world.block_entity_at(&firebox).unwrap() {
        crate::world::BlockEntity::Steam(steam) => steam.fuel,
        _ => unreachable!(),
    };
    world.tick_entities(1.0);
    let fuel_closed = match world.block_entity_at(&firebox).unwrap() {
        crate::world::BlockEntity::Steam(steam) => steam.fuel,
        _ => unreachable!(),
    };
    assert_eq!(
        fuel_closed, fuel_before,
        "a closed physical draft must bank fuel"
    );
    assert!(
        matches!(
            world.block_entity_at(&firebox),
            Some(crate::world::BlockEntity::Steam(steam)) if steam.draft_closed
        ),
        "the ordinary lit/unlit visual swap must preserve the latch"
    );

    let dir = world.save_dir_for_saving();
    save_world(&mut world);
    drop(world);
    let mut world = World::load_or_create(dir, base_reg()).unwrap();
    world.ensure_chunk(firebox.chunk());
    assert!(
        matches!(
            world.block_entity_at(&firebox),
            Some(crate::world::BlockEntity::Steam(steam)) if steam.draft_closed
        ),
        "the embodied draft latch must survive a process restart"
    );

    let opened = world
        .begin_nudge_mechanism_working(
            [33; 16],
            "nudge-latch",
            source,
            wand.arcane_id,
            firebox,
            false,
        )
        .unwrap();
    world.complete_working(opened.stable_id).unwrap();
    assert!(matches!(
        world.block_entity_at(&firebox),
        Some(crate::world::BlockEntity::Steam(steam)) if !steam.draft_closed
    ));
    world.tick_entities(1.0);
    let fuel_open = match world.block_entity_at(&firebox).unwrap() {
        crate::world::BlockEntity::Steam(steam) => steam.fuel,
        _ => unreachable!(),
    };
    assert!(
        fuel_open < fuel_closed,
        "opening the draft must resume ordinary steam work"
    );
}

#[test]
fn nudge_targets_one_host_owned_dropped_stack_through_ordinary_physics() {
    let (mut world, source, wand) = workings_world("workings-nudge-dropped-item");
    let pos = source
        .entity_center()
        .translated(glam::Vec3::new(2.0, 0.5, 0.0))
        .unwrap()
        .pos;
    let drop = crate::entity::ItemEntity::new(
        pos,
        glam::Vec3::new(0.0, 1.0, 0.25),
        it(&world.reg, "base:stone"),
        64,
    );
    let drop_id = world.spawn_loose_item(drop);
    assert_ne!(drop_id, 0);

    let started = world
        .begin_nudge_working(
            [34; 16],
            "nudge-drop",
            source,
            wand.arcane_id,
            drop_id,
            1,
            false,
        )
        .unwrap();
    let reserved = &world.workings_state.as_ref().unwrap().active[&started.stable_id];
    assert!(matches!(
        reserved.effect,
        crate::workings::WorkingEffect::Impulse {
            entity_kind: crate::workings::NudgeEntityKind::DroppedItem,
            ..
        }
    ));
    let distance = match reserved.effect {
        crate::workings::WorkingEffect::Impulse { target, .. } => source
            .entity_center()
            .local_delta_to(target.entity_center())
            .length()
            .ceil() as u16,
        _ => unreachable!(),
    };
    assert_eq!(
        reserved.definition.quote(4, distance, 0).unwrap().charge,
        reserved.reserved_current.total(),
        "a full stack must pay the declared highest bounded mass band"
    );

    // The target is not frozen while the wand settles. Release refreshes the
    // exact write-ahead velocity, then applies only an impulse.
    world.tick_entities(0.1);
    let before_release = world
        .loose_items()
        .iter()
        .find(|item| item.stable_id == drop_id)
        .unwrap()
        .clone();
    settle_channel(&mut world, started.stable_id);
    world.release_working(started.stable_id).unwrap();
    let after = world
        .loose_items()
        .iter()
        .find(|item| item.stable_id == drop_id)
        .unwrap();
    assert_eq!(
        after.pos, before_release.pos,
        "Nudge may not teleport a drop"
    );
    assert_ne!(after.vel, before_release.vel);
    assert!(after.vel.length() <= 40.0);
    assert_eq!(after.item, before_release.item);
    assert_eq!(after.count, 64);
}

#[test]
fn dropped_item_nudge_crash_replays_the_host_entity_exactly_once() {
    let (mut world, dir, source, wand) =
        persistent_workings_world("workings-nudge-drop-crash-replay");
    let drop_pos = source
        .entity_center()
        .translated(glam::Vec3::new(2.0, 1.0, 0.0))
        .unwrap()
        .pos;
    let item = it(&world.reg, "base:stone");
    let stable_id = world.spawn_loose_item(crate::entity::ItemEntity::new(
        drop_pos,
        glam::Vec3::ZERO,
        item,
        8,
    ));
    save_world(&mut world);
    let total_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let started = world
        .begin_nudge_working(
            [37; 16],
            "drop-crash-worker",
            source,
            wand.arcane_id,
            stable_id,
            3,
            false,
        )
        .unwrap();
    world.fail_loose_item_save_for_test(true);
    let error = world.complete_working(started.stable_id).unwrap_err();
    assert!(
        error.contains("injected loose-item sidecar save failure"),
        "unexpected completion error: {error}"
    );
    assert_eq!(
        world.workings_state.as_ref().unwrap().active[&started.stable_id].phase,
        crate::workings::WorkingPhase::PendingApply
    );
    let applied_velocity = world
        .loose_items()
        .iter()
        .find(|drop| drop.stable_id == stable_id)
        .unwrap()
        .vel;
    assert!(applied_velocity.x > 0.0);
    drop(world);

    let reloaded = World::load_or_create(dir, base_reg()).unwrap();
    let replayed = reloaded
        .loose_items()
        .iter()
        .find(|drop| drop.stable_id == stable_id)
        .unwrap();
    assert_eq!(replayed.vel, applied_velocity);
    assert!(
        !reloaded
            .workings_state
            .as_ref()
            .unwrap()
            .active
            .contains_key(&started.stable_id)
    );
    let matching = reloaded
        .workings_state
        .as_ref()
        .unwrap()
        .history
        .iter()
        .filter(|event| event.id == started.stable_id)
        .collect::<Vec<_>>();
    assert_eq!(matching.len(), 1);
    assert_eq!(matching[0].outcome, "crash_replay_complete");
    assert_eq!(
        reloaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        total_before
    );
}
