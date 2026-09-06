//! Join for the authoritative host session.

use super::{
    AuthenticatedJoin, DisplayName, HostFx, HostSession, ModerationStore, PendingGuest,
    ProfileStore, Refusal, RefusalCode, S2C, Server, net, shares_principal,
};

impl HostSession {
    pub(super) fn on_join(
        &mut self,
        server: &mut Server,
        join: AuthenticatedJoin,
        _fx: &mut Vec<HostFx>,
    ) {
        let AuthenticatedJoin {
            id,
            display_name,
            principal,
            principals,
            verification_cached,
            verified_handle,
            public_handle,
            content_hash,
            style,
        } = join;
        let name = display_name.to_string();
        if self.banned.contains(&principal) {
            self.net.send(
                id,
                &S2C::Refused(Refusal::new(RefusalCode::Banned, "banned by host")),
            );
            self.net.kick(id);
            return;
        }
        if self
            .guests
            .values()
            .any(|guest| shares_principal(&guest.principals, &principals))
            || self
                .pending_guests
                .values()
                .any(|guest| shares_principal(&guest.runtime.principals, &principals))
        {
            self.net.send(
                id,
                &S2C::Refused(Refusal::new(
                    RefusalCode::AlreadyConnected,
                    "this identity is already connected",
                )),
            );
            self.net.kick(id);
            return;
        }
        if self
            .guests
            .values()
            .filter_map(|guest| DisplayName::parse(&guest.name).ok())
            .any(|other| other.collision_key() == display_name.collision_key())
            || self
                .pending_guests
                .values()
                .filter_map(|guest| DisplayName::parse(&guest.name).ok())
                .any(|other| other.collision_key() == display_name.collision_key())
            || self
                .host_name
                .as_deref()
                .and_then(|host| DisplayName::parse(host).ok())
                .is_some_and(|host| host.collision_key() == display_name.collision_key())
        {
            self.net.send(
                id,
                &S2C::Refused(Refusal::new(
                    RefusalCode::NameInUse,
                    "that display name is already in use",
                )),
            );
            self.net.kick(id);
            return;
        }
        let world_root = server.world.save_dir_for_saving();
        if self.profiles.is_none() {
            self.profiles = match ProfileStore::load(world_root.clone()) {
                Ok(store) => Some(store),
                Err(e) => {
                    self.refuse_server_error(id, "player profile store", &e);
                    return;
                }
            };
        }
        if self.moderation.is_none() {
            self.moderation = match ModerationStore::load(&world_root) {
                Ok(store) => Some(store),
                Err(e) => {
                    self.refuse_server_error(id, "moderation store", &e);
                    return;
                }
            };
        }
        if let Err(refusal) = self.moderation.as_mut().unwrap().check_bans(&principals) {
            self.net.send(id, &S2C::Refused(refusal));
            self.net.kick(id);
            return;
        }
        let reg = server.world.reg.clone();
        // A new arrival lands where a player can actually stand: the
        // old default was a fixed point at y=80, which is the sky over
        // some worlds and the seabed under others. Resolved once for
        // the session — every arrival shares the world's doorstep.
        let fresh_spawn = match self.fresh_spawn {
            Some(p) => p,
            None => {
                let Some(p) = server.world.common_spawn() else {
                    self.refuse_server_error(
                        id,
                        "qualified common spawn",
                        &std::io::Error::other(
                            "world entry has not completed its preparation manifest",
                        ),
                    );
                    return;
                };
                self.fresh_spawn = Some(p);
                p
            }
        };
        let runtime = match self.profiles.as_mut().unwrap().open_or_create(
            &principals,
            &display_name,
            style,
            fresh_spawn,
            &reg,
        ) {
            Ok(runtime) => runtime,
            Err(e) => {
                let code = if e.kind() == std::io::ErrorKind::AlreadyExists {
                    RefusalCode::ProfileConflict
                } else {
                    RefusalCode::Server
                };
                self.net.send(
                    id,
                    &S2C::Refused(Refusal::new(
                        code,
                        format!("player profile could not be opened: {e}"),
                    )),
                );
                self.net.kick(id);
                return;
            }
        };
        if let Err(refusal) = self.moderation.as_mut().unwrap().admit(
            &runtime.principals,
            Some(runtime.player_id),
            self.admission_policy,
        ) {
            self.net.send(id, &S2C::Refused(refusal));
            self.net.kick(id);
            return;
        }
        if content_hash != self.content_hash {
            // Stream the mods dir so the guest can match us exactly.
            let files = net::collect_mod_files(std::path::Path::new("mods"));
            self.net.send(id, &S2C::ModFiles(files));
        }
        let required = crate::world::player_entry_chunks(runtime.pos.surface());
        self.pending_guests.insert(
            id,
            PendingGuest {
                name,
                principal,
                verification_cached,
                verified_handle,
                public_handle,
                runtime,
                required,
                progress_age: 0.0,
            },
        );
    }
}
