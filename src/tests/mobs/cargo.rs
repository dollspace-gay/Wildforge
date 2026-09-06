//! Cargo scenarios.

use super::*;

#[test]
fn saddlebag_cargo_survives_save_and_load() {
    let reg = base_reg();
    let dir = tmp_dir("packmule");
    let deer_i = reg.animal_id("base:deer").unwrap();
    let salt = it(&reg, "base:salt_crystal");
    {
        let mut w = World::new(42, dir.clone(), reg.clone());
        let mut m = crate::mobs::Mob::new(deer_i, Vec3::new(8.5, 220.0, 8.5), 0.0);
        m.health = reg.animals[deer_i].health;
        m.tamed = true;
        m.tame_fed = 4;
        m.tame_need = 4;
        let mut cargo: Box<[Option<ItemStack>; 12]> = Default::default();
        cargo[0] = Some(ItemStack::new(&reg, salt, 30));
        cargo[11] = Some(ItemStack::new(&reg, salt, 2));
        m.cargo = Some(cargo);
        w.spawn_mob(m);
        save_world(&mut w);
    }
    let w = World::load_or_create(dir, reg.clone()).unwrap();
    let m = w
        .mobs()
        .iter()
        .find(|m| m.tamed)
        .expect("the pack deer came back");
    let cargo = m.cargo.as_ref().expect("with its saddlebags");
    assert_eq!(cargo[0].unwrap().count, 30, "the salt rode through");
    assert_eq!(cargo[11].unwrap().count, 2);
    assert_eq!(m.tame_need, 4, "taming state persists");
}

#[test]
fn boats_float_carry_cargo_and_wreck_into_salvage() {
    let reg = base_reg();
    let boat_i = reg.animal_id("base:boat").unwrap();
    assert!(reg.animals[boat_i].vehicle, "the boat is a vehicle");
    assert!(reg.animals[boat_i].carrier, "and takes saddlebags");
    let mut w = test_world("boatfloat");
    // A deep water column: stone floor, six water cells.
    let stone = w.reg.block_id("base:stone").unwrap();
    for x in 6..=10 {
        for z in 6..=10 {
            w.set_block(x, 150, z, stone);
            for y in 151..=156 {
                w.set_block(x, y, z, w.reg.water_block(0));
            }
            for y in 157..200 {
                w.set_block(x, y, z, AIR);
            }
        }
    }
    let mut boat = crate::mobs::Mob::new(boat_i, Vec3::new(8.5, 158.0, 8.5), 0.0);
    boat.health = reg.animals[boat_i].health;
    boat.tamed = true;
    let def = &reg.animals[boat_i];
    let players = [crate::server::PlayerCtx {
        id: 0,
        pos: ep(Vec3::new(50.0, 160.0, 50.0)),
        spawn: ep(Vec3::ZERO),
        attackable: true,
        aggro_mod: 0.0,
        quiet_charm: None,
    }];
    let mut rng = 9u32;
    let mut events = Vec::new();
    for _ in 0..200 {
        boat.tick(&w, def, &players, 0.05, &mut rng, &mut events);
    }
    assert!(
        boat.pos.y > 154.5,
        "the hull bobbed to the surface ({})",
        boat.pos.y
    );
    // A wrecked boat spills its pack — the sweep logic reads cargo,
    // and the drop table returns the hull as lumber.
    assert_eq!(reg.animals[boat_i].drops[0].0, it(&reg, "base:boat"));
}
