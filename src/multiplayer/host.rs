//! Host-side multiplayer session: applies guest requests through the
//! same authoritative code paths local play uses, and broadcasts the
//! world back. Shared by the windowed host (pause menu → OPEN TO
//! FRIENDS) and the headless `--server` — one code path.

#[path = "host/chunk_jobs.rs"]
mod chunk_jobs;
#[path = "moderation.rs"]
mod moderation;
#[path = "profiles.rs"]
mod profiles;
#[path = "roster.rs"]
mod roster;
#[path = "settings.rs"]
mod settings;
#[path = "host/streaming.rs"]
mod streaming;
#[path = "host/streaming_errors.rs"]
mod streaming_errors;

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use glam::Vec3;

use crate::chunk::ChunkPos;
pub use crate::identity::Role;
use crate::identity::{AdmissionPolicy, DisplayName, IdentityPolicy, Principal};
use crate::inventory::{HOTBAR_SLOTS, ItemStack, click_stack};
use crate::machines::MachineHandler;
use crate::net::{
    self, C2S, HostEvent, MAX_GUEST_VIEW_DIST, ModerationAction, Refusal, RefusalCode, S2C,
    StackSnap,
};
use crate::planet::{BlockPos, EntityPos};
use crate::server::Server;
use crate::world::{BlockEntity, MachineInstance, World};
use moderation::{BanIdentity, ModerationStore};
use profiles::{PlayerRuntime, ProfileStore};
pub use settings::ServerSettings;

const DISCOVERY_SETTLE: Duration = Duration::from_millis(1_200);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingDiscoveryKind {
    Observation(net::DiscoveryTargetSnap),
    Experiment(BlockPos, crate::discovery::ExperimentKind),
}

#[derive(Clone, Copy, Debug)]
struct PendingDiscovery {
    kind: PendingDiscoveryKind,
    began: Instant,
}

impl PendingDiscovery {
    fn settled_for(self, kind: PendingDiscoveryKind, now: Instant) -> bool {
        self.kind == kind && now.saturating_duration_since(self.began) >= DISCOVERY_SETTLE
    }
}

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
    pub pos: EntityPos,
    pub yaw: f32,
    pub container: Option<BlockPos>,
    /// The mob pack this guest has open (host-validated).
    pub mob_cargo: Option<u32>,
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
    pub bodily_dross: u64,
    pub spawn: EntityPos,
    pub pitch: f32,
    pub hotbar: usize,
    pub cursor: Option<ItemStack>,
    pub craft_grid: [Option<ItemStack>; 9],
    has_moved: bool,
    sprinting: bool,
    action_cooldown: f32,
    /// Host-owned proof that a lens or apparatus physically settled.
    pending_discovery: Option<PendingDiscovery>,
    /// At most one held wand channel per player. The durable transaction is
    /// owned by the world; this is only the request-routing handle.
    active_working: Option<u64>,
    since_damage: f32,
    regen_timer: f32,
    hunger_charm_credit: f32,
    starve_timer: f32,
    chat_count: u8,
    chat_window: f32,
    command_count: u16,
    command_window: f32,
    airborne_rise: f32,
    /// Chunks this guest is believed to hold. Entries are dropped once the
    /// guest walks out of range of them, because the guest evicts on the same
    /// rule — a set that only ever grew meant a guest who left an area and
    /// came back was never sent it again, and stood in a hole.
    sent_chunks: HashSet<ChunkPos>,
    /// Pending admission is authenticated but not yet a world actor. The host
    /// streams this exact safety set and waits for EntryReady.
    entry_required: Vec<ChunkPos>,
    entry_ready: bool,
    /// Granted view distance in chunks: what the guest asked for, clamped to
    /// `MAX_GUEST_VIEW_DIST`.
    view_dist: i32,
    /// Chunk requests are rate-limited like every other guest message.
    chunk_requests: u32,
    chunk_window: f32,
    edits: u32,
    edit_window: f32,
    /// Last bounded item-charge view sent to this guest. Keeping the sorted
    /// snapshot here makes unchanged wire state cost nothing.
    last_arcane_items: Vec<(u64, u64)>,
    last_implements: Vec<crate::implements::ImplementPublicState>,
    last_apparatus: Vec<crate::implements::ApparatusCue>,
    /// Movement packets arrive at ~20 Hz; rendering interpolates from
    /// here toward (pos, yaw) so guests glide instead of stutter.
    /// Embedded render position and yaw at the start of the current network
    /// interpolation span. Embedded space is continuous across cube-face
    /// seams, unlike either face's local `(u, v)` chart.
    render_from: (Vec3, f32),
    net_age: f32,
    net_interval: f32,
}

impl Guest {
    /// True only after the client has decoded and acknowledged its bounded
    /// entry terrain. Pre-entry guests are transport state, not world actors.
    pub fn is_active(&self) -> bool {
        self.entry_ready
    }

    /// View distance in chunks this guest was granted.
    #[cfg(test)]
    pub fn granted_view_dist(&self) -> i32 {
        self.view_dist
    }

    /// Does the host believe this guest already holds this chunk?
    #[cfg(test)]
    pub fn holds_chunk(&self, cx: i32, cz: i32) -> bool {
        ChunkPos::from_centered(crate::planet::Face::PosZ, cx, cz)
            .is_ok_and(|pos| self.sent_chunks.contains(&pos))
    }

    #[cfg(test)]
    pub fn holds_chunk_at(&self, pos: ChunkPos) -> bool {
        self.sent_chunks.contains(&pos)
    }

    #[cfg(test)]
    pub fn prime_move_for_test(&mut self, pos: EntityPos) {
        self.pos = pos;
        self.render_from = (pos.render_pos(), self.yaw);
        self.net_age = 0.3;
        self.net_interval = 0.3;
        self.has_moved = true;
    }

    /// Position/yaw to draw this guest at (the sim uses the latest).
    pub fn render_pos(&self) -> (Vec3, f32) {
        let t = (self.net_age / self.net_interval.max(0.001)).clamp(0.0, 1.0);
        (
            self.render_from.0.lerp(self.pos.render_pos(), t),
            crate::mobs::lerp_yaw(self.render_from.1, self.yaw, t),
        )
    }

    /// Canonical position for topology-aware light and label queries. Actual
    /// geometry uses [`Self::render_pos`] so a face transition remains smooth.
    pub fn render_entity_pos(&self) -> EntityPos {
        self.pos
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
    ImplementActivation {
        pos: EntityPos,
        cue: crate::implements::ImplementCue,
        visual: Option<crate::implements::ImplementVisual>,
    },
    WorkingEvent(crate::workings::WorkingCue),
    AlchemyEvent(crate::alchemy::AlchemyCue),
    /// Everyone slept: the host's own dawn side-effects should run.
    AllSlept,
    /// A guest clicked an action button on a mod screen (capability E11):
    /// both ids are already validated; dispatch the script hook.
    ScreenClick {
        screen: String,
        action: String,
    },
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
    pending_guests: HashMap<u32, PendingGuest>,
    moderation: Option<ModerationStore>,
    /// Principals kicked this session: refused if they reconnect.
    banned: HashSet<Principal>,
    /// Where new arrivals land, resolved once for the session.
    pub fresh_spawn: Option<EntityPos>,
    /// Pure terrain work for cold guest horizons. Created lazily from the
    /// authoritative world's seed/content; the host pump only adopts results.
    chunk_jobs: chunk_jobs::HostChunkState,
    /// Horizon assigned before a client negotiates one. Tests may shrink this
    /// for compact protocol fixtures; production retains the legacy five.
    initial_view_dist: i32,
    snapshot_timer: f32,
    /// Snapshot generation counter, so a split snapshot's parts can be
    /// recognised as belonging together on the far side.
    snapshot_seq: u32,
    state_timer: f32,
    container_timer: f32,
    perish_timer: f32,
    /// How long everyone-asleep has held: the dawn waits a breath so
    /// a sleep vote withdrawn in flight still counts as withdrawn.
    sleep_settle: f32,
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

struct PendingGuest {
    name: String,
    principal: Principal,
    verification_cached: bool,
    verified_handle: Option<String>,
    public_handle: Option<String>,
    runtime: PlayerRuntime,
    required: Vec<ChunkPos>,
    progress_age: f32,
}

const REACH: f32 = 7.0;
const EDITS_PER_SEC: u32 = 10;

/// Chunk requests honoured per guest per second. A guest returning to ground
/// it evicted may legitimately want a screenful at once; sustained flooding
/// is what this refuses.
const CHUNK_REQUESTS_PER_SECOND: u32 = 32;

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
            pending_guests: HashMap::new(),
            moderation: None,
            banned: HashSet::new(),
            fresh_spawn: None,
            chunk_jobs: chunk_jobs::HostChunkState::default(),
            initial_view_dist: 5,
            snapshot_timer: 0.0,
            snapshot_seq: 0,
            state_timer: 0.0,
            container_timer: 0.0,
            perish_timer: 0.0,
            sleep_settle: 0.0,
        })
    }

    /// PlayerCtx list for the simulation: host (when windowed) + guests.
    pub fn authoritative_player_ctxs(
        &self,
        world: &crate::world::World,
        host: Option<crate::server::PlayerCtx>,
    ) -> Vec<crate::server::PlayerCtx> {
        self.player_ctxs_impl(Some(world), host)
    }

    fn player_ctxs_impl(
        &self,
        world: Option<&crate::world::World>,
        host: Option<crate::server::PlayerCtx>,
    ) -> Vec<crate::server::PlayerCtx> {
        let mut out = Vec::new();
        if let Some(h) = host {
            out.push(h);
        }
        for (id, g) in self.guests.iter().filter(|(_, guest)| guest.entry_ready) {
            let quiet_charm = self.profiles.as_ref().and_then(|profiles| {
                g.armor[4]
                    .is_some_and(|stack| {
                        stack.arcane_id != 0
                            && profiles.registry_hint().item(stack.item).charm.as_deref()
                                == Some("quiet")
                    })
                    .then_some(g.armor[4])
                    .flatten()
            });
            let quiet_charm = quiet_charm
                .filter(|stack| world.is_none_or(|world| world.charm_can_pay(*stack, "quiet")));
            out.push(crate::server::PlayerCtx {
                id: *id,
                pos: g.pos,
                spawn: g.pos,
                attackable: true,
                aggro_mod: if quiet_charm.is_some() {
                    -crate::implements::QUIET_CHARM_AGGRO_REDUCTION
                } else {
                    0.0
                },
                quiet_charm,
            });
        }
        out
    }

    #[cfg(test)]
    pub(crate) fn set_initial_view_distance_for_test(&mut self, chunks: i32) {
        self.initial_view_dist = chunks.clamp(2, MAX_GUEST_VIEW_DIST as i32);
    }

    #[cfg(test)]
    pub fn player_ctxs(
        &self,
        host: Option<crate::server::PlayerCtx>,
    ) -> Vec<crate::server::PlayerCtx> {
        self.player_ctxs_impl(None, host)
    }

    /// Everything the host does per frame: drain guest messages, apply
    /// them authoritatively, stream state back.
    /// `host`: (pos, yaw, sleeping) for a windowed host; None when
    /// running headless (`--server`).
    pub fn pump(
        &mut self,
        server: &mut Server,
        host: Option<(EntityPos, f32, bool, u16, u32)>,
        dt: f32,
    ) -> Vec<HostFx> {
        self.pump_inner(server, host, None, dt)
    }

    pub fn pump_with_host_stack(
        &mut self,
        server: &mut Server,
        host: Option<(EntityPos, f32, bool, Option<ItemStack>, u32)>,
        dt: f32,
    ) -> Vec<HostFx> {
        let visual = host
            .and_then(|(_, _, _, stack, _)| stack)
            .and_then(|stack| server.world.implement_visual(stack));
        let host = host.map(|(pos, yaw, sleeping, stack, style)| {
            (
                pos,
                yaw,
                sleeping,
                stack.map_or(u16::MAX, |stack| stack.item.0),
                style,
            )
        });
        self.pump_inner(server, host, visual, dt)
    }

    fn pump_inner(
        &mut self,
        server: &mut Server,
        host: Option<(EntityPos, f32, bool, u16, u32)>,
        host_visual: Option<crate::implements::ImplementVisual>,
        dt: f32,
    ) -> Vec<HostFx> {
        let host_pos = host.map(|(p, _, _, _, _)| p).unwrap_or_else(|| {
            EntityPos::from_local(crate::planet::Face::PosZ, Vec3::new(0.5, 80.0, 0.5))
                .expect("default host position is canonical")
        });
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
                    self.pending_guests.remove(&id);
                    if let Some(g) = self.guests.remove(&id) {
                        if let Err(error) = server.world.interrupt_actor_workings(g.player_id.0) {
                            eprintln!(
                                "workings: disconnect settlement for {} failed: {error}",
                                g.player_id
                            );
                        }
                        if let Some(profiles) = &self.profiles
                            && let Err(e) =
                                profiles.save(&PlayerRuntime::from_guest(&g), &server.world.reg)
                        {
                            eprintln!("profiles: save {} failed: {e}", g.name);
                        }
                        if g.entry_ready {
                            self.broadcast_ready(&S2C::Left { id });
                            fx.push(HostFx::Left(g.name));
                        }
                    }
                }
                HostEvent::Msg { id, msg } => {
                    self.on_msg(server, id, msg, &mut fx);
                }
            }
        }

        let mut progress = Vec::new();
        for (id, pending) in &mut self.pending_guests {
            pending.progress_age += dt;
            if pending.progress_age >= 1.0 {
                pending.progress_age -= 1.0;
                let resident = pending
                    .required
                    .iter()
                    .filter(|position| server.world.has_chunk(**position))
                    .count();
                progress.push((*id, resident as u16, pending.required.len() as u16));
            }
        }
        for (id, resident, total) in progress {
            self.net.send(id, &S2C::EntryProgress { resident, total });
        }

        // Rate-limit windows + movement interpolation clocks.
        let creative = server.world.mode == "creative";
        let mut survival_changed = Vec::new();
        for g in self.guests.values_mut() {
            if !g.entry_ready {
                continue;
            }
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
            g.chunk_window += dt;
            if g.chunk_window >= 1.0 {
                g.chunk_window = 0.0;
                g.chunk_requests = 0;
            }
            if !creative {
                let old = (g.health, g.hunger, g.nutrition);
                if g.hunger_charm_credit <= 0.0
                    && let Some(mut charm) = g.armor[4]
                    && server.world.reg.item(charm.item).charm.as_deref() == Some("hunger")
                    && let Some(pos) = g.pos.block()
                    && server.world.debit_charm_at(
                        pos,
                        &mut charm,
                        "hunger",
                        "guest slow-hunger charm prepaid an active interval",
                    )
                {
                    g.armor[4] = Some(charm);
                    g.hunger_charm_credit = crate::implements::HUNGER_CHARM_INTERVAL_SECS;
                }
                let hunger_charm = g.hunger_charm_credit > 0.0;
                g.hunger_charm_credit = (g.hunger_charm_credit - dt).max(0.0);
                let drain = (0.01 + if g.sprinting { 0.02 } else { 0.0 })
                    * if hunger_charm {
                        crate::implements::HUNGER_CHARM_MULTIPLIER
                    } else {
                        1.0
                    };
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
        if self.state_timer + dt >= 1.0 {
            let mut status_cues = Vec::new();
            for (id, guest) in &mut self.guests {
                if !guest.entry_ready {
                    continue;
                }
                let Some(actor_pos) = guest.pos.block() else {
                    continue;
                };
                let max_health = 14.0
                    + guest
                        .nutrition
                        .iter()
                        .filter(|&&value| value >= 40.0)
                        .count() as f32
                        * 2.0;
                let physiology = crate::alchemy::PreparationPhysiology {
                    health: guest.health,
                    max_health,
                    hunger: guest.hunger,
                    nutrition: guest.nutrition,
                    strain: 0.0,
                    bodily_dross: guest.bodily_dross,
                };
                match server.world.tick_preparation_statuses(
                    guest.player_id.0,
                    actor_pos,
                    physiology,
                ) {
                    Ok(result) => {
                        guest.health = result.physiology.health;
                        guest.hunger = result.physiology.hunger;
                        guest.nutrition = result.physiology.nutrition;
                        guest.bodily_dross = result.physiology.bodily_dross;
                        self.net.send(
                            *id,
                            &S2C::PreparationState {
                                modifiers: result.modifiers,
                                bodily_dross: guest.bodily_dross,
                            },
                        );
                        status_cues.extend(result.cues);
                    }
                    Err(error) => eprintln!(
                        "alchemy: status update for {} failed: {error}",
                        guest.player_id
                    ),
                }
            }
            for cue in status_cues {
                for (observer, guest) in &self.guests {
                    if guest.entry_ready
                        && guest.pos.horizontal_distance_to(cue.pos.entity_center()) <= 96.0
                    {
                        self.net.send(*observer, &S2C::AlchemyEvent(cue.clone()));
                    }
                }
                fx.push(HostFx::AlchemyEvent(cue));
            }
        }
        let out_of_range = self
            .guests
            .iter()
            .filter_map(|(id, guest)| {
                let active = guest.active_working?;
                let source = guest.pos.block()?;
                (!server.world.wand_working_reachable_from(active, source)).then_some((*id, active))
            })
            .collect::<Vec<_>>();
        for (id, active) in out_of_range {
            let mut cue = server
                .world
                .working_cues()
                .into_iter()
                .find(|cue| cue.stable_id == active);
            if let Some(guest) = self.guests.get_mut(&id) {
                guest.active_working = None;
            }
            if let Ok(mut result) = server.world.interrupt_working(active) {
                result.message =
                    "The wand path leaves its bounded reach and breaks cleanly.".into();
                if let Some(cue) = cue.as_mut() {
                    cue.kind = result.cue;
                    cue.warning_band = result.warning_band;
                    cue.completion_permille = 1_000;
                }
                self.net.send(id, &S2C::WorkingResult(result));
                if let Some(cue) = cue {
                    self.broadcast_ready(&S2C::WorkingEvent(cue.clone()));
                    fx.push(HostFx::WorkingEvent(cue));
                }
            }
        }
        if self.state_timer + dt >= 1.0 && !survival_changed.is_empty() {
            let ids: Vec<u32> = self
                .guests
                .iter()
                .filter(|(_, guest)| guest.entry_ready)
                .map(|(id, _)| *id)
                .collect();
            for id in ids {
                self.send_player_state(id);
            }
        }

        // Authoritative block edits out.
        if !server.world.edits().is_empty() {
            for (pos, b, meta, salt_mass, soil_salinity) in server.world.take_edits() {
                if let Some(jobs) = self.chunk_jobs.as_mut() {
                    jobs.invalidate_encoded(pos.chunk());
                }
                self.broadcast_ready(&S2C::BlockSet {
                    pos,
                    id: b.0,
                    meta,
                    salt_mass,
                    soil_salinity,
                });
            }
        }
        // Items owed to guests (arrow recovery, mining, mob drops,
        // brush finds) — full stacks so durability crosses the wire.
        for (owner, s) in server.world.take_pending_gives() {
            let mut delivered = 0;
            let mut overflow_at = None;
            if let Some(guest) = self.guests.get_mut(&owner) {
                let left = guest.inventory.add_stack(&server.world.reg, s);
                delivered = s.count - left;
                overflow_at = guest.pos.block().map(|pos| (pos, left));
            }
            if delivered != 0 {
                self.net.send(
                    owner,
                    &S2C::Give {
                        item: s.item.0,
                        count: delivered,
                        durability: s.durability,
                        arcane_id: s.arcane_id,
                        current_units: server
                            .world
                            .inspectable_item_current(s.arcane_id)
                            .unwrap_or(0),
                    },
                );
            }
            if let Some((pos, left)) = overflow_at.filter(|(_, left)| *left != 0)
                && let Some(ledger) = &mut server.world.material_ledger
                && let Err(error) = ledger.bury_stack(
                    &server.world.reg,
                    pos,
                    ItemStack { count: left, ..s },
                    "full guest inventory",
                )
            {
                eprintln!("materials: guest delivery overflow accounting failed: {error}");
            }
            if let Some((pos, left)) = overflow_at.filter(|(_, left)| *left != 0) {
                server.world.retire_arcane_stack_at(
                    pos,
                    ItemStack { count: left, ..s },
                    "full guest inventory",
                );
            }
            self.send_player_state(owner);
        }
        // Dropped items are the same host-owned physical entities on a
        // windowed or dedicated host. Nearby guests pick them up through the
        // authoritative inventory; full packs leave the remainder in-world.
        let mut loose = server.world.take_loose_items();
        let mut changed = std::collections::BTreeSet::new();
        let mut index = 0;
        while index < loose.len() {
            let item = &loose[index];
            let nearest = (item.age > crate::entity::PICKUP_DELAY)
                .then(|| {
                    self.guests
                        .iter()
                        .filter(|(_, guest)| guest.entry_ready && guest.health > 0.0)
                        .filter_map(|(id, guest)| {
                            let target = guest.pos.translated(Vec3::new(0.0, 0.9, 0.0)).ok()?.pos;
                            let distance = item.pos.distance_to(target);
                            (distance < 1.4).then_some((*id, distance))
                        })
                        .min_by(|(left_id, left), (right_id, right)| {
                            left.total_cmp(right).then_with(|| left_id.cmp(right_id))
                        })
                        .map(|(id, _)| id)
                })
                .flatten();
            let Some(id) = nearest else {
                index += 1;
                continue;
            };
            let mut stack = ItemStack::new(&server.world.reg, item.item, item.count);
            stack.durability = item.durability;
            stack.arcane_id = item.arcane_id;
            let guest = self.guests.get_mut(&id).expect("selected guest exists");
            let left = guest.inventory.add_stack(&server.world.reg, stack);
            let delivered = stack.count.saturating_sub(left);
            if delivered != 0 {
                self.net.send(
                    id,
                    &S2C::Give {
                        item: stack.item.0,
                        count: delivered,
                        durability: stack.durability,
                        arcane_id: stack.arcane_id,
                        current_units: server
                            .world
                            .inspectable_item_current(stack.arcane_id)
                            .unwrap_or(0),
                    },
                );
                changed.insert(id);
            }
            if left == 0 {
                loose.swap_remove(index);
            } else {
                loose[index].count = left;
                index += 1;
            }
        }
        server.world.replace_loose_items(loose);
        for id in changed {
            self.send_player_state(id);
        }

        self.stream_chunks(server);
        self.stream_snapshots(
            server,
            host.map(|_| (host_pos, host_yaw, host_held, host_style, host_visual)),
            dt,
        );
        // Open containers stay live: furnaces smelt and other players
        // shuffle stacks while a guest is looking at them.
        self.container_timer += dt;
        if self.container_timer >= 0.5 {
            self.container_timer = 0.0;
            let open: Vec<(u32, BlockPos)> = self
                .guests
                .iter()
                .filter(|(_, guest)| guest.entry_ready)
                .filter_map(|(id, g)| g.container.map(|c| (*id, c)))
                .collect();
            for (id, pos) in open {
                self.send_container(server, id, pos);
            }
        }
        // Guest inventory mirrors age like everyone else's pack, so a
        // relog can't refresh yesterday's venison.
        self.perish_timer += dt;
        if self.perish_timer >= 20.0 {
            self.perish_timer -= 20.0;
            let reg = server.world.reg.clone();
            let mush = reg.item_id("base:spoiled_mush");
            let mut consumed = Vec::new();
            let step = (20.0 * crate::world::FRESHNESS_PER_SEC) as u32;
            let sweep_ticks = 20u64 * 20;
            let mut aged_guests = Vec::new();
            for (guest_id, g) in self
                .guests
                .iter_mut()
                .filter(|(_, guest)| guest.entry_ready)
            {
                let mut changed = false;
                let pack_temperature_millic = (server
                    .world
                    .weather_at_surface(g.pos.surface())
                    .temperature_c
                    * 1_000.0)
                    .round()
                    .clamp(i32::MIN as f32, i32::MAX as f32)
                    as i32;
                for (slot, s) in g.inventory.slots.iter_mut().enumerate() {
                    let Some(st) = s else { continue };
                    if st.arcane_id != 0 {
                        let holdfast_step =
                            server
                                .world
                                .holdfast_age_step(g.player_id.0, slot, *st, step, 20);
                        let ordinary_age_ticks = sweep_ticks
                            .saturating_mul(u64::from(holdfast_step))
                            .div_ceil(u64::from(step.max(1)));
                        match server.world.age_preparation_storage(
                            *st,
                            pack_temperature_millic,
                            ordinary_age_ticks,
                        ) {
                            Ok(Some(newly_spoiled)) => {
                                changed |= newly_spoiled;
                                continue;
                            }
                            Ok(None) => {}
                            Err(error) => {
                                eprintln!("alchemy: guest storage aging failed: {error}");
                                continue;
                            }
                        }
                    }
                    let full = reg.item(st.item).durability;
                    let food = reg.item(st.item).food.is_some();
                    let viable_seed = reg.item(st.item).name.ends_with("_seed");
                    if (!food && !viable_seed) || full == 0 {
                        continue;
                    }
                    if st.durability == 0 {
                        st.durability = full;
                        changed = true;
                    } else {
                        let holdfast_step =
                            server
                                .world
                                .holdfast_age_step(g.player_id.0, slot, *st, step, 20);
                        let actual_step = if st.arcane_id == 0 {
                            holdfast_step
                        } else {
                            server.world.coated_specimen_age_advance(
                                st.arcane_id,
                                u64::from(holdfast_step),
                                pack_temperature_millic,
                            ) as u32
                        };
                        if st.durability > actual_step {
                            st.durability -= actual_step;
                            changed = true;
                            continue;
                        }
                        if food {
                            consumed.push(*st);
                            *s = mush.map(|m| {
                                let mut sp = ItemStack::new(&reg, m, 1);
                                sp.count = st.count;
                                sp
                            });
                        } else {
                            st.durability = 0;
                        }
                        changed = true;
                    }
                }
                if let Some(at) = g.pos.block() {
                    for slot in 0..g.inventory.slots.len() {
                        if let Some(stack) = g.inventory.slots[slot]
                            && let Err(error) = server.world.leak_fragile_item_charge(
                                g.player_id.0,
                                slot,
                                stack,
                                at,
                                20,
                            )
                        {
                            eprintln!("arcane specimen leakage failed: {error}");
                        }
                    }
                }
                if changed {
                    aged_guests.push(*guest_id);
                }
            }
            if let Err(error) = server.world.record_consumed_stacks(consumed) {
                eprintln!("materials: spoiled guest food accounting failed: {error}");
            }
            for guest_id in aged_guests {
                self.send_player_state(guest_id);
            }
        }
        // Vehicles follow their riders exactly (the rider's client
        // owns their motion; the boat is presentation that floats).
        {
            let riders: Vec<(u32, EntityPos)> = self
                .guests
                .iter()
                .filter(|(_, guest)| guest.entry_ready)
                .map(|(gid, g)| (*gid, g.pos))
                .collect();
            for m in server.world.mobs_mut() {
                if let Some(rid) = m.ridden_by
                    && rid != 0
                {
                    match riders.iter().find(|(gid, _)| *gid == rid) {
                        Some((_, at)) => {
                            m.pos = at
                                .translated(Vec3::new(0.0, -0.35, 0.0))
                                .expect("vehicle remains below rider")
                                .pos;
                            m.vel = Vec3::ZERO;
                        }
                        None => m.ridden_by = None,
                    }
                }
            }
        }
        self.state_timer += dt;
        if self.state_timer >= 1.0 {
            self.state_timer = 0.0;
            self.broadcast_ready(&S2C::TimeIre {
                time: server.time_of_day,
                ire: server.world.ire,
                day: server.world.day(),
            });
            // Active effects are durable host state, not one-shot animation
            // packets. Refreshing these small qualitative cues lets late
            // joiners and packet-delayed guests see Gleam, ritual paths, and
            // persistent warning bands without exposing private accounting.
            for cue in server.world.working_cues() {
                self.broadcast_ready(&S2C::WorkingEvent(cue));
            }
            if let Some(atlas) = server.world.planet_atlas() {
                let side = atlas.side();
                let updates: Vec<_> =
                    self.guests
                        .iter()
                        .filter(|(_, guest)| guest.entry_ready)
                        .map(|(id, guest)| {
                            let center = atlas.atlas_pos(guest.pos.surface());
                            let mut positions = vec![center];
                            positions.extend(center.neighbors8(side));
                            positions.sort();
                            positions.dedup();
                            let cells = positions
                                .into_iter()
                                .map(|pos| {
                                    let center = pos.center(side);
                                    let surface =
                                        crate::planet::SurfacePos::new(
                                            center.face,
                                            center.u.floor().clamp(
                                                0.0,
                                                f64::from(crate::planet::FACE_BLOCKS - 1),
                                            ) as u16,
                                            center.v.floor().clamp(
                                                0.0,
                                                f64::from(crate::planet::FACE_BLOCKS - 1),
                                            ) as u16,
                                        )
                                        .expect("atlas weather center is canonical");
                                    (pos, server.world.weather_at_surface(surface))
                                })
                                .collect();
                            (*id, S2C::WeatherCells { side, cells })
                        })
                        .collect();
                for (id, update) in updates {
                    self.net.send(id, &update);
                }
                for (id, guest) in self.guests.iter().filter(|(_, guest)| guest.entry_ready) {
                    let region = atlas.atlas_pos(guest.pos.surface());
                    let (bands, dominant) = server.world.arcane_sensory_cue_at(region);
                    let ecology = server
                        .world
                        .arcane_ecology_observation_at(guest.pos.surface(), 72.0)
                        .map(|observation| (observation.text, observation.damped));
                    self.net.send(
                        *id,
                        &S2C::ArcaneCue {
                            bands,
                            dominant,
                            ecology,
                        },
                    );
                }
                let item_updates = self
                    .guests
                    .iter()
                    .filter(|(_, guest)| guest.entry_ready)
                    .map(|(id, guest)| (*id, inspectable_arcane_items(&server.world, guest)))
                    .collect::<Vec<_>>();
                for (id, (charges, implements, apparatus)) in item_updates {
                    let Some(guest) = self.guests.get_mut(&id) else {
                        continue;
                    };
                    if guest.last_arcane_items != charges
                        || guest.last_implements != implements
                        || guest.last_apparatus != apparatus
                    {
                        guest.last_arcane_items.clone_from(&charges);
                        guest.last_implements.clone_from(&implements);
                        guest.last_apparatus.clone_from(&apparatus);
                        // Public implement metadata is bounded to 1 KiB per
                        // identity, but a legitimately open chest/cargo pack
                        // can expose many identities at once. Split the
                        // reliable replacement snapshot so no mod-valid
                        // inventory can exceed the transport frame budget.
                        const IMPLEMENTS_PER_FRAME: usize = 16;
                        let batches = implements.len().div_ceil(IMPLEMENTS_PER_FRAME).max(1);
                        for batch in 0..batches {
                            let start = batch * IMPLEMENTS_PER_FRAME;
                            let end = (start + IMPLEMENTS_PER_FRAME).min(implements.len());
                            self.net.send(
                                id,
                                &S2C::ArcaneItems {
                                    reset: batch == 0,
                                    charges: if batch == 0 {
                                        charges.clone()
                                    } else {
                                        Vec::new()
                                    },
                                    implements: implements[start..end].to_vec(),
                                    apparatus: if batch == 0 {
                                        apparatus.clone()
                                    } else {
                                        Vec::new()
                                    },
                                },
                            );
                        }
                    }
                }
            }
        }

        // Sleep vote.
        if !host_sleeping && !self.guests.values().any(|g| g.entry_ready && g.sleeping) {
            self.sleep_settle = 0.0;
        }
        if host_sleeping || self.guests.values().any(|g| g.entry_ready && g.sleeping) {
            let present = self.guests.values().filter(|g| g.entry_ready).count() as u32
                + host.is_some() as u32;
            let sleeping = self
                .guests
                .values()
                .filter(|g| g.entry_ready && g.sleeping)
                .count() as u32
                + host_sleeping as u32;
            self.broadcast_ready(&S2C::Sleep { sleeping, present });
            self.sleep_settle = if sleeping == present {
                self.sleep_settle + dt
            } else {
                0.0
            };
            if sleeping == present && self.sleep_settle >= 0.75 {
                self.sleep_settle = 0.0;
                let skipped = (1.0 + 0.3 - server.time_of_day) % 1.0;
                if server.world.tick_ire(skipped) {
                    server.world.accept_offerings();
                }
                server.sleep_to_dawn();
                for g in self.guests.values_mut().filter(|guest| guest.entry_ready) {
                    g.sleeping = false;
                }
                self.broadcast_ready(&S2C::TimeIre {
                    time: server.time_of_day,
                    ire: server.world.ire,
                    day: server.world.day(),
                });
                self.broadcast_ready(&S2C::Toast("Dawn. The camp wakes.".into()));
                fx.push(HostFx::AllSlept);
            }
        }
        fx
    }

    fn on_join(&mut self, server: &mut Server, join: AuthenticatedJoin, _fx: &mut Vec<HostFx>) {
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

    fn try_finish_pending_entry(&mut self, server: &mut Server, id: u32) {
        let Some(mut pending) = self.pending_guests.remove(&id) else {
            return;
        };
        if pending
            .required
            .iter()
            .any(|position| !server.world.has_chunk(*position))
        {
            self.pending_guests.insert(id, pending);
            return;
        }
        // Terrain is resident before a stale saved position is repaired, so
        // the rescue scan cannot perform cold generation on the host pump.
        pending.runtime.pos = server.world.free_position_at(pending.runtime.pos);
        let required = crate::world::player_entry_chunks(pending.runtime.pos.surface());
        if required
            .iter()
            .any(|position| !server.world.has_chunk(*position))
        {
            pending.required = required;
            self.pending_guests.insert(id, pending);
            return;
        }

        if let Some(at) = pending.runtime.pos.block() {
            let migrated = server.world.migrate_legacy_player_charms(
                at,
                &mut pending.runtime.inventory,
                &mut pending.runtime.armor,
                &mut pending.runtime.cursor,
                &format!("multiplayer profile {}", pending.runtime.player_id),
            );
            if migrated != 0
                && let Some(profiles) = self.profiles.as_ref()
                && let Err(error) = profiles.save(&pending.runtime, &server.world.reg)
            {
                eprintln!(
                    "implements: migrated {migrated} guest charms but profile save failed: {error}"
                );
            }
        }

        match server.world.resume_pending_inventory_workings(
            pending.runtime.player_id.0,
            &mut pending.runtime.inventory,
        ) {
            Ok(ids) if !ids.is_empty() => {
                let checkpoint = self.profiles.as_ref().map_or_else(
                    || {
                        Err(std::io::Error::other(
                            "profile store is unavailable for pending Fieldmend replay",
                        ))
                    },
                    |profiles| profiles.save(&pending.runtime, &server.world.reg),
                );
                if let Err(error) = checkpoint {
                    eprintln!(
                        "workings: resumed guest Fieldmend remains pending after checkpoint failure: {error}"
                    );
                } else {
                    for working in ids {
                        if let Err(error) = server.world.finish_inventory_working(working) {
                            eprintln!(
                                "workings: guest Fieldmend profile landed but finalization failed: {error}"
                            );
                        }
                    }
                }
            }
            Ok(_) => {}
            Err(error) => {
                eprintln!("workings: pending guest Fieldmend is inconsistent: {error}")
            }
        }

        let reg = server.world.reg.clone();
        let palette: Vec<String> = reg.blocks.iter().map(|block| block.name.clone()).collect();
        let items: Vec<String> = reg.items.iter().map(|item| item.name.clone()).collect();
        let mut roster = Vec::new();
        if let Some(host_name) = &self.host_name {
            roster.push(roster::host_presence(host_name));
        }
        roster.extend(
            self.guests
                .iter()
                .filter(|(_, guest)| guest.entry_ready)
                .map(|(id, guest)| roster::guest_presence(*id, guest)),
        );
        let presence = roster::presence(
            id,
            pending.name.clone(),
            &pending.principal,
            pending.verification_cached,
            pending.public_handle.clone(),
        );
        roster.push(presence);
        let role = self
            .moderation
            .as_ref()
            .map(|store| store.role(&pending.principal))
            .unwrap_or_default();
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
                spawn: pending.runtime.pos,
                world_name: self.world_name.clone(),
                player_state: pending.runtime.to_snap(),
            },
        );
        self.net.send(
            id,
            &S2C::EntryManifest {
                spawn: pending.runtime.pos,
                required: required.clone(),
            },
        );
        if let Some(ledger) = &server.world.material_ledger {
            for notice in ledger.retrogen_notices() {
                self.net.send(id, &S2C::Toast(notice));
            }
        }
        let signs: Vec<(BlockPos, [String; 3])> = server
            .world
            .sign_texts()
            .map(|(position, sign)| (position, sign.lines.clone()))
            .collect();
        for (pos, lines) in signs {
            self.net.send(id, &S2C::SignText { pos, lines });
        }
        let runtime = pending.runtime;
        self.guests.insert(
            id,
            Guest {
                player_id: runtime.player_id,
                principals: runtime.principals,
                previous_names: runtime.previous_names,
                first_seen: runtime.first_seen,
                name: pending.name,
                principal: pending.principal,
                verification_cached: pending.verification_cached,
                verified_handle: pending.verified_handle,
                public_handle: pending.public_handle,
                pos: runtime.pos,
                yaw: runtime.yaw,
                container: None,
                mob_cargo: None,
                sleeping: false,
                held: runtime.held,
                style: runtime.style,
                inventory: runtime.inventory,
                armor: runtime.armor,
                health: runtime.health,
                hunger: runtime.hunger,
                nutrition: runtime.nutrition,
                bodily_dross: runtime.bodily_dross,
                spawn: runtime.spawn,
                pitch: runtime.pitch,
                hotbar: runtime.hotbar,
                cursor: runtime.cursor,
                craft_grid: [None; 9],
                has_moved: false,
                sprinting: false,
                action_cooldown: 0.0,
                pending_discovery: None,
                active_working: None,
                since_damage: 100.0,
                regen_timer: 0.0,
                hunger_charm_credit: 0.0,
                starve_timer: 0.0,
                chat_count: 0,
                chat_window: 0.0,
                command_count: 0,
                command_window: 0.0,
                airborne_rise: 0.0,
                sent_chunks: HashSet::new(),
                entry_required: required,
                entry_ready: false,
                view_dist: self.initial_view_dist,
                chunk_requests: 0,
                chunk_window: 0.0,
                edits: 0,
                edit_window: 0.0,
                last_arcane_items: Vec::new(),
                last_implements: Vec::new(),
                last_apparatus: Vec::new(),
                render_from: (runtime.pos.render_pos(), 0.0),
                net_age: 0.0,
                net_interval: 0.05,
            },
        );
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

    /// Gameplay broadcasts exclude authenticated connections that are still
    /// decoding their entry terrain. They do not yet have a coherent world
    /// mirror and are not members of the active roster.
    fn broadcast_ready(&self, msg: &S2C) {
        for (id, guest) in &self.guests {
            if guest.entry_ready {
                self.net.send(*id, msg);
            }
        }
    }

    pub fn broadcast_working_cue(&self, cue: crate::workings::WorkingCue) {
        self.broadcast_ready(&S2C::WorkingEvent(cue));
    }

    pub fn broadcast_alchemy_cue(&self, cue: crate::alchemy::AlchemyCue) {
        self.broadcast_ready(&S2C::AlchemyEvent(cue));
    }

    pub fn broadcast_dross_cue(&self, world: &crate::world::World, cue: crate::dross::DrossCue) {
        let Some(atlas) = world.planet_atlas() else {
            return;
        };
        for (id, guest) in &self.guests {
            if guest.entry_ready && atlas.atlas_pos(guest.pos.surface()) == cue.region {
                self.net.send(*id, &S2C::DrossEvent(cue));
            }
        }
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

    /// Apply simulation damage to server-owned survival state. The `Hit`
    /// packet is presentation; the following `PlayerState` is the authority.
    pub fn hurt_guest(
        &mut self,
        server: &mut Server,
        id: u32,
        amount: f32,
        from: crate::planet::EntityPos,
    ) {
        let ready_observers = self
            .guests
            .iter()
            .filter_map(|(observer, guest)| guest.entry_ready.then_some(*observer))
            .collect::<Vec<_>>();
        let Some(guest) = self.guests.get_mut(&id) else {
            return;
        };
        if !guest.entry_ready || guest.health <= 0.0 {
            return;
        }
        let mut armor_points: u32 = guest
            .armor
            .iter()
            .flatten()
            .filter_map(|stack| server_item_armor_points(stack, self.profiles.as_ref()))
            .sum();
        if let Some(mut charm) = guest.armor[4]
            && let Some(pos) = guest.pos.block()
            && server.world.debit_charm_at(
                pos,
                &mut charm,
                "bark",
                "guest bark charm prevented warden damage",
            )
        {
            guest.armor[4] = Some(charm);
            armor_points = armor_points.saturating_add(crate::implements::BARK_CHARM_ARMOR_POINTS);
        }
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
                        *armor = registry
                            .item(stack.item)
                            .broken_into
                            .map(|broken| ItemStack {
                                item: broken,
                                count: 1,
                                durability: 0,
                                arcane_id: stack.arcane_id,
                            });
                    }
                }
            }
        }
        guest.health = (guest.health - reduced).max(0.0);
        guest.since_damage = 0.0;
        if guest.health <= 0.0 {
            guest.active_working = None;
            let prior = server
                .world
                .working_cues()
                .into_iter()
                .filter(|cue| {
                    server
                        .world
                        .workings_state
                        .as_ref()
                        .and_then(|state| state.active.get(&cue.stable_id))
                        .is_some_and(|transaction| transaction.actor == guest.player_id.0)
                })
                .map(|cue| (cue.stable_id, cue))
                .collect::<std::collections::HashMap<_, _>>();
            if let Ok(results) = server.world.interrupt_actor_workings(guest.player_id.0) {
                for result in results {
                    if let Some(mut cue) = prior.get(&result.stable_id).cloned() {
                        cue.kind = result.cue;
                        cue.warning_band = result.warning_band;
                        cue.completion_permille = 1_000;
                        for observer in &ready_observers {
                            self.net.send(*observer, &S2C::WorkingEvent(cue.clone()));
                        }
                    }
                }
            }
            if let Some(actor_pos) = guest.pos.block().or_else(|| guest.spawn.block())
                && let Err(error) = server
                    .world
                    .settle_preparations_on_death(guest.player_id.0, actor_pos)
            {
                eprintln!(
                    "alchemy: hosted death settlement for {} failed: {error}",
                    guest.player_id
                );
            }
            let mut lost = guest.inventory.drain();
            lost.extend(guest.armor.iter_mut().filter_map(Option::take));
            lost.extend(guest.cursor.take());
            lost.extend(guest.craft_grid.iter_mut().filter_map(Option::take));
            // Death moves physical stacks into the world's ordinary drop
            // path. Burying them here destroyed the durable reference while
            // leaving a charged implement account behind (and made hosted
            // death behave differently from local death). A windowed host
            // renders these as loose items; a dedicated host routes them
            // through its bounded delivery/banking policy.
            if let Some(pos) = guest.pos.block().or_else(|| guest.spawn.block()) {
                for stack in lost {
                    server.world.push_drop_at(pos, stack);
                }
            } else {
                // A valid player should always have either a present or spawn
                // block. If corrupted coordinates defeat both, settle every
                // charged identity explicitly instead of leaking custody.
                let fallback = crate::planet::BlockPos::new(
                    guest.pos.face(),
                    guest
                        .pos
                        .u()
                        .floor()
                        .clamp(0.0, f32::from(crate::planet::FACE_BLOCKS - 1))
                        as u16,
                    0,
                    guest
                        .pos
                        .v()
                        .floor()
                        .clamp(0.0, f32::from(crate::planet::FACE_BLOCKS - 1))
                        as u16,
                )
                .expect("clamped player surface is a block");
                for stack in lost {
                    if let Some(ledger) = &mut server.world.material_ledger
                        && let Err(error) = ledger.bury_stack(
                            &server.world.reg,
                            fallback,
                            stack,
                            "invalid-position hosted death",
                        )
                    {
                        eprintln!("materials: guest death settlement failed: {error}");
                    }
                    server.world.retire_arcane_stack_at(
                        fallback,
                        stack,
                        "invalid-position hosted death",
                    );
                }
            }
            refresh_held(guest);
        }
        self.net.send(id, &S2C::Hit { dmg: reduced, from });
        self.send_player_state(id);
    }

    fn on_msg(&mut self, server: &mut Server, id: u32, msg: C2S, fx: &mut Vec<HostFx>) {
        if matches!(&msg, C2S::EntryReady) {
            let accepted = {
                let Some(guest) = self.guests.get_mut(&id) else {
                    return;
                };
                if guest.entry_ready
                    || guest
                        .entry_required
                        .iter()
                        .any(|position| !guest.sent_chunks.contains(position))
                {
                    return;
                }
                guest.entry_ready = true;
                (roster::guest_presence(id, guest), guest.name.clone())
            };
            self.net.send(id, &S2C::EntryAccepted);
            self.broadcast_ready(&S2C::Joined {
                presence: accepted.0,
            });
            fx.push(HostFx::Joined(accepted.1));
            return;
        }
        // An authenticated connection is inert until it has acknowledged
        // every host-declared entry chunk. In particular it cannot move,
        // chat, moderate, request arbitrary terrain, or affect simulation.
        if self.guests.get(&id).is_none_or(|guest| !guest.entry_ready) {
            return;
        }
        if let C2S::Moderate { target, action } = &msg {
            self.on_moderation_request(id, *target, *action);
            return;
        }
        // Both of these are answered before the long-lived guest borrow
        // below, because both have to reach back into the session while
        // holding it would forbid that.
        match &msg {
            C2S::RequestChunk { face, u, v } => {
                let requested = crate::planet::Face::from_u8(*face)
                    .and_then(|face| ChunkPos::new(face, *u, *v).ok());
                let serve = {
                    let Some(g) = self.guests.get_mut(&id) else {
                        return;
                    };
                    if g.chunk_requests >= CHUNK_REQUESTS_PER_SECOND {
                        false
                    } else {
                        g.chunk_requests += 1;
                        // Only ground this guest could be standing near. The
                        // request fills its own holes; it is not a way to
                        // read the map from across the world.
                        let Some(center) = g.pos.chunk() else {
                            return;
                        };
                        requested.is_some_and(|pos| {
                            pos.distance(center) <= f64::from((g.view_dist + 2) * 16)
                        })
                    }
                };
                if serve && let Some(pos) = requested {
                    self.stream_chunk(server, id, pos);
                }
                return;
            }
            C2S::SetViewDistance { chunks } => {
                let granted = (*chunks).clamp(2, MAX_GUEST_VIEW_DIST) as i32;
                let Some(g) = self.guests.get_mut(&id) else {
                    return;
                };
                // Unchanged is free, so a client that resends every frame
                // costs nothing.
                if g.view_dist == granted {
                    return;
                }
                g.view_dist = granted;
                self.net.send(
                    id,
                    &S2C::ViewDistance {
                        chunks: granted as u8,
                    },
                );
                return;
            }
            _ => {}
        }
        let implement_observers = if matches!(
            &msg,
            C2S::OperateBindingFrame { .. }
                | C2S::OperateWorking { .. }
                | C2S::OperateAlchemy { .. }
                | C2S::UsePreparation { .. }
        ) {
            self.guests
                .iter()
                .filter(|(_, guest)| guest.entry_ready)
                .map(|(observer, guest)| (*observer, guest.pos))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let Some(guest) = self.guests.get_mut(&id) else {
            return;
        };
        if !matches!(&msg, C2S::Move { .. }) {
            // The window is SIM time: when the host runs slow, a
            // second stretches, and an honest busy guest (an agent
            // mid-craft) must still fit inside it. Abuse is orders
            // of magnitude past this.
            if guest.command_count >= 160 {
                return;
            }
            guest.command_count += 1;
        }
        match msg {
            // Handled above, before the guest borrow.
            C2S::Hello { .. }
            | C2S::Authenticate { .. }
            | C2S::Moderate { .. }
            | C2S::RequestChunk { .. }
            | C2S::SetViewDistance { .. }
            | C2S::EntryReady
            | C2S::Bye => {}
            C2S::Move {
                pos,
                yaw,
                hotbar,
                sprint,
            } => {
                let elapsed = guest.net_age.clamp(0.03, 0.3);
                let delta = guest.pos.local_delta_to(pos);
                let horizontal = Vec3::new(delta.x, 0.0, delta.z).length();
                let probe = crate::physics::Player::new_at(pos);
                let grounded = pos
                    .translated(Vec3::new(0.0, -0.05, 0.0))
                    .ok()
                    .and_then(|p| p.pos.block())
                    .is_some_and(|p| server.world.reg.is_solid(server.world.get_block_at(p)));
                let in_water = pos
                    .translated(Vec3::new(0.0, 0.6, 0.0))
                    .ok()
                    .and_then(|p| p.pos.block())
                    .is_some_and(|p| server.world.reg.is_water(server.world.get_block_at(p)));
                let airborne_rise = if grounded || in_water {
                    0.0
                } else {
                    guest.airborne_rise + delta.y.max(0.0)
                };
                let valid = pos.is_canonical()
                    && yaw.is_finite()
                    && hotbar < HOTBAR_SLOTS as u8
                    && !probe.collides(&server.world, probe.pos)
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
                if let Some(at) = pos.block() {
                    server.world.tread_at(at);
                }
            }
            C2S::Break { pos } => {
                if guest.pos.distance_to(pos.entity_center()) > REACH
                    || guest.edits >= EDITS_PER_SEC
                {
                    return;
                }
                guest.edits += 1;
                let creative = server.world.mode == "creative";
                let held = guest.inventory.slots[guest.hotbar].map(|stack| stack.item);
                let sheared = held.is_some_and(|item| server.world.reg.item(item).shears)
                    && server
                        .world
                        .reg
                        .block(server.world.get_block_at(pos))
                        .name
                        .contains("leaves");
                let Some(result) =
                    server
                        .world
                        .break_block_at(pos, held, !creative && !sheared, !creative)
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
                if !creative
                    && let Some(stack) =
                        server
                            .world
                            .roll_bonus_drop_at(pos, result.block, &mut server.rng)
                {
                    server.world.queue_give(id, stack);
                }
                self.send_player_state(id);
            }
            C2S::Scoop { pos } => {
                if guest.pos.distance_to(pos.entity_center()) > REACH
                    || guest.edits >= EDITS_PER_SEC
                {
                    return;
                }
                // Only a full cell fills a bucket — partials would let
                // a guest mint fluid out of films. Either fluid dips.
                let b = server.world.get_block_at(pos);
                if server.world.reg.fluid_volume(b) != Some(8) {
                    return;
                }
                let water_class = server
                    .world
                    .water_mass_at(pos)
                    .map(|mass| mass.water_class());
                let full_name = if server.world.reg.is_lava(b) {
                    "base:bucket_lava"
                } else {
                    match water_class {
                        Some(crate::planet_atlas::WaterClass::Brackish) => "base:bucket_brackish",
                        Some(crate::planet_atlas::WaterClass::Salt) => "base:bucket_salt",
                        _ => "base:bucket_water",
                    }
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
                if server.world.reg.is_lava(b) {
                    server.world.set_block_at(pos, crate::registry::AIR);
                } else if server.world.scoop_water_at(pos).is_none() {
                    return;
                }
                if server.world.mode != "creative"
                    && let Some(full) = server.world.reg.item_id(full_name)
                {
                    guest.inventory.slots[guest.hotbar] =
                        Some(ItemStack::new(&server.world.reg, full, 1));
                    refresh_held(guest);
                    self.send_player_state(id);
                }
            }
            C2S::Place { pos } => {
                if guest.pos.distance_to(pos.entity_center()) > REACH
                    || guest.edits >= EDITS_PER_SEC
                {
                    return;
                }
                let selected = guest.inventory.slots[guest.hotbar];
                let creative = server.world.mode == "creative";
                let block = selected
                    .and_then(|stack| server.world.reg.item(stack.item).places)
                    .or_else(|| {
                        let item = selected.map(|stack| stack.item)?;
                        let reg = &server.world.reg;
                        if Some(item) == reg.item_id("base:bucket_water")
                            || Some(item) == reg.item_id("base:bucket_brackish")
                            || Some(item) == reg.item_id("base:bucket_salt")
                        {
                            Some(reg.water_block(0))
                        } else if Some(item) == reg.item_id("base:bucket_lava") {
                            Some(reg.lava_for_volume(8))
                        } else {
                            None
                        }
                    });
                let Some(block) = block else { return };
                let overlaps = {
                    let player = crate::physics::Player::new_at(guest.pos);
                    player.overlaps_block_at(pos)
                };
                if overlaps {
                    return;
                }
                let held_item = selected.map(|stack| stack.item);
                let water_class = if held_item == server.world.reg.item_id("base:bucket_water") {
                    Some(crate::planet_atlas::WaterClass::Fresh)
                } else if held_item == server.world.reg.item_id("base:bucket_brackish") {
                    Some(crate::planet_atlas::WaterClass::Brackish)
                } else if held_item == server.world.reg.item_id("base:bucket_salt") {
                    Some(crate::planet_atlas::WaterClass::Salt)
                } else {
                    None
                };
                let placed = if let Some(class) = water_class {
                    server.world.place_portable_water_at(pos, class)
                } else if held_item == server.world.reg.item_id("base:bucket_lava") {
                    server.world.place_block_at(pos, block)
                } else if !creative {
                    selected.is_some_and(|stack| server.world.place_item_block_at(pos, stack))
                } else {
                    server.world.place_block_at(pos, block)
                };
                if !placed {
                    return;
                }
                guest.edits += 1;
                if !creative {
                    let full_bucket = held_item == server.world.reg.item_id("base:bucket_water")
                        || held_item == server.world.reg.item_id("base:bucket_brackish")
                        || held_item == server.world.reg.item_id("base:bucket_salt")
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
            C2S::AttackMob { id: mob_id, heavy } => {
                // Stable ids: snapshots lag the sim, so an index would
                // race deaths/spawns and strike the wrong creature.
                if guest.action_cooldown > 0.0 {
                    return;
                }
                guest.action_cooldown = 0.35;
                let held = guest.inventory.slots[guest.hotbar];
                let dmg = held
                    .map(|stack| server.world.reg.item(stack.item).damage)
                    .unwrap_or(1.0)
                    .clamp(0.0, 16.0);
                let dmg_type =
                    held.and_then(|stack| server.world.reg.item(stack.item).damage_type.clone());
                let from = guest
                    .pos
                    .translated(Vec3::new(0.0, 1.6, 0.0))
                    .expect("guest attack origin stays in the shell")
                    .pos;
                let gpos = guest.pos;
                let reg = server.world.reg.clone();
                if let Some(m) = server.world.mob_by_id_mut(mob_id)
                    && m.pos.distance_to(gpos) <= REACH
                {
                    let def = reg.animals[m.species].clone();
                    let surface = m.pos.surface();
                    let mut final_dmg = dmg;
                    let mut crit = false;
                    if heavy {
                        final_dmg *= crate::game::combat::HEAVY_MULT;
                        crit = true;
                    }
                    if crate::game::combat::mob_facing_away(m.yaw, m.pos, gpos) {
                        final_dmg *= crate::game::combat::BACKSTAB_MULT;
                        crit = true;
                    }
                    m.hurt(&def, final_dmg, dmg_type.as_deref(), from);
                    m.last_hit_by = id;
                    if !def.hostile {
                        server.world.add_ire_at_surface(surface, 2.0);
                    }
                    if server.world.mode != "creative" {
                        guest.hunger = (guest.hunger - 0.01).max(0.0);
                        guest.inventory.wear_tool(&reg, guest.hotbar);
                        refresh_held(guest);
                        self.send_player_state(id);
                    }
                    // Report the authoritative number for the guest's
                    // floating damage feedback.
                    self.net.send(
                        id,
                        &net::S2C::MobHit {
                            id: mob_id,
                            dmg: final_dmg,
                            crit,
                        },
                    );
                }
            }
            C2S::FeedMob { id: mob_id } => {
                let gpos = guest.pos;
                let reg = server.world.reg.clone();
                if let Some(m) = server.world.mob_by_id_mut(mob_id) {
                    let def = &reg.animals[m.species];
                    let can_breed = m.breed_cd <= 0.0 && !m.fed;
                    let can_tame = !m.tamed;
                    if (m.pos - gpos).length() <= REACH
                        && !def.hostile
                        && def.breed_food.is_some()
                        && m.growth >= 1.0
                        && (can_breed || can_tame)
                        && guest.inventory.slots[guest.hotbar].map(|stack| stack.item)
                            == def.breed_food
                    {
                        if can_tame {
                            m.feed_tame();
                        }
                        if can_breed {
                            m.fed = true;
                        }
                        m.calm = 30.0;
                        if server.world.mode != "creative" {
                            let consumed = guest.inventory.slots[guest.hotbar]
                                .map(|stack| ItemStack::new(&reg, stack.item, 1));
                            if let Some(stack) = consumed
                                && let Err(error) = server.world.record_consumed_stacks([stack])
                            {
                                eprintln!(
                                    "materials: guest animal feed accounting failed: {error}"
                                );
                            }
                            guest.inventory.take_one(guest.hotbar);
                            refresh_held(guest);
                            self.send_player_state(id);
                        }
                    }
                }
            }
            C2S::HackMob { id: mob_id } => {
                let gpos = guest.pos;
                let reg = server.world.reg.clone();
                let held = guest.inventory.slots[guest.hotbar].map(|s| s.item);
                if let Some(m) = server.world.mob_by_id(mob_id)
                    && (m.pos - gpos).length() <= REACH
                    && let Some(def) = reg.animals.get(m.species)
                    && let Some(hack) = &def.hack
                    && let Some(tool) = hack.tool.as_deref()
                    && held.is_some_and(|i| {
                        let item = reg.item(i);
                        match tool {
                            "hack" => item.hack,
                            other => item.name.ends_with(&format!(":{other}")),
                        }
                    })
                {
                    if let Some(index) = server.world.mobs().iter().position(|x| x.id == mob_id) {
                        server.world.hack_mob(index, &mut server.rng);
                    }
                    guest.action_cooldown = 0.5;
                }
            }
            C2S::LeadMob { id: mob_id } => {
                let gpos = guest.pos;
                let lead = server.world.reg.item_id("base:lead");
                let holding = guest.inventory.slots[guest.hotbar].map(|s| s.item);
                if let Some(m) = server.world.mob_by_id_mut(mob_id)
                    && m.tamed
                    && m.led_by.is_none()
                    && (m.pos - gpos).length() <= REACH
                    && holding == lead
                {
                    m.led_by = Some(id);
                    if server.world.mode != "creative"
                        && let Some(lead) = lead
                    {
                        take_item(&mut guest.inventory, lead);
                        refresh_held(guest);
                    }
                } else if let Some(m) = server.world.mob_by_id_mut(mob_id)
                    && m.led_by == Some(id)
                {
                    // Second use releases; the strip comes back.
                    m.led_by = None;
                    if let Some(lead) = lead {
                        let reg = server.world.reg.clone();
                        let _ = guest.inventory.add(&reg, lead, 1);
                        refresh_held(guest);
                    }
                }
            }
            C2S::SaddleMob { id: mob_id } => {
                let gpos = guest.pos;
                let reg = server.world.reg.clone();
                let bags = reg.item_id("base:saddlebags");
                let holding = guest.inventory.slots[guest.hotbar].map(|s| s.item);
                if let Some(m) = server.world.mob_by_id_mut(mob_id)
                    && m.tamed
                    && reg.animals[m.species].carrier
                    && m.cargo.is_none()
                    && (m.pos - gpos).length() <= REACH
                    && holding == bags
                {
                    m.cargo = Some(Default::default());
                    if server.world.mode != "creative"
                        && let Some(bags) = bags
                    {
                        take_item(&mut guest.inventory, bags);
                        refresh_held(guest);
                    }
                }
            }
            C2S::OpenMobCargo { id: mob_id } => {
                let gpos = guest.pos;
                if let Some(m) = server.world.mob_by_id(mob_id)
                    && m.tamed
                    && m.cargo.is_some()
                    && (m.pos - gpos).length() <= REACH
                {
                    guest.mob_cargo = Some(mob_id);
                    self.send_mob_cargo(server, id, mob_id);
                }
            }
            C2S::MobCargoClick {
                id: mob_id,
                slot,
                right,
            } => {
                if guest.mob_cargo != Some(mob_id) || slot >= 12 {
                    return;
                }
                let reg = server.world.reg.clone();
                let mut held = guest.cursor;
                if let Some(m) = server.world.mob_by_id_mut(mob_id)
                    && let Some(cargo) = m.cargo.as_mut()
                {
                    let (ns, nh) = click_stack(&reg, cargo[slot as usize], held, right);
                    cargo[slot as usize] = ns;
                    held = nh;
                }
                let snap = held.map(|s| StackSnap {
                    item: s.item.0,
                    count: s.count,
                    durability: s.durability,
                    arcane_id: s.arcane_id,
                    current_units: 0,
                });
                if let Some(g) = self.guests.get_mut(&id) {
                    g.cursor = held;
                }
                self.net.send(id, &S2C::HeldResult(snap));
                self.send_mob_cargo(server, id, mob_id);
            }
            C2S::RideMob { id: mob_id, mount } => {
                let gpos = guest.pos;
                let reg = server.world.reg.clone();
                if let Some(m) = server.world.mob_by_id_mut(mob_id)
                    && reg.animals[m.species].vehicle
                {
                    if mount && m.ridden_by.is_none() && (m.pos - gpos).length() <= REACH {
                        m.ridden_by = Some(id);
                    } else if !mount && m.ridden_by == Some(id) {
                        m.ridden_by = None;
                    }
                }
            }
            C2S::StallBuy { pos } => {
                if guest.pos.distance_to(pos.entity_center()) > REACH
                    || !server.world.check_stall_at(pos)
                {
                    return;
                }
                let reg = server.world.reg.clone();
                // A banned seller's stall stops trading (their goods
                // stay theirs).
                let banned_owner = |owner: [u8; 16]| {
                    self.moderation
                        .as_ref()
                        .is_some_and(|m| m.player_banned(crate::identity::PlayerId(owner)))
                };
                let Some(BlockEntity::Stall(stall)) = server.world.block_entity_mut_at(&pos) else {
                    return;
                };
                if stall.owner == [0; 16] || banned_owner(stall.owner) {
                    return;
                }
                let Ok(purchase) = crate::player_ops::trade::purchase(&reg, stall, &mut guest.inventory) else {
                    return;
                };
                if let Some(stack) = purchase.overflow
                    && let Some(at) = guest.pos.block()
                {
                    server.world.push_drop_at(at, stack);
                }
                refresh_held(guest);
                self.send_player_state(id);
                self.send_container(server, id, pos);
            }
            C2S::SetSign { pos, lines } => {
                if guest.pos.distance_to(pos.entity_center()) > REACH {
                    return;
                }
                let b = server.world.get_block_at(pos);
                let station = server.world.reg.block(b).interaction.as_deref();
                if !matches!(station, Some("sign") | Some("waystone")) {
                    return;
                }
                let mut lines = lines;
                for l in lines.iter_mut() {
                    l.truncate(14);
                    l.retain(|c| c.is_ascii_alphanumeric() || " :_-'".contains(c));
                }
                server.world.insert_block_entity_at(
                    pos,
                    BlockEntity::Sign(crate::world::SignState {
                        lines: lines.clone(),
                    }),
                );
                self.broadcast_ready(&S2C::SignText { pos, lines });
            }
            C2S::DepotDeposit { pos } => {
                // Capability E13: a guest delivers held goods to a depot.
                // The host validates the need against its own registry,
                // consumes from the guest's inventory, and pays the
                // reputation through HostFx (the KV namespace lives in the
                // windowed host).
                if guest.pos.distance_to(pos.entity_center()) > REACH {
                    return;
                }
                let interaction = server
                    .world
                    .reg
                    .block(server.world.get_block_at(pos))
                    .interaction
                    .clone();
                let Some(interaction) = interaction else {
                    return;
                };
                if !interaction.starts_with("depot:") {
                    return;
                }
                let Some(held) = guest.inventory.slots[guest.hotbar] else {
                    return;
                };
                let Some((settlement, item, units, rep_per_unit)) =
                    server
                        .world
                        .deliver_to_depot(pos, &mut guest.inventory, guest.hotbar)
                else {
                    self.net.send(
                        id,
                        &S2C::Toast(format!(
                            "The depot has no appetite for {} right now.",
                            server.world.reg.item(held.item).label
                        )),
                    );
                    return;
                };
                refresh_held(guest);
                self.send_player_state(id);
                self.net.send(
                    id,
                    &S2C::SettlementDelivery {
                        settlement,
                        item: server.world.reg.item(item).name.clone(),
                        units,
                        rep_per_unit,
                    },
                );
            }
            C2S::ScreenClick { screen, action } => {
                // Capability E11: a mod-screen button click. Both ids are
                // validated against the host's own registry, so a tampered
                // client can only ever name buttons that exist. Scripts
                // live on the windowed host, so the click rides HostFx.
                let Some(def) = server.world.reg.screens.iter().find(|s| s.id == screen) else {
                    return;
                };
                if !def.rows().iter().any(
                    |row| matches!(row, crate::screens::ScreenWidget::Button { action: a, .. } if a == &action),
                ) {
                    return;
                }
                fx.push(HostFx::ScreenClick { screen, action });
            }
            C2S::ToggleSwitch { pos } => {
                if guest.pos.distance_to(pos.entity_center()) > REACH {
                    return;
                }
                let b = server.world.get_block_at(pos);
                let interaction = server.world.reg.block(b).interaction.as_deref();
                if !matches!(interaction, Some("rail_switch") | Some("belt_switch")) {
                    return;
                }
                server.world.toggle_switch(pos);
                let selected = server
                    .world
                    .switch_selected(pos)
                    .unwrap_or(crate::planet::Direction4::North);
                self.broadcast_ready(&S2C::SwitchState {
                    pos,
                    selected: selected as u8,
                });
            }
            C2S::DungeonUse { pos, kind } => {
                if guest.pos.distance_to(pos.entity_center()) > REACH {
                    return;
                }
                let interaction = server
                    .world
                    .reg
                    .block(server.world.get_block_at(pos))
                    .interaction
                    .clone();
                match (kind, interaction.as_deref()) {
                    // Entry: stand the guest at their run's spawn point.
                    (0, Some(s)) if s.starts_with("dungeon_entry:") => {
                        let name = s.trim_start_matches("dungeon_entry:").to_string();
                        if let Some(spawn) = server.world.enter_dungeon(id, guest.pos, &name) {
                            if let Some(g) = self.guests.get_mut(&id) {
                                g.pos = spawn;
                                self.net.send(
                                    id,
                                    &S2C::PlayerState(PlayerRuntime::from_guest(g).to_snap()),
                                );
                            }
                            self.net.send(
                                id,
                                &S2C::Toast("The dark takes you. The door is behind you.".into()),
                            );
                        }
                    }
                    // Exit: route back to the participant's own door.
                    (1, Some("dungeon_exit")) => {
                        if let Some(back) = server.world.exit_dungeon(id, guest.pos) {
                            if let Some(g) = self.guests.get_mut(&id) {
                                g.pos = back;
                                self.net.send(
                                    id,
                                    &S2C::PlayerState(PlayerRuntime::from_guest(g).to_snap()),
                                );
                            }
                            self.net.send(
                                id,
                                &S2C::Toast("Daylight again. The deep forgets you.".into()),
                            );
                        }
                    }
                    // Checkpoint: party-shared, host-owned.
                    (2, Some("dungeon_checkpoint")) => {
                        server.world.set_dungeon_checkpoint(guest.pos);
                        self.net
                            .send(id, &S2C::Toast("The shrine remembers you.".into()));
                    }
                    _ => {}
                }
            }
            C2S::BrushBlock { pos } => {
                if guest.pos.distance_to(pos.entity_center()) > REACH {
                    return;
                }
                let b = server.world.get_block_at(pos);
                let archaeology = server.world.reg.block(b).brush.is_some();
                if !archaeology && !server.world.can_sift_salvage_at(pos) {
                    return;
                }
                if !guest.inventory.slots[guest.hotbar]
                    .is_some_and(|stack| server.world.reg.item(stack.item).brush_tool)
                {
                    return;
                }
                let found = if archaeology {
                    let mut r = server.rng;
                    let found = server.world.brush_block_at(pos, &mut r);
                    server.rng = r;
                    found
                } else {
                    match server.world.sift_salvage_at(pos) {
                        Ok(found) => found,
                        Err(error) => {
                            eprintln!("materials: guest regional salvage recovery failed: {error}");
                            None
                        }
                    }
                };
                if let Some(stack) = found {
                    server.world.queue_give(id, stack);
                    if !archaeology {
                        self.net.send(
                            id,
                            &S2C::Toast("The brush turns up usable buried stock.".into()),
                        );
                    }
                } else if !archaeology {
                    self.net.send(
                        id,
                        &S2C::Toast("Nothing recoverable gathers in this ground yet.".into()),
                    );
                }
                if server.world.mode != "creative" {
                    guest.inventory.wear_tool(&server.world.reg, guest.hotbar);
                    refresh_held(guest);
                    self.send_player_state(id);
                }
            }
            C2S::BeginObserve { target } => {
                guest.pending_discovery = None;
                let lens_ready = guest.action_cooldown <= 0.0
                    && guest.inventory.slots[guest.hotbar].is_some_and(|stack| {
                        server
                            .world
                            .reg
                            .item(stack.item)
                            .discovery
                            .as_ref()
                            .is_some_and(|definition| definition.kind == "tuning_lens")
                            && stack.arcane_id != 0
                    });
                let target_ready = match target {
                    net::DiscoveryTargetSnap::Region => guest.pos.block().is_some(),
                    net::DiscoveryTargetSnap::Block(pos) => {
                        discovery_reachable(&server.world, guest, pos)
                    }
                    net::DiscoveryTargetSnap::Held { slot } => guest
                        .inventory
                        .slots
                        .get(usize::from(slot))
                        .is_some_and(Option::is_some),
                };
                if lens_ready && target_ready {
                    guest.pending_discovery = Some(PendingDiscovery {
                        kind: PendingDiscoveryKind::Observation(target),
                        began: Instant::now(),
                    });
                } else {
                    self.net.send(
                        id,
                        &S2C::Toast("The tuning lens cannot begin settling on that target.".into()),
                    );
                }
            }
            C2S::Observe {
                target,
                ledger_slot,
                calibration_slot,
                label,
            } => {
                let settled = guest.pending_discovery.is_some_and(|pending| {
                    pending.settled_for(PendingDiscoveryKind::Observation(target), Instant::now())
                });
                guest.pending_discovery = None;
                if guest.action_cooldown > 0.0 || !settled {
                    self.net.send(
                        id,
                        &S2C::Toast(
                            "The reading was refused: hold the lens steady until it settles."
                                .into(),
                        ),
                    );
                    return;
                }
                let lens_slot = guest.hotbar;
                if !guest.inventory.slots[lens_slot].is_some_and(|stack| {
                    server
                        .world
                        .reg
                        .item(stack.item)
                        .discovery
                        .as_ref()
                        .is_some_and(|definition| definition.kind == "tuning_lens")
                        && stack.arcane_id != 0
                }) {
                    self.net
                        .send(id, &S2C::Toast("Hold a fitted tuning lens.".into()));
                    return;
                }
                let holder = match discovery_holder_id(
                    &mut server.world,
                    guest,
                    net::RecordHolderSnap::Inventory { slot: ledger_slot },
                ) {
                    Ok(holder) => holder,
                    Err(error) => {
                        self.net.send(id, &S2C::Toast(error));
                        return;
                    }
                };
                let calibration =
                    match discovery_calibration(&mut server.world, guest, calibration_slot) {
                        Ok(calibration) => calibration,
                        Err(error) => {
                            self.net.send(id, &S2C::Toast(error));
                            return;
                        }
                    };
                let measured = match target {
                    net::DiscoveryTargetSnap::Region => guest
                        .pos
                        .block()
                        .map(crate::world::ObservationTarget::Region),
                    net::DiscoveryTargetSnap::Block(pos) => {
                        discovery_reachable(&server.world, guest, pos)
                            .then_some(crate::world::ObservationTarget::Block(pos))
                    }
                    net::DiscoveryTargetSnap::Held { slot } => guest
                        .inventory
                        .slots
                        .get(usize::from(slot))
                        .copied()
                        .flatten()
                        .zip(guest.pos.block())
                        .map(|(stack, at)| crate::world::ObservationTarget::Item(stack, at)),
                };
                let Some(measured) = measured else {
                    self.net.send(
                        id,
                        &S2C::Toast("The target is not physically measurable from here.".into()),
                    );
                    return;
                };
                match server.world.record_observation(
                    holder,
                    (guest.player_id, &guest.name),
                    measured,
                    calibration,
                    label,
                    None,
                ) {
                    Ok(summary) => {
                        guest.action_cooldown = 1.25;
                        let at = guest.pos.block().unwrap_or(measured.position());
                        let spent =
                            server
                                .world
                                .wear_tuning_lens_at(at, &mut guest.inventory, lens_slot);
                        refresh_held(guest);
                        self.net.send(id, &S2C::DiscoveryReport(summary));
                        if spent {
                            self.net.send(
                                id,
                                &S2C::Toast(
                                    "The Wellglass element clouds; the fitted frame and plate remain."
                                        .into(),
                                ),
                            );
                        }
                        self.send_player_state(id);
                    }
                    Err(error) => self
                        .net
                        .send(id, &S2C::Toast(format!("Observation refused: {error}"))),
                }
            }
            C2S::ReadKnowledge { slot } => {
                let index = usize::from(slot);
                let Some(mut stack) = guest.inventory.slots.get(index).copied().flatten() else {
                    return;
                };
                let Some(at) = guest.pos.block() else {
                    return;
                };
                if let Err(error) = server.world.bind_discovery_stack_at(at, &mut stack) {
                    self.net.send(id, &S2C::Toast(error.to_string()));
                    return;
                }
                guest.inventory.slots[index] = Some(stack);
                if let Some(text) = server.world.discovery_artifact_text(&mut stack, at) {
                    guest.inventory.slots[index] = Some(stack);
                    self.net.send(
                        id,
                        &S2C::KnowledgeText {
                            instance_id: stack.arcane_id,
                            text,
                        },
                    );
                } else if let Ok(records) = server.world.discovery_summaries(stack.arcane_id, true)
                {
                    let capacity = if server
                        .world
                        .reg
                        .item(stack.item)
                        .discovery
                        .as_ref()
                        .is_some_and(|definition| definition.kind == "survey_folio")
                    {
                        crate::discovery::SURVEY_FOLIO_RECORDS
                    } else {
                        crate::discovery::FIELD_LEDGER_RECORDS
                    };
                    self.net.send(
                        id,
                        &S2C::DiscoveryRecords {
                            holder: net::RecordHolderSnap::Inventory { slot },
                            records,
                            capacity: capacity as u16,
                        },
                    );
                }
                self.send_player_state(id);
            }
            C2S::OpenDiscovery { holder } => {
                match discovery_holder_id(&mut server.world, guest, holder) {
                    Ok(object_id) => match server.world.discovery_summaries(object_id, true) {
                        Ok(records) => {
                            let capacity =
                                discovery_holder_capacity(&server.world, guest, holder) as u16;
                            self.net.send(
                                id,
                                &S2C::DiscoveryRecords {
                                    holder,
                                    records,
                                    capacity,
                                },
                            );
                        }
                        Err(error) => self.net.send(id, &S2C::Toast(error.to_string())),
                    },
                    Err(error) => self.net.send(id, &S2C::Toast(error)),
                }
            }
            C2S::CopyObservation {
                writing_pos,
                source,
                record_id,
                destination,
                include_location,
            } => {
                let writing_surface = discovery_reachable(&server.world, guest, writing_pos)
                    && server
                        .world
                        .reg
                        .block(server.world.get_block_at(writing_pos))
                        .discovery_fixture
                        .as_ref()
                        .is_some_and(|fixture| fixture.kind == "writing_surface")
                    && discovery_holder_at_writing_surface(source, writing_pos)
                    && discovery_holder_at_writing_surface(destination, writing_pos);
                if !writing_surface {
                    self.net.send(
                        id,
                        &S2C::Toast(
                            "Signed records can only be copied at a writing surface; placed folios must be adjacent."
                                .into(),
                        ),
                    );
                    return;
                }
                let source_id = match discovery_holder_id(&mut server.world, guest, source) {
                    Ok(value) => value,
                    Err(error) => {
                        self.net.send(id, &S2C::Toast(error));
                        return;
                    }
                };
                let destination_id =
                    match discovery_holder_id(&mut server.world, guest, destination) {
                        Ok(value) => value,
                        Err(error) => {
                            self.net.send(id, &S2C::Toast(error));
                            return;
                        }
                    };
                match server.world.copy_discovery_record(
                    source_id,
                    record_id,
                    destination_id,
                    include_location,
                ) {
                    Ok(_) => {
                        let _ = server.world.save_discovery();
                        self.net.send(id, &S2C::Toast("Observation copied.".into()));
                        if let Ok(records) = server.world.discovery_summaries(destination_id, true)
                        {
                            let capacity =
                                discovery_holder_capacity(&server.world, guest, destination) as u16;
                            self.net.send(
                                id,
                                &S2C::DiscoveryRecords {
                                    holder: destination,
                                    records,
                                    capacity,
                                },
                            );
                        }
                    }
                    Err(error) => self.net.send(id, &S2C::Toast(error.to_string())),
                }
            }
            C2S::BeginExperiment { pos, kind } => {
                guest.pending_discovery = None;
                let fixture_ready = guest.action_cooldown <= 0.0
                    && discovery_reachable(&server.world, guest, pos)
                    && server
                        .world
                        .reg
                        .block(server.world.get_block_at(pos))
                        .discovery_fixture
                        .as_ref()
                        .is_some_and(|fixture| {
                            fixture.kind == "experiment_apparatus"
                                && fixture.experiments.contains(&kind)
                        });
                let lens_ready = guest.inventory.slots[guest.hotbar].is_some_and(|stack| {
                    server
                        .world
                        .reg
                        .item(stack.item)
                        .discovery
                        .as_ref()
                        .is_some_and(|definition| definition.kind == "tuning_lens")
                        && stack.arcane_id != 0
                });
                if fixture_ready && lens_ready {
                    guest.pending_discovery = Some(PendingDiscovery {
                        kind: PendingDiscoveryKind::Experiment(pos, kind),
                        began: Instant::now(),
                    });
                } else {
                    self.net.send(
                        id,
                        &S2C::Toast("The controlled trial cannot begin at that apparatus.".into()),
                    );
                }
            }
            C2S::SetExperimentItem { pos, slot } => {
                if guest.action_cooldown > 0.0 || !discovery_reachable(&server.world, guest, pos) {
                    return;
                }
                let index = usize::from(slot);
                match server
                    .world
                    .exchange_experiment_item_at(pos, &mut guest.inventory, index)
                {
                    Ok(message) => {
                        guest.action_cooldown = 0.25;
                        refresh_held(guest);
                        self.net.send(id, &S2C::Toast(message));
                        self.send_player_state(id);
                    }
                    Err(error) => self.net.send(id, &S2C::Toast(error)),
                }
            }
            C2S::RunExperiment {
                pos,
                kind,
                ledger_slot,
                calibration_slot,
            } => {
                let settled = guest.pending_discovery.is_some_and(|pending| {
                    pending.settled_for(PendingDiscoveryKind::Experiment(pos, kind), Instant::now())
                });
                guest.pending_discovery = None;
                if !settled
                    || guest.action_cooldown > 0.0
                    || !discovery_reachable(&server.world, guest, pos)
                    || !server
                        .world
                        .reg
                        .block(server.world.get_block_at(pos))
                        .discovery_fixture
                        .as_ref()
                        .is_some_and(|fixture| {
                            fixture.kind == "experiment_apparatus"
                                && fixture.experiments.contains(&kind)
                        })
                {
                    if !settled {
                        self.net.send(
                            id,
                            &S2C::Toast(
                                "The trial was refused: hold the apparatus steady until it settles."
                                    .into(),
                            ),
                        );
                    }
                    return;
                }
                let lens_slot = guest.hotbar;
                if !guest.inventory.slots[lens_slot].is_some_and(|stack| {
                    server
                        .world
                        .reg
                        .item(stack.item)
                        .discovery
                        .as_ref()
                        .is_some_and(|definition| definition.kind == "tuning_lens")
                        && stack.arcane_id != 0
                }) {
                    self.net
                        .send(id, &S2C::Toast("Hold a fitted tuning lens.".into()));
                    return;
                }
                let sample = match server.world.experiment_sample_at(pos, kind) {
                    Ok(sample) => sample,
                    Err(error) => {
                        self.net.send(id, &S2C::Toast(error));
                        return;
                    }
                };
                let holder = match discovery_holder_id(
                    &mut server.world,
                    guest,
                    net::RecordHolderSnap::Inventory { slot: ledger_slot },
                ) {
                    Ok(value) => value,
                    Err(error) => {
                        self.net.send(id, &S2C::Toast(error));
                        return;
                    }
                };
                let calibration =
                    match discovery_calibration(&mut server.world, guest, calibration_slot) {
                        Ok(value) => value,
                        Err(error) => {
                            self.net.send(id, &S2C::Toast(error));
                            return;
                        }
                    };
                match server.world.record_observation(
                    holder,
                    (guest.player_id, &guest.name),
                    crate::world::ObservationTarget::Item(sample, pos),
                    calibration,
                    Some(kind.label().into()),
                    Some(kind),
                ) {
                    Ok(summary) => {
                        guest.action_cooldown = 1.25;
                        let spent =
                            server
                                .world
                                .wear_tuning_lens_at(pos, &mut guest.inventory, lens_slot);
                        refresh_held(guest);
                        self.net.send(id, &S2C::DiscoveryReport(summary));
                        if spent {
                            self.net.send(
                                id,
                                &S2C::Toast(
                                    "The Wellglass element clouds; the fitted frame and plate remain."
                                        .into(),
                                ),
                            );
                        }
                        self.send_player_state(id);
                    }
                    Err(error) => self.net.send(id, &S2C::Toast(error.to_string())),
                }
            }
            C2S::AssembleTuningLens { pos } => {
                if !discovery_reachable(&server.world, guest, pos)
                    || server
                        .world
                        .reg
                        .block(server.world.get_block_at(pos))
                        .discovery_fixture
                        .as_ref()
                        .is_none_or(|fixture| fixture.kind != "lens_assembly")
                {
                    return;
                }
                match server
                    .world
                    .assemble_tuning_lens_at(pos, &mut guest.inventory)
                {
                    Ok(_) => {
                        refresh_held(guest);
                        self.net.send(
                            id,
                            &S2C::Toast(
                                "The Wellglass settles against the Echo Slate plate.".into(),
                            ),
                        );
                        self.send_player_state(id);
                    }
                    Err(error) => self.net.send(id, &S2C::Toast(error)),
                }
            }
            C2S::OperateBindingFrame {
                pos,
                slot,
                action,
                expected_revision,
            } => {
                if guest.action_cooldown > 0.0
                    || !discovery_reachable(&server.world, guest, pos)
                    || server
                        .world
                        .reg
                        .block(server.world.get_block_at(pos))
                        .interaction
                        .as_deref()
                        != Some("binding_frame")
                {
                    return;
                }
                let index = usize::from(slot);
                let actor = guest.player_id.to_string();
                match server.world.operate_binding_frame(
                    pos,
                    &mut guest.inventory,
                    index,
                    action,
                    expected_revision,
                    &actor,
                ) {
                    Ok(result) => {
                        guest.action_cooldown = 0.25;
                        refresh_held(guest);
                        let event_pos = pos.entity_center();
                        let visual =
                            server
                                .world
                                .block_entity_at(&pos)
                                .and_then(|entity| match entity {
                                    crate::world::BlockEntity::BindingFrame(frame) => frame
                                        .output
                                        .and_then(|stack| server.world.implement_visual(stack)),
                                    _ => None,
                                });
                        for (observer, observer_pos) in &implement_observers {
                            if *observer != id
                                && observer_pos.horizontal_distance_to(event_pos) <= 96.0
                            {
                                self.net.send(
                                    *observer,
                                    &S2C::ImplementActivation {
                                        actor: id,
                                        pos: event_pos,
                                        cue: result.cue,
                                        visual,
                                    },
                                );
                            }
                        }
                        fx.push(HostFx::ImplementActivation {
                            pos: event_pos,
                            cue: result.cue,
                            visual,
                        });
                        self.net.send(id, &S2C::BindingFrameResult { pos, result });
                        self.send_player_state(id);
                    }
                    Err(error) => {
                        let revision = match server.world.block_entity_at(&pos) {
                            Some(crate::world::BlockEntity::BindingFrame(frame)) => frame.revision,
                            _ => 0,
                        };
                        self.net.send(
                            id,
                            &S2C::BindingFrameResult {
                                pos,
                                result: crate::implements::FrameResult {
                                    success: false,
                                    revision,
                                    cue: crate::implements::error_cue(&error),
                                    message: error,
                                    preview: None,
                                    lines: Vec::new(),
                                },
                            },
                        );
                    }
                }
            }
            C2S::OperateAlchemy {
                pos,
                expected_revision,
                action,
            } => {
                if guest.action_cooldown > 0.0
                    || !discovery_reachable(&server.world, guest, pos)
                    || !matches!(
                        server
                            .world
                            .reg
                            .block(server.world.get_block_at(pos))
                            .interaction
                            .as_deref(),
                        Some(
                            "alchemy_mortar"
                                | "alchemy_basin"
                                | "alchemy_alembic"
                                | "alchemy_filter"
                        )
                    )
                {
                    return;
                }
                let request = crate::alchemy::AlchemyRequest {
                    actor: guest.player_id.0,
                    actor_label: guest.name.clone(),
                    expected_revision,
                    action,
                };
                match server
                    .world
                    .operate_alchemy(pos, &mut guest.inventory, request)
                {
                    Ok(result) => {
                        guest.action_cooldown = 0.15;
                        refresh_held(guest);
                        let cue = result.cue.clone();
                        for (observer, observer_pos) in &implement_observers {
                            if *observer != id
                                && observer_pos.horizontal_distance_to(cue.pos.entity_center())
                                    <= 96.0
                            {
                                self.net.send(*observer, &S2C::AlchemyEvent(cue.clone()));
                            }
                        }
                        fx.push(HostFx::AlchemyEvent(cue));
                        self.net.send(id, &S2C::AlchemyResult { pos, result });
                        self.send_player_state(id);
                    }
                    Err(error) => self.net.send(id, &S2C::Toast(error)),
                }
            }
            C2S::UsePreparation { slot, target } => {
                if guest.action_cooldown > 0.0 {
                    return;
                }
                let Some(actor_pos) = guest.pos.block() else {
                    return;
                };
                let target_is_reachable = match target {
                    crate::alchemy::AlchemyTarget::SelfActor => true,
                    crate::alchemy::AlchemyTarget::Plot(pos)
                    | crate::alchemy::AlchemyTarget::Surface(pos) => {
                        discovery_reachable(&server.world, guest, pos)
                    }
                    crate::alchemy::AlchemyTarget::Item(item_id) => item_id != 0,
                };
                if !target_is_reachable {
                    return;
                }
                match server.world.use_preparation(
                    guest.player_id.0,
                    &guest.name,
                    actor_pos,
                    &mut guest.inventory,
                    usize::from(slot),
                    target,
                ) {
                    Ok(result) => {
                        guest.action_cooldown = 0.3;
                        refresh_held(guest);
                        let cue = result.cue.clone();
                        for (observer, observer_pos) in &implement_observers {
                            if *observer != id
                                && observer_pos.horizontal_distance_to(cue.pos.entity_center())
                                    <= 96.0
                            {
                                self.net.send(*observer, &S2C::AlchemyEvent(cue.clone()));
                            }
                        }
                        fx.push(HostFx::AlchemyEvent(cue));
                        self.net.send(id, &S2C::PreparationResult(result));
                        self.send_player_state(id);
                    }
                    Err(error) => self.net.send(id, &S2C::Toast(error)),
                }
            }
            C2S::OperateWorking {
                working_id,
                held_instance,
                target,
                intent,
            } => {
                if guest.action_cooldown > 0.0
                    && matches!(
                        intent,
                        crate::workings::WorkingIntent::Start
                            | crate::workings::WorkingIntent::StartForced
                    )
                {
                    return;
                }
                let prior = guest.active_working.and_then(|active| {
                    server
                        .world
                        .working_cues()
                        .into_iter()
                        .find(|cue| cue.stable_id == active)
                });
                match operate_guest_working(
                    &mut server.world,
                    guest,
                    &working_id,
                    held_instance,
                    target,
                    intent,
                ) {
                    Ok(mut result) => {
                        if result.phase == Some(crate::workings::WorkingPhase::PendingApply) {
                            let checkpoint = self
                                .profiles
                                .as_ref()
                                .ok_or_else(|| {
                                    "The authoritative profile store is unavailable.".to_string()
                                })
                                .and_then(|profiles| {
                                    profiles
                                        .save(&PlayerRuntime::from_guest(guest), &server.world.reg)
                                        .map_err(|error| error.to_string())
                                });
                            match checkpoint.and_then(|()| {
                                server.world.finish_inventory_working(result.stable_id)
                            }) {
                                Ok(finished) => result = finished,
                                Err(message) => {
                                    guest.active_working = Some(result.stable_id);
                                    self.net.send(
                                        id,
                                        &S2C::WorkingResult(crate::workings::WorkingResult {
                                            success: false,
                                            stable_id: result.stable_id,
                                            phase: Some(
                                                crate::workings::WorkingPhase::PendingApply,
                                            ),
                                            cue: crate::workings::WorkingCueKind::Strain,
                                            warning_band: result.warning_band,
                                            message: format!(
                                                "Fieldmend landed but its profile checkpoint failed: {message}"
                                            ),
                                        }),
                                    );
                                    return;
                                }
                            }
                        }
                        guest.action_cooldown = if matches!(
                            intent,
                            crate::workings::WorkingIntent::Start
                                | crate::workings::WorkingIntent::StartForced
                                | crate::workings::WorkingIntent::Release
                        ) {
                            crate::workings::WAND_RECOVERY_SECONDS
                        } else {
                            0.05
                        };
                        let mut cue = server
                            .world
                            .working_cues()
                            .into_iter()
                            .find(|cue| cue.stable_id == result.stable_id)
                            .or(prior);
                        if let Some(cue) = cue.as_mut()
                            && result.phase.is_none()
                        {
                            cue.kind = result.cue;
                            cue.completion_permille = 1_000;
                        }
                        if let Some(cue) = cue {
                            let event_pos = cue.source.entity_center();
                            for (observer, observer_pos) in &implement_observers {
                                if *observer != id
                                    && observer_pos.horizontal_distance_to(event_pos) <= 96.0
                                {
                                    self.net.send(*observer, &S2C::WorkingEvent(cue.clone()));
                                }
                            }
                            fx.push(HostFx::WorkingEvent(cue));
                        }
                        self.net.send(id, &S2C::WorkingResult(result));
                        self.send_player_state(id);
                    }
                    Err(message) => self.net.send(
                        id,
                        &S2C::WorkingResult(crate::workings::WorkingResult {
                            success: false,
                            stable_id: guest.active_working.unwrap_or_default(),
                            phase: None,
                            cue: crate::workings::WorkingCueKind::Refuse,
                            warning_band: 0,
                            message,
                        }),
                    ),
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
                let (speed, damage, tile, drop_item, preparation_payload) =
                    if let Some(bow) = def.bow {
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
                            None,
                        )
                    } else if let Some(speed) = def.throw_speed {
                        let state_bearing = selected.arcane_id != 0;
                        let removed = if creative && !state_bearing {
                            None
                        } else {
                            guest.inventory.take_one_stack(guest.hotbar)
                        };
                        if state_bearing && removed.is_none() {
                            return;
                        }
                        (
                            speed,
                            0.0,
                            def.icon,
                            None,
                            removed.filter(|stack| stack.arcane_id != 0),
                        )
                    } else {
                        return;
                    };
                guest.action_cooldown = 0.25;
                let pos = guest
                    .pos
                    .translated(Vec3::new(0.0, 1.6, 0.0) + direction * 0.4)
                    .expect("guest projectile starts beside the player")
                    .pos;
                server.world.spawn_projectile(crate::mobs::Projectile {
                    stable_id: 0,
                    pos,
                    vel: direction * speed.min(40.0),
                    tile,
                    damage: damage.clamp(0.0, 12.0),
                    damage_type: None,
                    age: 0.0,
                    from_player: true,
                    drop_item,
                    preparation_payload,
                    owner: id,
                });
                refresh_held(guest);
                self.send_player_state(id);
            }
            C2S::OpenContainer { pos } => {
                if guest.pos.distance_to(pos.entity_center()) > REACH {
                    return;
                }
                let b = server.world.get_block_at(pos);
                // Capability E7: machine interactions resolve to a
                // `MachineDef` and ride `S2C::MachineContainer`; the
                // classic containers keep their `S2C::Container` kind code.
                let machine =
                    server
                        .world
                        .reg
                        .block(b)
                        .interaction
                        .as_deref()
                        .and_then(|i| server.world.reg.machine_by_interaction(i))
                        .filter(|kind| {
                            server.world.reg.machine(*kind).is_some_and(|def| {
                                def.handler.has_fire() || def.handler.is_station()
                            })
                        });
                let kind = match (machine, server.world.reg.block(b).interaction.as_deref()) {
                    (Some(_), _) => 7u8,
                    (None, Some("chest")) => 0,
                    (None, Some("furnace")) => 1,
                    (None, Some("offering")) => 2,
                    (None, Some("stall")) => 6,
                    _ => return,
                };
                let default = if let Some(mkind) = machine {
                    BlockEntity::Multiblock(MachineInstance {
                        kind: mkind,
                        ..Default::default()
                    })
                } else {
                    match kind {
                        0 => BlockEntity::Chest(Default::default()),
                        1 => BlockEntity::Furnace(Default::default()),
                        6 => BlockEntity::Stall(Default::default()),
                        _ => BlockEntity::Offering(Default::default()),
                    }
                };
                let entry = server.world.ensure_block_entity_at(pos, default);
                // A fresh counter belongs to whoever opens it first.
                if let BlockEntity::Stall(st) = entry
                    && st.owner == [0; 16]
                {
                    st.owner = guest.player_id.0;
                    st.owner_name = guest.name.clone();
                }
                if let BlockEntity::Chest(c) = entry
                    && c.wild_owned
                {
                    c.wild_owned = false;
                    server.world.add_ire_at_surface(pos.surface(), 1.0);
                    self.net
                        .send(id, &S2C::Toast("The wild keeps its trophies.".into()));
                }
                if let Some(g) = self.guests.get_mut(&id) {
                    g.container = Some(pos);
                }
                self.send_container(server, id, pos);
            }
            C2S::ContainerClick { pos, slot, right } => {
                self.container_click(server, id, pos, slot as usize, right);
            }
            C2S::CloseContainer => {
                guest.container = None;
                guest.mob_cargo = None;
            }
            C2S::LightBloomery { pos } => {
                if guest.pos.distance_to(pos.entity_center()) > REACH {
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
                let b = server.world.get_block_at(pos);
                // Capability E7: light any fire handler's machine by its
                // interaction, not a hardcoded three-way match.
                let res = match server
                    .world
                    .reg
                    .block(b)
                    .interaction
                    .as_deref()
                    .and_then(|interaction| server.world.reg.machine_by_interaction(interaction))
                    .filter(|kind| {
                        server
                            .world
                            .reg
                            .machine(*kind)
                            .is_some_and(|def| def.handler.has_fire())
                    }) {
                    Some(kind) => {
                        let matched = match kind.validate(&server.world, pos) {
                            Some(matched) => matched,
                            None => {
                                self.net
                                    .send(id, &S2C::Toast("the stack is breached".into()));
                                return;
                            }
                        };
                        crate::world::machines::light_machine_at(
                            &mut server.world,
                            pos,
                            kind,
                            matched,
                        )
                    }
                    None => return,
                };
                match res {
                    Ok(()) => {
                        if server.world.mode != "creative"
                            && let Some(ember) = server.world.reg.item_id("base:ember")
                            && let Some(slot) =
                                guest.inventory.slots.iter().position(|stack| {
                                    stack.is_some_and(|stack| stack.item == ember)
                                })
                            && let Some(stack) = guest.inventory.slots[slot]
                        {
                            guest.inventory.take_one(slot);
                            server.world.retire_arcane_stack_at(
                                pos,
                                ItemStack { count: 1, ..stack },
                                "high-heat station ignition",
                            );
                            refresh_held(guest);
                            self.send_player_state(id);
                        }
                        self.send_container(server, id, pos);
                    }
                    Err(e) => self.net.send(id, &S2C::Toast(e.into())),
                }
            }
            C2S::LightClamp { pos } => {
                if guest.pos.distance_to(pos.entity_center()) > REACH {
                    return;
                }
                if server.world.mode != "creative"
                    && guest.inventory.slots[guest.hotbar].map(|stack| stack.item)
                        != server.world.reg.item_id("base:ember")
                {
                    return;
                }
                match server.world.try_light_clamp_at(pos) {
                    Ok(n) => {
                        if server.world.mode != "creative" {
                            let consumed = guest.inventory.slots[guest.hotbar];
                            guest.inventory.take_one(guest.hotbar);
                            if let Some(stack) = consumed {
                                server.world.retire_arcane_stack_at(
                                    pos,
                                    ItemStack { count: 1, ..stack },
                                    "clamp ignition",
                                );
                            }
                            refresh_held(guest);
                            self.send_player_state(id);
                        }
                        self.net
                            .send(id, &S2C::Toast(format!("The clamp smolders ({n} logs).")));
                    }
                    Err(e) => self.net.send(id, &S2C::Toast(e.into())),
                }
            }
            C2S::AnvilPut { pos } => {
                if guest.pos.distance_to(pos.entity_center()) > REACH {
                    return;
                }
                let Some(stack) = guest.inventory.slots[guest.hotbar] else {
                    return;
                };
                let one = ItemStack { count: 1, ..stack };
                if server.world.anvil_put_at(pos, one) && server.world.mode != "creative" {
                    guest.inventory.take_one(guest.hotbar);
                    refresh_held(guest);
                    self.send_player_state(id);
                }
            }
            C2S::AnvilStrike { pos } => {
                if guest.pos.distance_to(pos.entity_center()) > REACH {
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
                if let Some(out) = server.world.anvil_strike_at(pos) {
                    server.world.queue_give(id, out);
                }
                self.send_player_state(id);
            }
            C2S::AnvilTake { pos } => {
                if guest.pos.distance_to(pos.entity_center()) > REACH {
                    return;
                }
                if let Some(b) = server.world.anvil_take_at(pos) {
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
                        crate::player_ops::equipment::exchange(
                            reg, &mut guest.armor, &mut guest.cursor, slot,
                        );
                    }
                    _ => return,
                }
                refresh_held(guest);
                self.send_player_state(id);
            }
            C2S::CraftResult { size } => {
                let Ok(effects) = crate::player_ops::craft::take_result(
                    &server.world.reg,
                    usize::from(size),
                    &mut guest.craft_grid,
                    &mut guest.inventory,
                    &mut guest.cursor,
                ) else {
                    return;
                };
                effects.finish(
                    &mut server.world,
                    guest.pos.block(),
                    &mut guest.inventory,
                    true,
                    "full guest inventory after crafting",
                );
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
                    let consumed = ItemStack::new(&server.world.reg, stack.item, 1);
                    if let Err(error) = server.world.record_consumed_stacks([consumed]) {
                        eprintln!("materials: guest eaten food accounting failed: {error}");
                    }
                    guest.inventory.take_one(guest.hotbar);
                }
                refresh_held(guest);
                self.send_player_state(id);
            }
            C2S::Respawn => {
                if guest.health > 0.0 {
                    return;
                }
                // A dungeon death (capability E10) wakes at the party's
                // checkpoint with belongings intact; the host never
                // scattered them.
                if let Some(cp) = server.world.dungeon_checkpoint_for(guest.pos) {
                    guest.pos = cp;
                    guest.health = 14.0;
                    guest.hunger = 20.0;
                    guest.since_damage = 100.0;
                    self.send_player_state(id);
                    return;
                }
                // The saved spawn may be buried or dug out by now.
                guest.pos = server.world.settle_spawn_at(guest.spawn);
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
        pos: BlockPos,
        slot: usize,
        right: bool,
    ) {
        let reg = server.world.reg.clone();
        let Some(guest) = self.guests.get(&id).filter(|guest| guest.container == Some(pos)) else {
            return;
        };
        let actor = guest.player_id.0;
        let mut held = guest.cursor;
        let Some(entity) = server.world.block_entity_mut_at(&pos) else { return; };
        let result = crate::player_ops::container::click(
            &reg, entity, &mut held,
            crate::player_ops::container::Click { slot, right, actor: Some(actor) },
        );
        // Depots retain their deposit-only request path. Other rejected clicks
        // still receive the unchanged cursor/container echo, as before.
        if result == Err(crate::player_ops::container::Rejected::DepositOnly) { return; }
        let snap = held.map(|s| StackSnap {
            item: s.item.0,
            count: s.count,
            durability: s.durability,
            arcane_id: s.arcane_id,
            current_units: 0,
        });
        if let Some(guest) = self.guests.get_mut(&id) {
            guest.cursor = held;
        }
        self.net.send(id, &S2C::HeldResult(snap));
        self.send_player_state(id);
        self.send_container(server, id, pos);
    }

    pub fn broadcast_sign_at(&mut self, pos: BlockPos, lines: &[String; 3]) {
        self.broadcast_ready(&S2C::SignText {
            pos,
            lines: lines.clone(),
        });
    }

    fn send_mob_cargo(&mut self, server: &Server, id: u32, mob_id: u32) {
        let Some(slots) = server
            .world
            .mob_by_id(mob_id)
            .and_then(|m| m.cargo.as_deref())
        else {
            return;
        };
        let slots = slots
            .iter()
            .map(|s| {
                s.map(|s| StackSnap {
                    item: s.item.0,
                    count: s.count,
                    durability: s.durability,
                    arcane_id: s.arcane_id,
                    current_units: server
                        .world
                        .inspectable_item_current(s.arcane_id)
                        .unwrap_or(0),
                })
            })
            .collect();
        self.net.send(id, &S2C::MobCargo { id: mob_id, slots });
    }

    fn send_container(&mut self, server: &Server, id: u32, pos: BlockPos) {
        let Some(entity) = server.world.block_entity_at(&pos) else {
            return;
        };
        let snap = |s: &Option<ItemStack>| {
            s.map(|s| StackSnap {
                item: s.item.0,
                count: s.count,
                durability: s.durability,
                arcane_id: s.arcane_id,
                current_units: server
                    .world
                    .inspectable_item_current(s.arcane_id)
                    .unwrap_or(0),
            })
        };
        let reg = server.world.reg.clone();
        // Capability E7: machine kinds ride `S2C::MachineContainer`, keyed
        // by the machine id (host and guest remap by id like the palette).
        // The layout comes from the def, so any data-driven kind works.
        if let BlockEntity::Multiblock(b) = entity {
            let Some(def) = reg.machine(b.kind) else {
                return;
            };
            let fire = def.fire_secs.max(1.0);
            let (slots, aux) = match def.handler {
                MachineHandler::Kiln => (
                    b.charge
                        .iter()
                        .chain([&b.reagent])
                        .chain(b.fuel.iter())
                        .map(snap)
                        .collect(),
                    vec![if b.lit { 1.0 } else { 0.0 }, b.progress / fire],
                ),
                MachineHandler::Workbench => (Vec::new(), Vec::new()),
                _ => (
                    b.charge.iter().chain(b.fuel.iter()).map(snap).collect(),
                    vec![if b.lit { 1.0 } else { 0.0 }, b.progress / fire],
                ),
            };
            self.net.send(
                id,
                &S2C::MachineContainer {
                    pos,
                    machine: def.id.clone(),
                    slots,
                    aux,
                },
            );
            return;
        }
        let (kind, slots, aux): (u8, Vec<Option<StackSnap>>, Vec<f32>) = match entity {
            // Depots have no container screen; deposits go through the
            // interaction arm and C2S::DepotDeposit.
            BlockEntity::Depot(_) => return,
            BlockEntity::Chest(c) => (0, c.slots.iter().map(snap).collect(), Vec::new()),
            BlockEntity::Furnace(f) => (
                1,
                vec![snap(&f.input), snap(&f.fuel), snap(&f.output)],
                vec![f.progress, f.burn_left, f.burn_total],
            ),
            BlockEntity::Offering(o) => (2, o.slots.iter().map(snap).collect(), Vec::new()),
            BlockEntity::Stall(st) => {
                let owner = self
                    .guests
                    .get(&id)
                    .is_some_and(|g| g.player_id.0 == st.owner);
                let mut slots: Vec<Option<StackSnap>> = st.goods.iter().map(snap).collect();
                slots.push(snap(&st.price));
                for t in st.till.iter() {
                    slots.push(if owner { snap(t) } else { None });
                }
                (6, slots, vec![if owner { 1.0 } else { 0.0 }])
            }
            BlockEntity::Clamp(_)
            | BlockEntity::Anvil(_)
            | BlockEntity::Sign(_)
            | BlockEntity::Smoker(_)
            | BlockEntity::Steam(_)
            | BlockEntity::Multiblock(_)
            | BlockEntity::SurveyFolio(_)
            | BlockEntity::DiscoveryApparatus(_)
            | BlockEntity::BindingFrame(_)
            | BlockEntity::ChargeVessel(_)
            | BlockEntity::Switch(_) => return,
        };
        self.net.send(
            id,
            &S2C::Container {
                pos,
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

fn discovery_reachable(world: &World, guest: &Guest, pos: BlockPos) -> bool {
    discovery_reachable_from(world, guest.pos, pos)
}

fn operate_guest_working(
    world: &mut World,
    guest: &mut Guest,
    working_id: &str,
    held_instance: u64,
    target: crate::workings::WorkingTargetIntent,
    intent: crate::workings::WorkingIntent,
) -> Result<crate::workings::WorkingResult, String> {
    use crate::workings::{WorkingHandler, WorkingIntent, WorkingTargetIntent};

    let source = guest
        .pos
        .block()
        .ok_or("The player is outside a valid working cell.")?;
    let actor = guest.player_id.0;
    match intent {
        WorkingIntent::Start | WorkingIntent::StartForced => {
            let forced = intent == WorkingIntent::StartForced;
            if guest.active_working.is_some() {
                return Err("Finish or cancel the working already in hand.".into());
            }
            if working_id == "base:auto_ritual" {
                if forced {
                    return Err("A physical ritual has no forced wand draw mode.".into());
                }
                let WorkingTargetIntent::Ritual { controller } = target else {
                    return Err("A contextual ritual requires its physical controller.".into());
                };
                if held_instance != 0 || !discovery_reachable(world, guest, controller) {
                    return Err("The ritual controller is out of sight or reach.".into());
                }
                return world.begin_contextual_ritual(actor, &guest.name, controller);
            }
            let definition = world
                .reg
                .workings
                .get(working_id)
                .cloned()
                .ok_or("That working is not registered on this host.")?;
            if definition.mode == crate::workings::DeliveryMode::Ritual {
                if forced {
                    return Err("A physical ritual has no forced wand draw mode.".into());
                }
                let WorkingTargetIntent::Ritual { controller } = target else {
                    return Err("A constructed ritual requires its physical controller.".into());
                };
                if held_instance != 0 {
                    return Err(
                        "Ritual authority belongs to its apparatus, not a held item.".into(),
                    );
                }
                if !discovery_reachable(world, guest, controller) {
                    return Err("The ritual controller is out of sight or reach.".into());
                }
                let result = world.begin_ritual(actor, &guest.name, working_id, controller)?;
                guest.active_working = Some(result.stable_id);
                return Ok(result);
            }
            let held = guest.inventory.slots[guest.hotbar]
                .ok_or("A physical wand must be held to begin a working.")?;
            if held.arcane_id == 0
                || held.arcane_id != held_instance
                || world
                    .reg
                    .item(held.item)
                    .implement
                    .as_ref()
                    .is_none_or(|definition| {
                        definition.kind != crate::implements::ImplementItemKind::Wand
                    })
            {
                return Err(
                    "The requested held instance is not the host-authoritative wand.".into(),
                );
            }
            if definition.mode != crate::workings::DeliveryMode::Wand {
                return Err(
                    "A constructed ritual cannot be requested as a held wand working.".into(),
                );
            }
            let result = world.begin_wand_working(
                actor,
                &guest.name,
                source,
                held_instance,
                working_id,
                target,
                Some(&guest.inventory),
                forced,
            )?;
            guest.active_working = Some(result.stable_id);
            Ok(result)
        }
        WorkingIntent::Hold | WorkingIntent::Release | WorkingIntent::Cancel => {
            let active = guest
                .active_working
                .ok_or("There is no active working to hold, release, or cancel.")?;
            let transaction = world
                .workings_state
                .as_ref()
                .and_then(|state| state.active.get(&active))
                .ok_or("The host no longer has that active working.")?;
            let apparatus_matches = match transaction.apparatus {
                crate::workings::WorkingApparatus::Wand { instance_id, .. } => {
                    instance_id == held_instance
                }
                crate::workings::WorkingApparatus::Ritual { controller, .. } => {
                    held_instance == 0
                        && matches!(target, WorkingTargetIntent::Ritual { controller: at } if at == controller)
                }
            };
            if transaction.actor != actor
                || transaction.definition.id != working_id
                || !apparatus_matches
            {
                return Err("Working identity, actor, or held apparatus no longer matches.".into());
            }
            let handler = transaction.definition.handler;
            if intent != WorkingIntent::Cancel && !world.wand_working_reachable_from(active, source)
            {
                guest.active_working = None;
                let mut result = world.interrupt_working(active)?;
                result.message =
                    "The wand path leaves its bounded reach and breaks cleanly.".into();
                return Ok(result);
            }
            let result = match intent {
                WorkingIntent::Hold => world.activate_working(active),
                WorkingIntent::Release if handler == WorkingHandler::Fieldmend => {
                    world.complete_inventory_working(active, &mut guest.inventory)
                }
                WorkingIntent::Release => world.release_working(active),
                WorkingIntent::Cancel => world.cancel_working(active),
                WorkingIntent::Start | WorkingIntent::StartForced => unreachable!(),
            }?;
            if !matches!(intent, WorkingIntent::Hold) {
                guest.active_working = None;
            }
            Ok(result)
        }
    }
}

fn discovery_reachable_from(world: &World, actor: EntityPos, pos: BlockPos) -> bool {
    if actor.distance_to(pos.entity_center()) > REACH {
        return false;
    }
    let Ok(eye) = actor.translated(Vec3::new(0.0, crate::physics::EYE_HEIGHT, 0.0)) else {
        return false;
    };
    let delta = eye.pos.local_delta_to(pos.entity_center());
    crate::raycast::raycast_at(world, eye.pos, delta, delta.length() + 0.15)
        .is_some_and(|hit| hit.block == pos)
}

fn discovery_holder_id(
    world: &mut World,
    guest: &mut Guest,
    holder: net::RecordHolderSnap,
) -> Result<u64, String> {
    match holder {
        net::RecordHolderSnap::Inventory { slot } => {
            let index = usize::from(slot);
            let mut stack = guest
                .inventory
                .slots
                .get(index)
                .copied()
                .flatten()
                .ok_or_else(|| "That pack slot is empty.".to_string())?;
            let is_record_holder =
                world
                    .reg
                    .item(stack.item)
                    .discovery
                    .as_ref()
                    .is_some_and(|definition| {
                        matches!(definition.kind.as_str(), "field_ledger" | "survey_folio")
                    });
            if !is_record_holder {
                return Err("That item cannot hold observations.".into());
            }
            let at = guest
                .pos
                .block()
                .ok_or_else(|| "Your position is outside the world.".to_string())?;
            world
                .bind_discovery_stack_at(at, &mut stack)
                .map_err(|error| error.to_string())?;
            guest.inventory.slots[index] = Some(stack);
            Ok(stack.arcane_id)
        }
        net::RecordHolderSnap::Folio { pos } => {
            if !discovery_reachable(world, guest, pos) {
                return Err("The folio is out of reach or sight.".into());
            }
            let is_folio = world
                .reg
                .block(world.get_block_at(pos))
                .discovery_fixture
                .as_ref()
                .is_some_and(|fixture| fixture.kind == "survey_folio");
            if !is_folio {
                return Err("There is no survey folio there.".into());
            }
            match world.block_entities().find(|(at, _)| **at == pos) {
                Some((_, BlockEntity::SurveyFolio(folio))) if folio.object_id != 0 => {
                    Ok(folio.object_id)
                }
                _ => Err("That survey folio has no recoverable record identity.".into()),
            }
        }
    }
}

fn discovery_holder_capacity(world: &World, guest: &Guest, holder: net::RecordHolderSnap) -> usize {
    match holder {
        net::RecordHolderSnap::Folio { .. } => crate::discovery::SURVEY_FOLIO_RECORDS,
        net::RecordHolderSnap::Inventory { slot } => {
            if guest
                .inventory
                .slots
                .get(usize::from(slot))
                .and_then(Option::as_ref)
                .and_then(|stack| world.reg.item(stack.item).discovery.as_ref())
                .is_some_and(|definition| definition.kind == "survey_folio")
            {
                crate::discovery::SURVEY_FOLIO_RECORDS
            } else {
                crate::discovery::FIELD_LEDGER_RECORDS
            }
        }
    }
}

fn discovery_holder_at_writing_surface(
    holder: net::RecordHolderSnap,
    writing_pos: BlockPos,
) -> bool {
    match holder {
        net::RecordHolderSnap::Inventory { .. } => true,
        net::RecordHolderSnap::Folio { pos } => [(1, 0), (-1, 0), (0, 1), (0, -1)]
            .into_iter()
            .filter_map(|(du, dv)| writing_pos.offset(du, 0, dv))
            .any(|adjacent| adjacent == pos),
    }
}

fn discovery_calibration(
    world: &mut World,
    guest: &mut Guest,
    slot: Option<u8>,
) -> Result<crate::discovery::CalibrationGrade, String> {
    let Some(slot) = slot else {
        return Ok(crate::discovery::CalibrationGrade::Field);
    };
    let index = usize::from(slot);
    let mut stack = guest
        .inventory
        .slots
        .get(index)
        .copied()
        .flatten()
        .ok_or_else(|| "That calibration slot is empty.".to_string())?;
    let at = guest
        .pos
        .block()
        .ok_or_else(|| "Your position is outside the world.".to_string())?;
    world
        .bind_discovery_stack_at(at, &mut stack)
        .map_err(|error| error.to_string())?;
    let calibration = world
        .calibration_grade_for(stack)
        .ok_or_else(|| "That is not a calibration plate.".to_string())?;
    guest.inventory.slots[index] = Some(stack);
    Ok(calibration)
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

fn inspectable_arcane_items(
    world: &World,
    guest: &Guest,
) -> (
    Vec<(u64, u64)>,
    Vec<crate::implements::ImplementPublicState>,
    Vec<crate::implements::ApparatusCue>,
) {
    let mut ids = guest
        .inventory
        .slots
        .iter()
        .chain(guest.armor.iter())
        .chain(std::iter::once(&guest.cursor))
        .chain(guest.craft_grid.iter())
        .flatten()
        .filter_map(|stack| (stack.arcane_id != 0).then_some(stack.arcane_id))
        .collect::<std::collections::BTreeSet<_>>();
    if let Some(pos) = guest.container
        && let Some(entity) = world.block_entity_at(&pos)
    {
        ids.extend(
            World::block_entity_stacks(entity)
                .into_iter()
                .filter_map(|stack| (stack.arcane_id != 0).then_some(stack.arcane_id)),
        );
    }
    if let Some(mob_id) = guest.mob_cargo
        && let Some(cargo) = world.mob_by_id(mob_id).and_then(|mob| mob.cargo.as_ref())
    {
        ids.extend(
            cargo
                .iter()
                .flatten()
                .filter_map(|stack| (stack.arcane_id != 0).then_some(stack.arcane_id)),
        );
    }
    let charges = ids
        .iter()
        .map(|id| (*id, world.inspectable_item_current(*id).unwrap_or(0)))
        .collect();
    let implements = ids
        .iter()
        .filter_map(|id| {
            let instance = world
                .implements_state
                .as_ref()
                .and_then(|state| state.instance(*id))?;
            let dross = world
                .arcane_ledger
                .as_ref()
                .map_or(0, |ledger| ledger.item_dross_total(*id));
            Some(crate::implements::ImplementPublicState::from_authority(
                instance, dross,
            ))
        })
        .collect();
    let mut apparatus = world.apparatus_cues_near(guest.pos, 48.0);
    apparatus.sort_by_key(|cue| cue.pos);
    (charges, implements, apparatus)
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

    #[test]
    fn discovery_completion_requires_a_matching_host_elapsed_ticket() {
        let began = Instant::now();
        let target = net::DiscoveryTargetSnap::Region;
        let pending = PendingDiscovery {
            kind: PendingDiscoveryKind::Observation(target),
            began,
        };
        assert!(!pending.settled_for(
            PendingDiscoveryKind::Observation(target),
            began + DISCOVERY_SETTLE - Duration::from_millis(1)
        ));
        assert!(!pending.settled_for(
            PendingDiscoveryKind::Observation(net::DiscoveryTargetSnap::Held { slot: 2 }),
            began + DISCOVERY_SETTLE
        ));
        assert!(pending.settled_for(
            PendingDiscoveryKind::Observation(target),
            began + DISCOVERY_SETTLE
        ));
    }

    #[test]
    fn discovery_raycast_refuses_occluded_and_out_of_range_blocks() {
        let reg = std::sync::Arc::new(crate::registry::load(std::path::Path::new("mods")));
        let mut world = World::new(5, std::path::PathBuf::new(), reg.clone());
        let actor =
            EntityPos::from_local(crate::planet::Face::PosZ, Vec3::new(0.5, 80.0, 0.5)).unwrap();
        let target = BlockPos::of_world(0, 81, 3).unwrap();
        world.ensure_chunk(target.chunk());
        world.set_block_at(target, reg.block_id("base:stone").unwrap());
        assert!(discovery_reachable_from(&world, actor, target));

        let wall = BlockPos::of_world(0, 81, 2).unwrap();
        world.set_block_at(wall, reg.block_id("base:stone").unwrap());
        assert!(!discovery_reachable_from(&world, actor, target));

        let far = BlockPos::of_world(0, 81, 12).unwrap();
        world.set_block_at(far, reg.block_id("base:stone").unwrap());
        assert!(!discovery_reachable_from(&world, actor, far));
    }

    #[test]
    fn placed_folio_copying_requires_physical_writing_surface_adjacency() {
        let writing = BlockPos::of_world(0, 80, 0).unwrap();
        assert!(discovery_holder_at_writing_surface(
            net::RecordHolderSnap::Inventory { slot: 2 },
            writing
        ));
        assert!(discovery_holder_at_writing_surface(
            net::RecordHolderSnap::Folio {
                pos: BlockPos::of_world(1, 80, 0).unwrap(),
            },
            writing
        ));
        assert!(!discovery_holder_at_writing_surface(
            net::RecordHolderSnap::Folio {
                pos: BlockPos::of_world(2, 80, 0).unwrap(),
            },
            writing
        ));
    }
}
