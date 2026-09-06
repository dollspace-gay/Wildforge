//! Writing panel layout and UI composition.

use crate::game::Game;
use crate::ui::UiBatch;

impl Game {
    pub(in crate::game) fn draw_sign_edit_screen(&mut self, ui: &mut UiBatch, w: f32, h: f32) {
        ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.6]);
        let title = "WRITE";
        let tw = UiBatch::text_width(3.0, title);
        ui.text_shadow((w - tw) / 2.0, h / 2.0 - 150.0, 3.0, title, [1.0; 4]);
        for i in 0..3 {
            let y = h / 2.0 - 90.0 + i as f32 * 44.0;
            let active = i == self.ui_state.sign_line;
            ui.rect(
                w / 2.0 - 170.0,
                y,
                340.0,
                34.0,
                if active {
                    [0.25, 0.25, 0.28, 0.9]
                } else {
                    [0.12, 0.12, 0.14, 0.9]
                },
            );
            let text = format!(
                "{}{}",
                self.ui_state.sign_lines[i].to_uppercase(),
                if active { "_" } else { "" }
            );
            ui.text_shadow(w / 2.0 - 160.0, y + 8.0, 2.0, &text, [1.0; 4]);
        }
        ui.text_shadow(
            w / 2.0 - 170.0,
            h / 2.0 + 60.0,
            1.5,
            "ENTER: NEXT LINE / DONE",
            [0.8, 0.8, 0.8, 1.0],
        );
    }
}
