//! Moderation menu actions.

use crate::audio::Sfx;
use crate::game::Game;
use crate::game::navigation::Screen;
use crate::mp;
use crate::net;

impl Game {
    pub(in crate::game) fn click_moderation_menu(&mut self, id: u32) {
        for action in 0..6 {
            if !self.hit(self.menu_button_rect(action)) {
                continue;
            }
            self.sfx(Sfx::Click);
            // Disconnecting and ban actions require a second click.
            if action <= 3 && self.ui_state.moderation_confirm != Some(action as u8) {
                self.ui_state.moderation_confirm = Some(action as u8);
                return;
            }
            self.ui_state.moderation_confirm = None;
            let result = if let Some(host) = &mut self.multiplayer.host {
                match action {
                    0 => Ok(host.kick_guest(id).map(|name| format!("{name} kicked"))),
                    1 => host
                        .mute_guest(id, "windowed host mute", Some(600), "host")
                        .map(|changed| changed.then_some("player muted for 10 minutes".into())),
                    2 => host
                        .ban_guest(id, "windowed host timed ban", Some(3600), "host")
                        .map(|name| name.map(|name| format!("{name} banned for one hour"))),
                    3 => host
                        .ban_guest(id, "windowed host permanent ban", None, "host")
                        .map(|name| name.map(|name| format!("{name} permanently banned"))),
                    4 => host
                        .allow_guest(id, "host")
                        .map(|changed| changed.then_some("player added to allowlist".into())),
                    _ => {
                        let next = match host.guest_role(id).unwrap_or_default() {
                            mp::Role::Player => mp::Role::Moderator,
                            mp::Role::Moderator => mp::Role::Admin,
                            mp::Role::Admin | mp::Role::Owner => mp::Role::Player,
                        };
                        host.set_guest_role(id, next, "host")
                            .map(|changed| changed.then_some(format!("role set to {next:?}")))
                    }
                }
            } else if let Some(remote) = &self.multiplayer.remote {
                let moderation_action = match action {
                    0 => net::ModerationAction::Kick,
                    1 => net::ModerationAction::Mute { seconds: 600 },
                    2 => net::ModerationAction::Ban {
                        seconds: Some(3600),
                    },
                    3 => net::ModerationAction::Ban { seconds: None },
                    4 => net::ModerationAction::Allow,
                    _ => net::ModerationAction::CycleRole,
                };
                remote.session.send(&net::C2S::Moderate {
                    target: id,
                    action: moderation_action,
                });
                Ok(Some("Moderation request sent to the host.".into()))
            } else {
                Ok(None)
            };
            match result {
                Ok(Some(message)) => self.toast(message),
                Ok(None) => self.toast("Player is no longer connected.".into()),
                Err(error) => self.toast(format!("Moderation failed: {error}")),
            }
            if matches!(action, 0 | 2 | 3) {
                self.set_screen(Screen::Paused);
            }
            return;
        }
        if self.hit(self.menu_button_rect(6)) {
            self.sfx(Sfx::Click);
            self.ui_state.moderation_confirm = None;
            self.set_screen(Screen::Paused);
        }
    }
}
