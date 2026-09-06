//! Gear graphical inventory ui adapter.

use crate::crafting;
use crate::game::Game;
use crate::game::widgets;
use crate::inventory::ItemStack;
use crate::ui::UiBatch;

impl Game {
    pub(super) fn draw_inventory_gear(&self, ui: &mut UiBatch) {
        let panel = self.inventory_layout().panel_rect();
        let avatar = self.inventory_layout().avatar_rect();
        let first = self.inventory_layout().craft_slot_rect(0);
        ui.rect(
            panel.0 + 8.0,
            panel.1 + 42.0,
            58.0,
            198.0,
            [0.02, 0.025, 0.03, 0.94],
        );
        ui.rect(
            first.0 - 16.0,
            panel.1 + 48.0,
            panel.0 + panel.2 - first.0,
            184.0,
            [0.02, 0.025, 0.03, 0.94],
        );
        ui.rect(
            avatar.0,
            avatar.1,
            avatar.2,
            avatar.3,
            [0.01, 0.02, 0.03, 0.55],
        );
        ui.rect(avatar.0, avatar.1, avatar.2, 2.0, [0.48, 0.52, 0.55, 0.72]);
        ui.rect(
            avatar.0,
            avatar.1 + avatar.3 - 2.0,
            avatar.2,
            2.0,
            [0.48, 0.52, 0.55, 0.72],
        );
        ui.rect(avatar.0, avatar.1, 2.0, avatar.3, [0.48, 0.52, 0.55, 0.72]);
        ui.rect(
            avatar.0 + avatar.2 - 2.0,
            avatar.1,
            2.0,
            avatar.3,
            [0.48, 0.52, 0.55, 0.72],
        );

        let (active_name, social_name) = self.selected_multiplayer_name();
        let mut active_name = active_name.to_uppercase();
        if active_name.chars().count() > 20 {
            active_name = format!("{}...", active_name.chars().take(17).collect::<String>());
        }
        let name_width = UiBatch::text_width(1.55, &active_name);
        ui.rect(
            avatar.0 + 3.0,
            avatar.1 + 3.0,
            avatar.2 - 6.0,
            if social_name { 34.0 } else { 22.0 },
            [0.01, 0.02, 0.03, 0.76],
        );
        ui.text_shadow(
            avatar.0 + (avatar.2 - name_width) * 0.5,
            avatar.1 + 7.0,
            1.55,
            &active_name,
            [0.72, 1.0, 0.78, 1.0],
        );
        if social_name
            && let Some(handle) = self
                .atproto_account
                .as_ref()
                .and_then(|account| account.handle.as_deref())
        {
            let mut handle = format!("@{handle}");
            if handle.chars().count() > 28 {
                handle = format!("{}...", handle.chars().take(25).collect::<String>());
            }
            let handle_width = UiBatch::text_width(1.05, &handle);
            ui.text_shadow(
                avatar.0 + (avatar.2 - handle_width) * 0.5,
                avatar.1 + 23.0,
                1.05,
                &handle,
                [0.65, 0.78, 1.0, 1.0],
            );
        }

        for (i, label) in ["H", "C", "L", "B", "*"].iter().enumerate() {
            let slot = self.armor_slot_rect(i);
            if i == 4 {
                ui.text_shadow(
                    slot.0 - 1.0,
                    slot.1 - 15.0,
                    1.0,
                    "CHARM",
                    [0.72, 0.75, 0.78, 1.0],
                );
            }
            widgets::slot(
                &self.content.reg,
                ui,
                slot,
                self.survival.armor[i],
                false,
                self.hit(slot),
            );
            if self.survival.armor[i].is_none() {
                ui.text_shadow(
                    slot.0 + slot.2 * 0.5 - 5.0,
                    slot.1 + slot.3 * 0.5 - 7.0,
                    2.0,
                    label,
                    [0.55, 0.55, 0.55, 0.8],
                );
            }
        }

        let count = self.interaction.craft_size * self.interaction.craft_size;
        ui.text_shadow(
            first.0,
            first.1 - 22.0,
            1.35,
            "CRAFT",
            [0.72, 0.75, 0.78, 1.0],
        );
        for i in 0..count {
            let slot = self.inventory_layout().craft_slot_rect(i);
            widgets::slot(
                &self.content.reg,
                ui,
                slot,
                self.interaction.craft_grid[i],
                false,
                self.hit(slot),
            );
        }
        let result_slot = self.inventory_layout().result_slot_rect();
        ui.text_shadow(
            result_slot.0 - 34.0,
            result_slot.1 + 16.0,
            2.5,
            "-",
            [1.0; 4],
        );
        ui.text_shadow(
            result_slot.0 - 24.0,
            result_slot.1 + 14.0,
            2.5,
            ">",
            [1.0; 4],
        );
        let result =
            crafting::match_repair(&self.content.reg, &self.interaction.craft_grid[..count])
                .map(|repair| repair.output)
                .or_else(|| {
                    crafting::match_recipe(
                        &self.content.reg,
                        &self.interaction.craft_grid[..count],
                        self.interaction.craft_size,
                    )
                    .map(|recipe| ItemStack::new(&self.content.reg, recipe.output, recipe.count))
                });
        widgets::slot(
            &self.content.reg,
            ui,
            result_slot,
            result,
            false,
            self.hit(result_slot),
        );
        // Spec 3.5: a locked recipe's preview shows a dimmed lock badge.
        let locked = crafting::match_recipe(
            &self.content.reg,
            &self.interaction.craft_grid[..count],
            self.interaction.craft_size,
        )
        .is_some_and(|r| self.recipe_locked(r));
        if locked {
            ui.rect(
                result_slot.0,
                result_slot.1,
                result_slot.2,
                result_slot.3,
                [0.0, 0.0, 0.0, 0.55],
            );
            ui.text_shadow(
                result_slot.0 + result_slot.2 / 2.0 - 24.0,
                result_slot.1 + result_slot.3 / 2.0 - 8.0,
                1.5,
                "LOCKED",
                [1.0, 0.8, 0.3, 1.0],
            );
        }
    }
}
