//! Script panel layout and UI composition.

use crate::game::Game;
use crate::game::widgets;
use crate::ui::UiBatch;

impl Game {
    pub(in crate::game) fn draw_mod_screen(
        &mut self,
        ui: &mut UiBatch,
        w: f32,
        h: f32,
        idx: usize,
    ) {
        // Capability E11: a data-driven mod screen. Rows render
        // from the def; clicks route through `mod_screen_click`.
        ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.55]);
        let reg = &self.content.reg;
        let title = reg
            .screens
            .get(idx)
            .map(|def| def.title.clone())
            .unwrap_or_else(|| "SCREEN".to_string())
            .to_uppercase();
        let tw = UiBatch::text_width(3.0, &title);
        ui.text_shadow((w - tw) / 2.0, h / 2.0 - 310.0, 3.0, &title, [1.0; 4]);
        if let Some(def) = reg.screens.get(idx) {
            for (i, row) in def.rows().iter().enumerate() {
                let rr = self.mod_screen_row_rect(i);
                match row {
                    crate::screens::ScreenWidget::Label(text) => {
                        ui.text_shadow(
                            rr.0 + 12.0,
                            rr.1 + rr.3 / 2.0 - 10.0,
                            2.0,
                            text,
                            [0.85, 0.85, 0.8, 1.0],
                        );
                    }
                    crate::screens::ScreenWidget::KvLabel { key, prefix } => {
                        let value = self.read_player_kv(key).unwrap_or_else(|| "-".into());
                        ui.text_shadow(
                            rr.0 + 12.0,
                            rr.1 + rr.3 / 2.0 - 10.0,
                            2.0,
                            &format!("{prefix}{value}"),
                            [0.7, 0.9, 0.7, 1.0],
                        );
                    }
                    crate::screens::ScreenWidget::Toggle { label, key } => {
                        let on = self.read_player_kv(key).as_deref() == Some("1");
                        let border = if self.hit(rr) {
                            [1.0, 1.0, 1.0, 0.6]
                        } else {
                            [0.35, 0.35, 0.35, 0.9]
                        };
                        ui.rect(rr.0, rr.1, rr.2, rr.3, border);
                        ui.rect(
                            rr.0 + 2.0,
                            rr.1 + 2.0,
                            rr.2 - 4.0,
                            rr.3 - 4.0,
                            [0.15, 0.15, 0.15, 0.95],
                        );
                        ui.text_shadow(rr.0 + 12.0, rr.1 + rr.3 / 2.0 - 10.0, 2.0, label, [1.0; 4]);
                        let state = if on { "ON" } else { "OFF" };
                        let color = if on {
                            [0.5, 1.0, 0.5, 1.0]
                        } else {
                            [0.6, 0.6, 0.6, 1.0]
                        };
                        ui.text_shadow(
                            rr.0 + rr.2 - 70.0,
                            rr.1 + rr.3 / 2.0 - 10.0,
                            2.0,
                            state,
                            color,
                        );
                    }
                    crate::screens::ScreenWidget::Button { label, .. } => {
                        let border = if self.hit(rr) {
                            [1.0, 1.0, 1.0, 0.6]
                        } else {
                            [0.45, 0.4, 0.25, 0.95]
                        };
                        ui.rect(rr.0, rr.1, rr.2, rr.3, border);
                        ui.rect(
                            rr.0 + 2.0,
                            rr.1 + 2.0,
                            rr.2 - 4.0,
                            rr.3 - 4.0,
                            [0.16, 0.14, 0.1, 0.97],
                        );
                        ui.text_shadow(
                            rr.0 + 12.0,
                            rr.1 + rr.3 / 2.0 - 10.0,
                            2.0,
                            label,
                            [1.0, 0.95, 0.8, 1.0],
                        );
                    }
                }
            }
        }
        self.draw_player_inventory(&mut *ui);
        self.draw_browser(&mut *ui);
        widgets::held_stack(
            &self.content.reg,
            &mut *ui,
            self.input.ui_cursor,
            self.ui_state.held_stack,
        );
    }
}
