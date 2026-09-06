//! Authenticated chat request adapter.

use super::{C2S, HostFx, HostSession, S2C, Server};

impl HostSession {
    pub(super) fn request_chat(
        &mut self,
        server: &mut Server,
        id: u32,
        msg: C2S,
        fx: &mut Vec<HostFx>,
    ) {
        let Some(guest) = self.guests.get_mut(&id) else {
            return;
        };
        if let C2S::Chat(msg) = msg {
            if guest.chat_count >= 5
                || self
                    .moderation
                    .as_ref()
                    .is_some_and(|store| store.is_muted(&guest.principal))
            {
                self.net
                    .send(id, &S2C::Toast("Chat is rate-limited or muted.".into()));
                return;
            }
            guest.chat_count += 1;
            let msg: String = msg.chars().take(200).collect();
            let from = guest.name.clone();
            // Capture & stamp commands (spec Part 1.4) run against the
            // host world and answer the guest with toasts; instant stamp
            // needs the invoker's inventory, which the host does not
            // hold, so it is host-console only.
            if let Some(rest) = msg.strip_prefix('!') {
                let rest = rest.trim_start();
                let reply: Vec<String> = if rest.starts_with("stamp ") {
                    vec![
                        "stamp runs from the host player's console (ghost/capture work here)"
                            .into(),
                    ]
                } else {
                    server.world.template_command(&msg)
                };
                for text in reply {
                    self.net.send(id, &S2C::Toast(text));
                }
                return;
            }
            self.broadcast_ready(&S2C::Chat {
                from: from.clone(),
                msg: msg.clone(),
            });
            fx.push(HostFx::Chat { from, msg });
        }
    }
}
