//! Tests for belt transport (spec §2.2): straight travel in all four
//! orientations, curves, inclines, switches, splitters/mergers, machine
//! feed, persistence, and the same load-tier power draw machinery trains
//! use.

use super::*;
use crate::entity::ItemEntity;
use crate::inventory::ItemStack;
use crate::world::belt::{BELT_CELL_CAPACITY, BeltKind};

const MY: i32 = 120;

fn belt_cell(w: &mut World, rc: &Registry, pos: (i32, i32, i32)) {
    w.set_block_at(bp(pos.0, pos.1, pos.2), b(rc, "base:belt"));
}

/// Park a loose item at belt level in the mouth cell `pos` (directly north
/// of the last belt cell). The belt floats well above the generated terrain,
/// so a solid pad beneath the mouth plus small settle ticks keeps the cap
/// item resting in that cell instead of falling to the distant floor.
fn park_mouth_item(
    w: &mut World,
    rc: &Registry,
    pos: (i32, i32, i32),
    ingot: crate::registry::ItemId,
) {
    for dy in 1..=5 {
        w.set_block_at(bp(pos.0, pos.1 - dy, pos.2), b(rc, "base:stone"));
    }
    w.spawn_loose_item(ItemEntity::new(
        bp(pos.0, pos.1, pos.2).entity_center(),
        glam::Vec3::ZERO,
        ingot,
        1,
    ));
    for _ in 0..5 {
        w.tick_entities(0.05);
    }
}

#[test]
fn belt_kind_classifies_the_belt_block() {
    let rc = base_reg();
    assert_eq!(
        BeltKind::from_block(&rc, b(&rc, "base:belt")),
        Some(BeltKind::Straight(crate::planet::Direction4::North))
    );
    assert_eq!(BeltKind::from_block(&rc, b(&rc, "base:stone")), None);
}

#[test]
fn belt_insert_rejects_non_belts_and_overflow() {
    let rc = base_reg();
    let mut w = test_world_with("belt-insert", rc.clone());
    let stack = ItemStack::new(&rc, it(&rc, "base:copper_ingot"), 1);
    // Not a belt: refuse.
    assert!(!w.belt_insert_at(bp(0, MY, 0), stack));
    // A belt accepts one stack per cell (capacity 1), then refuses more.
    belt_cell(&mut w, &rc, (0, MY, 0));
    assert!(w.belt_insert_at(bp(0, MY, 0), stack));
    assert!(!w.belt_insert_at(bp(0, MY, 0), stack), "one stack per cell");
    assert_eq!(
        w.belt_cell_at(bp(0, MY, 0)).unwrap().cargo.len(),
        BELT_CELL_CAPACITY
    );
}

#[test]
fn belt_carries_cargo_north_one_cell_per_half_second() {
    let rc = base_reg();
    let mut w = test_world_with("belt-advance", rc.clone());
    belt_cell(&mut w, &rc, (0, MY, 0));
    belt_cell(&mut w, &rc, (0, MY, 1));
    let ingot = it(&rc, "base:copper_ingot");
    assert!(w.belt_insert_at(bp(0, MY, 0), ItemStack::new(&rc, ingot, 1)));

    // BELT_SPEED = 2 cells/s, so half a second moves the front item on.
    w.tick_entities(0.5);
    assert!(
        w.belt_cell_at(bp(0, MY, 0))
            .is_none_or(|state| state.cargo.is_empty()),
        "the cargo left the first cell"
    );
    let next = w.belt_cell_at(bp(0, MY, 1)).unwrap();
    assert_eq!(next.cargo.len(), 1, "the cargo rides the next cell");
    assert_eq!(next.cargo[0].item, ingot);
}

#[test]
fn belt_drops_cargo_as_a_loose_item_at_the_mouth() {
    let rc = base_reg();
    let mut w = test_world_with("belt-mouth", rc.clone());
    belt_cell(&mut w, &rc, (0, MY, 0));
    let ingot = it(&rc, "base:copper_ingot");
    w.belt_insert_at(bp(0, MY, 0), ItemStack::new(&rc, ingot, 1));

    w.tick_entities(0.5);
    let drops = w.pending_drops();
    assert_eq!(drops.len(), 1, "the cargo left the belt");
    assert_eq!(drops[0].0, bp(0, MY, 1), "dropped just past the mouth");
    assert_eq!(drops[0].1.item, ingot);
    assert!(
        w.belt_cell_at(bp(0, MY, 0))
            .is_none_or(|state| state.cargo.is_empty()),
        "the belt cell is drained"
    );
}

#[test]
fn belt_backpressures_through_a_full_line_to_a_capped_mouth() {
    let rc = base_reg();
    let mut w = test_world_with("belt-blocked", rc.clone());
    belt_cell(&mut w, &rc, (0, MY, 0));
    belt_cell(&mut w, &rc, (0, MY, 1));
    let ingot = it(&rc, "base:copper_ingot");
    // Cap the mouth of the far cell (0,MY,1) — the next cell north — with a
    // loose item, parked at belt level before any cargo is fed.
    park_mouth_item(&mut w, &rc, (0, MY, 2), ingot);
    w.belt_insert_at(bp(0, MY, 0), ItemStack::new(&rc, ingot, 1));
    w.belt_insert_at(bp(0, MY, 1), ItemStack::new(&rc, ingot, 1));

    w.tick_entities(0.3);
    // The far cell can't push into the capped mouth, so the cell behind it
    // finds it still full and stalls too: neither advances, nothing drops.
    assert!(w.pending_drops().is_empty(), "nothing left the line");
    assert_eq!(
        w.belt_cell_at(bp(0, MY, 0)).unwrap().cargo.len(),
        1,
        "the first cell held by the full cell ahead"
    );
    assert_eq!(
        w.belt_cell_at(bp(0, MY, 1)).unwrap().cargo.len(),
        1,
        "the far cell held by the capped mouth"
    );

    // Clear the mouth: the line drains North. The far cell frees itself,
    // then its neighbour's cargo rides forward into the emptied far cell.
    // The freshly-dropped item then caps the mouth again, so only that one
    // item leaves the belt.
    w.clear_loose_items();
    w.tick_entities(0.3);
    w.tick_entities(0.3);
    let drops = w.take_pending_drops();
    assert_eq!(drops.len(), 1, "the far cell frees itself");
    w.tick_entities(0.3);
    assert!(
        w.belt_cell_at(bp(0, MY, 0))
            .is_none_or(|state| state.cargo.is_empty()),
        "the near cell's cargo rode forward"
    );
}

#[test]
fn belt_stalls_when_the_mouth_is_capped_with_loose_items() {
    let rc = base_reg();
    let mut w = test_world_with("belt-capped", rc.clone());
    belt_cell(&mut w, &rc, (0, MY, 0));
    let ingot = it(&rc, "base:copper_ingot");
    // A single loose item already sits at the mouth (0,MY,1).
    park_mouth_item(&mut w, &rc, (0, MY, 1), ingot);
    w.belt_insert_at(bp(0, MY, 0), ItemStack::new(&rc, ingot, 1));

    w.tick_entities(0.3);
    assert!(
        w.pending_drops().is_empty(),
        "a capped mouth holds the belt"
    );
    assert_eq!(
        w.belt_cell_at(bp(0, MY, 0)).unwrap().cargo.len(),
        1,
        "the cargo stays put"
    );
    assert_eq!(
        w.belt_cell_at(bp(0, MY, 0)).unwrap().progress,
        0.0,
        "a stalled belt demands nothing"
    );

    // Clear the mouth: the belt resumes.
    w.clear_loose_items();
    w.tick_entities(0.3);
    w.tick_entities(0.3);
    assert_eq!(w.pending_drops().len(), 1, "the belt resumes once freed");
}

#[test]
fn belt_with_heavy_cargo_stalls_without_power_and_rolls_with_it() {
    let rc = base_reg();
    let mut w = test_world_with("belt-power", rc.clone());
    belt_cell(&mut w, &rc, (12, MY, 10));
    belt_cell(&mut w, &rc, (12, MY, 11));
    // 30 copper ingots × 1.2 mass = 36 → Heavy tier (draw 1.2).
    let ingot = it(&rc, "base:copper_ingot");
    w.belt_insert_at(bp(12, MY, 10), ItemStack::new(&rc, ingot, 30));

    // Unpowered: Heavy can't meet its draw, so the belt holds.
    w.tick_entities(0.5);
    assert_eq!(
        w.belt_cell_at(bp(12, MY, 10)).unwrap().progress,
        0.0,
        "a heavy belt without line power jams"
    );

    // Steam plant above the belt cell (same layout as the rail test).
    let (fx, fy, fz) = (10, 120, 10);
    assert!(w.place_block((fx, fy, fz), b(&rc, "base:firebox")));
    w.set_block(fx, fy + 1, fz, b(&rc, "base:boiler"));
    w.set_block(fx + 1, fy + 1, fz, b(&rc, "base:steam_engine"));
    w.set_block(fx + 2, fy + 1, fz, b(&rc, "base:gear"));
    if let Some(crate::world::BlockEntity::Steam(s)) = w.block_entity_mut(&(fx, fy, fz)) {
        s.fuel = 60.0;
    }
    w.set_block(fx, fy + 1, fz - 1, rc.water_block(0));
    // Short powered ticks keep the Heavy belt on its first cell: BELT_SPEED
    // (2.0) × the Heavy step (0.55) is a sliver under one cell per second,
    // so a 0.25s tick rolls it without handing off.
    w.tick_entities(0.25);
    assert!(w.power_at(12, MY, 10) > 0.0, "the boiler drives the line");
    w.tick_entities(0.25);
    assert!(
        w.belt_cell_at(bp(12, MY, 10)).unwrap().progress > 0.0,
        "powered steam rolls the heavy belt"
    );
}

/// Place a named belt piece (capability E8 geometry).
fn belt_of(w: &mut World, rc: &Registry, pos: (i32, i32, i32), name: &str) {
    w.set_block_at(bp(pos.0, pos.1, pos.2), b(rc, name));
}

#[test]
fn belt_straights_carry_along_their_facing() {
    let rc = base_reg();
    let ingot = it(&rc, "base:copper_ingot");
    // East: (12,MY,12) -> (13,MY,12).
    let mut w = test_world_with("belt-orient-e", rc.clone());
    belt_of(&mut w, &rc, (12, MY, 12), "base:belt_e");
    belt_of(&mut w, &rc, (13, MY, 12), "base:belt_e");
    assert!(w.belt_insert_at(bp(12, MY, 12), ItemStack::new(&rc, ingot, 1)));
    w.tick_entities(0.5);
    assert!(
        w.belt_cell_at(bp(12, MY, 12))
            .is_none_or(|s| s.cargo.is_empty()),
        "the cargo left the east-facing cell"
    );
    assert_eq!(
        w.belt_cell_at(bp(13, MY, 12)).unwrap().cargo.len(),
        1,
        "an east belt carries east"
    );
    // South: (12,MY,12) -> (12,MY,11).
    let mut w = test_world_with("belt-orient-s", rc.clone());
    belt_of(&mut w, &rc, (12, MY, 12), "base:belt_s");
    belt_of(&mut w, &rc, (12, MY, 11), "base:belt_s");
    assert!(w.belt_insert_at(bp(12, MY, 12), ItemStack::new(&rc, ingot, 1)));
    w.tick_entities(0.5);
    assert_eq!(
        w.belt_cell_at(bp(12, MY, 11)).unwrap().cargo.len(),
        1,
        "a south belt carries south"
    );
    // West: (12,MY,12) -> (11,MY,12).
    let mut w = test_world_with("belt-orient-w", rc.clone());
    belt_of(&mut w, &rc, (12, MY, 12), "base:belt_w");
    belt_of(&mut w, &rc, (11, MY, 12), "base:belt_w");
    assert!(w.belt_insert_at(bp(12, MY, 12), ItemStack::new(&rc, ingot, 1)));
    w.tick_entities(0.5);
    assert_eq!(
        w.belt_cell_at(bp(11, MY, 12)).unwrap().cargo.len(),
        1,
        "a west belt carries west"
    );
}

#[test]
fn belt_curves_turn_cargo() {
    let rc = base_reg();
    let ingot = it(&rc, "base:copper_ingot");
    // Traveling North into a NE curve turns East.
    let mut w = test_world_with("belt-curve-ne", rc.clone());
    belt_of(&mut w, &rc, (12, MY, 12), "base:belt_curve_ne");
    belt_of(&mut w, &rc, (13, MY, 12), "base:belt_e");
    belt_of(&mut w, &rc, (14, MY, 12), "base:belt_e");
    assert!(w.belt_insert_at(bp(12, MY, 12), ItemStack::new(&rc, ingot, 1)));
    w.tick_entities(0.5);
    assert_eq!(
        w.belt_cell_at(bp(13, MY, 12)).unwrap().cargo.len(),
        1,
        "a NE curve turns northbound cargo east"
    );
    // Traveling East into an SE curve turns South.
    let mut w = test_world_with("belt-curve-se", rc.clone());
    belt_of(&mut w, &rc, (11, MY, 12), "base:belt_e");
    belt_of(&mut w, &rc, (12, MY, 12), "base:belt_curve_se");
    belt_of(&mut w, &rc, (12, MY, 11), "base:belt_s");
    assert!(w.belt_insert_at(bp(11, MY, 12), ItemStack::new(&rc, ingot, 1)));
    w.tick_entities(0.5);
    w.tick_entities(0.5);
    assert_eq!(
        w.belt_cell_at(bp(12, MY, 11)).unwrap().cargo.len(),
        1,
        "an SE curve turns eastbound cargo south"
    );
}

#[test]
fn belt_incline_climbs_north() {
    let rc = base_reg();
    let mut w = test_world_with("belt-incline", rc.clone());
    let ingot = it(&rc, "base:copper_ingot");
    belt_of(&mut w, &rc, (12, MY, 11), "base:belt");
    belt_of(&mut w, &rc, (12, MY, 12), "base:belt_incline_n");
    belt_of(&mut w, &rc, (12, MY + 1, 13), "base:belt");
    assert!(w.belt_insert_at(bp(12, MY, 11), ItemStack::new(&rc, ingot, 1)));
    w.tick_entities(0.5);
    w.tick_entities(0.5);
    assert!(
        w.belt_cell_at(bp(12, MY, 12))
            .is_none_or(|s| s.cargo.is_empty()),
        "the cargo left the incline"
    );
    assert_eq!(
        w.belt_cell_at(bp(12, MY + 1, 13)).unwrap().cargo.len(),
        1,
        "an incline climbs one block north"
    );
}

#[test]
fn belt_switch_defaults_straight_and_branches_when_toggled() {
    let rc = base_reg();
    let ingot = it(&rc, "base:copper_ingot");
    // Default selection keeps the mainline.
    let mut w = test_world_with("belt-switch-def", rc.clone());
    belt_of(&mut w, &rc, (12, MY, 11), "base:belt");
    belt_of(&mut w, &rc, (12, MY, 12), "base:belt_switch");
    belt_of(&mut w, &rc, (12, MY, 13), "base:belt");
    assert!(w.belt_insert_at(bp(12, MY, 11), ItemStack::new(&rc, ingot, 1)));
    w.tick_entities(0.5);
    w.tick_entities(0.5);
    assert_eq!(
        w.belt_cell_at(bp(12, MY, 13)).unwrap().cargo.len(),
        1,
        "an un-toggled switch continues the mainline north"
    );
    // Toggled: northbound cargo branches east.
    let mut w = test_world_with("belt-switch-branch", rc.clone());
    belt_of(&mut w, &rc, (12, MY, 11), "base:belt");
    belt_of(&mut w, &rc, (12, MY, 12), "base:belt_switch");
    belt_of(&mut w, &rc, (13, MY, 12), "base:belt_e");
    w.toggle_switch(bp(12, MY, 12));
    assert!(w.belt_insert_at(bp(12, MY, 11), ItemStack::new(&rc, ingot, 1)));
    w.tick_entities(0.5);
    w.tick_entities(0.5);
    assert_eq!(
        w.belt_cell_at(bp(13, MY, 12)).unwrap().cargo.len(),
        1,
        "a toggled switch branches east"
    );
}

#[test]
fn belt_splitter_alternates_north_and_east() {
    let rc = base_reg();
    let mut w = test_world_with("belt-splitter", rc.clone());
    let ingot = it(&rc, "base:copper_ingot");
    belt_of(&mut w, &rc, (12, MY, 12), "base:belt_splitter");
    belt_of(&mut w, &rc, (12, MY, 13), "base:belt");
    belt_of(&mut w, &rc, (13, MY, 12), "base:belt_e");
    assert!(w.belt_insert_at(bp(12, MY, 12), ItemStack::new(&rc, ingot, 1)));
    w.tick_entities(0.5);
    assert_eq!(
        w.belt_cell_at(bp(12, MY, 13)).unwrap().cargo.len(),
        1,
        "the first split item goes north"
    );
    assert!(w.belt_insert_at(bp(12, MY, 12), ItemStack::new(&rc, ingot, 1)));
    w.tick_entities(0.5);
    assert_eq!(
        w.belt_cell_at(bp(13, MY, 12)).unwrap().cargo.len(),
        1,
        "the second split item goes east"
    );
}

#[test]
fn belt_merger_joins_two_inputs_into_one_output() {
    let rc = base_reg();
    let mut w = test_world_with("belt-merger", rc.clone());
    let ingot = it(&rc, "base:copper_ingot");
    belt_of(&mut w, &rc, (12, MY, 11), "base:belt");
    belt_of(&mut w, &rc, (11, MY, 12), "base:belt_e");
    belt_of(&mut w, &rc, (12, MY, 12), "base:belt_merger");
    for v in 13..=16 {
        belt_of(&mut w, &rc, (12, MY, v), "base:belt");
    }
    assert!(w.belt_insert_at(bp(12, MY, 11), ItemStack::new(&rc, ingot, 1)));
    assert!(w.belt_insert_at(bp(11, MY, 12), ItemStack::new(&rc, ingot, 1)));
    for _ in 0..4 {
        w.tick_entities(0.5);
    }
    let on_line = (13..=16)
        .filter(|v| {
            w.belt_cell_at(bp(12, MY, *v))
                .is_some_and(|s| !s.cargo.is_empty())
        })
        .count();
    assert_eq!(on_line, 2, "both inputs merged north onto the output line");
}

#[test]
fn belt_backpressures_through_a_curve() {
    let rc = base_reg();
    let mut w = test_world_with("belt-curve-block", rc.clone());
    let ingot = it(&rc, "base:copper_ingot");
    belt_of(&mut w, &rc, (12, MY, 11), "base:belt");
    belt_of(&mut w, &rc, (12, MY, 12), "base:belt_curve_ne");
    belt_of(&mut w, &rc, (13, MY, 12), "base:belt_e");
    // Cap the east mouth with a loose item.
    park_mouth_item(&mut w, &rc, (14, MY, 12), ingot);
    assert!(w.belt_insert_at(bp(12, MY, 11), ItemStack::new(&rc, ingot, 1)));
    assert!(w.belt_insert_at(bp(12, MY, 12), ItemStack::new(&rc, ingot, 1)));
    w.tick_entities(0.3);
    assert!(w.pending_drops().is_empty(), "nothing left the line");
    assert_eq!(
        w.belt_cell_at(bp(12, MY, 12)).unwrap().cargo.len(),
        1,
        "the curve cell held by the capped mouth"
    );
    assert_eq!(
        w.belt_cell_at(bp(12, MY, 11)).unwrap().cargo.len(),
        1,
        "the feeder held by the full curve cell"
    );
}

#[test]
fn belt_feeds_a_machine_mouth_and_backpressures_when_full() {
    let reg = base_reg();
    let mut w = test_world_with("belt-feed", reg.clone());
    let my = 120;
    build_bloomery(&mut w, &reg, 12, my, 12);
    let iron = it(&reg, "base:iron_ingot");
    // The bloomery shell leaves its West side open, so the belt runs West
    // of the mouth and feeds East into it.
    belt_of(&mut w, &reg, (11, my, 12), "base:belt_e");
    assert!(w.belt_insert_at(bp(11, my, 12), ItemStack::new(&reg, iron, 1)));
    w.tick_entities(0.5);
    let Some(crate::world::BlockEntity::Multiblock(m)) = w.block_entity(&(12, my, 12)) else {
        panic!("the belt created the machine instance");
    };
    assert!(
        m.charge
            .iter()
            .any(|s| s.as_ref().is_some_and(|s| s.item == iron)),
        "the first belt-fed ingot landed in a charge slot"
    );
    assert!(
        w.pending_drops().is_empty(),
        "no loose drop while it accepts"
    );
    // Feed three more distinct ingots so all four charge slots fill (same
    // items would merge into one slot, which is fine but doesn't fill it).
    for name in ["base:copper_ingot", "base:gold_ingot", "base:silver_ingot"] {
        let item = it(&reg, name);
        assert!(w.belt_insert_at(bp(11, my, 12), ItemStack::new(&reg, item, 1)));
        w.tick_entities(0.5);
    }
    let Some(crate::world::BlockEntity::Multiblock(m)) = w.block_entity(&(12, my, 12)) else {
        panic!()
    };
    let n = m.charge.iter().filter(|s| s.is_some()).count();
    assert_eq!(n, 4, "the machine holds its four belt-fed ingots");
    // A full machine back-pressures the belt instead of overflowing.
    assert!(w.belt_insert_at(bp(11, my, 12), ItemStack::new(&reg, iron, 1)));
    w.tick_entities(0.5);
    assert_eq!(
        w.belt_cell_at(bp(11, my, 12)).unwrap().cargo.len(),
        1,
        "the belt stalls against a full machine"
    );
    assert!(w.pending_drops().is_empty(), "no overflow drop");
}

#[test]
fn belt_cargo_survives_save_and_reload() {
    let rc = base_reg();
    let mut w = test_world_with("belt-persist", rc.clone());
    let ingot = it(&rc, "base:copper_ingot");
    belt_of(&mut w, &rc, (12, MY, 11), "base:belt");
    belt_of(&mut w, &rc, (12, MY, 12), "base:belt");
    assert!(w.belt_insert_at(bp(12, MY, 11), ItemStack::new(&rc, ingot, 1)));
    w.tick_entities(0.4);
    save_world(&mut w);
    let dir = w.save_dir_for_test();
    let mut reloaded = World::load_or_create(dir, rc.clone()).expect("world reloads");
    for x in -2..=2 {
        for z in -2..=2 {
            reloaded.ensure_chunk(tchunk(x, z));
        }
    }
    let cell = reloaded.belt_cell_at(bp(12, MY, 11)).unwrap();
    assert_eq!(cell.cargo.len(), 1, "belt cargo persisted");
    assert!(
        (0.0..1.0).contains(&cell.progress),
        "belt progress persisted"
    );
    // And the reloaded line resumes moving.
    reloaded.tick_entities(0.2);
    reloaded.tick_entities(0.1);
    assert!(
        reloaded
            .belt_cell_at(bp(12, MY, 11))
            .is_none_or(|s| s.cargo.is_empty()),
        "the reloaded belt finished its cell"
    );
    assert_eq!(
        reloaded.belt_cell_at(bp(12, MY, 12)).unwrap().cargo.len(),
        1,
        "the reloaded cargo rode forward"
    );
}

#[test]
fn belt_splitter_phase_survives_a_drain_and_reload() {
    let rc = base_reg();
    let mut w = test_world_with("belt-split-persist", rc.clone());
    let ingot = it(&rc, "base:copper_ingot");
    belt_of(&mut w, &rc, (12, MY, 12), "base:belt_splitter");
    belt_of(&mut w, &rc, (12, MY, 13), "base:belt");
    assert!(w.belt_insert_at(bp(12, MY, 12), ItemStack::new(&rc, ingot, 1)));
    w.tick_entities(0.5);
    assert!(
        w.belt_cell_at(bp(12, MY, 12)).unwrap().split_phase,
        "the splitter flipped after its first handoff"
    );
    save_world(&mut w);
    let mut reloaded = World::load_or_create(w.save_dir_for_test(), rc.clone()).expect("reloads");
    for x in -2..=2 {
        for z in -2..=2 {
            reloaded.ensure_chunk(tchunk(x, z));
        }
    }
    assert!(
        reloaded.belt_cell_at(bp(12, MY, 12)).unwrap().split_phase,
        "the splitter phase persisted"
    );
    // The reloaded splitter prefers the East branch next.
    belt_of(&mut reloaded, &rc, (13, MY, 12), "base:belt_e");
    assert!(reloaded.belt_insert_at(bp(12, MY, 12), ItemStack::new(&rc, ingot, 1)));
    reloaded.tick_entities(0.5);
    assert_eq!(
        reloaded.belt_cell_at(bp(13, MY, 12)).unwrap().cargo.len(),
        1,
        "the reloaded splitter resumed alternating east"
    );
}
