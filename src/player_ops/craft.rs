//! Shared crafting/repair transaction and its ordered conservation effects.

use crate::crafting;
use crate::inventory::{Inventory, ItemStack};
use crate::planet::BlockPos;
use crate::registry::{ItemId, MaterialVector, Registry};
use crate::world::World;

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub(crate) enum CraftError {
    #[error("invalid crafting grid")]
    InvalidGrid,
    #[error("no matching recipe")]
    NoRecipe,
    #[error("required blueprint is missing")]
    MissingBlueprint,
    #[error("cursor cannot hold the result")]
    CursorFull,
}

#[derive(Clone, Copy)]
pub(crate) enum CraftKind {
    Repair,
    Recipe(ItemId),
}

#[must_use = "complete crafting conservation effects before presenting the result"]
pub(crate) struct CraftEffects {
    kind: CraftKind,
    retired: Vec<ItemStack>,
    loss: MaterialVector,
    byproducts: Vec<(ItemId, u32)>,
}

/// Tech unlock policy stays with its existing player-KV adapter. Blueprint,
/// cursor capacity, ingredient consumption, and repair rules are shared.
pub(crate) fn take_result(
    registry: &Registry,
    size: usize,
    grid: &mut [Option<ItemStack>],
    inventory: &mut Inventory,
    cursor: &mut Option<ItemStack>,
) -> Result<CraftEffects, CraftError> {
    if !(2..=3).contains(&size) || grid.len() < size * size {
        return Err(CraftError::InvalidGrid);
    }
    let grid = &mut grid[..size * size];
    if let Some(repair) = crafting::match_repair(registry, grid) {
        if cursor.is_some() {
            return Err(CraftError::CursorFull);
        }
        let retired = grid[repair.part_slot].into_iter()
            .filter(|stack| stack.arcane_id != 0)
            .map(|stack| ItemStack { count: 1, ..stack })
            .collect();
        *cursor = Some(repair.output);
        crafting::consume_repair(grid, &repair);
        return Ok(CraftEffects {
            kind: CraftKind::Repair,
            retired,
            loss: repair.scale_loss,
            byproducts: Vec::new(),
        });
    }
    let recipe = crafting::match_recipe(registry, grid, size).ok_or(CraftError::NoRecipe)?;
    if let Some(blueprint) = recipe.blueprint
        && !inventory.can_afford(&[(blueprint, 1)])
    {
        return Err(CraftError::MissingBlueprint);
    }
    let output = ItemStack::new(registry, recipe.output, recipe.count);
    let next_cursor = match *cursor {
        None => Some(output),
        Some(held) if held.can_merge(registry, &output)
            && held.count + output.count <= registry.item(held.item).max_stack =>
        {
            Some(ItemStack { count: held.count + output.count, ..held })
        }
        _ => return Err(CraftError::CursorFull),
    };
    let effects = CraftEffects {
        kind: CraftKind::Recipe(recipe.output),
        retired: grid.iter().flatten().filter(|stack| stack.arcane_id != 0)
            .map(|stack| ItemStack { count: 1, ..*stack }).collect(),
        loss: recipe.loss.clone(),
        byproducts: recipe.byproducts.clone(),
    };
    *cursor = next_cursor;
    crafting::consume(grid);
    if let Some(blueprint) = recipe.blueprint {
        inventory.try_consume(&[(blueprint, 1)]);
    }
    Ok(effects)
}

impl CraftEffects {
    /// Retire charged inputs, record scale/loss, then publish secondary outputs
    /// and bury overflow, preserving the pre-extraction side-effect order.
    pub(crate) fn finish(
        mut self,
        world: &mut World,
        position: Option<BlockPos>,
        inventory: &mut Inventory,
        authoritative: bool,
        overflow_reason: &str,
    ) -> CraftKind {
        let reason = match self.kind {
            CraftKind::Repair => "charged repair part consumed",
            CraftKind::Recipe(_) => "charged crafting ingredient consumed",
        };
        if authoritative && let Some(position) = position {
            for stack in std::mem::take(&mut self.retired) {
                world.retire_arcane_stack_at(position, stack, reason);
            }
        }
        self.finish_outputs(&world.reg, world.material_ledger.as_mut(), position, inventory, overflow_reason)
    }

    /// The guest mirrors inventory byproducts without material or Current books.
    pub(crate) fn finish_prediction(self, registry: &Registry, inventory: &mut Inventory) -> CraftKind {
        self.finish_outputs(registry, None, None, inventory, "")
    }

    fn finish_outputs(
        self, registry: &Registry, mut ledger: Option<&mut crate::materials::MaterialLedger>,
        position: Option<BlockPos>, inventory: &mut Inventory, overflow_reason: &str,
    ) -> CraftKind {
        if let Some(ledger) = ledger.as_deref_mut()
            && let Err(error) = ledger.record_recipe_loss(&self.loss)
        {
            eprintln!("materials: crafting loss accounting failed: {error}");
        }
        for (item, count) in self.byproducts {
            if crate::materials::is_secondary_item(registry, item)
                && let Some(ledger) = ledger.as_deref_mut()
            {
                let materials = crate::materials::stack_materials(
                    registry, ItemStack::new(registry, item, count),
                );
                if let Err(error) = ledger.record_secondary_output(&materials) {
                    eprintln!("materials: crafting secondary output failed: {error}");
                }
            }
            let remainder = inventory.add(registry, item, count);
            if remainder != 0
                && let (Some(position), Some(ledger)) = (position, ledger.as_deref_mut())
                && let Err(error) = ledger.bury_stack(
                    registry, position, ItemStack::new(registry, item, remainder), overflow_reason,
                )
            {
                eprintln!("materials: crafting byproduct salvage failed: {error}");
            }
        }
        self.kind
    }
}
