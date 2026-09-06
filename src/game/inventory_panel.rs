//! Shared inventory geometry and read-only storage-grid painting.

use super::Game;
use super::widgets::{self, Rect};
use crate::inventory::{HOTBAR_SLOTS, ItemStack, TOTAL_SLOTS};
use crate::registry::Registry;
use crate::ui::UiBatch;

pub(super) const SLOT: f32 = 46.0;

#[derive(Clone, Copy)]
pub(super) struct InventoryLayout {
    width: f32,
    height: f32,
    craft_size: usize,
}

impl InventoryLayout {
    fn new(width: u32, height: u32, craft_size: usize) -> Self {
        Self {
            width: width as f32,
            height: height as f32,
            craft_size,
        }
    }

    pub(super) fn hotbar_origin(&self) -> (f32, f32) {
        let w = self.width;
        let h = self.height;
        ((w - 9.0 * SLOT) / 2.0, h - SLOT - 8.0)
    }

    pub(super) fn hotbar_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let (x0, y0) = self.hotbar_origin();
        (x0 + i as f32 * SLOT, y0, SLOT, SLOT)
    }

    /// Slot rects for the inventory screen: 0..9 hotbar row, 9..36 storage grid.
    pub(super) fn slot_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.width;
        let h = self.height;
        let panel_w = 9.0 * SLOT;
        let x0 = (w - panel_w) / 2.0;
        // The complete inventory card (equipment above, storage below) is
        // vertically centered. The old offset centered only the storage rows,
        // leaving the paper doll stranded near the corner of the screen.
        let grid_y = h / 2.0 + 16.0;
        if i < HOTBAR_SLOTS {
            (x0 + i as f32 * SLOT, grid_y + 3.0 * SLOT + 14.0, SLOT, SLOT)
        } else {
            let j = i - HOTBAR_SLOTS;
            (
                x0 + (j % 9) as f32 * SLOT,
                grid_y + (j / 9) as f32 * SLOT,
                SLOT,
                SLOT,
            )
        }
    }

    /// Unified inventory card containing identity, gear, crafting and storage.
    pub(super) fn panel_rect(&self) -> (f32, f32, f32, f32) {
        let (grid_x, grid_y, _, _) = self.slot_rect(HOTBAR_SLOTS);
        (grid_x - 134.0, grid_y - 248.0, 9.0 * SLOT + 268.0, 464.0)
    }

    pub(super) fn avatar_rect(&self) -> (f32, f32, f32, f32) {
        let (panel_x, panel_y, _, _) = self.panel_rect();
        (panel_x + 70.0, panel_y + 48.0, 170.0, 184.0)
    }

    /// Header controls use one source of geometry for drawing and hit-testing.
    pub(super) fn tab_rect(&self, tab: usize) -> (f32, f32, f32, f32) {
        let (x, y, width, _) = self.panel_rect();
        let button_width = 94.0;
        let right_pad = 8.0 + (3usize.saturating_sub(tab)) as f32 * 98.0;
        (
            x + width - right_pad - button_width,
            y + 9.0,
            button_width,
            28.0,
        )
    }

    /// Craft grid layout: grid slots then the result slot to their right.
    pub(super) fn craft_slot_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let n = self.craft_size;
        let (sx, sy, _, _) = self.slot_rect(HOTBAR_SLOTS); // storage top-left
        let y0 = sy - (n as f32) * SLOT - 26.0;
        let x0 = sx + 4.25 * SLOT;
        (
            x0 + (i % n) as f32 * SLOT,
            y0 + (i / n) as f32 * SLOT,
            SLOT,
            SLOT,
        )
    }

    pub(super) fn result_slot_rect(&self) -> (f32, f32, f32, f32) {
        let n = self.craft_size;
        let (gx, gy, _, _) = self.craft_slot_rect(0);
        (
            gx + n as f32 * SLOT + SLOT,
            gy + ((n as f32) - 1.0) * SLOT / 2.0,
            SLOT,
            SLOT,
        )
    }
}

/// A borrowed presentation snapshot. Drawing cannot change the inventory.
struct PlayerInventory<'a> {
    layout: InventoryLayout,
    slots: &'a [Option<ItemStack>; TOTAL_SLOTS],
    selected: usize,
    cursor: (f32, f32),
    registry: &'a Registry,
}

impl PlayerInventory<'_> {
    fn draw(&self, ui: &mut UiBatch) {
        for i in 0..TOTAL_SLOTS {
            let r: Rect = self.layout.slot_rect(i);
            widgets::slot(
                self.registry,
                ui,
                r,
                self.slots[i],
                i == self.selected,
                widgets::hit(self.cursor, r),
            );
        }
    }
}

impl Game {
    pub(super) fn inventory_layout(&self) -> InventoryLayout {
        InventoryLayout::new(
            self.renderer.config.width,
            self.renderer.config.height,
            self.interaction.craft_size,
        )
    }

    pub(super) fn draw_player_inventory(&self, ui: &mut UiBatch) {
        PlayerInventory {
            layout: self.inventory_layout(),
            slots: &self.inventory.slots,
            selected: self.input.hotbar_sel,
            cursor: self.input.ui_cursor,
            registry: &self.content.reg,
        }
        .draw(ui);
    }
}
