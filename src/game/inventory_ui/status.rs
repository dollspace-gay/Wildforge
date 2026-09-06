//! Status graphical inventory ui adapter.

use crate::ui::UiBatch;
use crate::world;
use crate::game::Game;
use crate::game::MAX_HEALTH;

impl Game {
    pub(super) fn draw_inventory_tab(
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

    pub(super) fn draw_inventory_status(&self, ui: &mut UiBatch, rect: (f32, f32, f32, f32)) {
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

        let tier = self.runtime.view().ire_tier();
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
            self.runtime.view().ire() * 1.12,
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

        let dross_text = match self.survival.preparation_modifiers.dross_band {
            1 => "TRACE / GLASS HAZE",
            2 => "STRAINED / TWO-PULSE",
            3 => "SEEP / BRANCHING",
            4 => "SCAR / BROKEN RING",
            5 => "BREACH / SHEAR",
            _ => "CLEAR / EVEN",
        };
        ui.text_shadow(
            info_x,
            rect.1 + 124.0,
            1.1,
            &format!("DROSS {dross_text}"),
            [0.78, 0.86, 0.92, 1.0],
        );

        let world = self.runtime.view();
        let season = world.season_at_surface(self.player.pos.surface());
        let third = ["EARLY", "MID", "LATE"][((world.season_progress() * 3.0) as usize).min(2)];
        ui.text_shadow(
            info_x,
            rect.1 + 146.0,
            1.4,
            &format!("DAY {} - {third} {}", world.day() + 1, world::SEASONS[season]),
            [0.78, 0.86, 1.0, 1.0],
        );
        let weather = world.weather_at_surface(self.player.pos.surface());
        ui.text_shadow(
            info_x,
            rect.1 + 164.0,
            1.0,
            &format!(
                "{}  {:+.0}C  WIND {:.1}",
                weather.kind.name().to_uppercase(),
                weather.temperature_c,
                weather.wind[0].hypot(weather.wind[1]),
            ),
            [0.68, 0.76, 0.86, 1.0],
        );
    }
}
