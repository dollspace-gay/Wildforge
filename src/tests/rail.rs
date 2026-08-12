//! Unit tests for rail geometry & path-following motion (spec Part 2.2).

use std::collections::HashMap;

use super::*;
use crate::planet::Direction4;
use crate::world::local_structure::{LocalStructureId, RailState};
use crate::world::multiblock::Rotation;
use crate::world::rail::interpolated_pose;
use crate::world::rail::{CurveOrientation, RailKind};

const MY: i32 = 120;

fn rail_car(w: &mut World, rc: &Registry) -> crate::world::template::Template {
    w.set_block_at(bp(0, MY, 0), b(rc, "base:firebrick"));
    w.capture_and_save(bp(0, MY, 0), bp(0, MY, 0), "car")
        .expect("captured");
    w.template("car").cloned().expect("car template")
}

fn lay(w: &mut World, rc: &Registry, name: &str, cells: &[(i32, i32, i32)]) {
    let block = b(rc, name);
    for &(u, y, v) in cells {
        w.set_block_at(bp(u, y, v), block);
    }
}

fn onto(
    w: &mut World,
    id: LocalStructureId,
    from: (i32, i32, i32),
    to: (i32, i32, i32),
    speed: f32,
) {
    assert!(w.set_rail(
        id,
        Some(RailState {
            current_cell: bp(from.0, from.1, from.2),
            next_cell: bp(to.0, to.1, to.2),
            progress: 0.0,
            speed,
        })
    ));
}

#[test]
fn rail_kind_classifies_base_blocks() {
    let rc = base_reg();
    let cases = [
        ("base:rail", Some(RailKind::Straight)),
        ("base:rail_switch", Some(RailKind::Switch)),
        (
            "base:rail_curve_ne",
            Some(RailKind::Curve(CurveOrientation::NE)),
        ),
        (
            "base:rail_curve_nw",
            Some(RailKind::Curve(CurveOrientation::NW)),
        ),
        ("base:rail_incline_n", Some(RailKind::Incline)),
        ("base:stone", None),
    ];
    for (name, expected) in cases {
        let block = b(&rc, name);
        assert_eq!(RailKind::from_block(&rc, block), expected, "{name}");
    }
}

#[test]
fn straight_track_moves_one_cell_per_second_and_reorients() {
    let rc = base_reg();
    let mut w = test_world_with("rail-straight", rc.clone());
    lay(
        &mut w,
        &rc,
        "base:rail",
        &[(0, MY, 0), (0, MY, 1), (0, MY, 2), (0, MY, 3)],
    );
    let tpl = rail_car(&mut w, &rc);
    let id = w
        .spawn_structure(&tpl, bp(0, MY, 0), Rotation::R0)
        .expect("spawns");
    onto(&mut w, id, (0, MY, 0), (0, MY, 1), 1.0);

    // One cell per second, heading North (+v). Traveling North is R270.
    w.tick_entities(1.0);
    let s = w.local_structure(id).unwrap();
    assert_eq!(s.transform.anchor, bp(0, MY, 1));
    assert_eq!(s.transform.rotation, Rotation::R270);

    w.tick_entities(1.0);
    let s = w.local_structure(id).unwrap();
    assert_eq!(s.transform.anchor, bp(0, MY, 2));
    assert_eq!(s.rail.as_ref().unwrap().next_cell, bp(0, MY, 3));
}

#[test]
fn high_speed_crosses_multiple_segments_in_one_tick() {
    let rc = base_reg();
    let mut w = test_world_with("rail-multisegment", rc.clone());
    lay(
        &mut w,
        &rc,
        "base:rail",
        &[(0, MY, 0), (0, MY, 1), (0, MY, 2), (0, MY, 3)],
    );
    let tpl = rail_car(&mut w, &rc);
    let id = w
        .spawn_structure(&tpl, bp(0, MY, 0), Rotation::R0)
        .expect("spawns");
    onto(&mut w, id, (0, MY, 0), (0, MY, 1), 3.0);

    // 3 cells/s with a 1s tick crosses all three segments at once, and the
    // transform ends aligned with the newest travel direction.
    w.tick_entities(1.0);
    let s = w.local_structure(id).unwrap();
    assert_eq!(s.transform.anchor, bp(0, MY, 3));
    assert_eq!(s.transform.rotation, Rotation::R270);
    assert_eq!(s.rail.as_ref().unwrap().progress, 0.0);
}

#[test]
fn curve_turns_the_structure_and_updates_rotation() {
    let rc = base_reg();
    let mut w = test_world_with("rail-curve", rc.clone());
    lay(&mut w, &rc, "base:rail", &[(0, MY, 0), (0, MY, 1)]);
    lay(&mut w, &rc, "base:rail_curve_ne", &[(0, MY, 2)]);
    lay(&mut w, &rc, "base:rail", &[(1, MY, 2), (2, MY, 2)]);
    let tpl = rail_car(&mut w, &rc);
    let id = w
        .spawn_structure(&tpl, bp(0, MY, 0), Rotation::R0)
        .expect("spawns");
    onto(&mut w, id, (0, MY, 0), (0, MY, 1), 1.0);

    w.tick_entities(1.0);
    let s = w.local_structure(id).unwrap();
    assert_eq!(s.transform.anchor, bp(0, MY, 1));
    assert_eq!(s.transform.rotation, Rotation::R270, "still heading North");

    // Entering the NE curve from the South turns the structure to face East.
    w.tick_entities(1.0);
    let s = w.local_structure(id).unwrap();
    assert_eq!(s.transform.anchor, bp(0, MY, 2));
    assert_eq!(s.transform.rotation, Rotation::R0, "turned to face East");
    assert_eq!(s.rail.as_ref().unwrap().next_cell, bp(1, MY, 2));

    // The East leg continues straight.
    w.tick_entities(1.0);
    let s = w.local_structure(id).unwrap();
    assert_eq!(s.transform.anchor, bp(1, MY, 2));
    assert_eq!(s.transform.rotation, Rotation::R0);
    assert_eq!(s.rail.as_ref().unwrap().next_cell, bp(2, MY, 2));
}

#[test]
fn incline_rises_traveling_north_and_descends_traveling_south() {
    let rc = base_reg();
    let mut w = test_world_with("rail-incline", rc.clone());
    lay(&mut w, &rc, "base:rail", &[(0, MY, 0), (0, MY, 1)]);
    lay(&mut w, &rc, "base:rail_incline_n", &[(0, MY, 2)]);
    lay(&mut w, &rc, "base:rail", &[(0, MY + 1, 3), (0, MY + 1, 4)]);
    let tpl = rail_car(&mut w, &rc);

    // North-bound: the structure gains +1 y crossing the incline.
    let up = w
        .spawn_structure(&tpl, bp(0, MY, 0), Rotation::R0)
        .expect("spawns");
    onto(&mut w, up, (0, MY, 0), (0, MY, 1), 1.0);
    w.tick_entities(1.0);
    w.tick_entities(1.0);
    let s = w.local_structure(up).unwrap();
    assert_eq!(s.transform.anchor, bp(0, MY, 2));
    assert_eq!(s.rail.as_ref().unwrap().next_cell, bp(0, MY + 1, 3));
    w.tick_entities(1.0);
    let s = w.local_structure(up).unwrap();
    assert_eq!(s.transform.anchor, bp(0, MY + 1, 3), "rose one block");

    // South-bound: the same piece drops the structure back down.
    let down = w
        .spawn_structure(&tpl, bp(0, MY + 1, 4), Rotation::R0)
        .expect("spawns");
    onto(&mut w, down, (0, MY + 1, 4), (0, MY + 1, 3), 1.0);
    w.tick_entities(1.0);
    let s = w.local_structure(down).unwrap();
    assert_eq!(s.transform.anchor, bp(0, MY + 1, 3));
    w.tick_entities(1.0);
    let s = w.local_structure(down).unwrap();
    assert_eq!(s.transform.anchor, bp(0, MY, 2), "descended one block");
    assert_eq!(s.rail.as_ref().unwrap().next_cell, bp(0, MY, 1));
}

#[test]
fn switch_selection_controls_the_junction() {
    let rc = base_reg();
    let mut w = test_world_with("rail-switch", rc.clone());
    lay(
        &mut w,
        &rc,
        "base:rail",
        &[(0, MY, 0), (0, MY, 1), (0, MY, 3)],
    );
    lay(&mut w, &rc, "base:rail_switch", &[(0, MY, 2)]);
    lay(&mut w, &rc, "base:rail", &[(1, MY, 2)]);
    let tpl = rail_car(&mut w, &rc);

    // Default (straight-through, North): a North-bound car continues North.
    let a = w
        .spawn_structure(&tpl, bp(0, MY, 0), Rotation::R0)
        .expect("spawns");
    onto(&mut w, a, (0, MY, 0), (0, MY, 1), 1.0);
    w.tick_entities(1.0);
    w.tick_entities(1.0);
    let s = w.local_structure(a).unwrap();
    assert_eq!(s.rail.as_ref().unwrap().next_cell, bp(0, MY, 3), "straight");
    w.tick_entities(1.0);
    assert_eq!(w.local_structure(a).unwrap().transform.anchor, bp(0, MY, 3));

    // Toggle to East: the next car takes the branch.
    assert_eq!(w.switch_selected(bp(0, MY, 2)), None, "no entity yet");
    w.toggle_switch(bp(0, MY, 2));
    assert_eq!(w.switch_selected(bp(0, MY, 2)), Some(Direction4::East));

    let b = w
        .spawn_structure(&tpl, bp(0, MY, 0), Rotation::R0)
        .expect("spawns");
    onto(&mut w, b, (0, MY, 0), (0, MY, 1), 1.0);
    w.tick_entities(1.0);
    w.tick_entities(1.0);
    let s = w.local_structure(b).unwrap();
    assert_eq!(s.rail.as_ref().unwrap().next_cell, bp(1, MY, 2), "branch");
    w.tick_entities(1.0);
    assert_eq!(w.local_structure(b).unwrap().transform.anchor, bp(1, MY, 2));

    // And toggling back sends a third car straight through again.
    w.toggle_switch(bp(0, MY, 2));
    assert_eq!(w.switch_selected(bp(0, MY, 2)), Some(Direction4::North));
    let c = w
        .spawn_structure(&tpl, bp(0, MY, 0), Rotation::R0)
        .expect("spawns");
    onto(&mut w, c, (0, MY, 0), (0, MY, 1), 1.0);
    w.tick_entities(1.0);
    w.tick_entities(1.0);
    assert_eq!(
        w.local_structure(c)
            .unwrap()
            .rail
            .as_ref()
            .unwrap()
            .next_cell,
        bp(0, MY, 3),
        "straight again"
    );
}

#[test]
fn dead_end_parks_the_structure_on_the_last_rail_cell() {
    let rc = base_reg();
    let mut w = test_world_with("rail-deadend", rc.clone());
    lay(&mut w, &rc, "base:rail", &[(0, MY, 0), (0, MY, 1)]);
    let tpl = rail_car(&mut w, &rc);
    let id = w
        .spawn_structure(&tpl, bp(0, MY, 0), Rotation::R0)
        .expect("spawns");
    onto(&mut w, id, (0, MY, 0), (0, MY, 1), 1.0);

    // The car rides the second segment but the track ends at (0,MY,2):
    // on arrival it finds no continuation and parks at the last rail cell.
    w.tick_entities(1.0);
    let s = w.local_structure(id).unwrap();
    assert_eq!(s.transform.anchor, bp(0, MY, 1), "parks on the last rail");
    assert_eq!(s.rail.as_ref().unwrap().speed, 0.0, "dead end halts motion");
    assert_eq!(w.get_block_at(bp(0, MY, 2)), AIR, "no track was conjured");
}

#[test]
fn motion_never_touches_world_blocks() {
    let rc = base_reg();
    let mut w = test_world_with("rail-no-touch", rc.clone());
    lay(
        &mut w,
        &rc,
        "base:rail",
        &[(0, MY, 0), (0, MY, 1), (0, MY, 2), (0, MY, 3)],
    );
    let tpl = rail_car(&mut w, &rc);
    let id = w
        .spawn_structure(&tpl, bp(0, MY, 0), Rotation::R0)
        .expect("spawns");
    onto(&mut w, id, (0, MY, 0), (0, MY, 1), 1.0);

    let mut snapshot: HashMap<crate::planet::BlockPos, crate::registry::BlockId> = HashMap::new();
    for u in 0..4 {
        for v in 0..4 {
            for y in MY - 1..=MY + 1 {
                let pos = bp(u, y, v);
                snapshot.insert(pos, w.get_block_at(pos));
            }
        }
    }

    w.tick_entities(1.0);
    w.tick_entities(1.0);
    w.tick_entities(1.0);

    for (pos, block) in &snapshot {
        assert_eq!(w.get_block_at(*pos), *block, "world untouched at {pos:?}");
    }
}

#[test]
fn switch_state_survives_save_and_reload() {
    let rc = base_reg();
    let mut w = test_world_with("rail-switch-persist", rc.clone());
    lay(&mut w, &rc, "base:rail_switch", &[(0, MY, 0), (0, MY, 1)]);

    w.toggle_switch(bp(0, MY, 0));
    assert_eq!(w.switch_selected(bp(0, MY, 0)), Some(Direction4::East));
    assert_eq!(w.switch_selected(bp(0, MY, 1)), None, "untouched switch");

    save_world(&mut w);
    let dir = w.save_dir_for_test();
    let reloaded = World::load_or_create(dir.clone(), rc).expect("world reloads");
    assert_eq!(
        reloaded.switch_selected(bp(0, MY, 0)),
        Some(Direction4::East),
        "toggled selection survives"
    );
    assert_eq!(
        reloaded.switch_selected(bp(0, MY, 1)),
        None,
        "untouched switch still has no entity"
    );
}

// ---------------- phase 7a: rendering & interpolation ----------------

#[test]
fn static_structure_pose_resolves_directly_from_the_transform() {
    let rc = base_reg();
    let mut w = test_world_with("rail-pose-static", rc.clone());
    let tpl = rail_car(&mut w, &rc);
    for (i, rot) in Rotation::CARDINAL.iter().copied().enumerate() {
        let anchor = bp(5 + i as i32 * 3, MY, 5);
        let id = w.spawn_structure(&tpl, anchor, rot).expect("spawns");
        let structure = w.local_structure(id).unwrap();
        let pose = interpolated_pose(structure, &w);
        assert_eq!(pose.position, anchor.entity_center().render_pos());
        assert_eq!(pose.facing, rot);
        assert!(structure.rail.is_none(), "static");
    }
}

#[test]
fn rail_pose_lerps_between_cell_centers_and_snaps_facing() {
    let rc = base_reg();
    let mut w = test_world_with("rail-pose-lerp", rc.clone());
    lay(
        &mut w,
        &rc,
        "base:rail",
        &[(0, MY, 0), (0, MY, 1), (0, MY, 2)],
    );
    let tpl = rail_car(&mut w, &rc);
    let id = w
        .spawn_structure(&tpl, bp(0, MY, 0), Rotation::R0)
        .expect("spawns");
    let from = bp(0, MY, 0).entity_center().render_pos();
    let to = bp(0, MY, 1).entity_center().render_pos();

    let set = |w: &mut World, progress: f32| {
        assert!(w.set_rail(
            id,
            Some(RailState {
                current_cell: bp(0, MY, 0),
                next_cell: bp(0, MY, 1),
                progress,
                speed: 1.0,
            })
        ));
    };

    set(&mut w, 0.0);
    let pose = interpolated_pose(w.local_structure(id).unwrap(), &w);
    assert_eq!(
        pose.position, from,
        "progress 0.0 is exactly the current cell"
    );

    set(&mut w, 1.0);
    let pose = interpolated_pose(w.local_structure(id).unwrap(), &w);
    assert_eq!(pose.position, to, "progress 1.0 is exactly the next cell");

    set(&mut w, 0.5);
    let pose = interpolated_pose(w.local_structure(id).unwrap(), &w);
    assert_eq!(
        pose.position,
        from.lerp(to, 0.5),
        "midpoint lies on the render-space line between the two centers"
    );

    // Facing never lerps: it is the structure's discrete rotation the whole
    // way, snapping only at cell-boundary crossings in the tick.
    assert_eq!(pose.facing, Rotation::R0);

    // Out-of-range progress clamps to the segment endpoints rather than
    // extending beyond the two known cells.
    set(&mut w, 2.0);
    let pose = interpolated_pose(w.local_structure(id).unwrap(), &w);
    assert_eq!(pose.position, to, "over-shoot clamps to the next cell");
    set(&mut w, -0.5);
    let pose = interpolated_pose(w.local_structure(id).unwrap(), &w);
    assert_eq!(
        pose.position, from,
        "under-shoot clamps to the current cell"
    );
}

#[test]
fn cross_face_segment_snaps_instead_of_lerping_raw_ints() {
    let rc = base_reg();
    let mut w = test_world_with("rail-pose-cross-face", rc.clone());
    let tpl = rail_car(&mut w, &rc);
    let id = w
        .spawn_structure(&tpl, bp(0, MY, 0), Rotation::R0)
        .expect("spawns");
    let current = bp(0, MY, 0);
    let across = crate::planet::BlockPos::new(crate::planet::Face::PosX, 10u16, MY as u8, 10u16)
        .expect("valid PosX cell");
    assert_ne!(current.face(), across.face());

    assert!(w.set_rail(
        id,
        Some(RailState {
            current_cell: current,
            next_cell: across,
            progress: 0.5,
            speed: 1.0,
        })
    ));

    let pose = interpolated_pose(w.local_structure(id).unwrap(), &w);
    assert_eq!(
        pose.position,
        current.entity_center().render_pos(),
        "a seam-straddling segment snaps to the current cell instead of lerping"
    );
}
