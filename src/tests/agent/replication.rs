//! Agent perception receives complete authoritative entity snapshots.

use std::time::Duration;

use super::admission::paused_host;
use super::pump_until;
use crate::net::{MobSnap, S2C, Snapshot};

#[test]
fn agent_mobs_retain_host_health_and_hurt_state() {
    let (host, mut agent) = paused_host("agent-mob-state");
    let snapshot = MobSnap {
        id: 847,
        species: 0,
        pos: agent.player.pos,
        yaw: 0.25,
        growth: 0.6,
        hurt: 0.4,
        health: 7.25,
        fed: true,
    };
    host.with(|session, _| {
        session.net.send(
            agent.my_id,
            &S2C::Mobs(Snapshot::whole(1_000_000, vec![snapshot.clone()])),
        );
    });
    assert!(pump_until(&mut agent, Duration::from_secs(5), |agent| {
        agent.world.mobs().iter().any(|mob| mob.id == snapshot.id)
    }));
    let mob = agent
        .world
        .mobs()
        .iter()
        .find(|mob| mob.id == snapshot.id)
        .unwrap();
    assert_eq!(mob.pos, snapshot.pos);
    assert_eq!(mob.yaw, snapshot.yaw);
    assert_eq!(mob.growth, snapshot.growth);
    assert_eq!(mob.fed, snapshot.fed);
    assert_eq!(mob.health, snapshot.health);
    assert_eq!(mob.hurt_flash, snapshot.hurt);
}
