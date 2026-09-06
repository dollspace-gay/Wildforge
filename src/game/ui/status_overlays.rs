//! Status overlays layout and UI composition.

use crate::ui::UiBatch;
use crate::game::Game;
use super::{wrap_ui_status};

impl Game {
    pub(in crate::game) fn draw_status_overlays(&mut self, ui: &mut UiBatch, w: f32, h: f32) {
        // Damage flash vignette.
        if self.survival.damage_flash > 0.0 {
            if self.presentation.juice {
                let a = self.survival.damage_flash * 0.5;
                for (frac, aa) in [(1.0, 0.5), (0.66, 0.35), (0.33, 0.25)] {
                    let bw = w * 0.14 * frac;
                    let bh = h * 0.18 * frac;
                    let col = [0.8, 0.1, 0.1, a * aa];
                    ui.rect(0.0, 0.0, bw, h, col);
                    ui.rect(w - bw, 0.0, bw, h, col);
                    ui.rect(0.0, 0.0, w, bh, col);
                    ui.rect(0.0, h - bh, w, bh, col);
                }
            } else {
                ui.rect(
                    0.0,
                    0.0,
                    w,
                    h,
                    [0.8, 0.1, 0.1, self.survival.damage_flash * 0.55],
                );
            }
        }

        // Mod/system toasts, top center. Ordinary screenshot sessions hide
        // them so transient UI does not contaminate visual baselines; a
        // focused presentation qualification may opt in explicitly.
        let show_capture_toasts = std::env::var_os("WILDFORGE_SHOT_TOASTS").is_some();
        let mut toast_row = 0usize;
        for (msg, ttl) in self
            .presentation
            .toasts
            .iter()
            .filter(|_| self.auto_shot.is_none() || show_capture_toasts)
        {
            let a = ttl.min(1.0);
            let m = msg.to_uppercase();
            for line in wrap_ui_status(&m, (w - 32.0).max(120.0), 2.0, 3) {
                let tw = UiBatch::text_width(2.0, &line);
                ui.text_shadow(
                    (w - tw) / 2.0,
                    16.0 + toast_row as f32 * 22.0,
                    2.0,
                    &line,
                    [1.0, 1.0, 0.6, a],
                );
                toast_row += 1;
            }
        }

    }
}
