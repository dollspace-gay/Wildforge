//! Inventory graphical containers adapter.

use crate::audio::Sfx;
use crate::crafting;
use crate::inventory;
use crate::inventory::ItemStack;
use crate::net;
use crate::game::Game;
use crate::registry::RecipeDef;
use super::recipe_gates_met;

impl Game {
    pub(in crate::game) fn slot_get(&self, craft: bool, i: usize) -> Option<ItemStack> {
        if craft {
            self.interaction.craft_grid[i]
        } else {
            self.inventory.slots[i]
        }
    }

    pub(in crate::game) fn slot_set(&mut self, craft: bool, i: usize, v: Option<ItemStack>) {
        if craft {
            self.interaction.craft_grid[i] = v;
        } else {
            self.inventory.slots[i] = v;
        }
    }

    pub(in crate::game) fn inventory_click(&mut self, craft: bool, slot: usize, right: bool) {
        if let Some(remote) = &self.multiplayer.remote {
            remote.session.send(&net::C2S::InventoryClick {
                area: if craft {
                    net::InventoryArea::Craft
                } else {
                    net::InventoryArea::Inventory
                },
                slot: slot as u8,
                right,
            });
        }
        let cur = self.slot_get(craft, slot);
        let (new_slot, new_held) =
            inventory::click_stack(&self.content.reg, cur, self.ui_state.held_stack, right);
        self.slot_set(craft, slot, new_slot);
        self.ui_state.held_stack = new_held;
    }

    /// Click the craft result slot: take the output, consume ingredients.
    pub(in crate::game) fn result_click(&mut self) {
        let reg = self.content.reg.clone();
        let n2 = self.interaction.craft_size * self.interaction.craft_size;
        let recipe = crafting::match_recipe(
            &reg,
            &self.interaction.craft_grid[..n2],
            self.interaction.craft_size,
        );
        // Spec 3.5 gate: a recipe locked by its tech flag or missing blueprint
        // item is refused before the multiplayer request leaves the client (the
        // KV lives client-side; the host re-enforces the blueprint gate).
        if let Some(r) = recipe
            && self.recipe_locked(r)
        {
            return;
        }
        if let Some(remote) = &self.multiplayer.remote {
            remote.session.send(&net::C2S::CraftResult {
                size: self.interaction.craft_size as u8,
            });
        }
        let Ok(effects) = crate::player_ops::craft::take_result(
            &reg,
            self.interaction.craft_size,
            &mut self.interaction.craft_grid,
            &mut self.inventory,
            &mut self.ui_state.held_stack,
        ) else {
            return;
        };
        let kind = self.runtime.finish_craft(effects, self.player.pos.block(), &mut self.inventory);
        self.sfx(Sfx::Craft);
        if let crate::player_ops::craft::CraftKind::Recipe(output) = kind {
            self.grant_xp("craft");
            if self.content.scripts.wants("on_craft") {
                let name = reg.item(output).name.clone();
                self.content.scripts.dispatch_view(&self.runtime.view(), "on_craft", (name,));
                self.apply_script_cmds();
            }
        }
    }

    /// Spec 3.5 gate: a recipe is locked when its tech KV key reads falsy or
    /// when the player lacks the required blueprint item.
    pub(in crate::game) fn recipe_locked(&self, recipe: &RecipeDef) -> bool {
        let tech_value = recipe
            .tech
            .as_deref()
            .and_then(|key| self.read_player_kv(key));
        recipe_gates_met(tech_value.as_deref(), &self.inventory, recipe).is_some()
    }
}
