//! Settings layout layout and UI composition.

use crate::game::Game;

impl Game {
    pub(in crate::game) fn slider_bar_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (w / 2.0 - 20.0, h * 0.30 + i as f32 * 64.0, 300.0, 30.0)
    }

    /// The toggle rows under the sliders (lights, darkness, outline).
    /// Top of the toggle stack: directly under the four sliders.
    pub(in crate::game) fn settings_toggles_top(h: f32) -> f32 {
        h * 0.30 + 4.0 * 64.0
    }

    /// Vertical step between settings toggles. The designed 56 wherever there's
    /// room; on a short window it compresses (down to a 2px gap) so five rows
    /// plus BACK still land on screen instead of running off the bottom.
    pub(in crate::game) fn settings_step(h: f32) -> f32 {
        const ROWS: f32 = 5.0;
        let avail = h - Self::settings_toggles_top(h) - 50.0;
        (avail / (ROWS + 1.0)).clamp(44.0, 56.0)
    }

    pub(in crate::game) fn settings_toggle_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (
            w / 2.0 - 20.0,
            Self::settings_toggles_top(h) - 8.0 + i as f32 * Self::settings_step(h),
            300.0,
            42.0,
        )
    }

    pub(in crate::game) fn settings_back_rect(&self) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (
            w / 2.0 - 150.0,
            Self::settings_toggles_top(h) + 5.0 * Self::settings_step(h) + 8.0,
            300.0,
            42.0,
        )
    }

    pub(in crate::game) fn slider_frac(&self, i: usize) -> f32 {
        match i {
            0 => self.config.volume,
            1 => (self.config.sensitivity - 0.1) / 2.9,
            2 => {
                let top = self.presentation.max_view_dist;
                (self.config.view_dist - crate::config::MIN_VIEW_DIST) as f32
                    / (top - crate::config::MIN_VIEW_DIST).max(1) as f32
            }
            _ => (self.config.fov - 50.0) / 60.0,
        }
    }

    pub(in crate::game) fn slider_label(&self, i: usize) -> String {
        match i {
            0 => format!("{:.0}", self.config.volume * 100.0),
            1 => format!("{:.2}", self.config.sensitivity),
            2 => format!("{}", self.config.view_dist),
            _ => format!("{:.0}", self.config.fov),
        }
    }

    pub(in crate::game) fn set_slider(&mut self, i: usize, frac: f32) {
        let f = frac.clamp(0.0, 1.0);
        match i {
            0 => self.config.volume = (f * 20.0).round() / 20.0,
            1 => self.config.sensitivity = ((0.1 + f * 2.9) * 20.0).round() / 20.0,
            2 => {
                // Four-chunk steps past the old maximum: nobody is
                // choosing between 47 and 48 chunks, and a long throw
                // with a fine step makes the slider unusable.
                //
                // The top of the throw is what this machine can hold, not a
                // constant. The slider used to run to 64 everywhere, which is
                // over 4 GB of resident chunks — a setting that ended the
                // process rather than showing you the next valley.
                let min = crate::config::MIN_VIEW_DIST;
                let top = self.presentation.max_view_dist;
                let raw = min as f32 + f * (top - min).max(1) as f32;
                let stepped = if raw <= 16.0 {
                    raw.round() as i32
                } else {
                    (raw / 4.0).round() as i32 * 4
                };
                self.config.view_dist = stepped.clamp(min, top);
            }
            _ => self.config.fov = 50.0 + (f * 60.0).round(),
        }
        self.apply_config();
    }
}
