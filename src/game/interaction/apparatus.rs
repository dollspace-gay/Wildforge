//! Apparatus interaction adapter.

use crate::audio::Sfx;
use crate::game::Game;
use crate::net;
use crate::world;

impl Game {
    pub(in crate::game) fn open_discovery_folio(&mut self, pos: crate::planet::BlockPos) {
        if let Some(remote) = &self.multiplayer.remote {
            self.ui_state.discovery_holder = Some(net::RecordHolderSnap::Folio { pos });
            self.ui_state.discovery_copy_target = None;
            self.ui_state.discovery_writing_pos = None;
            remote.session.send(&net::C2S::OpenDiscovery {
                holder: net::RecordHolderSnap::Folio { pos },
            });
            return;
        }
        let object_id = match self.runtime.view().block_entity_at(&pos) {
            Some(world::BlockEntity::SurveyFolio(folio)) if folio.object_id != 0 => folio.object_id,
            _ => {
                self.toast("This folio has no recoverable record identity.".into());
                return;
            }
        };
        match self
            .runtime
            .local()
            .world
            .discovery_library_index(object_id, true)
        {
            Ok(index) => {
                self.open_discovery_catalogue(
                    net::RecordHolderSnap::Folio { pos },
                    index.records,
                    crate::discovery::SURVEY_FOLIO_RECORDS as u16,
                    None,
                    None,
                );
            }
            Err(error) => self.toast(error.to_string()),
        }
    }

    pub(in crate::game) fn assemble_tuning_lens(&mut self, pos: crate::planet::BlockPos) {
        if let Some(remote) = &self.multiplayer.remote {
            remote.session.send(&net::C2S::AssembleTuningLens { pos });
            return;
        }
        match self
            .runtime
            .local_mut()
            .world
            .assemble_tuning_lens_at(pos, &mut self.inventory)
        {
            Ok(_) => {
                self.toast("The Wellglass settles against the Echo Slate plate.".into());
                self.sfx(Sfx::Craft);
            }
            Err(error) => self.toast(error),
        }
    }

    pub(in crate::game) fn exchange_discovery_apparatus_item(
        &mut self,
        pos: crate::planet::BlockPos,
    ) {
        let slot = self.input.hotbar_sel;
        if let Some(kind) = self.inventory.slots[slot]
            .and_then(|stack| self.content.reg.item(stack.item).discovery.as_ref())
            .and_then(|definition| definition.experiment)
            && let Some(index) = crate::discovery::ExperimentKind::ALL
                .iter()
                .position(|candidate| *candidate == kind)
        {
            self.interaction.experiment_kind = index;
        }
        if let Some(remote) = &self.multiplayer.remote {
            remote.session.send(&net::C2S::SetExperimentItem {
                pos,
                slot: slot as u8,
            });
            return;
        }
        match self.runtime.local_mut().world.exchange_experiment_item_at(
            pos,
            &mut self.inventory,
            slot,
        ) {
            Ok(message) => self.toast(message),
            Err(error) => self.toast(error),
        }
    }

    pub(in crate::game) fn operate_binding_frame(&mut self, pos: crate::planet::BlockPos) {
        let slot = self.input.hotbar_sel;
        if self.inventory.slots[slot].is_none() {
            if let Some(remote) = &self.multiplayer.remote {
                remote.session.send(&net::C2S::OperateWorking {
                    working_id: "base:auto_ritual".into(),
                    held_instance: 0,
                    target: crate::workings::WorkingTargetIntent::Ritual { controller: pos },
                    intent: crate::workings::WorkingIntent::Start,
                });
                return;
            }
            let player_id = crate::identity::local_player_id(
                &self.runtime.local().world.save_dir_for_saving(),
                self.identity.device_id(),
            )
            .unwrap_or(crate::identity::PlayerId([0; 16]));
            match self.runtime.local_mut().world.begin_contextual_ritual(
                player_id.0,
                &self.config.display_name,
                pos,
            ) {
                Ok(result) => {
                    if let Some(cue) = self
                        .runtime
                        .local()
                        .world
                        .working_cues()
                        .into_iter()
                        .find(|cue| cue.stable_id == result.stable_id)
                    {
                        self.present_working_cue(cue);
                    }
                    self.toast(result.message);
                    self.sfx(Sfx::ImplementUse);
                }
                Err(error) => {
                    self.toast(error);
                    self.sfx(Sfx::ImplementFailure);
                }
            }
            return;
        }
        let expected_revision = self.interaction.binding_revisions.get(&pos).copied();
        if let Some(remote) = &self.multiplayer.remote {
            remote.session.send(&net::C2S::OperateBindingFrame {
                pos,
                slot: slot as u8,
                action: crate::implements::FrameAction::Contextual,
                expected_revision,
            });
            return;
        }
        match self.runtime.local_mut().world.operate_binding_frame(
            pos,
            &mut self.inventory,
            slot,
            crate::implements::FrameAction::Contextual,
            expected_revision,
            "local-player",
        ) {
            Ok(result) => {
                self.interaction
                    .binding_revisions
                    .insert(pos, result.revision);
                self.presentation.swing = 1.0;
                self.toast(result.message);
                for line in result.lines.into_iter().take(3) {
                    self.toast(line);
                }
                self.sfx(match result.cue {
                    crate::implements::ImplementCue::Use => Sfx::ImplementUse,
                    crate::implements::ImplementCue::Transfer => Sfx::ImplementTransfer,
                    crate::implements::ImplementCue::Strain => Sfx::ImplementStrain,
                    crate::implements::ImplementCue::Empty => Sfx::ImplementEmpty,
                    crate::implements::ImplementCue::Failure => Sfx::ImplementFailure,
                });
            }
            Err(error) => {
                self.sfx(match crate::implements::error_cue(&error) {
                    crate::implements::ImplementCue::Empty => Sfx::ImplementEmpty,
                    _ => Sfx::ImplementFailure,
                });
                self.toast(error);
            }
        }
    }
}
