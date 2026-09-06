//! Steam scenarios.

use super::*;

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
        BlockEntity::Multiblock(crate::world::MachineInstance {
            kind: reg.machine_kind("base:separator").unwrap_or_default(),
            powder: 2,
            separator_fuel: 2,
            ..Default::default()
        }),
    );
    for _ in 0..100 {
        w.tick_entities(0.5);
    }
    let Some(BlockEntity::Multiblock(sp)) = w.block_entity(&(sx, sy, sz)) else {
        panic!("separator entity")
    };
    assert_eq!(sp.neodymium, 1, "one neodymium a batch");
    assert_eq!(sp.cerium, 2, "cerium is most of the ore - the honest sink");
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
    clear_machine_outputs(&mut w);
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
        take_machine_outputs(&mut w)
            .iter()
            .any(|stack| stack.item == it(&reg, "base:verdigris_powder")),
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
