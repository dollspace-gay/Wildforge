//! Layout graphical containers adapter.

use crate::game::Game;

impl Game {
    /// Furnace slot rects: 0 input, 1 fuel, 2 output (centered panel).
    /// Bloomery slots: 0-3 charge (top row), 4-7 fuel (bottom row).
    pub(in crate::game) fn bloomery_slot_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        let (col, row) = (i % 4, i / 4);
        (
            w / 2.0 - 2.0 * (Self::SLOT + 10.0) + col as f32 * (Self::SLOT + 10.0) + 5.0,
            h / 2.0 - 250.0 + row as f32 * (Self::SLOT + 34.0),
            Self::SLOT,
            Self::SLOT,
        )
    }

    /// Mob pack slots: 12 in two rows of six.
    pub(in crate::game) fn mob_cargo_slot_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        let (col, row) = (i % 6, i / 6);
        (
            w / 2.0 - 3.0 * (Self::SLOT + 10.0) + col as f32 * (Self::SLOT + 10.0) + 5.0,
            h / 2.0 - 250.0 + row as f32 * (Self::SLOT + 10.0),
            Self::SLOT,
            Self::SLOT,
        )
    }

    /// Stall layout: goods 0-5 (top row), price 6 (left mid), till
    /// 7-12 (bottom row); the BUY button sits mid-right.
    pub(in crate::game) fn stall_slot_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        let (col, row) = match i {
            0..=5 => (i, 0),
            6 => (0, 1),
            _ => (i - 7, 2),
        };
        (
            w / 2.0 - 3.0 * (Self::SLOT + 10.0) + col as f32 * (Self::SLOT + 10.0) + 5.0,
            h / 2.0 - 260.0 + row as f32 * (Self::SLOT + 34.0),
            Self::SLOT,
            Self::SLOT,
        )
    }

    pub(in crate::game) fn stall_buy_rect(&self) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (
            w / 2.0 + (Self::SLOT + 10.0) + 20.0,
            h / 2.0 - 260.0 + (Self::SLOT + 34.0),
            110.0,
            36.0,
        )
    }

    pub(in crate::game) fn bloomery_light_rect(&self) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (
            w / 2.0 + 2.0 * (Self::SLOT + 10.0) + 20.0,
            h / 2.0 - 230.0,
            110.0,
            36.0,
        )
    }

    /// Kiln slots: 0-3 sand (top), 4 powder (middle), 5-8 fuel (bottom).
    pub(in crate::game) fn kiln_slot_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        let (col, row) = if i == 4 {
            (1.5, 1.0)
        } else if i < 4 {
            (i as f32, 0.0)
        } else {
            ((i - 5) as f32, 2.0)
        };
        (
            w / 2.0 - 2.0 * (Self::SLOT + 10.0) + col * (Self::SLOT + 10.0) + 5.0,
            h / 2.0 - 270.0 + row * (Self::SLOT + 26.0),
            Self::SLOT,
            Self::SLOT,
        )
    }

    pub(in crate::game) fn furnace_slot_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        let (cx, cy) = (w / 2.0, h / 2.0 - 190.0);
        match i {
            0 => (cx - 120.0, cy - 46.0, Self::SLOT, Self::SLOT),
            1 => (cx - 120.0, cy + 34.0, Self::SLOT, Self::SLOT),
            _ => (cx + 50.0, cy - 6.0, Self::SLOT, Self::SLOT),
        }
    }

    /// Chest slot rects: 9x3 grid centered above the inventory panel.
    pub(in crate::game) fn chest_slot_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        let x0 = w / 2.0 - 4.5 * Self::SLOT;
        let y0 = h / 2.0 - 300.0;
        (
            x0 + (i % 9) as f32 * Self::SLOT,
            y0 + (i / 9) as f32 * Self::SLOT,
            Self::SLOT,
            Self::SLOT,
        )
    }

    /// Armor column beside the paper doll — head, chest, legs, feet, then charm.
    pub(in crate::game) fn armor_slot_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let (panel_x, panel_y, _, _) = self.inventory_layout().panel_rect();
        if i == 4 {
            let (avatar_x, avatar_y, avatar_w, avatar_h) = self.inventory_layout().avatar_rect();
            (
                avatar_x + avatar_w + 8.0,
                avatar_y + avatar_h - Self::SLOT,
                Self::SLOT,
                Self::SLOT,
            )
        } else {
            (
                panel_x + 16.0,
                panel_y + 48.0 + i as f32 * Self::SLOT,
                Self::SLOT,
                Self::SLOT,
            )
        }
    }

    /// Offering stone: three slots, centered.
    pub(in crate::game) fn offering_slot_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (
            w / 2.0 + (i as f32 - 1.5) * (Self::SLOT + 10.0) + 5.0,
            h / 2.0 - 200.0,
            Self::SLOT,
            Self::SLOT,
        )
    }

    /// One recipe row of the workbench list (capability E7). The list
    /// starts under the title and drops one row per recipe.
    pub(in crate::game) fn workbench_recipe_rect(&self, index: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (
            w / 2.0 - 330.0,
            h / 2.0 - 250.0 + index as f32 * 62.0,
            660.0,
            56.0,
        )
    }

    /// One widget row on a mod screen (capability E11).
    pub(in crate::game) fn mod_screen_row_rect(&self, index: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (
            w / 2.0 - 330.0,
            h / 2.0 - 240.0 + index as f32 * 56.0,
            660.0,
            46.0,
        )
    }
}
