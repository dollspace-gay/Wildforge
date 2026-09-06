//! Selection scenarios.

use super::*;

// ---- Phase 8: structure-aware targeting and break/place ----

/// Direct DDA against a structure's block store in local space.
#[test]
fn flat_cast_detects_structure_cell() {
    let rc = base_reg();
    let mut w = test_world_with("flat-cast-struct", rc.clone());
    let tpl = make_single_abs(&mut w, &rc);
    let id = w
        .spawn_structure(&tpl, bp_abs(TU + 10, MY, TV), Rotation::R0)
        .expect("spawn");
    let s = w.local_structure(id).unwrap();
    let hit = crate::raycast::cast(
        Vec3::new(-4.5, 0.5, 0.5),
        Vec3::X,
        15.0,
        |p| s.blocks.get(&p).copied().unwrap_or(AIR),
        |b| b != AIR,
    );
    assert!(
        hit.is_some(),
        "flat cast must hit the structure cell at (0,0,0)"
    );
    assert_eq!(hit.unwrap().block, (0, 0, 0));
}

#[test]
fn raycast_target_at_tracks_a_structure_in_the_void() {
    use crate::raycast::raycast_target_at;
    let rc = base_reg();
    let mut w = test_world_with("target-void-struct", rc.clone());
    spawn_single_abs(&mut w, &rc, 5);
    let origin = origin_abs(4096.0 + 0.5, MY as f32 + 0.5, 4096.0 + 0.5);
    assert!(raycast_target_at(&w, origin, Vec3::X, 15.0).is_some());
}

#[test]
fn raycast_target_at_returns_structure_when_closer() {
    use crate::raycast::{TargetHit, raycast_target_at};
    let rc = base_reg();
    let mut w = test_world_with("target-struct-closer", rc.clone());
    w.set_block_at(bp_abs(4096 + 10, MY, TV), b(&rc, "base:stone"));
    spawn_single_abs(&mut w, &rc, 5);
    let origin = origin_abs(4096.0 + 0.5, MY as f32 + 0.5, TV as f32 + 0.5);
    let aim = raycast_target_at(&w, origin, Vec3::X, 15.0).expect("aim");
    assert!(matches!(aim, TargetHit::Structure { .. }));
}

#[test]
fn raycast_target_at_returns_world_when_closer() {
    use crate::raycast::{TargetHit, raycast_target_at};
    let rc = base_reg();
    let mut w = test_world_with("target-world-closer", rc.clone());
    w.set_block_at(bp_abs(4096 + 5, MY, TV), b(&rc, "base:stone"));
    spawn_single_abs(&mut w, &rc, 12);
    let origin = origin_abs(4096.0 + 0.5, MY as f32 + 0.5, TV as f32 + 0.5);
    let aim = raycast_target_at(&w, origin, Vec3::X, 15.0).unwrap();
    match aim {
        TargetHit::World(h) => assert_eq!(h.block, bp_abs(4096 + 5, MY, TV)),
        other => panic!("expected World, got {other:?}"),
    }
}

#[test]
fn raycast_target_at_hits_structure_only() {
    use crate::raycast::{TargetHit, raycast_target_at};
    let rc = base_reg();
    let mut w = test_world_with("target-struct-only", rc.clone());
    spawn_single_abs(&mut w, &rc, 8);
    let origin = origin_abs(4096.0 + 0.5, MY as f32 + 0.5, TV as f32 + 0.5);
    let aim = raycast_target_at(&w, origin, Vec3::X, 15.0).unwrap();
    assert!(matches!(aim, TargetHit::Structure { .. }));
}

#[test]
fn raycast_target_at_out_of_range() {
    use crate::raycast::raycast_target_at;
    let rc = base_reg();
    let mut w = test_world_with("target-struct-far", rc.clone());
    spawn_single_abs(&mut w, &rc, 30);
    let origin = origin_abs(4096.0 + 0.5, MY as f32 + 0.5, TV as f32 + 0.5);
    assert!(raycast_target_at(&w, origin, Vec3::X, 15.0).is_none());
}
