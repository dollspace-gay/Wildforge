//! Inventory-screen composition.
//!
//! The inventory is one focused workspace: identity and equipment, crafting,
//! then storage. Detailed survival readouts and recipe discovery are secondary
//! views, opened deliberately instead of competing with the core task.

use super::*;

impl Game {
    fn draw_inventory_tab(
        &self,
        ui: &mut UiBatch,
        rect: (f32, f32, f32, f32),
        label: &str,
        active: bool,
    ) {
        let color = if active {
            [0.28, 0.42, 0.34, 0.98]
        } else if self.hit(rect) {
            [0.28, 0.30, 0.32, 0.98]
        } else {
            [0.16, 0.18, 0.20, 0.96]
        };
        ui.rect(rect.0, rect.1, rect.2, rect.3, color);
        if active {
            ui.rect(
                rect.0,
                rect.1 + rect.3 - 3.0,
                rect.2,
                3.0,
                [0.58, 0.9, 0.65, 1.0],
            );
        }
        let text_width = UiBatch::text_width(1.5, label);
        ui.text_shadow(
            rect.0 + (rect.2 - text_width) * 0.5,
            rect.1 + 8.0,
            1.5,
            label,
            [1.0; 4],
        );
    }

    fn draw_inventory_status(&self, ui: &mut UiBatch, rect: (f32, f32, f32, f32)) {
        ui.rect(rect.0, rect.1, rect.2, rect.3, [0.02, 0.03, 0.04, 0.94]);
        ui.text_shadow(rect.0 + 14.0, rect.1 + 12.0, 1.75, "NUTRITION", [1.0; 4]);

        let names = ["GRAIN", "VEG", "FRUIT", "FUNGI", "PROT"];
        let colors = [
            [0.85, 0.7, 0.25, 1.0],
            [0.35, 0.75, 0.3, 1.0],
            [0.85, 0.3, 0.3, 1.0],
            [0.6, 0.45, 0.3, 1.0],
            [0.8, 0.4, 0.35, 1.0],
        ];
        let bar_x = rect.0 + 90.0;
        let bar_width = 150.0;
        for i in 0..5 {
            let y = rect.1 + 43.0 + i as f32 * 26.0;
            ui.text_shadow(rect.0 + 14.0, y, 1.35, names[i], [0.92, 0.94, 0.96, 1.0]);
            ui.rect(bar_x, y + 1.0, bar_width, 9.0, [0.10, 0.11, 0.12, 0.95]);
            ui.rect(
                bar_x,
                y + 1.0,
                bar_width * (self.survival.nutrition[i] / 100.0),
                9.0,
                colors[i],
            );
        }

        let split_x = rect.0 + rect.2 * 0.52;
        ui.rect(
            split_x,
            rect.1 + 12.0,
            2.0,
            rect.3 - 24.0,
            [0.4, 0.43, 0.46, 0.45],
        );
        let info_x = split_x + 18.0;
        let bonus = (self.max_health() - MAX_HEALTH) as i32 / 2;
        ui.text_shadow(info_x, rect.1 + 17.0, 1.5, "VITALS", [1.0; 4]);
        ui.text_shadow(
            info_x,
            rect.1 + 47.0,
            1.4,
            &format!("MAX HEALTH +{bonus}"),
            [0.9, 0.92, 0.95, 1.0],
        );

        let tier = self.server.world.ire_tier();
        let tier_color = [
            [0.45, 0.75, 0.4, 1.0],
            [0.8, 0.75, 0.35, 1.0],
            [0.9, 0.55, 0.25, 1.0],
            [0.9, 0.3, 0.25, 1.0],
        ][tier];
        ui.text_shadow(
            info_x,
            rect.1 + 78.0,
            1.4,
            "THE WILD",
            [0.9, 0.92, 0.95, 1.0],
        );
        ui.rect(
            info_x + 96.0,
            rect.1 + 80.0,
            112.0,
            9.0,
            [0.10, 0.11, 0.12, 0.95],
        );
        ui.rect(
            info_x + 96.0,
            rect.1 + 80.0,
            self.server.world.ire * 1.12,
            9.0,
            tier_color,
        );
        ui.text_shadow(
            info_x,
            rect.1 + 105.0,
            1.4,
            world::IRE_TIERS[tier],
            tier_color,
        );

        let world = &self.server.world;
        let third = ["EARLY", "MID", "LATE"][((world.season_progress() * 3.0) as usize).min(2)];
        ui.text_shadow(
            info_x,
            rect.1 + 139.0,
            1.4,
            &format!(
                "DAY {} - {third} {}",
                world.day + 1,
                world::SEASONS[world.season()]
            ),
            [0.78, 0.86, 1.0, 1.0],
        );
    }

    fn draw_inventory_gear(&self, ui: &mut UiBatch) {
        let panel = self.inventory_panel_rect();
        let avatar = self.inventory_avatar_rect();
        let first = self.craft_slot_rect(0);
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
            Self::draw_slot(
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
            let slot = self.craft_slot_rect(i);
            Self::draw_slot(
                &self.content.reg,
                ui,
                slot,
                self.interaction.craft_grid[i],
                false,
                self.hit(slot),
            );
        }
        let result_slot = self.result_slot_rect();
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
        let result = crafting::match_recipe(
            &self.content.reg,
            &self.interaction.craft_grid[..count],
            self.interaction.craft_size,
        )
        .map(|recipe| ItemStack::new(&self.content.reg, recipe.output, recipe.count));
        Self::draw_slot(
            &self.content.reg,
            ui,
            result_slot,
            result,
            false,
            self.hit(result_slot),
        );
    }

    pub(super) fn draw_inventory_screen(&self, ui: &mut UiBatch) {
        let width = self.renderer.config.width as f32;
        let height = self.renderer.config.height as f32;
        ui.rect(0.0, 0.0, width, height, [0.0, 0.0, 0.0, 0.48]);

        let panel = self.inventory_panel_rect();
        ui.rect(
            panel.0,
            panel.1,
            panel.2,
            panel.3,
            [0.055, 0.065, 0.075, 0.38],
        );
        ui.rect(panel.0, panel.1, panel.2, 42.0, [0.06, 0.07, 0.08, 0.94]);
        ui.rect(panel.0, panel.1, panel.2, 2.0, [0.55, 0.58, 0.60, 0.78]);
        ui.rect(
            panel.0,
            panel.1 + panel.3 - 2.0,
            panel.2,
            2.0,
            [0.18, 0.20, 0.22, 0.9],
        );
        ui.text_shadow(panel.0 + 14.0, panel.1 + 13.0, 1.8, "INVENTORY", [1.0; 4]);

        let gear_tab = self.inventory_tab_rect(0);
        let status_tab = self.inventory_tab_rect(1);
        let recipe_tab = self.inventory_tab_rect(2);
        self.draw_inventory_tab(ui, gear_tab, "GEAR", !self.ui_state.inventory_status_open);
        self.draw_inventory_tab(
            ui,
            status_tab,
            "STATUS",
            self.ui_state.inventory_status_open,
        );
        self.draw_inventory_tab(
            ui,
            recipe_tab,
            "RECIPES",
            self.ui_state.inventory_browser_open,
        );

        if self.ui_state.inventory_status_open {
            self.draw_inventory_status(ui, (panel.0 + 16.0, panel.1 + 48.0, panel.2 - 32.0, 184.0));
        } else {
            self.draw_inventory_gear(ui);
        }

        let (_, grid_y, _, _) = self.inv_slot_rect(HOTBAR_SLOTS);
        ui.rect(
            panel.0 + 8.0,
            grid_y - 8.0,
            panel.2 - 16.0,
            panel.1 + panel.3 - grid_y,
            [0.02, 0.025, 0.03, 0.94],
        );
        for i in 0..TOTAL_SLOTS {
            let slot = self.inv_slot_rect(i);
            Self::draw_slot(
                &self.content.reg,
                ui,
                slot,
                self.inventory.slots[i],
                i == self.input.hotbar_sel,
                self.hit(slot),
            );
        }

        if self.ui_state.inventory_browser_open {
            self.draw_browser(ui);
        }

        if let Some(stack) = self.ui_state.held_stack {
            let (cursor_x, cursor_y) = self.input.ui_cursor;
            let icon = self.content.reg.item(stack.item).icon;
            ui.tile(cursor_x - 16.0, cursor_y - 16.0, 32.0, 32.0, icon, [1.0; 4]);
            if stack.count > 1 {
                ui.text_shadow(
                    cursor_x + 6.0,
                    cursor_y + 4.0,
                    2.0,
                    &format!("{}", stack.count),
                    [1.0; 4],
                );
            }
        }
    }
}
