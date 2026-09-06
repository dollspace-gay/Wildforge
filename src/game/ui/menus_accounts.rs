//! Menus accounts layout and UI composition.

use crate::game::Game;
use crate::game::widgets;
use crate::identity;
use crate::net;
use crate::ui::UiBatch;

impl Game {
    pub(in crate::game) fn draw_accounts_screen(&mut self, ui: &mut UiBatch, w: f32, h: f32) {
        ui.rect(0.0, 0.0, w, h, [0.02, 0.05, 0.1, 0.82]);
        let title = if self.config.profile_complete {
            "ACCOUNTS"
        } else {
            "CREATE LOCAL PROFILE"
        };
        let tw = UiBatch::text_width(4.0, title);
        ui.text_shadow((w - tw) / 2.0, h * 0.07, 4.0, title, [1.0; 4]);
        ui.text_shadow(
            w / 2.0 - 310.0,
            h * 0.14,
            1.5,
            "LOCAL PLAY NEVER REQUIRES AN ONLINE ACCOUNT. NAMES ARE LABELS, NOT SAVE KEYS.",
            [0.75, 0.85, 0.75, 1.0],
        );
        for (row, label, value) in [
            (
                0usize,
                "WILDFORGE NAME",
                self.ui_state.account_name.as_str(),
            ),
            (
                1usize,
                "ATPROTO HANDLE OR DID",
                self.ui_state.account_handle.as_str(),
            ),
        ] {
            let r = self.account_field_rect(row);
            ui.text_shadow(w / 2.0 - 310.0, r.1 + 8.0, 1.5, label, [1.0; 4]);
            ui.rect(r.0, r.1, r.2, r.3, [0.08, 0.08, 0.08, 0.98]);
            ui.text_shadow(r.0 + 8.0, r.1 + 8.0, 2.0, &value.to_uppercase(), [1.0; 4]);
            if self.ui_state.account_focus as usize == row {
                ui.rect(r.0, r.1 + r.3 - 3.0, r.2, 3.0, [0.6, 1.0, 0.6, 1.0]);
            }
        }
        let device = format!(
            "LOCAL PROFILE: {}  /  DEVICE {}",
            self.config.display_name,
            self.identity.device_id().short()
        );
        ui.text_shadow(w / 2.0 - 310.0, h * 0.35, 1.5, &device, [0.7; 4]);
        let linked = self.atproto_account.as_ref();
        let labels = [
            "SAVE LOCAL NAME".to_string(),
            if linked.is_some() {
                "REFRESH / RELINK ATPROTO"
            } else {
                "LINK ATPROTO"
            }
            .to_string(),
            format!(
                "SOCIAL DISPLAY NAME: {}",
                if linked.is_some_and(|a| a.use_social_display_name) {
                    "ON"
                } else {
                    "OFF"
                }
            ),
            format!(
                "SOCIAL AVATAR: {}",
                if linked.is_some_and(|a| a.use_social_avatar) {
                    "ON"
                } else {
                    "OFF"
                }
            ),
            format!(
                "SHARE ATPROTO HANDLE: {}",
                if linked.is_some_and(|a| a.share_social_handle) {
                    "ON"
                } else {
                    "OFF"
                }
            ),
            "REVOKE THIS DEVICE".to_string(),
            "UNLINK LOCALLY".to_string(),
            if self.config.profile_complete {
                "BACK"
            } else {
                "SAVE A NAME TO CONTINUE"
            }
            .to_string(),
        ];
        for (i, label) in labels.iter().enumerate() {
            let r = self.account_button_rect(i);
            widgets::button(&mut *ui, r, label, self.hit(r));
        }
        if let Some(account) = linked {
            let (active_name, social_name) = self.selected_multiplayer_name();
            let join_line = format!(
                "MULTIPLAYER NAME: {active_name}  ({})",
                if social_name {
                    "ATPROTO PROFILE"
                } else {
                    "LOCAL PROFILE"
                }
            );
            ui.text_shadow(
                w / 2.0 - 310.0,
                h * 0.77,
                1.5,
                &join_line,
                [1.0, 0.95, 0.72, 1.0],
            );
            let profile_line = format!(
                "ATPROTO PROFILE: {}{}",
                account
                    .profile_display_name
                    .as_deref()
                    .unwrap_or("NO DISPLAY NAME PUBLISHED"),
                account
                    .handle
                    .as_deref()
                    .map(|handle| format!("  (@{handle})"))
                    .unwrap_or_default()
            );
            ui.text_shadow(
                w / 2.0 - 310.0,
                h * 0.81,
                1.25,
                &profile_line.to_uppercase(),
                [0.6, 1.0, 0.7, 1.0],
            );
            let disclosure = if account.share_social_handle {
                "OTHER PLAYERS SEE YOUR VERIFIED BADGE AND PUBLIC HANDLE"
            } else {
                "OTHER PLAYERS SEE ONLY A VERIFIED BADGE"
            };
            let did_line = format!("LINKED ID: {}  ({disclosure})", account.did.short());
            ui.text_shadow(
                w / 2.0 - 310.0,
                h * 0.85,
                1.25,
                &did_line.to_uppercase(),
                [0.6, 1.0, 0.7, 1.0],
            );
        }
        if !self.ui_state.account_status.is_empty() {
            ui.text_shadow(
                w / 2.0 - 310.0,
                h * 0.91,
                1.25,
                &self.ui_state.account_status,
                [1.0, 0.75, 0.5, 1.0],
            );
        }
        ui.text_shadow(
            w / 2.0 - 310.0,
            h * 0.95,
            1.0,
            "LINKING WRITES A PUBLIC DEVICE RECORD. A VERIFIED SERVER CAN RESOLVE YOUR DID AND PUBLIC PROFILE.",
            [0.65, 0.65, 0.65, 1.0],
        );
    }
    pub(in crate::game) fn draw_moderation_screen(
        &mut self,
        ui: &mut UiBatch,
        w: f32,
        h: f32,
        id: u32,
    ) {
        ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.78]);
        let title = "PLAYER MODERATION";
        let tw = UiBatch::text_width(4.0, title);
        ui.text_shadow((w - tw) / 2.0, h * 0.08, 4.0, title, [1.0; 4]);
        let summary = self
            .multiplayer
            .host
            .as_ref()
            .and_then(|host| host.guest_identity_summary(id))
            .or_else(|| {
                let remote = self.multiplayer.remote.as_ref()?;
                remote.session.roster().get(&id).map(|presence| {
                    format!(
                        "{} | YOUR ROLE {:?}",
                        crate::game::remote::presence_label(presence),
                        remote.role
                    )
                })
            })
            .unwrap_or_else(|| "PLAYER DISCONNECTED".into());
        ui.text_shadow(
            w / 2.0 - 360.0,
            h * 0.18,
            1.5,
            &summary.to_uppercase(),
            [0.8; 4],
        );
        let labels = [
            "KICK",
            "MUTE 10 MINUTES",
            "BAN 1 HOUR",
            "BAN PERMANENTLY",
            "ADD TO ALLOWLIST",
            "CYCLE ROLE",
            "BACK",
        ];
        for (i, label) in labels.iter().enumerate() {
            let r = self.menu_button_rect(i);
            let label = if self.ui_state.moderation_confirm == Some(i as u8) {
                format!("CONFIRM {label}")
            } else {
                (*label).to_string()
            };
            widgets::button(&mut *ui, r, &label, self.hit(r));
        }
    }
    pub(in crate::game) fn draw_join_screen(&mut self, ui: &mut UiBatch, w: f32, h: f32) {
        ui.rect(0.0, 0.0, w, h, [0.02, 0.05, 0.1, 0.75]);
        let tw = UiBatch::text_width(4.0, "JOIN GAME");
        ui.text_shadow((w - tw) / 2.0, h * 0.08, 4.0, "JOIN GAME", [1.0; 4]);
        if let Some(d) = &mut self.multiplayer.discovery {
            d.poll();
        }
        let found: Vec<net::DiscoveredServer> = self
            .multiplayer
            .discovery
            .as_ref()
            .map(|d| d.found.clone())
            .unwrap_or_default();
        if found.is_empty() {
            ui.text_shadow(
                w / 2.0 - 220.0,
                h * 0.20 + 10.0,
                2.0,
                "SEARCHING THE LAN...",
                [0.7, 0.7, 0.7, 1.0],
            );
        }
        for (i, found) in found.iter().take(5).enumerate() {
            let r = (w / 2.0 - 220.0, h * 0.20 + i as f32 * 56.0, 440.0, 42.0);
            widgets::button(
                &mut *ui,
                r,
                &format!("{} - {}", found.name.to_uppercase(), found.addr),
                self.hit(r),
            );
            let policy = match found.identity {
                identity::IdentityPolicy::AtprotoRequired => "VERIFIED ATPROTO REQUIRED",
                identity::IdentityPolicy::AtprotoOptional => "ATPROTO OPTIONAL",
                identity::IdentityPolicy::Local => "LOCAL IDENTITIES ACCEPTED",
            };
            ui.text_shadow(r.0 + 8.0, r.1 + 29.0, 1.0, policy, [0.65, 0.85, 0.65, 1.0]);
        }
        // The searching line occupies one row when the list is
        // empty; the click handler mirrors this formula.
        let y = h * 0.20 + found.len().clamp(1, 5) as f32 * 56.0 + 26.0;
        ui.text_shadow(w / 2.0 - 220.0, y, 2.0, "DIRECT IP:", [1.0; 4]);
        ui.rect(w / 2.0 - 80.0, y - 6.0, 300.0, 34.0, [0.1, 0.1, 0.1, 0.95]);
        let shown = if self.multiplayer.join_ip.is_empty() {
            "TYPE ADDRESS"
        } else {
            &self.multiplayer.join_ip
        };
        let col = if self.multiplayer.join_ip.is_empty() {
            [0.5, 0.5, 0.5, 1.0]
        } else {
            [1.0; 4]
        };
        ui.text_shadow(w / 2.0 - 72.0, y, 2.0, &shown.to_uppercase(), col);
        let cr = (w / 2.0 + 240.0, y - 6.0, 160.0, 34.0);
        widgets::button(&mut *ui, cr, "CONNECT", self.hit(cr));
        if !self.multiplayer.join_status.is_empty() {
            ui.text_shadow(
                w / 2.0 - 220.0,
                y + 46.0,
                2.0,
                &self.multiplayer.join_status,
                [1.0, 0.6, 0.5, 1.0],
            );
        }
        let br = self.pack_back_rect();
        widgets::button(&mut *ui, br, "BACK", self.hit(br));
    }
}
