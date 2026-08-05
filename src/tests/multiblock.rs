//! Tests for the generic multiblock matcher, using synthetic shapes only —
//! deliberately independent of any real machine, to prove `match_shape` is
//! genuinely generic and not coupled to the firebrick-ring case.

use super::*;
use crate::world::multiblock::{
    BlockConstraint, MultiblockShape, Rotation, ShapeCell, match_shape,
};

/// A two-block horizontal stick along +X. R0 points +X; R90 points -Z;
/// R180 points -X; R270 points +Z.
fn stick_shape() -> MultiblockShape {
    let stone = b(&base_reg(), "base:stone");
    MultiblockShape {
        cells: vec![
            ShapeCell {
                offset: (1, 0, 0),
                constraint: BlockConstraint::Exact(stone),
            },
            ShapeCell {
                offset: (2, 0, 0),
                constraint: BlockConstraint::Exact(stone),
            },
        ],
        core: (1, 0, 0),
        rotations: &Rotation::CARDINAL,
    }
}

#[test]
fn match_shape_rotates_onto_the_placed_orientation() {
    let reg = base_reg();
    let stone = b(&reg, "base:stone");
    // A stick laid along -Z: R0 (+X) sees nothing, R90 (+X->-Z) does.
    let mut w = test_world_with("mb-rotate", reg.clone());
    let anchor = bp(5, 100, 5);
    w.set_block(5, 100, 4, stone);
    w.set_block(5, 100, 3, stone);
    let result = match_shape(&w, anchor, &stick_shape()).expect("rotated stick matches");
    assert_eq!(
        result.core,
        bp(5, 100, 4),
        "core lands on the near stick block"
    );
    assert_eq!(result.matched.len(), 2, "every cell is collected");
}

#[test]
fn match_shape_fails_when_a_cell_is_missing() {
    let reg = base_reg();
    let mut w = test_world_with("mb-missing", reg.clone());
    // Only one of the two stick cells is present.
    w.set_block(5, 100, 4, b(&reg, "base:stone"));
    assert!(
        match_shape(&w, bp(5, 100, 5), &stick_shape()).is_none(),
        "one missing cell fails every rotation"
    );
}

#[test]
fn match_shape_fails_on_a_wrong_block() {
    let reg = base_reg();
    let mut w = test_world_with("mb-wrong", reg.clone());
    // The far cell is dirt, not stone.
    w.set_block(5, 100, 4, b(&reg, "base:stone"));
    w.set_block(5, 100, 3, b(&reg, "base:dirt"));
    assert!(
        match_shape(&w, bp(5, 100, 5), &stick_shape()).is_none(),
        "a substituted block fails the match"
    );
}

/// Every constraint kind in one shape: exact, one-of, tag, air, and
/// solid-or-glass. Pass builds satisfy all; fail builds flip one cell.
fn kitchen_sink_shape() -> MultiblockShape {
    let reg = base_reg();
    MultiblockShape {
        cells: vec![
            ShapeCell {
                offset: (0, 0, 0),
                constraint: BlockConstraint::Exact(b(&reg, "base:firebrick")),
            },
            ShapeCell {
                offset: (0, 0, 1),
                constraint: BlockConstraint::OneOf(vec![
                    b(&reg, "base:stone"),
                    b(&reg, "base:cobblestone"),
                ]),
            },
            ShapeCell {
                offset: (0, 0, 2),
                constraint: BlockConstraint::Tag("base:logs"),
            },
            ShapeCell {
                offset: (0, 0, 3),
                constraint: BlockConstraint::Air,
            },
            ShapeCell {
                offset: (0, 0, 4),
                constraint: BlockConstraint::SolidOrGlass,
            },
        ],
        core: (0, 0, 0),
        rotations: &[Rotation::R0],
    }
}

#[test]
fn match_shape_satisfies_every_constraint_kind() {
    let reg = base_reg();
    let mut w = test_world_with("mb-kitchen", reg.clone());
    let anchor = bp(10, 100, 10);
    w.set_block(10, 100, 10, b(&reg, "base:firebrick"));
    w.set_block(10, 100, 11, b(&reg, "base:cobblestone")); // OneOf
    w.set_block(10, 100, 12, b(&reg, "base:log")); // Tag base:logs
    // (0,0,3) stays air
    w.set_block(10, 100, 14, b(&reg, "base:glass")); // SolidOrGlass
    let result = match_shape(&w, anchor, &kitchen_sink_shape()).expect("kitchen sink matches");
    assert_eq!(result.core, anchor);
}

#[test]
fn match_shape_rejects_a_one_of_cell() {
    let reg = base_reg();
    let mut w = test_world_with("mb-oneof", reg.clone());
    let anchor = bp(10, 100, 10);
    w.set_block(10, 100, 10, b(&reg, "base:firebrick"));
    w.set_block(10, 100, 11, b(&reg, "base:dirt")); // not stone/cobble
    w.set_block(10, 100, 12, b(&reg, "base:log"));
    w.set_block(10, 100, 14, b(&reg, "base:glass"));
    assert!(
        match_shape(&w, anchor, &kitchen_sink_shape()).is_none(),
        "a block outside the OneOf set fails the cell"
    );
}

#[test]
fn match_shape_rejects_a_tag_cell() {
    let reg = base_reg();
    let mut w = test_world_with("mb-tag", reg.clone());
    let anchor = bp(10, 100, 10);
    w.set_block(10, 100, 10, b(&reg, "base:firebrick"));
    w.set_block(10, 100, 11, b(&reg, "base:stone"));
    w.set_block(10, 100, 12, b(&reg, "base:planks")); // not a log
    w.set_block(10, 100, 14, b(&reg, "base:glass"));
    assert!(
        match_shape(&w, anchor, &kitchen_sink_shape()).is_none(),
        "a non-tagged block fails the tag cell"
    );
}

#[test]
fn match_shape_rejects_an_air_cell() {
    let reg = base_reg();
    let mut w = test_world_with("mb-air", reg.clone());
    let anchor = bp(10, 100, 10);
    w.set_block(10, 100, 10, b(&reg, "base:firebrick"));
    w.set_block(10, 100, 11, b(&reg, "base:stone"));
    w.set_block(10, 100, 12, b(&reg, "base:log"));
    w.set_block(10, 100, 13, b(&reg, "base:stone")); // must be air
    w.set_block(10, 100, 14, b(&reg, "base:glass"));
    assert!(
        match_shape(&w, anchor, &kitchen_sink_shape()).is_none(),
        "an occupied air cell fails the match"
    );
}

#[test]
fn match_shape_rejects_a_solid_or_glass_cell() {
    let reg = base_reg();
    let mut w = test_world_with("mb-sog", reg.clone());
    let anchor = bp(10, 100, 10);
    w.set_block(10, 100, 10, b(&reg, "base:firebrick"));
    w.set_block(10, 100, 11, b(&reg, "base:stone"));
    w.set_block(10, 100, 12, b(&reg, "base:log"));
    w.set_block(10, 100, 14, b(&reg, "base:torch")); // not solid, not glass
    assert!(
        match_shape(&w, anchor, &kitchen_sink_shape()).is_none(),
        "a torch is neither solid nor glazing"
    );
}

#[test]
fn rotation_order_is_deterministic_but_direction_agnostic() {
    // A stick laid along +X matches the same shape's R0 rotation, and the
    // reported core is the near block either way.
    let reg = base_reg();
    let mut w = test_world_with("mb-cardinal", reg.clone());
    w.set_block(7, 100, 7, b(&reg, "base:stone"));
    w.set_block(8, 100, 7, b(&reg, "base:stone"));
    let result = match_shape(&w, bp(6, 100, 7), &stick_shape()).expect("stick matches +X");
    assert_eq!(result.core, bp(7, 100, 7));
}

/// A shape whose single cell is a `Module` slot (spec Part 1.3).
fn module_slot_shape() -> MultiblockShape {
    MultiblockShape {
        cells: vec![ShapeCell {
            offset: (0, 0, 0),
            constraint: BlockConstraint::Module("casing"),
        }],
        core: (0, 0, 0),
        rotations: &[Rotation::R0],
    }
}

#[test]
fn match_shape_accepts_and_rejects_a_module_cell() {
    let reg = base_reg();
    let mut w = test_world_with("mb-module", reg.clone());
    let anchor = bp(10, 100, 10);
    // The catalog's baseline module holds the slot, and the match
    // reports the cell as a "casing" slot (what capability folding reads).
    w.set_block(10, 100, 10, b(&reg, "base:firebrick"));
    let result =
        match_shape(&w, anchor, &module_slot_shape()).expect("a catalog member holds the slot");
    assert_eq!(result.slots.get(&anchor).copied(), Some("casing"));
    // Another catalog member works too.
    w.set_block(10, 100, 10, b(&reg, "base:casing_porcelain"));
    let result =
        match_shape(&w, anchor, &module_slot_shape()).expect("porcelain is a catalog module");
    assert_eq!(result.slots.get(&anchor).copied(), Some("casing"));
    // A block outside the category's catalog breaks the slot.
    w.set_block(10, 100, 10, b(&reg, "base:stone"));
    assert!(
        match_shape(&w, anchor, &module_slot_shape()).is_none(),
        "a non-catalog block fails the module cell"
    );
    // Air is not a module either.
    w.set_block(10, 100, 10, AIR);
    assert!(
        match_shape(&w, anchor, &module_slot_shape()).is_none(),
        "an empty slot cell fails the match"
    );
}
