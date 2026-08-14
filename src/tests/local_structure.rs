//! Unit tests for bounded local-structure entities (spec Part 1.1, scoped).

use std::collections::HashMap;

use super::*;
use crate::world::local_structure::{LocalStructureId, LocalTransform, from_template};
use crate::world::multiblock::Rotation;
use crate::world::template::capture_region;

fn reg() -> Arc<Registry> {
    base_reg()
}

const MY: i32 = 120;

#[test]
fn from_template_round_trips_cells_cell_for_cell() {
    let rc = reg();
    let mut w = test_world_with("localstructure-from-template", rc.clone());
    let fb = b(&rc, "base:firebrick");
    let st = b(&rc, "base:stone");
    // 2x2x2 firebrick box with a single stone block swapped in.
    for dx in 0..2 {
        for dy in 0..2 {
            for dz in 0..2 {
                w.set_block_at(bp(dx, MY + dy, dz), fb);
            }
        }
    }
    w.set_block_at(bp(1, MY, 1), st);
    let tpl = capture_region(&w, bp(0, MY, 0), bp(1, MY + 1, 1), "box").expect("captures");
    assert_eq!(tpl.cells.len(), 8);

    let structure = from_template(&tpl, &rc);
    assert_eq!(structure.name, "box");
    assert_eq!(structure.cell_count(), 8);
    for cell in &tpl.cells {
        let block = rc.block_id(&cell.block).expect("cell resolves");
        assert_eq!(
            structure.get_block((cell.du, cell.dy, cell.dv)),
            block,
            "cell {cell:?}"
        );
    }
    // Local accessors read independently of any world.
    assert_eq!(structure.get_block((5, 5, 5)), AIR, "empty cell reads air");
    let origin = crate::planet::BlockPos::new(crate::planet::Face::PosZ, 0, 0, 0).unwrap();
    assert_eq!(
        structure.transform,
        LocalTransform {
            anchor: origin,
            rotation: Rotation::R0,
        }
    );
}

#[test]
fn rotating_a_structure_composes_orientation_without_touching_cells() {
    let rc = reg();
    let mut w = test_world_with("localstructure-rotate", rc.clone());
    let fb = b(&rc, "base:firebrick");
    // A bar along +u: cells (0,0,0),(1,0,0),(2,0,0).
    for dx in 0..3 {
        w.set_block_at(bp(dx, MY, 0), fb);
    }
    let tpl = capture_region(&w, bp(0, MY, 0), bp(2, MY, 0), "bar").expect("captures");
    let mut structure = from_template(&tpl, &rc);
    assert_eq!(structure.cell_count(), 3);

    // Rotation is recorded on the transform only; the block store stays
    // canonical (exactly like Template::cells).
    structure.rotate(Rotation::R90);
    assert_eq!(structure.transform.rotation, Rotation::R90);
    for cell in &tpl.cells {
        assert_eq!(
            structure.get_block((cell.du, cell.dy, cell.dv)),
            fb,
            "store unrotated"
        );
    }

    // A full circle composes O(1) each: R90 -> R180 -> R270 -> R0.
    structure.rotate(Rotation::R90);
    assert_eq!(structure.transform.rotation, Rotation::R180);
    structure.rotate(Rotation::R90);
    assert_eq!(structure.transform.rotation, Rotation::R270);
    structure.rotate(Rotation::R90);
    assert_eq!(structure.transform.rotation, Rotation::R0);
    assert_eq!(structure.cell_count(), 3, "rotation never remaps the store");
}

#[test]
fn world_position_applies_rotation_and_anchor_exactly_once() {
    let rc = reg();
    let mut w = test_world_with("localstructure-world-position", rc.clone());
    let fb = b(&rc, "base:firebrick");
    w.set_block_at(bp(0, MY, 0), fb);
    let tpl = capture_region(&w, bp(0, MY, 0), bp(0, MY, 0), "single").expect("captures");
    let anchor = bp(10, MY, 10);
    let id = w
        .spawn_structure(&tpl, anchor, Rotation::R90)
        .expect("spawns");
    let structure = w.local_structure(id).expect("present");

    for cell in &tpl.cells {
        let offset = (cell.du, cell.dy, cell.dv);
        let rotated = Rotation::R90.apply(offset);
        let expected = anchor.offset(rotated.0, rotated.1, rotated.2);
        assert_eq!(
            structure.world_position(offset),
            expected,
            "cell {offset:?}"
        );
    }
}

#[test]
fn spawning_at_every_rotation_shares_one_canonical_store() {
    let rc = reg();
    let mut w = test_world_with("localstructure-rotations-share-store", rc.clone());
    let fb = b(&rc, "base:firebrick");
    for dx in 0..3 {
        w.set_block_at(bp(dx, MY, 0), fb);
    }
    w.capture_and_save(bp(0, MY, 0), bp(2, MY, 0), "bar")
        .expect("captured");
    let tpl = w.template("bar").cloned().unwrap();

    let mut stores: Vec<std::collections::HashMap<(i32, i32, i32), crate::registry::BlockId>> =
        Vec::new();
    for (i, rot) in Rotation::CARDINAL.iter().enumerate() {
        let anchor = bp(10 + i as i32 * 6, MY, 10);
        let id = w.spawn_structure(&tpl, anchor, *rot).expect("spawns");
        let structure = w.local_structure(id).unwrap();
        stores.push(structure.blocks.clone());
    }
    assert_eq!(stores.len(), 4);
    // Four structures with different orientations all share identical cells.
    let first = &stores[0];
    for other in &stores[1..] {
        assert_eq!(first, other, "canonical store is rotation-independent");
    }
}

#[test]
fn set_block_mutates_the_store_independently() {
    let rc = reg();
    let fb = b(&rc, "base:firebrick");
    let st = b(&rc, "base:stone");
    let mut w = test_world_with("localstructure-set", rc.clone());
    w.set_block_at(bp(0, MY, 0), fb);
    let tpl = capture_region(&w, bp(0, MY, 0), bp(0, MY, 0), "single").expect("captures");
    let mut structure = from_template(&tpl, &rc);
    assert_eq!(structure.cell_count(), 1);

    structure.set_block((3, 1, -2), st);
    assert_eq!(structure.get_block((3, 1, -2)), st);
    structure.set_block((3, 1, -2), fb);
    assert_eq!(structure.get_block((3, 1, -2)), fb);
    structure.set_block((3, 1, -2), AIR);
    assert_eq!(structure.get_block((3, 1, -2)), AIR, "air clears the cell");
    assert_eq!(structure.cell_count(), 1, "cleared cell is removed");
}

#[test]
fn spawning_a_structure_does_not_touch_world_blocks() {
    let rc = reg();
    let mut w = test_world_with("localstructure-no-touch", rc.clone());
    let fb = b(&rc, "base:firebrick");
    for dx in 0..3 {
        w.set_block_at(bp(dx, MY, 0), fb);
    }
    w.capture_and_save(bp(0, MY, 0), bp(2, MY, 0), "bar")
        .expect("captured");
    let tpl = w.template("bar").cloned().expect("template saved");

    let anchor = bp(10, MY, 10);
    let world = &w;
    let before: HashMap<crate::planet::BlockPos, crate::registry::BlockId> = (0..3)
        .flat_map(move |dx| {
            (0..3).flat_map(move |dz| {
                (0..3).map(move |dy| {
                    let pos = anchor.offset(dx, dy, dz).unwrap();
                    (pos, world.get_block_at(pos))
                })
            })
        })
        .collect();

    let id = w
        .spawn_structure(&tpl, anchor, Rotation::R0)
        .expect("spawns");
    assert_eq!(id.0, 0, "first structure gets id zero");

    // Not a single world block changed — in contrast to stamp_instant and
    // stamp_ghost, both of which deliberately write into the chunk grid.
    for (pos, block) in &before {
        assert_eq!(w.get_block_at(*pos), *block, "world untouched at {pos:?}");
    }

    // But the structure's own store is populated, matching the template.
    let structure = w.local_structure(id).expect("structure present");
    for cell in &tpl.cells {
        let block = rc.block_id(&cell.block).unwrap();
        assert_eq!(structure.get_block((cell.du, cell.dy, cell.dv)), block);
    }
    assert_eq!(structure.transform.anchor, anchor);
    assert_eq!(structure.transform.rotation, Rotation::R0);
}

#[test]
fn local_structures_survive_a_save_and_reload() {
    let rc = reg();
    let mut w = test_world_with("localstructure-persist", rc.clone());
    let fb = b(&rc, "base:firebrick");
    let st = b(&rc, "base:stone");
    for dx in 0..3 {
        w.set_block_at(bp(dx, MY, 0), fb);
    }
    w.set_block_at(bp(1, MY, 0), st);
    w.capture_and_save(bp(0, MY, 0), bp(2, MY, 0), "bar")
        .expect("captured");
    let tpl = w.template("bar").cloned().unwrap();

    let anchor = bp(10, MY, 10);
    let id = w
        .spawn_structure(&tpl, anchor, Rotation::R90)
        .expect("first spawns");
    assert_eq!(id.0, 0);
    let id2 = w
        .spawn_structure(&tpl, bp(20, MY, 20), Rotation::R0)
        .expect("second spawns");
    assert_eq!(id2.0, 1, "the id counter advances");

    save_world(&mut w);
    let dir = w.save_dir_for_test();
    let reloaded = World::load_or_create(dir.clone(), rc).expect("world reloads");

    let structures = reloaded.local_structures();
    assert_eq!(structures.len(), 2);

    let first = reloaded
        .local_structure(LocalStructureId(0))
        .expect("first survives");
    assert_eq!(first.name, "bar");
    assert_eq!(first.transform.anchor, anchor);
    assert_eq!(first.transform.rotation, Rotation::R90);
    // Storage is canonical regardless of rotation: the template's stone stays
    // at local offset (1,0,0); only the transform carries the R90.
    assert_eq!(first.get_block((1, 0, 0)), st);
    assert_eq!(
        first.get_block((0, 0, -1)),
        AIR,
        "nothing baked into the keys"
    );
    assert_eq!(first.cell_count(), 3);
    // And world_position still resolves the reoriented placement.
    let rotated = Rotation::R90.apply((1, 0, 0));
    assert_eq!(
        first.world_position((1, 0, 0)),
        anchor.offset(rotated.0, rotated.1, rotated.2),
        "reoriented resolution survives reload"
    );

    let second = reloaded
        .local_structure(LocalStructureId(1))
        .expect("second survives");
    assert_eq!(second.transform.rotation, Rotation::R0);
    assert_eq!(second.get_block((1, 0, 0)), st, "id-1 structure intact");
}

#[test]
fn removing_a_structure_is_persisted() {
    let rc = reg();
    let mut w = test_world_with("localstructure-despawn", rc.clone());
    let fb = b(&rc, "base:firebrick");
    for dx in 0..3 {
        w.set_block_at(bp(dx, MY, 0), fb);
    }
    w.capture_and_save(bp(0, MY, 0), bp(2, MY, 0), "bar")
        .expect("captured");
    let tpl = w.template("bar").cloned().unwrap();

    let id0 = w
        .spawn_structure(&tpl, bp(10, MY, 10), Rotation::R0)
        .expect("first");
    let id1 = w
        .spawn_structure(&tpl, bp(20, MY, 20), Rotation::R90)
        .expect("second");
    assert_eq!(id0.0, 0);
    assert_eq!(id1.0, 1);
    assert_eq!(w.local_structures().len(), 2);

    // Removing a nonexistent id is a clean, non-erroring no-op.
    assert!(!w.remove_structure(LocalStructureId(99)));
    assert_eq!(w.local_structures().len(), 2);

    assert!(w.remove_structure(id0));
    assert_eq!(w.local_structures().len(), 1);
    assert!(w.local_structure(id0).is_none());
    assert!(w.local_structure(id1).is_some());

    save_world(&mut w);
    let dir = w.save_dir_for_test();
    let reloaded = World::load_or_create(dir.clone(), rc).expect("world reloads");
    let structures = reloaded.local_structures();
    assert_eq!(structures[0].id, id1, "the surviving structure remains");
}

#[test]
fn spawn_and_despawn_commands_route_through_template_command() {
    let rc = reg();
    let mut w = test_world_with("localstructure-commands", rc.clone());
    let fb = b(&rc, "base:firebrick");
    w.set_block_at(bp(0, MY, 0), fb);
    w.capture_and_save(bp(0, MY, 0), bp(0, MY, 0), "single")
        .expect("captured");

    // Spawn via the command surface (face u y v, plus an optional rot).
    let replies = w.template_command("spawn single pos_z 10 120 10 r90");
    assert_eq!(replies.len(), 1);
    assert!(replies[0].contains("structure #0"), "{replies:?}");
    assert_eq!(w.local_structures().len(), 1);

    // Despawning a nonexistent id is a no-op reply.
    let replies = w.template_command("despawn 42");
    assert!(
        replies[0].contains("no structure with id 42"),
        "{replies:?}"
    );
    assert_eq!(w.local_structures().len(), 1);

    // Despawning the real id removes it.
    let replies = w.template_command("despawn 0");
    assert!(replies[0].contains("despawned structure #0"), "{replies:?}");
    assert!(w.local_structures().is_empty());
}
