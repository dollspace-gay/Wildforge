//! Workshops scenarios.

use super::*;

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
    clear_machine_outputs(&mut w);
    for _ in 0..12 {
        w.tick_entities(0.5);
    }
    let ground: u32 = take_machine_outputs(&mut w)
        .into_iter()
        .filter(|stack| stack.item == it(&reg, "base:verdigris_powder"))
        .map(|stack| stack.count)
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
    clear_machine_outputs(&mut w);
    for _ in 0..12 {
        w.tick_entities(0.5);
    }
    let planks: u32 = take_machine_outputs(&mut w)
        .into_iter()
        .filter(|stack| stack.item == it(&reg, "base:planks"))
        .map(|stack| stack.count)
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
    clear_machine_outputs(&mut w);
    for _ in 0..30 {
        w.tick_entities(0.5);
    }
    assert!(
        take_machine_outputs(&mut w)
            .iter()
            .any(|stack| stack.item == it(&reg, "base:steel_ingot")),
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
    clear_machine_outputs(&mut w);
    for _ in 0..16 {
        w.tick_entities(0.5);
    }
    let screws: u32 = take_machine_outputs(&mut w)
        .into_iter()
        .filter(|stack| stack.item == it(&reg, "base:screw"))
        .map(|stack| stack.count)
        .sum();
    assert_eq!(screws, 2, "one soft ingot turns two screws");
    // The iron lathe: true tolerance, but only with workholding.
    let precise = (wx - 1, wy, wz + 1);
    w.set_block(precise.0, precise.1, precise.2, b(&reg, "base:iron_lathe"));
    assert!(w.anvil_put(precise, ItemStack::new(&reg, iron, 1)));
    clear_machine_outputs(&mut w);
    for _ in 0..16 {
        w.tick_entities(0.5);
    }
    assert!(
        take_machine_outputs(&mut w).is_empty(),
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
        take_machine_outputs(&mut w)
            .iter()
            .any(|stack| stack.item == it(&reg, "base:iron_shaft")),
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
    clear_machine_outputs(&mut w);
    for _ in 0..40 {
        w.tick_entities(0.5);
    }
    assert!(
        take_machine_outputs(&mut w)
            .iter()
            .any(|stack| stack.item == it(&reg, "base:cylinder")),
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
