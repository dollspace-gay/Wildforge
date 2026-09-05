//! Shared hunger and nutrient admission, used by authority and prediction.

use crate::registry::FoodDef;

pub(crate) fn wants_food(hunger: f32, nutrition: &[f32; 5], food: &FoodDef) -> bool {
    hunger < 19.5 || food.nutrition.iter().zip(nutrition)
        .any(|(add, value)| *add > 0.0 && *value < 99.0)
}

/// Returns whether the meal was accepted. The caller retains its existing
/// inventory/accounting and creative-mode policy after this state transition.
pub(crate) fn eat(hunger: &mut f32, nutrition: &mut [f32; 5], food: &FoodDef) -> bool {
    if !wants_food(*hunger, nutrition, food) { return false; }
    *hunger = (*hunger + food.hunger).min(20.0);
    for (value, add) in nutrition.iter_mut().zip(&food.nutrition) {
        *value = (*value + add).min(100.0);
    }
    true
}
