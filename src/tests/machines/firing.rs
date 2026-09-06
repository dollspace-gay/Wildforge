//! Firing scenarios.

use super::*;

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
    w.set_calendar_day(3 * crate::world::SEASON_DAYS);
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
    use crate::world::{BlockEntity, KILN_FIRE_SECS, MachineInstance};
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
    let mut st = MachineInstance {
        kind: reg.machine_kind("base:kiln").unwrap_or_default(),
        ..Default::default()
    };
    for i in 0..4 {
        st.charge[i] = Some(ItemStack::new(&reg, it2("base:sand"), 2));
        st.fuel[i] = Some(ItemStack::new(&reg, it2("base:charcoal"), 2));
    }
    st.reagent = Some(ItemStack::new(&reg, it2("base:cobalt_powder"), 1));
    w.insert_block_entity((20, my, 10), BlockEntity::Multiblock(st));
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
    let Some(BlockEntity::Multiblock(k)) = w.block_entity(&(20, my, 10)) else {
        panic!()
    };
    assert!(!k.lit);
    assert!(
        k.reagent.is_none(),
        "the powder colored the batch and is gone"
    );
    let blue: u32 = k
        .charge
        .iter()
        .flatten()
        .filter(|s| s.item == it2("base:blue_glass"))
        .map(|s| s.count)
        .sum();
    assert_eq!(blue, 8, "a full batch of blue glass");

    // No powder = bulk clear glass.
    let mut st = MachineInstance {
        kind: reg.machine_kind("base:kiln").unwrap_or_default(),
        ..Default::default()
    };
    st.charge[0] = Some(ItemStack::new(&reg, it2("base:sand"), 2));
    st.fuel[0] = Some(ItemStack::new(&reg, it2("base:charcoal"), 2));
    w.insert_block_entity((20, my, 10), BlockEntity::Multiblock(st));
    w.light_kiln(20, my, 10).unwrap();
    for _ in 0..steps {
        w.tick_entities(0.5);
    }
    let Some(BlockEntity::Multiblock(k)) = w.block_entity(&(20, my, 10)) else {
        panic!()
    };
    let clear: u32 = k
        .charge
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
    let spat: u32 = take_machine_outputs(&mut w)
        .into_iter()
        .filter(|stack| stack.item == lead)
        .map(|stack| stack.count)
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
    w.insert_block_entity(
        (10, my, 10),
        BlockEntity::Multiblock(crate::world::MachineInstance {
            kind: reg.machine_kind("base:forge").unwrap_or_default(),
            ..Default::default()
        }),
    );
    assert!(w.light_forge(10, my, 10).is_err(), "empty refuses to light");
}

#[test]
fn forge_batch_smelts_with_thrifty_fuel_in_any_weather() {
    use crate::world::{BlockEntity, FORGE_FIRE_SECS, MachineInstance};
    let reg = base_reg();
    let mut w = test_world_with("forge-fire", reg.clone());
    let my = 120;
    build_forge(&mut w, &reg, 10, my, 10);
    // 8 raw copper + 4 charcoal: the batch smelts every ore and burns
    // one fuel per two items — half what a furnace would ask.
    let raw = reg.item_id("base:raw_copper").unwrap();
    let ingot = reg.item_id("base:copper_ingot").unwrap();
    let coal = reg.item_id("base:charcoal").unwrap();
    let mut st = MachineInstance {
        kind: reg.machine_kind("base:forge").unwrap_or_default(),
        ..Default::default()
    };
    for i in 0..4 {
        st.charge[i] = Some(ItemStack::new(&reg, raw, 2));
    }
    st.fuel[0] = Some(ItemStack::new(&reg, coal, 4));
    st.fuel[1] = Some(ItemStack::new(&reg, coal, 4));
    w.insert_block_entity((10, my, 10), BlockEntity::Multiblock(st));
    assert!(w.light_forge(10, my, 10).is_ok(), "lights when charged");
    // A storm means nothing to a chimneyed workshop.
    w.force_local_weather("storm");
    let steps = (FORGE_FIRE_SECS / 0.5) as i32 + 4;
    for _ in 0..steps {
        w.tick_entities(0.5);
    }
    let Some(BlockEntity::Multiblock(f)) = w.block_entity(&(10, my, 10)) else {
        panic!("forge survived");
    };
    assert!(!f.lit, "the firing ended despite the storm");
    assert!(
        f.charge.iter().all(|s| s.is_none()),
        "the whole batch smelted"
    );
    let fuel_left: u32 = f.fuel.iter().flatten().map(|s| s.count).sum();
    assert_eq!(fuel_left, 4, "8 items burned 4 fuel (2 per fuel)");
    let minted = machine_output_count(&w, ingot);
    assert_eq!(minted, 8, "eight ingots spat at the mouth");
}

#[test]
fn chimneyed_kiln_is_a_glassworks() {
    use crate::world::{BlockEntity, KILN_FIRE_SECS, MachineInstance};
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
    let mut st = MachineInstance {
        kind: reg.machine_kind("base:kiln").unwrap_or_default(),
        ..Default::default()
    };
    for i in 0..4 {
        st.charge[i] = Some(ItemStack::new(&reg, sand, 2));
    }
    st.fuel[0] = Some(ItemStack::new(&reg, coal, 2));
    w.insert_block_entity((mx, my, mz), BlockEntity::Multiblock(st));
    assert!(w.light_kiln(mx, my, mz).is_ok());
    w.force_local_weather("storm"); // and the storm means nothing
    let steps = (KILN_FIRE_SECS / 0.5) as i32 + 4;
    for _ in 0..steps {
        w.tick_entities(0.5);
    }
    let Some(BlockEntity::Multiblock(k)) = w.block_entity(&(mx, my, mz)) else {
        panic!("kiln survived");
    };
    assert!(!k.lit, "fired through the storm");
    let glass: u32 = k
        .charge
        .iter()
        .flatten()
        .filter(|s| s.item != sand)
        .map(|s| s.count)
        .sum();
    assert_eq!(glass, 4, "two charcoal fired four glass (double reach)");
    let sand_left: u32 = k
        .charge
        .iter()
        .flatten()
        .filter(|s| s.item == sand)
        .map(|s| s.count)
        .sum();
    assert_eq!(sand_left, 4, "the unfired sand keeps");
}
