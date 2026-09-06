//! Entry scenarios.

use super::*;

#[test]
fn the_agent_joins_speaks_and_sees() {
    let host = TestHost::start("agent-join");
    let mut a = Agent::connect_for_test(host.addr, "SCOUT").expect("scout joins");
    let mut b = Agent::connect_for_test(host.addr, "ECHO").expect("echo joins");
    assert!(a.in_world && b.in_world, "both admitted as ordinary guests");
    // Let physics land everyone and the first snapshots arrive.
    for _ in 0..75 {
        a.pump(0.02);
        b.pump(0.02);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    // Chat crosses the wire like any player's.
    a.send(&crate::net::C2S::Chat("three logs, coming up".into()));
    for _ in 0..100 {
        a.pump(0.02);
        b.pump(0.02);
        if b.events.iter().any(|e| e.contains("three logs")) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        b.events
            .iter()
            .any(|e| e.contains("SCOUT") && e.contains("three logs")),
        "chat reached the other guest"
    );
    // Perception reads only the streamed mirror.
    let look = a.look_around();
    assert!(look.contains("pos ") && look.contains('@'), "digest + map");
    assert!(look.contains("ECHO"), "sees the other player: {look}");
    // A log placed by the host turns up in nearest().
    let spot = crate::agent::cell_of(a.player.pos).expect("agent occupies a world cell");
    host.with(|_, sim| {
        let log = sim.world.reg.block_id("base:log").unwrap();
        sim.world
            .set_block_at(spot.offset(3, 0, 0).expect("nearby test cell"), log);
    });
    for _ in 0..50 {
        a.pump(0.02);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let found = a.nearest("#base:logs", 16);
    assert!(found.contains("base:log"), "nearest finds it: {found}");
}
