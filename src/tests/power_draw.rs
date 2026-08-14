//! Tests for mass-driven power draw (spec §2.2): the material-weight
//! catalog, tier quantization, draw/speed rules, and rail power gating.

use super::*;
use crate::inventory::ItemStack;
use crate::world::local_structure::{LocalStructureId, RailState};
use crate::world::multiblock::Rotation;
use crate::world::power_draw::{
    LoadTier, TIER_THRESHOLDS, block_mass, effective_speed, incline_multiplier, item_mass,
    item_stack_mass, load_tier_for_mass, load_tier_rate, material_unit_mass, tier_speed_step,
};
use crate::world::rail::RailKind;

const MY: i32 = 120;

#[test]
fn material_unit_mass_resolves_known_and_unknown_names() {
    let rc = base_reg();
    let _ = &rc;
    assert_eq!(material_unit_mass("copper"), 0.0010);
    assert_eq!(material_unit_mass("choirstone"), 0.0012);
    assert_eq!(material_unit_mass("wood"), 0.0005);
    assert_eq!(material_unit_mass("gold"), 0.0020);
    assert_eq!(
        material_unit_mass("netherite"),
        0.001,
        "unknown materials use a middle default so mod content is never free"
    );
}

#[test]
fn block_mass_uses_declared_materials_and_falls_back_by_class() {
    let rc = base_reg();
    // Choirstone declares 1200 units of choirstone (0.0012 each) = 1.44.
    assert_eq!(
        block_mass(&rc, b(&rc, "base:choirstone")),
        1200.0 * 0.0012,
        "declared materials win"
    );
    // Firebrick declares nothing: its inferred class (TransformativeFinite)
    // falls back to the per-class nominal mass.
    assert_eq!(
        block_mass(&rc, b(&rc, "base:firebrick")),
        0.8,
        "undeclared content uses the class fallback"
    );
}

#[test]
fn item_mass_and_stack_mass_follow_the_same_rules() {
    let rc = base_reg();
    // Copper ingot declares 1200 units of copper (0.0010 each) = 1.2.
    let ingot = it(&rc, "base:copper_ingot");
    assert_eq!(item_mass(&rc, ingot), 1200.0 * 0.0010);
    assert!(
        (item_stack_mass(&rc, ItemStack::new(&rc, ingot, 3)) - 3.6).abs() < 1e-5,
        "three ingots weigh 3.6"
    );
}

#[test]
fn load_tiers_quantize_mass_at_the_declared_thresholds() {
    assert_eq!(load_tier_for_mass(0.0), LoadTier::Empty);
    assert_eq!(load_tier_for_mass(3.99), LoadTier::Empty);
    assert_eq!(load_tier_for_mass(TIER_THRESHOLDS[0]), LoadTier::Light);
    assert_eq!(load_tier_for_mass(11.99), LoadTier::Light);
    assert_eq!(load_tier_for_mass(TIER_THRESHOLDS[1]), LoadTier::Medium);
    assert_eq!(load_tier_for_mass(31.99), LoadTier::Medium);
    assert_eq!(load_tier_for_mass(TIER_THRESHOLDS[2]), LoadTier::Heavy);
    assert_eq!(load_tier_for_mass(127.99), LoadTier::Heavy);
    assert_eq!(load_tier_for_mass(TIER_THRESHOLDS[3]), LoadTier::Overloaded);
}

#[test]
fn heavier_is_a_strict_tier_walk_that_never_leaves_overloaded() {
    assert_eq!(LoadTier::Empty.heavier(), LoadTier::Light);
    assert_eq!(LoadTier::Light.heavier(), LoadTier::Medium);
    assert_eq!(LoadTier::Medium.heavier(), LoadTier::Heavy);
    assert_eq!(LoadTier::Heavy.heavier(), LoadTier::Overloaded);
    assert_eq!(LoadTier::Overloaded.heavier(), LoadTier::Overloaded);
}

#[test]
fn draw_rates_rise_and_speed_steps_fall_with_the_tier() {
    let tiers = [
        LoadTier::Empty,
        LoadTier::Light,
        LoadTier::Medium,
        LoadTier::Heavy,
        LoadTier::Overloaded,
    ];
    let rates: Vec<f32> = tiers.iter().map(|t| load_tier_rate(*t)).collect();
    for w in rates.windows(2) {
        assert!(w[1] > w[0], "draw rates strictly rise with tier");
    }
    assert_eq!(
        load_tier_rate(LoadTier::Empty),
        0.0,
        "empty cargo draws nothing"
    );
    let steps: Vec<f32> = tiers.iter().map(|t| tier_speed_step(*t)).collect();
    for w in steps.windows(2) {
        assert!(w[1] < w[0], "speed steps strictly fall with tier");
    }
    assert_eq!(
        tier_speed_step(LoadTier::Overloaded),
        0.0,
        "overload is a jam"
    );
}

#[test]
fn effective_speed_meets_draw_steps_heavier_and_stalls_on_overload() {
    let nominal = 1.0;
    // Draw met: the tier runs at its own step.
    assert_eq!(
        effective_speed(LoadTier::Empty, 0.0, 0.0, nominal),
        1.0,
        "empty runs full speed on a dead line"
    );
    assert_eq!(
        effective_speed(LoadTier::Light, 0.4, 0.4, nominal),
        0.85,
        "draw met holds the tier's step"
    );
    // Shortfall: step one heavier (slower), never harder than overload.
    assert_eq!(
        effective_speed(LoadTier::Light, 0.3, 0.4, nominal),
        0.7,
        "a light load short on power runs at the medium step"
    );
    assert_eq!(
        effective_speed(LoadTier::Heavy, 0.0, 1.2, nominal),
        0.0,
        "a heavy load with no line power jams"
    );
    // Overloaded always stalls, even with ample power.
    assert_eq!(
        effective_speed(LoadTier::Overloaded, 10.0, 1.8, nominal),
        0.0
    );
}

#[test]
fn incline_only_costs_more_while_climbing() {
    assert_eq!(incline_multiplier(None, false), 1.0);
    assert_eq!(incline_multiplier(Some(RailKind::Straight), true), 1.0);
    assert_eq!(
        incline_multiplier(
            Some(RailKind::Curve(crate::world::rail::CurveOrientation::NE)),
            false
        ),
        1.0
    );
    assert_eq!(
        incline_multiplier(Some(RailKind::Incline), true),
        1.5,
        "climbing a ramp costs half again"
    );
    assert_eq!(
        incline_multiplier(Some(RailKind::Incline), false),
        1.0,
        "descending is not cheaper"
    );
}

fn rail_car(w: &mut World, rc: &Registry) -> crate::world::template::Template {
    w.set_block_at(bp(0, MY, 0), b(rc, "base:firebrick"));
    w.capture_and_save(bp(0, MY, 0), bp(0, MY, 0), "power-car")
        .expect("captured");
    w.template("power-car").cloned().expect("car template")
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

/// A 1-block car loaded with enough choirstone to cross into the Heavy tier
/// (mass 32.0, draw 1.2). The base block plus 30 choirstone (0.8 + 30×1.44
/// = 44.0) clears Heavy comfortably without reaching Overloaded.
fn heavy_car(w: &mut World, rc: &Registry) -> LocalStructureId {
    let tpl = rail_car(w, rc);
    let id = w
        .spawn_structure(&tpl, bp(12, MY, 10), Rotation::R0)
        .expect("spawns");
    let choirstone = b(rc, "base:choirstone");
    for du in 1..=30 {
        w.local_structure_mut(id)
            .unwrap()
            .set_block((du, 0, 0), choirstone);
    }
    id
}

#[test]
fn heavy_car_stalls_without_power_and_rolls_when_steam_arrives() {
    let rc = base_reg();
    let mut w = test_world_with("rail-power", rc.clone());
    // A steam plant whose drive gear sits directly above the rail cell, so
    // the cell reads live power once the boiler is lit.
    let (fx, fy, fz) = (10, 120, 10);
    assert!(w.place_block((fx, fy, fz), b(&rc, "base:firebox")));
    w.set_block(fx, fy + 1, fz, b(&rc, "base:boiler"));
    w.set_block(fx + 1, fy + 1, fz, b(&rc, "base:steam_engine"));
    w.set_block(fx + 2, fy + 1, fz, b(&rc, "base:gear"));
    // A second gear above the next rail cell keeps the line live under the
    // car the whole way, so the heavy load keeps rolling on arrival.
    w.set_block(fx + 2, fy + 1, fz + 1, b(&rc, "base:gear"));
    lay(&mut w, &rc, "base:rail", &[(12, MY, 10), (12, MY, 11)]);
    let id = heavy_car(&mut w, &rc);
    onto(&mut w, id, (12, MY, 10), (12, MY, 11), 1.0);

    // Unpowered: the Heavy draw (1.2) can't be met, so the car holds.
    w.tick_entities(1.0);
    let s = w.local_structure(id).unwrap();
    assert_eq!(
        s.rail.as_ref().unwrap().progress,
        0.0,
        "a heavy car on a dead line jams"
    );
    assert_eq!(
        s.rail.as_ref().unwrap().speed,
        1.0,
        "the nominal speed is kept so the load can resume"
    );

    // Light the boiler; the line delivers enough to roll the load.
    if let Some(crate::world::BlockEntity::Steam(s)) = w.block_entity_mut(&(fx, fy, fz)) {
        s.fuel = 60.0;
    }
    w.set_block(fx, fy + 1, fz - 1, rc.water_block(0));
    w.tick_entities(1.0);
    assert!(
        w.power_at(12, MY, 10) > 0.0,
        "the boiler lit and drives the line"
    );
    let s = w.local_structure(id).unwrap();
    assert!(
        s.rail.as_ref().unwrap().progress > 0.0,
        "steam rolls the heavy load on the powered line"
    );
}

#[test]
fn empty_car_coasts_on_a_dead_line() {
    let rc = base_reg();
    let mut w = test_world_with("rail-coast", rc.clone());
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
    onto(&mut w, id, (0, MY, 0), (0, MY, 1), 1.0);

    w.tick_entities(1.0);
    let s = w.local_structure(id).unwrap();
    assert_eq!(
        s.transform.anchor,
        bp(0, MY, 1),
        "an empty car coasts an unpowered line at full speed"
    );
    assert_eq!(
        s.rail.as_ref().unwrap().next_cell,
        bp(0, MY, 2),
        "and keeps looking ahead"
    );
}
