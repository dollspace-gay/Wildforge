//! Magic graphical guest adapter.

use super::RemoteFlow;
use crate::audio::Sfx;
use crate::net;
use crate::game::Remote;

impl Game {
    pub(in crate::game) fn remote_magic_message(&mut self, r: &mut Remote, message: net::S2C) -> RemoteFlow {
        match message {
                net::S2C::BindingFrameResult { pos, result } => {
                    self.interaction
                        .binding_revisions
                        .insert(pos, result.revision);
                    let cue = result.cue;
                    self.presentation.swing = 1.0;
                    self.toast(result.message);
                    for line in result.lines.into_iter().take(3) {
                        self.toast(line);
                    }
                    self.sfx(match cue {
                        crate::implements::ImplementCue::Use => Sfx::ImplementUse,
                        crate::implements::ImplementCue::Transfer => Sfx::ImplementTransfer,
                        crate::implements::ImplementCue::Strain => Sfx::ImplementStrain,
                        crate::implements::ImplementCue::Empty => Sfx::ImplementEmpty,
                        crate::implements::ImplementCue::Failure => Sfx::ImplementFailure,
                    });
                }
                net::S2C::AlchemyResult { pos, result } => {
                    self.interaction
                        .alchemy_revisions
                        .insert(pos, result.revision);
                    self.present_alchemy_cue(result.cue);
                }
                net::S2C::PreparationResult(result) => {
                    self.present_alchemy_cue(result.cue);
                }
                net::S2C::PreparationState {
                    modifiers,
                    bodily_dross,
                } => {
                    let old_dross_band = self.survival.preparation_modifiers.dross_band;
                    self.survival.preparation_modifiers = modifiers;
                    self.survival.bodily_dross = bodily_dross;
                    if modifiers.dross_band > old_dross_band && modifiers.dross_band != 0 {
                        self.sfx(Sfx::DrossWarning(modifiers.dross_band));
                        let (band, pattern) =
                            crate::game::status::dross_warning_text(modifiers.dross_band);
                        self.toast(format!("DROSS {band} — {pattern}"));
                    }
                }
                net::S2C::DrossEvent(cue) => self.present_dross_cue(cue),
                net::S2C::AlchemyEvent(cue) => self.present_alchemy_cue(cue),
                net::S2C::ImplementActivation {
                    actor,
                    pos,
                    cue,
                    visual,
                } => {
                    self.present_implement_activation(
                        pos,
                        cue,
                        visual,
                        Some(r.session.content().items()),
                    );
                    // The next player snapshot remains authoritative for the
                    // held model; this short-lived event only drives the
                    // visible settling gesture and local envelope.
                    if actor != r.my_id {
                        r.player_age = r.player_age.min(r.player_interval * 0.5);
                    }
                }
                net::S2C::WorkingResult(result) => {
                    if result.success {
                        if let Some(channel) = self.interaction.working.as_mut()
                            && result.phase.is_some()
                        {
                            channel.stable_id = result.stable_id;
                        }
                        if result.phase.is_none() {
                            self.interaction.working = None;
                        }
                    } else {
                        self.interaction.working = None;
                    }
                    self.toast(result.message);
                    self.sfx(if result.success {
                        Sfx::ImplementUse
                    } else {
                        Sfx::ImplementFailure
                    });
                }
                net::S2C::WorkingEvent(cue) => self.present_working_cue(cue),
            _ => {}
        }
        RemoteFlow::Continue
    }
}
