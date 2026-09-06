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

#[path = "host/inventory_runtime.rs"]
mod inventory_runtime;
use inventory_runtime::{refresh_held, server_item_armor_points, take_ammo, take_item};
#[path = "host/discovery_context.rs"]
mod discovery_context;
use discovery_context::{
    discovery_calibration, discovery_holder_at_writing_surface, discovery_holder_capacity,
    discovery_holder_id, discovery_reachable,
};
#[path = "host/working_context.rs"]
mod working_context;
use working_context::operate_guest_working;
#[path = "host/observation_packet.rs"]
mod observation_packet;
use observation_packet::inspectable_arcane_items;

#[cfg(test)]
mod identity_tests {
    use super::discovery_context::discovery_reachable_from;
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

#[path = "host/requests_alchemy.rs"]
mod requests_alchemy;
#[path = "host/requests_animals.rs"]
mod requests_animals;
#[path = "host/requests_chat.rs"]
mod requests_chat;
#[path = "host/requests_containers.rs"]
mod requests_containers;
#[path = "host/requests_experiments.rs"]
mod requests_experiments;
#[path = "host/requests_field_observation.rs"]
mod requests_field_observation;
#[path = "host/requests_implements.rs"]
mod requests_implements;
#[path = "host/requests_inventory.rs"]
mod requests_inventory;
#[path = "host/requests_knowledge.rs"]
mod requests_knowledge;
#[path = "host/requests_movement.rs"]
mod requests_movement;
#[path = "host/requests_projectiles.rs"]
mod requests_projectiles;
#[path = "host/requests_terrain.rs"]
mod requests_terrain;
#[path = "host/requests_world_use.rs"]
mod requests_world_use;

#[path = "host/admission.rs"]
mod admission;
#[path = "host/damage.rs"]
mod damage;
#[path = "host/entry.rs"]
mod entry;
#[path = "host/join.rs"]
mod join;
#[path = "host/moderation_actions.rs"]
mod moderation_actions;
#[path = "host/player_contexts.rs"]
mod player_contexts;
#[path = "host/pump.rs"]
mod pump;
#[path = "host/pump_containers.rs"]
mod pump_containers;
#[path = "host/pump_delivery.rs"]
mod pump_delivery;
#[path = "host/pump_events.rs"]
mod pump_events;
#[path = "host/pump_guest_state.rs"]
mod pump_guest_state;
#[path = "host/pump_observations.rs"]
mod pump_observations;
#[path = "host/pump_riders.rs"]
mod pump_riders;
#[path = "host/pump_sleep.rs"]
mod pump_sleep;
#[path = "host/pump_spoilage.rs"]
mod pump_spoilage;
#[path = "host/replies.rs"]
mod replies;
#[path = "host/startup.rs"]
mod startup;
