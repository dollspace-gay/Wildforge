//! Seasons scenarios.

use super::*;

#[test]
fn offering_stone_values_and_dawn() {
    let reg = base_reg();
    let mut w = test_world("offer");
    let stone = reg.block_id("base:offering_stone").unwrap();
    assert_eq!(reg.block(stone).interaction.as_deref(), Some("offering"));
    assert_eq!(reg.block(stone).light_emit, 5, "faint wildlight");
    assert!(!reg.recipes_for(it(&reg, "base:offering_stone")).is_empty());
    // Value table: the wild's own materials 2.0, meat 1.0, bread hunger*0.25.
    let v = |name: &str, n: u32| w.offering_value(&ItemStack::new(&reg, it(&reg, name), n));
    assert_eq!(v("base:heartwood", 1), 2.0);
    assert_eq!(v("base:raw_venison", 2), 2.0);
    assert!(
        (v("base:bread", 1) - 1.5).abs() < 0.01,
        "bread hunger 6 * 0.25"
    );
    assert_eq!(v("base:oak_sapling", 1), 1.0);
    // Dawn: items taken, refund capped at 10.
    w.ire = 60.0;
    let mut st = crate::world::OfferingState::default();
    st.slots[0] = Some(ItemStack::new(&reg, it(&reg, "base:raw_venison"), 6)); // 6.0
    st.slots[1] = Some(ItemStack::new(&reg, it(&reg, "base:raw_rabbit"), 5)); // 5.0
    w.insert_block_entity((3, 90, 3), crate::world::BlockEntity::Offering(st));
    let r = w.accept_offerings();
    assert!((r - 10.0).abs() < 0.01, "capped at 10, got {r}");
    assert!((w.ire - 50.0).abs() < 0.01);
    let Some(crate::world::BlockEntity::Offering(o)) = w.block_entity(&(3, 90, 3)) else {
        panic!()
    };
    assert!(
        o.slots.iter().all(|s| s.is_none()),
        "the wild took everything"
    );
    assert_eq!(w.accept_offerings(), 0.0, "empty stone gives nothing");
}

#[test]
fn winter_gates_growth_and_freezes_exposed_water() {
    use crate::worldgen::Biome;

    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("wx-winter"), reg.clone());
    let b = |n: &str| reg.block_id(n).unwrap();
    let anchor = find_biome_where(&w.generator, Biome::Plains, |pos| {
        let t = w.generator.climate_at(pos).t;
        let latitude = w.latitude_at_surface(pos);
        let winter_temperature = t * 22.0 + 8.0 - latitude.sin().abs() as f32 * 14.0;
        winter_temperature < -0.5 && w.generator.surface_estimate_at(pos) > SEA_LEVEL + 2
    })
    .expect("seasonally freezing planetary country");
    w.set_calendar_day(local_season_day(&w, anchor, 3));
    ensure_surface_neighborhood(&mut w, anchor, 1);
    let y = 200;

    // A strip of sky-open wheat on farmland never advances in winter...
    let open: Vec<_> = (0..16)
        .map(|du| block_pos(surface_offset(anchor, du - 8, -4), y + 1))
        .collect();
    for &crop in &open {
        w.set_block_at(crop.offset(0, -1, 0).unwrap(), b("base:farmland"));
        w.set_block_at(crop, b("base:wheat_seeds"));
    }
    // ...while a roofed, torchlit one still creeps (the greenhouse).
    let roofed: Vec<_> = (0..16)
        .map(|du| block_pos(surface_offset(anchor, du - 8, 0), y + 1))
        .collect();
    for (index, &crop) in roofed.iter().enumerate() {
        w.set_block_at(crop.offset(0, -1, 0).unwrap(), b("base:farmland"));
        w.set_block_at(crop, b("base:wheat_seeds"));
        w.set_block_at(crop.offset(0, 2, 0).unwrap(), b("base:planks"));
        if index % 3 == 0 {
            let torch = block_pos(surface_offset(crop.surface(), 0, 1), y + 1);
            w.set_block_at(torch.offset(0, -1, 0).unwrap(), b("base:planks"));
            w.set_block_at(torch, b("base:torch"));
        }
    }
    let mut rng = 7u32;
    for _ in 0..1_000 {
        w.set_simulation_clock(w.clock() + 100.0);
        w.random_tick(&mut rng);
    }
    let open_grown = open
        .iter()
        .filter(|&&pos| w.get_block_at(pos) != b("base:wheat_seeds"))
        .count();
    let roofed_grown = roofed
        .iter()
        .filter(|&&pos| {
            let g = w.get_block_at(pos);
            g != b("base:wheat_seeds") && g != AIR
        })
        .count();
    assert_eq!(open_grown, 0, "winter halts sky-open crops");
    assert!(
        roofed_grown > 0,
        "roof + torchlight keeps a greenhouse alive"
    );

    // Exposed still water freezes over in winter...
    let pool: Vec<_> = (0..8)
        .map(|du| block_pos(surface_offset(anchor, du - 4, 6), y + 1))
        .collect();
    for &water in &pool {
        w.set_block_at(water.offset(0, -1, 0).unwrap(), b("base:planks"));
        w.set_block_at(water, reg.water_block(0));
    }
    // (support keeps it a still pool; sky above is open)
    for _ in 0..1_000 {
        w.set_simulation_clock(w.clock() + 100.0);
        w.random_tick(&mut rng);
    }
    let iced = pool
        .iter()
        .filter(|&&pos| w.get_block_at(pos) == b("base:ice"))
        .count();
    let winter_weather = w.weather_at_surface(anchor);
    assert!(
        iced > 0,
        "winter freezes exposed pools, froze {iced} at {:.2} C and latitude {:.1} degrees",
        winter_weather.temperature_c,
        w.latitude_at_surface(anchor).to_degrees()
    );

    // ...and spring gives them back.
    w.set_calendar_day(local_season_day(&w, anchor, 0));
    for _ in 0..1_000 {
        w.set_simulation_clock(w.clock() + 100.0);
        w.random_tick(&mut rng);
    }
    let thawed = pool
        .iter()
        .filter(|&&pos| w.get_block_at(pos) == reg.water_block(0))
        .count();
    assert!(thawed > 0, "spring thaws the ice, thawed {thawed}");
}

#[test]
fn snow_settles_melts_and_snowballs_fly() {
    use crate::worldgen::Biome;
    use glam::Vec3;

    let reg = base_reg();
    let mut w = test_world_with("wx-snow", reg.clone());
    let b = |n: &str| reg.block_id(n).unwrap();
    let layer = b("base:snow_layer");
    assert_eq!(
        reg.block(layer).height,
        Some(0.125),
        "snow layers render thin"
    );

    // Snowfall settles one layer on a cold, sky-open column - once.
    let cold = find_biome(&w.generator, Biome::Arctic).expect("cold land on the planet");
    let temperate = find_biome_where(&w.generator, Biome::Plains, |pos| {
        let t = w.generator.climate_at(pos).t;
        (0.32..=0.5).contains(&t) && w.generator.surface_estimate_at(pos) > SEA_LEVEL + 2
    })
    .expect("temperate land on the planet");
    w.set_calendar_day(local_season_day(&w, cold, 3));
    w.force_local_weather("precip");
    ensure_surface_neighborhood(&mut w, cold, 1);
    ensure_surface_neighborhood(&mut w, temperate, 1);
    let cy = w.surface_height_at(cold);
    let snow = block_pos(cold, cy + 1);
    w.settle_snow_at(cold);
    assert_eq!(
        w.get_block_at(snow),
        layer,
        "snow settled on the cold column"
    );
    w.settle_snow_at(cold);
    assert_eq!(block_at(&w, cold, cy + 2), AIR, "layers never stack");
    let wy = w.surface_height_at(temperate);
    w.settle_snow_at(temperate);
    assert_ne!(
        block_at(&w, temperate, wy + 1),
        layer,
        "temperate columns shrug it off"
    );

    // Torchlight melts layers even in an arctic winter.
    let torch_surface = surface_offset(cold, 1, 0);
    w.set_block_at(block_pos(torch_surface, cy), b("base:stone"));
    w.set_block_at(block_pos(torch_surface, cy + 1), b("base:torch"));
    let mut rng = 9u32;
    for _ in 0..30_000 {
        w.random_tick(&mut rng);
        if w.get_block_at(snow) != layer {
            break;
        }
    }
    assert_eq!(
        reg.water_volume(w.get_block_at(snow)),
        Some(1),
        "bright light turns snow into its exact meltwater"
    );

    // Breaking a snow block yields snowballs; the crafting loop closes.
    assert_eq!(
        reg.block(b("base:snow")).drops,
        Some((reg.item_id("base:snowball").unwrap(), 4))
    );
    let ball = reg.item_id("base:snowball").unwrap();
    assert_eq!(
        reg.item(ball).throw_speed,
        Some(18.0),
        "snowballs are throwable"
    );
    let grid = [Some(ItemStack::new(&reg, ball, 1)); 4];
    let r = crate::crafting::match_recipe(&reg, &grid, 2).expect("4 snowballs pack a block");
    assert_eq!(r.output, reg.item_id("base:snow").unwrap());

    // A zero-damage projectile still shoves: snowball knockback.
    // Staged high in open sky so terrain can't intercept the shot.
    let sy = 140.0;
    let wild = reg.animals.iter().position(|a| !a.hostile).unwrap();
    let mi = w.mob_count();
    let mut m = crate::mobs::Mob::new(wild, Vec3::new(4.5, sy, 4.5), 0.0);
    m.health = 10.0;
    w.spawn_mob(m);
    w.spawn_projectile(crate::mobs::Projectile {
        stable_id: 0,
        pos: ep(Vec3::new(4.5, sy + 0.4, 3.0)),
        vel: Vec3::new(0.0, 0.0, 12.0),
        tile: 0,
        damage: 0.0,
        damage_type: None,
        age: 0.0,
        from_player: true,
        drop_item: None,
        preparation_payload: None,
        owner: 0,
    });
    for _ in 0..60 {
        w.tick_projectiles(&[], 1.0 / 30.0);
    }
    assert_eq!(w.mobs()[mi].health, 10.0, "a snowball draws no blood");
    assert!(
        w.mobs()[mi].hurt_flash > 0.0 || w.mobs()[mi].vel.length() > 0.1,
        "but it definitely lands"
    );

    // Removing a layer's support pops it as a drop.
    let py = w.surface_height(10, 10);
    w.set_block(10, py + 2, 10, b("base:planks"));
    w.set_block(10, py + 3, 10, layer);
    w.clear_pending_drops();
    w.set_block(10, py + 2, 10, AIR);
    assert_eq!(
        w.get_block(10, py + 3, 10),
        AIR,
        "unsupported layers fall away"
    );
    assert!(
        w.pending_drops().iter().any(|(_, s)| s.item == ball),
        "and hand back their snowball"
    );
}

#[test]
fn weather_and_season_touch_the_sim() {
    let reg = base_reg();
    // Winter pauses breeding even for fed adults side by side.
    let mut w = test_world_with("wx-breed", reg.clone());
    let wild = reg
        .animals
        .iter()
        .position(|a| !a.hostile && a.breed_food.is_some())
        .expect("breedable wildlife");
    let stone2 = reg.block_id("base:stone").unwrap();
    for x in 2..=7 {
        for z in 2..=7 {
            w.set_block(x, 139, z, stone2);
        }
    }
    let y = 140.05f32;
    let breeding_surface = ep(glam::Vec3::new(4.5, y, 4.5)).surface();
    w.set_calendar_day(local_season_day(&w, breeding_surface, 3));
    assert_eq!(w.season_at_surface(breeding_surface), 3);
    let before = w.mob_count();
    for dx in 0..2 {
        let mut m = crate::mobs::Mob::new(wild, glam::Vec3::new(4.5 + dx as f32, y, 4.5), 0.0);
        m.health = 10.0;
        m.fed = true;
        w.spawn_mob(m);
    }
    let mut rng = 3u32;
    for _ in 0..120 {
        w.tick_mobs(&[], 1.0, 1.0 / 30.0, &mut rng);
    }
    assert!(
        w.mobs().iter().all(|m| m.growth >= 1.0),
        "no winter litters"
    );
    assert!(w.mob_count() <= before + 2, "no winter births");
    // Summer: the same pair bears young. Winter wander drifts them
    // apart, so stand them back side by side first.
    w.set_calendar_day(local_season_day(&w, breeding_surface, 1));
    assert_eq!(w.season_at_surface(breeding_surface), 1);
    for m in w.mobs_mut() {
        m.fed = true;
        m.breed_cd = 0.0;
    }
    for (moved, m) in w.mobs_mut().iter_mut().enumerate() {
        m.pos = ep(glam::Vec3::new(4.5 + moved as f32, 140.05, 4.5));
        m.vel = glam::Vec3::ZERO;
    }
    let before = w.mob_count();
    for _ in 0..120 {
        w.tick_mobs(&[], 1.0, 1.0 / 30.0, &mut rng);
    }
    assert!(w.mob_count() > before, "summer births arrive");
}

#[test]
fn snow_trod_swaps_persists_melts_and_drops() {
    let reg = base_reg();
    let root = tmp_dir("snow-trod").join("world");
    crate::world::create_world_fixture_atomic(
        &root,
        42,
        "survival",
        8,
        &crate::planet_atlas::CancellationToken::default(),
        |_| {},
    )
    .unwrap();
    let mut w = World::load_or_create(root, reg.clone()).unwrap();
    let layer = b(&reg, "base:snow_layer");
    let trod = b(&reg, "base:snow_layer_trod");
    let dirt = b(&reg, "base:dirt");
    let surface =
        crate::planet::SurfacePos::from_centered(crate::planet::Face::PosZ, 3, 3).unwrap();
    w.ensure_chunk(crate::planet::ChunkPos::from_surface(surface));
    let y = 200;
    let ground = block_pos(surface, y);
    let print = block_pos(surface, y + 1);
    let empty = block_pos(surface, y + 5);
    w.set_block_at(ground, dirt);
    w.set_block_at(print, layer);
    w.set_block_at(empty, AIR);

    // Walking through presses the layer into a print; treading again
    // (or treading air/dirt) changes nothing.
    w.tread_at(print);
    assert_eq!(w.get_block_at(print), trod, "layer pressed to trod");
    w.tread_at(print);
    assert_eq!(w.get_block_at(print), trod, "idempotent");
    w.tread_at(empty);
    assert_eq!(w.get_block_at(empty), AIR, "air stays air");

    // Same shovel yield as fresh snow — the content graph is unmoved.
    assert_eq!(
        reg.drops_for(trod, None),
        reg.drops_for(layer, None),
        "trodden snow drops the same snowball"
    );

    // The trail persists across save/load.
    save_world(&mut w);
    let mut w2 = World::load_or_create(w.save_dir_for_test(), reg.clone()).unwrap();
    w2.ensure_chunk(crate::planet::ChunkPos::from_surface(surface));
    assert_eq!(w2.get_block_at(print), trod, "footprints persist");

    // And melts by the same rule as the untouched layer: torchlight.
    let torch_surface = surface_offset(surface, 1, 0);
    w2.set_block_at(block_pos(torch_surface, y), dirt);
    w2.set_block_at(block_pos(torch_surface, y + 1), b(&reg, "base:torch"));
    let mut rng = 5u32;
    for _ in 0..30_000 {
        w2.random_tick(&mut rng);
        if w2.get_block_at(print) != trod {
            break;
        }
    }
    assert_eq!(
        reg.water_volume(w2.get_block_at(print)),
        Some(1),
        "prints thaw into the same exact meltwater as fresh snow"
    );

    // Guest movement consumes read-only terrain; the host echo owns tread edits.
    let mut wr = ReplicaWorld::new(0, reg.clone(), 0.0);
    let bytes = crate::world::encode_chunk_for_test(&crate::chunk::Chunk::new());
    wr.insert_remote_chunks([(print.chunk(), bytes.as_slice())], &[AIR]);
    wr.apply_remote_block_states([(ground, dirt, 0, 0, 0), (print, layer, 0, 0, 0)]);
    let _ = wr
        .view()
        .standable_at(print.surface(), i32::from(print.y()) + 1);
    assert_eq!(
        wr.get_block_at(print),
        layer,
        "guest reads wait for the echo"
    );
}

#[test]
fn the_stone_states_its_season_and_doubles_it() {
    use crate::world::{BlockEntity, OfferingState, SEASON_DAYS};
    let reg = base_reg();
    let mut w = test_world_with("wants", reg.clone());
    w.set_calendar_day(3 * SEASON_DAYS); // winter: the wild hungers
    let (want, line) = w.season_want();
    assert_eq!(want, 3);
    assert!(line.contains("Food"), "the stone speaks plainly: {line}");
    let bread = it(&reg, "base:bread");
    let stone_ore = it(&reg, "base:raw_copper");
    assert!(
        w.satisfies_want(want, &ItemStack::new(&reg, bread, 1)),
        "bread feeds"
    );
    assert!(
        !w.satisfies_want(want, &ItemStack::new(&reg, stone_ore, 1)),
        "ore does not"
    );
    // A wanted offering credits double, and the stone's own valley
    // remembers the kindness.
    let sy = w.surface_height(4, 4);
    let mut o = OfferingState::default();
    o.slots[0] = Some(ItemStack::new(&reg, bread, 2));
    w.insert_block_entity((4, sy + 1, 4), BlockEntity::Offering(o));
    w.ire = 50.0;
    let refund = w.accept_offerings();
    // bread: hunger 6 -> 1.5 value each, doubled = 3.0 x2 loaves = 6.
    assert!(
        (refund - 6.0).abs() < 0.01,
        "winter bread counts double ({refund})"
    );
    assert!(
        w.regional_ire_at(4, 4) <= -5.9,
        "the valley remembers ({})",
        w.regional_ire_at(4, 4)
    );
    // Out of season the same loaves count single.
    w.set_calendar_day(SEASON_DAYS); // summer wants water, not bread
    let mut o2 = OfferingState::default();
    o2.slots[0] = Some(ItemStack::new(&reg, bread, 2));
    w.insert_block_entity((4, sy + 1, 4), BlockEntity::Offering(o2));
    let refund2 = w.accept_offerings();
    assert!(
        (refund2 - 3.0).abs() < 0.01,
        "unwanted still counts, singly ({refund2})"
    );
}
