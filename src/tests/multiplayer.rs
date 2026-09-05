//! Protocol codecs and loopback host/guest behavior.

use super::*;

fn prepare_test_entry(session: &mut crate::mp::HostSession, sim: &crate::server::Server) {
    let y = sim.world.surface_height(8, 8) as f32 + 1.0;
    session.fresh_spawn = Some(ep(Vec3::new(8.5, y, 8.5)));
}

#[derive(Default)]
struct TestEntry {
    required: Option<std::collections::HashSet<ChunkPos>>,
    ready_sent: bool,
    accepted: bool,
}

fn acknowledge_test_entry(
    client: &crate::net::Client,
    entry: &mut TestEntry,
    messages: &[crate::net::S2C],
) {
    for message in messages {
        match message {
            crate::net::S2C::EntryManifest { required, .. } => {
                entry.required = Some(required.iter().copied().collect());
            }
            crate::net::S2C::Chunk { face, u, v, .. } => {
                if let Some(position) = crate::planet::Face::from_u8(*face)
                    .and_then(|face| ChunkPos::new(face, *u, *v).ok())
                    && let Some(required) = &mut entry.required
                {
                    required.remove(&position);
                }
            }
            crate::net::S2C::EntryAccepted => entry.accepted = true,
            _ => {}
        }
    }
    if entry
        .required
        .as_ref()
        .is_some_and(|required| required.is_empty())
        && !entry.ready_sent
    {
        client.send(&crate::net::C2S::EntryReady);
        entry.ready_sent = true;
    }
}

#[test]
fn host_residency_keeps_the_prepared_doorstep_without_guests() {
    let mut session = crate::mp::HostSession::start_on("spawn-residency".into(), 0).unwrap();
    let spawn = ep(Vec3::new(17.5, 80.2, -9.5));
    let chunk = spawn.chunk().unwrap();
    session.fresh_spawn = Some(spawn);
    let (centers, radius) = session.residency();
    assert_eq!(centers, vec![chunk]);
    assert_eq!(radius, 1);
}

#[test]
fn net_protocol_round_trips() {
    use crate::net::{C2S, S2C, decode, encode};
    let c2s = [
        C2S::Hello {
            protocol: 2,
            display_name: "DOLL".into(),
            device_public_key: [3; 32],
            client_nonce: [4; 32],
            content_hash: 42,
            style: 0x0102_0304,
        },
        C2S::Authenticate {
            signature: vec![5; 64],
            atproto: None,
        },
        C2S::Move {
            pos: ep(Vec3::new(1.5, 80.0, -3.5)),
            yaw: 1.2,
            hotbar: 2,
            sprint: true,
        },
        C2S::Break { pos: bp(1, 2, 3) },
        C2S::Place { pos: bp(-9, 70, 4) },
        C2S::AttackMob {
            id: 3,
            heavy: false,
        },
        C2S::FeedMob { id: 12 },
        C2S::BrushBlock { pos: bp(4, 30, -2) },
        C2S::BeginObserve {
            target: crate::net::DiscoveryTargetSnap::Block(bp(4, 30, -2)),
        },
        C2S::Observe {
            target: crate::net::DiscoveryTargetSnap::Block(bp(4, 30, -2)),
            ledger_slot: 3,
            calibration_slot: Some(4),
            label: Some("north spring".into()),
        },
        C2S::CopyObservation {
            writing_pos: bp(4, 30, -1),
            source: crate::net::RecordHolderSnap::Inventory { slot: 3 },
            record_id: 17,
            destination: crate::net::RecordHolderSnap::Folio { pos: bp(5, 30, -1) },
            include_location: false,
        },
        C2S::BeginExperiment {
            pos: bp(5, 30, -2),
            kind: crate::discovery::ExperimentKind::Conductivity,
        },
        C2S::SetExperimentItem {
            pos: bp(5, 30, -2),
            slot: 5,
        },
        C2S::RunExperiment {
            pos: bp(5, 30, -2),
            kind: crate::discovery::ExperimentKind::Conductivity,
            ledger_slot: 3,
            calibration_slot: Some(4),
        },
        C2S::OperateWorking {
            working_id: "base:nudge".into(),
            held_instance: 0x1234_5678_9abc_def0,
            target: crate::workings::WorkingTargetIntent::Entity {
                stable_id: (1u64 << 62) + 17,
            },
            intent: crate::workings::WorkingIntent::StartForced,
        },
        C2S::OperateWorking {
            working_id: "base:nudge".into(),
            held_instance: 0x1234_5678_9abc_def0,
            target: crate::workings::WorkingTargetIntent::Entity {
                stable_id: (1u64 << 62) + 17,
            },
            intent: crate::workings::WorkingIntent::Hold,
        },
        C2S::OperateWorking {
            working_id: "base:nudge".into(),
            held_instance: 0x1234_5678_9abc_def0,
            target: crate::workings::WorkingTargetIntent::Entity {
                stable_id: (1u64 << 62) + 17,
            },
            intent: crate::workings::WorkingIntent::Release,
        },
        C2S::OperateWorking {
            working_id: "base:nudge".into(),
            held_instance: 0x1234_5678_9abc_def0,
            target: crate::workings::WorkingTargetIntent::Entity {
                stable_id: (1u64 << 62) + 17,
            },
            intent: crate::workings::WorkingIntent::Cancel,
        },
        C2S::ContainerClick {
            pos: bp(1, 2, 3),
            slot: 4,
            right: true,
        },
        C2S::CloseContainer,
        C2S::Chat("hello wild".into()),
        C2S::Moderate {
            target: 9,
            action: crate::net::ModerationAction::Mute { seconds: 600 },
        },
        C2S::EntryReady,
        C2S::SleepRequest,
    ];
    for m in &c2s {
        let bytes = encode(m);
        assert!(!bytes.is_empty());
        let back: C2S = decode(&bytes).expect("c2s decodes");
        assert_eq!(format!("{m:?}"), format!("{back:?}"));
    }
    let s2c = [
        S2C::Challenge {
            nonce: [7; 32],
            server_fingerprint: [8; 32],
            identity_policy: crate::identity::IdentityPolicy::Local,
            admission_policy: crate::identity::AdmissionPolicy::Open,
        },
        S2C::BlockSet {
            pos: crate::planet::BlockPos::from_centered(crate::planet::Face::PosZ, 1, 2, 3)
                .unwrap(),
            id: 9,
            meta: 173,
            salt_mass: 44_321,
            soil_salinity: 91,
        },
        S2C::TimeIre {
            time: 0.5,
            ire: 33.0,
            day: 7,
        },
        S2C::WeatherCells {
            side: 8,
            cells: vec![(
                crate::planet_atlas::AtlasPos {
                    face: crate::planet::Face::PosZ,
                    u: 7,
                    v: 4,
                },
                crate::planet_atlas::LocalWeatherSample::default(),
            )],
        },
        S2C::ArcaneCue {
            bands: [2, 1],
            dominant: 4,
            ecology: Some(("Rainbells fold shut.".into(), false)),
        },
        S2C::Chat {
            from: "a".into(),
            msg: "b".into(),
        },
        S2C::Sleep {
            sleeping: 1,
            present: 3,
        },
        S2C::Chunk {
            face: crate::planet::Face::PosZ as u8,
            u: 0,
            v: 0,
            rle: vec![1, 2, 3],
        },
        S2C::EntryManifest {
            spawn: ep(Vec3::new(1.5, 80.0, -3.5)),
            required: vec![tchunk(0, 0), tchunk(1, 0)],
        },
        S2C::EntryProgress {
            resident: 4,
            total: 9,
        },
        S2C::EntryAccepted,
        S2C::HeldResult(Some(crate::net::StackSnap {
            item: 2,
            count: 1,
            durability: 40,
            arcane_id: 0,
            current_units: 0,
        })),
        S2C::WorkingResult(crate::workings::WorkingResult {
            success: true,
            stable_id: 91,
            phase: Some(crate::workings::WorkingPhase::Active),
            cue: crate::workings::WorkingCueKind::Active,
            warning_band: 2,
            message: "Nudge settles under visible strain.".into(),
        }),
        S2C::WorkingEvent(crate::workings::WorkingCue {
            stable_id: 91,
            working_id: "base:nudge".into(),
            handler: crate::workings::WorkingHandler::Nudge,
            source: bp(1, 70, 1),
            path: vec![bp(1, 70, 1), bp(3, 70, 1)],
            kind: crate::workings::WorkingCueKind::Active,
            warning_band: 2,
            completion_permille: 350,
        }),
        S2C::Mobs(crate::net::Snapshot::whole(
            1,
            vec![crate::net::MobSnap {
                id: 5,
                species: 1,
                pos: ep(Vec3::new(1.0, 2.0, 3.0)),
                yaw: 0.5,
                growth: 1.0,
                hurt: 0.0,
                health: 6.0,
                fed: true,
            }],
        )),
        S2C::ViewDistance { chunks: 8 },
    ];
    for m in &s2c {
        let back: S2C = decode(&encode(m)).expect("s2c decodes");
        assert_eq!(format!("{m:?}"), format!("{back:?}"));
    }
}

#[test]
fn arcane_interest_updates_stay_within_the_network_budget() {
    use crate::inventory::TOTAL_SLOTS;
    use crate::net::{DATAGRAM_FLOOR, S2C, encode};

    let cue = encode(&S2C::ArcaneCue {
        bands: [4, 4],
        dominant: 6,
        ecology: Some((
            "Lantern reeds bend over the spring margin; their amber light is steady, while the Current beneath them carries a muted tidal cadence. Ashlace farther upslope has caught a trace of dross in its grey-green threads without making it vanish."
                .into(),
            true,
        )),
    });
    assert!(
        cue.len() <= DATAGRAM_FLOOR,
        "the longest normal qualitative ecology cue is {} bytes",
        cue.len()
    );

    // A player can inspect only inventory, armor, and cursor custody. Use a
    // deliberately conservative armor allowance so a future slot expansion
    // fails this budget test before it silently bloats every host update.
    let inspectable_slots = TOTAL_SLOTS + 8 + 1;
    let items = encode(&S2C::ArcaneItems {
        reset: true,
        charges: (1..=inspectable_slots as u64)
            .map(|id| (id, u64::MAX - id))
            .collect(),
        implements: Vec::new(),
        apparatus: Vec::new(),
    });
    assert!(
        items.len() <= DATAGRAM_FLOOR,
        "{inspectable_slots} inspectable charge accounts encode to {} bytes",
        items.len()
    );

    // ArcaneItems uses the reliable channel and the host chunks public
    // implement metadata at sixteen records. Prove the worst declared
    // per-record budget plus a deliberately generous visible-owner and
    // apparatus census remains below the transport's 64 KiB frame ceiling.
    let apparatus = (0..128)
        .map(|index| crate::implements::ApparatusCue {
            pos: bp(index % 32, 100, index / 32),
            charge_band: 3,
            strain_band: 3,
        })
        .collect();
    let fixed = encode(&S2C::ArcaneItems {
        reset: true,
        charges: (1..=128).map(|id| (id, u64::MAX - id)).collect(),
        implements: Vec::new(),
        apparatus,
    })
    .len();
    assert!(
        fixed + 16 * crate::implements::MAX_IMPLEMENT_PUBLIC_BYTES < 64 * 1024,
        "chunked implement snapshot can exceed its reliable frame budget"
    );
}

#[test]
fn remote_roles_are_authorized_by_the_host_not_the_client_ui() {
    use crate::identity::Role;
    use crate::net::{C2S, ModerationAction, S2C};

    let reg = base_reg();
    let world = test_world_with("mp-remote-roles", reg);
    let mut sim = crate::server::Server::new(world, 0.3, 5);
    let mut sess = crate::mp::HostSession::start_on("remote-roles".into(), 0).unwrap();
    prepare_test_entry(&mut sess, &sim);
    let addr: std::net::SocketAddr = format!("127.0.0.1:{}", sess.net.port).parse().unwrap();
    let actor_identity =
        crate::identity::LocalIdentity::load_or_create(&tmp_dir("remote-role-actor")).unwrap();
    let target_identity =
        crate::identity::LocalIdentity::load_or_create(&tmp_dir("remote-role-target")).unwrap();
    let mut actor = crate::net::Client::connect(
        addr,
        "Actor".into(),
        sess.content_hash,
        0,
        &actor_identity,
        None,
    )
    .unwrap();
    let mut target = crate::net::Client::connect(
        addr,
        "Target".into(),
        sess.content_hash,
        0,
        &target_identity,
        None,
    )
    .unwrap();

    let mut actor_entry = TestEntry::default();
    let mut target_entry = TestEntry::default();
    for _ in 0..600 {
        sess.pump(&mut sim, None, 0.05);
        let actor_messages = actor.poll();
        let target_messages = target.poll();
        acknowledge_test_entry(&actor, &mut actor_entry, &actor_messages);
        acknowledge_test_entry(&target, &mut target_entry, &target_messages);
        if actor_entry.accepted && target_entry.accepted {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(sess.guests.len(), 2);
    let actor_id = sess
        .guests
        .iter()
        .find_map(|(id, guest)| (guest.name == "ACTOR").then_some(*id))
        .unwrap();
    let target_id = sess
        .guests
        .iter()
        .find_map(|(id, guest)| (guest.name == "TARGET").then_some(*id))
        .unwrap();

    // A forged privileged packet from an ordinary player changes nothing.
    actor.send(&C2S::Moderate {
        target: target_id,
        action: ModerationAction::Kick,
    });
    for _ in 0..60 {
        sess.pump(&mut sim, None, 0.05);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(sess.guests.contains_key(&target_id));
    assert!(actor.poll().iter().any(|message| {
        matches!(message, S2C::Toast(text) if text.contains("does not permit"))
    }));

    // A moderator still cannot grant roles.
    assert!(
        sess.set_guest_role(actor_id, Role::Moderator, "test owner")
            .unwrap()
    );
    actor.send(&C2S::Moderate {
        target: target_id,
        action: ModerationAction::CycleRole,
    });
    for _ in 0..60 {
        sess.pump(&mut sim, None, 0.05);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(sess.guest_role(target_id), Some(Role::Player));

    // An admin request is accepted, persisted, and reflected to the target.
    assert!(
        sess.set_guest_role(actor_id, Role::Admin, "test owner")
            .unwrap()
    );
    actor.send(&C2S::Moderate {
        target: target_id,
        action: ModerationAction::CycleRole,
    });
    for _ in 0..60 {
        sess.pump(&mut sim, None, 0.05);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(sess.guest_role(target_id), Some(Role::Moderator));

    // The same authorized request path applies a durable mute, and the host
    // rejects the target's next chat packet instead of broadcasting it.
    let _ = actor.poll();
    let _ = target.poll();
    actor.send(&C2S::Moderate {
        target: target_id,
        action: ModerationAction::Mute { seconds: 600 },
    });
    for _ in 0..60 {
        sess.pump(&mut sim, None, 0.05);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(actor.poll().iter().any(|message| {
        matches!(message, S2C::Toast(text) if text.contains("muted for 600 seconds"))
    }));
    target.send(&C2S::Chat("this must not be broadcast".into()));
    for _ in 0..60 {
        sess.pump(&mut sim, None, 0.05);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(
        target
            .poll()
            .iter()
            .any(|message| { matches!(message, S2C::Toast(text) if text.contains("muted")) })
    );
    assert!(!actor.poll().iter().any(|message| {
        matches!(message, S2C::Chat { msg, .. } if msg == "this must not be broadcast")
    }));

    actor.send(&C2S::Moderate {
        target: target_id,
        action: ModerationAction::Kick,
    });
    for _ in 0..60 {
        sess.pump(&mut sim, None, 0.05);
        if !sess.guests.contains_key(&target_id) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(!sess.guests.contains_key(&target_id));
}

#[test]
fn loopback_join_stream_and_edit() {
    use crate::net::{C2S, S2C};
    let reg = base_reg();
    // Host: a real session on an ephemeral port, with a real world.
    let world = test_world_with("mphost", reg.clone());
    let mut sim = crate::server::Server::new(world, 0.3, 5);
    sim.world.set_edit_logging(true);
    let mut sess = crate::mp::HostSession::start_on("loop".into(), 0).expect("host binds");
    prepare_test_entry(&mut sess, &sim);
    let port = sess.net.port;

    // Guest connects over localhost, wearing a chosen look.
    let host_held = reg.item_id("base:torch").unwrap().0;
    let host_style = crate::style::Style {
        skin: 1,
        hair: 2,
        shirt: 3,
        trousers: 4,
        beard: 3,
        ..Default::default()
    }
    .pack();
    let guest_style = crate::style::Style {
        skin: 4,
        hair: 6,
        shirt: 8,
        trousers: 2,
        hair_style: 3,
        legwear: 1,
        build: 0,
        ..Default::default()
    };
    let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let identity = crate::identity::LocalIdentity::load_or_create(&tmp_dir("mp-client-id"))
        .expect("test identity");
    let mut client = crate::net::Client::connect(
        addr,
        "tester".into(),
        sess.content_hash,
        guest_style.pack(),
        &identity,
        None,
    )
    .expect("connect");

    // Pump both sides until the Welcome lands.
    let ground = sim.world.surface_height(8, 8) as f32 + 1.0;
    let gpos = Vec3::new(8.5, ground, 8.5);
    let mut welcome = None;
    let mut torch_wire: Option<usize> = None;
    let mut held_echo: Option<((u16, u32), (u16, u32))> = None;
    let mut got_chunk = false;
    let mut chunk_data: Option<(ChunkPos, Vec<u8>)> = None;
    let mut entry = TestEntry::default();
    for _ in 0..600 {
        sess.pump(
            &mut sim,
            Some((ep(gpos), 0.0, false, host_held, host_style)),
            0.06,
        );
        let messages = client.poll();
        acknowledge_test_entry(&client, &mut entry, &messages);
        for msg in messages {
            match msg {
                S2C::Welcome {
                    palette,
                    items,
                    your_id,
                    ..
                } => {
                    assert!(!palette.is_empty(), "palette shipped");
                    assert!(your_id > 0);
                    torch_wire = items.iter().position(|n| n == "base:torch");
                    welcome = Some(palette);
                    client.send(&C2S::Move {
                        pos: ep(Vec3::new(0.5, 80.0, 0.5)),
                        yaw: 0.0,
                        hotbar: 0,
                        sprint: false,
                    });
                }
                S2C::Chunk { face, u, v, rle } => {
                    got_chunk = true;
                    if chunk_data.is_none()
                        && let Some(face) = crate::planet::Face::from_u8(face)
                        && let Ok(pos) = ChunkPos::new(face, u, v)
                    {
                        chunk_data = Some((pos, rle));
                    }
                }
                _ => {}
            }
        }
        if welcome.is_some() && got_chunk && entry.accepted {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let palette = welcome.expect("welcome arrived");
    assert!(got_chunk, "chunks streamed to the guest");

    // Tests may seed server-owned state, but the wire no longer can. Put the
    // guest near the fixture and give it a real selected torch on the host.
    let gid = *sess.guests.keys().next().expect("guest admitted");
    {
        let guest = sess.guests.get_mut(&gid).unwrap();
        guest.pos = ep(gpos);
        let torch = reg.item_id("base:torch").unwrap();
        guest.inventory.slots[0] = Some(ItemStack::new(&reg, torch, 4));
        guest.hotbar = 0;
        guest.held = torch.0;
    }
    client.send(&C2S::Move {
        pos: ep(gpos),
        yaw: 0.0,
        hotbar: 0,
        sprint: false,
    });
    client.send(&C2S::Move {
        pos: ep(gpos + Vec3::new(100.0, 40.0, 100.0)),
        yaw: 0.0,
        hotbar: 0,
        sprint: true,
    });
    for _ in 0..15 {
        sess.pump(
            &mut sim,
            Some((ep(gpos), 0.0, false, host_held, host_style)),
            0.05,
        );
    }
    assert_eq!(
        sess.guests[&gid].pos,
        ep(gpos),
        "teleport intent is rejected"
    );

    // The streamed chunk decodes into an identical remote chunk.
    let (pos, rle) = chunk_data.unwrap();
    assert!(
        rle.starts_with(b"WFC9"),
        "live terrain carries the host's settled light field"
    );
    let mut remote = World::new(1, tmp_dir("mpguest"), reg.clone());
    remote.set_remote(true);
    let content = crate::client_session::ContentMap::new(reg.clone(), palette, Vec::new());
    remote.insert_remote_chunk(pos, &rle, content.blocks());
    let host_chunk = sim.world.chunks().get(&pos).unwrap();
    let guest_chunk = remote.chunks().get(&pos).unwrap();
    assert_eq!(
        host_chunk.raw(),
        guest_chunk.raw(),
        "chunk survives the wire"
    );
    for x in 0..crate::chunk::CHUNK_X {
        for z in 0..crate::chunk::CHUNK_Z {
            for y in 0..crate::chunk::CHUNK_Y {
                assert_eq!(
                    guest_chunk.light(x, y, z),
                    host_chunk.light(x, y, z),
                    "guest reuses authoritative host lighting at {x},{y},{z}"
                );
            }
        }
    }
    for x in 0..crate::chunk::CHUNK_X {
        for z in 0..crate::chunk::CHUNK_Z {
            for y in 0..crate::chunk::CHUNK_Y {
                assert_eq!(host_chunk.meta(x, y, z), guest_chunk.meta(x, y, z));
                assert_eq!(
                    host_chunk.water_salt(x, y, z),
                    guest_chunk.water_salt(x, y, z),
                    "host and guest retain exact salt mass"
                );
                assert_eq!(
                    host_chunk.soil_salinity(x, y, z),
                    guest_chunk.soil_salinity(x, y, z),
                    "host and guest retain managed soil salinity"
                );
            }
        }
    }
    assert_eq!(
        host_chunk.hydrology_volumes(),
        guest_chunk.hydrology_volumes(),
        "host and guest agree on detailed/coarse ownership"
    );
    // Remote worlds never generate on their own.
    assert!(!remote.ensure_chunk(tchunk(90, 90)));
    assert!(!remote.chunks().contains_key(&tchunk(90, 90)));

    // Guest breaks a block: host applies it authoritatively and echoes.
    let y = sim.world.surface_height(9, 9);
    // Use a known hand-harvestable fixture. Temporary planetary terrain can
    // put tool-gated rock at this coordinate, which correctly yields nothing
    // to a torch-wielding guest.
    let dirt = reg.block_id("base:dirt").unwrap();
    sim.world.set_block(9, y, 9, dirt);
    client.send(&C2S::Break { pos: bp(9, y, 9) });
    let mut echoed = false;
    let mut given = false;
    for _ in 0..600 {
        sess.pump(
            &mut sim,
            Some((ep(gpos), 0.0, false, host_held, host_style)),
            0.06,
        );
        for msg in client.poll() {
            match msg {
                S2C::BlockSet { pos, id: 0, .. }
                    if pos == crate::planet::BlockPos::of_world(9, y, 9).unwrap() =>
                {
                    echoed = true
                }
                S2C::Give { .. } => given = true,
                S2C::Players(list) => {
                    // Held items and styles ride the snapshot: the
                    // host's and our own, round-tripped.
                    let host = list.items.iter().find(|p| p.0 == 0).map(|p| (p.3, p.4));
                    let me = list.items.iter().find(|p| p.0 != 0).map(|p| (p.3, p.4));
                    if let (Some(h), Some(m)) = (host, me) {
                        held_echo = Some((h, m));
                    }
                }
                _ => {}
            }
        }
        if echoed && given && held_echo.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(sim.world.get_block(9, y, 9), AIR, "host applied the break");
    assert!(echoed, "edit echoed to the guest");
    let ((h, hst), (m, mst)) = held_echo.expect("players snapshot carried held items");
    assert_eq!(h, host_held, "host's torch visible to guests");
    assert_eq!(m as usize, torch_wire.unwrap(), "our held id round-trips");
    assert_eq!(hst, host_style, "host style visible to guests");
    assert_eq!(
        crate::style::Style::unpack(mst),
        guest_style,
        "our chosen style round-trips through Hello and the snapshot"
    );
    assert!(
        sess.guests
            .values()
            .all(|g| g.held as usize == torch_wire.unwrap()),
        "host tracks the guest's held item"
    );
    assert!(given, "drops crossed the wire to the breaker");

    // Out of reach is refused.
    let far_y = sim.world.surface_height(200, 200);
    sim.world.ensure_chunk(ChunkPos::of_world(200, 200));
    let far_block = sim.world.get_block(200, far_y, 200);
    client.send(&C2S::Break {
        pos: bp(200, far_y, 200),
    });
    for _ in 0..90 {
        sess.pump(
            &mut sim,
            Some((ep(gpos), 0.0, false, host_held, host_style)),
            0.06,
        );
        client.poll();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        sim.world.get_block(200, far_y, 200),
        far_block,
        "beyond reach: request rejected"
    );

    // The bucket over the wire: a full cell scoops to air, a partial
    // one is refused — no minting water from films. Both cells sit in
    // walled pans so the flow tick can't redistribute them mid-test.
    let stone = reg.block_id("base:stone").unwrap();
    let wy = sim.world.surface_height(8, 8) + 3;
    for (cx, cz) in [(8, 10), (11, 8)] {
        sim.world.set_block(cx, wy - 1, cz, stone);
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            sim.world.set_block(cx + dx, wy - 1, cz + dz, stone);
            sim.world.set_block(cx + dx, wy, cz + dz, stone);
        }
    }
    sim.world.set_block(8, wy, 10, reg.water_block(0));
    sim.world.set_block(11, wy, 8, reg.water_for_volume(3));
    let bucket = reg.item_id("base:bucket").unwrap();
    sess.guests.get_mut(&gid).unwrap().inventory.slots[0] = Some(ItemStack::new(&reg, bucket, 1));
    client.send(&C2S::Scoop { pos: bp(8, wy, 10) });
    client.send(&C2S::Scoop { pos: bp(11, wy, 8) });
    for _ in 0..90 {
        sess.pump(
            &mut sim,
            Some((ep(gpos), 0.0, false, host_held, host_style)),
            0.06,
        );
        client.poll();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(sim.world.get_block(8, wy, 10), AIR, "full cell scooped");
    assert_eq!(
        reg.water_volume(sim.world.get_block(11, wy, 8)),
        Some(3),
        "partial cell refused: buckets can't mint water"
    );

    // Containers are transactional. The server owns the cursor; the packet
    // contains only which slot was clicked. A worn tool keeps its durability.
    let chest = reg.block_id("base:chest").expect("chest exists");
    let sword = reg.item_id("base:bronze_sword").expect("sword exists");
    let cy = sim.world.surface_height(8, 8) + 1;
    sim.world.set_block(10, cy, 8, chest);
    let chest_pos = bp(10, cy, 8);
    client.send(&C2S::OpenContainer { pos: chest_pos });
    let mut opened = false;
    for _ in 0..300 {
        sess.pump(
            &mut sim,
            Some((ep(gpos), 0.0, false, host_held, host_style)),
            0.06,
        );
        for msg in client.poll() {
            if matches!(msg, S2C::Container { pos, kind: 0, .. } if pos == chest_pos) {
                opened = true;
            }
        }
        if opened {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(opened, "chest opened over the wire");
    // Seed the authoritative cursor, deposit a worn sword into slot 2...
    sess.guests.get_mut(&gid).unwrap().cursor = Some(ItemStack {
        item: sword,
        count: 1,
        durability: 7,
        arcane_id: 0,
    });
    client.send(&C2S::ContainerClick {
        pos: chest_pos,
        slot: 2,
        right: false,
    });
    // ...then immediately pick it back up.
    client.send(&C2S::ContainerClick {
        pos: chest_pos,
        slot: 2,
        right: false,
    });
    let mut cursor_back = None;
    for _ in 0..300 {
        sess.pump(
            &mut sim,
            Some((ep(gpos), 0.0, false, host_held, host_style)),
            0.06,
        );
        for msg in client.poll() {
            if let S2C::HeldResult(Some(s)) = msg {
                cursor_back = Some(s);
            }
        }
        if cursor_back.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let s = cursor_back.expect("cursor echoed back");
    assert_eq!(s.item, sword.0, "same sword returns");
    assert_eq!(s.durability, 7, "worn stays worn across the wire");
    if let Some(crate::world::BlockEntity::Chest(c)) = sim.world.block_entity(&(10, cy, 8)) {
        assert!(c.slots[2].is_none(), "host chest slot emptied again");
    } else {
        panic!("host chest entity exists");
    }

    // Sleep vote: host asleep + guest asleep = dawn.
    sim.time_of_day = 0.75;
    client.send(&C2S::SleepRequest);
    let mut dawned = false;
    for _ in 0..300 {
        sess.pump(
            &mut sim,
            Some((ep(gpos), 0.0, true, host_held, host_style)),
            0.06,
        );
        client.poll();
        if (sim.time_of_day - 0.3).abs() < 0.01 {
            dawned = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(dawned, "unanimous camp sleeps to dawn");

    // Chat relays.
    client.send(&C2S::Chat("hello".into()));
    let mut chatted = false;
    for _ in 0..300 {
        let fx = sess.pump(
            &mut sim,
            Some((ep(gpos), 0.0, false, host_held, host_style)),
            0.06,
        );
        if fx
            .iter()
            .any(|f| matches!(f, crate::mp::HostFx::Chat { .. }))
        {
            chatted = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(chatted, "chat reached the host");

    // Gravity over the wire: break sand's support and the guest sees
    // the tumble (Falling datagrams) and the authoritative landing.
    let sand = reg.item_id("base:sand").unwrap();
    let sand_b = reg.block_id("base:sand").unwrap();
    let sy2 = sim.world.surface_height(11, 8);
    let plank_b = reg.block_id("base:planks").unwrap();
    sim.world.set_block(11, sy2 + 3, 8, plank_b);
    sim.world.set_block(11, sy2 + 4, 8, sand_b);
    client.send(&C2S::Break {
        pos: bp(11, sy2 + 3, 8),
    });
    let (mut saw_falling, mut saw_land) = (false, false);
    for _ in 0..600 {
        sess.pump(
            &mut sim,
            Some((ep(gpos), 0.0, false, host_held, host_style)),
            0.06,
        );
        sim.advance(0.06, &[], &mut Vec::new());
        for msg in client.poll() {
            match msg {
                S2C::Falling(f) if !f.items.is_empty() => saw_falling = true,
                S2C::BlockSet { pos, id, .. }
                    if pos.surface().centered_u() == 11
                        && pos.surface().centered_v() == 8
                        && id == sand_b.0 =>
                {
                    saw_land = true
                }
                _ => {}
            }
        }
        if saw_falling && saw_land {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(saw_falling, "the guest watches the sand tumble");
    assert!(saw_land, "and receives its authoritative landing");
    let _ = sand;

    // The steelworks over the wire: a guest charges and lights a
    // bloomery through the container RPC, then hammers at the anvil.
    let by = sim.world.surface_height(12, 8) + 1;
    build_bloomery(&mut sim.world, &reg, 12, by, 8);
    client.send(&C2S::OpenContainer { pos: bp(12, by, 8) });
    let mut got_bloomery = false;
    for _ in 0..300 {
        sess.pump(
            &mut sim,
            Some((ep(gpos), 0.0, false, host_held, host_style)),
            0.06,
        );
        for msg in client.poll() {
            if matches!(
                msg,
                S2C::MachineContainer { machine, .. } if machine == "base:bloomery"
            ) {
                got_bloomery = true;
            }
        }
        if got_bloomery {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(got_bloomery, "bloomery streams as a MachineContainer");
    let iron = reg.item_id("base:iron_ingot").unwrap();
    let coal = reg.item_id("base:charcoal").unwrap();
    if let Some(crate::world::BlockEntity::Multiblock(state)) =
        sim.world.block_entity_mut(&(12, by, 8))
    {
        state.charge[0] = Some(ItemStack::new(&reg, iron, 2));
        state.fuel[0] = Some(ItemStack::new(&reg, coal, 2));
    }
    let ember = reg.item_id("base:ember").unwrap();
    sess.guests.get_mut(&gid).unwrap().inventory.slots[1] = Some(ItemStack::new(&reg, ember, 1));
    client.send(&C2S::LightBloomery { pos: bp(12, by, 8) });
    let lit = reg.block_id("base:bloomery_lit").unwrap();
    let mut is_lit = false;
    for _ in 0..300 {
        sess.pump(
            &mut sim,
            Some((ep(gpos), 0.0, false, host_held, host_style)),
            0.06,
        );
        client.poll();
        if sim.world.get_block(12, by, 8) == lit {
            is_lit = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(is_lit, "a guest can charge and light the stack");
    // Anvil: put a bloom, strike thrice, the bar comes back as a Give.
    let anvil = reg.block_id("base:stone_anvil").unwrap();
    sim.world.set_block(11, by, 10, anvil);
    let bloom = reg.item_id("base:steel_bloom").unwrap();
    let ingot = reg.item_id("base:steel_ingot").unwrap();
    {
        let guest = sess.guests.get_mut(&gid).unwrap();
        guest.inventory.slots[0] = Some(ItemStack::new(&reg, bloom, 1));
        guest.hotbar = 0;
    }
    client.send(&C2S::AnvilPut {
        pos: bp(11, by, 10),
    });
    for _ in 0..300 {
        sess.pump(
            &mut sim,
            Some((ep(gpos), 0.0, false, host_held, host_style)),
            0.06,
        );
        client.poll();
        if matches!(
            sim.world.block_entity(&(11, by, 10)),
            Some(crate::world::BlockEntity::Anvil(state)) if state.bloom.is_some()
        ) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(matches!(
        sim.world.block_entity(&(11, by, 10)),
        Some(crate::world::BlockEntity::Anvil(state)) if state.bloom.is_some()
    ));
    let hammer = reg.item_id("base:smith_hammer").expect("smith hammer");
    sess.guests.get_mut(&gid).unwrap().inventory.slots[0] = Some(ItemStack::new(&reg, hammer, 1));
    for expected_strikes in 1..=3 {
        client.send(&C2S::AnvilStrike {
            pos: bp(11, by, 10),
        });
        for _ in 0..300 {
            sess.pump(
                &mut sim,
                Some((ep(gpos), 0.0, false, host_held, host_style)),
                0.06,
            );
            let observed = match sim.world.block_entity(&(11, by, 10)) {
                Some(crate::world::BlockEntity::Anvil(state)) => state.strikes,
                _ => 0,
            };
            if observed == expected_strikes || expected_strikes == 3 && observed == 0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let observed = match sim.world.block_entity(&(11, by, 10)) {
            Some(crate::world::BlockEntity::Anvil(state)) => state.strikes,
            _ => 0,
        };
        assert!(
            observed == expected_strikes || expected_strikes == 3 && observed == 0,
            "authoritative anvil reached strike {expected_strikes}, observed {observed}"
        );
        // A human click cannot arrive faster than the authoritative action
        // cooldown. Advance it before queuing the next strike so this test
        // checks the work result rather than deliberately rate-limited input.
        for _ in 0..21 {
            sess.pump(
                &mut sim,
                Some((ep(gpos), 0.0, false, host_held, host_style)),
                0.06,
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    let mut bar = false;
    for _ in 0..300 {
        sess.pump(
            &mut sim,
            Some((ep(gpos), 0.0, false, host_held, host_style)),
            0.06,
        );
        for msg in client.poll() {
            if let S2C::Give { item, .. } = msg
                && item == ingot.0
            {
                bar = true;
            }
        }
        if bar {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(bar, "guest hammer strikes work the bloom into a bar");

    // The kiln streams to guests as container kind 4.
    let ky = sim.world.surface_height(6, 12) + 1;
    build_bloomery(&mut sim.world, &reg, 6, ky, 12);
    let kiln_b = reg.block_id("base:kiln").unwrap();
    sim.world.set_block(6, ky, 12, kiln_b);
    client.send(&C2S::OpenContainer { pos: bp(6, ky, 12) });
    let mut got_kiln = false;
    for _ in 0..300 {
        sess.pump(
            &mut sim,
            Some((ep(gpos), 0.0, false, host_held, host_style)),
            0.06,
        );
        for msg in client.poll() {
            if matches!(
                msg,
                S2C::MachineContainer { machine, .. } if machine == "base:kiln"
            ) {
                got_kiln = true;
            }
        }
        if got_kiln {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(got_kiln, "kiln streams as a MachineContainer");

    // A withdrawn sleep vote blocks the dawn.
    sim.time_of_day = 0.75;
    client.send(&C2S::SleepRequest);
    client.send(&C2S::SleepCancel);
    for _ in 0..90 {
        sess.pump(
            &mut sim,
            Some((ep(gpos), 0.0, true, host_held, host_style)),
            0.06,
        );
        client.poll();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        (sim.time_of_day - 0.75).abs() < 0.02,
        "host sleeping alone after a cancel must not dawn"
    );

    // Kick: the guest is dropped and its authenticated principal is banned.
    let gid = *sess.guests.keys().next().expect("guest present");
    assert!(sess.kick_guest(gid).is_some());
    assert!(sess.guests.is_empty(), "kicked guest removed");
    for _ in 0..300 {
        sess.pump(&mut sim, None, 0.06);
        client.poll();
        if !client.is_connected() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(!client.is_connected(), "kicked guest disconnected");
    // Rejoining with the banned key never yields a Welcome, even after rename.
    let mut client2 = crate::net::Client::connect(
        addr,
        "another name".into(),
        sess.content_hash,
        0,
        &identity,
        None,
    )
    .expect("reconnect");
    let mut turned_away = false;
    for _ in 0..450 {
        sess.pump(&mut sim, None, 0.06);
        for msg in client2.poll() {
            match msg {
                S2C::Refused(_) => turned_away = true,
                S2C::Welcome { .. } => panic!("banned principal re-admitted"),
                _ => {}
            }
        }
        if turned_away || !client2.is_connected() {
            turned_away = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(turned_away, "banned principal turned away");
    assert!(
        sess.guests.is_empty(),
        "banned principal never becomes a guest"
    );
}

#[test]
fn late_join_gets_complete_roster_and_duplicate_name_is_refused() {
    use crate::net::{RefusalCode, S2C};
    let reg = base_reg();
    let world = test_world_with("mp-roster", reg);
    let mut sim = crate::server::Server::new(world, 0.3, 7);
    let mut sess = crate::mp::HostSession::start_on_with_policy(
        "roster".into(),
        0,
        Some(crate::identity::DisplayName::parse("Host").unwrap()),
        crate::identity::IdentityPolicy::Local,
        crate::identity::AdmissionPolicy::Open,
    )
    .unwrap();
    prepare_test_entry(&mut sess, &sim);
    let addr: std::net::SocketAddr = format!("127.0.0.1:{}", sess.net.port).parse().unwrap();
    let first_identity =
        crate::identity::LocalIdentity::load_or_create(&tmp_dir("roster-one")).unwrap();
    let mut first = crate::net::Client::connect(
        addr,
        "Fern".into(),
        sess.content_hash,
        0,
        &first_identity,
        None,
    )
    .unwrap();
    let mut first_entry = TestEntry::default();
    for _ in 0..300 {
        sess.pump(&mut sim, None, 0.05);
        let messages = first.poll();
        acknowledge_test_entry(&first, &mut first_entry, &messages);
        if first_entry.accepted {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(sess.guests.len(), 1);

    let second_identity =
        crate::identity::LocalIdentity::load_or_create(&tmp_dir("roster-two")).unwrap();
    let mut second = crate::net::Client::connect(
        addr,
        "Moss".into(),
        sess.content_hash,
        0,
        &second_identity,
        None,
    )
    .unwrap();
    let mut names = Vec::new();
    let mut second_entry = TestEntry::default();
    for _ in 0..300 {
        sess.pump(&mut sim, None, 0.05);
        let messages = second.poll();
        acknowledge_test_entry(&second, &mut second_entry, &messages);
        for message in messages {
            if let S2C::Welcome { roster, .. } = message {
                names = roster
                    .into_iter()
                    .map(|presence| presence.display_name)
                    .collect();
            }
        }
        if !names.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    names.sort();
    assert_eq!(names, ["FERN", "HOST", "MOSS"]);

    let third_identity =
        crate::identity::LocalIdentity::load_or_create(&tmp_dir("roster-three")).unwrap();
    let mut duplicate = crate::net::Client::connect(
        addr,
        "mOsS".into(),
        sess.content_hash,
        0,
        &third_identity,
        None,
    )
    .unwrap();
    let mut refused = false;
    for _ in 0..300 {
        sess.pump(&mut sim, None, 0.05);
        refused |= duplicate.poll().iter().any(|message| {
            matches!(
                message,
                S2C::Refused(refusal) if refusal.code == RefusalCode::NameInUse
            )
        });
        if refused {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(refused);
    assert_eq!(sess.guests.len(), 2);
}

#[test]
fn atproto_required_refuses_a_local_client_before_admission() {
    let mut host = crate::net::Host::start(
        "required-policy-test".into(),
        0,
        crate::identity::IdentityPolicy::AtprotoRequired,
        crate::identity::AdmissionPolicy::Open,
        0,
    )
    .unwrap();
    let identity =
        crate::identity::LocalIdentity::load_or_create(&tmp_dir("required-local")).unwrap();
    let addr: std::net::SocketAddr = format!("127.0.0.1:{}", host.port).parse().unwrap();
    let error = crate::net::Client::connect(addr, "Fern".into(), 0, 0, &identity, None)
        .err()
        .expect("required policy refuses an unlinked local client");
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    assert!(
        error
            .to_string()
            .contains("requires a linked ATProto account")
    );
    std::thread::sleep(std::time::Duration::from_millis(20));
    assert!(
        host.poll()
            .iter()
            .all(|event| !matches!(event, crate::net::HostEvent::Joined { .. }))
    );
}

#[test]
fn pre_entry_gameplay_is_ignored_until_terrain_is_acknowledged() {
    use crate::net::{C2S, S2C};

    let reg = base_reg();
    let world = test_world_with("mp-auth-order", reg);
    let mut sim = crate::server::Server::new(world, 0.3, 7);
    let mut session = crate::mp::HostSession::start_on("auth-order".into(), 0).unwrap();
    prepare_test_entry(&mut session, &sim);
    let identity =
        crate::identity::LocalIdentity::load_or_create(&tmp_dir("auth-order-client")).unwrap();
    let address: std::net::SocketAddr = format!("127.0.0.1:{}", session.net.port).parse().unwrap();
    let mut client =
        crate::net::Client::connect(address, "Fern".into(), 0, 0, &identity, None).unwrap();
    // Queue gameplay immediately after Authenticate, before the host's game
    // loop has processed the authenticated Join event.
    client.send(&C2S::Chat("too early to overtake welcome".into()));

    let mut order = Vec::new();
    for _ in 0..600 {
        session.pump(&mut sim, None, 0.05);
        for message in client.poll() {
            match message {
                S2C::Welcome { .. } => order.push("welcome"),
                S2C::Chat { .. } => order.push("chat"),
                _ => {}
            }
        }
        if order.contains(&"welcome") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(order, ["welcome"]);
}

#[test]
fn pending_guest_is_inert_until_the_exact_entry_set_is_acknowledged() {
    use crate::net::{C2S, S2C};

    let world = test_world("mp-entry-gate");
    let mut sim = crate::server::Server::new(world, 0.3, 7);
    let mut session = crate::mp::HostSession::start_on("entry-gate".into(), 0).unwrap();
    prepare_test_entry(&mut session, &sim);
    let identity =
        crate::identity::LocalIdentity::load_or_create(&tmp_dir("entry-gate-client")).unwrap();
    let address: std::net::SocketAddr = format!("127.0.0.1:{}", session.net.port).parse().unwrap();
    let mut client =
        crate::net::Client::connect(address, "Fern".into(), 0, 0, &identity, None).unwrap();
    let mut required: Option<std::collections::HashSet<ChunkPos>> = None;
    for _ in 0..1_000 {
        let fx = session.pump(&mut sim, None, 0.05);
        assert!(
            fx.iter()
                .all(|event| !matches!(event, crate::mp::HostFx::Joined(_))),
            "a terrain-decoding connection was announced as joined"
        );
        for message in client.poll() {
            match message {
                S2C::EntryManifest {
                    required: manifest, ..
                } => required = Some(manifest.into_iter().collect()),
                S2C::Chunk { face, u, v, .. } => {
                    if let Some(position) = crate::planet::Face::from_u8(face)
                        .and_then(|face| ChunkPos::new(face, u, v).ok())
                        && let Some(required) = &mut required
                    {
                        required.remove(&position);
                    }
                }
                _ => {}
            }
        }
        if required.as_ref().is_some_and(|set| set.is_empty()) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(required.is_some_and(|set| set.is_empty()));
    let id = *session.guests.keys().next().unwrap();
    let before = (
        session.guests[&id].pos,
        session.guests[&id].health,
        session.guests[&id].hunger,
    );
    assert!(!session.guests[&id].is_active());
    assert!(session.player_ctxs(None).is_empty());

    client.send(&C2S::Move {
        pos: ep(Vec3::new(100.5, 120.0, 100.5)),
        yaw: 1.0,
        hotbar: 0,
        sprint: true,
    });
    client.send(&C2S::Chat("I should not exist yet".into()));
    for _ in 0..40 {
        session.pump(&mut sim, None, 0.25);
        assert!(
            client
                .poll()
                .iter()
                .all(|message| !matches!(message, S2C::Chat { .. }))
        );
    }
    assert_eq!(
        (
            session.guests[&id].pos,
            session.guests[&id].health,
            session.guests[&id].hunger,
        ),
        before,
        "movement and survival are frozen before entry"
    );

    client.send(&C2S::EntryReady);
    let mut accepted = false;
    let mut announced = false;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        let fx = session.pump(&mut sim, None, 0.05);
        announced |= fx
            .iter()
            .any(|event| matches!(event, crate::mp::HostFx::Joined(name) if name == "FERN"));
        accepted |= client
            .poll()
            .iter()
            .any(|message| matches!(message, S2C::EntryAccepted));
        if accepted {
            break;
        }
        // Reliable transport has its own runtime thread. A fixed-count busy
        // loop can consume all 200 pumps before that thread is scheduled under
        // the full serial suite, even though the host accepted correctly.
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(accepted);
    assert!(announced);
    assert!(session.guests[&id].is_active());
    assert_eq!(session.player_ctxs(None).len(), 1);
}

#[test]
fn loopback_reconnect_reopens_the_same_server_profile() {
    use crate::net::S2C;

    let reg = base_reg();
    let world = test_world_with("mp-reconnect", reg.clone());
    let mut sim = crate::server::Server::new(world, 0.3, 7);
    let mut session = crate::mp::HostSession::start_on("reconnect".into(), 0).unwrap();
    prepare_test_entry(&mut session, &sim);
    let identity =
        crate::identity::LocalIdentity::load_or_create(&tmp_dir("reconnect-client")).unwrap();
    let address: std::net::SocketAddr = format!("127.0.0.1:{}", session.net.port).parse().unwrap();
    let mut first =
        crate::net::Client::connect(address, "Fern".into(), 0, 0, &identity, None).unwrap();
    for _ in 0..600 {
        session.pump(&mut sim, None, 0.05);
        if first
            .poll()
            .iter()
            .any(|message| matches!(message, S2C::Welcome { .. }))
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let first_connection = *session.guests.keys().next().unwrap();
    let player_id = session.guests[&first_connection].player_id;
    let torch = reg.item_id("base:torch").unwrap();
    session
        .guests
        .get_mut(&first_connection)
        .unwrap()
        .inventory
        .slots[0] = Some(ItemStack::new(&reg, torch, 6));
    // Exercise the protocol's graceful disconnect while the client's writer
    // runtime is still alive. Relying only on Drop races the queued Bye
    // against QUIC shutdown under a highly parallel test run.
    first.send(&crate::net::C2S::Bye);
    for _ in 0..1_000 {
        session.pump(&mut sim, None, 0.05);
        if session.guests.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(session.guests.is_empty());
    drop(first);

    let mut second =
        crate::net::Client::connect(address, "New Name".into(), 0, 0, &identity, None).unwrap();
    for _ in 0..600 {
        session.pump(&mut sim, None, 0.05);
        if second
            .poll()
            .iter()
            .any(|message| matches!(message, S2C::Welcome { .. }))
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let guest = session.guests.values().next().unwrap();
    assert_eq!(guest.player_id, player_id);
    assert_eq!(guest.inventory.slots[0].unwrap().count, 6);
    assert_eq!(guest.name, "NEW NAME");
}

// ---------------- scaling: the guest is a first-class citizen ----------------

use crate::net::S2C;

/// Build `n` mob snapshots spread over a wide area.
fn mob_snaps(n: usize) -> Vec<crate::net::MobSnap> {
    (0..n)
        .map(|i| crate::net::MobSnap {
            id: i as u32 + 1,
            species: (i % 7) as u16,
            pos: ep(Vec3::new(i as f32 * 3.5, 64.0, i as f32 * -2.5)),
            yaw: 0.7,
            growth: 1.0,
            hurt: 0.0,
            health: 8.0,
            fed: i % 2 == 0,
        })
        .collect()
}

#[test]
fn a_full_world_of_mobs_is_batched_under_the_datagram_budget() {
    use crate::net::{DATAGRAM_FLOOR, S2C, batch_snapshot};

    // The old code put every mob in the world into one datagram. QUIC will
    // not fragment a datagram, so past roughly forty mobs the whole snapshot
    // was refused and guests silently stopped seeing wildlife entirely.
    let whole = crate::net::encode(&S2C::Mobs(crate::net::Snapshot::whole(
        1,
        mob_snaps(crate::world::MOB_CAP),
    )));
    assert!(
        whole.len() > DATAGRAM_FLOOR * 4,
        "a full world of mobs is far past one datagram ({} bytes) — \
         if this ever stops being true the batching below is untested",
        whole.len()
    );

    for count in [0, 1, 2, 39, 40, 41, 120, crate::world::MOB_CAP] {
        let parts = batch_snapshot(7, mob_snaps(count), DATAGRAM_FLOOR, S2C::Mobs);
        assert!(!parts.is_empty(), "{count} mobs must still send something");
        for (i, bytes) in parts.iter().enumerate() {
            assert!(
                bytes.len() <= DATAGRAM_FLOOR,
                "{count} mobs: part {i} is {} bytes, over the {DATAGRAM_FLOOR} budget",
                bytes.len()
            );
        }
    }
}

#[test]
fn a_split_snapshot_is_applied_only_once_it_is_whole() {
    use crate::client_session::SnapshotAssembler;
    use crate::net::{DATAGRAM_FLOOR, S2C, batch_snapshot, decode};

    let sent = mob_snaps(200);
    let parts = batch_snapshot(9, sent.clone(), DATAGRAM_FLOOR, S2C::Mobs);
    assert!(parts.len() > 1, "200 mobs must actually split");

    let mut rx: SnapshotAssembler<crate::net::MobSnap> = Default::default();
    let mut delivered = None;
    for (i, bytes) in parts.iter().enumerate() {
        let Some(S2C::Mobs(part)) = decode::<S2C>(bytes) else {
            panic!("part {i} decodes")
        };
        let last = i + 1 == parts.len();
        match rx.accept(part) {
            Some(whole) => {
                assert!(last, "a generation must not apply before its last part");
                delivered = Some(whole);
            }
            None => assert!(!last, "the last part completes the generation"),
        }
    }
    let got = delivered.expect("the whole snapshot arrives");
    assert_eq!(got.len(), sent.len(), "every mob survives the split");
    let ids: Vec<u32> = got.iter().map(|m| m.id).collect();
    let want: Vec<u32> = sent.iter().map(|m| m.id).collect();
    assert_eq!(ids, want, "order and identity survive reassembly");
}

#[test]
fn host_owned_loose_item_ids_survive_batched_guest_and_agent_snapshots() {
    use crate::client_session::SnapshotAssembler;
    use crate::net::{DATAGRAM_FLOOR, S2C, batch_snapshot, decode};

    let sent = (0..200)
        .map(|index| crate::net::LooseItemSnap {
            id: (1u64 << 62) + index,
            pos: ep(Vec3::new(index as f32 * 0.25, 80.0, 0.5)),
            vel: Vec3::new(0.1, 0.0, -0.1),
            item: (index % 16) as u16,
            count: (index % 64 + 1) as u32,
            age: index as f32 * 0.1,
            durability: index as u32,
            arcane_id: 0,
        })
        .collect::<Vec<_>>();
    let parts = batch_snapshot(11, sent.clone(), DATAGRAM_FLOOR, S2C::LooseItems);
    assert!(parts.len() > 1);
    assert!(parts.iter().all(|part| part.len() <= DATAGRAM_FLOOR));

    let mut receiver: SnapshotAssembler<crate::net::LooseItemSnap> = Default::default();
    let mut delivered = None;
    for bytes in parts {
        let Some(S2C::LooseItems(part)) = decode::<S2C>(&bytes) else {
            panic!("loose-item snapshot did not decode")
        };
        if let Some(items) = receiver.accept(part) {
            delivered = Some(items);
        }
    }
    let delivered = delivered.expect("the whole loose-item generation arrives");
    assert_eq!(delivered.len(), sent.len());
    assert_eq!(
        delivered.iter().map(|item| item.id).collect::<Vec<_>>(),
        sent.iter().map(|item| item.id).collect::<Vec<_>>()
    );
    assert!(delivered.iter().all(|item| item.id < (1u64 << 63)));
}

#[test]
fn a_lost_part_costs_its_generation_and_nothing_after_it() {
    use crate::client_session::SnapshotAssembler;
    use crate::net::{DATAGRAM_FLOOR, S2C, batch_snapshot, decode};

    let mut rx: SnapshotAssembler<crate::net::MobSnap> = Default::default();
    let dropped = batch_snapshot(1, mob_snaps(200), DATAGRAM_FLOOR, S2C::Mobs);
    assert!(dropped.len() > 1);
    // Everything but the last part of generation 1 lands.
    for bytes in &dropped[..dropped.len() - 1] {
        let Some(S2C::Mobs(part)) = decode::<S2C>(bytes) else {
            panic!()
        };
        assert!(rx.accept(part).is_none());
    }
    // Generation 2 arrives whole and is applied regardless.
    let good = batch_snapshot(2, mob_snaps(200), DATAGRAM_FLOOR, S2C::Mobs);
    let mut applied = None;
    for bytes in &good {
        let Some(S2C::Mobs(part)) = decode::<S2C>(bytes) else {
            panic!()
        };
        if let Some(whole) = rx.accept(part) {
            applied = Some(whole);
        }
    }
    assert_eq!(
        applied.map(|m| m.len()),
        Some(200),
        "a torn generation must not stall the stream behind it"
    );
}

/// Stand up a host with one connected guest. Returns the session, the sim,
/// the client, and the guest's id.
fn loopback_pair(
    name: &str,
) -> (
    crate::mp::HostSession,
    crate::server::Server,
    crate::net::Client,
    u32,
) {
    let (sess, sim, client, id, _) = loopback_pair_drained(name);
    (sess, sim, client, id)
}

/// As `loopback_pair`, but also hands back everything that arrived while the
/// handshake was settling — the host starts streaming its ring immediately,
/// so a test that counts chunks has to count those too.
fn loopback_pair_drained(
    name: &str,
) -> (
    crate::mp::HostSession,
    crate::server::Server,
    crate::net::Client,
    u32,
    Vec<S2C>,
) {
    loopback_pair_drained_with_reg(name, base_reg())
}

fn loopback_pair_drained_with_reg(
    name: &str,
    reg: Arc<Registry>,
) -> (
    crate::mp::HostSession,
    crate::server::Server,
    crate::net::Client,
    u32,
    Vec<S2C>,
) {
    let world = test_world_with(name, reg);
    let mut sim = crate::server::Server::new(world, 0.3, 5);
    sim.world.set_edit_logging(true);
    let mut sess = crate::mp::HostSession::start_on(name.into(), 0).expect("host binds");
    prepare_test_entry(&mut sess, &sim);
    let addr: std::net::SocketAddr = format!("127.0.0.1:{}", sess.net.port).parse().unwrap();
    let identity =
        crate::identity::LocalIdentity::load_or_create(&tmp_dir(&format!("{name}-id"))).unwrap();
    let mut client =
        crate::net::Client::connect(addr, "tester".into(), sess.content_hash, 0, &identity, None)
            .expect("connect");
    let mut drained = Vec::new();
    let mut required: Option<std::collections::HashSet<ChunkPos>> = None;
    let mut entry_ready_sent = false;
    for _ in 0..600 {
        sess.pump(&mut sim, None, 0.05);
        let batch = client.poll();
        for message in &batch {
            match message {
                S2C::EntryManifest {
                    required: manifest, ..
                } => required = Some(manifest.iter().copied().collect()),
                S2C::Chunk { face, u, v, .. } => {
                    if let Some(position) = crate::planet::Face::from_u8(*face)
                        .and_then(|face| ChunkPos::new(face, *u, *v).ok())
                        && let Some(required) = &mut required
                    {
                        required.remove(&position);
                    }
                }
                _ => {}
            }
        }
        if required
            .as_ref()
            .is_some_and(|required| required.is_empty())
            && !entry_ready_sent
        {
            client.send(&crate::net::C2S::EntryReady);
            entry_ready_sent = true;
        }
        let done = batch
            .iter()
            .any(|message| matches!(message, S2C::EntryAccepted));
        drained.extend(batch);
        if done {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let id = *sess.guests.keys().next().expect("guest admitted");
    (sess, sim, client, id, drained)
}

#[test]
fn gameplay_guest_depot_debits_only_the_accepted_held_goods() {
    use crate::net::C2S;
    let reg = Arc::new(registry::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("test-fixtures/gameplay/mods"),
    ));
    let (mut host, mut sim, mut client, id, _) =
        loopback_pair_drained_with_reg("proof-guest-depot", reg.clone());
    let pos = bp(8, 200, 10);
    sim.world.set_block_at(pos, AIR);
    assert!(sim.world.place_block_at(pos, b(&reg, "proof:depot")));
    let clay = it(&reg, "base:clay_ball");
    assert_eq!(
        sim.world
            .depot_deposit(pos, &ItemStack::new(&reg, clay, 60)),
        60
    );
    let guest = host.guests.get_mut(&id).unwrap();
    guest.pos = ep(Vec3::new(8.5, 200.0, 8.5));
    guest.inventory = Inventory::new();
    guest.hotbar = 3;
    guest.inventory.slots[0] = Some(ItemStack::new(&reg, clay, 7));
    guest.inventory.slots[3] = Some(ItemStack::new(&reg, clay, 16));
    client.send(&C2S::DepotDeposit { pos });
    let mut delivered = None;
    let mut inventory_echo = None;
    for _ in 0..200 {
        host.pump(&mut sim, None, 0.05);
        for message in client.poll() {
            match message {
                S2C::SettlementDelivery { units, .. } => delivered = Some(units),
                S2C::PlayerState(state) => inventory_echo = Some(state.inventory),
                _ => {}
            }
        }
        if delivered.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(
        delivered,
        Some(4),
        "the real guest received a delivery acknowledgement"
    );
    let guest = &host.guests[&id];
    let Some(crate::world::BlockEntity::Depot(depot)) = sim.world.block_entity_at(&pos) else {
        panic!("depot still exists");
    };
    let stock: u32 = depot.storage.iter().flatten().map(|s| s.count).sum();
    let other = guest.inventory.slots[0].map_or(0, |s| s.count);
    let held = guest.inventory.slots[3].map_or(0, |s| s.count);
    eprintln!(
        "PROOF guest depot: before other=7 held=16 staged=60; after other={other} held={held} staged={stock} credited={delivered:?}"
    );
    assert_eq!(
        (other, held, stock),
        (7, 12, 64),
        "guest delivery transfers exactly four units from the held stack"
    );
    let echo = inventory_echo.expect("the guest sees its new inventory immediately");
    assert_eq!(echo[3].as_ref().map(|s| s.count), Some(12));
}

#[test]
fn host_and_guest_cross_a_planet_seam_smoothly_with_both_faces_streamed() {
    use crate::net::{C2S, S2C};
    use crate::planet::{EntityPos, FACE_BLOCKS, Face};

    let (mut sess, mut sim, mut client, id) = loopback_pair("mp-planet-seam");
    let start = EntityPos::new(Face::PosZ, f32::from(FACE_BLOCKS) - 0.2, 120.0, 4096.5).unwrap();
    let end = start.translated(Vec3::X * 0.7).unwrap().pos;
    assert_ne!(start.face(), end.face(), "fixture crosses a cube face");
    sess.guests.get_mut(&id).unwrap().prime_move_for_test(start);
    let mut streamed_faces = std::collections::HashSet::new();

    client.send(&C2S::Move {
        pos: end,
        yaw: 0.4,
        hotbar: 0,
        sprint: false,
    });
    for _ in 0..200 {
        sess.pump(&mut sim, Some((end, 0.4, false, u16::MAX, 0)), 0.0);
        for message in client.poll() {
            if let S2C::Chunk { face, .. } = message
                && let Some(face) = Face::from_u8(face)
            {
                streamed_faces.insert(face);
            }
        }
        if sess.guests[&id].pos == end {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(sess.guests[&id].pos, end, "the host accepts the seam step");

    let from_render = start.render_pos();
    let to_render = end.render_pos();
    let just_accepted = sess.guests[&id].render_pos().0;
    assert!(
        just_accepted.distance(from_render) < 1.0e-4,
        "the render span starts at the pre-seam position"
    );
    sess.pump(&mut sim, Some((end, 0.4, false, u16::MAX, 0)), 0.15);
    let midway = sess.guests[&id].render_pos().0;
    assert!(
        midway.distance(from_render) > 0.05 && midway.distance(to_render) > 0.05,
        "embedded interpolation must not snap to either face endpoint"
    );

    let mut saw_host_across = false;
    let streaming_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < streaming_deadline {
        sess.pump(&mut sim, Some((end, 0.4, false, u16::MAX, 0)), 0.06);
        for message in client.poll() {
            match message {
                S2C::Chunk { face, .. } => {
                    if let Some(face) = Face::from_u8(face) {
                        streamed_faces.insert(face);
                    }
                }
                S2C::Players(part) => {
                    saw_host_across |= part
                        .items
                        .iter()
                        .any(|(player, pos, ..)| *player == 0 && pos.face() == end.face());
                }
                _ => {}
            }
        }
        let guest = &sess.guests[&id];
        if guest.holds_chunk_at(start.chunk().unwrap())
            && guest.holds_chunk_at(end.chunk().unwrap())
            && saw_host_across
            && streamed_faces.contains(&start.face())
            && streamed_faces.contains(&end.face())
        {
            break;
        }
        // Chunk load/generation and RLE encoding are intentionally off the
        // host pump, and recording a reliable send is not the same as the
        // client having polled it. Yield until both sides of that contract
        // are observed instead of racing the transport after send-side state
        // happens to become complete.
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let guest = &sess.guests[&id];
    assert!(
        guest.holds_chunk_at(start.chunk().unwrap()),
        "the source-face seam chunk remains resident"
    );
    assert!(
        guest.holds_chunk_at(end.chunk().unwrap()),
        "the destination-face seam chunk streams"
    );
    assert!(
        streamed_faces.contains(&start.face()) && streamed_faces.contains(&end.face()),
        "the client receives terrain from both sides of the seam"
    );
    assert!(
        saw_host_across,
        "the guest receives the host's canonical destination-face snapshot"
    );
}

#[test]
fn a_crowded_world_still_reaches_the_guest() {
    use crate::mobs::Mob;

    let (mut sess, mut sim, mut client, id) = loopback_pair("mp-crowded");
    let gpos = Vec3::new(8.5, sim.world.surface_height(8, 8) as f32 + 1.0, 8.5);
    sess.guests.get_mut(&id).unwrap().pos = ep(gpos);

    // Two hundred mobs inside the guest's reach: far past what ever fit in
    // one datagram, which is exactly the case that used to go silent.
    for i in 0..200 {
        let angle = i as f32 * 0.31;
        let pos = gpos + Vec3::new(angle.sin() * 30.0, 0.0, angle.cos() * 30.0);
        let mut mob = Mob::new(0, pos, 0.0);
        mob.id = i as u32 + 1;
        sim.world.spawn_mob(mob);
    }
    assert!(sim.world.mob_count() >= 200);

    let mut seen: std::collections::HashSet<u32> = Default::default();
    for _ in 0..400 {
        sess.pump(&mut sim, Some((ep(gpos), 0.0, false, u16::MAX, 0)), 0.06);
        for msg in client.poll() {
            if let S2C::Mobs(part) = msg {
                seen.extend(part.items.iter().map(|m| m.id));
            }
        }
        if seen.len() >= 200 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(
        sess.net.datagram_failures(),
        0,
        "the host must not be dropping state datagrams on the floor"
    );
    assert!(
        seen.len() >= 200,
        "a guest standing among 200 mobs saw only {} of them",
        seen.len()
    );
}

#[test]
fn a_guest_that_dropped_a_chunk_can_ask_for_it_again() {
    let (mut sess, mut sim, mut client, id, drained) = loopback_pair_drained("mp-rechunk");
    let gpos = Vec3::new(8.5, sim.world.surface_height(8, 8) as f32 + 1.0, 8.5);
    sess.guests.get_mut(&id).unwrap().pos = ep(gpos);

    // The host starts the ordinary ring as soon as admission completes, so
    // the first chunk is allowed to share the poll that carried
    // EntryAccepted. Ignoring the handshake drain made this test depend on
    // thread scheduling even though the host had correctly recorded and sent
    // the ground.
    let mut first = drained.into_iter().find_map(|message| {
        let S2C::Chunk { face, u, v, .. } = message else {
            return None;
        };
        crate::planet::Face::from_u8(face).and_then(|face| ChunkPos::new(face, u, v).ok())
    });
    for _ in 0..400 {
        sess.pump(&mut sim, Some((ep(gpos), 0.0, false, u16::MAX, 0)), 0.06);
        for msg in client.poll() {
            if let S2C::Chunk { face, u, v, .. } = msg
                && let Some(face) = crate::planet::Face::from_u8(face)
                && let Ok(pos) = ChunkPos::new(face, u, v)
            {
                first.get_or_insert(pos);
            }
        }
        if first.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let pos = first.expect("the host streams a ring unprompted");
    assert!(
        sess.guests[&id].holds_chunk(pos.centered_u(), pos.centered_v()),
        "the host records what it sent"
    );

    // The guest evicts it (walking away and back does this for real), then
    // asks. Before RequestChunk existed this was a permanent hole.
    client.send(&crate::net::C2S::RequestChunk {
        face: pos.face() as u8,
        u: pos.u(),
        v: pos.v(),
    });
    let mut resent = false;
    for _ in 0..400 {
        sess.pump(&mut sim, Some((ep(gpos), 0.0, false, u16::MAX, 0)), 0.06);
        for msg in client.poll() {
            resent |= matches!(
                msg,
                S2C::Chunk { face, u, v, .. }
                    if (face, u, v) == (pos.face() as u8, pos.u(), pos.v())
            );
        }
        if resent {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(resent, "the host re-serves ground the guest asked for");
}

#[test]
fn the_host_serves_the_view_distance_a_guest_asks_for() {
    use crate::net::MAX_GUEST_VIEW_DIST;

    let (mut sess, mut sim, mut client, id) = loopback_pair("mp-viewdist");
    let gpos = Vec3::new(8.5, sim.world.surface_height(8, 8) as f32 + 1.0, 8.5);
    sess.guests.get_mut(&id).unwrap().pos = ep(gpos);
    // The old host served a hardcoded ring of five however far the guest
    // could actually see.
    assert_eq!(sess.guests[&id].granted_view_dist(), 5);

    client.send(&crate::net::C2S::SetViewDistance { chunks: 9 });
    let mut granted = None;
    for _ in 0..300 {
        sess.pump(&mut sim, Some((ep(gpos), 0.0, false, u16::MAX, 0)), 0.06);
        for msg in client.poll() {
            if let S2C::ViewDistance { chunks } = msg {
                granted = Some(chunks);
            }
        }
        if granted.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(granted, Some(9), "the host honours a reasonable request");
    assert_eq!(sess.guests[&id].granted_view_dist(), 9);

    // And refuses to page in the world on one client's say-so.
    client.send(&crate::net::C2S::SetViewDistance { chunks: 200 });
    let mut capped = None;
    for _ in 0..300 {
        sess.pump(&mut sim, Some((ep(gpos), 0.0, false, u16::MAX, 0)), 0.06);
        for msg in client.poll() {
            if let S2C::ViewDistance { chunks } = msg {
                capped = Some(chunks);
            }
        }
        if capped.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(capped, Some(MAX_GUEST_VIEW_DIST), "a greedy ask is clamped");
}

#[test]
fn a_world_releases_chunks_no_player_is_near() {
    let mut w = test_world("mp-residency");
    for x in -6..=6 {
        for z in -6..=6 {
            w.ensure_chunk(tchunk(x, z));
        }
    }
    let loaded = w.chunk_count();
    assert!(loaded >= 169);

    // One player near the origin: distant ground goes.
    let report = w.retain_chunks(&[tchunk(0, 0)], 2);
    assert!(
        report.released > 0,
        "chunks far from every player must be released"
    );
    assert!(report.is_ok(), "clean eviction: {}", report.summary());
    assert_eq!(
        w.chunk_count(),
        13,
        "a geodesic radius keeps a circular neighborhood"
    );
    assert!(w.has_chunk(tchunk(2, 0)));
    assert!(!w.has_chunk(tchunk(2, 2)));
    assert!(!w.has_chunk(tchunk(5, 5)));

    // Two players far apart each keep their own ground — the dedicated
    // server's case, where there is no local player at all.
    let mut w = test_world("mp-residency-two");
    for x in -6..=6 {
        for z in -6..=6 {
            w.ensure_chunk(tchunk(x, z));
        }
    }
    let report = w.retain_chunks(&[tchunk(-5, -5), tchunk(5, 5)], 1);
    assert!(report.is_ok(), "two-center eviction: {}", report.summary());
    assert!(w.has_chunk(tchunk(-5, -5)), "first player's ground");
    assert!(w.has_chunk(tchunk(5, 5)), "second player's ground");
    assert!(!w.has_chunk(tchunk(0, 0)), "the empty middle goes");

    // Nobody home: an idle server holds no world.
    let mut w = test_world("mp-residency-empty");
    for x in -3..=3 {
        for z in -3..=3 {
            w.ensure_chunk(tchunk(x, z));
        }
    }
    assert!(w.chunk_count() > 0);
    let report = w.retain_chunks(&[], 12);
    assert!(
        report.is_ok(),
        "empty-server eviction: {}",
        report.summary()
    );
    assert_eq!(w.chunk_count(), 0, "no players means no resident chunks");
}

#[test]
fn failed_dirty_chunk_eviction_keeps_only_the_unsaved_ground() {
    let mut w = test_world("mp-residency-save-failure");
    let failed = tchunk(-2, 0);
    let saved = tchunk(2, 0);
    for pos in [failed, saved] {
        w.ensure_chunk(pos);
        let x = pos.centered_u() * crate::chunk::CHUNK_X as i32;
        let y = w.surface_height(x, 0) + 1;
        let stone = w.reg.block_id("base:stone").unwrap();
        w.set_block(x, y, 0, stone);
    }
    w.fail_chunk_save_for_test(failed, true);

    let first = w.retain_chunks(&[], 0);
    assert_eq!(
        first.released, 24,
        "every healthy fixture chunk still leaves"
    );
    assert_eq!(first.retained_dirty, 1);
    assert_eq!(first.failures.len(), 1);
    assert_eq!(w.chunk_count(), 1, "only the failed chunk remains");
    assert!(w.has_chunk(failed), "the newest copy stays in memory");
    assert!(!w.has_chunk(saved), "successful ground was released");

    w.fail_chunk_save_for_test(failed, false);
    let retry = w.retain_chunks(&[], 0);
    assert_eq!(retry.released, 1);
    assert_eq!(retry.retained_dirty, 0);
    assert!(retry.is_ok(), "retry lands: {}", retry.summary());
    assert!(!w.has_chunk(failed));
}

#[test]
fn the_wild_hurts_the_guest_it_actually_struck() {
    use crate::server::{PlayerCtx, SimEvent};

    // `who` is a stable id, not a position in the players slice. Two guests
    // whose ids do not match their order is the case that used to hurt the
    // wrong person — and it only ever worked because nothing joined or left
    // between building the slice and reading the event back.
    let players = [
        PlayerCtx {
            id: 0,
            pos: ep(Vec3::ZERO),
            spawn: ep(Vec3::ZERO),
            attackable: true,
            aggro_mod: 0.0,
            quiet_charm: None,
        },
        PlayerCtx {
            id: 77,
            pos: ep(Vec3::new(50.0, 64.0, 50.0)),
            spawn: ep(Vec3::ZERO),
            attackable: true,
            aggro_mod: 0.0,
            quiet_charm: None,
        },
    ];
    // The event the sim emits for the SECOND entry names 77, not 1.
    let hit = SimEvent::PlayerHit {
        who: players[1].id,
        dmg: 3.0,
        dmg_type: None,
        attack: "melee".into(),
        from: ep(Vec3::ZERO),
    };
    let SimEvent::PlayerHit { who, .. } = hit else {
        panic!()
    };
    assert_eq!(who, 77, "the wild names the guest, not its index");
    assert_ne!(who, 1, "an index would have hurt whoever sorted second");
}

#[test]
fn wildlife_returns_to_every_country_someone_lives_in() {
    use crate::server::PlayerCtx;

    // Two players a long way apart, both standing on hunted-out ground.
    // Restocking used to follow players.first() only, so the second
    // player's country stayed empty however long they waited in it.
    let reg = base_reg();
    let mut w = World::new(9, tmp_dir("mp-repop"), reg);
    let far = 900i32;
    let far_cx = far >> 4;
    for x in -6..=6 {
        for z in -6..=6 {
            w.ensure_chunk(tchunk(x, z));
            w.ensure_chunk(tchunk(far_cx + x, z));
        }
    }
    // Overhunted: nothing left alive anywhere. Only repopulation can
    // put wildlife back now, and this is the state it exists for.
    w.replace_mobs(Vec::new());

    let ctx = |p: Vec3| PlayerCtx {
        id: 0,
        pos: ep(p),
        spawn: ep(p),
        attackable: true,
        aggro_mod: 0.0,
        quiet_charm: None,
    };
    let home = Vec3::new(8.0, w.surface_height(8, 8) as f32 + 1.0, 8.0);
    let away = Vec3::new(far as f32, w.surface_height(far, 8) as f32 + 1.0, 8.0);
    let players = [ctx(home), ctx(away)];

    let mut rng = 12345u32;
    for _ in 0..600 {
        w.tick_mobs(&players, 1.0, 1.0, &mut rng);
    }
    let near_away = w
        .mobs()
        .iter()
        .filter(|m| (m.pos - away).length() < 128.0)
        .count();
    let near_home = w
        .mobs()
        .iter()
        .filter(|m| (m.pos - home).length() < 128.0)
        .count();
    assert!(
        near_home > 0,
        "the first player's country restocks (it always did)"
    );
    assert!(
        near_away > 0,
        "the second player's country never restocked: {near_home} mobs came \
         back around player one and {near_away} around player two"
    );
}

#[test]
fn a_guest_receives_the_ring_it_was_granted() {
    // Asking for a view distance has to actually put that much ground on the
    // wire. The host used to serve a hardcoded ring of five however far the
    // guest could see; this pins the ring to the grant.
    let (mut sess, mut sim, mut client, id, drained) = loopback_pair_drained("mp-ring");
    let gpos = Vec3::new(8.5, sim.world.surface_height(8, 8) as f32 + 1.0, 8.5);
    sess.guests.get_mut(&id).unwrap().pos = ep(gpos);
    let center = ChunkPos::of_world(8, 8);

    client.send(&crate::net::C2S::SetViewDistance { chunks: 6 });
    let mut got: std::collections::HashSet<ChunkPos> = drained
        .iter()
        .filter_map(|m| match m {
            S2C::Chunk { face, u, v, .. } => crate::planet::Face::from_u8(*face)
                .and_then(|face| ChunkPos::new(face, *u, *v).ok()),
            _ => None,
        })
        .collect();
    let want = (-6..=6)
        .flat_map(|du| (-6..=6).map(move |dv| center.offset(du, dv)))
        .filter(|pos| pos.distance(center) <= 6.0 * 16.0 + 1.0)
        .collect::<std::collections::HashSet<_>>()
        .len();
    for _ in 0..4000 {
        sess.pump(&mut sim, Some((ep(gpos), 0.0, false, u16::MAX, 0)), 0.06);
        for msg in client.poll() {
            if let S2C::Chunk { face, u, v, .. } = msg
                && let Some(face) = crate::planet::Face::from_u8(face)
                && let Ok(pos) = ChunkPos::new(face, u, v)
            {
                got.insert(pos);
            }
        }
        if got.len() >= want {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(
        got.len(),
        want,
        "a guest granted 6 chunks should receive a 13x13 ring, got {}",
        got.len()
    );
    // ...and all of it around the guest, not somewhere else.
    for pos in &got {
        assert!(
            pos.distance(center) <= 6.0 * 16.0 + 1.0,
            "chunk {pos:?} is outside the granted ring around {center:?}"
        );
    }
}

#[test]
fn host_refuses_and_consumes_blueprint_gated_craft() {
    use crate::net::C2S;
    let reg = base_reg();
    let world = test_world_with("mp-blueprint", reg.clone());
    let mut sim = crate::server::Server::new(world, 0.3, 5);
    sim.world.set_edit_logging(true);
    let mut sess = crate::mp::HostSession::start_on("mp-blueprint".into(), 0).expect("host binds");
    prepare_test_entry(&mut sess, &sim);
    let addr: std::net::SocketAddr = format!("127.0.0.1:{}", sess.net.port).parse().unwrap();
    let identity = crate::identity::LocalIdentity::load_or_create(&tmp_dir("mp-blueprint-id"))
        .expect("test identity");
    let mut client =
        crate::net::Client::connect(addr, "tester".into(), sess.content_hash, 0, &identity, None)
            .expect("connect");

    let mut entry = TestEntry::default();
    let ground = sim.world.surface_height(8, 8) as f32 + 1.0;
    let gpos = Vec3::new(8.5, ground, 8.5);
    for _ in 0..600 {
        sess.pump(&mut sim, None, 0.05);
        let messages = client.poll();
        acknowledge_test_entry(&client, &mut entry, &messages);
        if entry.accepted {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(entry.accepted, "guest admitted");
    let gid = *sess.guests.keys().next().expect("guest present");

    // Seed the guest's craft grid with the Maker's Tablet recipe (clay, clay)
    // in a 2x2 grid and no blueprint item in the inventory.
    let clay = reg.item_id("base:clay_ball").unwrap();
    let plate = reg.item_id("base:maker_calibration_plate").unwrap();
    let tablet = reg.item_id("base:etched_tablet").unwrap();
    {
        let guest = sess.guests.get_mut(&gid).unwrap();
        guest.pos = ep(gpos);
        // ["c", "c"] is a 1x2 column; place both clay in the first column of
        // the 2x2 grid.
        guest.craft_grid[0] = Some(ItemStack::new(&reg, clay, 1));
        guest.craft_grid[2] = Some(ItemStack::new(&reg, clay, 1));
    }
    client.send(&C2S::CraftResult { size: 2 });
    for _ in 0..120 {
        sess.pump(&mut sim, None, 0.05);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    {
        let guest = sess.guests.get(&gid).unwrap();
        assert!(
            guest.cursor.is_none(),
            "blueprint-less craft must not put output on the cursor"
        );
        assert!(
            guest.craft_grid[0].is_some(),
            "blueprint-less craft must not consume the grid"
        );
    }

    // Now add the blueprint; the craft succeeds and consumes exactly one.
    {
        let guest = sess.guests.get_mut(&gid).unwrap();
        guest.inventory.slots[0] = Some(ItemStack::new(&reg, plate, 1));
    }
    client.send(&C2S::CraftResult { size: 2 });
    for _ in 0..120 {
        sess.pump(&mut sim, None, 0.05);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    {
        let guest = sess.guests.get(&gid).unwrap();
        let cursor = guest
            .cursor
            .expect("blueprint-present craft puts the tablet on the cursor");
        assert_eq!(cursor.item, tablet, "crafted the gated output");
        assert_eq!(
            guest.inventory.count_of(plate),
            0,
            "exactly one blueprint consumed"
        );
    }
}
