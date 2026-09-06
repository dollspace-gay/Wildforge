//! An owned deterministic loopback host for client protocol scenarios.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::registry::AIR;
use crate::tests::{base_reg, bp, tmp_dir};
use crate::world::World;

/// A host running on a thread, shared so tests can reach into the
/// authoritative world between pumps.
pub(crate) struct TestHost {
    shared: Arc<Mutex<(crate::mp::HostSession, crate::server::Server)>>,
    stop: Arc<AtomicBool>,
    pub(crate) addr: std::net::SocketAddr,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl TestHost {
    pub(crate) fn start(world_tag: &str) -> TestHost {
        Self::start_inner(world_tag, false)
    }

    pub(crate) fn start_with_discovery(world_tag: &str) -> TestHost {
        Self::start_inner(world_tag, true)
    }

    fn start_inner(world_tag: &str, discovery: bool) -> TestHost {
        Self::start_at(world_tag, discovery, bp(0, 200, 0))
    }

    pub(crate) fn start_at(
        world_tag: &str,
        discovery: bool,
        origin: crate::planet::BlockPos,
    ) -> TestHost {
        let reg = base_reg();
        // These are protocol/behavior tests over a deliberately hand-built
        // stage. Generating 25 full terrain chunks here previously dominated
        // the entire suite without exercising any agent behavior.
        let root = tmp_dir(world_tag);
        let mut world = if discovery {
            let atlas = Arc::new(crate::planet_atlas::PlanetAtlas::fixture(42, 16).unwrap());
            atlas.write_new(&root).unwrap();
            World::new_with_atlas(42, root, reg, atlas)
        } else {
            World::new(42, root, reg)
        };
        world.insert_empty_chunks_for_test((-2..=2).flat_map(|x| {
            (-2..=2).map(move |z| origin.offset(x * 16, 0, z * 16).unwrap().chunk())
        }));
        let mut sim = crate::server::Server::new(world, 0.3, 5);
        let mut sess = crate::mp::HostSession::start_on(world_tag.into(), 0).unwrap();
        // Arrivals land on a stage this harness owns, well above any
        // terrain: these tests exercise the wire, not the landscape,
        // and a spawn that follows the world's shoreline makes them
        // depend on whatever the seed happened to roll.
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
                    edits.push((origin.offset(x, -1, z).unwrap(), grass));
                    for dy in 0..10 {
                        let want = if rim && dy < 2 { stone } else { AIR };
                        let at = origin.offset(x, dy, z).unwrap();
                        if sim.world.get_block_at(at) != want {
                            edits.push((at, want));
                        }
                    }
                }
            }
            sim.world.edit_fixture_for_test(|world| {
                for (at, block) in edits {
                    world.set_block_at(at, block);
                }
            });
            for cx in (x0 >> 4) - 1..=(x1 >> 4) + 1 {
                for cz in (z0 >> 4) - 1..=(z1 >> 4) + 1 {
                    sim.world
                        .player_touched
                        .insert(origin.offset(cx * 16, 0, cz * 16).unwrap().chunk());
                }
            }
        }
        // Broadcast every subsequent world edit, the way the dedicated host
        // does — the agents' mirrors must see what the test changes.
        sim.world.set_edit_logging(true);
        sess.set_initial_view_distance_for_test(2);
        sess.fresh_spawn = Some(origin.entity_at_height(0.2));
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
                    sim.world.force_local_weather("clear");
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

    pub(crate) fn pause(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            handle.join().expect("host pump joins");
        }
    }

    pub(crate) fn with<R>(
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
