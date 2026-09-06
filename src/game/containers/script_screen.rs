//! Script screen graphical containers adapter.

use crate::audio::Sfx;
use crate::inventory::TOTAL_SLOTS;
use crate::net;
use crate::game::Game;
use crate::game::navigation::Screen;

impl Game {
    /// The def behind the open mod screen, if any.
    pub(in crate::game) fn mod_screen_def(&self) -> Option<&crate::screens::ScreenDef> {
        match self.ui_state.screen {
            Screen::Mod(i) => self.content.reg.screens.get(i),
            _ => None,
        }
    }

    /// Click handling for a mod screen (capability E11): toggles flip
    /// their player-KV key client-side; buttons dispatch the mod's
    /// `on_screen_click` hook — directly solo, via the host for a guest.
    pub(in crate::game) fn mod_screen_click(&mut self) {
        // Scope the registry borrow: everything needed from the def is
        // cloned before any `&mut self` work below.
        let open = self
            .mod_screen_def()
            .map(|d| (d.id.clone(), d.rows().to_vec()));
        let Some((id, rows)) = open else {
            return;
        };
        for (i, row) in rows.iter().enumerate() {
            if !self.hit(self.mod_screen_row_rect(i)) {
                continue;
            }
            match row {
                crate::screens::ScreenWidget::Toggle { key, .. } => {
                    let next = if self.read_player_kv(key).as_deref() == Some("1") {
                        "0"
                    } else {
                        "1"
                    };
                    self.write_player_kv(key, next.to_string());
                    self.sfx(Sfx::Click);
                }
                crate::screens::ScreenWidget::Button { action, .. } => {
                    self.sfx(Sfx::Click);
                    if let Some(rc) = &self.multiplayer.remote {
                        // The host validates both ids against its own
                        // registry before dispatching anything.
                        rc.session.send(&net::C2S::ScreenClick {
                            screen: id.clone(),
                            action: action.clone(),
                        });
                    } else if self.content.scripts.wants("on_screen_click") {
                        // Rhai's FuncArgs wants 'static: hand over owned
                        // clones rather than borrows.
                        let sid = id.clone();
                        let act = action.clone();
                        self.content.scripts.dispatch_view(
                            &self.runtime.view(),
                            "on_screen_click",
                            (sid, act),
                        );
                        self.apply_script_cmds();
                    }
                }
                _ => {}
            }
            return;
        }
        for i in 0..TOTAL_SLOTS {
            if self.hit(self.inventory_layout().slot_rect(i)) {
                self.inventory_click(false, i, false);
                return;
            }
        }
    }
}
