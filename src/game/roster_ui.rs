//! Compact, hold-to-view multiplayer roster.

use super::*;

impl Game {
    pub(super) fn draw_roster_overlay(&self, ui: &mut UiBatch, screen_width: f32) {
        if !self.multiplayer.roster_open {
            return;
        }
        let mut rows = if let Some(remote) = &self.multiplayer.remote {
            remote
                .names
                .iter()
                .map(|(id, label)| {
                    let suffix = if *id == remote.my_id {
                        if remote.role == identity::Role::Player {
                            "  (YOU)".to_string()
                        } else {
                            format!("  (YOU, {:?})", remote.role)
                        }
                    } else if *id == 0 {
                        "  (HOST)".to_string()
                    } else {
                        String::new()
                    };
                    (*id, format!("{label}{suffix}"))
                })
                .collect::<Vec<_>>()
        } else if let Some(host) = &self.multiplayer.host {
            let mut rows = vec![(0, format!("{}  (HOST)", self.config.display_name))];
            rows.extend(
                host.guests
                    .iter()
                    .map(|(id, guest)| (*id, guest.public_label())),
            );
            rows
        } else {
            return;
        };
        rows.sort_by_key(|(id, _)| *id);

        let visible = rows.len().min(16);
        let width = 440.0;
        let x = (screen_width - width) * 0.5;
        let y = 70.0;
        let height = 48.0 + visible as f32 * 25.0;
        ui.rect(x, y, width, height, [0.015, 0.025, 0.035, 0.9]);
        ui.rect(x, y, width, 3.0, [0.5, 0.85, 0.65, 1.0]);
        ui.text_shadow(x + 16.0, y + 14.0, 2.0, "PLAYERS", [1.0; 4]);
        for (row, (_, label)) in rows.iter().take(visible).enumerate() {
            let mut label = label.to_uppercase();
            if label.chars().count() > 56 {
                label = format!("{}...", label.chars().take(53).collect::<String>());
            }
            ui.text_shadow(
                x + 16.0,
                y + 44.0 + row as f32 * 25.0,
                1.5,
                &label,
                [0.9, 0.94, 0.9, 1.0],
            );
        }
    }
}
