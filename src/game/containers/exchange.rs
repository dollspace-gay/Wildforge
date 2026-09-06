//! Exchange graphical containers adapter.

use super::ContainerPanel;
use crate::game::Game;
use crate::net;

impl Game {
    pub(in crate::game) fn offering_click(
        &mut self,
        pos: crate::planet::BlockPos,
        slot: usize,
        right: bool,
    ) {
        self.exchange_container_slot(pos, slot, right, ContainerPanel::Offering);
    }

    /// The same exchange updates a local container or predicts the received
    /// snapshot. The host echo remains the truth for a graphical guest.
    pub(super) fn exchange_container_slot(
        &mut self,
        pos: crate::planet::BlockPos,
        slot: usize,
        right: bool,
        panel: ContainerPanel,
    ) {
        self.remote_container_notify(pos, slot, right);
        if !self
            .runtime
            .view()
            .block_entity_at(&pos)
            .is_some_and(|entity| panel.accepts(entity, &self.content.reg))
        {
            return;
        }
        let result = self.runtime.click_container(
            pos,
            &mut self.ui_state.held_stack,
            crate::player_ops::container::Click {
                slot,
                right,
                actor: None,
            },
        );
        if result.is_ok_and(|effect| effect.took_furnace_output) {
            self.grant_xp("smelt");
        }
    }

    /// Guests mirror container clicks to the host with the cursor stack
    /// riding along; the local mutation that follows is a prediction
    /// (same click_stack, same synced content) and the Container +
    /// HeldResult echo is the truth that reconciles it.
    pub(in crate::game) fn remote_container_notify(
        &mut self,
        pos: crate::planet::BlockPos,
        slot: usize,
        right: bool,
    ) {
        let Some(r) = &self.multiplayer.remote else {
            return;
        };
        r.session.send(&net::C2S::ContainerClick {
            pos,
            slot: slot as u8,
            right,
        });
    }

    pub(in crate::game) fn chest_click(
        &mut self,
        pos: crate::planet::BlockPos,
        slot: usize,
        right: bool,
    ) {
        self.exchange_container_slot(pos, slot, right, ContainerPanel::Chest);
    }

    pub(in crate::game) fn furnace_click(
        &mut self,
        pos: crate::planet::BlockPos,
        slot: usize,
        right: bool,
    ) {
        self.exchange_container_slot(pos, slot, right, ContainerPanel::Furnace);
    }
}
