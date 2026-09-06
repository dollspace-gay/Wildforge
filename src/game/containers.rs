//! Inventory, crafting, armor, and machine-container interactions.

use crate::inventory;
use crate::world;
use crate::game::Game;
use crate::registry::RecipeDef;

#[derive(Clone, Copy)]
enum ContainerPanel { Chest, Offering, Furnace, Bloomery, Kiln }

impl ContainerPanel {
    fn accepts(self, entity: &world::BlockEntity, registry: &crate::registry::Registry) -> bool {
        match (self, entity) {
            (Self::Chest, world::BlockEntity::Chest(_))
            | (Self::Offering, world::BlockEntity::Offering(_))
            | (Self::Furnace, world::BlockEntity::Furnace(_)) => true,
            (Self::Bloomery, world::BlockEntity::Multiblock(machine)) => matches!(machine.kind.handler(registry),
                Some(crate::machines::MachineHandler::Bloomery | crate::machines::MachineHandler::Forge)),
            (Self::Kiln, world::BlockEntity::Multiblock(_)) => true,
            _ => false,
        }
    }
}

impl Game {

    pub(super) const BCOLS: usize = 6;
    pub(super) const BROWS: usize = 8;
    pub(super) const BSLOT: f32 = 40.0;
}

/// Spec 3.5 gate check as a pure seam: `tech_value` is the per-player KV
/// value for `recipe.tech` (None = key absent). Returns `Some` with the
/// unmet gate's label when the recipe is locked. Standalone so both craft
/// sites and the tests share one implementation.
pub(crate) fn recipe_gates_met(
    tech_value: Option<&str>,
    inventory: &crate::inventory::Inventory,
    recipe: &RecipeDef,
) -> Option<&'static str> {
    if let Some(_tech) = &recipe.tech {
        match tech_value {
            Some(value) if !value.is_empty() && value != "false" && value != "0" => {}
            _ => return Some("locked"),
        }
    }
    if let Some(blueprint) = recipe.blueprint
        && !inventory.can_afford(&[(blueprint, 1)])
    {
        return Some("blueprint");
    }
    None
}

mod inventory;
mod layout;
mod cargo;
mod stall;
mod smelting;
mod equipment;
mod exchange;
mod script_screen;
mod workbench;
