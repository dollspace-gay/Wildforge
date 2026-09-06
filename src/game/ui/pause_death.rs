//! Pause death layout and UI composition.

use crate::game::Game;
use crate::game::widgets;
use crate::ui::UiBatch;

impl Game {
    pub(in crate::game) fn draw_paused_screen(&mut self, ui: &mut UiBatch, w: f32, h: f32) {
        ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.6]);
        let title = "GAME PAUSED";
        let tw = UiBatch::text_width(4.0, title);
        ui.text_shadow((w - tw) / 2.0, h / 2.0 - 130.0, 4.0, title, [1.0; 4]);
        let mode = if self.creative {
            "MODE: CREATIVE"
        } else {
            "MODE: SURVIVAL"
        };
        let friends = match &self.multiplayer.host {
            Some(h) => format!(
                "FRIENDS: {} CONNECTED ({})",
                h.guests.values().filter(|guest| guest.is_active()).count(),
                h.identity_policy.as_str().to_uppercase()
            ),
            None if self.multiplayer.remote.is_some() => "CONNECTED AS GUEST".to_string(),
            None => "OPEN TO FRIENDS".to_string(),
        };
        for (i, label) in [
            "RESUME",
            mode,
            &friends,
            "SETTINGS",
            "APPEARANCE",
            "SAVE AND QUIT TO TITLE",
        ]
        .iter()
        .enumerate()
        {
            let r = self.menu_button_rect(i);
            widgets::button(&mut *ui, r, label, self.hit(r));
        }
        // Hosting: each guest gets a name row and a KICK button.
        for (row, (_, name)) in self.guest_rows().iter().enumerate() {
            let r = self.kick_rect(row);
            ui.text_shadow(
                r.0,
                r.1 - 16.0,
                1.5,
                &name.to_uppercase(),
                [0.9, 0.9, 0.9, 1.0],
            );
            widgets::button(&mut *ui, r, "MANAGE", self.hit(r));
        }
    }
    pub(in crate::game) fn draw_dead_screen(&mut self, ui: &mut UiBatch, w: f32, h: f32) {
        ui.rect(0.0, 0.0, w, h, [0.5, 0.0, 0.0, 0.5]);
        let title = "YOU DIED";
        let tw = UiBatch::text_width(5.0, title);
        ui.text_shadow(
            (w - tw) / 2.0,
            h / 2.0 - 120.0,
            5.0,
            title,
            [1.0, 0.85, 0.85, 1.0],
        );
        if self.survival.killed_by_wild {
            let sub = "RECLAIMED BY THE WILD";
            let sw = UiBatch::text_width(2.0, sub);
            ui.text_shadow(
                (w - sw) / 2.0,
                h / 2.0 - 60.0,
                2.0,
                sub,
                [0.8, 0.95, 0.75, 1.0],
            );
        }
        let r = self.menu_button_rect(0);
        let hover = self.hit(r);
        let bg = if hover {
            [0.5, 0.5, 0.5, 0.95]
        } else {
            [0.25, 0.25, 0.25, 0.95]
        };
        ui.rect(r.0, r.1, r.2, r.3, [0.1, 0.1, 0.1, 0.95]);
        ui.rect(r.0 + 2.0, r.1 + 2.0, r.2 - 4.0, r.3 - 4.0, bg);
        let lw = UiBatch::text_width(2.0, "RESPAWN");
        ui.text_shadow(
            r.0 + (r.2 - lw) / 2.0,
            r.1 + (r.3 - 14.0) / 2.0,
            2.0,
            "RESPAWN",
            [1.0; 4],
        );
    }
}
