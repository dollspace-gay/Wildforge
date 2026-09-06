//! Loopback join stream and edit scenarios.

use super::*;

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
    let mut remote = ReplicaWorld::new(1, reg.clone(), 0.0);
    let content = crate::client_session::ContentMap::new(reg.clone(), palette, Vec::new());
    remote.insert_remote_chunks([(pos, rle.as_slice())], content.blocks());
    let host_chunk = sim.world.chunks().get(&pos).unwrap();
    let guest_chunk = remote.chunk(pos).unwrap();
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
    assert!(remote.chunk(tchunk(90, 90)).is_none());
    assert!(!remote.has_chunk(tchunk(90, 90)));

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
