//! Inventory, crafting, physics, survival, and player interaction behavior.

use super::*;

#[test]
fn raycast_hits_nonsolid_plants() {
    let reg = base_reg();
    let mut w = test_world_with("plantray", reg.clone());
    let h = w.surface_height(0, 0);
    let bush = b(&reg, "base:berry_bush");
    w.set_block(3, h + 5, 0, bush);
    let hit = raycast(&w, Vec3::new(0.5, h as f32 + 5.5, 0.5), Vec3::X, 6.0)
        .expect("plants must be targetable");
    assert_eq!(hit.block, (3, h + 5, 0));
    assert!(!reg.is_solid(bush), "bush stays non-solid for physics");
}

#[test]
fn raycast_hits_placed_block_with_correct_adjacent() {
    let reg = base_reg();
    let mut w = test_world_with("ray", reg.clone());
    let h = w.surface_height(0, 0);
    let y = h + 10;
    w.set_block(5, y, 0, b(&reg, "base:stone"));
    let hit = raycast(&w, Vec3::new(0.5, y as f32 + 0.5, 0.5), Vec3::X, 10.0).unwrap();
    assert_eq!(hit.block, (5, y, 0));
    assert_eq!(hit.adjacent, (4, y, 0));
    assert!(raycast(&w, Vec3::new(0.5, y as f32 + 0.5, 0.5), Vec3::X, 4.0).is_none());
}

#[test]
fn player_falls_lands_and_jumps() {
    let mut w = test_world("fall");
    // A built platform in open sky: the plate map decides where the
    // sea is, so physics tests bring their own ground.
    let reg = w.reg.clone();
    let stone = reg.block_id("base:stone").unwrap();
    for x in 2..=6 {
        for z in 2..=6 {
            w.set_block(x, 119, z, stone);
        }
    }
    let h = 119;
    let mut p = Player::new(Vec3::new(4.5, h as f32 + 6.0, 4.5));
    let idle = Input {
        forward: 0.0,
        strafe: 0.0,
        jump: false,
        sprint: false,
    };
    for _ in 0..300 {
        p.update(&w, &idle, Vec3::Z, Vec3::X, 1.0 / 60.0);
    }
    assert!(p.on_ground);
    let ground = p.pos.y;
    let jump = Input { jump: true, ..idle };
    let mut peak = ground;
    for _ in 0..60 {
        p.update(&w, &jump, Vec3::Z, Vec3::X, 1.0 / 60.0);
        peak = peak.max(p.pos.y);
    }
    let gain = peak - ground;
    assert!(gain > 1.0 && gain < 2.0, "jump height {gain}");
}

#[test]
fn inventory_and_clicks() {
    let reg = base_reg();
    let dirt = it(&reg, "base:dirt");
    let stone = it(&reg, "base:stone");
    let pick = it(&reg, "base:wood_pickaxe");
    let mut inv = Inventory::new();
    assert_eq!(inv.add(&reg, dirt, 70), 0);
    assert_eq!(inv.slots[0].unwrap().count, 64);
    assert_eq!(inv.slots[1].unwrap().count, 6);
    inv.add(&reg, pick, 1);
    inv.add(&reg, pick, 1);
    assert_eq!(inv.slots[2].unwrap().count, 1);
    assert_eq!(inv.slots[3].unwrap().count, 1, "tools must not stack");
    // Wear the tool out.
    let uses = reg.item(pick).durability;
    for _ in 0..uses {
        inv.wear_tool(&reg, 2);
    }
    assert!(inv.slots[2].is_none(), "tool breaks at zero durability");
    // Click matrix.
    let d = |n| Some(ItemStack::new(&reg, dirt, n));
    let s = |n| Some(ItemStack::new(&reg, stone, n));
    assert_eq!(click_stack(&reg, d(5), None, false), (None, d(5)));
    assert_eq!(click_stack(&reg, d(60), d(10), false), (d(64), d(6)));
    assert_eq!(click_stack(&reg, s(3), d(5), false), (d(5), s(3)));
    assert_eq!(click_stack(&reg, None, d(5), true), (d(1), d(4)));
    assert_eq!(click_stack(&reg, d(5), None, true), (d(2), d(3)));
}

#[test]
fn crafting_matches_shapes_and_grids() {
    let reg = base_reg();
    let planks = it(&reg, "base:planks");
    let stick = it(&reg, "base:stick");
    let cobble = it(&reg, "base:cobblestone");
    let grid = |size: usize, cells: &[(usize, crate::registry::ItemId)]| {
        let mut g = vec![None; size * size];
        for &(i, item) in cells {
            g[i] = Some(ItemStack::new(&reg, item, 1));
        }
        g
    };
    // Log -> planks in 2x2.
    let log = it(&reg, "base:log");
    let g = grid(2, &[(3, log)]);
    let r = crate::crafting::match_recipe(&reg, &g, 2).expect("log->planks");
    assert_eq!((r.output, r.count), (planks, 4));
    // Pickaxe in 3x3, not in 2x2.
    let g = grid(
        3,
        &[
            (0, planks),
            (1, planks),
            (2, planks),
            (4, stick),
            (7, stick),
        ],
    );
    assert_eq!(
        crate::crafting::match_recipe(&reg, &g, 3).unwrap().output,
        it(&reg, "base:wood_pickaxe")
    );
    let g2 = grid(2, &[(0, planks), (1, planks), (2, stick)]);
    assert!(crate::crafting::match_recipe(&reg, &g2, 2).is_none());
    // Mirrored axe.
    let g = grid(
        3,
        &[
            (0, cobble),
            (1, cobble),
            (4, cobble),
            (3, stick),
            (6, stick),
        ],
    );
    assert_eq!(
        crate::crafting::match_recipe(&reg, &g, 3).unwrap().output,
        it(&reg, "base:stone_axe")
    );
    // Mod recipe works too (loaded in data_mod test above; here just consume).
    let mut g = grid(2, &[(0, planks), (1, planks)]);
    crate::crafting::consume(&mut g);
    assert!(g[0].is_none() && g[1].is_none());
}

#[test]
fn armor_reduction_curve() {
    assert_eq!(crate::reduced_damage(10.0, 0), 10.0);
    assert!(
        (crate::reduced_damage(10.0, 7) - 7.2).abs() < 0.01,
        "full leather: 28%"
    );
    assert!(
        (crate::reduced_damage(10.0, 11) - 5.6).abs() < 0.01,
        "full bronze: 44%"
    );
    assert!(
        (crate::reduced_damage(10.0, 50) - 4.0).abs() < 0.001,
        "capped at 60%"
    );
}

#[test]
fn player_arrows_strike_mobs_and_stick_in_walls() {
    let reg = base_reg();
    let mut w = test_world("arrows");
    let deer_i = reg.animal_id("base:deer").unwrap();
    let mut deer = crate::mobs::Mob::new(deer_i, Vec3::new(8.5, 220.0, 8.5), 0.0);
    deer.health = 10.0;
    let di = w.mob_count(); // natural wildlife is seeded too — track ours
    w.spawn_mob(deer);
    let arrow_item = it(&reg, "base:arrow");
    // Arrow flying at the deer: hits through the normal hurt path.
    w.spawn_projectile(crate::mobs::Projectile {
        pos: Vec3::new(8.5, 220.5, 5.0),
        vel: Vec3::new(0.0, 0.5, 18.0),
        tile: 0,
        damage: 6.0,
        age: 0.0,
        from_player: true,
        drop_item: Some(arrow_item),
        owner: 0,
    });
    let far = Vec3::new(300.0, 80.0, 300.0);
    let mut player_dmg = 0.0;
    for _ in 0..40 {
        player_dmg += w
            .tick_projectiles(
                &[crate::server::PlayerCtx {
                    pos: far,
                    spawn: Vec3::ZERO,
                    attackable: true,
                    aggro_mod: 0.0,
                }],
                1.0 / 30.0,
            )
            .iter()
            .map(|(_, d)| d)
            .sum::<f32>();
    }
    assert!(
        w.mobs()[di].health < 10.0,
        "arrow connected (health {})",
        w.mobs()[di].health
    );
    assert_eq!(player_dmg, 0.0, "player arrows never hit the player");
    assert!(w.pending_drops().is_empty(), "flesh hits consume the arrow");
    // Arrow into a wall drops a recoverable arrow item.
    let stone = reg.block_id("base:stone").unwrap();
    for y in 220..226 {
        w.set_block(2, y, 20, stone);
    }
    w.spawn_projectile(crate::mobs::Projectile {
        pos: Vec3::new(2.5, 222.5, 16.0),
        vel: Vec3::new(0.0, 0.0, 16.0),
        tile: 0,
        damage: 6.0,
        age: 0.0,
        from_player: true,
        drop_item: Some(arrow_item),
        owner: 0,
    });
    for _ in 0..40 {
        w.tick_projectiles(
            &[crate::server::PlayerCtx {
                pos: far,
                spawn: Vec3::ZERO,
                attackable: true,
                aggro_mod: 0.0,
            }],
            1.0 / 30.0,
        );
    }
    assert!(
        w.pending_drops().iter().any(|(_, s)| s.item == arrow_item),
        "wall hit dropped the arrow for recovery"
    );
}

#[test]
fn footstep_materials_map_by_surface() {
    use crate::audio::{BreakMat, StepMat, step_mat};
    // Specials win on name alone.
    assert_eq!(step_mat("base:snow", BreakMat::Soft), StepMat::Snow);
    assert_eq!(step_mat("base:snow_layer", BreakMat::Leafy), StepMat::Snow);
    assert_eq!(step_mat("base:sand", BreakMat::Soft), StepMat::Loose);
    assert_eq!(step_mat("base:gravel", BreakMat::Soft), StepMat::Loose);
    // Everything else follows the break-material family.
    assert_eq!(step_mat("base:stone", BreakMat::Stone), StepMat::Stone);
    assert_eq!(step_mat("base:planks", BreakMat::Wood), StepMat::Wood);
    assert_eq!(step_mat("base:dirt", BreakMat::Soft), StepMat::Soft);
    assert_eq!(step_mat("base:leaves", BreakMat::Leafy), StepMat::Leafy);

    // And the real registry agrees on the interesting surfaces.
    let reg = base_reg();
    for (name, want) in [
        ("base:snow", StepMat::Snow),
        ("base:snow_layer", StepMat::Snow),
        ("base:sand", StepMat::Loose),
    ] {
        let b = reg.block_id(name).expect(name);
        let fallback = match reg.block(b).tool {
            Some(crate::registry::ToolKind::Pickaxe) => BreakMat::Stone,
            Some(crate::registry::ToolKind::Axe) => BreakMat::Wood,
            Some(crate::registry::ToolKind::Shovel) => BreakMat::Soft,
            _ => BreakMat::Leafy,
        };
        assert_eq!(step_mat(&reg.block(b).name, fallback), want, "{name}");
    }
}

#[test]
fn pickup_ramp_steps_and_caps() {
    use crate::audio::pickup_pitch;
    assert_eq!(pickup_pitch(0), 1.0);
    // Monotonic rise, one near-semitone per step.
    for s in 0..7 {
        let step = pickup_pitch(s + 1) / pickup_pitch(s);
        assert!((step - 2.0f32.powf(1.0 / 12.0)).abs() < 1e-4);
    }
    // Caps at +7 semitones no matter the streak.
    assert_eq!(pickup_pitch(7), pickup_pitch(100));
    assert!((pickup_pitch(7) - 2.0f32.powf(7.0 / 12.0)).abs() < 1e-4);
}
