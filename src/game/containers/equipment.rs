//! Equipment graphical containers adapter.

use crate::game::Game;
use crate::net;

impl Game {
    pub(in crate::game) fn armor_click(&mut self, i: usize) {
        if let Some(remote) = &self.multiplayer.remote {
            remote.session.send(&net::C2S::InventoryClick {
                area: net::InventoryArea::Armor,
                slot: i as u8,
                right: false,
            });
        }
        // Swapping or removing a frame must not strand its components.
        if i < 4 {
            self.return_loadout_components(i);
        }
        crate::player_ops::equipment::exchange(
            &self.content.reg,
            &mut self.survival.armor,
            &mut self.ui_state.held_stack,
            i,
        );
    }
}
