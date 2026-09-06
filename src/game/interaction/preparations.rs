//! Preparations interaction adapter.

use crate::audio::Sfx;
use crate::game::Game;
use crate::net;

impl Game {
    pub(in crate::game) fn perform_alchemy_action(
        &mut self,
        pos: crate::planet::BlockPos,
        action: crate::alchemy::ApparatusAction,
    ) {
        let expected_revision = self.interaction.alchemy_revisions.get(&pos).copied();
        if let Some(remote) = &self.multiplayer.remote {
            remote.session.send(&net::C2S::OperateAlchemy {
                pos,
                expected_revision,
                action,
            });
            return;
        }
        let actor = crate::identity::local_player_id(
            &self.runtime.local().world.save_dir_for_saving(),
            self.identity.device_id(),
        )
        .unwrap_or(crate::identity::PlayerId([0; 16]));
        let request = crate::alchemy::AlchemyRequest {
            actor: actor.0,
            actor_label: self.config.display_name.clone(),
            expected_revision,
            action,
        };
        match self
            .runtime
            .local_mut()
            .world
            .operate_alchemy(pos, &mut self.inventory, request)
        {
            Ok(result) => {
                self.interaction
                    .alchemy_revisions
                    .insert(pos, result.revision);
                self.present_alchemy_cue(result.cue);
            }
            Err(error) => {
                self.sfx(Sfx::ImplementFailure);
                self.toast(error);
            }
        }
    }

    pub(in crate::game) fn use_selected_preparation(
        &mut self,
        target: crate::alchemy::AlchemyTarget,
    ) {
        let slot = self.input.hotbar_sel;
        if let Some(remote) = &self.multiplayer.remote {
            remote.session.send(&net::C2S::UsePreparation {
                slot: slot as u8,
                target,
            });
            return;
        }
        let Some(actor_pos) = self.player.pos.block() else {
            return;
        };
        let actor = crate::identity::local_player_id(
            &self.runtime.local().world.save_dir_for_saving(),
            self.identity.device_id(),
        )
        .unwrap_or(crate::identity::PlayerId([0; 16]));
        match self.runtime.local_mut().world.use_preparation(
            actor.0,
            &self.config.display_name,
            actor_pos,
            &mut self.inventory,
            slot,
            target,
        ) {
            Ok(result) => self.present_alchemy_cue(result.cue),
            Err(error) => {
                self.sfx(Sfx::ImplementFailure);
                self.toast(error);
            }
        }
    }
}
