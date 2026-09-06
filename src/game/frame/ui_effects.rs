//! Ui effects in the graphical frame pipeline.

use crate::game::Game;
use crate::game::navigation::Screen;

impl Game {
    pub(in crate::game) fn prepare_frame_ui(&mut self) {
        self.build_ui();
        // Screen-open ease: scale from 0.96 and fade in over ~140ms.
        // Animation this short reads as *faster* than a snap, not slower.
        if self.presentation.juice
            && self.ui_state.screen != Screen::Playing
            && self.presentation.screen_age < 1.0
        {
            let t = self.presentation.screen_age;
            let e = 1.0 - (1.0 - t) * (1.0 - t);
            let sc = 0.96 + 0.04 * e;
            let al = 0.85 + 0.15 * e;
            let cx = self.renderer.config.width as f32 / 2.0;
            let cy = self.renderer.config.height as f32 / 2.0;
            for v in &mut self.ui.verts {
                v.pos[0] = cx + (v.pos[0] - cx) * sc;
                v.pos[1] = cy + (v.pos[1] - cy) * sc;
                v.color[3] *= al;
            }
        }

        if self.auto_shot.is_some() {
            self.apply_look_env();
        }
    }
}
