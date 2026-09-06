//! Fliers scenarios.

use super::*;

/// A flier's cruise height is measured from the ground *under it*.
/// Reading the world's surface height instead told a bat thirty blocks
/// underground to climb into daylight, so it spent its whole life
/// pressed into the cave roof — which from below looks exactly like a
/// bat hovering in one spot doing nothing.
#[test]
fn a_flier_cruises_over_the_floor_beneath_it_not_the_worlds_surface() {
    let reg = base_reg();
    let mut w = test_world_with("cave-cruise", reg.clone());
    let stone = b(&reg, "base:stone");
    // A tall hall, deep down: floor at 10, roof at 34, so the cruise
    // height falls comfortably inside it.
    for x in 2..=14 {
        for z in 2..=14 {
            for y in [10, 34] {
                w.set_block(x, y, z, stone);
            }
            for y in 11..34 {
                w.set_block(x, y, z, AIR);
            }
        }
    }
    let bat_si = reg.animal_id("base:bat").unwrap();
    w.spawn_mob(beast(&reg, "base:bat", glam::Vec3::new(8.5, 12.0, 8.5)));
    let mut rng = 61u32;
    for _ in 0..400 {
        w.tick_mobs(&[], 0.0, 0.05, &mut rng);
    }
    let bat = w.mobs().iter().find(|m| m.species == bat_si).expect("bat");
    assert!(
        bat.pos.y > 14.0,
        "it climbed off the cave floor (y {:.1})",
        bat.pos.y
    );
    assert!(
        bat.pos.y < 32.0,
        "and did not grind itself against the roof (y {:.1})",
        bat.pos.y
    );
}

/// Wings do not loiter. Before this a flier idled in mid-air for
/// seconds at a time, wandered at a grazer's amble, and read open water
/// as a landfolk's wall — so a gull over the sea turned back at once
/// and hung there. All three showed up as "it just hovers".
#[test]
fn a_flier_crosses_ground_instead_of_hanging_in_the_air() {
    let reg = base_reg();
    let mut w = test_world_with("gull-cruise", reg.clone());
    let water = reg.water_block(0);
    let stone = b(&reg, "base:stone");
    // A little sea to cross: the shoreline veto is what pinned it.
    for x in -8..=8 {
        for z in -8..=8 {
            w.set_block(x, SEA_LEVEL - 2, z, stone);
            w.set_block(x, SEA_LEVEL - 1, z, water);
            w.set_block(x, SEA_LEVEL, z, water);
        }
    }
    let gull_si = reg.animal_id("base:gull").unwrap();
    let start = glam::Vec3::new(0.5, SEA_LEVEL as f32 + 6.0, 0.5);
    w.spawn_mob(beast(&reg, "base:gull", start));
    let mut rng = 67u32;
    let mut travelled = 0.0f32;
    let mut last = start;
    for _ in 0..600 {
        w.tick_mobs(&[], 1.0, 0.05, &mut rng);
        let g = w.mobs().iter().find(|m| m.species == gull_si).unwrap();
        travelled += (g.pos.local() - last).length();
        last = g.pos.local();
    }
    // Thirty seconds of flight. A cruising gull covers well over a
    // block a second; the old idle-heavy wander barely managed a third
    // of this even when it wasn't stalled at the water's edge.
    assert!(
        travelled > 40.0,
        "the gull actually flew somewhere ({travelled:.1} blocks in 30s)"
    );
}

/// The wings had no animation at all: only a box literally named "leg"
/// was ever rotated, so every bird in the game glided with its wings
/// nailed out flat.
#[test]
fn wings_beat_and_wingless_animals_hold_still() {
    let reg = base_reg();
    let lum = ([1.0f32; 3], 1.0f32);
    let frame = |name: &str, phase: f32| {
        let mut m = beast(&reg, name, glam::Vec3::ZERO);
        m.anim_phase = phase;
        let (mut v, mut i) = (Vec::new(), Vec::new());
        m.emit(&reg, lum, &mut v, &mut i);
        v.iter().map(|x| x.pos).collect::<Vec<_>>()
    };
    let down = frame("base:gull", 0.0);
    let up = frame("base:gull", std::f32::consts::FRAC_PI_2);
    assert_eq!(down.len(), up.len(), "same model, same box count");
    assert!(
        down.iter().zip(&up).any(|(a, b)| a != b),
        "the wing moved between the top and the bottom of the beat"
    );
    // A standing animal with no wings is identical at any phase, so
    // the difference above is the beat and not the gait.
    let a = frame("base:deer", 0.0);
    let b2 = frame("base:deer", std::f32::consts::FRAC_PI_2);
    assert_eq!(a, b2, "a standing deer does not animate");
}
