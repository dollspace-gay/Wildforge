//! Water weather scenarios.

use super::*;

#[test]
fn exposed_water_evaporates_without_a_depth_or_basin_exemption() {
    let reg = base_reg();
    let mut w = test_world_with("evap", reg.clone());
    w.set_calendar_day(crate::world::SEASON_DAYS); // summer
    let stone = b(&reg, "base:stone");
    let h = w.surface_height(4, 4);
    let y = h + 8;
    // A walled 2x2 shallow pan, sky open.
    for x in 2..=7 {
        for z in 2..=7 {
            w.set_block(x, y - 1, z, stone);
            w.set_block(x, y, z, stone);
        }
    }
    w.replace_mobs(Vec::new());
    for (x, z) in [(4, 4), (5, 4), (4, 5), (5, 5)] {
        w.set_block(x, y, z, reg.water_block(0));
    }
    // A deep walled shaft: three stacked cells, sky open.
    for x in 10..=12 {
        for z in 3..=5 {
            for yy in (y - 3)..=y {
                w.set_block(x, yy, z, stone);
            }
        }
    }
    for yy in (y - 2)..=y {
        w.set_block(11, yy, 4, reg.water_block(0));
    }
    // An open spill: a lone film on flat ground.
    w.set_block(9, y - 1, 9, stone);
    w.set_block(9, y, 9, reg.water_for_volume(1));
    let mut rng = 11u32;
    for _ in 0..40_000 {
        w.random_tick(&mut rng);
        w.tick_water(1_000);
    }
    let pan_after = [(4, 4), (5, 4), (4, 5), (5, 5)]
        .into_iter()
        .map(|(x, z)| u32::from(reg.water_volume(w.get_block(x, y, z)).unwrap_or(0)))
        .sum::<u32>();
    assert!(pan_after < 32, "the exposed pan lost water to evaporation");
    let shaft_after = ((y - 2)..=y)
        .map(|yy| u32::from(reg.water_volume(w.get_block(11, yy, 4)).unwrap_or(0)))
        .sum::<u32>();
    assert!(
        shaft_after < 24,
        "surface depth is no longer a magical evaporation exemption"
    );
    assert_eq!(w.get_block(9, y, 9), AIR, "open spills dry entirely");
}

#[test]
fn rain_refills_surface_water() {
    let reg = base_reg();
    let mut w = test_world_with("rain", reg.clone());
    w.force_local_weather("rain");
    let stone = b(&reg, "base:stone");
    let h = w.surface_height(4, 4);
    let y = h + 8;
    w.set_block(4, y - 1, 4, stone);
    for (x, z) in [(3, 4), (5, 4), (4, 3), (4, 5)] {
        w.set_block(x, y, z, stone);
    }
    w.set_block(4, y, 4, reg.water_for_volume(2));
    for _ in 0..12 {
        w.rain_fill(4, 4);
    }
    assert_eq!(
        reg.water_volume(w.get_block(4, y, 4)),
        Some(8),
        "rain topped the cell back up to full"
    );
}

#[test]
fn exposed_glaze_and_marsh_films_both_evaporate() {
    let reg = base_reg();
    let mut w = test_world_with("glaze", reg.clone());
    w.set_calendar_day(2 * crate::world::SEASON_DAYS); // autumn: not summer, not winter
    let stone = b(&reg, "base:stone");
    let y = 200;
    // An open film sheet on a flat slab — a drained pool's residue...
    for x in 0..8 {
        for z in 0..8 {
            w.set_block(x, y, z, stone);
        }
    }
    for x in 2..6 {
        for z in 2..6 {
            w.set_block(x, y + 1, z, reg.water_for_volume(1));
        }
    }
    // ...and a solid-walled pocket holding a marsh film.
    for x in 10..13 {
        for z in 0..3 {
            w.set_block(x, y, z, stone);
            w.set_block(x, y + 1, z, stone);
        }
    }
    w.set_block(11, y + 1, 1, reg.water_for_volume(1));
    let mut rng = 7u32;
    for _ in 0..30_000 {
        w.random_tick(&mut rng);
    }
    let sheet: u32 = (2..6)
        .flat_map(|x| (2..6).map(move |z| (x, z)))
        .map(|(x, z)| reg.water_volume(w.get_block(x, y + 1, z)).unwrap_or(0) as u32)
        .sum();
    assert_eq!(sheet, 0, "the open glaze dries away");
    assert_eq!(
        reg.water_volume(w.get_block(11, y + 1, 1)),
        None,
        "walls do not grant a magical exemption from evaporation"
    );
    // And rain can start a pond from nothing in a walled pocket: dry
    // the pocket by hand, then let a shower find it.
    w.set_block(11, y + 1, 1, AIR);
    w.force_local_weather("rain");
    w.rain_fill(11, 1);
    assert_eq!(
        reg.water_volume(w.get_block(11, y + 1, 1)),
        Some(1),
        "rain seeds a film in a dry pothole"
    );
}
