//! Navigation scenarios.

use super::*;

#[test]
fn the_agent_walks_and_chops() {
    let host = TestHost::start("agent-chop");
    let mut a = Agent::connect_for_test(host.addr, "SAWYER").expect("joins");
    a.pump_for(0.5);
    let player_cell = crate::agent::cell_of(a.player.pos).expect("agent occupies a world cell");
    let trunk = player_cell.offset(8, 0, 0).expect("nearby trunk cell");
    // A four-log trunk eight blocks east.
    host.with(|_, sim| {
        let log = sim.world.reg.block_id("base:log").unwrap();
        for dy in 0..4 {
            sim.world
                .set_block_at(trunk.offset(0, dy, 0).expect("trunk height"), log);
        }
    });
    a.pump_for(0.5);
    let ire_before = host.with(|_, sim| sim.world.ire);
    let report = a
        .chop_at(trunk.offset(0, 1, 0).expect("trunk target"))
        .expect("the chop succeeds");
    assert!(report.contains("felled"), "{report}");
    // The design guard, as a test: the wild keeps score on hired
    // hands too — an agent's felling charges the shared meter.
    let ire_after = host.with(|_, sim| sim.world.ire);
    assert!(
        ire_after > ire_before,
        "agent taking raises shared ire ({ire_before} -> {ire_after})"
    );
    let logs: u32 = a
        .inventory
        .slots
        .iter()
        .flatten()
        .filter(|s| a.reg.item(s.item).name.contains("log"))
        .map(|s| s.count)
        .sum();
    assert!(
        logs >= 3,
        "the host awarded the drops over the wire ({logs})"
    );
    // And the world agrees the trunk is gone.
    assert_eq!(a.world.get_block_at(trunk), AIR);
}

#[test]
fn the_agent_follows_the_leader() {
    let host = TestHost::start("agent-follow");
    let mut lead = Agent::connect_for_test(host.addr, "LEADER").expect("joins");
    let mut tail = Agent::connect_for_test(host.addr, "HEELER").expect("joins");
    lead.pump_for(0.4);
    tail.pump_for(0.4);
    let lead_id = tail
        .players
        .iter()
        .find(|(_, (n, _, _))| n.contains("LEADER"))
        .map(|(id, _)| *id)
        .expect("heeler sees the leader");
    tail.behavior = Behavior::Follow {
        id: lead_id,
        distance: 3.0,
    };
    // The leader walks a bent path; the heeler chases the trail.
    let start = crate::agent::cell_of(lead.player.pos).expect("leader occupies a world cell");
    let turn = start.offset(14, 0, 0).expect("first destination");
    lead.go_to(turn).expect("leg one plans");
    for _ in 0..900 {
        lead.pump(0.02);
        tail.pump(0.02);
        if matches!(lead.behavior, Behavior::Idle) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    lead.go_to(turn.offset(0, 0, 10).expect("second destination"))
        .expect("leg two plans");
    for _ in 0..900 {
        lead.pump(0.02);
        tail.pump(0.02);
        if matches!(lead.behavior, Behavior::Idle) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    // Let the heeler close the last stretch.
    for _ in 0..600 {
        lead.pump(0.02);
        tail.pump(0.02);
        let gap = (tail.player.pos - lead.player.pos).length();
        if gap < 5.0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let gap = (tail.player.pos - lead.player.pos).length();
    assert!(gap < 6.0, "the heeler kept the trail (gap {gap:.1})");
}

#[test]
fn the_agent_follows_past_off_center_breadcrumbs() {
    let host = TestHost::start("agent-follow-off-center");
    let mut leader = Agent::connect_for_test(host.addr, "LEADER").expect("leader joins");
    let mut follower = Agent::connect_for_test(host.addr, "FOLLOWER").expect("follower joins");
    leader.pump_for(0.3);
    follower.pump_for(0.3);
    let start = crate::agent::cell_of(follower.player.pos).unwrap();
    // Record a trail before following. Real player snapshots can lie anywhere
    // within a cell; the navigation route ends at that cell's center.
    for x in [4, 8, 14] {
        let cell = start.offset(x, 0, 0).unwrap();
        let pos = crate::planet::EntityPos::new(
            cell.face(),
            f32::from(cell.u()) + 0.8,
            f32::from(cell.y()),
            f32::from(cell.v()) + 0.8,
        )
        .unwrap();
        leader.player = Player::new_at(pos);
        host.with(|session, _| {
            session.guests.get_mut(&leader.my_id).unwrap().pos = pos;
        });
        leader.pump_for(0.2);
        assert!(pump_until(
            &mut follower,
            std::time::Duration::from_secs(5),
            |agent| agent
                .players
                .get(&leader.my_id)
                .is_some_and(|(_, seen, _)| { seen.distance_to(pos) < 0.1 })
        ));
    }
    follower.behavior = Behavior::Follow {
        id: leader.my_id,
        distance: 2.0,
    };
    for _ in 0..1_000 {
        leader.pump(0.02);
        follower.pump(0.02);
        if follower.player.pos.distance_to(leader.player.pos) < 3.0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let gap = follower.player.pos.distance_to(leader.player.pos);
    assert!(
        gap < 3.0,
        "follower stopped at an old breadcrumb: gap {gap:.2}"
    );
    follower.pump_for(0.3);
    let authoritative = host.with(|session, _| session.guests[&follower.my_id].pos);
    assert!(authoritative.distance_to(leader.player.pos) < 3.0);
}

#[test]
fn the_agent_follows_around_a_leaf_canopy() {
    let host = TestHost::start("agent-leaf-follow");
    follows_around_leaf_canopy(host, false);
}

#[test]
fn the_agent_follows_around_leaves_near_a_planet_edge() {
    let origin = crate::planet::BlockPos::new(crate::planet::Face::PosZ, 640, 200, 8180).unwrap();
    follows_around_leaf_canopy(
        TestHost::start_at("agent-leaf-edge-follow", false, origin),
        false,
    );
}

#[test]
fn the_agent_follows_through_turns_between_leaf_blocks() {
    follows_around_leaf_canopy(TestHost::start("agent-leaf-turns"), true);
}

#[test]
fn a_timed_out_walk_does_not_report_an_old_arrival() {
    let host = TestHost::start("agent-wait-result");
    let mut agent = Agent::connect_for_test(host.addr, "WALKER").expect("joins");
    agent.event("arrived at a previous destination".into());
    let goal = crate::agent::cell_of(agent.player.pos)
        .unwrap()
        .offset(8, 0, 0)
        .unwrap();
    agent.go_to(goal).expect("new walk plans");
    let result = agent.wait_idle(0.0);
    assert!(
        result.contains("timed out"),
        "a pending walk reported stale success: {result}"
    );
    assert!(!result.contains("arrived"));
}
