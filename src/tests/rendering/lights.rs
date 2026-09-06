//! Lights scenarios.

use super::*;

#[test]
fn light_promotion_scores_and_hysteresis() {
    use crate::lights::{Key, promote};
    let (a, b, c) = (
        Key::Block(crate::planet::BlockPos::of_world(0, 1, 0).unwrap()),
        Key::Block(crate::planet::BlockPos::of_world(1, 1, 0).unwrap()),
        Key::Block(crate::planet::BlockPos::of_world(2, 1, 0).unwrap()),
    );

    // Empty slots fill best-first.
    let s = promote(&[None, None], &[(a, 1.0), (b, 3.0), (c, 2.0)], 2);
    assert_eq!(s, vec![Some(b), Some(c)]);

    // A marginally better challenger does NOT evict (hysteresis)...
    let s2 = promote(&s, &[(a, 2.2), (b, 3.0), (c, 2.0)], 2);
    assert_eq!(s2, vec![Some(b), Some(c)], "1.1x is not decisive");
    // ...a decisive one does, and takes the weakest slot.
    let s3 = promote(&s, &[(a, 2.6), (b, 3.0), (c, 2.0)], 2);
    assert_eq!(s3, vec![Some(b), Some(a)], "1.3x evicts the weakest");

    // Vanished candidates free their slot in place; slot order is stable
    // (slots are cube-map layers — stability is the cache).
    let s4 = promote(&s3, &[(a, 2.6)], 2);
    assert_eq!(s4, vec![None, Some(a)]);
}

#[test]
fn light_director_caches_until_an_edit_lands_nearby() {
    use crate::lights::{Director, DynLight, Emitter, Key};
    let mut d = Director::new();
    let torch_pos = crate::planet::BlockPos::of_world(4, 64, 4).unwrap();
    let torch = Emitter {
        pos: torch_pos,
        rgb: [14, 11, 6],
        emit: 14,
    };
    d.chunk_meshed(tchunk(0, 0), vec![torch]);
    let cam = crate::planet::block_to_render(torch_pos.surface().center(), 64.5).as_vec3();

    // Steady state: same key, same epoch -> the renderer skips all six
    // cube faces. (Flicker moves the color, never the epoch.)
    let l1 = d.frame(cam, &[], 0.016, true);
    let l2 = d.frame(cam, &[], 0.016, true);
    assert_eq!(l1.len(), 1, "torch promoted");
    assert_eq!((l1[0].key, l1[0].epoch), (l2[0].key, l2[0].epoch));
    assert!(l1[0].suppress.0 > 0.0, "static lights suppress their flood");

    // An edit in a far chunk leaves the cube cached...
    d.chunk_meshed(tchunk(8, 8), vec![]);
    let l3 = d.frame(cam, &[], 0.016, true);
    assert_eq!(l3[0].epoch, l2[0].epoch, "far edits don't invalidate");
    // ...an edit within range invalidates it.
    d.chunk_meshed(tchunk(0, 0), vec![torch]);
    let l4 = d.frame(cam, &[], 0.016, true);
    assert!(l4[0].epoch > l3[0].epoch, "near edits re-render the cube");

    // Dynamic lights: standing still is a cache hit; moving is not.
    let held = |p: Vec3| DynLight {
        key: Key::Held,
        pos: p,
        color: Vec3::new(1.8, 1.4, 0.7),
        range: 16.0,
    };
    let h1 = d.frame(cam, &[held(cam)], 0.016, true);
    let h2 = d.frame(cam, &[held(cam + Vec3::new(0.05, 0.0, 0.0))], 0.016, true);
    let (e1, e2) = (h1[1].epoch, h2[1].epoch);
    assert_eq!(e1, e2, "sub-threshold movement keeps the cube");
    assert_eq!(h2[1].suppress.0, 0.0, "dynamic lights aren't in the flood");
    let h3 = d.frame(cam, &[held(cam + Vec3::new(1.0, 0.0, 0.0))], 0.016, true);
    assert!(h3[1].epoch > e2, "real movement re-renders");

    // A world edit near a STANDING-STILL held light re-renders its
    // cube too — the bug report was shadows of walls no longer there,
    // resetting only once the player wandered past the threshold.
    let hp = cam + Vec3::new(1.0, 0.0, 0.0);
    d.chunk_meshed(tchunk(0, 0), vec![torch]);
    let h4 = d.frame(cam, &[held(hp)], 0.016, true);
    assert!(
        h4[1].epoch > h3[1].epoch,
        "a nearby remesh invalidates a still held light's cube"
    );
    // And the far chunk still doesn't.
    d.chunk_meshed(tchunk(8, 8), vec![]);
    let h5 = d.frame(cam, &[held(hp)], 0.016, true);
    assert_eq!(h5[1].epoch, h4[1].epoch, "far edits leave it cached");
}

#[test]
fn config_lights_and_darkness_roundtrip() {
    use crate::config::Config;
    let d = Config::default();
    assert_eq!(d.lights, 2, "full shadows by default");
    assert!(d.stark, "stark by default");

    let c = Config {
        lights: 1,
        stark: false,
        ..Default::default()
    };
    let c2 = Config::from_text(&c.to_text());
    assert_eq!(c2.lights, 1);
    assert!(!c2.stark);

    let c3 = Config::from_text("lights=off\ndarkness=soft\noutline=off\n");
    assert_eq!(c3.lights, 0);
    assert!(!c3.stark);
    assert!(!c3.outline, "outline=off persists");
    assert!(Config::default().outline, "outline defaults on");
    let c4 = Config::from_text("lights=banana\ndarkness=???\n");
    assert_eq!(c4.lights, 2, "unknown value falls back to full");
    assert!(c4.stark, "unknown darkness falls back to stark");

    // Point-light shadow technique: the exact voxel-grid march is the default;
    // only an explicit `cube` opts back into the legacy distance cube map.
    assert!(d.point_grid, "grid point shadows by default");
    let c5 = Config {
        point_grid: false,
        ..Default::default()
    };
    assert!(
        !Config::from_text(&c5.to_text()).point_grid,
        "point_shadows=cube survives a round trip"
    );
    assert!(!Config::from_text("point_shadows=cube\n").point_grid);
    assert!(
        Config::from_text("point_shadows=wobble\n").point_grid,
        "unknown value falls back to grid"
    );
    assert!(
        Config::from_text("volume=0.5\n").point_grid,
        "a config predating the setting gets grid"
    );
}
