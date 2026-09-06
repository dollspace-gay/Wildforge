//! Fire scenarios.

use super::*;

/// The wild's fire renews what it takes, and refuses to set foot on
/// ground a player has worked. That invariant already governed
/// lightning; fire inherits it whole.
#[test]
fn the_wilds_fire_pays_bloom_and_stops_at_worked_ground() {
    let (mut w, reg, y) = kindling("fire-wild");
    assert!(w.light_fire(2, y + 1, 2, false), "lightning takes");
    burn(&mut w, 60);
    assert!(w.bloom_at(2, 2) > 0.0, "a natural burn leaves regrowth");
    assert_eq!(w.regional_ire_at(2, 2), 0.0, "and costs the player nothing");
    assert!(
        reg.block_id("base:charred_soil") == Some(w.get_block(2, y, 2)),
        "it chars the ground it cleared"
    );

    // Now claim the ground and try again: the wild will not light it.
    let (mut w, _, y) = kindling("fire-wild-stop");
    w.player_touched.insert(tchunk(0, 0));
    assert!(
        !w.light_fire(2, y + 1, 2, false),
        "the wild does not burn what you built on"
    );
}

/// Your fire in the wild is arson: ire for every wild thing it eats,
/// and no bloom at all. Burn it yourself and the ground gives you ash
/// rather than renewal — the exploit that closes.
#[test]
fn your_fire_in_the_wild_costs_ire_and_pays_no_bloom() {
    let (mut w, _reg, y) = kindling("fire-arson");
    assert!(w.light_fire(2, y + 1, 2, true), "a striker takes anywhere");
    burn(&mut w, 60);
    assert!(
        w.regional_ire_at(2, 2) > 0.0,
        "the country holds it against you"
    );
    assert_eq!(
        w.bloom_at(2, 2),
        0.0,
        "and gives back nothing: ash, not renewal"
    );
}

/// Burning your own field on your own ground is agriculture, not
/// arson, and the wild has no opinion about it.
#[test]
fn burning_your_own_crop_on_your_own_ground_is_husbandry() {
    let reg = base_reg();
    let mut w = test_world_with("fire-stubble", reg.clone());
    let farm = b(&reg, "base:farmland");
    let crop = reg
        .block_id("base:wheat_seeds")
        .expect("wheat is a crop block");
    assert!(reg.block(crop).burns > 0, "a field is easy to lose");
    for x in 0..6 {
        for z in 0..6 {
            let y = w.surface_height(x, z);
            w.set_block(x, y, z, farm);
            w.set_block(x, y + 1, z, crop);
        }
    }
    // Worked ground: in play, planting a field marks it. set_block is
    // the raw poke that does not, so say it outright.
    w.player_touched.insert(tchunk(0, 0));
    let y = w.surface_height(2, 2);
    assert!(w.light_fire(2, y + 2, 2, true));
    burn(&mut w, 60);
    assert_eq!(
        w.regional_ire_at(2, 2),
        0.0,
        "your own stubble is nobody's business"
    );
}

/// A fire remembers whose it is all the way down the hill. Without
/// that, an arsonist lights a fire on their own field and lets it walk
/// into the forest to collect the bloom.
#[test]
fn guilt_is_inherited_by_spread() {
    let (mut w, _reg, y) = kindling("fire-inherit");
    assert!(w.light_fire(0, y + 1, 0, true));
    burn(&mut w, 120);
    // It reached across the patch. Counted in canopy, not in charred
    // ground: fire climbs, and a crown fire eats the leaves while the
    // grass under it stays green.
    let leaves = w.reg.block_id("base:leaves");
    let left = (0..8)
        .flat_map(|x| (0..8).map(move |z| (x, z)))
        .filter(|&(x, z)| leaves == Some(w.get_block(x, y + 2, z)))
        .count();
    assert!(left < 60, "the fire ran ({left} of 64 leaf cells left)");
    // ...and every cell it reached is still on the arsonist's account.
    assert_eq!(w.bloom_at(6, 6), 0.0, "no bloom anywhere it went");
}

#[test]
fn fire_spreads_across_a_real_planet_face_seam() {
    use crate::planet::BlockPos;

    let reg = base_reg();
    let crop = b(&reg, "base:wheat_seeds");
    let burns = reg.block(crop).burns;

    for (index, seam) in directed_planet_seams().into_iter().enumerate() {
        let mut world = World::new(
            44,
            tmp_dir(&format!("planet-fire-seam-{index}")),
            reg.clone(),
        );
        let flame =
            BlockPos::new(seam.source.face(), seam.source.u(), 100, seam.source.v()).unwrap();
        let fuel =
            BlockPos::new(seam.across.face(), seam.across.u(), 100, seam.across.v()).unwrap();

        world.insert_empty_chunks_for_test([flame.chunk(), fuel.chunk()]);
        world.set_block_at(fuel, crop);
        assert!(world.light_fire_at(flame, true));

        // Pick a deterministic predecessor whose next fire roll catches.
        let mut rng = (0..10_000u32)
            .find(|candidate| {
                let next = candidate
                    .wrapping_mul(1_664_525)
                    .wrapping_add(1_013_904_223);
                (next >> 16) % 10 < u32::from(burns)
            })
            .unwrap();
        assert!(world.tick_fire(1, &mut rng));
        assert_eq!(
            reg.block(world.get_block_at(fuel)).name,
            "base:fire",
            "fire did not cross {:?} {:?}",
            seam.face,
            seam.direction
        );
    }
}
