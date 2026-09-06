//! Inventory-screen composition.
//!
//! The inventory is one focused workspace: identity and equipment, crafting,
//! then storage. Detailed survival readouts and recipe discovery are secondary
//! views, opened deliberately instead of competing with the core task.

use crate::game::widgets;
use crate::inventory::HOTBAR_SLOTS;
use crate::ui::UiBatch;
use crate::game::Game;

impl Game {
    pub(super) const DISCOVERY_ROWS: usize = 7;

    pub(super) fn draw_inventory_screen(&self, ui: &mut UiBatch) {
        let width = self.renderer.config.width as f32;
        let height = self.renderer.config.height as f32;
        ui.rect(0.0, 0.0, width, height, [0.0, 0.0, 0.0, 0.48]);

        let panel = self.inventory_layout().panel_rect();
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

        let gear_tab = self.inventory_layout().tab_rect(0);
        let status_tab = self.inventory_layout().tab_rect(1);
        let recipe_tab = self.inventory_layout().tab_rect(2);
        let discovery_tab = self.inventory_layout().tab_rect(3);
        self.draw_inventory_tab(
            ui,
            gear_tab,
            "GEAR",
            !self.ui_state.inventory_status_open
                && !self.ui_state.inventory_browser_open
                && !self.ui_state.inventory_discovery_open,
        );
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
        self.draw_inventory_tab(
            ui,
            discovery_tab,
            "RECORDS",
            self.ui_state.inventory_discovery_open,
        );

        if self.ui_state.inventory_discovery_open {
            self.draw_discovery_catalogue(ui);
            return;
        }

        if self.ui_state.inventory_status_open {
            self.draw_inventory_status(ui, (panel.0 + 16.0, panel.1 + 48.0, panel.2 - 32.0, 184.0));
        } else {
            self.draw_inventory_gear(ui);
        }

        let (_, grid_y, _, _) = self.inventory_layout().slot_rect(HOTBAR_SLOTS);
        ui.rect(
            panel.0 + 8.0,
            grid_y - 8.0,
            panel.2 - 16.0,
            panel.1 + panel.3 - grid_y,
            [0.02, 0.025, 0.03, 0.94],
        );
        self.draw_player_inventory(ui);

        if self.ui_state.inventory_browser_open {
            self.draw_browser(ui);
        }

        widgets::held_stack(&self.content.reg, ui, self.input.ui_cursor, self.ui_state.held_stack);
    }
}

mod discovery;
mod status;
mod gear;
