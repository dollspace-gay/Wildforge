//! Authoritative movement budgets exercised through an admitted QUIC guest.

use super::*;
use crate::net::C2S;

/// A flat, clear corridor in actual authoritative terrain.
fn movement_corridor(sim: &mut crate::server::Server) {
    let stone = sim.world.reg.block_id("base:stone").unwrap();
    for x in 5..70 {
        for z in 6..11 {
            sim.world.ensure_chunk(bp(x, 199, z).chunk());
            sim.world.set_block_at(bp(x, 199, z), stone);
            for y in 200..204 {
                sim.world.set_block_at(bp(x, y, z), AIR);
            }
        }
    }
}

fn send_move(client: &crate::net::Client, pos: crate::planet::EntityPos) {
    client.send(&C2S::Move {
        pos,
        yaw: 0.0,
        hotbar: 0,
        sprint: false,
    });
}

/// The protocol sends face-local u/v as f32. Physics integrates centered
/// coordinates with smaller magnitudes, so adding the face half-width can
/// round away low bits before serialization. Reconstruct the specified wire
/// position exactly; do not replace position equality with a broad tolerance.
fn wire_position(pos: crate::planet::EntityPos) -> crate::planet::EntityPos {
    crate::planet::EntityPos::new(pos.face(), pos.u(), pos.y(), pos.v()).unwrap()
}

/// The reliable sleep marker follows every movement on the same QUIC stream.
/// Observing it proves that the entire burst reached admission, even when its
/// final position was refused. Transport scheduling never advances sim time.
fn drain_moves(
    session: &mut crate::mp::HostSession,
    sim: &mut crate::server::Server,
    client: &mut crate::net::Client,
    id: u32,
) -> Vec<S2C> {
    session.guests.get_mut(&id).unwrap().sleeping = false;
    client.send(&C2S::SleepRequest);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let mut replies = Vec::new();
    while std::time::Instant::now() < deadline {
        session.pump(sim, None, 0.0);
        replies.extend(client.poll());
        if session.guests[&id].sleeping {
            return replies;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("the reliable marker after movement was not processed");
}

#[test]
fn burst_movement_cannot_spend_time_repeatedly() {
    let (mut session, mut sim, mut client, id) = loopback_pair("mp-movement-burst");
    movement_corridor(&mut sim);
    let start = ep(Vec3::new(8.5, 200.0, 8.5));
    session
        .guests
        .get_mut(&id)
        .unwrap()
        .prime_move_for_test(start);

    for i in 1..=100 {
        send_move(&client, ep(Vec3::new(8.5 + i as f32 * 0.5, 200.0, 8.5)));
    }
    let mut replies = drain_moves(&mut session, &mut sim, &mut client, id);
    let travelled = start.horizontal_distance_to(session.guests[&id].pos);
    assert!(
        travelled <= 3.001,
        "100 requests at zero simulation time traveled {travelled} blocks"
    );
    // The marker acknowledges upstream processing. Host replies use the
    // independent reverse stream, so allow that writer/reader to deliver the
    // correction as well, without advancing any movement time.
    let expected = wire_position(session.guests[&id].pos);
    let corrected = |messages: &[S2C]| {
        messages
            .iter()
            .any(|reply| matches!(reply, S2C::PlayerState(state) if state.pos == expected))
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while !corrected(&replies) && std::time::Instant::now() < deadline {
        session.pump(&mut sim, None, 0.0);
        replies.extend(client.poll());
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(
        corrected(&replies),
        "the refused move's authoritative correction arrives"
    );
}

/// Run the actual player physics at 50 Hz, then deliver five 20 Hz movement
/// snapshots together. This models 300 ms of delayed reliable delivery without
/// granting the host more time while it drains the queued input.
fn play_batch(
    session: &mut crate::mp::HostSession,
    sim: &mut crate::server::Server,
    client: &mut crate::net::Client,
    id: u32,
    player: &mut crate::physics::Player,
    input: &crate::physics::Input,
    flying: bool,
) {
    let mut positions = Vec::with_capacity(5);
    for step in 0..15 {
        let frame = crate::planet::local_frame(player.pos.surface_point());
        let (east, north) = frame.chart_basis(player.pos);
        if flying {
            player.fly(
                &sim.world,
                east * crate::physics::FLY_SPEED + Vec3::Y * crate::physics::FLY_VERTICAL_SPEED,
                0.02,
            );
        } else {
            player.update(&sim.world, input, east, north, 0.02);
        }
        session.pump(sim, None, 0.02);
        if step % 3 == 2 {
            positions.push(player.pos);
        }
    }
    for pos in positions {
        send_move(client, pos);
    }
    drain_moves(session, sim, client, id);
    assert_eq!(
        session.guests[&id].pos,
        wire_position(player.pos),
        "the host accepts the exact wire form of the actual physics position"
    );
}

fn movement_input() -> crate::physics::Input {
    crate::physics::Input {
        forward: 1.0,
        strafe: 0.0,
        jump: false,
        sprint: false,
        speed_mult: 1.0,
    }
}

#[test]
fn delayed_walk_sprint_jump_and_dodge_follow_actual_physics() {
    let (mut session, mut sim, mut client, id) = loopback_pair("mp-movement-controls");
    movement_corridor(&mut sim);
    let start = ep(Vec3::new(8.5, 200.0, 8.5));
    session
        .guests
        .get_mut(&id)
        .unwrap()
        .prime_move_for_test(start);
    let mut player = crate::physics::Player::new_at(start);
    let mut input = movement_input();
    let mut jumped = false;
    for batch in 0..16 {
        input.sprint = batch >= 4;
        input.jump = batch == 4;
        if batch == 8 {
            let (east, _) =
                crate::planet::local_frame(player.pos.surface_point()).chart_basis(player.pos);
            player.vel =
                east * crate::game::combat::DODGE_SPEED + Vec3::new(0.0, player.vel.y * 0.3, 0.0);
        }
        play_batch(
            &mut session,
            &mut sim,
            &mut client,
            id,
            &mut player,
            &input,
            false,
        );
        jumped |= player.pos.y() > start.y() + 0.5;
    }
    assert!(jumped, "the actual jump rose off the corridor floor");
    assert!(start.horizontal_distance_to(player.pos) > 25.0);
}

#[test]
fn creative_flight_keeps_its_horizontal_and_vertical_speed() {
    let (mut session, mut sim, mut client, id) = loopback_pair("mp-movement-flight");
    sim.world.mode = "creative".into();
    let start = ep(Vec3::new(8.5, 500.0, 8.5));
    session
        .guests
        .get_mut(&id)
        .unwrap()
        .prime_move_for_test(start);
    let mut player = crate::physics::Player::new_at(start);
    let input = movement_input();
    for _ in 0..12 {
        play_batch(
            &mut session,
            &mut sim,
            &mut client,
            id,
            &mut player,
            &input,
            true,
        );
    }
    // Horizontal validation uses surface geodesics; the camera/physics axes
    // are calibrated at the player's altitude. Convert that surface arc back
    // to its physical altitude before checking 9 blocks/s for 3.6 seconds.
    let mid_height = (start.y() + player.pos.y()) * 0.5;
    let physical_horizontal = start.horizontal_distance_to(player.pos)
        * (1.0 + f64::from(mid_height) / crate::planet::PLANET_RADIUS) as f32;
    assert!((32.0..32.8).contains(&physical_horizontal));
    assert!((28.7..28.9).contains(&(player.pos.y() - start.y())));
}

#[test]
fn terminal_falls_follow_actual_gravity_without_being_refused() {
    let (mut session, mut sim, mut client, id) = loopback_pair("mp-movement-fall");
    let start = ep(Vec3::new(8.5, 500.0, 8.5));
    session
        .guests
        .get_mut(&id)
        .unwrap()
        .prime_move_for_test(start);
    let mut player = crate::physics::Player::new_at(start);
    let mut input = movement_input();
    input.forward = 0.0;
    for _ in 0..12 {
        play_batch(
            &mut session,
            &mut sim,
            &mut client,
            id,
            &mut player,
            &input,
            false,
        );
    }
    assert_eq!(player.vel.y, -crate::physics::TERMINAL);
    assert!(start.y() - player.pos.y() > 140.0);
}

#[test]
fn stance_updates_do_not_refresh_exhausted_travel_credit() {
    let (mut session, mut sim, mut client, id) = loopback_pair("mp-movement-stance");
    movement_corridor(&mut sim);
    let start = ep(Vec3::new(8.5, 200.0, 8.5));
    session
        .guests
        .get_mut(&id)
        .unwrap()
        .prime_move_for_test(start);
    for i in 1..=100 {
        send_move(&client, ep(Vec3::new(8.5 + i as f32 * 0.5, 200.0, 8.5)));
    }
    drain_moves(&mut session, &mut sim, &mut client, id);
    let stopped = session.guests[&id].pos;
    for _ in 0..40 {
        client.send(&C2S::Move {
            pos: stopped,
            yaw: 0.8,
            hotbar: 8,
            sprint: true,
        });
    }
    let next = stopped.translated(Vec3::X * 0.5).unwrap().pos;
    send_move(&client, next);
    drain_moves(&mut session, &mut sim, &mut client, id);
    let guest = &session.guests[&id];
    assert_eq!(guest.pos, stopped, "stance packets cannot mint travel time");
    assert_eq!(guest.yaw, 0.8, "stationary aiming still works");
    assert_eq!(guest.hotbar, 8, "reliable agent hotbar anchors still work");
}

#[test]
fn refused_moves_preserve_credit_for_a_valid_request() {
    let (mut session, mut sim, mut client, id) = loopback_pair("mp-movement-refusal");
    movement_corridor(&mut sim);
    let start = ep(Vec3::new(8.5, 200.0, 8.5));
    session
        .guests
        .get_mut(&id)
        .unwrap()
        .prime_move_for_test(start);
    send_move(&client, ep(Vec3::new(1008.5, 200.0, 8.5)));
    // A colliding endpoint must also leave the pending credit intact.
    let stone = sim.world.reg.block_id("base:stone").unwrap();
    sim.world.set_block_at(bp(9, 200, 8), stone);
    send_move(&client, ep(Vec3::new(9.5, 200.0, 8.5)));
    let valid = ep(Vec3::new(6.0, 200.0, 8.5));
    send_move(&client, valid);
    drain_moves(&mut session, &mut sim, &mut client, id);
    assert_eq!(session.guests[&id].pos, valid);
}

#[test]
fn swimming_preserves_rise_but_vertical_bursts_have_one_budget() {
    let (mut session, mut sim, mut client, id) = loopback_pair("mp-movement-swim");
    let water = sim.world.reg.water_block(0);
    for x in 7..11 {
        for z in 7..11 {
            for y in 200..220 {
                sim.world.set_block_at(bp(x, y, z), water);
            }
        }
    }
    let start = ep(Vec3::new(8.5, 201.0, 8.5));
    session
        .guests
        .get_mut(&id)
        .unwrap()
        .prime_move_for_test(start);
    for i in 1..=100 {
        send_move(&client, ep(Vec3::new(8.5, 201.0 + i as f32 * 0.1, 8.5)));
    }
    drain_moves(&mut session, &mut sim, &mut client, id);
    assert!(session.guests[&id].pos.y() - start.y() <= 4.951);

    let swim_start = session.guests[&id].pos;
    let mut player = crate::physics::Player::new_at(swim_start);
    let mut input = movement_input();
    input.forward = 0.0;
    input.jump = true;
    for _ in 0..8 {
        play_batch(
            &mut session,
            &mut sim,
            &mut client,
            id,
            &mut player,
            &input,
            false,
        );
    }
    assert!(player.in_water);
    assert!(player.pos.y() - swim_start.y() > 7.0);
}

#[test]
fn equipment_skill_and_loadout_boosts_survive_and_the_ceiling_updates() {
    use crate::stats::{StatKind, StatModifier};

    let mut registry = (*base_reg()).clone();
    let boots = registry.item_id("base:leather_boots").unwrap();
    let component = registry.item_id("base:stick").unwrap();
    registry.items[boots.0 as usize].durability = 50;
    registry.items[boots.0 as usize].stats =
        vec![StatModifier::new(StatKind::MoveSpeed, 0.0, 1_250)];
    registry.items[boots.0 as usize].frame = Some(crate::equipment::FrameDef {
        slots: vec![crate::equipment::FrameSlotDef {
            slot_type: "test-speed".into(),
            max: 1,
        }],
    });
    registry.items[component.0 as usize].component = Some("test-speed".into());
    registry.items[component.0 as usize].stats =
        vec![StatModifier::new(StatKind::MoveSpeed, 0.0, 1_500)];
    registry.skills.nodes.push(crate::skills::SkillNodeDef {
        id: "test:runner".into(),
        name: "Runner".into(),
        branch: "test:movement".into(),
        tier: 1,
        cost: 1,
        description: String::new(),
        stats: vec![StatModifier::new(StatKind::MoveSpeed, 0.0, 1_500)],
    });
    registry.modes.push(crate::registry::ModeDef {
        id: "test-movement".into(),
        base: Some("survival".into()),
        creative: None,
        hunger: None,
        fall_damage: None,
        drowning: None,
        lava_burn: None,
        hostile_spawns: None,
        ire: None,
        hearts: None,
        weather_extremes: None,
        pvp: None,
        skills: Some(true),
        equipment: Some(true),
        industrial_ire: None,
        nest_spawns: None,
    });
    let registry = Arc::new(registry);
    let (mut session, mut sim, mut client, id, _) =
        loopback_pair_drained_with_reg("mp-movement-boosts", registry.clone());
    sim.world.mode = "test-movement".into();
    movement_corridor(&mut sim);
    let start = ep(Vec3::new(8.5, 200.0, 8.5));
    let guest = session.guests.get_mut(&id).unwrap();
    guest.prime_move_for_test(start);
    guest.armor[3] = Some(ItemStack::new(&registry, boots, 1));
    let mut player = crate::physics::Player::new_at(start);
    let mut input = movement_input();
    input.sprint = true;
    input.speed_mult = 1.25 * 1.5 * 1.5;
    for _ in 0..6 {
        play_batch(
            &mut session,
            &mut sim,
            &mut client,
            id,
            &mut player,
            &input,
            false,
        );
    }
    assert!(start.horizontal_distance_to(player.pos) > 30.0);

    // Removing the frame invalidates the gear ceiling. The skill alone must
    // not retain the frame/component's extra allowance.
    session.guests.get_mut(&id).unwrap().armor[3] = None;
    session.pump(&mut sim, None, 0.3);
    let before = session.guests[&id].pos;
    let target = before.translated(Vec3::X * 5.0).unwrap().pos;
    send_move(&client, target);
    drain_moves(&mut session, &mut sim, &mut client, id);
    assert_eq!(session.guests[&id].pos, before);

    // A new immutable registry can share its content hash but still have new
    // stat definitions. Cache identity follows the actual registry lifetime.
    let mut replacement = (*registry).clone();
    replacement.skills.nodes[0].stats[0].mult_permille = 3_000;
    sim.world.reg = Arc::new(replacement);
    send_move(&client, target);
    drain_moves(&mut session, &mut sim, &mut client, id);
    assert_eq!(session.guests[&id].pos, target);
}
