//! Connection graphical guest adapter.

use crate::client_session::ContentMap;
use crate::client_session::GuestSession;
use crate::client_session::PresentationRequirement;
use crate::game::Game;
use crate::game::Remote;
use crate::identity;
use crate::inventory::HOTBAR_SLOTS;
use crate::net;
use crate::physics::Player;
use glam::Vec3;
use std::sync::Arc;

impl Game {
    /// The name this client will present to a multiplayer host, plus whether
    /// it came from the explicitly enabled ATProto profile preference.
    pub(in crate::game) fn selected_multiplayer_name(&self) -> (String, bool) {
        if let Some(name) = self
            .atproto_account
            .as_ref()
            .filter(|account| account.use_social_display_name)
            .and_then(|account| account.profile_display_name.as_deref())
            .and_then(|name| identity::DisplayName::parse(name).ok())
        {
            return (name.to_string(), true);
        }
        (self.config.display_name.clone(), false)
    }

    pub(in crate::game) fn apply_remote_player_state(
        &mut self,
        content: &ContentMap,
        state: net::PlayerStateSnap,
        initial: bool,
    ) {
        if initial {
            self.player = Player::new_at(state.pos);
            // Dev: WILDFORGE_POS frames multiplayer captures too —
            // movement is client-stated, so the host accepts it.
            if let Ok(s) = std::env::var("WILDFORGE_POS") {
                let p: Vec<f32> = s.split(',').filter_map(|v| v.trim().parse().ok()).collect();
                let face = std::env::var("WILDFORGE_FACE")
                    .ok()
                    .as_deref()
                    .and_then(crate::planet::Face::from_name)
                    .unwrap_or(state.pos.face());
                if p.len() == 3
                    && let Ok(pos) =
                        crate::planet::EntityPos::from_local(face, Vec3::new(p[0], p[1], p[2]))
                {
                    self.player = Player::new_at(pos);
                }
            }
            self.camera.follow_planet(self.player.eye());
            self.camera.yaw = state.yaw;
            self.camera.pitch = state.pitch;
        }
        self.survival.spawn_point = state.spawn;
        self.inventory.slots = content.slots(&state.inventory);
        self.survival.armor = content.slots(&state.armor);
        self.ui_state.held_stack = state.cursor.as_ref().and_then(|stack| content.stack(stack));
        self.survival.health = state.health;
        self.survival.hunger = state.hunger;
        self.survival.nutrition = state.nutrition;
        self.input.hotbar_sel = (state.hotbar as usize).min(HOTBAR_SLOTS - 1);
    }

    /// Join a host: blocks briefly for the QUIC handshake, then the
    /// Welcome/ModFiles flow finishes in remote_pump.
    pub(in crate::game) fn request_join(
        &mut self,
        addr: std::net::SocketAddr,
        advertised_policy: Option<identity::IdentityPolicy>,
    ) {
        let could_disclose_atproto = self.atproto_account.is_some()
            && advertised_policy != Some(identity::IdentityPolicy::Local);
        if could_disclose_atproto && self.multiplayer.pending_join_disclosure != Some(addr) {
            self.multiplayer.pending_join_disclosure = Some(addr);
            self.multiplayer.join_status =
                "SERVER MAY RESOLVE YOUR PUBLIC DID - CLICK AGAIN".into();
            return;
        }
        self.multiplayer.pending_join_disclosure = None;
        self.multiplayer.join_status = "CONNECTING...".into();
        self.join_server(addr);
    }

    pub(in crate::game) fn join_server(&mut self, addr: std::net::SocketAddr) {
        if let Err(error) = self.content.reg.validate() {
            eprintln!("join: {error}");
            self.multiplayer.join_status = format!("FAILED: {error}").to_uppercase();
            return;
        }
        let (name, _) = self.selected_multiplayer_name();
        let hash = net::content_hash(std::path::Path::new("mods"));
        match net::Client::connect(
            addr,
            name,
            hash,
            self.style.pack(),
            &self.identity,
            self.atproto_account.as_ref(),
        ) {
            Ok(client) => {
                let policy = match client.identity_policy {
                    identity::IdentityPolicy::AtprotoRequired => {
                        "VERIFIED ATPROTO REQUIRED; SERVER CAN RESOLVE YOUR PUBLIC PROFILE"
                    }
                    identity::IdentityPolicy::AtprotoOptional => {
                        "LOCAL OR ATPROTO IDENTITY ACCEPTED"
                    }
                    identity::IdentityPolicy::Local => "LOCAL IDENTITIES ACCEPTED",
                };
                let admission = match client.admission_policy {
                    identity::AdmissionPolicy::Open => "OPEN ADMISSION",
                    identity::AdmissionPolicy::Allowlist => "ALLOWLIST ONLY",
                };
                self.multiplayer.remote = Some(Remote {
                    my_id: 0,
                    role: identity::Role::Player,
                    session: GuestSession::with_client(
                        Arc::clone(&self.content.reg),
                        PresentationRequirement::FirstFrame,
                        std::time::Instant::now(),
                        client,
                    ),
                    players: Default::default(),
                    player_positions: Default::default(),
                    player_held: Default::default(),
                    player_implement: Default::default(),
                    player_style: Default::default(),
                    sleeping: false,
                    player_lerp: Default::default(),
                    player_age: 0.0,
                    player_interval: 0.05,
                    mob_lerp: Default::default(),
                    mob_age: 0.0,
                    mob_interval: 0.05,
                    // Until the host answers, assume the old fixed ring.
                    granted_view_dist: 5,
                    asked_view_dist: 0,
                    wants: Default::default(),
                });
                self.multiplayer.join_status = format!("{policy} - {admission} - SYNCING...");
            }
            Err(e) => {
                self.multiplayer.join_status = format!("FAILED: {e}").to_uppercase();
            }
        }
    }
}
