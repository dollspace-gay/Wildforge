//! Graphical guest transport ownership and ordered response routing.

use crate::game::Game;
use crate::net;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RemoteFlow {
    Continue,
    Abort,
}

impl Game {
    /// Everything a guest does per frame: apply the host's stream, send
    /// our movement. The local Server never advances in remote mode.
    pub(super) fn remote_pump(&mut self, dt: f32) {
        let Some(mut r) = self.multiplayer.remote.take() else {
            return;
        };
        if !r.session.is_connected() {
            r.session.close();
            if self.in_world {
                self.toast("Disconnected from host.".to_string());
                self.quit_to_title();
            } else {
                self.multiplayer.join_status = "DISCONNECTED DURING WORLD PREPARATION".into();
                self.multiplayer.remote = None;
            }
            return;
        }
        let msgs = r.session.poll();
        if !msgs.is_empty() {
            r.session.note_activity(std::time::Instant::now());
        } else if !self.in_world && r.session.admission().timed_out(std::time::Instant::now()) {
            self.multiplayer.join_status = "WORLD PREPARATION TIMED OUT".into();
            self.multiplayer.remote = None;
            return;
        }
        for msg in msgs {
            let message = if let Some((world, time)) = self.runtime.guest_mut() {
                r.session.apply_world_message(msg, world, time)
            } else {
                Some(msg)
            };
            let Some(msg) = message else {
                continue;
            };
            match msg {
                net::S2C::TimeIre { .. }
                | net::S2C::WeatherCells { .. }
                | net::S2C::ArcaneCue { .. }
                | net::S2C::ArcaneItems { .. }
                | net::S2C::SignText { .. }
                | net::S2C::SwitchState { .. } => {}
                net::S2C::Challenge { .. } => {}
                message @ net::S2C::ModFiles(..) => {
                    if self.remote_entry_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::Welcome { .. } => {
                    if self.remote_entry_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::EntryManifest { .. } => {
                    if self.remote_entry_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::EntryProgress { .. } => {
                    if self.remote_entry_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::EntryAccepted => {
                    if self.remote_entry_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::Refused(..) => {
                    if self.remote_entry_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::Chunk { .. } => {
                    if self.remote_terrain_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::BlockSet { .. } => {
                    if self.remote_terrain_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::Players(..) => {
                    if self.remote_entities_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::Mobs(..) => {
                    if self.remote_entities_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::ViewDistance { .. } => {
                    if self.remote_terrain_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::Falling(..) => {
                    if self.remote_entities_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::Bolts(..) => {
                    if self.remote_entities_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::LooseItems(..) => {
                    if self.remote_entities_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::DiscoveryReport(..) => {
                    if self.remote_discovery_message(message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::DiscoveryRecords { .. } => {
                    if self.remote_discovery_message(message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::KnowledgeText { .. } => {
                    if self.remote_discovery_message(message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::BindingFrameResult { .. } => {
                    if self.remote_magic_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::AlchemyResult { .. } => {
                    if self.remote_magic_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::PreparationResult(..) => {
                    if self.remote_magic_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::PreparationState { .. } => {
                    if self.remote_magic_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::DrossEvent(..) => {
                    if self.remote_magic_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::AlchemyEvent(..) => {
                    if self.remote_magic_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::ImplementActivation { .. } => {
                    if self.remote_magic_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::WorkingResult(..) => {
                    if self.remote_magic_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::WorkingEvent(..) => {
                    if self.remote_magic_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::Hit { .. } => {
                    if self.remote_player_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::MobHit { .. } => {
                    if self.remote_player_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::Give { .. } => {
                    if self.remote_player_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::PlayerState(..) => {
                    if self.remote_player_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::SettlementDelivery { .. } => {
                    if self.remote_player_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::MobCargo { .. } => {
                    if self.remote_containers_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::Container { .. } => {
                    if self.remote_containers_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::MachineContainer { .. } => {
                    if self.remote_containers_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::HeldResult(..) => {
                    if self.remote_containers_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::Sleep { .. } => {
                    if self.remote_presence_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::Toast(..) => {
                    if self.remote_presence_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::Chat { .. } => {
                    if self.remote_presence_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::Joined { .. } => {
                    if self.remote_presence_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::Left { .. } => {
                    if self.remote_presence_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
                message @ net::S2C::RoleChanged { .. } => {
                    if self.remote_presence_message(&mut r, message) == RemoteFlow::Abort {
                        return;
                    }
                }
            }
        }
        if self.remote_entry_terrain(&mut r) == RemoteFlow::Abort {
            return;
        }
        self.remote_presentation(&mut r, dt);
        self.remote_upstream(&mut r, dt);
        self.multiplayer.remote = Some(r);
    }
}

pub(super) fn presence_label(presence: &net::PlayerPresence) -> String {
    let handle = presence
        .handle
        .as_deref()
        .map(|handle| format!(" @{handle}"))
        .unwrap_or_default();
    if presence.cached_verification {
        format!("{}{handle} [VERIFIED/CACHED]", presence.display_name)
    } else if presence.verified {
        format!("{}{handle} [VERIFIED]", presence.display_name)
    } else {
        presence.display_name.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roster_labels_only_show_an_explicitly_disclosed_handle() {
        let private = net::PlayerPresence {
            id: 1,
            display_name: "MOSS".into(),
            verified: true,
            cached_verification: false,
            handle: None,
        };
        assert_eq!(presence_label(&private), "MOSS [VERIFIED]");

        let public = net::PlayerPresence {
            handle: Some("moss.example".into()),
            ..private
        };
        assert_eq!(presence_label(&public), "MOSS @moss.example [VERIFIED]");
    }
}

mod connection;
mod containers;
mod discovery;
mod entities;
mod entry;
mod frame;
mod magic;
mod player;
mod presence;
mod terrain;
mod terrain_requests;
