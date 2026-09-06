//! Water balance scenarios.

use super::*;

#[test]
fn water_conserves_and_spreads_finite() {
    let reg = base_reg();
    assert_eq!(reg.water_volume(reg.water_block(0)), Some(8));
    assert_eq!(reg.water_volume(reg.water_block(7)), Some(1));
    assert_eq!(reg.water_for_volume(8), reg.water_block(0));
    assert_eq!(reg.water_for_volume(0), AIR);

    let mut w = test_world_with("finitewater", reg.clone());
    let h = w.surface_height(4, 4);
    let y = h + 5;
    let stone = b(&reg, "base:stone");
    for x in -8..=16 {
        for z in -8..=16 {
            w.set_block(x, y - 1, z, stone);
        }
    }
    let before = total_water(&w);
    w.set_block(4, y, 4, reg.water_block(0));
    settle_water(&mut w);
    assert_eq!(total_water(&w), before + 8, "volume neither made nor lost");
    // One cell can't stay full on open ground: it spread into a film.
    assert!(reg.water_volume(w.get_block(4, y, 4)).unwrap_or(0) < 8);
}

#[test]
fn flowing_water_moves_exact_salt_mass_with_deterministic_remainders() {
    let reg = base_reg();
    let mut world = test_world_with("saltwater-flow", reg.clone());
    let y = world.surface_height(4, 4) + 5;
    let stone = b(&reg, "base:stone");
    for x in -4..=12 {
        for z in -4..=12 {
            world.set_block(x, y - 1, z, stone);
        }
    }
    let source = crate::planet::BlockPos::of_world(4, y, 4).unwrap();
    world.set_block_water_at(source, reg.water_block(0), 156, 40_000);
    settle_water(&mut world);
    let mut water_hu = 0u64;
    let mut salt_mass = 0u64;
    for x in -4..=12 {
        for z in -4..=12 {
            let at = crate::planet::BlockPos::of_world(x, y, z).unwrap();
            if let Some(mass) = world.water_mass_at(at) {
                water_hu += mass.water_hu;
                salt_mass += mass.salt_mass;
            }
        }
    }
    assert_eq!(water_hu, 256);
    assert_eq!(salt_mass, 40_000);
}

#[test]
fn water_equalizes_within_the_band() {
    let reg = base_reg();
    let mut w = test_world_with("equalize", reg.clone());
    let h = w.surface_height(4, 4);
    let y = h + 5;
    let stone = b(&reg, "base:stone");
    // A sealed two-cell trench.
    for x in 3..=6 {
        for z in 3..=5 {
            for yy in (y - 1)..=y {
                w.set_block(x, yy, z, stone);
            }
        }
    }
    w.set_block(4, y, 4, AIR);
    w.set_block(5, y, 4, AIR);
    w.set_block(4, y, 4, reg.water_block(0));
    settle_water(&mut w);
    let a = reg.water_volume(w.get_block(4, y, 4)).unwrap_or(0);
    let c = reg.water_volume(w.get_block(5, y, 4)).unwrap_or(0);
    assert_eq!(a + c, 8, "the trench holds all 8 units");
    assert!(a.abs_diff(c) < 2, "levels equalized: {a} vs {c}");
}

#[test]
fn breached_pond_drains_only_what_left() {
    let reg = base_reg();
    let mut w = test_world_with("breach", reg.clone());
    let h = w.surface_height(8, 8);
    let y = h + 6;
    let stone = b(&reg, "base:stone");
    // A platform, a walled basin on it, a full 3x3 pond inside.
    for x in 0..=16 {
        for z in 0..=16 {
            w.set_block(x, y - 1, z, stone);
        }
    }
    for x in 6..=10 {
        for z in 6..=10 {
            if x == 6 || x == 10 || z == 6 || z == 10 {
                w.set_block(x, y, z, stone);
            }
        }
    }
    for x in 7..=9 {
        for z in 7..=9 {
            w.set_block(x, y, z, reg.water_block(0));
        }
    }
    settle_water(&mut w);
    assert_eq!(reg.water_volume(w.get_block(8, y, 8)), Some(8));
    let total = total_water(&w);
    // Breach the rim: the pond genuinely lowers, nothing duplicates.
    w.set_block(10, y, 8, AIR);
    settle_water(&mut w);
    assert_eq!(total_water(&w), total, "no volume created by the breach");
    assert!(
        reg.water_volume(w.get_block(8, y, 8)).unwrap_or(0) < 8,
        "the pond actually dropped"
    );
    assert!(
        (11..=14).any(|x| reg.is_water(w.get_block(x, y, 8))),
        "water escaped through the breach"
    );
}

#[test]
fn water_defers_at_the_worlds_edge() {
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("borderwater"), reg.clone());
    w.ensure_chunk(tchunk(0, 0));
    let stone = b(&reg, "base:stone");
    let y = 250;
    // A shelf against the +x seam, walled on every loaded side.
    w.set_block(15, y - 1, 4, stone);
    w.set_block(14, y, 4, stone);
    w.set_block(15, y, 3, stone);
    w.set_block(15, y, 5, stone);
    w.set_block(15, y, 4, reg.water_block(0));
    for _ in 0..50 {
        w.tick_water(10_000);
    }
    assert_eq!(
        reg.water_volume(w.get_block(15, y, 4)),
        Some(8),
        "water waits at the ungenerated seam instead of vanishing"
    );
    // The neighbor generates: the seam wakes and the flow resumes.
    w.ensure_chunk(tchunk(1, 0));
    let t1 = total_water(&w);
    settle_water(&mut w);
    assert_eq!(total_water(&w), t1, "crossing the seam conserved volume");
    assert!(
        reg.water_volume(w.get_block(15, y, 4)).unwrap_or(0) < 8,
        "the seam wake resumed the flow"
    );
}
