//! Protocol codecs and loopback host/guest behavior.

use super::*;

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
            pos: Vec3::new(1.5, 80.0, -3.5),
            yaw: 1.2,
            hotbar: 2,
            sprint: true,
        },
        C2S::Break { x: 1, y: 2, z: 3 },
        C2S::Place { x: -9, y: 70, z: 4 },
        C2S::AttackMob { id: 3 },
        C2S::FeedMob { id: 12 },
        C2S::BrushBlock { x: 4, y: 30, z: -2 },
        C2S::ContainerClick {
            x: 1,
            y: 2,
            z: 3,
            slot: 4,
            right: true,
        },
        C2S::CloseContainer,
        C2S::Chat("hello wild".into()),
        C2S::Moderate {
            target: 9,
            action: crate::net::ModerationAction::Mute { seconds: 600 },
        },
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
            x: 1,
            y: 2,
            z: 3,
            id: 9,
            meta: 0,
        },
        S2C::TimeIre {
            time: 0.5,
            ire: 33.0,
            day: 7,
            weather: 2,
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
            x: 0,
            z: 0,
            rle: vec![1, 2, 3],
        },
        S2C::HeldResult(Some(crate::net::StackSnap {
            item: 2,
            count: 1,
            durability: 40,
        })),
        S2C::Mobs(vec![crate::net::MobSnap {
            id: 5,
            species: 1,
            pos: Vec3::new(1.0, 2.0, 3.0),
            yaw: 0.5,
            growth: 1.0,
            hurt: 0.0,
            fed: true,
        }]),
    ];
    for m in &s2c {
        let back: S2C = decode(&encode(m)).expect("s2c decodes");
        assert_eq!(format!("{m:?}"), format!("{back:?}"));
    }
}

#[test]
fn remote_roles_are_authorized_by_the_host_not_the_client_ui() {
    use crate::identity::Role;
    use crate::net::{C2S, ModerationAction, S2C};

    let reg = base_reg();
    let world = test_world_with("mp-remote-roles", reg);
    let mut sim = crate::server::Server::new(world, 0.3, 5);
    let mut sess = crate::mp::HostSession::start_on("remote-roles".into(), 0).unwrap();
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

    for _ in 0..600 {
        sess.pump(&mut sim, None, 0.05);
        let _ = actor.poll();
        if sess.guests.len() == 2 {
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
    let mut chunk_data: Option<(i32, i32, Vec<u8>)> = None;
    for _ in 0..600 {
        sess.pump(
            &mut sim,
            Some((gpos, 0.0, false, host_held, host_style)),
            0.06,
        );
        for msg in client.poll() {
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
                        pos: Vec3::new(0.5, 80.0, 0.5),
                        yaw: 0.0,
                        hotbar: 0,
                        sprint: false,
                    });
                }
                S2C::Chunk { x, z, rle } => {
                    got_chunk = true;
                    if chunk_data.is_none() {
                        chunk_data = Some((x, z, rle));
                    }
                }
                _ => {}
            }
        }
        if welcome.is_some() && got_chunk {
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
        guest.pos = gpos;
        let torch = reg.item_id("base:torch").unwrap();
        guest.inventory.slots[0] = Some(ItemStack::new(&reg, torch, 4));
        guest.hotbar = 0;
        guest.held = torch.0;
    }
    client.send(&C2S::Move {
        pos: gpos,
        yaw: 0.0,
        hotbar: 0,
        sprint: false,
    });
    client.send(&C2S::Move {
        pos: gpos + Vec3::new(100.0, 40.0, 100.0),
        yaw: 0.0,
        hotbar: 0,
        sprint: true,
    });
    for _ in 0..15 {
        sess.pump(
            &mut sim,
            Some((gpos, 0.0, false, host_held, host_style)),
            0.05,
        );
    }
    assert_eq!(sess.guests[&gid].pos, gpos, "teleport intent is rejected");

    // The streamed chunk decodes into an identical remote chunk.
    let (cx, cz, rle) = chunk_data.unwrap();
    let mut remote = World::new(1, tmp_dir("mpguest"), reg.clone());
    remote.set_remote(true);
    let remap = crate::mp::block_remap(&remote, &palette);
    remote.insert_remote_chunk(ChunkPos { x: cx, z: cz }, &rle, &remap);
    let host_chunk = sim.world.chunks().get(&ChunkPos { x: cx, z: cz }).unwrap();
    let guest_chunk = remote.chunks().get(&ChunkPos { x: cx, z: cz }).unwrap();
    assert_eq!(
        host_chunk.raw(),
        guest_chunk.raw(),
        "chunk survives the wire"
    );
    // Remote worlds never generate on their own.
    assert!(!remote.ensure_chunk(ChunkPos { x: 90, z: 90 }));
    assert!(!remote.chunks().contains_key(&ChunkPos { x: 90, z: 90 }));

    // Guest breaks a block: host applies it authoritatively and echoes.
    let y = sim.world.surface_height(9, 9);
    let target_block = sim.world.get_block(9, y, 9);
    assert_ne!(target_block, AIR);
    client.send(&C2S::Break { x: 9, y, z: 9 });
    let mut echoed = false;
    let mut given = false;
    for _ in 0..600 {
        sess.pump(
            &mut sim,
            Some((gpos, 0.0, false, host_held, host_style)),
            0.06,
        );
        for msg in client.poll() {
            match msg {
                S2C::BlockSet {
                    x: 9,
                    y: yy,
                    z: 9,
                    id: 0,
                    ..
                } if yy == y => echoed = true,
                S2C::Give { .. } => given = true,
                S2C::Players(list) => {
                    // Held items and styles ride the snapshot: the
                    // host's and our own, round-tripped.
                    let host = list.iter().find(|p| p.0 == 0).map(|p| (p.3, p.4));
                    let me = list.iter().find(|p| p.0 != 0).map(|p| (p.3, p.4));
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
        x: 200,
        y: far_y,
        z: 200,
    });
    for _ in 0..90 {
        sess.pump(
            &mut sim,
            Some((gpos, 0.0, false, host_held, host_style)),
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
    client.send(&C2S::Scoop { x: 8, y: wy, z: 10 });
    client.send(&C2S::Scoop { x: 11, y: wy, z: 8 });
    for _ in 0..90 {
        sess.pump(
            &mut sim,
            Some((gpos, 0.0, false, host_held, host_style)),
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
    client.send(&C2S::OpenContainer { x: 10, y: cy, z: 8 });
    let mut opened = false;
    for _ in 0..300 {
        sess.pump(
            &mut sim,
            Some((gpos, 0.0, false, host_held, host_style)),
            0.06,
        );
        for msg in client.poll() {
            if matches!(msg, S2C::Container { x: 10, kind: 0, .. }) {
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
    });
    client.send(&C2S::ContainerClick {
        x: 10,
        y: cy,
        z: 8,
        slot: 2,
        right: false,
    });
    // ...then immediately pick it back up.
    client.send(&C2S::ContainerClick {
        x: 10,
        y: cy,
        z: 8,
        slot: 2,
        right: false,
    });
    let mut cursor_back = None;
    for _ in 0..300 {
        sess.pump(
            &mut sim,
            Some((gpos, 0.0, false, host_held, host_style)),
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
            Some((gpos, 0.0, true, host_held, host_style)),
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
            Some((gpos, 0.0, false, host_held, host_style)),
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
        x: 11,
        y: sy2 + 3,
        z: 8,
    });
    let (mut saw_falling, mut saw_land) = (false, false);
    for _ in 0..600 {
        sess.pump(
            &mut sim,
            Some((gpos, 0.0, false, host_held, host_style)),
            0.06,
        );
        sim.advance(0.06, &[], &mut Vec::new());
        for msg in client.poll() {
            match msg {
                S2C::Falling(f) if !f.is_empty() => saw_falling = true,
                S2C::BlockSet {
                    x: 11, z: 8, id, ..
                } if id == sand_b.0 => saw_land = true,
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
    client.send(&C2S::OpenContainer { x: 12, y: by, z: 8 });
    let mut got_kind3 = false;
    for _ in 0..300 {
        sess.pump(
            &mut sim,
            Some((gpos, 0.0, false, host_held, host_style)),
            0.06,
        );
        for msg in client.poll() {
            if matches!(msg, S2C::Container { kind: 3, .. }) {
                got_kind3 = true;
            }
        }
        if got_kind3 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(got_kind3, "bloomery streams as container kind 3");
    let iron = reg.item_id("base:iron_ingot").unwrap();
    let coal = reg.item_id("base:charcoal").unwrap();
    if let Some(crate::world::BlockEntity::Bloomery(state)) =
        sim.world.block_entity_mut(&(12, by, 8))
    {
        state.charge[0] = Some(ItemStack::new(&reg, iron, 2));
        state.fuel[0] = Some(ItemStack::new(&reg, coal, 2));
    }
    let ember = reg.item_id("base:ember").unwrap();
    sess.guests.get_mut(&gid).unwrap().inventory.slots[1] = Some(ItemStack::new(&reg, ember, 1));
    client.send(&C2S::LightBloomery { x: 12, y: by, z: 8 });
    let lit = reg.block_id("base:bloomery_lit").unwrap();
    let mut is_lit = false;
    for _ in 0..300 {
        sess.pump(
            &mut sim,
            Some((gpos, 0.0, false, host_held, host_style)),
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
        x: 11,
        y: by,
        z: 10,
    });
    for _ in 0..300 {
        sess.pump(
            &mut sim,
            Some((gpos, 0.0, false, host_held, host_style)),
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
            x: 11,
            y: by,
            z: 10,
        });
        for _ in 0..300 {
            sess.pump(
                &mut sim,
                Some((gpos, 0.0, false, host_held, host_style)),
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
                Some((gpos, 0.0, false, host_held, host_style)),
                0.06,
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    let mut bar = false;
    for _ in 0..300 {
        sess.pump(
            &mut sim,
            Some((gpos, 0.0, false, host_held, host_style)),
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
    client.send(&C2S::OpenContainer { x: 6, y: ky, z: 12 });
    let mut got_kind4 = false;
    for _ in 0..300 {
        sess.pump(
            &mut sim,
            Some((gpos, 0.0, false, host_held, host_style)),
            0.06,
        );
        for msg in client.poll() {
            if matches!(msg, S2C::Container { kind: 4, .. }) {
                got_kind4 = true;
            }
        }
        if got_kind4 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(got_kind4, "kiln streams as container kind 4");

    // A withdrawn sleep vote blocks the dawn.
    sim.time_of_day = 0.75;
    client.send(&C2S::SleepRequest);
    client.send(&C2S::SleepCancel);
    for _ in 0..90 {
        sess.pump(
            &mut sim,
            Some((gpos, 0.0, true, host_held, host_style)),
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
    for _ in 0..300 {
        sess.pump(&mut sim, None, 0.05);
        if first
            .poll()
            .iter()
            .any(|message| matches!(message, S2C::Welcome { .. }))
        {
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
    for _ in 0..300 {
        sess.pump(&mut sim, None, 0.05);
        for message in second.poll() {
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
fn welcome_precedes_any_authenticated_gameplay_message() {
    use crate::net::{C2S, S2C};

    let reg = base_reg();
    let world = test_world_with("mp-auth-order", reg);
    let mut sim = crate::server::Server::new(world, 0.3, 7);
    let mut session = crate::mp::HostSession::start_on("auth-order".into(), 0).unwrap();
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
        if order.contains(&"welcome") && order.contains(&"chat") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(order, ["welcome", "chat"]);
}

#[test]
fn loopback_reconnect_reopens_the_same_server_profile() {
    use crate::net::S2C;

    let reg = base_reg();
    let world = test_world_with("mp-reconnect", reg.clone());
    let mut sim = crate::server::Server::new(world, 0.3, 7);
    let mut session = crate::mp::HostSession::start_on("reconnect".into(), 0).unwrap();
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
