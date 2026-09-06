//! Cargo graphical containers adapter.

use crate::inventory;
use crate::net;
use crate::world;
use crate::game::Game;

impl Game {
    /// One click in a mob's pack: local worlds mutate directly;
    /// guests send the click and predict nothing (the host echoes).
    pub(in crate::game) fn mob_cargo_click(&mut self, mob_id: u32, slot: usize, right: bool) {
        if slot >= 12 {
            return;
        }
        let reg = self.content.reg.clone();
        if let Some(rc) = &self.multiplayer.remote {
            rc.session.send(&net::C2S::MobCargoClick {
                id: mob_id,
                slot: slot as u8,
                right,
            });
            return;
        }
        let held = self.ui_state.held_stack;
        let Some(mob) = self.runtime.local_mut().world.mob_by_id_mut(mob_id) else {
            return;
        };
        let Some(cargo) = mob.cargo.as_mut() else {
            return;
        };
        let (ns, nh) = inventory::click_stack(&reg, cargo[slot], held, right);
        cargo[slot] = ns;
        self.ui_state.held_stack = nh;
    }
}
