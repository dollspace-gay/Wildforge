//! Ordered text input and repeat filtering before high-level key mapping.

use winit::event::KeyEvent;
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use super::{Game, Screen};
use super::text_input::TextAction;
use crate::net;

impl Game {
    pub(super) fn keyboard_event(&mut self, event: &KeyEvent, event_loop: &ActiveEventLoop) {
        let action = self.ui_state.edit_startup_text(event);
        if self.apply_text_action(action) {
            return;
        }
        // Join-screen IP entry.
        if self.ui_state.screen == Screen::Join && event.state.is_pressed() {
            match event.physical_key {
                PhysicalKey::Code(KeyCode::Backspace) => {
                    self.multiplayer.join_ip.pop();
                }
                _ => {
                    if let Some(t) = &event.text {
                        for ch in t.chars() {
                            if (ch.is_ascii_alphanumeric() || ".:".contains(ch))
                                && self.multiplayer.join_ip.len() < 40
                            {
                                self.multiplayer.join_ip.push(ch);
                            }
                        }
                    }
                }
            }
            // Esc still handled below for leaving the screen.
            if !matches!(event.physical_key, PhysicalKey::Code(KeyCode::Escape)) {
                return;
            }
        }
        // Chat entry (multiplayer).
        if self.multiplayer.chat_open && event.state.is_pressed() {
            match event.physical_key {
                PhysicalKey::Code(KeyCode::Escape) => {
                    self.multiplayer.chat_open = false;
                    self.multiplayer.chat_text.clear();
                }
                PhysicalKey::Code(KeyCode::Enter) => {
                    let msg: String = self
                        .multiplayer
                        .chat_text
                        .trim()
                        .chars()
                        .take(200)
                        .collect();
                    self.multiplayer.chat_open = false;
                    self.multiplayer.chat_text.clear();
                    if !msg.is_empty() {
                        let me = self.config.display_name.clone();
                        if let Some(r) = &self.multiplayer.remote {
                            r.session.send(&net::C2S::Chat(msg.clone()));
                        } else if msg.starts_with('!') {
                            // Capture & stamp commands (spec Part 1.4)
                            // run against the local/host world and
                            // answer with toasts instead of chat.
                            for reply in self.template_command(&msg) {
                                self.toast(reply);
                            }
                        } else if let Some(h) = &self.multiplayer.host {
                            h.net.broadcast(&net::S2C::Chat {
                                from: me.clone(),
                                msg: msg.clone(),
                            });
                            self.toast(format!("{me}: {msg}"));
                        } else {
                            self.toast(format!("{me}: {msg}"));
                        }
                    }
                }
                PhysicalKey::Code(KeyCode::Backspace) => {
                    self.multiplayer.chat_text.pop();
                }
                _ => {
                    if let Some(t) = &event.text {
                        for ch in t.chars() {
                            if !ch.is_control() && self.multiplayer.chat_text.len() < 200 {
                                self.multiplayer.chat_text.push(ch);
                            }
                        }
                    }
                }
            }
            return;
        }
        let action = self.ui_state.edit_inventory_text(event);
        if self.apply_text_action(action) {
            return;
        }
        if let PhysicalKey::Code(code) = event.physical_key {
            // The OS repeats a held key, and every edge-triggered
            // action below reads a repeat as a fresh press. Holding
            // space to climb in creative therefore double-tapped
            // itself back out of flight the moment the repeat delay
            // elapsed — which is why it only ever "caught" once you
            // were already a little way up. Holding Escape flapped
            // the pause menu, and holding the drop key emptied the
            // stack, for the same reason. Held-key state is set from
            // the first press and cleared on release, so dropping
            // repeats costs movement nothing.
            //
            // Text entry needs repeats and is handled above, before
            // this point.
            if event.repeat && event.state.is_pressed() {
                return;
            }
            self.key(code, event.state.is_pressed(), event_loop);
        }
    }

    fn apply_text_action(&mut self, action: TextAction) -> bool {
        match action {
            TextAction::Unhandled => return false,
            TextAction::Consumed => {}
            TextAction::CreateWorld => self.create_new_world(),
            TextAction::CommitSign(pos) => self.commit_sign(pos),
        }
        true
    }
}
