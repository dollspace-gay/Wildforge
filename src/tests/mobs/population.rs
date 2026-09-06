//! Population scenarios.

use super::*;

#[test]
fn wildlife_seeds_matching_biomes_only() {
    let reg = base_reg();
    let mut w = test_world_with("mobseed", reg.clone());
    // Sweep a wide area; every spawned mob must belong to its chunk's biome.
    for cx in -12..12 {
        for cz in -12..12 {
            w.ensure_chunk(tchunk(cx, cz));
        }
    }
    for m in w.mobs() {
        let def = &reg.animals[m.species];
        // The dark rolls independently of the surface: bats belong
        // to whatever chunk has a cave, not to its biome.
        if def.biomes.iter().any(|b| b == "underground") {
            continue;
        }
        // The group roll uses the chunk-center biome; members may scatter a
        // few blocks over a biome edge, which is fine.
        let cp = ChunkPos::of_world(m.pos.x.floor() as i32, m.pos.z.floor() as i32);
        let country = w
            .generator
            .biome(cp.centered_u() * 16 + 8, cp.centered_v() * 16 + 8)
            .name()
            .to_lowercase();
        // The water rolls on its own key, so salt-water natives are
        // checked against the sea and not against the coast behind it.
        let ocean = "ocean".to_string();
        let ok = def.biomes.contains(&country)
            || (def.biomes.contains(&ocean)
                && w.is_open_water(cp.centered_u() * 16 + 8, cp.centered_v() * 16 + 8));
        assert!(ok, "{} rolled in {country} chunk", def.name);
        assert!(m.health > 0.0, "spawned alive");
    }
    assert!(w.mob_count() <= crate::world::MOB_CAP);
}

#[test]
fn mob_persistence_round_trips_and_skips_unknown() {
    let reg = base_reg();
    let dir = tmp_dir("mobsave");
    let mut w = World::new(11, dir.clone(), reg.clone());
    let si = reg.animal_id("base:goat").unwrap();
    let mut m = crate::mobs::Mob::new(si, Vec3::new(3.5, 90.0, -2.5), 1.25);
    m.health = 7.0;
    w.spawn_mob(m);
    save_world(&mut w);
    // Unknown species entries (removed mod) skip cleanly on load.
    let extra = "\n[[mob]]\nspecies = \"gone:wolf\"\nface = 4\nu = 4096.0\ny = 80.0\nv = 4096.0\nyaw = 0\nhealth = 5\n";
    let path = dir.join("animals.toml");
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push_str(extra);
    std::fs::write(&path, text).unwrap();

    let w2 = World::load_or_create(dir, reg.clone()).unwrap();
    assert_eq!(w2.mob_count(), 1, "goat loaded, unknown skipped");
    let g = &w2.mobs()[0];
    assert_eq!(g.species, si);
    assert_eq!(g.health, 7.0);
    assert!((g.pos - Vec3::new(3.5, 90.0, -2.5)).length() < 0.01);
    assert!((g.yaw - 1.25).abs() < 0.01);
}

#[test]
fn npc_companion_persistence_round_trips() {
    let reg = base_reg();
    let dir = tmp_dir("npcsave");
    let mut w = World::new(13, dir.clone(), reg.clone());
    // Spawn the elder via the normal seam; save and reload.
    let def_idx = reg.npc_id("base:elder").expect("base elder registers");
    let at = ep(Vec3::new(3.5, 90.0, -2.5));
    let mob_id = w.spawn_npc_at(def_idx, at).expect("npc fits caps");
    assert!(
        w.npc_by_mob(mob_id).is_some(),
        "instance exists after spawn"
    );
    save_world(&mut w);

    let w2 = World::load_or_create(dir, reg.clone()).unwrap();
    assert_eq!(w2.npc_count(), 1, "npc instance restored on load");
    let npc = w2.npcs().first().expect("restored instance");
    assert_eq!(
        npc.mob_id, mob_id,
        "instance links to the same companion mob"
    );
    assert_eq!(npc.def, def_idx);
    assert_eq!(npc.dialogue.as_deref(), Some("base:elder"));
    let companion = w2
        .mobs()
        .iter()
        .find(|m| m.id == npc.mob_id)
        .expect("companion mob restored");
    assert_eq!(companion.species, reg.npcs[def_idx].species);
    assert!((companion.pos - Vec3::new(3.5, 90.0, -2.5)).length() < 0.01);
}

#[test]
fn wildlife_seed_marks_persist() {
    let reg = base_reg();
    let dir = tmp_dir("mobmark");
    let mut w = World::new(5, dir.clone(), reg.clone());
    w.ensure_chunk(tchunk(0, 0));
    let first = w
        .mobs()
        .iter()
        .filter(|m| {
            let def = &reg.animals[m.species];
            !def.hostile && !def.movement_swim
        })
        .count();
    save_world(&mut w);
    // Reload: regenerating the same chunk must NOT reroll wildlife.
    let mut w2 = World::load_or_create(dir, reg).unwrap();
    w2.ensure_chunk(tchunk(0, 0));
    assert_eq!(
        w2.mob_count(),
        first,
        "seeded mark survives; saved land wildlife is not duplicated on revisit"
    );
}

#[test]
fn mod_can_add_species() {
    let root = tmp_dir("modanimal");
    let dir = root.join("fauna");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mod.toml"), "id = \"fauna\"\nworld_api = 2\n").unwrap();
    std::fs::write(
        dir.join("animals.toml"),
        r#"
[[animal]]
id = "shadow_cat"
biomes = ["forest"]
health = 6
tex = "@deer"
drops = [{ item = "base:hide", min = 1, max = 1 }]
"#,
    )
    .unwrap();
    let reg = registry::load(&root);
    let si = reg
        .animal_id("fauna:shadow_cat")
        .expect("mod species registers");
    let def = &reg.animals[si];
    assert!(!def.model.is_empty(), "default model filled in");
    assert_eq!(def.drops.len(), 1, "drop resolved to base:hide");
}

#[test]
fn mob_ids_stamped_unique_and_yaw_lerps_short_arc() {
    // Interpolation turns the short way around the circle.
    let y = crate::mobs::lerp_yaw(0.1, std::f32::consts::TAU - 0.1, 0.5);
    assert!(
        y.abs() < 0.01 || (y - std::f32::consts::TAU).abs() < 0.01,
        "short arc, got {y}"
    );

    let reg = base_reg();
    let mut w = test_world_with("mobids", reg.clone());
    let wild = reg
        .animals
        .iter()
        .position(|a| !a.hostile)
        .expect("wildlife exists");
    let sy = w.surface_height(4, 4) as f32 + 1.0;
    for i in 0..3 {
        let mut m = crate::mobs::Mob::new(wild, Vec3::new(4.5 + i as f32, sy, 4.5), 0.0);
        m.health = 5.0;
        w.spawn_mob(m);
    }
    let mut rng = 1u32;
    w.tick_mobs(&[], 1.0, 0.05, &mut rng);
    assert!(
        w.mobs().iter().all(|m| m.id > 0),
        "every mob stamped with an id"
    );
    let mut ids: Vec<u32> = w.mobs().iter().map(|m| m.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), w.mob_count(), "ids unique");
}
