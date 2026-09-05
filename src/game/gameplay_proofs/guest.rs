//! Native first-frame readiness and ordinary guest behavior over real QUIC.

use std::path::Path;
use std::time::{Duration, Instant};

use super::Game;
use crate::agent::Agent;
use crate::net::C2S;
use crate::tests::fixtures::TestHost;

pub(super) fn run(game: &mut Game) {
    let host = TestHost::start("native-guest-entry");
    game.config.display_name = "VIEWER".into();
    game.config.view_dist = 2;
    game.input.right_held = false;
    game.join_server(host.addr);
    let deadline = Instant::now() + Duration::from_secs(20);
    while !game.in_world {
        game.update();
        assert!(
            game.multiplayer.remote.is_some(),
            "entry failed: {}",
            game.multiplayer.join_status
        );
        assert!(Instant::now() < deadline, "graphical entry did not finish");
        std::thread::sleep(Duration::from_millis(10));
    }
    let viewer_id = game.multiplayer.remote.as_ref().unwrap().my_id;
    let center = game.player.pos.chunk().unwrap();
    assert!(game.server.world.is_remote());
    assert!(
        game.renderer.has_chunk(center),
        "entry requires a real uploaded mesh"
    );
    assert!(host.with(|session, _| session.guests[&viewer_id].is_active()));
    let mut agent =
        Agent::connect_for_test(host.addr, "PROOFAGENT").expect("agent joins the same host");
    let agent_id = agent.my_id;
    agent.send(&C2S::Chat("native guest proof".into()));
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        game.update();
        agent.pump(0.02);
        let graph_sees_agent = game
            .multiplayer
            .remote
            .as_ref()
            .unwrap()
            .players
            .contains_key(&agent_id);
        let agent_sees_graph = agent.players.contains_key(&viewer_id);
        let chat_received = game
            .presentation
            .toasts
            .iter()
            .any(|(text, _)| text.contains("native guest proof"));
        if graph_sees_agent && agent_sees_graph && chat_received {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "guests did not share their roster, positions, and chat"
        );
        std::thread::sleep(Duration::from_millis(10));
    }

    let initial = game.player.pos;
    game.input.keys.w = true;
    let walk_until = Instant::now() + Duration::from_secs(1);
    while Instant::now() < walk_until {
        game.update();
        agent.pump(0.02);
        std::thread::sleep(Duration::from_millis(10));
    }
    game.input.keys.w = false;
    let delta = initial.local_delta_to(game.player.pos);
    let walked = delta.x.hypot(delta.z);
    assert!(
        walked > 0.5,
        "ordinary graphical input did not move the guest: {walked}"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        game.update();
        agent.pump(0.02);
        let authoritative_delta =
            host.with(|session, _| initial.local_delta_to(session.guests[&viewer_id].pos));
        if authoritative_delta.x.hypot(authoritative_delta.z) > 0.5 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "host did not receive ordinary guest movement"
        );
        std::thread::sleep(Duration::from_millis(10));
    }

    let screenshot = "native-guest-entry.ppm";
    game.renderer.pending_screenshot = Some(screenshot.into());
    game.update();
    assert!(
        Path::new(screenshot)
            .metadata()
            .is_ok_and(|metadata| metadata.len() > 1000)
    );
    let report = serde_json::json!({
        "source_commit": env!("WILDFORGE_BUILD_COMMIT"),
        "source_dirty": env!("WILDFORGE_BUILD_DIRTY") == "true",
        "adapter": game.renderer.adapter_name,
        "backend": game.renderer.adapter_backend,
        "hardware": game.renderer.adapter_hardware,
        "entry_mesh_uploaded": true,
        "shared_roster_and_chat": true,
        "movement_received_by_host": true,
        "walked_blocks": walked,
        "screenshot": screenshot,
    });
    game.quit_to_title();
    assert!(!game.in_world);
    assert!(game.multiplayer.remote.is_none());
    assert!(game.gen_pool.is_none());
    assert!(game.mesh_pool.is_none());
    assert_eq!(game.renderer.chunk_count(), 0);
    let deadline = Instant::now() + Duration::from_secs(5);
    while host.with(|session, _| session.guests.contains_key(&viewer_id)) {
        assert!(
            Instant::now() < deadline,
            "host did not observe the graphical disconnect"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut report = report;
    report["normal_disconnect"] = true.into();
    std::fs::write(
        "native-guest-entry.json",
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    eprintln!("PROOF native guest: {report}");
}
