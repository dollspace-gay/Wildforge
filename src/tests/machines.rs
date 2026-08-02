//! Containers, furnaces, production machines, and falling-block behavior.

use super::*;

#[test]
fn furnace_smelts_with_fuel_over_time() {
    use crate::world::{BlockEntity, FurnaceState};
    let reg = base_reg();
    let mut w = test_world_with("furnace", reg.clone());
    let pos = (2, 80, 2);
    w.set_block(pos.0, pos.1, pos.2, b(&reg, "base:furnace"));
    let raw = it(&reg, "base:raw_copper");
    let log = it(&reg, "base:log");
    w.insert_block_entity(
        pos,
        BlockEntity::Furnace(FurnaceState {
            input: Some(ItemStack::new(&reg, raw, 2)),
            fuel: Some(ItemStack::new(&reg, log, 2)),
            ..Default::default()
        }),
    );
    // 8s smelt at 0.1s ticks; the first log (15s) lights immediately.
    for _ in 0..85 {
        w.tick_entities(0.1);
    }
    let Some(BlockEntity::Furnace(f)) = w.block_entity(&pos) else {
        panic!("furnace")
    };
    assert_eq!(f.output.unwrap().item, it(&reg, "base:copper_ingot"));
    assert_eq!(f.input.unwrap().count, 1, "one raw consumed");
    assert_eq!(f.fuel.unwrap().count, 1, "first log consumed for fuel");
    assert!(f.burn_left > 0.0, "log burns 15s, smelt took 8");
    // Second smelt needs the second log (relights at the 15s mark).
    for _ in 0..90 {
        w.tick_entities(0.1);
    }
    let Some(BlockEntity::Furnace(f)) = w.block_entity(&pos) else {
        panic!("furnace")
    };
    assert_eq!(f.output.unwrap().count, 2);
    assert!(f.input.is_none());
    assert!(f.fuel.is_none(), "second log lit");
    // No fuel, no input: idle without panicking.
    for _ in 0..50 {
        w.tick_entities(0.1);
    }
}

#[test]
fn furnace_state_persists_and_breaks_drop_contents() {
    use crate::world::{BlockEntity, FurnaceState};
    let reg = base_reg();
    let mut w = test_world_with("furnace-save", reg.clone());
    let pos = (3, 80, 3);
    w.set_block(pos.0, pos.1, pos.2, b(&reg, "base:furnace"));
    w.insert_block_entity(
        pos,
        BlockEntity::Furnace(FurnaceState {
            input: Some(ItemStack::new(&reg, it(&reg, "base:raw_tin"), 5)),
            fuel: Some(ItemStack::new(&reg, it(&reg, "base:charcoal"), 3)),
            ..Default::default()
        }),
    );
    save_world(&mut w);
    // Reload: state comes back by item name.
    let mut w2 = World::load_or_create(w.save_dir_for_test(), reg.clone()).unwrap();
    for x in -2..=2 {
        for z in -2..=2 {
            w2.ensure_chunk(tchunk(x, z));
        }
    }
    let Some(BlockEntity::Furnace(f)) = w2.block_entity(&pos) else {
        panic!("furnace")
    };
    assert_eq!(f.input.unwrap().count, 5);
    assert_eq!(f.fuel.unwrap().item, it(&reg, "base:charcoal"));
    // Breaking the block spills the contents.
    w2.set_block(pos.0, pos.1, pos.2, AIR);
    assert!(!w2.has_block_entity(&pos));
    assert_eq!(w2.pending_drops().len(), 2, "input + fuel drop");
}

#[test]
fn chest_stores_spills_and_persists() {
    let reg = base_reg();
    let dir = tmp_dir("chestsave");
    let mut w = World::new(9, dir.clone(), reg.clone());
    w.ensure_chunk(tchunk(0, 0));
    let chest = reg.block_id("base:chest").unwrap();
    assert_eq!(reg.block(chest).interaction.as_deref(), Some("chest"));
    let pos = (4, 100, 4);
    w.set_block(pos.0, pos.1, pos.2, chest);
    let mut state = crate::world::ChestState::default();
    state.slots[0] = Some(ItemStack::new(&reg, it(&reg, "base:bread"), 3));
    state.slots[26] = Some(ItemStack::new(&reg, it(&reg, "base:bronze_ingot"), 7));
    w.insert_block_entity(pos, crate::world::BlockEntity::Chest(state));
    save_world(&mut w);

    // Round-trip by name, plus an unknown item that must skip cleanly.
    let path = dir.join("entities.toml");
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push_str("\n[[chest]]\npos = { face = \"PosZ\", u = 4105, y = 90, v = 4105 }\n[[chest.slot]]\nindex = 0\nitem = \"gone:widget\"\ncount = 5\ndurability = 0\n");
    std::fs::write(&path, text).unwrap();
    let w2 = World::load_or_create(dir, reg.clone()).unwrap();
    let Some(crate::world::BlockEntity::Chest(c)) = w2.block_entity(&pos) else {
        panic!("chest reloaded")
    };
    assert_eq!(
        c.slots[0].map(|s| (reg.item(s.item).name.clone(), s.count)),
        Some(("base:bread".to_string(), 3))
    );
    assert_eq!(c.slots[26].map(|s| s.count), Some(7));
    let Some(crate::world::BlockEntity::Chest(c2)) = w2.block_entity(&(9, 90, 9)) else {
        panic!("second chest reloaded")
    };
    assert!(c2.slots.iter().all(|s| s.is_none()), "unknown item skipped");

    // Breaking the chest spills every stack.
    let mut w3 = w2;
    // Worlds load chunks lazily. Load the saved voxel chunk before editing
    // the chest that lives in it.
    assert!(w3.ensure_chunk(tchunk(0, 0)));
    w3.set_block(pos.0, pos.1, pos.2, AIR);
    assert!(!w3.has_block_entity(&pos));
    let spilled: Vec<_> = w3.pending_drops().iter().map(|(_, s)| s.count).collect();
    assert_eq!(
        spilled.iter().sum::<u32>(),
        10,
        "3 bread + 7 ingots spilled"
    );

    // Recipe: 8 planks in a ring.
    let mut g = vec![None; 9];
    for (i, slot) in g.iter_mut().enumerate() {
        if i != 4 {
            *slot = Some(ItemStack::new(&reg, it(&reg, "base:planks"), 1));
        }
    }
    let r = crate::crafting::match_recipe(&reg, &g, 3).expect("chest recipe");
    assert_eq!(r.output, it(&reg, "base:chest"));
}

#[test]
fn ember_fuel_speeds_the_furnace() {
    let reg = base_reg();
    let mut w = test_world("emberfast");
    let f = crate::world::FurnaceState {
        input: Some(ItemStack::new(&reg, it(&reg, "base:raw_iron"), 1)),
        fuel: Some(ItemStack::new(&reg, it(&reg, "base:ember"), 1)),
        ..Default::default()
    };
    w.insert_block_entity((0, 90, 0), crate::world::BlockEntity::Furnace(f));
    // A 10 s iron smelt at the ember's 2x finishes in ~5 s; without the
    // speedup, 8 s of ticks would not be enough.
    for _ in 0..80 {
        w.tick_entities(0.1);
    }
    let Some(crate::world::BlockEntity::Furnace(f)) = w.block_entity(&(0, 90, 0)) else {
        panic!()
    };
    assert!(
        f.output.map(|s| reg.item(s.item).name.clone()).as_deref() == Some("base:iron_ingot"),
        "iron done in 8s of ember fire (progress {})",
        f.progress
    );
}

#[test]
fn charged_furnace_fuel_and_inputs_leave_no_orphan_current() {
    use crate::arcane::{ArcaneOwner, Reservoir};
    use crate::world::{BlockEntity, FurnaceState};

    let reg = base_reg();
    let mut world = World::load_or_create(
        tmp_dir("charged-furnace-lifecycle").join("world"),
        reg.clone(),
    )
    .unwrap();
    let at = crate::planet::BlockPos::of_world(0, 90, 0).unwrap();

    let mut ember = ItemStack::new(&reg, it(&reg, "base:ember"), 1);
    world
        .bind_arcane_stack_at(at, &mut ember, "charged furnace test fuel")
        .unwrap();
    let ember_owner = ArcaneOwner::Item(ember.arcane_id);
    let dross_before = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .audit()
        .unwrap()
        .reservoirs[&Reservoir::Dross];
    world.insert_block_entity_at(
        at,
        BlockEntity::Furnace(FurnaceState {
            input: Some(ItemStack::new(&reg, it(&reg, "base:raw_iron"), 1)),
            fuel: Some(ember),
            ..Default::default()
        }),
    );
    world.tick_entities(0.1);
    let ledger = world.arcane_ledger.as_ref().unwrap();
    assert!(ledger.account(&ember_owner).is_none());
    assert_eq!(
        ledger.audit().unwrap().reservoirs[&Reservoir::Dross],
        dross_before + 512,
        "ember fuel disorders its exact charge"
    );

    let quartz_at = crate::planet::BlockPos::of_world(1, 90, 0).unwrap();
    let mut quartz = ItemStack::new(&reg, it(&reg, "base:quartz_shard"), 1);
    world
        .bind_arcane_stack_at(quartz_at, &mut quartz, "charged furnace test input")
        .unwrap();
    let quartz_owner = ArcaneOwner::Item(quartz.arcane_id);
    let ambient_before = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .audit()
        .unwrap()
        .reservoirs[&Reservoir::Ambient];
    world.insert_block_entity_at(
        quartz_at,
        BlockEntity::Furnace(FurnaceState {
            input: Some(quartz),
            fuel: Some(ItemStack::new(&reg, it(&reg, "base:log"), 1)),
            ..Default::default()
        }),
    );
    for _ in 0..120 {
        world.tick_entities(0.1);
    }
    let ledger = world.arcane_ledger.as_ref().unwrap();
    assert!(ledger.account(&quartz_owner).is_none());
    assert_eq!(
        ledger.audit().unwrap().reservoirs[&Reservoir::Ambient],
        ambient_before + 384,
        "transformed quartz returns its exact charge to the environment"
    );
    assert!(ledger.audit().unwrap().is_balanced());
}

#[test]
fn brushing_yields_once_and_transmutes() {
    let reg = base_reg();
    // Archaeological finds may carry finite Current, so this fixture needs
    // the same qualified atlas + ledger a production world always owns.
    let mut w = World::load_or_create(tmp_dir("brushing").join("world"), reg.clone()).unwrap();
    w.ensure_chunk(tchunk(0, 0));
    let masonry = reg.block_id("base:cracked_masonry").unwrap();
    w.set_block(3, 150, 3, masonry);
    let mut rng = 7u32;
    let found = w.brush_block(3, 150, 3, &mut rng).expect("artifact found");
    assert!(found.count >= 1);
    if reg.item(found.item).arcane.is_some() {
        assert_ne!(found.arcane_id, 0);
        assert!(
            w.arcane_ledger
                .as_ref()
                .unwrap()
                .item_current_total(found.arcane_id)
                .is_some(),
            "a charged find remains in clean or explicitly sequestered item custody"
        );
    }
    assert_eq!(
        w.get_block(3, 150, 3),
        reg.block_id("base:cobblestone").unwrap(),
        "remnant becomes plain stone"
    );
    assert!(
        w.brush_block(3, 150, 3, &mut rng).is_none(),
        "artifact only once"
    );
    // Breaking a remnant instead just drops cobble (greed loses the find).
    let d = reg.drops_for(masonry, None).unwrap();
    assert_eq!(reg.item(d.0).name, "base:cobblestone");
}

#[test]
fn sealed_archaeological_dross_is_funded_and_materializes_once() {
    use crate::arcane::ArcaneOwner;

    let reg = base_reg();
    let mut world =
        World::load_or_create(tmp_dir("sealed-dross-brush").join("world"), reg.clone()).unwrap();
    world.ensure_chunk(tchunk(0, 0));
    let masonry = reg.block_id("base:cracked_masonry").unwrap();
    let sealed = reg.item_id("base:sealed_dross_ampoule").unwrap();
    let mut rng = 17u32;
    let mut recovered = None;
    for x in 1..15 {
        for z in 1..15 {
            world.set_block(x, 150, z, masonry);
            let stack = world.brush_block(x, 150, z, &mut rng).unwrap();
            if stack.item == sealed {
                recovered = Some((x, z, stack));
                break;
            }
        }
        if recovered.is_some() {
            break;
        }
    }
    let (x, z, stack) = recovered.expect("the deterministic site set includes sealed dross");
    let ledger = world.arcane_ledger.as_ref().unwrap();
    assert!(
        ledger
            .account(&ArcaneOwner::Item(stack.arcane_id))
            .is_none()
    );
    let contained = ledger.item_dross_total(stack.arcane_id);
    assert!(
        contained > 0,
        "the ampoule owns an actual contained reservoir"
    );
    assert!(ledger.audit().unwrap().is_balanced());

    assert!(world.brush_block(x, 150, z, &mut rng).is_none());
    let ledger = world.arcane_ledger.as_ref().unwrap();
    assert_eq!(ledger.item_dross_total(stack.arcane_id), contained);
    assert!(ledger.audit().unwrap().is_balanced());
}

#[test]
fn sand_falls_lands_chains_and_crushes() {
    let reg = base_reg();
    let mut w = test_world_with("gw-fall", reg.clone());
    let sand = reg.block_id("base:sand").unwrap();
    let plank = reg.block_id("base:planks").unwrap();
    let my = 120;
    // The ground it will land on (before we scaffold anything).
    let gy = w.surface_height(10, 10);
    // A supported sand column: pull the support and it all comes down.
    w.set_block(10, my, 10, plank);
    w.set_block(10, my + 1, 10, sand);
    w.set_block(10, my + 2, 10, sand);
    assert!(w.falling_blocks().is_empty(), "supported sand stays put");
    w.set_block(10, my, 10, AIR);
    assert_eq!(w.falling_blocks().len(), 2, "the whole column detaches");
    assert_eq!(
        w.get_block(10, my + 1, 10),
        AIR,
        "detached cells empty atomically"
    );
    for _ in 0..600 {
        w.tick_falling(1.0 / 30.0);
    }
    assert!(w.falling_blocks().is_empty(), "everything lands");
    assert_eq!(
        w.get_block(10, gy + 1, 10),
        sand,
        "first sand rests on ground"
    );
    assert_eq!(
        w.get_block(10, gy + 2, 10),
        sand,
        "second stacks on the first"
    );

    // Placing sand over air drops it immediately.
    w.set_block(12, my + 3, 12, sand);
    assert_eq!(
        w.get_block(12, my + 3, 12),
        AIR,
        "unsupported placement detaches"
    );
    assert_eq!(w.falling_blocks().len(), 1);
    w.settle_falling();
    assert!(w.falling_blocks().is_empty());

    // A crushed crop pops as its drop.
    let torch = reg.block_id("base:torch").unwrap();
    let ty2 = w.surface_height(14, 14);
    w.set_block(14, ty2 + 1, 14, torch);
    w.clear_pending_drops();
    w.set_block(14, ty2 + 6, 14, sand);
    w.settle_falling();
    assert_eq!(w.get_block(14, ty2 + 1, 14), sand, "sand took the cell");
    assert!(
        w.pending_drops()
            .iter()
            .any(|(_, st)| Some(st.item) == reg.item_id("base:torch")),
        "the torch popped as a drop"
    );
}

#[test]
fn falling_blocks_detach_and_land_on_both_sides_of_every_planet_seam() {
    use crate::planet::BlockPos;

    let reg = base_reg();
    let sand = b(&reg, "base:sand");
    let stone = b(&reg, "base:stone");
    let mut world = World::new(51, tmp_dir("planet-falling-all-seams"), reg);
    let seams = directed_planet_seams();
    let columns: Vec<_> = seams
        .iter()
        .flat_map(|seam| [seam.source, seam.across])
        .collect();
    let chunks: std::collections::BTreeSet<_> = columns
        .iter()
        .map(|surface| crate::planet::ChunkPos::from_surface(*surface))
        .collect();
    world.insert_empty_chunks_for_test(chunks);

    let fixture_set = |world: &mut World, pos: BlockPos, block| {
        let (x, y, z) = pos.local();
        world
            .chunks_mut()
            .get_mut(&pos.chunk())
            .expect("falling fixture chunk")
            .set(x, y, z, block);
    };
    for surface in &columns {
        // Save-time settling advances in coarse vertical steps.  Model the
        // planet's solid shell rather than a one-voxel floating platform, so
        // a coarse sample always finds rock below the landing surface.
        for y in 1..=95 {
            fixture_set(
                &mut world,
                BlockPos::new(surface.face(), surface.u(), y, surface.v()).unwrap(),
                stone,
            );
        }
        fixture_set(
            &mut world,
            BlockPos::new(surface.face(), surface.u(), 100, surface.v()).unwrap(),
            stone,
        );
        fixture_set(
            &mut world,
            BlockPos::new(surface.face(), surface.u(), 101, surface.v()).unwrap(),
            sand,
        );
    }

    for surface in &columns {
        world.set_block_at(
            BlockPos::new(surface.face(), surface.u(), 100, surface.v()).unwrap(),
            AIR,
        );
    }
    assert_eq!(
        world.falling_blocks().len(),
        columns.len(),
        "every seam-side column detaches exactly once"
    );
    for surface in &columns {
        let launched = BlockPos::new(surface.face(), surface.u(), 101, surface.v()).unwrap();
        assert_eq!(world.get_block_at(launched), AIR);
        assert!(world.falling_blocks().iter().any(|falling| {
            falling.pos.face() == surface.face()
                && falling.pos.u() == f32::from(surface.u())
                && falling.pos.v() == f32::from(surface.v())
        }));
    }

    world.settle_falling();
    assert!(world.falling_blocks().is_empty());
    for (index, seam) in seams.iter().enumerate() {
        for surface in [seam.source, seam.across] {
            assert_eq!(
                world.get_block_at(
                    BlockPos::new(surface.face(), surface.u(), 96, surface.v()).unwrap()
                ),
                sand,
                "falling block landed on the wrong chart for seam {index}: {:?} {:?}",
                seam.face,
                seam.direction
            );
        }
    }
}

#[test]
fn glass_smelts_passes_light_and_grows_winter_crops() {
    let reg = base_reg();
    let glass = reg.block_id("base:glass").unwrap();
    // Sand cooks into glass.
    let smelts = reg.smelts_for(reg.item_id("base:glass").unwrap());
    assert!(!smelts.is_empty(), "glass smelt registered");
    assert!(smelts[0].input.matches(reg.item_id("base:sand").unwrap()));
    assert!(!reg.block(glass).opaque, "glass is see-through");
    assert!(reg.block(glass).glass, "glass is glazing");

    let mut w = test_world_with("gw-glass", reg.clone());
    let b = |n: &str| reg.block_id(n).unwrap();
    let h = 120; // open sky, sea-proof
    // Sky light passes a glass roof (BFS treats it like leaves).
    w.set_block(4, h + 5, 4, glass);
    let (_, sl) = w.light_at(4, h + 1, 4);
    assert_eq!(sl, 15, "sky light passes glass");

    // Winter: glass-roofed crops grow at 0.75x; sky-open twins sleep.
    w.day = 3 * crate::world::SEASON_DAYS;
    for x in 0..16 {
        w.set_block(x, h + 6, 4, b("base:farmland"));
        w.set_block(x, h + 7, 4, b("base:wheat_seeds"));
        w.set_block(x, h + 10, 4, glass); // glass roof
        w.set_block(x, h + 6, 12, b("base:farmland"));
        w.set_block(x, h + 7, 12, b("base:wheat_seeds")); // open sky
    }
    let mut rng = 11u32;
    for _ in 0..30_000 {
        w.random_tick(&mut rng);
    }
    let grown = |z: i32| {
        (0..16)
            .filter(|&x| w.get_block(x, h + 7, z) != b("base:wheat_seeds"))
            .count()
    };
    assert!(grown(4) > 0, "the glasshouse grows in winter");
    assert_eq!(grown(12), 0, "open sky still sleeps");
}

#[test]
fn bloomery_multiblock_fires_batches_and_fears_the_rain() {
    use crate::world::{BLOOMERY_FIRE_SECS, BlockEntity, BloomeryState};
    let reg = base_reg();
    let mut w = test_world_with("steel-fire", reg.clone());
    let my = 120; // open sky, far above terrain
    build_bloomery(&mut w, &reg, 10, my, 10);
    assert!(w.check_bloomery(10, my, 10).is_some(), "shell validates");
    // Any missing brick breaches it.
    let fb = reg.block_id("base:firebrick").unwrap();
    w.set_block(11, my + 2, 11, AIR);
    assert!(w.check_bloomery(10, my, 10).is_none(), "breach detected");
    w.set_block(11, my + 2, 11, fb);
    assert!(
        w.check_bloomery(10, my, 10).is_some(),
        "repair re-validates"
    );

    // Charge it full (8 iron + 8 charcoal), light, and fire to the end.
    let iron = reg.item_id("base:iron_ingot").unwrap();
    let coal = reg.item_id("base:charcoal").unwrap();
    let bloom = reg.item_id("base:steel_bloom").unwrap();
    let mut st = BloomeryState::default();
    for i in 0..4 {
        st.charge[i] = Some(ItemStack::new(&reg, iron, 2));
        st.fuel[i] = Some(ItemStack::new(&reg, coal, 2));
    }
    w.insert_block_entity((10, my, 10), BlockEntity::Bloomery(st));
    assert!(w.light_bloomery(10, my, 10).is_ok(), "lights when charged");
    assert_eq!(
        w.get_block(10, my, 10),
        reg.block_id("base:bloomery_lit").unwrap(),
        "the mouth glows"
    );
    // Clear skies: full rate. Fire it through.
    w.force_local_weather("clear");
    let steps = (BLOOMERY_FIRE_SECS / 0.5) as i32 + 4;
    for _ in 0..steps {
        w.tick_entities(0.5);
    }
    let Some(BlockEntity::Bloomery(b)) = w.block_entity(&(10, my, 10)) else {
        panic!("bloomery survived");
    };
    assert!(!b.lit, "the firing ended");
    let blooms: u32 = b
        .charge
        .iter()
        .flatten()
        .filter(|s| s.item == bloom)
        .map(|s| s.count)
        .sum();
    assert_eq!(blooms, 6, "a full 8+8 firing yields 6 blooms");
    assert_eq!(
        w.get_block(10, my, 10),
        reg.block_id("base:bloomery").unwrap(),
        "the mouth cools"
    );

    // A partial 2+2 charge yields a single bloom.
    let mut st = BloomeryState::default();
    st.charge[0] = Some(ItemStack::new(&reg, iron, 2));
    st.fuel[0] = Some(ItemStack::new(&reg, coal, 2));
    w.insert_block_entity((10, my, 10), BlockEntity::Bloomery(st));
    w.light_bloomery(10, my, 10).unwrap();
    for _ in 0..steps {
        w.tick_entities(0.5);
    }
    let Some(BlockEntity::Bloomery(b)) = w.block_entity(&(10, my, 10)) else {
        panic!()
    };
    let blooms: u32 = b
        .charge
        .iter()
        .flatten()
        .filter(|s| s.item == bloom)
        .map(|s| s.count)
        .sum();
    assert_eq!(blooms, 1, "2+2 makes one bloom");

    // Rain halves an unroofed stack; a storm douses it outright.
    let mut st = BloomeryState::default();
    st.charge[0] = Some(ItemStack::new(&reg, iron, 2));
    st.fuel[0] = Some(ItemStack::new(&reg, coal, 2));
    w.insert_block_entity((10, my, 10), BlockEntity::Bloomery(st));
    w.light_bloomery(10, my, 10).unwrap();
    w.force_local_weather("precip");
    for _ in 0..20 {
        w.tick_entities(1.0);
    }
    let Some(BlockEntity::Bloomery(b)) = w.block_entity(&(10, my, 10)) else {
        panic!()
    };
    assert!(
        (b.progress - 10.0).abs() < 0.6,
        "rain fires at half rate, got {}",
        b.progress
    );
    w.force_local_weather("storm");
    w.tick_entities(1.0);
    let Some(BlockEntity::Bloomery(b)) = w.block_entity(&(10, my, 10)) else {
        panic!()
    };
    assert!(!b.lit, "a storm douses the unroofed stack");
    let kept: u32 = b.charge.iter().flatten().map(|s| s.count).sum();
    assert_eq!(kept, 2, "the charge survives a dousing");

    // Roofed, the same rain doesn't slow it. (Cover the core top.)
    let plank = reg.block_id("base:planks").unwrap();
    w.set_block(11, my + 4, 10, plank);
    let Some(BlockEntity::Bloomery(b)) = w.block_entity_mut(&(10, my, 10)) else {
        panic!()
    };
    b.lit = true;
    b.progress = 0.0;
    b.core = Some(bp(11, my, 10));
    w.force_local_weather("precip");
    for _ in 0..10 {
        w.tick_entities(1.0);
    }
    let Some(BlockEntity::Bloomery(b)) = w.block_entity(&(10, my, 10)) else {
        panic!()
    };
    assert!(
        (b.progress - 10.0).abs() < 0.6,
        "a roof keeps the fire honest, got {}",
        b.progress
    );
}

#[test]
fn clamp_smolders_logs_into_charcoal_and_vents() {
    use crate::world::CLAMP_SECS_PER_LOG;
    let reg = base_reg();
    let mut w = test_world_with("steel-clamp", reg.clone());
    let log = reg.block_id("base:log").unwrap();
    let dirt = reg.block_id("base:dirt").unwrap();
    let my = 120;
    // A 2-log pile at (10,my,10)-(11,my,10), fully cased in dirt except
    // one exposed face at (9,my,10).
    for x in 9..=12 {
        for y in my - 1..=my + 1 {
            for z in 9..=11 {
                w.set_block(x, y, z, dirt);
            }
        }
    }
    w.set_block(10, my, 10, log);
    w.set_block(11, my, 10, log);
    w.set_block(9, my, 10, AIR); // the lighting face
    assert_eq!(
        w.try_light_clamp(10, my, 10),
        Ok(2),
        "a covered pile lights"
    );
    // Too exposed fails: open a second face on a fresh pile elsewhere.
    w.set_block(10, my + 4, 10, log);
    w.set_block(11, my + 4, 10, log);
    assert!(
        w.try_light_clamp(10, my + 4, 10).is_err(),
        "an open pile refuses the ember"
    );
    // Burn it down: 2 logs = 2x CLAMP_SECS_PER_LOG.
    let total = 2.0 * CLAMP_SECS_PER_LOG + 5.0;
    let mut t = 0.0;
    while t < total {
        w.tick_entities(2.0);
        t += 2.0;
    }
    let cc = reg.block_id("base:charcoal_block").unwrap();
    assert_eq!(w.get_block(10, my, 10), cc, "logs became charcoal");
    assert_eq!(w.get_block(11, my, 10), cc);
    assert!(!w.has_block_entity(&(10, my, 10)), "the clamp retires");

    // Venting: uncover a burning pile and the exposed log burns away.
    w.set_block(10, my, 10, log);
    w.set_block(11, my, 10, log);
    w.set_block(9, my, 10, AIR);
    assert_eq!(w.try_light_clamp(10, my, 10), Ok(2));
    w.set_block(11, my + 1, 10, AIR); // rip the lid off log 2
    w.tick_entities(0.5);
    assert_eq!(
        w.get_block(11, my, 10),
        AIR,
        "the uncovered log burns to nothing"
    );
}

#[test]
fn anvil_works_blooms_into_bars() {
    use crate::world::BlockEntity;
    let reg = base_reg();
    let mut w = test_world_with("steel-anvil", reg.clone());
    let bloom = reg.item_id("base:steel_bloom").unwrap();
    let ingot = reg.item_id("base:steel_ingot").unwrap();
    let stick = reg.item_id("base:stick").unwrap();
    let pos = (10, 120, 10);
    w.set_block(
        pos.0,
        pos.1,
        pos.2,
        reg.block_id("base:stone_anvil").unwrap(),
    );
    // Only workable items rest on the anvil (iron rests too now —
    // it hammers into plate — so the refusal case is a stick).
    assert!(
        !w.anvil_put(pos, ItemStack::new(&reg, stick, 1)),
        "a stick is not workable"
    );
    assert!(
        w.anvil_put(pos, ItemStack::new(&reg, bloom, 1)),
        "a bloom rests"
    );
    assert!(
        !w.anvil_put(pos, ItemStack::new(&reg, bloom, 1)),
        "one at a time"
    );
    assert!(w.anvil_strike(pos).is_none(), "strike one");
    assert!(w.anvil_strike(pos).is_none(), "strike two");
    let out = w.anvil_strike(pos).expect("strike three finishes");
    assert_eq!(out.item, ingot, "the bloom became a bar");
    let Some(BlockEntity::Anvil(a)) = w.block_entity(&pos) else {
        panic!()
    };
    assert!(a.bloom.is_none() && a.strikes == 0, "the anvil clears");
    // Taking a half-worked bloom resets the count.
    w.anvil_put(pos, ItemStack::new(&reg, bloom, 1));
    w.anvil_strike(pos);
    let back = w.anvil_take(pos).expect("take it back");
    assert_eq!(back.item, bloom);
    w.anvil_put(pos, back);
    assert!(w.anvil_strike(pos).is_none());
    assert!(w.anvil_strike(pos).is_none());
    assert!(
        w.anvil_strike(pos).is_some(),
        "work starts over after a take"
    );
}

#[test]
fn quern_grinds_minerals_and_kiln_colors_glass() {
    use crate::world::{BlockEntity, KILN_FIRE_SECS, KilnState};
    let reg = base_reg();
    let mut w = test_world_with("gw-kiln", reg.clone());
    let b = |n: &str| reg.block_id(n).unwrap();
    let it2 = |n: &str| reg.item_id(n).unwrap();

    // The quern is a station: bare-hand turns, two per chunk, 2 powder out.
    let qpos = (10, 120, 10);
    w.set_block(10, 120, 10, b("base:quern"));
    let bloom = it2("base:steel_bloom");
    assert!(
        !w.anvil_put(qpos, ItemStack::new(&reg, bloom, 1)),
        "blooms don't grind"
    );
    assert!(
        w.anvil_put(qpos, ItemStack::new(&reg, it2("base:raw_cobalt"), 1)),
        "a mineral chunk rests on the quern"
    );
    let def = reg
        .worked
        .iter()
        .find(|d| d.input == it2("base:raw_cobalt"))
        .unwrap();
    assert!(!def.needs_hammer && def.station == "quern" && def.count == 2);
    assert!(w.anvil_strike(qpos).is_none(), "turn one");
    let out = w.anvil_strike(qpos).expect("turn two grinds");
    assert_eq!(out.item, it2("base:cobalt_powder"));
    assert_eq!(out.count, 2, "one chunk, two powders");

    // The kiln shares the bloomery's shell; a bloomery mouth still
    // validates its own and never the kiln's.
    let my = 130;
    build_bloomery(&mut w, &reg, 20, my, 10);
    assert!(w.check_bloomery(20, my, 10).is_some());
    assert!(
        w.check_kiln(20, my, 10).is_none(),
        "wrong mouth, wrong craft"
    );
    w.set_block(20, my, 10, b("base:kiln"));
    assert!(
        w.check_kiln(20, my, 10).is_some(),
        "swap the mouth, get a kiln"
    );

    // 8 sand + 1 cobalt powder + 8 charcoal -> 8 blue glass.
    let mut st = KilnState::default();
    for i in 0..4 {
        st.sand[i] = Some(ItemStack::new(&reg, it2("base:sand"), 2));
        st.fuel[i] = Some(ItemStack::new(&reg, it2("base:charcoal"), 2));
    }
    st.powder = Some(ItemStack::new(&reg, it2("base:cobalt_powder"), 1));
    w.insert_block_entity((20, my, 10), BlockEntity::Kiln(st));
    w.force_local_weather("clear");
    assert!(w.light_kiln(20, my, 10).is_ok());
    assert_eq!(
        w.get_block(20, my, 10),
        b("base:kiln_lit"),
        "the mouth glows white-gold"
    );
    let steps = (KILN_FIRE_SECS / 0.5) as i32 + 4;
    for _ in 0..steps {
        w.tick_entities(0.5);
    }
    let Some(BlockEntity::Kiln(k)) = w.block_entity(&(20, my, 10)) else {
        panic!()
    };
    assert!(!k.lit);
    assert!(
        k.powder.is_none(),
        "the powder colored the batch and is gone"
    );
    let blue: u32 = k
        .sand
        .iter()
        .flatten()
        .filter(|s| s.item == it2("base:blue_glass"))
        .map(|s| s.count)
        .sum();
    assert_eq!(blue, 8, "a full batch of blue glass");

    // No powder = bulk clear glass.
    let mut st = KilnState::default();
    st.sand[0] = Some(ItemStack::new(&reg, it2("base:sand"), 2));
    st.fuel[0] = Some(ItemStack::new(&reg, it2("base:charcoal"), 2));
    w.insert_block_entity((20, my, 10), BlockEntity::Kiln(st));
    w.light_kiln(20, my, 10).unwrap();
    for _ in 0..steps {
        w.tick_entities(0.5);
    }
    let Some(BlockEntity::Kiln(k)) = w.block_entity(&(20, my, 10)) else {
        panic!()
    };
    let clear: u32 = k
        .sand
        .iter()
        .flatten()
        .filter(|s| s.item == it2("base:glass"))
        .map(|s| s.count)
        .sum();
    assert_eq!(clear, 2, "an uncolored batch fires clear");

    // Ore gates: manganese refuses everything under steel.
    let mn = b("base:manganese_ore");
    assert_eq!(reg.block(mn).min_tier, 5, "manganese is steel-gated");
    let iron_pick = reg.item_id("base:iron_pickaxe");
    assert!(
        reg.drops_for(mn, iron_pick).is_none() || reg.block(mn).min_tier > 4,
        "iron picks get nothing from manganese"
    );
    // All three ore bands registered.
    for ore in ["base:cobalt_ore", "base:cinnabar_ore", "base:manganese_ore"] {
        assert!(
            reg.ores.iter().any(|o| o.block == b(ore)),
            "{ore} generates"
        );
    }
}

#[test]
fn cupellation_splits_silver_from_lead() {
    use crate::world::{BlockEntity, FurnaceState};
    let reg = base_reg();
    let mut w = test_world_with("cupel", reg.clone());
    let pos = (2, 80, 2);
    w.set_block(pos.0, pos.1, pos.2, b(&reg, "base:furnace"));
    let charged = it(&reg, "base:charged_crucible");
    let log = it(&reg, "base:log");
    w.insert_block_entity(
        pos,
        BlockEntity::Furnace(FurnaceState {
            input: Some(ItemStack::new(&reg, charged, 1)),
            fuel: Some(ItemStack::new(&reg, log, 2)),
            ..Default::default()
        }),
    );
    for _ in 0..140 {
        w.tick_entities(0.1);
    }
    let Some(BlockEntity::Furnace(f)) = w.block_entity(&pos) else {
        panic!("furnace")
    };
    assert_eq!(
        f.output.unwrap().item,
        it(&reg, "base:silver_ingot"),
        "silver lands in the slot"
    );
    assert!(f.input.is_none(), "the cupel is spent");
    let lead = it(&reg, "base:lead_ingot");
    let spat: u32 = w
        .take_pending_drops()
        .into_iter()
        .filter(|(p, s)| {
            *p == crate::planet::BlockPos::of_world(pos.0, pos.1, pos.2).unwrap() && s.item == lead
        })
        .map(|(_, s)| s.count)
        .sum();
    assert_eq!(spat, 2, "the lead pours out the mouth");
}

#[test]
fn forge_wants_its_whole_workshop() {
    use crate::world::BlockEntity;
    let reg = base_reg();
    let mut w = test_world_with("forge-shape", reg.clone());
    let my = 120;
    build_forge(&mut w, &reg, 10, my, 10);
    assert!(
        w.check_forge(10, my, 10).is_some(),
        "the workshop validates"
    );
    // No anvil, no forge.
    w.set_block(9, my, 10, AIR);
    assert!(w.check_forge(10, my, 10).is_none(), "the anvil is required");
    let anvil = reg.block_id("base:stone_anvil").unwrap();
    w.set_block(9, my, 10, anvil);
    // A breached chimney course kills it too.
    let fb = reg.block_id("base:firebrick").unwrap();
    w.set_block(12, my + 4, 10, AIR);
    assert!(
        w.check_forge(10, my, 10).is_none(),
        "the chimney is required"
    );
    w.set_block(12, my + 4, 10, fb);
    assert!(w.check_forge(10, my, 10).is_some(), "repair re-validates");
    // An uncharged forge refuses the ember.
    w.insert_block_entity((10, my, 10), BlockEntity::Forge(Default::default()));
    assert!(w.light_forge(10, my, 10).is_err(), "empty refuses to light");
}

#[test]
fn forge_batch_smelts_with_thrifty_fuel_in_any_weather() {
    use crate::world::{BlockEntity, BloomeryState, FORGE_FIRE_SECS};
    let reg = base_reg();
    let mut w = test_world_with("forge-fire", reg.clone());
    let my = 120;
    build_forge(&mut w, &reg, 10, my, 10);
    // 8 raw copper + 4 charcoal: the batch smelts every ore and burns
    // one fuel per two items — half what a furnace would ask.
    let raw = reg.item_id("base:raw_copper").unwrap();
    let ingot = reg.item_id("base:copper_ingot").unwrap();
    let coal = reg.item_id("base:charcoal").unwrap();
    let mut st = BloomeryState::default();
    for i in 0..4 {
        st.charge[i] = Some(ItemStack::new(&reg, raw, 2));
    }
    st.fuel[0] = Some(ItemStack::new(&reg, coal, 4));
    st.fuel[1] = Some(ItemStack::new(&reg, coal, 4));
    w.insert_block_entity((10, my, 10), BlockEntity::Forge(st));
    assert!(w.light_forge(10, my, 10).is_ok(), "lights when charged");
    // A storm means nothing to a chimneyed workshop.
    w.force_local_weather("storm");
    let steps = (FORGE_FIRE_SECS / 0.5) as i32 + 4;
    for _ in 0..steps {
        w.tick_entities(0.5);
    }
    let Some(BlockEntity::Forge(f)) = w.block_entity(&(10, my, 10)) else {
        panic!("forge survived");
    };
    assert!(!f.lit, "the firing ended despite the storm");
    assert!(
        f.charge.iter().all(|s| s.is_none()),
        "the whole batch smelted"
    );
    let fuel_left: u32 = f.fuel.iter().flatten().map(|s| s.count).sum();
    assert_eq!(fuel_left, 4, "8 items burned 4 fuel (2 per fuel)");
    let minted: u32 = w
        .pending_drops()
        .iter()
        .filter(|(_, s)| s.item == ingot)
        .map(|(_, s)| s.count)
        .sum();
    assert_eq!(minted, 8, "eight ingots spat at the mouth");
}

#[test]
fn food_spoils_slower_in_a_cellar_and_salted_keeps() {
    use crate::world::{BlockEntity, ChestState};
    let reg = base_reg();
    let mut w = test_world_with("perish", reg.clone());
    let stone = b(&reg, "base:stone");
    let meat = reg.item_id("base:raw_venison").unwrap();
    let salted = reg.item_id("base:salted_meat").unwrap();
    let mush = reg.item_id("base:spoiled_mush").unwrap();
    let chest = b(&reg, "base:chest");
    // A surface chest in daylight and a buried chest in the dark.
    let sy = w.surface_height(4, 4);
    w.set_block(4, sy + 1, 4, chest);
    for dx in -1..=1 {
        for dy in 0..=2 {
            for dz in -1..=1 {
                w.set_block(20 + dx, 40 + dy, 4 + dz, stone);
            }
        }
    }
    w.set_block(20, 41, 4, chest);
    w.relight_and_cascade(crate::chunk::ChunkPos::of_world(20, 4));
    let mut open = ChestState::default();
    open.slots[0] = Some(ItemStack::new(&reg, meat, 4));
    open.slots[1] = Some(ItemStack::new(&reg, salted, 4));
    w.insert_block_entity((4, sy + 1, 4), BlockEntity::Chest(open));
    let mut cellar = ChestState::default();
    cellar.slots[0] = Some(ItemStack::new(&reg, meat, 4));
    w.insert_block_entity((20, 41, 4), BlockEntity::Chest(cellar));
    // Long enough for the open chest's stack to burn its whole
    // freshness at whatever the current rate is, plus a margin. A bare
    // second count here stopped being long enough the moment the
    // calendar retuned FRESHNESS_PER_SEC.
    let full = reg.item(meat).durability as f32;
    let secs = full / crate::world::FRESHNESS_PER_SEC * 1.12;
    for _ in 0..(secs / 20.0).ceil() as u32 {
        w.tick_entities(20.0);
    }
    let surface_meat = match w.block_entity(&(4, sy + 1, 4)) {
        Some(BlockEntity::Chest(c)) => c.slots[0].unwrap(),
        _ => panic!("chest"),
    };
    let cellar_meat = match w.block_entity(&(20, 41, 4)) {
        Some(BlockEntity::Chest(c)) => c.slots[0].unwrap(),
        _ => panic!("cellar chest"),
    };
    assert_eq!(
        surface_meat.item, mush,
        "raw venison rots on the surface once its freshness runs out"
    );
    assert_eq!(cellar_meat.item, meat, "the cellar kept it");
    assert!(
        cellar_meat.durability as f32 >= full * 0.6,
        "cellar decay runs at quarter rate ({} of {full})",
        cellar_meat.durability
    );
    let salted_left = match w.block_entity(&(4, sy + 1, 4)) {
        Some(BlockEntity::Chest(c)) => c.slots[1].unwrap(),
        _ => panic!("chest"),
    };
    assert_eq!(salted_left.item, salted, "salted meat shrugs it off");
}

#[test]
fn legacy_food_stacks_initialize_instead_of_rotting() {
    use crate::world::{BlockEntity, ChestState};
    let reg = base_reg();
    let mut w = test_world_with("legacy-food", reg.clone());
    let berry = reg.item_id("base:berry").unwrap();
    let chest = b(&reg, "base:chest");
    let sy = w.surface_height(4, 4);
    w.set_block(4, sy + 1, 4, chest);
    let mut c = ChestState::default();
    // A pre-freshness save: durability 0 on a perishable.
    c.slots[0] = Some(ItemStack {
        item: berry,
        count: 5,
        durability: 0,
        arcane_id: 0,
    });
    w.insert_block_entity((4, sy + 1, 4), BlockEntity::Chest(c));
    w.tick_entities(20.0);
    let st = match w.block_entity(&(4, sy + 1, 4)) {
        Some(BlockEntity::Chest(c)) => c.slots[0].unwrap(),
        _ => panic!("chest"),
    };
    assert_eq!(st.item, berry, "legacy berries survive the sweep");
    assert_eq!(st.durability, 1200, "and start their clock fresh");
}

#[test]
fn chimneyed_kiln_is_a_glassworks() {
    use crate::world::{BlockEntity, KILN_FIRE_SECS, KilnState};
    let reg = base_reg();
    let mut w = test_world_with("glassworks", reg.clone());
    let my = 120;
    // A kiln stack, then the chimney courses over the core.
    let fb = b(&reg, "base:firebrick");
    let mouth = b(&reg, "base:kiln");
    let (mx, mz) = (10, 10);
    let (cx, cz) = (mx + 1, mz);
    for ly in 0..6 {
        for rx in -1..=1i32 {
            for rz in -1..=1i32 {
                if rx != 0 || rz != 0 {
                    w.set_block(cx + rx, my + ly, cz + rz, fb);
                }
            }
        }
        w.set_block(cx, my + ly, cz, AIR);
    }
    w.set_block(mx, my, mz, mouth);
    assert!(w.check_glassworks(mx, my, mz).is_some(), "chimney upgrades");
    // 8 sand + 2 charcoal: the draft doubles what each fuel fires -
    // four glass where a bare kiln stops at two.
    let sand = reg.item_id("base:sand").unwrap();
    let coal = reg.item_id("base:charcoal").unwrap();
    let mut st = KilnState::default();
    for i in 0..4 {
        st.sand[i] = Some(ItemStack::new(&reg, sand, 2));
    }
    st.fuel[0] = Some(ItemStack::new(&reg, coal, 2));
    w.insert_block_entity((mx, my, mz), BlockEntity::Kiln(st));
    assert!(w.light_kiln(mx, my, mz).is_ok());
    w.force_local_weather("storm"); // and the storm means nothing
    let steps = (KILN_FIRE_SECS / 0.5) as i32 + 4;
    for _ in 0..steps {
        w.tick_entities(0.5);
    }
    let Some(BlockEntity::Kiln(k)) = w.block_entity(&(mx, my, mz)) else {
        panic!("kiln survived");
    };
    assert!(!k.lit, "fired through the storm");
    let glass: u32 = k
        .sand
        .iter()
        .flatten()
        .filter(|s| s.item != sand)
        .map(|s| s.count)
        .sum();
    assert_eq!(glass, 4, "two charcoal fired four glass (double reach)");
    let sand_left: u32 = k
        .sand
        .iter()
        .flatten()
        .filter(|s| s.item == sand)
        .map(|s| s.count)
        .sum();
    assert_eq!(sand_left, 4, "the unfired sand keeps");
}

#[test]
fn signs_hold_their_words_through_save_and_load() {
    use crate::world::{BlockEntity, SignState};
    let reg = base_reg();
    let dir = tmp_dir("signsave");
    {
        let mut w = World::new(42, dir.clone(), reg.clone());
        w.ensure_chunk(tchunk(0, 0));
        let sign = b(&reg, "base:sign");
        let sy = w.surface_height(4, 4);
        w.set_block(4, sy + 1, 4, sign);
        w.insert_block_entity(
            (4, sy + 1, 4),
            BlockEntity::Sign(SignState {
                lines: [
                    "SALT FAIR".to_string(),
                    "PRICES".to_string(),
                    "NE 400".to_string(),
                ],
            }),
        );
        save_world(&mut w);
    }
    let w = World::load_or_create(dir, reg.clone()).unwrap();
    let found: Vec<_> = w.sign_texts().collect();
    assert_eq!(found.len(), 1, "the sign persisted");
    assert_eq!(found[0].1.lines[0], "SALT FAIR");
    assert_eq!(found[0].1.lines[2], "NE 400");
}

#[test]
fn stall_validates_and_persists_its_shop() {
    use crate::world::{BlockEntity, StallState};
    let reg = base_reg();
    let dir = tmp_dir("stall");
    let counter = b(&reg, "base:stall_counter");
    let log = b(&reg, "base:log");
    let planks = b(&reg, "base:planks");
    let salt = it(&reg, "base:salt_crystal");
    let silver = it(&reg, "base:silver_ingot");
    {
        let mut w = World::new(42, dir.clone(), reg.clone());
        w.ensure_chunk(tchunk(0, 0));
        let y = 200;
        w.set_block(4, y, 4, counter);
        assert!(!w.check_stall(4, y, 4), "a bare counter is not a stall");
        // Posts and awning along x.
        for side in [-1i32, 1] {
            w.set_block(4 + side, y, 4, log);
            w.set_block(4 + side, y + 1, 4, log);
        }
        for i in -1i32..=1 {
            w.set_block(4 + i, y + 2, 4, planks);
        }
        assert!(w.check_stall(4, y, 4), "posts + awning make the stall");
        // Break the awning: trading stops.
        w.set_block(4, y + 2, 4, AIR);
        assert!(!w.check_stall(4, y, 4), "no awning, no stall");
        w.set_block(4, y + 2, 4, planks);
        let mut st = StallState {
            owner: [7; 16],
            owner_name: "DOLL".to_string(),
            ..Default::default()
        };
        st.goods[0] = Some(ItemStack::new(&reg, salt, 20));
        st.price = Some(ItemStack::new(&reg, silver, 1));
        w.insert_block_entity((4, y, 4), BlockEntity::Stall(st));
        save_world(&mut w);
    }
    let w = World::load_or_create(dir, reg.clone()).unwrap();
    let Some(BlockEntity::Stall(st)) = w.block_entity(&(4, 200, 4)) else {
        panic!("the stall persisted");
    };
    assert_eq!(st.owner, [7; 16], "ownership survives");
    assert_eq!(st.owner_name, "DOLL");
    assert_eq!(st.goods[0].unwrap().count, 20);
    assert_eq!(st.price.unwrap().item, silver, "the price template holds");
}

#[test]
fn the_smoker_cures_over_a_live_torch() {
    use crate::world::{BlockEntity, SMOKE_SECS, SmokerState};
    let reg = base_reg();
    let mut w = test_world_with("smoker", reg.clone());
    let rack = b(&reg, "base:smoking_rack");
    let torch = b(&reg, "base:torch");
    let stone = b(&reg, "base:stone");
    let y = 200;
    w.set_block(4, y, 4, stone);
    w.set_block(4, y + 1, 4, torch);
    w.set_block(4, y + 2, 4, rack);
    let raw = reg.item_id("base:raw_boar").unwrap();
    let smoked = reg.item_id("base:smoked_meat").unwrap();
    let mut sm = SmokerState::default();
    sm.meat[0] = Some(ItemStack::new(&reg, raw, 1));
    sm.meat[2] = Some(ItemStack::new(&reg, raw, 1));
    w.insert_block_entity((4, y + 2, 4), BlockEntity::Smoker(sm));
    // Cold rack (no torch): nothing cures.
    w.set_block(4, y + 1, 4, crate::registry::AIR);
    for _ in 0..40 {
        w.tick_entities(20.0);
    }
    let Some(BlockEntity::Smoker(sm)) = w.block_entity(&(4, y + 2, 4)) else {
        panic!("rack stands");
    };
    assert_eq!(sm.meat[0].unwrap().item, raw, "cold smoke cures nothing");
    // Relight and let it cure through.
    w.set_block(4, y + 1, 4, torch);
    let steps = (SMOKE_SECS / 20.0) as i32 + 2;
    for _ in 0..steps {
        w.tick_entities(20.0);
    }
    let Some(BlockEntity::Smoker(sm)) = w.block_entity(&(4, y + 2, 4)) else {
        panic!("rack stands");
    };
    assert_eq!(sm.meat[0].unwrap().item, smoked, "the cut cured");
    assert_eq!(sm.meat[2].unwrap().item, smoked, "the whole load together");
    assert_eq!(
        sm.meat[0].unwrap().durability,
        reg.item(smoked).durability,
        "smoked keeps six days"
    );
}

// ---- mechanization: millwork (rung 0-1) ----

/// Raise the standard test water site: a sealed stone basin holding
/// one full cell at (10,119,10), a wheel placed over it, and a breach
/// helper that opens a lip so the pool becomes live water.
fn wheel_over_basin(w: &mut World, reg: &Registry) -> (i32, i32, i32) {
    let stone = b(reg, "base:stone");
    let (wx, wy, wz) = (10, 120, 10);
    for dx in -1..=1 {
        for dz in -1..=1 {
            w.set_block(wx + dx, wy - 2, wz + dz, stone);
            if dx != 0 || dz != 0 {
                w.set_block(wx + dx, wy - 1, wz + dz, stone);
            }
        }
    }
    w.set_block(wx, wy - 1, wz, reg.water_block(0));
    assert!(w.place_block((wx, wy, wz), b(reg, "base:water_wheel")));
    (wx, wy, wz)
}

fn breach_basin(w: &mut World, wheel: (i32, i32, i32)) {
    let (wx, wy, wz) = wheel;
    w.set_block(wx + 1, wy - 1, wz, AIR);
    w.set_block(wx + 1, wy - 2, wz, AIR);
}

#[test]
fn the_wheel_wants_live_water_and_shafts_carry_it() {
    let reg = base_reg();
    let mut w = test_world_with("millwork-power", reg.clone());
    let wheel = wheel_over_basin(&mut w, &reg);
    let (wx, wy, wz) = wheel;
    assert_eq!(
        w.wheel_live(wx, wy, wz),
        0.0,
        "a standing pool turns nothing"
    );
    breach_basin(&mut w, wheel);
    assert!(
        w.wheel_live(wx, wy, wz) > 0.0,
        "a breached lip is live water"
    );
    // Twelve wooden shafts carry the turn; a thirteenth refuses.
    let shaft = b(&reg, "base:shaft");
    for i in 1..=12 {
        w.set_block(wx, wy, wz + i, shaft);
    }
    assert!(w.power_at(wx, wy, wz + 13) > 0.0, "a 12-shaft run works");
    w.set_block(wx, wy, wz + 13, shaft);
    assert_eq!(
        w.power_at(wx, wy, wz + 14),
        0.0,
        "thirteen wooden shafts is one too many"
    );
    // A gear turns the corner that a shaft refuses.
    let gear = b(&reg, "base:gear");
    for i in 1..=13 {
        w.set_block(wx, wy, wz + i, AIR);
    }
    w.set_block(wx, wy, wz + 1, shaft);
    assert_eq!(
        w.power_at(wx + 1, wy, wz + 1),
        0.0,
        "a shaft never bends: nothing comes off its side"
    );
    w.set_block(wx, wy, wz + 2, gear);
    w.set_block(wx + 1, wy, wz + 2, shaft);
    assert!(
        w.power_at(wx + 2, wy, wz + 2) > 0.0,
        "the gear turns the corner"
    );
    // The wheel dresses itself: live water spins it to its run form.
    w.tick_entities(0.3);
    assert_eq!(
        w.get_block(wx, wy, wz),
        b(&reg, "base:water_wheel_run"),
        "a live wheel turns visibly"
    );
}

#[test]
fn multiblock_and_power_cross_a_rotated_planet_seam() {
    use crate::planet::{BlockPos, Direction6, FACE_BLOCKS, Face, step6};

    let reg = base_reg();
    let mut w = test_world_with("machine-seam", reg.clone());
    let y = 120;
    let mouth = BlockPos::new(Face::PosZ, FACE_BLOCKS - 1, y, 4100).unwrap();
    let core = step6(mouth, Direction6::East).unwrap().pos;

    let mut shell = Vec::new();
    for ly in 0..3 {
        for rx in -1..=1 {
            for rz in -1..=1 {
                if rx == 0 && rz == 0 {
                    continue;
                }
                shell.push(core.offset(rx, ly, rz).unwrap());
            }
        }
    }
    let mut chunks: std::collections::BTreeSet<_> = shell.iter().map(|pos| pos.chunk()).collect();
    chunks.insert(core.chunk());
    chunks.insert(mouth.chunk());
    for chunk in chunks {
        w.ensure_chunk(chunk);
    }
    let firebrick = reg.block_id("base:firebrick").unwrap();
    for at in shell {
        w.set_block_at(
            at,
            if at == mouth {
                reg.block_id("base:bloomery").unwrap()
            } else {
                firebrick
            },
        );
    }
    for ly in 0..3 {
        w.set_block_at(core.offset(0, ly, 0).unwrap(), AIR);
    }
    w.set_block_at(mouth, reg.block_id("base:bloomery").unwrap());
    assert_eq!(
        w.check_bloomery_at(mouth),
        Some(core),
        "the firebrick shell validates after its local axes rotate across a seam"
    );

    let source = BlockPos::new(Face::PosZ, FACE_BLOCKS - 2, y + 8, 4108).unwrap();
    let shaft = step6(source, Direction6::East).unwrap().pos;
    let receiver = step6(shaft, step6(source, Direction6::East).unwrap().direction)
        .unwrap()
        .pos;
    for chunk in [source.chunk(), shaft.chunk(), receiver.chunk()] {
        w.ensure_chunk(chunk);
    }
    w.set_block_at(source, reg.block_id("base:water_wheel").unwrap());
    w.set_block_at(shaft, reg.block_id("base:shaft").unwrap());
    let water = source.offset(0, -1, 0).unwrap();
    w.set_block_at(water, reg.water_block(0));
    w.set_block_at(water.offset(0, -1, 0).unwrap(), AIR);
    assert!(
        w.power_at_pos(receiver) > 0.0,
        "a straight shaft remains straight in the rotated destination chart"
    );
}

#[test]
fn shaft_power_crosses_every_directed_planet_seam() {
    use crate::planet::{BlockPos, Direction4, Direction6, step6};

    let reg = base_reg();
    let wheel = b(&reg, "base:water_wheel");
    let shaft_block = b(&reg, "base:shaft");
    let water = reg.water_block(0);
    let mut world = World::new(52, tmp_dir("planet-power-all-seams"), reg);

    for seam in directed_planet_seams() {
        let source =
            BlockPos::new(seam.source.face(), seam.source.u(), 120, seam.source.v()).unwrap();
        let outward = match seam.direction {
            Direction4::East => Direction6::East,
            Direction4::North => Direction6::North,
            Direction4::West => Direction6::West,
            Direction4::South => Direction6::South,
        };
        let first = step6(source, outward).unwrap();
        let shaft = first.pos;
        assert_eq!(shaft.surface(), seam.across);
        let receiver = step6(shaft, first.direction).unwrap().pos;
        let water_pos = source.offset(0, -1, 0).unwrap();
        let drain = water_pos.offset(0, -1, 0).unwrap();
        let missing: std::collections::BTreeSet<_> = [
            source.chunk(),
            shaft.chunk(),
            receiver.chunk(),
            water_pos.chunk(),
        ]
        .into_iter()
        .filter(|chunk| !world.has_chunk(*chunk))
        .collect();
        world.insert_empty_chunks_for_test(missing);

        let fixture_set = |world: &mut World, pos: BlockPos, block| {
            let (x, y, z) = pos.local();
            world
                .chunks_mut()
                .get_mut(&pos.chunk())
                .expect("power fixture chunk")
                .set(x, y, z, block);
        };
        fixture_set(&mut world, source, wheel);
        fixture_set(&mut world, shaft, shaft_block);
        fixture_set(&mut world, receiver, AIR);
        fixture_set(&mut world, water_pos, water);
        fixture_set(&mut world, drain, AIR);

        assert!(
            world.power_at_pos(receiver) > 0.0,
            "shaft power did not remain straight across {:?} {:?}",
            seam.face,
            seam.direction
        );
    }
}

#[test]
fn the_millstone_grinds_a_load_unattended() {
    let reg = base_reg();
    let mut w = test_world_with("millstone-bulk", reg.clone());
    let wheel = wheel_over_basin(&mut w, &reg);
    breach_basin(&mut w, wheel);
    let (wx, wy, wz) = wheel;
    w.set_block(wx, wy, wz + 1, b(&reg, "base:shaft"));
    let mill = (wx, wy, wz + 2);
    w.set_block(mill.0, mill.1, mill.2, b(&reg, "base:millstone"));
    let copper = it(&reg, "base:raw_copper");
    for _ in 0..4 {
        assert!(
            w.anvil_put(mill, ItemStack::new(&reg, copper, 1)),
            "the millstone piles a batch"
        );
    }
    w.clear_pending_drops();
    for _ in 0..12 {
        w.tick_entities(0.5);
    }
    let ground: u32 = w
        .take_pending_drops()
        .into_iter()
        .filter(|(_, s)| s.item == it(&reg, "base:verdigris_powder"))
        .map(|(_, s)| s.count)
        .sum();
    assert_eq!(ground, 8, "four ores grind to eight powder in one firing");
}

#[test]
fn the_sawmill_rips_logs_and_the_helve_works_the_anvil() {
    let reg = base_reg();
    let mut w = test_world_with("sawmill-helve", reg.clone());
    let wheel = wheel_over_basin(&mut w, &reg);
    breach_basin(&mut w, wheel);
    let (wx, wy, wz) = wheel;
    w.set_block(wx, wy, wz + 1, b(&reg, "base:shaft"));
    let saw = (wx, wy, wz + 2);
    w.set_block(saw.0, saw.1, saw.2, b(&reg, "base:sawmill"));
    let log = it(&reg, "base:log");
    for _ in 0..3 {
        assert!(w.anvil_put(saw, ItemStack::new(&reg, log, 1)));
    }
    w.clear_pending_drops();
    for _ in 0..12 {
        w.tick_entities(0.5);
    }
    let planks: u32 = w
        .take_pending_drops()
        .into_iter()
        .filter(|(_, s)| s.item == it(&reg, "base:planks"))
        .map(|(_, s)| s.count)
        .sum();
    assert_eq!(planks, 18, "the sawmill cuts six a log, hands cut four");
    // The helve hammer hangs off a gear (nothing comes off a shaft's
    // side): the anvil's bloom works itself.
    w.set_block(wx, wy, wz + 1, b(&reg, "base:gear"));
    w.set_block(wx + 1, wy, wz + 1, b(&reg, "base:helve_hammer"));
    let anvil = (wx + 2, wy, wz + 1);
    w.set_block(anvil.0, anvil.1, anvil.2, b(&reg, "base:stone_anvil"));
    let bloom = it(&reg, "base:steel_bloom");
    assert!(w.anvil_put(anvil, ItemStack::new(&reg, bloom, 1)));
    w.clear_pending_drops();
    for _ in 0..30 {
        w.tick_entities(0.5);
    }
    assert!(
        w.take_pending_drops()
            .iter()
            .any(|(_, s)| s.item == it(&reg, "base:steel_ingot")),
        "three helve strikes finish the bar with nobody watching"
    );
    assert_eq!(
        w.get_block(wx + 1, wy, wz + 1),
        b(&reg, "base:helve_hammer"),
        "the arm rests when the work is done"
    );
}

// ---- mechanization: the machining age (rung 2) ----

#[test]
fn the_lathes_hold_their_tolerances() {
    let reg = base_reg();
    let mut w = test_world_with("lathe-tolerance", reg.clone());
    let wheel = wheel_over_basin(&mut w, &reg);
    breach_basin(&mut w, wheel);
    let (wx, wy, wz) = wheel;
    w.set_block(wx, wy, wz + 1, b(&reg, "base:gear"));
    let crude = (wx + 1, wy, wz + 1);
    w.set_block(crude.0, crude.1, crude.2, b(&reg, "base:lathe"));
    // Sloppy tolerance turns soft metal only: iron never rests here.
    let iron = it(&reg, "base:iron_ingot");
    assert!(
        !w.anvil_put(crude, ItemStack::new(&reg, iron, 1)),
        "the crude lathe refuses iron"
    );
    let copper = it(&reg, "base:copper_ingot");
    assert!(w.anvil_put(crude, ItemStack::new(&reg, copper, 1)));
    w.clear_pending_drops();
    for _ in 0..16 {
        w.tick_entities(0.5);
    }
    let screws: u32 = w
        .take_pending_drops()
        .into_iter()
        .filter(|(_, s)| s.item == it(&reg, "base:screw"))
        .map(|(_, s)| s.count)
        .sum();
    assert_eq!(screws, 2, "one soft ingot turns two screws");
    // The iron lathe: true tolerance, but only with workholding.
    let precise = (wx - 1, wy, wz + 1);
    w.set_block(precise.0, precise.1, precise.2, b(&reg, "base:iron_lathe"));
    assert!(w.anvil_put(precise, ItemStack::new(&reg, iron, 1)));
    w.clear_pending_drops();
    for _ in 0..16 {
        w.tick_entities(0.5);
    }
    assert!(
        w.take_pending_drops().is_empty(),
        "no vice, no cut: the work only spins"
    );
    w.set_block(
        precise.0,
        precise.1 + 1,
        precise.2 + 1,
        b(&reg, "base:vice"),
    );
    for _ in 0..16 {
        w.tick_entities(0.5);
    }
    assert!(
        w.take_pending_drops()
            .iter()
            .any(|(_, s)| s.item == it(&reg, "base:iron_shaft")),
        "vice held, shaft turned true"
    );
}

#[test]
fn the_bearing_frees_the_wooden_run() {
    let reg = base_reg();
    let mut w = test_world_with("bearing-run", reg.clone());
    let wheel = wheel_over_basin(&mut w, &reg);
    breach_basin(&mut w, wheel);
    let (wx, wy, wz) = wheel;
    let shaft = b(&reg, "base:shaft");
    for i in 1..=13 {
        w.set_block(wx, wy, wz + i, shaft);
    }
    assert_eq!(
        w.power_at(wx, wy, wz + 14),
        0.0,
        "thirteen wooden shafts refuse"
    );
    w.set_block(wx, wy, wz + 7, b(&reg, "base:fitted_shaft"));
    assert!(
        w.power_at(wx, wy, wz + 14) > 0.0,
        "one fitted shaft mid-run and the same line turns"
    );
}

#[test]
fn the_boring_mill_bores_and_the_pump_drains_the_mine() {
    let reg = base_reg();
    let mut w = test_world_with("boring-pump", reg.clone());
    let wheel = wheel_over_basin(&mut w, &reg);
    breach_basin(&mut w, wheel);
    let (wx, wy, wz) = wheel;
    w.set_block(wx, wy, wz + 1, b(&reg, "base:gear"));
    // Only the boring mill cuts cylinders, and only held in a vice.
    let bore = (wx + 1, wy, wz + 1);
    w.set_block(bore.0, bore.1, bore.2, b(&reg, "base:boring_mill"));
    w.set_block(bore.0 + 1, bore.1, bore.2, b(&reg, "base:vice"));
    let plate = it(&reg, "base:plate");
    assert!(
        !w.anvil_put((wx, wy, wz + 1), ItemStack::new(&reg, plate, 1)),
        "a gear is no station"
    );
    assert!(w.anvil_put(bore, ItemStack::new(&reg, plate, 1)));
    w.clear_pending_drops();
    for _ in 0..40 {
        w.tick_entities(0.5);
    }
    assert!(
        w.take_pending_drops()
            .iter()
            .any(|(_, s)| s.item == it(&reg, "base:cylinder")),
        "eight true turns bore the cylinder"
    );
    // The pump: a flooded shaft under it, an open cell beside it.
    let pump = (wx - 1, wy, wz + 1);
    let stone = b(&reg, "base:stone");
    for dy in 1..=4 {
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            w.set_block(pump.0 + dx, pump.1 - dy, pump.2 + dz, stone);
        }
    }
    w.set_block(pump.0, pump.1 - 5, pump.2, stone);
    let full = reg.water_block(0);
    for dy in 2..=4 {
        w.set_block(pump.0, pump.1 - dy, pump.2, full);
    }
    assert!(w.place_block(pump, b(&reg, "base:pump")));
    for _ in 0..40 {
        w.tick_entities(0.5);
    }
    let left = (1..=5)
        .filter(|d| {
            reg.water_volume(w.get_block(pump.0, pump.1 - d, pump.2))
                .is_some()
        })
        .count();
    assert!(
        left < 3,
        "the pump lifts the flood out of the shaft ({left} cells left)"
    );
}

#[test]
fn steam_runs_anywhere_and_stops_hungry() {
    use crate::world::BlockEntity;
    let reg = base_reg();
    let mut w = test_world_with("steam-power", reg.clone());
    // No river anywhere near: firebox, boiler on top, engine beside.
    let (fx, fy, fz) = (10, 120, 10);
    assert!(w.place_block((fx, fy, fz), b(&reg, "base:firebox")));
    w.set_block(fx, fy + 1, fz, b(&reg, "base:boiler"));
    w.set_block(fx + 1, fy + 1, fz, b(&reg, "base:steam_engine"));
    // A shaft line off the engine down to a millstone.
    w.set_block(fx + 2, fy + 1, fz, b(&reg, "base:gear"));
    let mill = (fx + 2, fy, fz);
    w.set_block(mill.0, mill.1, mill.2, b(&reg, "base:millstone"));
    assert_eq!(w.power_at(mill.0, mill.1, mill.2), 0.0, "cold and dry");
    // Bank fire; the boiler drinks the trough cell beside it.
    if let Some(BlockEntity::Steam(s)) = w.block_entity_mut(&(fx, fy, fz)) {
        s.fuel = 60.0;
    }
    w.set_block(fx, fy + 1, fz - 1, reg.water_block(0));
    w.tick_entities(0.5);
    assert_eq!(
        w.get_block(fx, fy + 1, fz - 1),
        AIR,
        "the boiler drank the trough"
    );
    assert!(
        w.power_at(mill.0, mill.1, mill.2) > 1.0,
        "steam drives the line, no river in sight"
    );
    assert_eq!(
        w.get_block(fx, fy, fz),
        b(&reg, "base:firebox_lit"),
        "the door glows while it burns"
    );
    // Starve the fire: the engine stops, the dress reverts.
    if let Some(BlockEntity::Steam(s)) = w.block_entity_mut(&(fx, fy, fz)) {
        s.fuel = 0.0;
    }
    w.tick_entities(0.5);
    assert_eq!(w.power_at(mill.0, mill.1, mill.2), 0.0, "no coal, no steam");
    assert_eq!(w.get_block(fx, fy, fz), b(&reg, "base:firebox"));
}

#[test]
fn the_separator_splits_the_rare_earth_and_the_generator_lights_the_lamp() {
    use crate::world::BlockEntity;
    let reg = base_reg();
    let mut w = test_world_with("electric-age", reg.clone());
    // The separator rides the firebrick stack (kiln pattern).
    let (sx, sy, sz) = (30, 120, 24);
    build_bloomery(&mut w, &reg, sx, sy, sz);
    w.set_block(sx, sy, sz, b(&reg, "base:separator"));
    assert!(w.check_separator(sx, sy, sz).is_some(), "the stack holds");
    w.insert_block_entity(
        (sx, sy, sz),
        BlockEntity::Separator(crate::world::SeparatorState {
            powder: 2,
            fuel: 2,
            ..Default::default()
        }),
    );
    for _ in 0..100 {
        w.tick_entities(0.5);
    }
    let Some(BlockEntity::Separator(sp)) = w.block_entity(&(sx, sy, sz)) else {
        panic!("separator entity")
    };
    assert_eq!(sp.nd, 1, "one neodymium a batch");
    assert_eq!(sp.ce, 2, "cerium is most of the ore - the honest sink");
    // The generator: wheel -> shaft -> generator; its field lights
    // the lamp and turns the electric quern, no shafts to either.
    let wheel = wheel_over_basin(&mut w, &reg);
    breach_basin(&mut w, wheel);
    let (wx, wy, wz) = wheel;
    w.set_block(wx, wy, wz + 1, b(&reg, "base:shaft"));
    let dynamo = (wx, wy, wz + 2);
    assert!(w.place_block(dynamo, b(&reg, "base:generator")));
    let lamp = (wx + 3, wy, wz + 2);
    w.set_block(lamp.0, lamp.1, lamp.2, b(&reg, "base:arc_lamp"));
    let mill = (wx - 2, wy, wz + 2);
    w.set_block(mill.0, mill.1, mill.2, b(&reg, "base:millstone"));
    let copper = it(&reg, "base:raw_copper");
    assert!(w.anvil_put(mill, ItemStack::new(&reg, copper, 1)));
    w.clear_pending_drops();
    for _ in 0..14 {
        w.tick_entities(0.5);
    }
    assert_eq!(
        w.get_block(dynamo.0, dynamo.1, dynamo.2),
        b(&reg, "base:generator_run"),
        "the generator hums on its shaft"
    );
    assert_eq!(
        w.get_block(lamp.0, lamp.1, lamp.2),
        b(&reg, "base:arc_lamp_lit"),
        "light without torches"
    );
    assert!(
        w.take_pending_drops()
            .iter()
            .any(|(_, s)| s.item == it(&reg, "base:verdigris_powder")),
        "the electric quern grinds in the field, no shaft to it"
    );
    // Cut the line: the generator stops and the lamp dies with it.
    w.set_block(wx, wy, wz + 1, AIR);
    for _ in 0..4 {
        w.tick_entities(0.5);
    }
    assert_eq!(
        w.get_block(lamp.0, lamp.1, lamp.2),
        b(&reg, "base:arc_lamp"),
        "a lamp only burns while the shaft turns"
    );
}
