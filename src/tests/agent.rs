//! The agent is a guest, not a god: loopback coverage for the
//! headless client, its perception, locomotion, follow, and work.

use super::*;
use crate::agent::{Agent, Behavior};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// A host running on a thread, shared so tests can reach into the
/// authoritative world between pumps.
struct TestHost {
    shared: Arc<Mutex<(crate::mp::HostSession, crate::server::Server)>>,
    stop: Arc<AtomicBool>,
    addr: std::net::SocketAddr,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl TestHost {
    fn start(world_tag: &str) -> TestHost {
        let reg = base_reg();
        // These are protocol/behavior tests over a deliberately hand-built
        // stage. Generating 25 full terrain chunks here previously dominated
        // the entire suite without exercising any agent behavior.
        let mut world = World::new(42, tmp_dir(world_tag), reg);
        world.insert_empty_chunks_for_test(
            (-2..=2).flat_map(|x| (-2..=2).map(move |z| tchunk(x, z))),
        );
        let mut sim = crate::server::Server::new(world, 0.3, 5);
        let mut sess = crate::mp::HostSession::start_on(world_tag.into(), 0).unwrap();
        // Arrivals land on a stage this harness owns, well above any
        // terrain: these tests exercise the wire, not the landscape,
        // and a spawn that follows the world's shoreline makes them
        // depend on whatever the seed happened to roll.
        const STAGE_Y: i32 = 200;
        {
            // Build the largest stage any test needs before guests join.
            // Its state arrives in chunk snapshots; replaying thousands of
            // historical BlockSet messages made every guest relight each cell.
            let grass = sim.world.reg.block_id("base:grass").unwrap();
            let stone = sim.world.reg.block_id("base:stone").unwrap();
            let (x0, x1, z0, z1) = (-4, 20, -4, 20);
            let mut edits = Vec::new();
            for x in x0..=x1 {
                for z in z0..=z1 {
                    let rim = x == x0 || x == x1 || z == z0 || z == z1;
                    edits.push((x, STAGE_Y - 1, z, grass));
                    for dy in 0..10 {
                        let want = if rim && dy < 2 { stone } else { AIR };
                        if sim.world.get_block(x, STAGE_Y + dy, z) != want {
                            edits.push((x, STAGE_Y + dy, z, want));
                        }
                    }
                }
            }
            sim.world.set_blocks_for_test(edits);
            for cx in (x0 >> 4) - 1..=(x1 >> 4) + 1 {
                for cz in (z0 >> 4) - 1..=(z1 >> 4) + 1 {
                    sim.world.player_touched.insert(tchunk(cx, cz));
                }
            }
        }
        // Broadcast every subsequent world edit, the way the dedicated host
        // does — the agents' mirrors must see what the test changes.
        sim.world.set_edit_logging(true);
        sess.set_initial_view_distance_for_test(2);
        sess.fresh_spawn = Some(ep(glam::Vec3::new(0.5, STAGE_Y as f32 + 0.2, 0.5)));
        let addr = format!("127.0.0.1:{}", sess.net.port).parse().unwrap();
        let shared = Arc::new(Mutex::new((sess, sim)));
        let stop = Arc::new(AtomicBool::new(false));
        let (s2, st2) = (shared.clone(), stop.clone());
        let handle = std::thread::spawn(move || {
            while !st2.load(Ordering::Relaxed) {
                {
                    let mut g = s2.lock().unwrap();
                    let (sess, sim) = &mut *g;
                    // A deterministic stage: under parallel load these
                    // tests run long enough for weather to roll in
                    // (settling snow eats block placements) and for
                    // seeded wildlife to wander into placement cells.
                    // The agent tests exercise the wire, not the wild.
                    sim.world.weather = crate::world::Weather::Clear;
                    sim.world.replace_mobs(Vec::new());
                    sess.pump(sim, None, 0.02);
                    let players = sess.player_ctxs(None);
                    let mut evs = Vec::new();
                    sim.advance(0.02, &players, &mut evs);
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        });
        TestHost {
            shared,
            stop,
            addr,
            handle: Some(handle),
        }
    }

    fn with<R>(
        &self,
        f: impl FnOnce(&mut crate::mp::HostSession, &mut crate::server::Server) -> R,
    ) -> R {
        let mut g = self.shared.lock().unwrap();
        let (sess, sim) = &mut *g;
        f(sess, sim)
    }
}

impl Drop for TestHost {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

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
fn the_agent_crafts_places_and_deposits() {
    let host = TestHost::start("agent-craft");
    let mut a = Agent::connect_for_test(host.addr, "JOINER").expect("joins");
    a.pump_for(0.5);
    // The host gives raw logs the way any drop arrives.
    let gid = host.with(|sess, _| *sess.guests.keys().next().unwrap());
    host.with(|_, sim| {
        let log = sim.world.reg.item_id("base:log").unwrap();
        let stack = crate::inventory::ItemStack::new(&sim.world.reg, log, 4);
        sim.world.queue_give(gid, stack);
    });
    for _ in 0..100 {
        a.pump(0.02);
        if a.inventory.slots.iter().flatten().any(|s| s.count == 4) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    a.craft("planks", 3).expect("logs become planks");
    let planks: u32 = a
        .inventory
        .slots
        .iter()
        .flatten()
        .filter(|s| a.reg.item(s.item).name.contains("planks"))
        .map(|s| s.count)
        .sum();
    assert!(planks >= 12, "3 crafts x 4 planks ({planks})");
    let player_cell = crate::agent::cell_of(a.player.pos).expect("agent occupies a world cell");
    a.craft("crafting_table", 1).expect("table crafts");
    // Two cells out: the host (rightly) refuses placement into any
    // cell the placer's own body overlaps, and physics can settle an
    // agent right on a cell boundary.
    a.place_at(
        player_cell.offset(2, 0, 0).expect("table cell"),
        "base:crafting_table",
    )
    .expect("table places");
    a.craft("chest", 1).expect("a chest by the table");
    let chest = player_cell.offset(-2, 0, 0).expect("chest cell");
    a.place_at(chest, "base:chest").expect("chest places");
    let report = a.deposit(chest, None).expect("the pack empties into it");
    assert!(report.contains("stowed"), "{report}");
    // The host's chest — the authoritative one — holds the goods.
    let held: u32 = host.with(|_, sim| match sim.world.block_entity_at(&chest) {
        Some(crate::world::BlockEntity::Chest(c)) => {
            c.slots.iter().flatten().map(|s| s.count).sum()
        }
        _ => 0,
    });
    assert!(held > 0, "the authoritative chest holds the deposit");
}
