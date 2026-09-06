//! Admission scenarios.

use super::*;

#[test]
fn wand_authority_rejects_through_wall_and_out_of_range_targets() {
    let (mut world, source, wand) = workings_world("workings-wand-los");
    let wall = source.offset(1, 0, 0).unwrap();
    let hidden = source.offset(2, 0, 0).unwrap();
    world.set_block_at(wall, b(&world.reg, "base:stone"));
    world.set_block_at(hidden, b(&world.reg, "base:log"));
    assert!(
        world
            .begin_trace_working(
                [28; 16],
                "los-worker",
                source,
                wand.arcane_id,
                hidden,
                20,
                false,
            )
            .unwrap_err()
            .contains("line of sight")
    );
    let far = source.offset(12, 0, 0).unwrap();
    world.set_block_at(wall, AIR);
    world.set_block_at(far, AIR);
    assert!(
        world
            .begin_gleam_working(
                [28; 16],
                "range-worker",
                source,
                wand.arcane_id,
                far,
                1,
                20,
                false,
            )
            .unwrap_err()
            .contains("bounded reach")
    );
}
