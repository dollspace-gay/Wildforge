//! The agent is a guest, not a god: loopback coverage for the
//! headless client, its perception, locomotion, follow, and work.

use super::fixtures::TestHost;
use super::*;
use crate::agent::{Agent, Behavior};
use crate::world::TerrainRead;

mod admission;
mod replication;

/// Integration hosts run on their own thread. Under the full parallel suite,
/// a fixed number of client pumps says nothing about how often that host was
/// actually scheduled. Wait on the authoritative condition with a generous
/// wall-clock ceiling so tests fail for missing behavior, not CPU contention.
fn pump_until(
    agent: &mut Agent,
    timeout: std::time::Duration,
    mut condition: impl FnMut(&Agent) -> bool,
) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        agent.pump(0.02);
        if condition(agent) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn follows_around_leaf_canopy(host: TestHost, passage: bool) {
    let mut leader = Agent::connect_for_test(host.addr, "LEADER").expect("leader joins");
    let mut follower = Agent::connect_for_test(host.addr, "FOLLOWER").expect("follower joins");
    leader.pump_for(0.3);
    follower.pump_for(0.3);
    let start = crate::agent::cell_of(follower.player.pos).unwrap();
    let obstruction = start.offset(3, 0, 0).unwrap();
    host.with(|_, sim| {
        let leaves = sim.world.reg.block_id("base:acacia_leaves").unwrap();
        if passage {
            let corridor = [
                (0, 0),
                (1, 0),
                (2, 0),
                (3, 0),
                (3, 1),
                (3, 2),
                (3, 3),
                (4, 3),
                (5, 3),
                (6, 3),
                (6, 2),
                (6, 1),
                (6, 0),
                (7, 0),
                (8, 0),
            ];
            sim.world.edit_fixture_for_test(|world| {
                for x in -3..=19 {
                    for z in -3..=19 {
                        if !corridor.contains(&(x, z)) {
                            for y in 0..3 {
                                world.set_block_at(start.offset(x, y, z).unwrap(), leaves);
                            }
                        }
                    }
                }
            });
        } else {
            for dz in -2..=2 {
                for dy in 0..3 {
                    sim.world
                        .set_block_at(obstruction.offset(0, dy, dz).unwrap(), leaves);
                }
            }
        }
    });
    let obstruction = if passage {
        start.offset(4, 0, 0).unwrap()
    } else {
        obstruction
    };
    assert!(pump_until(
        &mut follower,
        std::time::Duration::from_secs(10),
        |agent| {
            agent.world.get_block_at(obstruction)
                == agent.reg.block_id("base:acacia_leaves").unwrap()
        }
    ));
    // A player already on the other side is a normal way to begin following;
    // there is no pre-recorded route around this intact leaf canopy.
    leader.player = Player::new_at(start.offset(8, 0, 0).unwrap().entity_at_height(0.0));
    host.with(|session, _| {
        session.guests.get_mut(&leader.my_id).unwrap().pos = leader.player.pos;
    });
    leader.pump_for(0.3);
    assert!(
        pump_until(&mut follower, std::time::Duration::from_secs(10), |agent| {
            agent
                .players
                .get(&leader.my_id)
                .is_some_and(|(_, pos, _)| pos.distance_to(leader.player.pos) < 0.5)
        }),
        "leader id {} at {:?}, host {:?}, follower sees {:?}",
        leader.my_id,
        leader.player.pos,
        host.with(|session, _| session.guests.get(&leader.my_id).map(|guest| guest.pos)),
        follower.players
    );
    follower.behavior = Behavior::Follow {
        id: leader.my_id,
        distance: 2.0,
    };
    let mut furthest_side_step = 0.0f32;
    let mut pinned_seconds = 0.0f32;
    let mut longest_pin = 0.0f32;
    for _ in 0..1_500 {
        leader.pump(0.02);
        let before = follower.player.pos;
        follower.pump(0.02);
        if follower.player.pushed_wall && before.horizontal_distance_to(follower.player.pos) < 0.02
        {
            pinned_seconds += 0.02;
            longest_pin = longest_pin.max(pinned_seconds);
        } else {
            pinned_seconds = 0.0;
        }
        let delta = start.entity_center().local_delta_to(follower.player.pos);
        furthest_side_step = furthest_side_step.max(delta.z.abs());
        if follower.player.pos.distance_to(leader.player.pos) < 3.0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let gap = follower.player.pos.distance_to(leader.player.pos);
    eprintln!(
        "leaf follow: gap {gap:.2}, maximum time pinned {longest_pin:.2}s, side step {furthest_side_step:.2}"
    );
    assert!(
        gap < 3.0,
        "follower stayed against intact leaves: gap {gap:.2}, side step {furthest_side_step:.2}, pos {:?}, events {:?}",
        follower.player.pos,
        follower.events
    );
    assert!(
        furthest_side_step > 2.5,
        "the follower must walk around the canopy"
    );
    assert!(
        longest_pin < 0.5,
        "follow blindly pushed against known leaves for {longest_pin:.2} seconds before routing around them"
    );
    follower.pump_for(0.3);
    host.with(|_, sim| {
        assert_eq!(
            sim.world.get_block_at(obstruction),
            sim.world.reg.block_id("base:acacia_leaves").unwrap()
        );
    });
    let authoritative = host.with(|session, _| session.guests[&follower.my_id].pos);
    assert!(
        authoritative.distance_to(leader.player.pos) < 3.0,
        "the host still sees the follower stuck: host {authoritative:?}, client {:?}",
        follower.player.pos
    );
}

mod alchemy;
mod dross;
mod entry;
mod implements;
mod inventory;
mod navigation;
mod society;
mod workings;
