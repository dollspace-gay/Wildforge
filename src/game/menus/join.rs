//! Join menu actions.

use crate::audio::Sfx;
use crate::identity;
use crate::net;
use crate::game::Game;
use crate::game::navigation::Screen;

impl Game {
    pub(in crate::game) fn click_join_menu(&mut self) {
                if self.hit(self.pack_back_rect()) {
                    self.sfx(Sfx::Click);
                    self.multiplayer.discovery = None;
                    self.multiplayer.pending_join_disclosure = None;
                    self.set_screen(Screen::Title);
                    return;
                }
                let w = self.renderer.config.width as f32;
                let h = self.renderer.config.height as f32;
                let found: Vec<net::DiscoveredServer> = self
                    .multiplayer
                    .discovery
                    .as_ref()
                    .map(|d| d.found.clone())
                    .unwrap_or_default();
                for (i, found) in found.iter().take(5).enumerate() {
                    let r = (w / 2.0 - 220.0, h * 0.20 + i as f32 * 56.0, 440.0, 42.0);
                    if self.hit(r) {
                        self.sfx(Sfx::Click);
                        self.request_join(found.addr, Some(found.identity));
                        return;
                    }
                }
                let y = h * 0.20 + found.len().clamp(1, 5) as f32 * 56.0 + 26.0;
                let cr = (w / 2.0 + 240.0, y - 6.0, 160.0, 34.0);
                if self.hit(cr) {
                    self.sfx(Sfx::Click);
                    let text = self.multiplayer.join_ip.trim().to_string();
                    let addr = if text.contains(':') {
                        text.parse().ok()
                    } else {
                        format!("{text}:{}", net::GAME_PORT).parse().ok()
                    };
                    match addr {
                        Some(a) => {
                            self.request_join(a, None);
                        }
                        None => self.multiplayer.join_status = "BAD ADDRESS".to_string(),
                    }
                }
                }
}
