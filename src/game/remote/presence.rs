//! Presence graphical guest adapter.

use super::RemoteFlow;
use super::presence_label;
use crate::game::Game;
use crate::game::Remote;
use crate::net;

impl Game {
    pub(in crate::game) fn remote_presence_message(
        &mut self,
        r: &mut Remote,
        message: net::S2C,
    ) -> RemoteFlow {
        match message {
            net::S2C::Sleep { sleeping, present } => {
                self.toast(format!("{sleeping}/{present} sleeping..."));
            }
            net::S2C::Toast(msg) => self.toast(msg),
            net::S2C::Chat { from, msg } => self.toast(format!("{from}: {msg}")),
            net::S2C::Joined { presence } => {
                if presence.id != r.my_id {
                    self.toast(format!("{} joined.", presence.display_name));
                }
                r.session.joined(presence);
            }
            net::S2C::Left { id } => {
                r.players.remove(&id);
                r.player_positions.remove(&id);
                r.player_lerp.remove(&id);
                r.player_held.remove(&id);
                r.player_implement.remove(&id);
                r.player_style.remove(&id);
                if let Some(presence) = r.session.left(id) {
                    self.toast(format!("{} left.", presence_label(&presence)));
                }
            }
            net::S2C::RoleChanged { role } => {
                r.role = role;
                self.toast(format!("Your server role is now {role:?}."));
            }
            _ => {}
        }
        RemoteFlow::Continue
    }
}
