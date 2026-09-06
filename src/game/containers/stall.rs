//! Stall graphical containers adapter.

use crate::audio::Sfx;
use crate::identity;
use crate::inventory;
use crate::net;
use crate::world;
use crate::game::Game;

impl Game {
    /// Whether the local player owns this stall (local worlds/hosts).
    pub(in crate::game) fn stall_is_mine(&self, pos: crate::planet::BlockPos) -> bool {
        let my_id = identity::local_player_id(
            &self.runtime.local().world.save_dir_for_saving(),
            self.identity.device_id(),
        )
        .map(|p| p.0)
        .unwrap_or([0; 16]);
        match self.runtime.view().block_entity_at(&pos) {
            Some(world::BlockEntity::Stall(st)) => st.owner == my_id,
            _ => false,
        }
    }

    pub(in crate::game) fn stall_click(&mut self, pos: crate::planet::BlockPos, slot: usize, right: bool) {
        if slot > 12 {
            return;
        }
        if let Some(rc) = &self.multiplayer.remote {
            rc.session.send(&net::C2S::ContainerClick {
                pos,
                slot: slot as u8,
                right,
            });
            return;
        }
        if !self.stall_is_mine(pos) {
            return; // visitors browse; the BUY button is theirs
        }
        let owner = match self.runtime.view().block_entity_at(&pos) {
            Some(world::BlockEntity::Stall(stall)) => stall.owner,
            _ => return,
        };
        let _ = self.runtime.click_container(
            pos, &mut self.ui_state.held_stack,
            crate::player_ops::container::Click { slot, right, actor: Some(owner) },
        );
    }

    /// Local purchase: the singleplayer/host mirror of C2S::StallBuy.
    pub(in crate::game) fn stall_buy_local(&mut self, pos: crate::planet::BlockPos) {
        let reg = self.content.reg.clone();
        if !self.runtime.view().check_stall_at(pos) {
            self.toast("The stall wants its posts and awning.".to_string());
            return;
        }
        let Some(world::BlockEntity::Stall(stall)) = self.runtime.local_mut().world.block_entity_mut_at(&pos) else {
            return;
        };
        let Ok(purchase) = crate::player_ops::trade::purchase(&reg, stall, &mut self.inventory) else {
            return;
        };
        if let Some(stack) = purchase.overflow {
            self.drop_stack(stack);
        }
        self.sfx(Sfx::Pickup);
    }
}
