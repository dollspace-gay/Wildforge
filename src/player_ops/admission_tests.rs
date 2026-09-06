//! Shared player rules retain their boundary behavior after adapter extraction.

use super::{equipment, feeding::FeedPlan, nutrition};
use crate::{inventory::ItemStack, registry};

#[test]
fn nutrition_thresholds_reject_a_satisfied_meal_and_cap_an_accepted_one() {
    let food = registry::FoodDef {
        hunger: 8.0,
        eat_time: 1.0,
        nutrition: [5.0, 0.0, 0.0, 0.0, 0.0],
    };
    let mut hunger = 19.5;
    let mut nutrients = [99.0; 5];
    assert!(!nutrition::eat(&mut hunger, &mut nutrients, &food));
    assert_eq!((hunger, nutrients), (19.5, [99.0; 5]));
    nutrients[0] = 98.5;
    assert!(nutrition::eat(&mut hunger, &mut nutrients, &food));
    assert_eq!(hunger, 20.0);
    assert_eq!(nutrients, [100.0, 99.0, 99.0, 99.0, 99.0]);
}

#[test]
fn an_ineligible_equipment_slot_never_spends_or_swaps_the_cursor() {
    let reg = registry::load(std::path::Path::new("/nonexistent-mods-dir"));
    let stack = ItemStack {
        arcane_id: 17,
        ..ItemStack::new(&reg, reg.item_id("base:stick").unwrap(), 1)
    };
    let mut armor = [None; 5];
    let mut cursor = Some(stack);
    for index in [0, 4, 5, usize::MAX] {
        equipment::exchange(&reg, &mut armor, &mut cursor, index);
        assert_eq!(cursor, Some(stack));
        assert_eq!(armor, [None; 5]);
    }
}

#[test]
fn feeding_refuses_immature_nonfinite_and_already_satisfied_animals() {
    let reg = registry::load(std::path::Path::new("/nonexistent-mods-dir"));
    let (species, definition) = reg
        .animals
        .iter()
        .enumerate()
        .find(|(_, animal)| !animal.hostile && animal.breed_food.is_some())
        .unwrap();
    let pos = crate::planet::EntityPos::new(crate::planet::Face::PosZ, 8.0, 90.0, 8.0).unwrap();
    let mut mob = crate::mobs::Mob::new_at(species, pos, 0.0);
    for growth in [0.5, f32::NAN] {
        mob.growth = growth;
        assert!(FeedPlan::prepare(definition, &mob, definition.breed_food).is_none());
    }
    mob.growth = 1.0;
    mob.tamed = true;
    mob.fed = true;
    assert!(FeedPlan::prepare(definition, &mob, definition.breed_food).is_none());
    mob.fed = false;
    mob.breed_cd = 0.0;
    let plan = FeedPlan::prepare(definition, &mob, definition.breed_food).unwrap();
    plan.apply(&mut mob);
    assert!(mob.fed);
    assert_eq!(mob.calm, 30.0);
}
