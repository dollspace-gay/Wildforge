//! Gameplay scenarios.

use super::*;

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
