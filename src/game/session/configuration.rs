//! Configuration graphical session adapter.

use crate::game::Game;
use crate::game::navigation::Screen;
use crate::world;

impl Game {
    pub(in crate::game) fn apply_config(&mut self) {
        self.camera.sens = self.config.sensitivity;
        self.camera.fovy = self.config.fov.to_radians();
        if let Some(a) = &mut self.audio {
            a.volume = self.config.volume;
        }
        self.config.save();
    }

    pub(in crate::game) fn refresh_worlds(&mut self) {
        self.worlds = world::list_worlds(std::path::Path::new("saves"));
        let inspection = world::inspect_worlds(std::path::Path::new("saves"));
        self.world_details = inspection
            .iter()
            .filter(|entry| entry.playable)
            .map(|entry| (entry.name.clone(), entry.status.clone()))
            .collect();
        self.world_problems = inspection
            .into_iter()
            .filter(|entry| !entry.playable)
            .map(|entry| (entry.name, entry.status))
            .collect();
    }

    pub(in crate::game) fn open_new_world(&mut self, mode: &str) {
        self.ui_state.new_world_mode = mode.to_string();
        self.roll_new_world_seed();
        self.ui_state.new_world_status.clear();
        self.set_screen(Screen::NewWorld);
    }

    pub(in crate::game) fn roll_new_world_seed(&mut self) {
        let seed = (self.rand01() * u32::MAX as f32) as u32;
        self.ui_state.new_world_seed = seed.to_string();
    }
}
