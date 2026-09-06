//! Workbench graphical containers adapter.

use super::recipe_gates_met;
use crate::audio::Sfx;
use crate::game::Game;
use crate::inventory::ItemStack;
use crate::world;

impl Game {
    /// Craft a workbench recipe from the inventory (capability E7). The
    /// screen lists the machine's `station` recipes; clicking one consumes
    /// one of each ingredient and adds the output, exactly like the free
    /// grid but bound to the machine rather than the player's hands.
    pub(in crate::game) fn workbench_craft(
        &mut self,
        pos: crate::planet::BlockPos,
        recipe_index: usize,
    ) {
        if self.reject_guest_action() {
            return;
        }
        let reg = self.content.reg.clone();
        let Some(world::BlockEntity::Multiblock(m)) = self.runtime.view().block_entity_at(&pos)
        else {
            return;
        };
        let recipes = reg.machine_recipes_for(m.kind);
        let Some(recipe) = recipes.get(recipe_index) else {
            return;
        };
        let tech_value = recipe
            .tech
            .as_deref()
            .and_then(|key| self.read_player_kv(key));
        if let Some(gate) = recipe_gates_met(tech_value.as_deref(), &self.inventory, recipe) {
            let label = if gate == "blueprint" {
                recipe
                    .blueprint
                    .map(|b| format!("requires {}", reg.item(b).label))
                    .unwrap_or_else(|| "requires a blueprint".to_string())
            } else {
                "is locked".to_string()
            };
            self.toast(format!("This recipe {label}."));
            return;
        }
        let mut found: Vec<usize> = Vec::new();
        'ingredients: for cell in recipe.pattern.iter().flatten() {
            for (index, slot) in self.inventory.slots.iter().enumerate() {
                if !found.contains(&index) && slot.is_some_and(|stack| cell.matches(stack.item)) {
                    found.push(index);
                    continue 'ingredients;
                }
            }
            self.toast("You're missing an ingredient.".to_string());
            return;
        }
        for index in found {
            let stack = self.inventory.slots[index].as_mut().expect("just located");
            stack.count -= 1;
            if stack.count == 0 {
                self.inventory.slots[index] = None;
            }
        }
        let output = ItemStack::new(&reg, recipe.output, recipe.count);
        let left = self.inventory.add_stack(&reg, output);
        if left > 0 {
            self.drop_stack(ItemStack {
                count: left,
                ..output
            });
        }
        self.sfx(Sfx::Craft);
        self.grant_xp("craft");
    }
}
