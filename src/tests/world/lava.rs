//! Lava scenarios.

use super::*;

#[test]
fn lava_conserves_creeps_and_quenches() {
    let reg = base_reg();
    let mut w = test_world_with("lava", reg.clone());
    let stone = b(&reg, "base:stone");
    let h = w.surface_height(4, 4);
    let y = h + 8;
    // A sealed 3x1 trench holds its lava: conservation + stiffer
    // hysteresis (a full cell spreads, films don't crawl).
    for x in 2..=8 {
        for z in 3..=5 {
            for yy in (y - 1)..=y {
                w.set_block(x, yy, z, stone);
            }
        }
    }
    for x in 4..=6 {
        w.set_block(x, y, 4, AIR);
    }
    w.set_block(4, y, 4, reg.lava_for_volume(8));
    for _ in 0..200 {
        if !w.tick_lava(10_000) {
            break;
        }
    }
    let total: u32 = (4..=6)
        .map(|x| reg.lava_volume(w.get_block(x, y, 4)).unwrap_or(0) as u32)
        .sum();
    assert_eq!(total, 8, "lava volume conserved in the trench");

    // Full lava + water neighbor -> obsidian, water boiled away.
    let (ox, oz) = (12, 12);
    w.set_block(ox, y - 1, oz, stone);
    w.set_block(ox + 1, y - 1, oz, stone);
    for (dx, dz) in [(-1, 0), (0, 1), (0, -1)] {
        w.set_block(ox + dx, y, oz + dz, stone);
        w.set_block(ox + 1 - dx.min(0) * 2, y, oz + dz, stone);
    }
    w.set_block(ox + 2, y, oz, stone);
    w.set_block(ox, y, oz, reg.lava_for_volume(8));
    w.set_block(ox + 1, y, oz, reg.water_block(0));
    for _ in 0..50 {
        w.tick_lava(1_000);
        w.tick_water(1_000);
    }
    assert_eq!(
        w.get_block(ox, y, oz),
        b(&reg, "base:obsidian"),
        "full lava quenched to obsidian"
    );
    assert_eq!(w.get_block(ox + 1, y, oz), AIR, "the water boiled away");

    // Partial lava + water -> basalt.
    let (px, pz) = (2, 12);
    w.set_block(px, y - 1, pz, stone);
    w.set_block(px + 1, y - 1, pz, stone);
    w.set_block(px - 1, y, pz, stone);
    w.set_block(px + 2, y, pz, stone);
    w.set_block(px, y, pz - 1, stone);
    w.set_block(px, y, pz + 1, stone);
    w.set_block(px + 1, y, pz - 1, stone);
    w.set_block(px + 1, y, pz + 1, stone);
    w.set_block(px, y, pz, reg.lava_for_volume(3));
    w.set_block(px + 1, y, pz, reg.water_block(0));
    for _ in 0..50 {
        w.tick_lava(1_000);
        w.tick_water(1_000);
    }
    assert_eq!(
        w.get_block(px, y, pz),
        b(&reg, "base:basalt"),
        "partial lava quenched to basalt"
    );
}

/// A lava flow down a slope has to read as one ribbon, not a row of
/// islands. It used to pour its whole volume over each edge and leave
/// air behind, so what you got was a single cell perched on each step
/// with bare rock between them — a staircase of disconnected blobs.
/// A viscous fluid coats what it runs over: every step the flow
/// crosses stays covered, and diagonal cells share a corner, which is
/// what the mesher's surface smoothing needs to join them into one
/// surface.
#[test]
fn a_lava_flow_coats_the_slope_it_runs_down() {
    let reg = base_reg();
    let mut w = test_world_with("lava-ribbon", reg.clone());
    let stone = b(&reg, "base:stone");
    // Keep the hand-built experiment in guaranteed open shell-space. At the
    // old y=80 the new jungle terrain could occupy the channel and make this
    // a test of the spawn country's canopy instead of lava viscosity.
    let y0 = 220;
    const STEPS: i32 = 10;
    // A staircase descending in +x, two cells deep per tread.
    for step in 0..STEPS {
        let top = y0 - step;
        for x in (step * 2)..(step * 2 + 2) {
            for z in -2..=2 {
                for fill in 0..10 {
                    w.set_block(x, top - fill, z, stone);
                }
                if z.abs() == 2 {
                    w.set_block(x, top + 1, z, stone);
                }
            }
        }
    }
    // A crater's worth behind it: reach is a question of volume.
    for z in -1..=1 {
        for x in 0..2 {
            for up in 1..=3 {
                w.set_block(x, y0 + up, z, reg.lava_for_volume(8));
            }
        }
    }
    for _ in 0..300 {
        w.tick_lava(256);
    }

    // Which treads the flow touched, and how much of each.
    let coated = |step: i32| -> usize {
        let top = y0 - step;
        ((step * 2)..(step * 2 + 2))
            .filter(|&x| reg.is_lava(w.get_block(x, top + 1, -1)))
            .count()
    };
    let reached: Vec<i32> = (0..STEPS).filter(|&s| coated(s) > 0).collect();
    let front = *reached.last().expect("the flow left the crest");
    assert!(
        front >= 5,
        "a crater's worth should run several treads down, got {front}"
    );
    // No bare tread between the vent and the front: that gap IS the
    // jankiness, and it is what the trail exists to close.
    for step in 0..=front {
        assert!(
            coated(step) > 0,
            "tread {step} is bare between the vent and the front at {front}"
        );
    }
    // And each tread is covered across, not perched on its lip — that
    // is what puts a shared corner under every diagonal join.
    let full = (0..=front).filter(|&s| coated(s) == 2).count();
    assert!(
        full * 2 >= (front as usize + 1),
        "most treads should be covered across, {full} of {} are",
        front + 1
    );
}
