//! Block-built interiors (task brief Phase 7b): a functioning machine
//! inside a `LocalStructure`'s own block store.
//!
//! Proof that the `BlockStore` generalization gave structures real block
//! behavior: an in-structure forge validates through the same matcher that
//! reads the main chunk grid, lights through the same logic, fires through
//! the same per-kind tick, stays scoped to its own edits, and survives a
//! save/reload round-trip — all without a single main-world block edit.
//!
//! Phase 8 extends this with structure-aware targeting (`raycast_target_at`),
//! structure-scoped break/place, and the Phase‑8 player-facing workflow.

use super::*;
use crate::world::local_structure::{LocalStructureId, from_template};
use crate::world::machines::light_machine_at;
use crate::world::multiblock::{BlockStore, Rotation};
use crate::world::template::capture_region;
use crate::world::{BlockEntity, FORGE_FIRE_SECS, MachineInstance};

const MY: i32 = 120;

/// The forge helpers below place their mouth at the local offset `(1, 0, 1)`
/// of the captured region, so every shape (which anchors at the mouth) can
/// be validated from this constant.
const MOUTH: (i32, i32, i32) = (1, 0, 1);

/// A forge machine holding 8 raw copper + 8 charcoal — the same batch the
/// world-hosted forge test fires, so outcomes are directly comparable.
fn charged_forge(rc: &Registry) -> MachineInstance {
    let raw = it(rc, "base:raw_copper");
    let coal = it(rc, "base:charcoal");
    let mut st = MachineInstance {
        kind: rc.machine_kind("base:forge").unwrap_or_default(),
        ..Default::default()
    };
    for i in 0..4 {
        st.charge[i] = Some(ItemStack::new(rc, raw, 2));
    }
    st.fuel[0] = Some(ItemStack::new(rc, coal, 4));
    st.fuel[1] = Some(ItemStack::new(rc, coal, 4));
    st
}

/// Captured copper ingots sitting in a structure's outbox.
fn outbox_ingots(structure: &crate::world::local_structure::LocalStructure) -> u32 {
    let ingot = structure.reg.item_id("base:copper_ingot");
    structure
        .outbox
        .iter()
        .filter(|s| Some(s.item) == ingot)
        .map(|s| s.count)
        .sum()
}

#[test]
fn the_generic_matcher_recognizes_machines_inside_a_structure() {
    let rc = base_reg();
    let mut w = test_world_with("interiors-matcher", rc.clone());

    // A bloomery shell captured from the main world, served back to the
    // matcher as a standalone LocalStructure store.
    build_bloomery(&mut w, &rc, 1, MY, 1);
    let bloom_tpl =
        capture_region(&w, bp(0, MY, 0), bp(3, MY + 2, 2), "bloomery").expect("bloomery captures");
    let bloom = from_template(&bloom_tpl, &rc);
    assert!(
        rc.machine_kind("base:bloomery").unwrap_or_default().validate(&bloom, MOUTH).is_some(),
        "a bloomery shell inside a structure validates via the generic matcher"
    );
    assert!(
        rc.machine_kind("base:forge").unwrap_or_default().validate(&bloom, MOUTH).is_none(),
        "the bloomery mouth never reads as a forge"
    );
    assert!(
        rc.machine_kind("base:bloomery").unwrap_or_default().validate(&bloom, (5, 5, 5)).is_none(),
        "an empty corner of the store has no machine"
    );

    // A forge shell (stack + chimney + anvil) in its own structure.
    build_forge(&mut w, &rc, 8, MY, 8);
    let forge_tpl =
        capture_region(&w, bp(7, MY, 7), bp(10, MY + 5, 9), "forge").expect("forge captures");
    let forge = from_template(&forge_tpl, &rc);
    assert!(
        rc.machine_kind("base:forge").unwrap_or_default().validate(&forge, MOUTH).is_some(),
        "a full forge workshop in a structure validates (stack + chimney + anvil)"
    );
}

#[test]
fn interior_forge_lights_ticks_and_completes_in_any_weather() {
    let rc = base_reg();
    let mut w = test_world_with("interiors-forge-lifecycle", rc.clone());
    build_forge(&mut w, &rc, 1, MY, 1);
    let tpl = capture_region(&w, bp(0, MY, 0), bp(3, MY + 5, 2), "forge").expect("forge captures");
    let id = w
        .spawn_structure(&tpl, bp(30, MY, 30), Rotation::R0)
        .expect("spawns");

    // Charge and light the in-structure forge exactly like a world forge.
    // Validate (immutable borrow) and insert+light (mutable borrow) are
    // kept in separate scopes so Rust's borrow checker is satisfied.
    let matched = {
        let s = w.local_structure(id).expect("structure present");
        rc.machine_kind("base:forge").unwrap_or_default()
            .validate(s, MOUTH)
            .expect("the spawned shell validates")
    };
    {
        let s = w.local_structure_mut(id).expect("structure present");
        s.block_entities_mut()
            .insert(MOUTH, BlockEntity::Multiblock(charged_forge(&rc)));
        light_machine_at(s, MOUTH, rc.machine_kind("base:forge").unwrap_or_default(), matched)
            .expect("a charged in-structure forge lights");
    }
    let s = w.local_structure(id).expect("structure present");
    assert_eq!(
        s.get_block(MOUTH),
        b(&rc, "base:forge_lit"),
        "the structure's mouth glows"
    );
    let Some(BlockEntity::Multiblock(m)) = s.block_entities().get(&MOUTH) else {
        panic!("machine survived");
    };
    assert!(m.lit, "the fire is banked");

    // A storm on the main world means nothing to the structure's fire.
    w.force_local_weather("storm");
    let steps = (FORGE_FIRE_SECS / 0.5) as i32 + 4;
    for _ in 0..steps {
        w.tick_entities(0.5);
    }

    let structure = w.local_structure(id).expect("structure present");
    let Some(BlockEntity::Multiblock(f)) = structure.block_entities().get(&MOUTH) else {
        panic!("machine survived");
    };
    assert!(!f.lit, "the firing ended despite the storm");
    assert!(
        f.charge.iter().all(|s| s.is_none()),
        "the whole batch smelted"
    );
    let fuel_left: u32 = f.fuel.iter().flatten().map(|s| s.count).sum();
    assert_eq!(fuel_left, 4, "8 items burned 4 fuel (one per two)");
    assert_eq!(
        structure.get_block(MOUTH),
        b(&rc, "base:forge"),
        "the structure's mouth cools"
    );
    assert_eq!(
        outbox_ingots(structure),
        8,
        "eight ingots landed in the structure's outbox"
    );
}

#[test]
fn structure_shell_edits_are_scoped_from_the_main_world() {
    let rc = base_reg();
    let mut w = test_world_with("interiors-scope", rc.clone());

    // A lit world-hosted bloomery off to the side, firing in clear skies.
    build_bloomery(&mut w, &rc, 10, MY, 10);
    let iron = it(&rc, "base:iron_ingot");
    let coal = it(&rc, "base:charcoal");
    let mut wb = MachineInstance {
        kind: rc.machine_kind("base:bloomery").unwrap_or_default(),
        ..Default::default()
    };
    for i in 0..4 {
        wb.charge[i] = Some(ItemStack::new(&rc, iron, 2));
        wb.fuel[i] = Some(ItemStack::new(&rc, coal, 2));
    }
    w.insert_block_entity((10, MY, 10), BlockEntity::Multiblock(wb));
    w.light_bloomery(10, MY, 10).expect("world bloomery lights");
    w.force_local_weather("clear");
    w.tick_entities(1.0);

    // And a lit forge inside a structure across the map.
    build_forge(&mut w, &rc, 1, MY, 1);
    let tpl = capture_region(&w, bp(0, MY, 0), bp(3, MY + 5, 2), "forge").expect("forge captures");
    let anchor = bp(30, MY, 30);
    let id = w
        .spawn_structure(&tpl, anchor, Rotation::R0)
        .expect("spawns");
    let matched = {
        let s = w.local_structure(id).expect("structure present");
        rc.machine_kind("base:forge").unwrap_or_default()
            .validate(s, MOUTH)
            .expect("shell validates")
    };
    {
        let s = w.local_structure_mut(id).expect("structure present");
        s.block_entities_mut()
            .insert(MOUTH, BlockEntity::Multiblock(charged_forge(&rc)));
        light_machine_at(s, MOUTH, rc.machine_kind("base:forge").unwrap_or_default(), matched).expect("structure forge lights");
    }

    // A main-world block placed where the structure footprints doesn't
    // touch the structure store at all, and never confuses the two
    // machines for one another.
    let footprint = w
        .local_structure(id)
        .expect("structure present")
        .world_position((2, 0, 1))
        .expect("core cell");
    w.set_block_at(footprint, b(&rc, "base:stone"));
    let structure = w.local_structure(id).expect("structure present");
    assert_eq!(
        structure.get_block((2, 0, 1)),
        AIR,
        "a world block can never enter the structure store"
    );
    let Some(BlockEntity::Multiblock(m)) = structure.block_entities().get(&MOUTH) else {
        panic!()
    };
    assert!(m.lit, "a world edit never douses the structure machine");

    // Breaking a casing cell inside the structure douses ONLY the
    // structure's forge. The world bloomery keeps working.
    w.local_structure_mut(id)
        .expect("structure present")
        .set_block((1, 0, 0), AIR);
    let structure = w.local_structure(id).expect("structure present");
    let Some(BlockEntity::Multiblock(m)) = structure.block_entities().get(&MOUTH) else {
        panic!()
    };
    assert!(!m.lit, "the breach douses the in-structure forge");
    assert_eq!(
        structure.get_block(MOUTH),
        b(&rc, "base:forge"),
        "the structure's mouth cools on breach"
    );
    assert_eq!(
        m.charge.iter().flatten().map(|s| s.count).sum::<u32>(),
        8,
        "the charge survives the dousing"
    );
    let Some(BlockEntity::Multiblock(_)) = w.block_entity(&(10, MY, 10)) else {
        panic!("world bloomery present")
    };
    assert_eq!(
        w.get_block(10, MY, 10),
        b(&rc, "base:bloomery_lit"),
        "the world bloomery was not revalidated by the structure edit"
    );
    w.tick_entities(1.0);
    let world_progress = match w.block_entity(&(10, MY, 10)) {
        Some(BlockEntity::Multiblock(b)) => b.progress,
        _ => panic!("bloomery present"),
    };
    assert!(
        world_progress > 0.0,
        "the world bloomery kept firing while the structure was edited"
    );
    let structure = w.local_structure(id).expect("structure present");
    let Some(BlockEntity::Multiblock(m)) = structure.block_entities().get(&MOUTH) else {
        panic!()
    };
    assert!(!m.lit, "a breached shell does not auto-relight");

    // Repairing the cell re-folds the shell but stays unlit, exactly like
    // a world repair after a breach, and scoped to the structure.
    w.local_structure_mut(id)
        .expect("structure present")
        .set_block((1, 0, 0), b(&rc, "base:firebrick"));
    let structure = w.local_structure(id).expect("structure present");
    assert!(
        rc.machine_kind("base:forge").unwrap_or_default().validate(structure, MOUTH).is_some(),
        "repair re-validates"
    );
    let Some(BlockEntity::Multiblock(m)) = structure.block_entities().get(&MOUTH) else {
        panic!()
    };
    assert!(!m.lit, "a repaired shell waits for a fresh lighting");
}

#[test]
fn structure_hosted_machine_survives_save_and_reload() {
    let rc = base_reg();
    let mut w = test_world_with("interiors-persist", rc.clone());
    build_forge(&mut w, &rc, 1, MY, 1);
    let tpl = capture_region(&w, bp(0, MY, 0), bp(3, MY + 5, 2), "forge").expect("forge captures");
    let id = w
        .spawn_structure(&tpl, bp(30, MY, 30), Rotation::R0)
        .expect("spawns");

    let matched = {
        let s = w.local_structure(id).expect("structure present");
        rc.machine_kind("base:forge").unwrap_or_default()
            .validate(s, MOUTH)
            .expect("shell validates")
    };
    {
        let s = w.local_structure_mut(id).expect("structure present");
        s.block_entities_mut()
            .insert(MOUTH, BlockEntity::Multiblock(charged_forge(&rc)));
        light_machine_at(s, MOUTH, rc.machine_kind("base:forge").unwrap_or_default(), matched).expect("charges and lights");
    }
    for _ in 0..6 {
        w.tick_entities(0.5);
    }

    // Save mid-fire: charge, lit face, and progress must all survive.
    save_world(&mut w);
    let dir = w.save_dir_for_test();
    let mut reloaded = World::load_or_create(dir.clone(), rc.clone()).expect("world reloads");
    let structure = reloaded
        .local_structure(LocalStructureId(0))
        .expect("structure survives");
    let Some(BlockEntity::Multiblock(m)) = structure.block_entities().get(&MOUTH) else {
        panic!("in-structure machine survives")
    };
    assert_eq!(m.kind, rc.machine_kind("base:forge").unwrap_or_default());
    assert!(m.lit, "the lit forge reloads lit");
    assert!(
        m.progress > 0.0,
        "progress survived the round-trip (got {})",
        m.progress
    );
    let charge: u32 = m.charge.iter().flatten().map(|s| s.count).sum();
    assert_eq!(charge, 8, "the charge survived");
    assert_eq!(
        structure.get_block(MOUTH),
        b(&reloaded.reg, "base:forge_lit"),
        "the lit mouth block persisted"
    );

    // And the reloaded machine completes exactly like the original.
    let steps = (FORGE_FIRE_SECS / 0.5) as i32 + 4;
    for _ in 0..steps {
        reloaded.tick_entities(0.5);
    }
    let structure = reloaded
        .local_structure(LocalStructureId(0))
        .expect("structure survives");
    let Some(BlockEntity::Multiblock(f)) = structure.block_entities().get(&MOUTH) else {
        panic!()
    };
    assert!(!f.lit, "the reloaded machine fires to completion");
    assert_eq!(outbox_ingots(structure), 8, "and lands its outputs");
}

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

/// Absolute‑frame BlockPos — the same coordinate frame the world DDA
/// (`cast_at`) walks in (absolute `u`, `y`, `v` on the face).
fn bp_abs(u: i32, y: i32, v: i32) -> crate::planet::BlockPos {
    crate::planet::BlockPos::new(crate::planet::Face::PosZ, u as u16, y as u8, v as u16)
        .expect("valid absolute coordinate")
}

/// Absolute‑frame EntityPos — same frame as the world DDA.
fn origin_abs(u: f32, y: f32, v: f32) -> crate::planet::EntityPos {
    crate::planet::EntityPos::new(crate::planet::Face::PosZ, u, y, v)
        .expect("valid absolute entity position")
}

/// A single‑cell template captured from a loaded absolute position, well
/// above the ray path. Only the cell offsets matter, not the capture site.
fn make_single_abs(w: &mut World, rc: &Registry) -> crate::world::template::Template {
    let site = bp_abs(4080, MY + 50, 4080);
    w.set_block_at(site, b(rc, "base:firebrick"));
    capture_region(w, site, site, "s").expect("capture")
}

/// Face‑centre absolute u/v coordinates.
const TU: i32 = 4096;
const TV: i32 = 4096;

/// Spawn a single‑cell structure at absolute `(TU + cu, MY, TV)`.
fn spawn_single_abs(w: &mut World, rc: &Registry, cu: i32) -> LocalStructureId {
    let tpl = make_single_abs(w, rc);
    w.spawn_structure(&tpl, bp_abs(TU + cu, MY, TV), Rotation::R0)
        .expect("spawn")
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

#[test]
fn structure_break_and_place_round_trip() {
    use crate::world::BlockEntity;

    let rc = base_reg();

    // Build a single‑cell structure, place a block, break it, check drop.
    {
        let mut w = test_world_with("str-break-place", rc.clone());
        w.set_block_at(bp_abs(4080, MY + 50, 4080), b(&rc, "base:stone"));
        let tpl = capture_region(
            &w,
            bp_abs(4080, MY + 50, 4080),
            bp_abs(4080, MY + 50, 4080),
            "s",
        )
        .expect("capture");
        let id = w
            .spawn_structure(&tpl, bp_abs(TU + 10, MY, TV), Rotation::R0)
            .expect("spawn");

        let s = w.local_structure_mut(id).unwrap();
        assert!(s.place_block((5, 0, 0), b(&rc, "base:stone")));
        assert_eq!(s.get_block((5, 0, 0)), b(&rc, "base:stone"));

        let pick = it(&rc, "base:wood_pickaxe");
        let drop = s.break_block((5, 0, 0), Some(pick));
        let Some(drop) = drop else {
            panic!("expected drop")
        };
        assert!(drop.item == rc.item_id("base:cobblestone").unwrap());
        assert_eq!(s.get_block((5, 0, 0)), AIR, "cell cleared");
    }

    // Breaking a firebrick inside a forge shell douses the machine
    // (revalidation).
    {
        let mut w = test_world_with("str-breach-revalidate", rc.clone());
        build_forge(&mut w, &rc, 1, MY, 1);
        let tpl =
            capture_region(&w, bp(0, MY, 0), bp(3, MY + 5, 2), "forge").expect("forge capture");
        let id = w
            .spawn_structure(&tpl, bp_abs(30, MY, 30), Rotation::R0)
            .expect("spawns");

        let s = w.local_structure_mut(id).unwrap();
        assert!(
            rc.machine_kind("base:forge").unwrap_or_default().validate(s, MOUTH).is_some(),
            "shell validates fresh from spawn"
        );

        s.block_entities_mut()
            .insert(MOUTH, BlockEntity::Multiblock(charged_forge(&rc)));
        let matched = rc.machine_kind("base:forge").unwrap_or_default()
            .validate(s, MOUTH)
            .expect("still validates");
        light_machine_at(s, MOUTH, rc.machine_kind("base:forge").unwrap_or_default(), matched).expect("lights");
        assert_eq!(s.get_block(MOUTH), b(&rc, "base:forge_lit"));

        s.break_block((1, 0, 0), None);
        let Some(BlockEntity::Multiblock(m)) = s.block_entities().get(&MOUTH) else {
            panic!("machine entity survives the break")
        };
        assert!(!m.lit, "the breach douses the forge");
        assert_eq!(
            s.get_block(MOUTH),
            b(&rc, "base:forge"),
            "mouth cools on breach"
        );
        let charge: u32 = m.charge.iter().flatten().map(|s| s.count).sum();
        assert_eq!(charge, 8, "charge is preserved through dousing");

        s.place_block((1, 0, 0), b(&rc, "base:firebrick"));
        assert!(
            rc.machine_kind("base:forge").unwrap_or_default().validate(s, MOUTH).is_some(),
            "repair re-validates"
        );
        let Some(BlockEntity::Multiblock(m)) = s.block_entities().get(&MOUTH) else {
            panic!("machine present after repair")
        };
        assert!(!m.lit, "repair does not auto-relight (matches world)");
    }
}
