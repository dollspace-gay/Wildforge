//! UI text editing with explicit app actions; no world or Game access.

use super::navigation::{Screen, UiState};
use crate::identity;
use crate::planet::BlockPos;
use winit::event::KeyEvent;
use winit::keyboard::{KeyCode, PhysicalKey};

pub(super) enum TextAction {
    Unhandled,
    Consumed,
    CreateWorld,
    CommitSign(BlockPos),
}

impl UiState {
    /// Startup fields precede join/chat input in the platform event order.
    pub(super) fn edit_startup_text(&mut self, event: &KeyEvent) -> TextAction {
        // New-planet seeds are explicit and reproducible. Keep this
        // field deliberately numeric so the UI and world.toml agree
        // on the complete u32 domain without a locale/parser layer.
        if self.screen == Screen::NewWorld && event.state.is_pressed() {
            match event.physical_key {
                PhysicalKey::Code(KeyCode::Backspace) => {
                    self.new_world_seed.pop();
                }
                PhysicalKey::Code(KeyCode::Enter) => return TextAction::CreateWorld,
                _ => {
                    if let Some(text) = &event.text {
                        for ch in text.chars() {
                            if ch.is_ascii_digit() && self.new_world_seed.len() < 10 {
                                self.new_world_seed.push(ch);
                            }
                        }
                    }
                }
            }
            if !matches!(event.physical_key, PhysicalKey::Code(KeyCode::Escape)) {
                return TextAction::Consumed;
            }
        }
        // First-run/profile and ATProto account text entry. OAuth
        // itself runs on a worker so the render/event loop stays live.
        if self.screen == Screen::Accounts && event.state.is_pressed() {
            match event.physical_key {
                PhysicalKey::Code(KeyCode::Tab) => {
                    self.account_focus = 1 - self.account_focus;
                }
                PhysicalKey::Code(KeyCode::Backspace) => {
                    if self.account_focus == 0 {
                        self.account_name.pop();
                    } else {
                        self.account_handle.pop();
                    }
                }
                _ => {
                    if let Some(t) = &event.text {
                        for ch in t.chars() {
                            if self.account_focus == 0
                                && (ch.is_ascii_alphanumeric() || matches!(ch, ' ' | '-' | '.'))
                                && self.account_name.chars().count() < identity::DISPLAY_NAME_MAX
                            {
                                self.account_name.push(ch);
                            } else if self.account_focus == 1
                                && (ch.is_ascii_alphanumeric()
                                    || matches!(ch, '.' | ':' | '-' | '@'))
                                && self.account_handle.len() < 255
                            {
                                self.account_handle.push(ch);
                            }
                        }
                    }
                }
            }
            if !matches!(event.physical_key, PhysicalKey::Code(KeyCode::Escape)) {
                return TextAction::Consumed;
            }
        }
        TextAction::Unhandled
    }

    /// In-world fields follow chat and precede repeat-filtered game keys.
    pub(super) fn edit_inventory_text(&mut self, event: &KeyEvent) -> TextAction {
        if let Screen::SignEdit(pos) = self.screen
            && event.state.is_pressed()
        {
            match event.physical_key {
                PhysicalKey::Code(KeyCode::Backspace) => {
                    let l = self.sign_line;
                    self.sign_lines[l].pop();
                }
                PhysicalKey::Code(KeyCode::Enter) => {
                    if self.sign_line < 2 {
                        self.sign_line += 1;
                    } else {
                        return TextAction::CommitSign(pos);
                    }
                }
                PhysicalKey::Code(KeyCode::Escape) => return TextAction::CommitSign(pos),
                _ => {
                    if let Some(t) = &event.text {
                        let l = self.sign_line;
                        for ch in t.chars() {
                            let ok = ch.is_ascii_alphanumeric() || " :_-'".contains(ch);
                            if ok && self.sign_lines[l].len() < 14 {
                                self.sign_lines[l].push(ch);
                            }
                        }
                    }
                }
            }
            return TextAction::Consumed;
        }
        if self.screen == Screen::Inventory
            && self.inventory_discovery_open
            && self.discovery_label_focus
            && event.state.is_pressed()
        {
            match event.physical_key {
                PhysicalKey::Code(KeyCode::Backspace) => {
                    self.discovery_label.pop();
                }
                PhysicalKey::Code(KeyCode::Escape) | PhysicalKey::Code(KeyCode::Enter) => {
                    self.discovery_label_focus = false;
                }
                _ => {
                    if let Some(text) = &event.text {
                        for ch in text.chars() {
                            let allowed = !ch.is_control()
                                && (ch.is_alphanumeric() || " _-':,.()/#".contains(ch));
                            if allowed && self.discovery_label.chars().count() < 48 {
                                self.discovery_label.push(ch);
                            }
                        }
                    }
                }
            }
            return TextAction::Consumed;
        }
        let searchable = matches!(
            self.screen,
            Screen::Inventory
                | Screen::Furnace(_)
                | Screen::Chest(_)
                | Screen::Offering(_)
                | Screen::Bloomery(_)
        );
        if self.search_focus && searchable && event.state.is_pressed() {
            match event.physical_key {
                PhysicalKey::Code(KeyCode::Backspace) => {
                    self.search.pop();
                    self.browse_page = 0;
                }
                PhysicalKey::Code(KeyCode::Escape) | PhysicalKey::Code(KeyCode::Enter) => {
                    self.search_focus = false;
                }
                _ => {
                    if let Some(t) = &event.text {
                        for ch in t.chars() {
                            if (ch.is_ascii_alphanumeric() || ch == ' ' || ch == ':' || ch == '_')
                                && self.search.len() < 24
                            {
                                self.search.push(ch);
                                self.browse_page = 0;
                            }
                        }
                    }
                }
            }
            return TextAction::Consumed;
        }
        TextAction::Unhandled
    }
}
