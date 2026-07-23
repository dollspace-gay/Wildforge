//! Host-side multiplayer session: applies guest requests through the
//! same authoritative code paths local play uses, and broadcasts the
//! world back. Shared by the windowed host (pause menu → OPEN TO
//! FRIENDS) and the headless `--server` — one code path.

#[path = "moderation.rs"]
mod moderation;
#[path = "profiles.rs"]
mod profiles;
#[path = "roster.rs"]
mod roster;
#[path = "settings.rs"]
mod settings;

use std::collections::{HashMap, HashSet};

use glam::Vec3;

use crate::chunk::ChunkPos;
pub use crate::identity::Role;
use crate::identity::{AdmissionPolicy, DisplayName, IdentityPolicy, Principal};
use crate::inventory::{HOTBAR_SLOTS, ItemStack, click_stack};
use crate::net::{
    self, C2S, HostEvent, MobSnap, ModerationAction, Refusal, RefusalCode, S2C, StackSnap,
};
use crate::server::Server;
use crate::world::{BlockEntity, World};
use moderation::{BanIdentity, ModerationStore};
use profiles::{PlayerRuntime, ProfileStore};
pub use settings::ServerSettings;

pub struct Guest {
    pub player_id: crate::identity::PlayerId,
    pub principals: Vec<Principal>,
    pub previous_names: Vec<String>,
    pub first_seen: u64,
    pub name: String,
    pub principal: Principal,
    pub verification_cached: bool,
    pub verified_handle: Option<String>,
    /// Handle safe to include in the public roster because the player opted in.
    pub public_handle: Option<String>,
    pub pos: Vec3,
    pub yaw: f32,
    pub container: Option<(i32, i32, i32)>,
    pub sleeping: bool,
    /// Wire item id in hand (u16::MAX = empty), from Move packets.
    pub held: u16,
    /// Packed appearance Style, from the Hello.
    pub style: u32,
    pub inventory: crate::inventory::Inventory,
    pub armor: [Option<ItemStack>; 5],
    pub health: f32,
    pub hunger: f32,
    pub nutrition: [f32; 5],
    pub spawn: Vec3,
    pub pitch: f32,
    pub hotbar: usize,
    pub cursor: Option<ItemStack>,
    pub craft_grid: [Option<ItemStack>; 9],
    has_moved: bool,
    sprinting: bool,
    action_cooldown: f32,
    since_damage: f32,
    regen_timer: f32,
    starve_timer: f32,
    chat_count: u8,
    chat_window: f32,
    command_count: u16,
    command_window: f32,
    airborne_rise: f32,
    sent_chunks: HashSet<(i32, i32)>,
    edits: u32,
    edit_window: f32,
    /// Movement packets arrive at ~20 Hz; rendering interpolates from
    /// here toward (pos, yaw) so guests glide instead of stutter.
    render_from: (Vec3, f32),
    net_age: f32,
    net_interval: f32,
}

impl Guest {
    /// Position/yaw to draw this guest at (the sim uses the latest).
    pub fn render_pos(&self) -> (Vec3, f32) {
        let t = (self.net_age / self.net_interval.max(0.001)).clamp(0.0, 1.0);
        (
            self.render_from.0.lerp(self.pos, t),
            crate::mobs::lerp_yaw(self.render_from.1, self.yaw, t),
        )
    }

    pub fn public_label(&self) -> String {
        roster::public_label(self)
    }
}

/// Presentation the host player should see.
pub enum HostFx {
    Chat {
        from: String,
        msg: String,
    },
    Joined(String),
    Left(String),
    /// Everyone slept: the host's own dawn side-effects should run.
    AllSlept,
}

pub struct HostSession {
    pub net: net::Host,
    pub guests: HashMap<u32, Guest>,
    pub content_hash: u64,
    pub world_name: String,
    pub identity_policy: IdentityPolicy,
    pub admission_policy: AdmissionPolicy,
    host_name: Option<String>,
    profiles: Option<ProfileStore>,
    moderation: Option<ModerationStore>,
    /// Principals kicked this session: refused if they reconnect.
    banned: HashSet<Principal>,
    snapshot_timer: f32,
    state_timer: f32,
    container_timer: f32,
}

struct AuthenticatedJoin {
    id: u32,
    display_name: DisplayName,
    principal: Principal,
    principals: Vec<Principal>,
    verification_cached: bool,
    verified_handle: Option<String>,
    public_handle: Option<String>,
    content_hash: u64,
    style: u32,
}

const REACH: f32 = 7.0;
const EDITS_PER_SEC: u32 = 10;

fn shares_principal(left: &[Principal], right: &[Principal]) -> bool {
    roster::shares_principal(left, right)
}

fn moderation_action_allowed(role: Role, action: ModerationAction) -> bool {
    role.can_moderate()
        && match action {
            ModerationAction::Kick => true,
            ModerationAction::Mute { seconds } => (1..=86_400).contains(&seconds),
            ModerationAction::Ban {
                seconds: Some(seconds),
            } => (1..=86_400).contains(&seconds),
            ModerationAction::Ban { seconds: None }
            | ModerationAction::Allow
            | ModerationAction::CycleRole => role.can_administer(),
        }
}

impl HostSession {
    pub fn start(world_name: String) -> std::io::Result<HostSession> {
        Self::start_configured(world_name, None)
    }

    fn start_configured(
        world_name: String,
        host_name: Option<DisplayName>,
    ) -> std::io::Result<HostSession> {
        let settings =
            ServerSettings::load_or_create(&std::path::PathBuf::from("saves").join(&world_name))?;
        Self::start_on_with_settings(
            world_name,
            settings.port,
            host_name,
            settings.identity,
            settings.admission,
            settings.verification_grace_secs,
        )
    }

    pub fn start_windowed(
        world_name: String,
        host_name: DisplayName,
    ) -> std::io::Result<HostSession> {
        Self::start_configured(world_name, Some(host_name))
    }

    /// Tests and second-hosts bind an OS-assigned port with 0.
    #[cfg(test)]
    pub fn start_on(world_name: String, port: u16) -> std::io::Result<HostSession> {
        Self::start_on_with_policy(
            world_name,
            port,
            None,
            IdentityPolicy::Local,
            AdmissionPolicy::Open,
        )
    }

    #[cfg(test)]
    pub fn start_on_with_policy(
        world_name: String,
        port: u16,
        host_name: Option<DisplayName>,
        identity_policy: IdentityPolicy,
        admission_policy: AdmissionPolicy,
    ) -> std::io::Result<HostSession> {
        Self::start_on_with_settings(
            world_name,
            port,
            host_name,
            identity_policy,
            admission_policy,
            3_600,
        )
    }

    fn start_on_with_settings(
        world_name: String,
        port: u16,
        host_name: Option<DisplayName>,
        identity_policy: IdentityPolicy,
        admission_policy: AdmissionPolicy,
        verification_grace_secs: u64,
    ) -> std::io::Result<HostSession> {
        Ok(HostSession {
            net: net::Host::start(
                world_name.clone(),
                port,
                identity_policy,
                admission_policy,
                verification_grace_secs,
            )?,
            guests: HashMap::new(),
            content_hash: net::content_hash(std::path::Path::new("mods")),
            world_name,
            identity_policy,
            admission_policy,
            host_name: host_name.map(|name| name.to_string()),
            profiles: None,
            moderation: None,
            banned: HashSet::new(),
            snapshot_timer: 0.0,
            state_timer: 0.0,
            container_timer: 0.0,
        })
    }

    /// PlayerCtx list for the simulation: host (when windowed) + guests.
    pub fn player_ctxs(
        &self,
        host: Option<crate::server::PlayerCtx>,
    ) -> Vec<crate::server::PlayerCtx> {
        let mut out = Vec::new();
        if let Some(h) = host {
            out.push(h);
        }
        for g in self.guests.values() {
            out.push(crate::server::PlayerCtx {
                pos: g.pos,
                spawn: g.pos,
                attackable: true,
                aggro_mod: 0.0,
            });
        }
        out
    }

    /// Everything the host does per frame: drain guest messages, apply
    /// them authoritatively, stream state back.
    /// `host`: (pos, yaw, sleeping) for a windowed host; None when
    /// running headless (`--server`).
    pub fn pump(
        &mut self,
        server: &mut Server,
        host: Option<(Vec3, f32, bool, u16, u32)>,
        dt: f32,
    ) -> Vec<HostFx> {
        let host_pos = host
            .map(|(p, _, _, _, _)| p)
            .unwrap_or(Vec3::new(0.5, 80.0, 0.5));
        let host_yaw = host.map(|(_, y, _, _, _)| y).unwrap_or(0.0);
        let host_sleeping = host.map(|(_, _, s, _, _)| s).unwrap_or(false);
        let host_held = host.map(|(_, _, _, h, _)| h).unwrap_or(u16::MAX);
        let host_style = host
            .map(|(_, _, _, _, st)| st)
            .unwrap_or(crate::style::Style::default().pack());
        let mut fx = Vec::new();
        for ev in self.net.poll() {
            match ev {
                HostEvent::Joined {
                    id,
                    display_name,
                    principal,
                    principals,
                    verification_cached,
                    verified_handle,
                    public_handle,
                    content_hash,
                    style,
                } => {
                    self.on_join(
                        server,
                        AuthenticatedJoin {
                            id,
                            display_name,
                            principal,
                            principals,
                            verification_cached,
                            verified_handle,
                            public_handle,
                            content_hash,
                            style,
                        },
                        &mut fx,
                    );
                }
                HostEvent::Left { id } => {
                    if let Some(g) = self.guests.remove(&id) {
                        if let Some(profiles) = &self.profiles
                            && let Err(e) =
                                profiles.save(&PlayerRuntime::from_guest(&g), &server.world.reg)
                        {
                            eprintln!("profiles: save {} failed: {e}", g.name);
                        }
                        self.net.broadcast(&S2C::Left { id });
                        fx.push(HostFx::Left(g.name));
                    }
                }
                HostEvent::Msg { id, msg } => {
                    self.on_msg(server, id, msg, &mut fx);
                }
            }
        }

        // Rate-limit windows + movement interpolation clocks.
        let creative = server.world.mode == "creative";
        let mut survival_changed = Vec::new();
        for g in self.guests.values_mut() {
            g.edit_window += dt;
            if g.edit_window >= 1.0 {
                g.edit_window = 0.0;
                g.edits = 0;
            }
            g.net_age += dt;
            g.action_cooldown = (g.action_cooldown - dt).max(0.0);
            g.since_damage += dt;
            g.chat_window += dt;
            if g.chat_window >= 10.0 {
                g.chat_window = 0.0;
                g.chat_count = 0;
            }
            g.command_window += dt;
            if g.command_window >= 1.0 {
                g.command_window = 0.0;
                g.command_count = 0;
            }
            if !creative {
                let old = (g.health, g.hunger, g.nutrition);
                let drain = 0.01 + if g.sprinting { 0.02 } else { 0.0 };
                g.hunger = (g.hunger - drain * dt).max(0.0);
                for value in &mut g.nutrition {
                    *value = (*value - dt * 0.01).max(0.0);
                }
                let max_health =
                    14.0 + g.nutrition.iter().filter(|&&value| value >= 40.0).count() as f32 * 2.0;
                g.health = g.health.min(max_health);
                if g.hunger >= 17.0 && g.health < max_health && g.since_damage > 4.0 {
                    g.regen_timer += dt;
                    if g.regen_timer >= 3.0 {
                        g.regen_timer = 0.0;
                        g.health = (g.health + 1.0).min(max_health);
                        g.hunger = (g.hunger - 0.5).max(0.0);
                    }
                }
                if g.hunger <= 0.0 {
                    g.starve_timer += dt;
                    if g.starve_timer >= 4.0 {
                        g.starve_timer = 0.0;
                        if g.health > 2.0 {
                            g.health -= 1.0;
                        }
                    }
                } else {
                    g.starve_timer = 0.0;
                }
                if old != (g.health, g.hunger, g.nutrition) {
                    survival_changed.push(g.player_id);
                }
            }
        }
        if self.state_timer + dt >= 1.0 && !survival_changed.is_empty() {
            let ids: Vec<u32> = self.guests.keys().copied().collect();
            for id in ids {
                self.send_player_state(id);
            }
        }

        // Authoritative block edits out.
        if !server.world.edits().is_empty() {
            for (x, y, z, b, meta) in server.world.take_edits() {
                self.net.broadcast(&S2C::BlockSet {
                    x,
                    y,
                    z,
                    id: b.0,
                    meta,
                });
            }
        }
        // Items owed to guests (arrow recovery, mining, mob drops,
        // brush finds) — full stacks so durability crosses the wire.
        for (owner, s) in server.world.take_pending_gives() {
            if let Some(guest) = self.guests.get_mut(&owner) {
                let _ = guest.inventory.add_stack(&server.world.reg, s);
            }
            self.net.send(
                owner,
                &S2C::Give {
                    item: s.item.0,
                    count: s.count,
                    durability: s.durability,
                },
            );
            self.send_player_state(owner);
        }

        // Chunk streaming toward each guest.
        let guest_ids: Vec<u32> = self.guests.keys().copied().collect();
        for id in guest_ids {
            let (gpos, mut needed) = {
                let g = &self.guests[&id];
                (g.pos, Vec::new())
            };
            let center = ChunkPos::of_world(gpos.x as i32, gpos.z as i32);
            'scan: for r in 0..5i32 {
                for dx in -r..=r {
                    for dz in -r..=r {
                        if dx.abs().max(dz.abs()) != r {
                            continue;
                        }
                        let cp = (center.x + dx, center.z + dz);
                        if !self.guests[&id].sent_chunks.contains(&cp) {
                            needed.push(cp);
                            if needed.len() >= 3 {
                                break 'scan;
                            }
                        }
                    }
                }
            }
            for (cx, cz) in needed {
                let cp = ChunkPos { x: cx, z: cz };
                server.world.ensure_chunk(cp);
                if let Some(rle) = server.world.chunk_rle(cp) {
                    self.net.send(id, &S2C::Chunk { x: cx, z: cz, rle });
                }
                if let Some(g) = self.guests.get_mut(&id) {
                    g.sent_chunks.insert((cx, cz));
                }
            }
        }

        // Snapshots: players + mobs + bolts at 20 Hz, time/ire at 1 Hz.
        self.snapshot_timer += dt;
        if self.snapshot_timer >= 0.05 {
            self.snapshot_timer = 0.0;
            let mut players = Vec::new();
            if host.is_some() {
                players.push((0u32, host_pos, host_yaw, host_held, host_style));
            }
            for (id, g) in &self.guests {
                players.push((*id, g.pos, g.yaw, g.held, g.style));
            }
            self.net.broadcast_datagram(&S2C::Players(players));
            let mobs: Vec<MobSnap> = server
                .world
                .mobs()
                .iter()
                .map(|m| MobSnap {
                    id: m.id,
                    species: m.species as u16,
                    pos: m.pos,
                    yaw: m.yaw,
                    growth: m.growth,
                    hurt: m.hurt_flash,
                    fed: m.fed || m.breed_cd > 0.0 || m.growth < 1.0,
                })
                .collect();
            self.net.broadcast_datagram(&S2C::Mobs(mobs));
            let bolts: Vec<net::BoltSnap> = server
                .world
                .projectiles()
                .iter()
                .map(|p| net::BoltSnap {
                    pos: p.pos,
                    vel: p.vel,
                    tile: p.tile,
                    age: p.age,
                })
                .collect();
            self.net.broadcast_datagram(&S2C::Bolts(bolts));
            // Sent even when empty so guests clear their last tumble.
            let falls: Vec<net::FallSnap> = server
                .world
                .falling_blocks()
                .iter()
                .map(|f| net::FallSnap {
                    pos: f.pos,
                    block: f.block.0,
                })
                .collect();
            self.net.broadcast_datagram(&S2C::Falling(falls));
        }
        // Open containers stay live: furnaces smelt and other players
        // shuffle stacks while a guest is looking at them.
        self.container_timer += dt;
        if self.container_timer >= 0.5 {
            self.container_timer = 0.0;
            let open: Vec<(u32, (i32, i32, i32))> = self
                .guests
                .iter()
                .filter_map(|(id, g)| g.container.map(|c| (*id, c)))
                .collect();
            for (id, pos) in open {
                self.send_container(server, id, pos);
            }
        }
        self.state_timer += dt;
        if self.state_timer >= 1.0 {
            self.state_timer = 0.0;
            self.net.broadcast(&S2C::TimeIre {
                time: server.time_of_day,
                ire: server.world.ire,
                day: server.world.day,
                weather: server.world.weather.as_u8(),
            });
        }

        // Sleep vote.
        if host_sleeping || self.guests.values().any(|g| g.sleeping) {
            let present = self.guests.len() as u32 + host.is_some() as u32;
            let sleeping =
                self.guests.values().filter(|g| g.sleeping).count() as u32 + host_sleeping as u32;
            self.net.broadcast(&S2C::Sleep { sleeping, present });
            if sleeping == present {
                let skipped = (1.0 + 0.3 - server.time_of_day) % 1.0;
                if server.world.tick_ire(skipped) {
                    server.world.accept_offerings();
                }
                server.sleep_to_dawn();
                for g in self.guests.values_mut() {
                    g.sleeping = false;
                }
                self.net.broadcast(&S2C::TimeIre {
                    time: server.time_of_day,
                    ire: server.world.ire,
                    day: server.world.day,
                    weather: server.world.weather.as_u8(),
                });
                self.net
                    .broadcast(&S2C::Toast("Dawn. The camp wakes.".into()));
                fx.push(HostFx::AllSlept);
            }
        }
        fx
    }

    fn on_join(&mut self, server: &mut Server, join: AuthenticatedJoin, fx: &mut Vec<HostFx>) {
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
        let mut runtime = match self.profiles.as_mut().unwrap().open_or_create(
            &principals,
            &display_name,
            style,
            Vec3::new(0.5, 80.0, 0.5),
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
        // A saved position goes stale — terrain regenerated under an old
        // world, or someone built over the spot. Free it before the
        // guest materializes inside a hill (mid-air and mid-swim saves
        // pass through untouched).
        runtime.pos = server.world.free_position(runtime.pos);
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
        let palette: Vec<String> = reg.blocks.iter().map(|b| b.name.clone()).collect();
        let items: Vec<String> = reg.items.iter().map(|i| i.name.clone()).collect();
        let mut roster = Vec::new();
        if let Some(host_name) = &self.host_name {
            roster.push(roster::host_presence(host_name));
        }
        roster.extend(
            self.guests
                .iter()
                .map(|(id, guest)| roster::guest_presence(*id, guest)),
        );
        let presence = roster::presence(
            id,
            name.clone(),
            &principal,
            verification_cached,
            public_handle.clone(),
        );
        let role = self
            .moderation
            .as_ref()
            .map(|store| store.role(&principal))
            .unwrap_or_default();
        roster.push(presence.clone());
        self.net.send(
            id,
            &S2C::Welcome {
                seed: server.world.seed,
                mode: server.world.mode.clone(),
                time: server.time_of_day,
                ire: server.world.ire,
                palette,
                items,
                your_id: id,
                your_role: role,
                roster,
                spawn: runtime.pos,
                world_name: self.world_name.clone(),
                player_state: runtime.to_snap(),
            },
        );
        self.net.broadcast(&S2C::Joined { presence });
        self.guests.insert(
            id,
            Guest {
                player_id: runtime.player_id,
                principals: runtime.principals,
                previous_names: runtime.previous_names,
                first_seen: runtime.first_seen,
                name: name.clone(),
                principal,
                verification_cached,
                verified_handle,
                public_handle,
                pos: runtime.pos,
                yaw: runtime.yaw,
                container: None,
                sleeping: false,
                held: runtime.held,
                style: runtime.style,
                inventory: runtime.inventory,
                armor: runtime.armor,
                health: runtime.health,
                hunger: runtime.hunger,
                nutrition: runtime.nutrition,
                spawn: runtime.spawn,
                pitch: runtime.pitch,
                hotbar: runtime.hotbar,
                cursor: runtime.cursor,
                craft_grid: [None; 9],
                has_moved: false,
                sprinting: false,
                action_cooldown: 0.0,
                since_damage: 100.0,
                regen_timer: 0.0,
                starve_timer: 0.0,
                chat_count: 0,
                chat_window: 0.0,
                command_count: 0,
                command_window: 0.0,
                airborne_rise: 0.0,
                sent_chunks: HashSet::new(),
                edits: 0,
                edit_window: 0.0,
                render_from: (Vec3::new(0.5, 80.0, 0.5), 0.0),
                net_age: 0.0,
                net_interval: 0.05,
            },
        );
        fx.push(HostFx::Joined(name));
    }

    fn refuse_server_error(&mut self, id: u32, area: &str, error: &std::io::Error) {
        self.net.send(
            id,
            &S2C::Refused(Refusal::new(
                RefusalCode::Server,
                format!("{area} could not be opened: {error}"),
            )),
        );
        self.net.kick(id);
    }

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
        self.net.broadcast(&S2C::Left { id });
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
        self.net.broadcast(&S2C::Left { id });
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

    /// Apply simulation damage to server-owned survival state. The `Hit`
    /// packet is presentation; the following `PlayerState` is the authority.
    pub fn hurt_guest(&mut self, id: u32, amount: f32, from: Vec3) {
        let Some(guest) = self.guests.get_mut(&id) else {
            return;
        };
        if guest.health <= 0.0 {
            return;
        }
        let armor_points: u32 = guest
            .armor
            .iter()
            .flatten()
            .filter_map(|stack| server_item_armor_points(stack, self.profiles.as_ref()))
            .sum();
        // This mirrors local survival: each point blocks four percent, capped.
        let reduced = amount.max(0.0) * (1.0 - armor_points.min(15) as f32 * 0.04);
        if armor_points > 0
            && let Some(registry) = self.profiles.as_ref().map(ProfileStore::registry_hint)
        {
            for armor in &mut guest.armor {
                if let Some(stack) = armor {
                    if registry.item(stack.item).durability == 0 {
                        continue;
                    }
                    stack.durability = stack.durability.saturating_sub(1);
                    if stack.durability == 0 {
                        *armor = None;
                    }
                }
            }
        }
        guest.health = (guest.health - reduced).max(0.0);
        guest.since_damage = 0.0;
        if guest.health <= 0.0 {
            let _ = guest.inventory.drain();
            guest.armor = [None; 5];
            guest.cursor = None;
            guest.craft_grid = [None; 9];
            refresh_held(guest);
        }
        self.net.send(id, &S2C::Hit { dmg: reduced, from });
        self.send_player_state(id);
    }

    fn on_msg(&mut self, server: &mut Server, id: u32, msg: C2S, fx: &mut Vec<HostFx>) {
        if let C2S::Moderate { target, action } = &msg {
            self.on_moderation_request(id, *target, *action);
            return;
        }
        let Some(guest) = self.guests.get_mut(&id) else {
            return;
        };
        if !matches!(&msg, C2S::Move { .. }) {
            if guest.command_count >= 80 {
                return;
            }
            guest.command_count += 1;
        }
        match msg {
            C2S::Hello { .. } | C2S::Authenticate { .. } | C2S::Moderate { .. } | C2S::Bye => {}
            C2S::Move {
                pos,
                yaw,
                hotbar,
                sprint,
            } => {
                let elapsed = guest.net_age.clamp(0.03, 0.3);
                let delta = pos - guest.pos;
                let horizontal = Vec3::new(delta.x, 0.0, delta.z).length();
                let probe = crate::physics::Player::new(pos);
                let grounded = server.world.reg.is_solid(server.world.get_block(
                    pos.x.floor() as i32,
                    (pos.y - 0.05).floor() as i32,
                    pos.z.floor() as i32,
                ));
                let in_water = server.world.reg.is_water(server.world.get_block(
                    pos.x.floor() as i32,
                    (pos.y + 0.6).floor() as i32,
                    pos.z.floor() as i32,
                ));
                let airborne_rise = if grounded || in_water {
                    0.0
                } else {
                    guest.airborne_rise + delta.y.max(0.0)
                };
                let valid = pos.is_finite()
                    && yaw.is_finite()
                    && hotbar < HOTBAR_SLOTS as u8
                    && !probe.collides(&server.world, pos)
                    && airborne_rise <= 2.4
                    && (guest.has_moved
                        && horizontal <= 8.0 * elapsed + 0.35
                        && delta.y.abs() <= 14.0 * elapsed + 0.75
                        || !guest.has_moved && delta.length() <= 3.0);
                if !valid {
                    self.net.send(
                        id,
                        &S2C::PlayerState(PlayerRuntime::from_guest(guest).to_snap()),
                    );
                    return;
                }
                guest.render_from = guest.render_pos();
                guest.net_interval = guest.net_age.clamp(0.03, 0.3);
                guest.net_age = 0.0;
                guest.pos = pos;
                guest.yaw = yaw;
                guest.hotbar = hotbar as usize;
                guest.sprinting = sprint && guest.hunger >= 6.0;
                guest.airborne_rise = airborne_rise;
                guest.has_moved = true;
                refresh_held(guest);
                // Guests leave footprints too; the edit echoes to all.
                server.world.tread(
                    pos.x.floor() as i32,
                    (pos.y + 0.1).floor() as i32,
                    pos.z.floor() as i32,
                );
            }
            C2S::Break { x, y, z } => {
                let p = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                if (p - guest.pos).length() > REACH || guest.edits >= EDITS_PER_SEC {
                    return;
                }
                guest.edits += 1;
                let creative = server.world.mode == "creative";
                let held = guest.inventory.slots[guest.hotbar].map(|stack| stack.item);
                let sheared = held.is_some_and(|item| server.world.reg.item(item).shears)
                    && server
                        .world
                        .reg
                        .block(server.world.get_block(x, y, z))
                        .name
                        .contains("leaves");
                let Some(result) =
                    server
                        .world
                        .break_block((x, y, z), held, !creative && !sheared, !creative)
                else {
                    return;
                };
                if !creative {
                    guest.hunger = (guest.hunger - 0.008).max(0.0);
                    guest.inventory.wear_tool(&server.world.reg, guest.hotbar);
                }
                refresh_held(guest);
                if let Some(stack) = result.drop {
                    server.world.queue_give(id, stack);
                }
                self.send_player_state(id);
            }
            C2S::Scoop { x, y, z } => {
                let p = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                if (p - guest.pos).length() > REACH || guest.edits >= EDITS_PER_SEC {
                    return;
                }
                // Only a full cell fills a bucket — partials would let
                // a guest mint fluid out of films. Either fluid dips.
                let b = server.world.get_block(x, y, z);
                if server.world.reg.fluid_volume(b) != Some(8) {
                    return;
                }
                let full_name = if server.world.reg.is_lava(b) {
                    "base:bucket_lava"
                } else {
                    "base:bucket_water"
                };
                let Some(empty) = server.world.reg.item_id("base:bucket") else {
                    return;
                };
                if server.world.mode != "creative"
                    && guest.inventory.slots[guest.hotbar].map(|stack| stack.item) != Some(empty)
                {
                    return;
                }
                guest.edits += 1;
                server.world.set_block(x, y, z, crate::registry::AIR);
                if server.world.mode != "creative"
                    && let Some(full) = server.world.reg.item_id(full_name)
                {
                    guest.inventory.slots[guest.hotbar] =
                        Some(ItemStack::new(&server.world.reg, full, 1));
                    refresh_held(guest);
                    self.send_player_state(id);
                }
            }
            C2S::Place { x, y, z } => {
                let p = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                if (p - guest.pos).length() > REACH || guest.edits >= EDITS_PER_SEC {
                    return;
                }
                let selected = guest.inventory.slots[guest.hotbar];
                let creative = server.world.mode == "creative";
                let block = selected
                    .and_then(|stack| server.world.reg.item(stack.item).places)
                    .or_else(|| {
                        let item = selected.map(|stack| stack.item)?;
                        let reg = &server.world.reg;
                        if Some(item) == reg.item_id("base:bucket_water") {
                            Some(reg.water_block(0))
                        } else if Some(item) == reg.item_id("base:bucket_lava") {
                            Some(reg.lava_for_volume(8))
                        } else {
                            None
                        }
                    });
                let Some(block) = block else { return };
                let overlaps = {
                    let player = crate::physics::Player::new(guest.pos);
                    player.overlaps_block(x, y, z)
                };
                if overlaps || !server.world.place_block((x, y, z), block) {
                    return;
                }
                guest.edits += 1;
                if !creative {
                    let held_item = selected.map(|stack| stack.item);
                    let full_bucket = held_item == server.world.reg.item_id("base:bucket_water")
                        || held_item == server.world.reg.item_id("base:bucket_lava");
                    if full_bucket {
                        if let Some(empty) = server.world.reg.item_id("base:bucket") {
                            guest.inventory.slots[guest.hotbar] =
                                Some(ItemStack::new(&server.world.reg, empty, 1));
                        }
                    } else {
                        guest.inventory.take_one(guest.hotbar);
                    }
                    refresh_held(guest);
                    self.send_player_state(id);
                }
            }
            C2S::AttackMob { id: mob_id } => {
                // Stable ids: snapshots lag the sim, so an index would
                // race deaths/spawns and strike the wrong creature.
                if guest.action_cooldown > 0.0 {
                    return;
                }
                guest.action_cooldown = 0.35;
                let dmg = guest.inventory.slots[guest.hotbar]
                    .map(|stack| server.world.reg.item(stack.item).damage)
                    .unwrap_or(1.0)
                    .clamp(0.0, 16.0);
                let from = guest.pos + Vec3::new(0.0, 1.6, 0.0);
                let gpos = guest.pos;
                let reg = server.world.reg.clone();
                if let Some(m) = server.world.mob_by_id_mut(mob_id)
                    && (m.pos - gpos).length() <= REACH
                {
                    let def = reg.animals[m.species].clone();
                    m.hurt(&def, dmg, from);
                    m.last_hit_by = id;
                    if !def.hostile {
                        server.world.add_ire(2.0);
                    }
                    if server.world.mode != "creative" {
                        guest.hunger = (guest.hunger - 0.01).max(0.0);
                        guest.inventory.wear_tool(&reg, guest.hotbar);
                        refresh_held(guest);
                        self.send_player_state(id);
                    }
                }
            }
            C2S::FeedMob { id: mob_id } => {
                let gpos = guest.pos;
                let reg = server.world.reg.clone();
                if let Some(m) = server.world.mob_by_id_mut(mob_id) {
                    let def = &reg.animals[m.species];
                    if (m.pos - gpos).length() <= REACH
                        && !def.hostile
                        && def.breed_food.is_some()
                        && m.growth >= 1.0
                        && m.breed_cd <= 0.0
                        && !m.fed
                        && guest.inventory.slots[guest.hotbar].map(|stack| stack.item)
                            == def.breed_food
                    {
                        m.fed = true;
                        m.calm = 30.0;
                        if server.world.mode != "creative" {
                            guest.inventory.take_one(guest.hotbar);
                            refresh_held(guest);
                            self.send_player_state(id);
                        }
                    }
                }
            }
            C2S::BrushBlock { x, y, z } => {
                let p = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                if (p - guest.pos).length() > REACH {
                    return;
                }
                let b = server.world.get_block(x, y, z);
                if server.world.reg.block(b).brush.is_none() {
                    return;
                }
                if !guest.inventory.slots[guest.hotbar]
                    .is_some_and(|stack| server.world.reg.item(stack.item).brush_tool)
                {
                    return;
                }
                let mut r = server.rng;
                let found = server.world.brush_block(x, y, z, &mut r);
                server.rng = r;
                if let Some(stack) = found {
                    server.world.queue_give(id, stack);
                }
                if server.world.mode != "creative" {
                    guest.inventory.wear_tool(&server.world.reg, guest.hotbar);
                    refresh_held(guest);
                    self.send_player_state(id);
                }
            }
            C2S::FireProjectile { direction, charge } => {
                if guest.action_cooldown > 0.0 || !direction.is_finite() || direction.length() < 0.5
                {
                    return;
                }
                let direction = direction.normalize();
                let selected = guest.inventory.slots[guest.hotbar];
                let Some(selected) = selected else { return };
                let def = server.world.reg.item(selected.item).clone();
                let creative = server.world.mode == "creative";
                let (speed, damage, tile, drop_item) = if let Some(bow) = def.bow {
                    let Some(ammo) =
                        take_ammo(&mut guest.inventory, &server.world.reg, "arrow", creative)
                    else {
                        return;
                    };
                    let charge = charge.clamp(0.0, 1.0);
                    if !creative {
                        guest.inventory.wear_tool(&server.world.reg, guest.hotbar);
                    }
                    (
                        bow.speed * (0.6 + 0.4 * charge),
                        bow.damage * (0.45 + 0.55 * charge),
                        server.world.reg.item(ammo).icon,
                        (!creative).then_some(ammo),
                    )
                } else if let Some(speed) = def.throw_speed {
                    if !creative {
                        guest.inventory.take_one(guest.hotbar);
                    }
                    (speed, 0.0, def.icon, None)
                } else {
                    return;
                };
                guest.action_cooldown = 0.25;
                let pos = guest.pos + Vec3::new(0.0, 1.6, 0.0) + direction * 0.4;
                server.world.spawn_projectile(crate::mobs::Projectile {
                    pos,
                    vel: direction * speed.min(40.0),
                    tile,
                    damage: damage.clamp(0.0, 12.0),
                    age: 0.0,
                    from_player: true,
                    drop_item,
                    owner: id,
                });
                refresh_held(guest);
                self.send_player_state(id);
            }
            C2S::OpenContainer { x, y, z } => {
                let p = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                if (p - guest.pos).length() > REACH {
                    return;
                }
                let b = server.world.get_block(x, y, z);
                let kind = match server.world.reg.block(b).interaction.as_deref() {
                    Some("chest") => 0u8,
                    Some("furnace") => 1,
                    Some("offering") => 2,
                    Some("bloomery") => 3,
                    Some("kiln") => 4,
                    _ => return,
                };
                let default = match kind {
                    0 => BlockEntity::Chest(Default::default()),
                    1 => BlockEntity::Furnace(Default::default()),
                    3 => BlockEntity::Bloomery(Default::default()),
                    4 => BlockEntity::Kiln(Default::default()),
                    _ => BlockEntity::Offering(Default::default()),
                };
                let entry = server.world.ensure_block_entity((x, y, z), default);
                if let BlockEntity::Chest(c) = entry
                    && c.wild_owned
                {
                    c.wild_owned = false;
                    server.world.add_ire(1.0);
                    self.net
                        .send(id, &S2C::Toast("The wild keeps its trophies.".into()));
                }
                if let Some(g) = self.guests.get_mut(&id) {
                    g.container = Some((x, y, z));
                }
                self.send_container(server, id, (x, y, z));
            }
            C2S::ContainerClick {
                x,
                y,
                z,
                slot,
                right,
            } => {
                self.container_click(server, id, (x, y, z), slot as usize, right);
            }
            C2S::CloseContainer => {
                guest.container = None;
            }
            C2S::LightBloomery { x, y, z } => {
                let p = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                if (p - guest.pos).length() > REACH {
                    return;
                }
                let ember = server.world.reg.item_id("base:ember");
                if server.world.mode != "creative"
                    && !guest
                        .inventory
                        .slots
                        .iter()
                        .flatten()
                        .any(|stack| Some(stack.item) == ember)
                {
                    return;
                }
                let b = server.world.get_block(x, y, z);
                let res = match server.world.reg.block(b).interaction.as_deref() {
                    Some("kiln") => server.world.light_kiln(x, y, z),
                    _ => server.world.light_bloomery(x, y, z),
                };
                match res {
                    Ok(()) => {
                        if server.world.mode != "creative"
                            && let Some(ember) = server.world.reg.item_id("base:ember")
                            && take_item(&mut guest.inventory, ember)
                        {
                            refresh_held(guest);
                            self.send_player_state(id);
                        }
                        self.send_container(server, id, (x, y, z));
                    }
                    Err(e) => self.net.send(id, &S2C::Toast(e.into())),
                }
            }
            C2S::LightClamp { x, y, z } => {
                let p = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                if (p - guest.pos).length() > REACH {
                    return;
                }
                if server.world.mode != "creative"
                    && guest.inventory.slots[guest.hotbar].map(|stack| stack.item)
                        != server.world.reg.item_id("base:ember")
                {
                    return;
                }
                match server.world.try_light_clamp(x, y, z) {
                    Ok(n) => {
                        if server.world.mode != "creative" {
                            guest.inventory.take_one(guest.hotbar);
                            refresh_held(guest);
                            self.send_player_state(id);
                        }
                        self.net
                            .send(id, &S2C::Toast(format!("The clamp smolders ({n} logs).")));
                    }
                    Err(e) => self.net.send(id, &S2C::Toast(e.into())),
                }
            }
            C2S::AnvilPut { x, y, z } => {
                let p = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                if (p - guest.pos).length() > REACH {
                    return;
                }
                let Some(stack) = guest.inventory.slots[guest.hotbar] else {
                    return;
                };
                let one = ItemStack { count: 1, ..stack };
                if server.world.anvil_put((x, y, z), one) && server.world.mode != "creative" {
                    guest.inventory.take_one(guest.hotbar);
                    refresh_held(guest);
                    self.send_player_state(id);
                }
            }
            C2S::AnvilStrike { x, y, z } => {
                let p = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                if (p - guest.pos).length() > REACH {
                    return;
                }
                if guest.action_cooldown > 0.0 {
                    return;
                }
                guest.action_cooldown = 0.35;
                let has_hammer = guest.inventory.slots[guest.hotbar]
                    .is_some_and(|stack| server.world.reg.item(stack.item).hammer);
                if !has_hammer && server.world.mode != "creative" {
                    return;
                }
                if has_hammer && server.world.mode != "creative" {
                    guest.inventory.wear_tool(&server.world.reg, guest.hotbar);
                    refresh_held(guest);
                }
                if let Some(out) = server.world.anvil_strike((x, y, z)) {
                    server.world.queue_give(id, out);
                }
                self.send_player_state(id);
            }
            C2S::AnvilTake { x, y, z } => {
                let p = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                if (p - guest.pos).length() > REACH {
                    return;
                }
                if let Some(b) = server.world.anvil_take((x, y, z)) {
                    server.world.queue_give(id, b);
                }
            }
            C2S::SleepRequest => {
                guest.sleeping = true;
            }
            C2S::SleepCancel => {
                guest.sleeping = false;
            }
            C2S::InventoryClick { area, slot, right } => {
                let slot = slot as usize;
                let reg = &server.world.reg;
                match area {
                    net::InventoryArea::Inventory if slot < guest.inventory.slots.len() => {
                        let (value, cursor) =
                            click_stack(reg, guest.inventory.slots[slot], guest.cursor, right);
                        guest.inventory.slots[slot] = value;
                        guest.cursor = cursor;
                    }
                    net::InventoryArea::Craft if slot < guest.craft_grid.len() => {
                        let (value, cursor) =
                            click_stack(reg, guest.craft_grid[slot], guest.cursor, right);
                        guest.craft_grid[slot] = value;
                        guest.cursor = cursor;
                    }
                    net::InventoryArea::Armor if slot < guest.armor.len() => {
                        match (guest.cursor, guest.armor[slot]) {
                            (Some(cursor), current) => {
                                let fits = if slot == 4 {
                                    reg.item(cursor.item).charm.is_some()
                                } else {
                                    reg.item(cursor.item).armor.map(|(kind, _)| kind as usize)
                                        == Some(slot)
                                };
                                if fits {
                                    guest.armor[slot] = Some(cursor);
                                    guest.cursor = current;
                                }
                            }
                            (None, Some(current)) => {
                                guest.armor[slot] = None;
                                guest.cursor = Some(current);
                            }
                            _ => {}
                        }
                    }
                    _ => return,
                }
                refresh_held(guest);
                self.send_player_state(id);
            }
            C2S::CraftResult { size } => {
                let size = size as usize;
                if !(2..=3).contains(&size) {
                    return;
                }
                let Some(recipe) = crate::crafting::match_recipe(
                    &server.world.reg,
                    &guest.craft_grid[..size * size],
                    size,
                ) else {
                    return;
                };
                let output = ItemStack::new(&server.world.reg, recipe.output, recipe.count);
                match guest.cursor {
                    None => guest.cursor = Some(output),
                    Some(cursor)
                        if cursor.can_merge(&server.world.reg, &output)
                            && cursor.count + output.count
                                <= server.world.reg.item(cursor.item).max_stack =>
                    {
                        guest.cursor = Some(ItemStack {
                            count: cursor.count + output.count,
                            ..cursor
                        });
                    }
                    _ => return,
                }
                crate::crafting::consume(&mut guest.craft_grid[..size * size]);
                self.send_player_state(id);
            }
            C2S::EatSelected => {
                let Some(stack) = guest.inventory.slots[guest.hotbar] else {
                    return;
                };
                let Some(food) = server.world.reg.item(stack.item).food.clone() else {
                    return;
                };
                let wants = guest.hunger < 19.5
                    || food
                        .nutrition
                        .iter()
                        .zip(&guest.nutrition)
                        .any(|(add, value)| *add > 0.0 && *value < 99.0);
                if !wants {
                    return;
                }
                guest.hunger = (guest.hunger + food.hunger).min(20.0);
                for (value, add) in guest.nutrition.iter_mut().zip(&food.nutrition) {
                    *value = (*value + add).min(100.0);
                }
                if server.world.mode != "creative" {
                    guest.inventory.take_one(guest.hotbar);
                }
                refresh_held(guest);
                self.send_player_state(id);
            }
            C2S::Respawn => {
                if guest.health > 0.0 {
                    return;
                }
                // The saved spawn may be buried or dug out by now.
                guest.pos = server.world.settle_spawn(guest.spawn);
                guest.health = 14.0;
                guest.hunger = 20.0;
                guest.since_damage = 100.0;
                self.send_player_state(id);
            }
            C2S::Chat(msg) => {
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
                self.net.broadcast(&S2C::Chat {
                    from: from.clone(),
                    msg: msg.clone(),
                });
                fx.push(HostFx::Chat { from, msg });
            }
        }
    }

    fn on_moderation_request(&mut self, actor: u32, target: u32, action: ModerationAction) {
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

    fn send_player_state(&self, id: u32) {
        if let Some(guest) = self.guests.get(&id) {
            self.net.send(
                id,
                &S2C::PlayerState(PlayerRuntime::from_guest(guest).to_snap()),
            );
        }
    }

    /// Apply one guest click with the cursor stack it sent, exactly as
    /// local play would, then echo the container and the new cursor.
    fn container_click(
        &mut self,
        server: &mut Server,
        id: u32,
        pos: (i32, i32, i32),
        slot: usize,
        right: bool,
    ) {
        let reg = server.world.reg.clone();
        let bloomery_chain = reg.bloomery.first().cloned();
        let kiln_base = reg.kiln_base;
        let kiln_powders: Vec<crate::registry::ItemId> =
            reg.kiln.iter().map(|(powder, _)| *powder).collect();
        if self
            .guests
            .get(&id)
            .is_none_or(|g| g.container != Some(pos))
        {
            return;
        }
        let Some(entity) = server.world.block_entity_mut(&pos) else {
            return;
        };
        let mut held = self.guests.get(&id).and_then(|guest| guest.cursor);
        match entity {
            BlockEntity::Bloomery(bl) => {
                // Sealed while firing; charge takes ore-chain items,
                // the bank takes its fuel. Taking out is always fine.
                if !bl.lit && slot < 8 {
                    let want = bloomery_chain
                        .map(|chain| if slot < 4 { chain.charge } else { chain.fuel });
                    let s = if slot < 4 {
                        &mut bl.charge[slot]
                    } else {
                        &mut bl.fuel[slot - 4]
                    };
                    if held.is_none() || held.map(|h| Some(h.item)) == Some(want) {
                        let (ns, nh) = click_stack(&reg, *s, held, right);
                        *s = ns;
                        held = nh;
                    }
                }
            }
            BlockEntity::Kiln(kl) => {
                // Sealed while firing. Sand slots 0-3, powder 4, fuel
                // 5-8; puts validate against the kiln tables.
                if !kl.lit && slot < 9 {
                    let ok_put = |it: crate::registry::ItemId| match slot {
                        0..=3 => kiln_base.map(|(sand, _, _)| sand) == Some(it),
                        4 => kiln_powders.contains(&it),
                        _ => kiln_base.map(|(_, fuel, _)| fuel) == Some(it),
                    };
                    let s = match slot {
                        0..=3 => &mut kl.sand[slot],
                        4 => &mut kl.powder,
                        _ => &mut kl.fuel[slot - 5],
                    };
                    if held.is_none() || held.map(|h| ok_put(h.item)) == Some(true) {
                        let (ns, nh) = click_stack(&reg, *s, held, right);
                        *s = ns;
                        held = nh;
                    }
                }
            }
            BlockEntity::Clamp(_) | BlockEntity::Anvil(_) => {}
            BlockEntity::Chest(c) => {
                if slot < c.slots.len() {
                    let (ns, nh) = click_stack(&reg, c.slots[slot], held, right);
                    c.slots[slot] = ns;
                    held = nh;
                }
            }
            BlockEntity::Offering(o) => {
                if slot < o.slots.len() {
                    let (ns, nh) = click_stack(&reg, o.slots[slot], held, right);
                    o.slots[slot] = ns;
                    held = nh;
                }
            }
            BlockEntity::Furnace(f) => match slot {
                0 | 1 => {
                    let cur = if slot == 0 { f.input } else { f.fuel };
                    let (ns, nh) = click_stack(&reg, cur, held, right);
                    if slot == 0 {
                        if f.input.map(|s| s.item) != ns.map(|s| s.item) {
                            f.progress = 0.0;
                        }
                        f.input = ns;
                    } else {
                        f.fuel = ns;
                    }
                    held = nh;
                }
                _ => {
                    // Output: take-only, merging into the cursor.
                    if let Some(out) = f.output {
                        match held {
                            None => {
                                held = Some(out);
                                f.output = None;
                            }
                            Some(h)
                                if h.item == out.item
                                    && h.count + out.count <= reg.item(h.item).max_stack =>
                            {
                                held = Some(ItemStack {
                                    count: h.count + out.count,
                                    ..h
                                });
                                f.output = None;
                            }
                            _ => {}
                        }
                    }
                }
            },
        }
        let snap = held.map(|s| StackSnap {
            item: s.item.0,
            count: s.count,
            durability: s.durability,
        });
        if let Some(guest) = self.guests.get_mut(&id) {
            guest.cursor = held;
        }
        self.net.send(id, &S2C::HeldResult(snap));
        self.send_player_state(id);
        self.send_container(server, id, pos);
    }

    fn send_container(&mut self, server: &Server, id: u32, pos: (i32, i32, i32)) {
        let Some(entity) = server.world.block_entity(&pos) else {
            return;
        };
        let snap = |s: &Option<ItemStack>| {
            s.map(|s| StackSnap {
                item: s.item.0,
                count: s.count,
                durability: s.durability,
            })
        };
        let (kind, slots, aux): (u8, Vec<Option<StackSnap>>, Vec<f32>) = match entity {
            BlockEntity::Chest(c) => (0, c.slots.iter().map(snap).collect(), Vec::new()),
            BlockEntity::Furnace(f) => (
                1,
                vec![snap(&f.input), snap(&f.fuel), snap(&f.output)],
                vec![f.progress, f.burn_left, f.burn_total],
            ),
            BlockEntity::Offering(o) => (2, o.slots.iter().map(snap).collect(), Vec::new()),
            BlockEntity::Bloomery(b) => (
                3,
                b.charge.iter().chain(b.fuel.iter()).map(snap).collect(),
                vec![
                    if b.lit { 1.0 } else { 0.0 },
                    b.progress / crate::world::BLOOMERY_FIRE_SECS,
                ],
            ),
            BlockEntity::Kiln(k) => (
                4,
                k.sand
                    .iter()
                    .chain([&k.powder])
                    .chain(k.fuel.iter())
                    .map(snap)
                    .collect(),
                vec![
                    if k.lit { 1.0 } else { 0.0 },
                    k.progress / crate::world::KILN_FIRE_SECS,
                ],
            ),
            BlockEntity::Clamp(_) | BlockEntity::Anvil(_) => return,
        };
        self.net.send(
            id,
            &S2C::Container {
                x: pos.0,
                y: pos.1,
                z: pos.2,
                kind,
                slots,
                aux,
            },
        );
    }
}

fn refresh_held(guest: &mut Guest) {
    guest.held = guest.inventory.slots[guest.hotbar]
        .map(|stack| stack.item.0)
        .unwrap_or(u16::MAX);
}

fn server_item_armor_points(stack: &ItemStack, profiles: Option<&ProfileStore>) -> Option<u32> {
    profiles?
        .registry_hint()
        .item(stack.item)
        .armor
        .map(|(_, points)| points)
}

fn take_item(inventory: &mut crate::inventory::Inventory, item: crate::registry::ItemId) -> bool {
    let Some(slot) = inventory
        .slots
        .iter()
        .position(|stack| stack.is_some_and(|stack| stack.item == item))
    else {
        return false;
    };
    inventory.take_one(slot).is_some()
}

fn take_ammo(
    inventory: &mut crate::inventory::Inventory,
    reg: &crate::registry::Registry,
    class: &str,
    creative: bool,
) -> Option<crate::registry::ItemId> {
    let item = inventory
        .slots
        .iter()
        .flatten()
        .find(|stack| reg.item(stack.item).ammo.as_deref() == Some(class))?
        .item;
    if !creative {
        let _ = take_item(inventory, item);
    }
    Some(item)
}

/// Build the host-id -> local-id block remap from a Welcome palette.
pub fn block_remap(world: &World, palette: &[String]) -> Vec<crate::registry::BlockId> {
    palette
        .iter()
        .map(|name| world.reg.block_id(name).unwrap_or(world.reg.unknown_block))
        .collect()
}

/// Build the host-id -> local-id item remap (unknown items map to None).
pub fn item_remap(world: &World, items: &[String]) -> Vec<Option<crate::registry::ItemId>> {
    items.iter().map(|name| world.reg.item_id(name)).collect()
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    use crate::identity::{AtprotoDid, DeviceKeyId};

    #[test]
    fn two_devices_for_one_did_share_an_active_principal() {
        let did = Principal::Atproto(AtprotoDid::parse("did:plc:sharedaccount").unwrap());
        let first = vec![did.clone(), Principal::LocalDevice(DeviceKeyId([1; 32]))];
        let second = vec![did, Principal::LocalDevice(DeviceKeyId([2; 32]))];
        assert!(shares_principal(&first, &second));
        assert!(!shares_principal(
            &first,
            &[Principal::LocalDevice(DeviceKeyId([3; 32]))]
        ));
    }

    #[test]
    fn remote_moderation_permissions_are_host_enforced() {
        assert!(!moderation_action_allowed(
            Role::Player,
            ModerationAction::Kick
        ));
        assert!(moderation_action_allowed(
            Role::Moderator,
            ModerationAction::Kick
        ));
        assert!(moderation_action_allowed(
            Role::Moderator,
            ModerationAction::Mute { seconds: 600 }
        ));
        assert!(moderation_action_allowed(
            Role::Moderator,
            ModerationAction::Ban {
                seconds: Some(3600)
            }
        ));
        assert!(!moderation_action_allowed(
            Role::Moderator,
            ModerationAction::Ban { seconds: None }
        ));
        assert!(!moderation_action_allowed(
            Role::Moderator,
            ModerationAction::CycleRole
        ));
        assert!(moderation_action_allowed(
            Role::Admin,
            ModerationAction::Ban { seconds: None }
        ));
        assert!(moderation_action_allowed(
            Role::Admin,
            ModerationAction::Allow
        ));
        assert!(moderation_action_allowed(
            Role::Admin,
            ModerationAction::CycleRole
        ));
        assert!(!moderation_action_allowed(
            Role::Owner,
            ModerationAction::Mute { seconds: 0 }
        ));
        assert!(!moderation_action_allowed(
            Role::Owner,
            ModerationAction::Ban {
                seconds: Some(86_401)
            }
        ));
    }
}
