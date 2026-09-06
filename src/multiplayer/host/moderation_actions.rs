//! Moderation actions for the authoritative host session.

use super::{
    BanIdentity, HostSession, ModerationAction, ModerationStore, PlayerRuntime, Principal, Refusal,
    RefusalCode, Role, S2C, moderation_action_allowed,
};

impl HostSession {
    /// Kick a guest and refuse them for the rest of the session.
    pub fn kick_guest(&mut self, id: u32) -> Option<String> {
        let g = self.guests.remove(&id)?;
        if let Some(profiles) = &self.profiles
            && let Err(e) = profiles.save(&PlayerRuntime::from_guest(&g), profiles.registry_hint())
        {
            eprintln!("profiles: save {} failed after kick: {e}", g.name);
        }
        self.banned.extend(g.principals.iter().cloned());
        self.net.send(
            id,
            &S2C::Refused(Refusal::new(RefusalCode::Kicked, "kicked by host")),
        );
        self.broadcast_ready(&S2C::Left { id });
        self.net.kick(id);
        Some(g.name)
    }

    /// Persistently ban a connected profile and each credential currently
    /// attached to it, then disconnect it.
    pub fn ban_guest(
        &mut self,
        id: u32,
        reason: &str,
        duration_secs: Option<u64>,
        created_by: &str,
    ) -> std::io::Result<Option<String>> {
        let Some(g) = self.guests.remove(&id) else {
            return Ok(None);
        };
        if let Some(profiles) = &self.profiles {
            profiles.save(&PlayerRuntime::from_guest(&g), profiles.registry_hint())?;
        }
        let moderation = self
            .moderation
            .as_mut()
            .ok_or_else(|| std::io::Error::other("moderation store is not initialized"))?;
        moderation.ban(
            BanIdentity {
                player_id: g.player_id,
                principals: &g.principals,
                display_name: &g.name,
                handle: g.verified_handle.as_deref(),
            },
            reason,
            created_by,
            duration_secs,
        )?;
        self.net
            .send(id, &S2C::Refused(Refusal::new(RefusalCode::Banned, reason)));
        self.broadcast_ready(&S2C::Left { id });
        self.net.kick(id);
        Ok(Some(g.name))
    }

    pub fn allow_guest(&mut self, id: u32, by: &str) -> std::io::Result<bool> {
        let Some(g) = self.guests.get(&id) else {
            return Ok(false);
        };
        let Some(moderation) = self.moderation.as_mut() else {
            return Ok(false);
        };
        for principal in &g.principals {
            moderation.allow_principal(principal.clone(), by)?;
        }
        moderation.allow_player(g.player_id, by)?;
        Ok(true)
    }

    pub fn set_guest_role(&mut self, id: u32, role: Role, by: &str) -> std::io::Result<bool> {
        let Some(g) = self.guests.get(&id) else {
            return Ok(false);
        };
        let Some(moderation) = self.moderation.as_mut() else {
            return Ok(false);
        };
        moderation.set_role(g.principal.clone(), role, by)?;
        self.net.send(id, &S2C::RoleChanged { role });
        Ok(true)
    }

    pub fn mute_guest(
        &mut self,
        id: u32,
        reason: &str,
        duration_secs: Option<u64>,
        by: &str,
    ) -> std::io::Result<bool> {
        let Some(g) = self.guests.get(&id) else {
            return Ok(false);
        };
        let Some(moderation) = self.moderation.as_mut() else {
            return Ok(false);
        };
        moderation.mute(g.principal.clone(), reason, by, duration_secs)?;
        Ok(true)
    }

    pub fn guest_identity_summary(&self, id: u32) -> Option<String> {
        let guest = self.guests.get(&id)?;
        let role = self
            .moderation
            .as_ref()
            .map(|store| store.role(&guest.principal))
            .unwrap_or_default();
        Some(format!(
            "{} | player {} | {} | role {:?}",
            guest.name,
            guest.player_id,
            match &guest.principal {
                Principal::LocalDevice(device) => format!("device {}", device.short()),
                Principal::Atproto(did) => match &guest.verified_handle {
                    Some(handle) => format!(
                        "@{handle} / {}{}",
                        did.short(),
                        if guest.verification_cached {
                            " (cached proof)"
                        } else {
                            ""
                        }
                    ),
                    None => format!(
                        "ATProto {}{}",
                        did.short(),
                        if guest.verification_cached {
                            " (cached proof)"
                        } else {
                            ""
                        }
                    ),
                },
            },
            role
        ))
    }

    pub fn guest_role(&self, id: u32) -> Option<Role> {
        let guest = self.guests.get(&id)?;
        Some(
            self.moderation
                .as_ref()
                .map(|store| store.role(&guest.principal))
                .unwrap_or_default(),
        )
    }

    pub fn unban_player(
        &mut self,
        player_id: crate::identity::PlayerId,
        by: &str,
    ) -> std::io::Result<bool> {
        if self.moderation.is_none() {
            self.moderation = Some(ModerationStore::load(
                &std::path::PathBuf::from("saves").join(&self.world_name),
            )?);
        }
        let moderation = self
            .moderation
            .as_mut()
            .ok_or_else(|| std::io::Error::other("moderation store is not initialized"))?;
        moderation.unban_player(player_id, by)
    }

    pub(super) fn on_moderation_request(
        &mut self,
        actor: u32,
        target: u32,
        action: ModerationAction,
    ) {
        let Some(actor_guest) = self.guests.get(&actor) else {
            return;
        };
        if actor == target || target == 0 || !self.guests.contains_key(&target) {
            self.net.send(
                actor,
                &S2C::Toast("That player cannot be moderated from this session.".into()),
            );
            return;
        }
        let role = self
            .moderation
            .as_ref()
            .map(|store| store.role(&actor_guest.principal))
            .unwrap_or_default();
        if !moderation_action_allowed(role, action) {
            self.net.send(
                actor,
                &S2C::Toast("Your server role does not permit that action.".into()),
            );
            return;
        }

        let by = format!(
            "remote:{}:{}",
            actor_guest.name,
            actor_guest.principal.storage_key()
        );
        let result: std::io::Result<Option<String>> = match action {
            ModerationAction::Kick => {
                Ok(self.kick_guest(target).map(|name| format!("{name} kicked")))
            }
            ModerationAction::Mute { seconds } => self
                .mute_guest(target, "remote moderator mute", Some(seconds), &by)
                .map(|changed| changed.then_some(format!("player muted for {seconds} seconds"))),
            ModerationAction::Ban { seconds } => self
                .ban_guest(target, "remote moderator ban", seconds, &by)
                .map(|name| {
                    name.map(|name| match seconds {
                        Some(seconds) => format!("{name} banned for {seconds} seconds"),
                        None => format!("{name} permanently banned"),
                    })
                }),
            ModerationAction::Allow => self
                .allow_guest(target, &by)
                .map(|changed| changed.then_some("player added to allowlist".into())),
            ModerationAction::CycleRole => {
                let next = match self.guest_role(target).unwrap_or_default() {
                    Role::Player => Role::Moderator,
                    Role::Moderator | Role::Admin | Role::Owner => Role::Player,
                };
                self.set_guest_role(target, next, &by)
                    .map(|changed| changed.then_some(format!("role set to {next:?}")))
            }
        };
        let message = match result {
            Ok(Some(message)) => message,
            Ok(None) => "Player is no longer connected.".into(),
            Err(error) => format!("Moderation failed: {error}"),
        };
        self.net.send(actor, &S2C::Toast(message));
    }
}
