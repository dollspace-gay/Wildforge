//! Entry failures are observable through the production guest and real wire.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use super::{Agent, TestHost, pump_until};
use crate::net::{C2S, HostEvent, PlayerStateSnap, S2C};
use crate::planet::EntityPos;

fn paused_host(tag: &str) -> (TestHost, Agent) {
    let mut host = TestHost::start(tag);
    let agent = Agent::connect_for_test(host.addr, tag).expect("fixture guest joins");
    host.stop.store(true, Ordering::Relaxed);
    host.handle.take().unwrap().join().expect("host pump joins");
    // Simulation stops, but the ordinary owned QUIC transport stays alive.
    (host, agent)
}

fn welcome(host: &TestHost, agent: &Agent) -> EntityPos {
    host.with(|session, sim| {
        let guest = &session.guests[&agent.my_id];
        let spawn = guest.spawn;
        session.net.send(
            agent.my_id,
            &S2C::Welcome {
                seed: sim.world.seed,
                mode: sim.world.mode.clone(),
                time: sim.time_of_day,
                ire: sim.world.ire,
                palette: sim
                    .world
                    .reg
                    .blocks
                    .iter()
                    .map(|b| b.name.clone())
                    .collect(),
                items: sim.world.reg.items.iter().map(|i| i.name.clone()).collect(),
                your_id: agent.my_id,
                your_role: Default::default(),
                roster: Vec::new(),
                spawn,
                world_name: "replacement-entry".into(),
                player_state: PlayerStateSnap {
                    pos: spawn,
                    yaw: 0.0,
                    pitch: 0.0,
                    spawn,
                    health: guest.health,
                    hunger: guest.hunger,
                    nutrition: guest.nutrition,
                    hotbar: 0,
                    inventory: Vec::new(),
                    armor: Vec::new(),
                    cursor: None,
                },
            },
        );
        spawn
    })
}

fn delivered(host: &TestHost, agent: &mut Agent, marker: &str) {
    host.with(|session, _| session.net.send(agent.my_id, &S2C::Toast(marker.into())));
    let expected = format!("toast: {marker}");
    assert!(pump_until(agent, Duration::from_secs(5), |agent| {
        agent.events.contains(&expected)
    }));
}

fn ready_messages(host: &TestHost, agent: &Agent) -> usize {
    let marker = "entry-outgoing-barrier";
    agent.send(&C2S::Chat(marker.into()));
    let mut ready = 0;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let mut delivered = false;
        for event in host.with(|session, _| session.net.poll()) {
            if let HostEvent::Msg { id, msg: message } = event
                && id == agent.my_id
            {
                match message {
                    C2S::EntryReady => ready += 1,
                    C2S::Chat(text) if text == marker => delivered = true,
                    _ => {}
                }
            }
        }
        if delivered {
            return ready;
        }
        assert!(
            Instant::now() < deadline,
            "outgoing barrier was not delivered"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn malformed_entry_chunks_do_not_acknowledge_terrain_or_accept_entry() {
    let (host, mut agent) = paused_host("entry-malformed");
    let spawn = welcome(&host, &agent);
    let center = spawn.chunk().unwrap();
    host.with(|session, _| {
        session.net.send(
            agent.my_id,
            &S2C::EntryManifest {
                spawn,
                required: vec![center],
            },
        );
        session.net.send(
            agent.my_id,
            &S2C::Chunk {
                face: center.face() as u8,
                u: center.u(),
                v: center.v(),
                rle: b"WFC9".to_vec(),
            },
        );
    });
    delivered(&host, &mut agent, "malformed-terrain-delivered");
    assert!(!agent.world.has_chunk(center));
    assert!(!agent.in_world);
    assert_eq!(ready_messages(&host, &agent), 0);
    host.with(|session, _| session.net.send(agent.my_id, &S2C::EntryAccepted));
    delivered(&host, &mut agent, "premature-acceptance-delivered");
    assert!(!agent.in_world);
    assert!(
        agent
            .events
            .iter()
            .any(|event| event.starts_with("refused: "))
    );
}

#[test]
fn a_new_welcome_cannot_reuse_acknowledgement_from_the_previous_world() {
    let (host, mut agent) = paused_host("entry-replaced");
    welcome(&host, &agent);
    host.with(|session, _| session.net.send(agent.my_id, &S2C::EntryAccepted));
    delivered(&host, &mut agent, "replacement-acceptance-delivered");
    assert!(!agent.in_world);
    assert_eq!(ready_messages(&host, &agent), 0);
    assert!(
        agent
            .events
            .iter()
            .any(|event| event.starts_with("refused: "))
    );
}

#[test]
fn disconnect_during_preparation_is_reported_once() {
    let (host, mut agent) = paused_host("entry-disconnect");
    welcome(&host, &agent);
    delivered(&host, &mut agent, "preparation-started");
    assert!(!agent.in_world);
    host.with(|session, _| session.net.kick(agent.my_id));
    let refusal = "refused: disconnected during world preparation";
    assert!(pump_until(&mut agent, Duration::from_secs(5), |agent| {
        agent.events.iter().any(|event| event == refusal)
    }));
    for _ in 0..3 {
        agent.pump(0.02);
    }
    assert_eq!(
        agent
            .events
            .iter()
            .filter(|event| *event == refusal)
            .count(),
        1
    );
}
