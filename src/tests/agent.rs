//! The agent is a guest, not a god: loopback coverage for the
//! headless client, its perception, locomotion, follow, and work.

use super::*;
use crate::agent::{Agent, Behavior};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

mod admission;
mod replication;

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
        Self::start_inner(world_tag, false)
    }

    fn start_with_discovery(world_tag: &str) -> TestHost {
        Self::start_inner(world_tag, true)
    }

    fn start_inner(world_tag: &str, discovery: bool) -> TestHost {
        Self::start_at(world_tag, discovery, bp(0, 200, 0))
    }

    fn start_at(world_tag: &str, discovery: bool, origin: crate::planet::BlockPos) -> TestHost {
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

    fn with<R>(
        &self,
        f: impl FnOnce(&mut crate::mp::HostSession, &mut crate::server::Server) -> R,
    ) -> R {
        let mut g = self.shared.lock().unwrap();
        let (sess, sim) = &mut *g;
        f(sess, sim)
    }
}

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

#[test]
fn four_identity_magic_society_shares_one_unprivileged_host() {
    let host = TestHost::start_with_discovery("agent-magic-society");
    let mut agents = [
        Agent::connect_for_test(host.addr, "SURVEYOR").expect("surveyor joins"),
        Agent::connect_for_test(host.addr, "CULTIVATOR").expect("cultivator joins"),
        Agent::connect_for_test(host.addr, "BINDER").expect("binder joins"),
        Agent::connect_for_test(host.addr, "APOTHECARY").expect("apothecary joins"),
    ];
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while std::time::Instant::now() < deadline {
        for agent in &mut agents {
            agent.pump(0.02);
        }
        if host.with(|session, _| session.guests.len() == 4) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let identities = host.with(|session, _| {
        session
            .guests
            .values()
            .map(|guest| guest.player_id)
            .collect::<std::collections::HashSet<_>>()
    });
    assert_eq!(
        identities.len(),
        4,
        "four roles collapsed onto one identity"
    );
    for (agent, role) in agents.iter().zip([
        "survey records ready",
        "cultivation stock ready",
        "wand workshop ready",
        "waste station ready",
    ]) {
        agent.send(&crate::net::C2S::Chat(role.into()));
    }
    assert!(
        pump_until(
            &mut agents[0],
            std::time::Duration::from_secs(15),
            |agent| {
                [
                    "survey records ready",
                    "cultivation stock ready",
                    "wand workshop ready",
                    "waste station ready",
                ]
                .into_iter()
                .all(|role| agent.events.iter().any(|event| event.contains(role)))
            },
        ),
        "the four physical roles did not share the ordinary society channel: {:?}",
        agents[0].events
    );
    host.with(|session, _| {
        assert!(session.guests.values().all(|guest| {
            guest
                .principals
                .iter()
                .any(|principal| principal == &guest.principal)
        }));
    });
}

#[test]
fn the_agent_earns_and_reads_a_host_signed_magic_observation() {
    let host = TestHost::start_with_discovery("agent-discovery");
    let mut agent = Agent::connect_for_test(host.addr, "OBSERVER").expect("observer joins");
    agent.pump_for(0.5);
    let guest_id = host.with(|session, _| *session.guests.keys().next().unwrap());
    host.with(|session, sim| {
        let at = session.guests[&guest_id]
            .pos
            .block()
            .expect("guest stands on the planetary stage");
        for item_name in ["base:tuning_lens", "base:field_ledger"] {
            let item = sim.world.reg.item_id(item_name).unwrap();
            let mut stack = crate::inventory::ItemStack::new(&sim.world.reg, item, 1);
            if item_name == "base:tuning_lens" {
                sim.world
                    .bind_arcane_stack_at(at, &mut stack, "agent discovery integration fixture")
                    .unwrap();
            }
            sim.world
                .record_external_stack(stack, "agent discovery integration fixture")
                .unwrap();
            sim.world.queue_give(guest_id, stack);
        }
    });
    for _ in 0..100 {
        agent.pump(0.02);
        let has = |name: &str| {
            let item = agent.reg.item_id(name).unwrap();
            agent
                .inventory
                .slots
                .iter()
                .flatten()
                .any(|stack| stack.item == item)
        };
        if has("base:tuning_lens") && has("base:field_ledger") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let report = agent
        .observe_discovery(
            None,
            "base:field_ledger",
            None,
            Some("agent field note".into()),
        )
        .expect("ordinary agent protocol earns a settled observation");
    assert!(
        report.contains("base:local_current_field")
            && report.contains("| region |")
            && report.contains("agent field note"),
        "{report}"
    );
    let ledger = agent
        .read_knowledge("base:field_ledger")
        .expect("the agent reads only its physical ledger");
    assert!(ledger.contains("records 1/32") && ledger.contains("agent field note"));

    host.with(|session, sim| {
        let object_id = session.guests[&guest_id]
            .inventory
            .slots
            .iter()
            .flatten()
            .find(|stack| sim.world.reg.item(stack.item).name == "base:field_ledger")
            .unwrap()
            .arcane_id;
        let records = sim.world.discovery_summaries(object_id, true).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].observer_name, "OBSERVER");
    });
}

#[test]
fn the_agent_perceives_and_excavates_dross_without_private_provenance() {
    let host = TestHost::start_with_discovery("agent-dross-parity");
    let mut agent = Agent::connect_for_test(host.addr, "REMEDIATOR").expect("agent joins");
    agent.pump_for(0.5);
    let guest_id = host.with(|session, _| *session.guests.keys().next().unwrap());
    let (scar_pos, scar_id, scar_region) = host.with(|session, sim| {
        let actor_pos = session.guests[&guest_id]
            .pos
            .block()
            .expect("agent stands on the test stage");
        let scar_pos = actor_pos.offset(2, 0, 0).unwrap();
        let atlas = sim.world.planet_atlas().unwrap().clone();
        let region = atlas.atlas_pos(scar_pos.surface());
        let ledger = sim.world.arcane_ledger.as_mut().unwrap();
        let scar_id = ledger.allocate_scar_id().unwrap();
        let mut current = ledger
            .account(&crate::arcane::ArcaneOwner::Deep)
            .unwrap()
            .current
            .clone();
        let current = current
            .take_units(
                32,
                crate::arcane::BASE_RESONANCES
                    .into_iter()
                    .map(str::to_string),
            )
            .unwrap();
        let transaction_id = ledger.system_transaction_id().unwrap();
        ledger
            .commit(crate::arcane::ArcaneTransaction::transfer(
                transaction_id,
                crate::arcane::ArcaneOwner::Deep,
                ledger.version_of(&crate::arcane::ArcaneOwner::Deep),
                crate::arcane::ArcaneOwner::Scar(scar_id),
                ledger.version_of(&crate::arcane::ArcaneOwner::Scar(scar_id)),
                current,
                crate::arcane::ArcaneAuthority::System,
                "agent dross parity fixture",
            ))
            .unwrap();
        let step = sim
            .world
            .arcane_geography
            .as_ref()
            .unwrap()
            .dynamic
            .dross_state
            .completed_steps;
        let kind = crate::dross::ScarKind::BrokenSymmetry;
        let content_id = kind.block_id().to_string();
        let geography = sim.world.arcane_geography.as_mut().unwrap();
        geography.dynamic.dross_state.cells[region.index(atlas.side())].band =
            crate::dross::DrossBand::Scar;
        geography.dynamic.dross_state.scars.insert(
            scar_id,
            crate::dross::ScarSite {
                id: scar_id,
                region,
                kind,
                content_id,
                site_slot: 0,
                created_step: step,
                last_changed_step: step,
                breach_count: 0,
                materialized_at: Some(scar_pos),
                resolved_step: None,
                actor_hint: Some([0xcc; 16]),
                installation_hint: Some(707),
                provenance: crate::dross::DrossProvenance {
                    entries: vec![crate::dross::DrossContribution {
                        actor: Some([0xcc; 16]),
                        installation_id: Some(707),
                        source_class: "private fixture laboratory".into(),
                        units: 32,
                        first_step: step,
                        last_step: step,
                        confidence_permille: 1_000,
                    }],
                    unknown_units: 0,
                },
            },
        );
        geography
            .dynamic
            .dross_state
            .materialized
            .insert(scar_pos, scar_id);
        geography.mark_scar(&atlas, region);
        let block = sim.world.reg.block_id(kind.block_id()).unwrap();
        sim.world.set_block_at(scar_pos, block);
        (scar_pos, scar_id, region)
    });

    assert!(
        pump_until(&mut agent, std::time::Duration::from_secs(15), |agent| {
            agent.world.get_block_at(scar_pos) != crate::registry::AIR
                && agent.events.iter().any(|event| {
                    event.contains("environmental dross band 4") && event.contains("pattern 4")
                })
        }),
        "the authoritative Scar cue/block never reached the agent; events: {:?}",
        agent.events
    );
    assert!(agent.events.iter().all(|event| {
        !event.contains("private fixture laboratory")
            && !event.contains("707")
            && !event.contains("cc, cc")
    }));
    host.with(|session, sim| {
        session.broadcast_dross_cue(
            &sim.world,
            crate::dross::DrossCue {
                region: scar_region,
                kind: crate::dross::DrossCueKind::Breach {
                    activity: Some(crate::dross::ScarActivityHandler::Shear),
                },
            },
        );
    });
    assert!(
        pump_until(&mut agent, std::time::Duration::from_secs(15), |agent| {
            agent.events.iter().any(|event| {
                event.contains("dross event")
                    && event.contains("field shear")
                    && event.contains("existing burden")
            })
        }),
        "the public breach activity never reached the agent; events: {:?}",
        agent.events
    );
    assert!(agent.events.iter().all(|event| {
        !event.contains("private fixture laboratory")
            && !event.contains("707")
            && !event.contains("cc, cc")
    }));
    agent
        .break_block_at(scar_pos)
        .expect("ordinary agent Break excavates the host-owned scar");
    let fragment = agent.reg.item_id("base:scar_fragment").unwrap();
    assert!(
        pump_until(&mut agent, std::time::Duration::from_secs(15), |agent| {
            agent
                .inventory
                .slots
                .iter()
                .flatten()
                .any(|stack| stack.item == fragment && stack.arcane_id != 0)
        }),
        "the exact scar fragment never reached the agent"
    );
    host.with(|_, sim| {
        let state = &sim
            .world
            .arcane_geography
            .as_ref()
            .unwrap()
            .dynamic
            .dross_state;
        assert!(state.scars[&scar_id].resolved_step.is_some());
        assert!(!state.materialized.contains_key(&scar_pos));
        assert!(
            sim.world
                .arcane_ledger
                .as_ref()
                .unwrap()
                .account(&crate::arcane::ArcaneOwner::Scar(scar_id))
                .is_none_or(|account| account.current.is_empty())
        );
    });
}

#[test]
fn the_agent_assembles_a_real_wand_and_its_identity_survives_hosted_death() {
    use crate::implements::FrameAction;

    let host = TestHost::start_with_discovery("agent-implements");
    let mut agent = Agent::connect_for_test(host.addr, "BINDER").expect("binder joins");
    agent.pump_for(0.5);
    let guest_id = host.with(|session, _| *session.guests.keys().next().unwrap());
    let player_cell = crate::agent::cell_of(agent.player.pos).expect("agent occupies stage");
    let frame = player_cell.offset(3, 0, 0).unwrap();
    // Keep the west face of the frame visible from the actor; apparatus on
    // that ray would correctly make the host reject the interaction.
    let vessel = frame.offset(0, 0, -1).unwrap();
    host.with(|_, sim| {
        sim.world
            .set_block_at(frame, sim.world.reg.block_id("base:binding_frame").unwrap());
        sim.world.set_block_at(
            frame.offset(0, 0, 1).unwrap(),
            sim.world.reg.block_id("base:focus_mount").unwrap(),
        );
        sim.world.set_block_at(
            frame.offset(1, 0, 0).unwrap(),
            sim.world.reg.block_id("base:arcane_conductor").unwrap(),
        );
        sim.world.set_block_at(
            frame.offset(0, 0, 2).unwrap(),
            sim.world.reg.block_id("base:containment_post").unwrap(),
        );
        let vessel_stack = ItemStack::new(
            &sim.world.reg,
            sim.world.reg.item_id("base:charge_vessel").unwrap(),
            1,
        );
        assert!(sim.world.place_item_block_at(vessel, vessel_stack));
        assert!(sim.world.binding_frame_layout(frame).valid);
        for name in [
            "base:seasoned_wand_body",
            "base:ritual_rod_socket",
            "base:echo_slate",
            "base:bronze_wand_binding",
        ] {
            let stack = ItemStack::new(&sim.world.reg, sim.world.reg.item_id(name).unwrap(), 1);
            sim.world
                .record_external_stack(stack, "agent implements integration fixture")
                .unwrap();
            sim.world.queue_give(guest_id, stack);
        }
    });
    for _ in 0..120 {
        agent.pump(0.02);
        let have = [
            "base:seasoned_wand_body",
            "base:ritual_rod_socket",
            "base:echo_slate",
            "base:bronze_wand_binding",
        ]
        .into_iter()
        .all(|name| {
            let item = agent.reg.item_id(name).unwrap();
            agent
                .inventory
                .slots
                .iter()
                .flatten()
                .any(|stack| stack.item == item)
        });
        if have {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    agent.pump_for(0.3);
    assert!(
        agent
            .operate_binding_frame(frame, FrameAction::Calibrate, None)
            .unwrap()
            .contains("calibration")
    );
    for name in [
        "base:seasoned_wand_body",
        "base:ritual_rod_socket",
        "base:echo_slate",
        "base:bronze_wand_binding",
    ] {
        let report = agent
            .operate_binding_frame(frame, FrameAction::ExchangeSelected, Some(name))
            .unwrap();
        assert!(report.contains("Mounted"), "{report}");
    }
    let assembled = agent
        .operate_binding_frame(frame, FrameAction::Assemble, None)
        .unwrap();
    assert!(assembled.contains("bound wand"), "{assembled}");
    let retrieved = agent
        .operate_binding_frame(frame, FrameAction::ExchangeSelected, None)
        .unwrap();
    assert!(retrieved.contains("Retrieved"), "{retrieved}");
    let wand_item = agent.reg.item_id("base:bound_wand").unwrap();
    let wand = agent
        .inventory
        .slots
        .iter()
        .flatten()
        .find(|stack| stack.item == wand_item)
        .copied()
        .expect("agent received the authoritative wand");
    assert_ne!(wand.arcane_id, 0);

    host.with(|session, sim| {
        let death_pos = session.guests[&guest_id].pos;
        session.hurt_guest(sim, guest_id, 1_000.0, death_pos);
        assert!(
            sim.world
                .pending_drops()
                .iter()
                .any(|(_, stack)| stack.arcane_id == wand.arcane_id)
        );
        assert!(
            sim.world
                .implements_state
                .as_ref()
                .unwrap()
                .instance(wand.arcane_id)
                .is_some()
        );
        assert!(
            sim.world
                .arcane_ledger
                .as_ref()
                .unwrap()
                .item_current_total(wand.arcane_id)
                .is_some()
        );
    });
    assert!(
        pump_until(&mut agent, std::time::Duration::from_secs(15), |agent| {
            agent
                .inventory
                .slots
                .iter()
                .flatten()
                .any(|stack| stack.item == wand_item && stack.arcane_id == wand.arcane_id)
        }),
        "the ordinary hosted pickup never returned stable wand {} after death; recent agent events: {:?}",
        wand.arcane_id,
        agent.events
    );
}

#[test]
fn the_agent_runs_and_cancels_a_real_working_through_the_dedicated_host() {
    use crate::implements::FrameAction;
    use crate::workings::{WorkingApparatus, WorkingIntent, WorkingPhase, WorkingTargetIntent};

    let host = TestHost::start_with_discovery("agent-workings");
    let mut agent = Agent::connect_for_test(host.addr, "PRACTITIONER")
        .expect("practitioner joins as an ordinary guest");
    agent.pump_for(0.5);
    let guest_id = host.with(|session, _| *session.guests.keys().next().unwrap());
    let aim = crate::agent::cell_of(agent.player.pos)
        .expect("agent occupies the stage")
        .offset(2, 0, 0)
        .unwrap();

    let wand_id = host.with(|_, sim| {
        let (frame, _) = super::implements::install_frame_fixture(&mut sim.world);
        let (_, revision) = super::implements::assemble_fixture_wand(&mut sim.world, frame);
        let mut transfer = Inventory::new();
        sim.world
            .operate_binding_frame(
                frame,
                &mut transfer,
                0,
                FrameAction::ExchangeSelected,
                Some(revision),
                "agent workings integration fixture",
            )
            .unwrap();
        let wand = transfer.slots[0]
            .take()
            .expect("the assembled wand leaves the physical frame");
        let wand_id = wand.arcane_id;
        assert_ne!(wand_id, 0);
        sim.world.queue_give(guest_id, wand);

        let lens_item = sim.world.reg.item_id("base:tuning_lens").unwrap();
        let mut lens = ItemStack::new(&sim.world.reg, lens_item, 1);
        sim.world
            .bind_arcane_stack_at(aim, &mut lens, "agent workings integration fixture")
            .unwrap();
        sim.world
            .record_external_stack(lens, "agent workings integration fixture")
            .unwrap();
        sim.world.queue_give(guest_id, lens);
        wand_id
    });

    for _ in 0..120 {
        agent.pump(0.02);
        let has = |name: &str| {
            let item = agent.reg.item_id(name).unwrap();
            agent
                .inventory
                .slots
                .iter()
                .flatten()
                .any(|stack| stack.item == item)
        };
        if has("base:bound_wand") && has("base:tuning_lens") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(
        agent.aim_working_at(aim).unwrap().contains("aiming"),
        "the ordinary agent aiming action remains available"
    );

    let started = agent
        .start_working(
            "base:trace",
            WorkingTargetIntent::None,
            Some("base:bound_wand"),
            false,
        )
        .expect("the dedicated host accepts the same Trace start as local play");
    assert!(started.contains("settle"), "{started}");
    let first_id = host.with(|_, sim| {
        let transaction = sim
            .world
            .workings_state
            .as_ref()
            .unwrap()
            .active
            .values()
            .find(|transaction| {
                matches!(
                    &transaction.apparatus,
                    WorkingApparatus::Wand { instance_id, .. } if *instance_id == wand_id
                )
            })
            .expect("the host owns the agent's active transaction");
        assert_eq!(transaction.definition.id, "base:trace");
        assert_eq!(transaction.actor_label, "PRACTITIONER");
        assert_eq!(transaction.phase, WorkingPhase::Charging);
        transaction.id
    });

    agent.pump_for(crate::workings::MIN_WAND_SETTLE_SECONDS + 0.2);
    let held = agent
        .continue_working(WorkingIntent::Hold)
        .expect("the agent advances the host-owned channel");
    assert!(held.contains("Trace"), "{held}");
    host.with(|_, sim| {
        assert_eq!(
            sim.world.workings_state.as_ref().unwrap().active[&first_id].phase,
            WorkingPhase::Active
        );
    });
    assert!(
        pump_until(&mut agent, std::time::Duration::from_secs(10), |agent| {
            agent.events.iter().any(|event| {
                event.contains("working cue")
                    && event.contains("base:trace")
                    && event.contains("Active")
            })
        }),
        "the reliable Active working cue never reached the agent; recent events: {:?}",
        agent.events
    );

    let released = agent
        .continue_working(WorkingIntent::Release)
        .expect("the agent releases through the ordinary completion path");
    assert!(released.contains("completes"), "{released}");
    host.with(|_, sim| {
        let state = sim.world.workings_state.as_ref().unwrap();
        assert!(!state.active.contains_key(&first_id));
        let event = state
            .history
            .iter()
            .find(|event| event.id == first_id)
            .unwrap();
        assert_eq!(event.working_id, "base:trace");
        assert_eq!(event.outcome, "completed");
    });

    // Prove cancellation is not merely exposed by the agent schema: begin a
    // second real reservation after recovery and settle it through the host.
    agent.pump_for(crate::workings::WAND_RECOVERY_SECONDS + 0.2);
    agent
        .start_working(
            "base:trace",
            WorkingTargetIntent::None,
            Some("base:bound_wand"),
            false,
        )
        .expect("the agent starts a second host-owned reservation");
    let second_id = host.with(|_, sim| {
        sim.world
            .workings_state
            .as_ref()
            .unwrap()
            .active
            .values()
            .find(|transaction| {
                matches!(
                    &transaction.apparatus,
                    WorkingApparatus::Wand { instance_id, .. } if *instance_id == wand_id
                )
            })
            .unwrap()
            .id
    });
    let cancelled = agent
        .continue_working(WorkingIntent::Cancel)
        .expect("the agent cancels through the ordinary settlement path");
    assert!(cancelled.contains("cancel"), "{cancelled}");
    host.with(|_, sim| {
        let state = sim.world.workings_state.as_ref().unwrap();
        assert!(!state.active.contains_key(&second_id));
        let event = state
            .history
            .iter()
            .find(|event| event.id == second_id)
            .unwrap();
        assert_eq!(event.outcome, "cancelled");
        assert!(state.history.iter().any(|event| {
            event.id == first_id && event.working_id == "base:trace" && event.outcome == "completed"
        }));
    });
}

#[test]
fn the_agent_runs_an_ordinary_alchemy_job_through_the_host() {
    use crate::alchemy::ApparatusAction;

    let host = TestHost::start_with_discovery("agent-alchemy");
    let mut agent = Agent::connect_for_test(host.addr, "APOTHECARY")
        .expect("apothecary joins as an ordinary guest");
    agent.pump_for(0.5);
    let guest_id = host.with(|session, _| *session.guests.keys().next().unwrap());
    let mortar = crate::agent::cell_of(agent.player.pos)
        .expect("agent occupies the stage")
        .offset(2, 0, 0)
        .unwrap();
    host.with(|_, sim| {
        sim.world.set_block_at(
            mortar,
            sim.world.reg.block_id("base:alchemy_mortar").unwrap(),
        );
        let seeds = ItemStack::new(
            &sim.world.reg,
            sim.world.reg.item_id("base:wheat_seeds").unwrap(),
            4,
        );
        sim.world
            .record_external_stack(seeds, "agent alchemy integration fixture")
            .unwrap();
        sim.world.queue_give(guest_id, seeds);
    });
    let seed_item = agent.reg.item_id("base:wheat_seeds").unwrap();
    assert!(
        pump_until(&mut agent, std::time::Duration::from_secs(10), |agent| {
            agent
                .inventory
                .slots
                .iter()
                .flatten()
                .any(|stack| stack.item == seed_item && stack.count == 4)
        }),
        "the authoritative seed stack never reached the alchemy agent"
    );

    let inspected = agent
        .operate_alchemy(mortar, ApparatusAction::Inspect, None)
        .expect("the agent inspects through the ordinary station request");
    assert!(inspected.contains("apparatus is inspected"), "{inspected}");
    let started = agent
        .operate_alchemy(
            mortar,
            ApparatusAction::PressOil { seed_slot: 0 },
            Some("base:wheat_seeds"),
        )
        .expect("the host reserves the agent's measured oil process");
    assert!(started.contains("begun"), "{started}");
    host.with(|_, sim| {
        let due = sim.world.alchemy_state().unwrap().ordinary_jobs[&mortar].due_tick;
        sim.freeze_clock = true;
        sim.world.clock = (due + 1) as f64 / 20.0;
    });
    let collected = agent
        .operate_alchemy(mortar, ApparatusAction::PressOil { seed_slot: 0 }, None)
        .unwrap_or_else(|error| {
            panic!(
                "the same host action did not collect the finished physical carrier: {error}; recent agent events: {:?}",
                agent.events
            )
        });
    assert!(collected.contains("measured finite carrier"), "{collected}");
    let oil_item = agent.reg.item_id("base:plant_oil").unwrap();
    assert!(
        pump_until(&mut agent, std::time::Duration::from_secs(10), |agent| {
            agent
                .inventory
                .slots
                .iter()
                .flatten()
                .filter(|stack| stack.item == oil_item)
                .map(|stack| stack.count)
                .sum::<u32>()
                == 4
        }),
        "the agent never received the host-authored oil yield"
    );
    host.with(|session, sim| {
        assert!(
            !sim.world
                .alchemy_state()
                .unwrap()
                .ordinary_jobs
                .contains_key(&mortar)
        );
        assert_eq!(
            session.guests[&guest_id]
                .inventory
                .slots
                .iter()
                .flatten()
                .filter(|stack| sim.world.reg.item(stack.item).name == "base:plant_oil")
                .map(|stack| stack.count)
                .sum::<u32>(),
            4
        );
    });
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
