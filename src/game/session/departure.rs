//! Departure graphical session adapter.

use crate::server;
use crate::world;
use crate::world::World;
use std::path::PathBuf;
use crate::game::Game;
use crate::game::navigation::Screen;

impl Game {
    pub(in crate::game) fn quit_to_title(&mut self) {
        if self.runtime.is_guest() || self.multiplayer.remote.is_some() {
            self.multiplayer.remote = None;
            self.multiplayer.host = None;
            self.multiplayer.host_sleeping = false;
            self.renderer.clear_chunks();
            self.gen_pool = None;
            self.mesh_pool = None;
            self.runtime.set_local(server::Server::new(
                World::new(0, PathBuf::from("saves/.none"), self.content.reg.clone()),
                0.3,
                1,
            ));
            self.runtime.local_mut().world.clear_loose_items();
            self.in_world = false;
            self.refresh_worlds();
            self.set_screen(Screen::Title);
            return;
        }
        if self.in_world
            && let Err(error) = self.save_session()
        {
            eprintln!("world: save and quit cancelled: {error}");
            self.toast(format!("Could not save; still in world: {error}"));
            return;
        }
        self.multiplayer.host = None; // closes connections after durable save
        self.multiplayer.host_sleeping = false;
        self.runtime.local_mut().world.set_edit_logging(false);
        self.renderer.clear_chunks();
        self.gen_pool = None;
        self.mesh_pool = None;
        self.runtime.set_local(server::Server::new(
            World::new(0, PathBuf::from("saves/.none"), self.content.reg.clone()),
            0.3,
            1,
        ));
        self.runtime.local_mut().world.clear_loose_items();
        self.in_world = false;
        self.refresh_worlds();
        self.set_screen(Screen::Title);
    }
}
