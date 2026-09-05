//! Unit tests for capture & stamp tooling (spec Part 1.4).

use std::collections::HashMap;

use super::*;
use crate::inventory::Inventory;
use crate::world::multiblock::Rotation;
use crate::world::template::{MAX_CAPTURE_CELLS, capture_region};

fn reg() -> Arc<Registry> {
    base_reg()
}

const MY: i32 = 120;

fn world_key(pos: crate::planet::BlockPos) -> (u16, u8, u16) {
    (pos.u(), pos.y(), pos.v())
}

#[test]
fn capture_and_stamp_round_trip_is_cell_for_cell_equal() {
    let rc = reg();
    let mut w = test_world_with("template-round-trip", rc.clone());
    // A small, simple box: 2x2x2 of firebrick, open under and over.
    let fb = b(&rc, "base:firebrick");
    for dx in 0..2 {
        for dy in 0..2 {
            for dz in 0..2 {
                w.set_block_at(bp(dx, MY + dy, dz), fb);
            }
        }
    }
    let a = bp(0, MY, 0);
    let bb = bp(1, MY + 1, 1);
    let n = w.capture_and_save(a, bb, "box").unwrap();
    assert_eq!(n, 8, "2x2x2 solid box");
    let tpl = w.template("box").cloned().expect("template saved");

    // Stamp it elsewhere, rotation 0, from a well-stocked inventory.
    let mut inv = Inventory::new();
    inv.add(&rc, it(&rc, "base:firebrick"), 64);
    let anchor = bp(8, MY, 8);
    let msg = w
        .stamp_instant(&tpl, anchor, Rotation::R0, &mut inv, false)
        .unwrap();
    assert!(msg.contains("placed 8"), "all eight cells placed: {msg}");

    // Every captured cell reproduces exactly.
    for cell in &tpl.cells {
        let pos = anchor.offset(cell.du, cell.dy, cell.dv).expect("in bounds");
        let got = w.reg.block(w.get_block_at(pos)).name.clone();
        assert_eq!(got, cell.block, "cell {cell:?} at {pos:?}");
    }

    // The inventory was charged one firebrick per placed block.
    assert_eq!(inv.count_of(it(&rc, "base:firebrick")), 64 - 8);
}

#[test]
fn four_rotations_place_at_rotated_positions() {
    let rc = reg();
    let mut w = test_world_with("template-rotations", rc.clone());
    let fb = b(&rc, "base:firebrick");
    // A long thin bar (not a square) makes a wrong rotation impossible to miss.
    for dx in 0..3 {
        w.set_block_at(bp(20, MY, 20).offset(dx, 0, 0).unwrap(), fb);
    }
    let bar =
        capture_region(&w, bp(20, MY, 20), bp(22, MY, 20), "bar").expect("bar captures on PosZ");
    assert_eq!(bar.cells.len(), 3);

    for (i, rot) in Rotation::CARDINAL.iter().enumerate() {
        let mut inv = Inventory::new();
        inv.add(&rc, it(&rc, "base:firebrick"), 64);
        let anchor = bp(30, MY, 30).offset(0, 0, i as i32 * 6).unwrap();
        // The world fixture loads only a small centered band of chunks;
        // the later rotation anchors drift a couple of 16-wide chunks away.
        let cu = i32::from(anchor.chunk().u());
        let cv = i32::from(anchor.chunk().v());
        for cx in -3..=3 {
            for cz in -3..=3 {
                let chunk = ChunkPos::new(
                    crate::planet::Face::PosZ,
                    (cu + cx) as u16,
                    (cv + cz) as u16,
                )
                .expect("test chunk");
                w.ensure_chunk(chunk);
            }
        }
        let msg = w
            .stamp_instant(&bar, anchor, *rot, &mut inv, false)
            .unwrap();
        assert!(msg.contains("placed 3"), "{msg}");

        // For each template cell, the world must hold the block at the
        // rotation-mapped offset...
        let mut expected: HashMap<(u16, u8, u16), String> = HashMap::new();
        for cell in &bar.cells {
            let (du, dy, dv) = rot.apply((cell.du, cell.dy, cell.dv));
            let pos = anchor.offset(du, dy, dv).expect("rotated cell in bounds");
            assert_eq!(
                w.reg.block(w.get_block_at(pos)).name,
                cell.block,
                "rotation {rot:?} cell {cell:?}"
            );
            expected.insert(world_key(pos), cell.block.clone());
        }
        // ...and nothing of the template's material shows up outside those
        // expected cells within a 6x6x6 box around the anchor.
        for dx in 0..6 {
            for dy in 0..6 {
                for dz in 0..6 {
                    let pos = anchor.offset(dx, dy, dz).unwrap();
                    let block = w.reg.block(w.get_block_at(pos)).name.clone();
                    if expected.get(&world_key(pos)).map(|s| s.as_str()) == Some(block.as_str()) {
                        continue;
                    }
                    assert_ne!(
                        block, "base:firebrick",
                        "stray firebrick at {pos:?} under {rot:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn instant_stamp_fails_atomically_when_short() {
    let rc = reg();
    let mut w = test_world_with("template-shortfall", rc.clone());
    let fb = b(&rc, "base:firebrick");
    for dx in 0..2 {
        for dy in 0..2 {
            for dz in 0..2 {
                w.set_block_at(bp(0, MY, 0).offset(dx, dy, dz).unwrap(), fb);
            }
        }
    }
    w.capture_and_save(bp(0, MY, 0), bp(1, MY + 1, 1), "box")
        .unwrap();
    let tpl = w.template("box").cloned().expect("template saved");

    let anchor = bp(12, MY, 12);
    let mut empty = Inventory::new();
    let err = w
        .stamp_instant(&tpl, anchor, Rotation::R0, &mut empty, false)
        .unwrap_err();
    assert!(err.contains("short"), "error names the gap: {err}");
    assert!(
        err.contains("firebrick"),
        "error names what's missing: {err}"
    );

    // Nothing was placed, and nothing was consumed.
    for dx in 0..2 {
        for dy in 0..2 {
            for dz in 0..2 {
                let pos = anchor.offset(dx, dy, dz).unwrap();
                assert_eq!(
                    w.get_block_at(pos),
                    AIR,
                    "shortfall must not place block at {pos:?}"
                );
            }
        }
    }
}

#[test]
fn ghost_fill_completion_runs_ordinary_revalidation() {
    let rc = reg();
    let mut w = test_world_with("template-ghost", rc.clone());
    build_forge(&mut w, &rc, 0, MY, 0);
    w.capture_and_save(bp(-1, MY, -1), bp(2, MY + 5, 1), "forge")
        .expect("forge capture");
    let tpl = w.template("forge").cloned().expect("template saved");
    assert!(tpl.cells.iter().any(|c| c.block == "base:forge"));

    let anchor = bp(20, MY, 20);
    w.stamp_ghost(&tpl, anchor, Rotation::R0).unwrap();
    let fill = w.pending_fill_at(anchor).expect("ghost fill registered");
    assert!(fill.remaining_count() > 0);

    // Fill every pending cell with its correct block via ordinary placement.
    let mut keys: Vec<crate::planet::BlockPos> = w
        .pending_fill_at(anchor)
        .unwrap()
        .remaining
        .keys()
        .copied()
        .collect();
    keys.sort_by_key(|p| (p.u(), p.y(), p.v()));
    for pos in keys {
        let name = w.pending_fill_at(anchor).unwrap().remaining[&pos].clone();
        let block = rc.block_id(&name).expect("pending block resolves");
        assert!(w.place_block_at(pos, block), "ordinary placement lands");
    }

    // The fill is exhausted and gone — no activation step, no special path.
    assert!(
        w.pending_fill_at(anchor).is_none(),
        "fill cleared by placement"
    );

    // The hand-fill result validates exactly like a hand-built forge.
    let mouth = anchor.offset(1, 0, 1).expect("mouth offset");
    assert!(
        w.check_forge_at(mouth).is_some(),
        "ghost-filled forge matches MachineKind::Forge"
    );
}

#[test]
fn captured_machine_restores_no_block_entity_contents() {
    let rc = reg();
    let mut w = test_world_with("template-shell", rc.clone());
    build_forge(&mut w, &rc, 0, MY, 0);
    w.capture_and_save(bp(-1, MY, -1), bp(2, MY + 5, 1), "forge")
        .unwrap();
    let tpl = w.template("forge").cloned().expect("template saved");

    let anchor = bp(24, MY, 24);
    let mut inv = Inventory::new();
    // Creative stamping: no inventory accounting, but the shell must still
    // be registered and folded.
    let msg = w
        .stamp_instant(&tpl, anchor, Rotation::R0, &mut inv, true)
        .unwrap();
    assert!(msg.starts_with("placed"), "{msg}");

    let mouth = anchor.offset(1, 0, 1).expect("mouth offset");
    let entity = w.block_entity_at(&mouth).expect("mouth has a shell entity");
    let crate::world::BlockEntity::Multiblock(m) = entity else {
        panic!("expected a machine shell entity");
    };
    assert_eq!(m.kind, rc.machine_kind("base:forge").unwrap_or_default());
    assert!(!m.lit, "a stamped forge is not lit");
    assert!(m.charge.iter().all(|s| s.is_none()), "no charge restored");
    assert!(m.fuel.iter().all(|s| s.is_none()), "no fuel restored");
    assert!(m.reagent.is_none(), "no reagent restored");
    assert!(m.core.is_none(), "an unlit stamped shell has no core yet");
}

#[test]
fn inventory_afford_check_is_all_or_nothing() {
    let rc = reg();
    let firebrick = it(&rc, "base:firebrick");
    let stone = it(&rc, "base:stone");
    let mut inv = Inventory::new();
    inv.add(&rc, firebrick, 5);
    inv.add(&rc, stone, 3);

    assert!(inv.can_afford(&[(firebrick, 4), (stone, 3)]));
    assert!(!inv.can_afford(&[(firebrick, 6)]));
    assert!(!inv.can_afford(&[(firebrick, 5), (stone, 4)]));

    let short = [(firebrick, 6)];
    assert!(!inv.try_consume(&short), "short cost refuses");
    assert_eq!(
        inv.count_of(firebrick),
        5,
        "refused consume changes nothing"
    );

    assert!(inv.try_consume(&[(firebrick, 4), (stone, 3)]));
    assert_eq!(inv.count_of(firebrick), 1);
    assert_eq!(inv.count_of(stone), 0);
}

#[test]
fn capture_is_bounded_and_discards_unplaceable_cells() {
    let rc = reg();
    let mut w = test_world_with("template-capture-guard", rc.clone());
    let fb = b(&rc, "base:firebrick");
    w.set_block_at(bp(0, MY, 0), fb);
    // Air and a fluid are never captured; firebrick is.
    let tpl = capture_region(&w, bp(0, MY, 0), bp(3, MY, 3), "sparse").expect("captures");
    assert_eq!(tpl.cells.len(), 1, "only the solid cell");
    assert_eq!(tpl.cells[0].block, "base:firebrick");
    assert!(tpl.cells.len() <= MAX_CAPTURE_CELLS);
}

#[test]
fn templates_persist_across_a_save() {
    let rc = reg();
    let mut w = test_world_with("template-persist", rc.clone());
    let fb = b(&rc, "base:firebrick");
    w.set_block_at(bp(0, MY, 0), fb);
    w.capture_and_save(bp(0, MY, 0), bp(0, MY, 0), "single")
        .unwrap();
    assert!(w.remove_template("single"), "removed");
    w.capture_and_save(bp(0, MY, 0), bp(0, MY, 0), "single")
        .unwrap();
    save_world(&mut w);

    let dir = w.save_dir_for_test();
    let reloaded = World::load_or_create(dir.clone(), rc).expect("world reloads");
    let t = reloaded.template("single").expect("template persisted");
    assert_eq!(t.cells.len(), 1);
    assert_eq!(t.cells[0].block, "base:firebrick");
}
