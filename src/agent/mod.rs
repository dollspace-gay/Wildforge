//! The agent: a headless guest that plays by the player protocol.
//!
//! Agents are guests, not gods (docs/agent-mcp-plan.md): this module
//! connects over the same QUIC handshake as a windowed player, keeps
//! the same streamed world mirror, moves with the same AABB physics,
//! and asks the host for everything the same way — same reach checks,
//! same rate limits, same shared ire. The layers above (perception,
//! motion, work, mcp) only ever act through what a player could do.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use glam::Vec3;

use crate::chunk::ChunkPos;
use crate::client_session::{ContentMap, GuestSession, PresentationRequirement};
use crate::inventory::{HOTBAR_SLOTS, Inventory, ItemStack, TOTAL_SLOTS};
use crate::physics::{self, Player};
use crate::registry::{self, ItemId, Registry};
use crate::world::World;
use crate::{identity, net};

/// How far an agent asks to see, in chunks. Enough to path somewhere it has
/// not been; the host clamps it like anyone else's request.
const AGENT_VIEW_DIST: u8 = 10;
/// Keep stdio/MCP and movement responsive while a wide cold horizon arrives.
/// The transport can deliver much faster than chunk adoption and lighting.
const CHUNKS_PER_PUMP: usize = 2;

mod mcp;
mod motion;
mod perception;
mod work;

pub use mcp::run_agent;
// The tests drive locomotion by cell; the lib itself goes via tools.
#[allow(unused_imports)]
pub use motion::cell_of;

/// What the agent keeps doing between thoughts.
pub enum Behavior {
    Idle,
    /// Chase a player's breadcrumb trail, hanging back `distance`.
    Follow {
        id: u32,
        distance: f32,
    },
    /// Walk a planned path; report arrival or failure as an event.
    GoTo {
        path: Vec<crate::planet::BlockPos>,
        goal: crate::planet::BlockPos,
    },
}

pub struct Agent {
    client: net::Client,
    pub reg: Arc<Registry>,
    pub world: World,
    pub my_id: u32,
    pub player: Player,
    pub yaw: f32,
    pub hotbar: usize,
    pub inventory: Inventory,
    pub cursor: Option<ItemStack>,
    pub health: f32,
    pub hunger: f32,
    pub spawn: crate::planet::EntityPos,
    pub time_of_day: f32,
    pub in_world: bool,
    /// Count of PlayerState echoes applied — the click choreography
    /// sends one click, waits for one echo, and only then trusts the
    /// inventory mirror for its next decision.
    pub echoes: u64,
    session: GuestSession,
    /// id -> (label, pos, yaw) for every other player on the wire.
    pub players: HashMap<u32, (String, crate::planet::EntityPos, f32)>,
    /// Breadcrumbs per player: the trail follow() chases.
    trail: HashMap<u32, VecDeque<crate::planet::EntityPos>>,
    /// Walkable waypoints are separate from observations of the leader.
    follow_path: Option<motion::FollowPath>,
    /// Human-readable happenings, drained by the events tool.
    pub events: VecDeque<String>,
    last_discovery: Option<crate::discovery::ObservationSummary>,
    last_discovery_records: Option<(Vec<crate::discovery::ObservationSummary>, u16)>,
    last_knowledge_text: Option<String>,
    last_binding_frame: Option<(crate::planet::BlockPos, crate::implements::FrameResult)>,
    binding_revisions: HashMap<crate::planet::BlockPos, u64>,
    last_alchemy_result: Option<(crate::planet::BlockPos, crate::alchemy::AlchemyResult)>,
    alchemy_revisions: HashMap<crate::planet::BlockPos, u64>,
    last_preparation_result: Option<crate::alchemy::PreparationUseResult>,
    last_working_result: Option<crate::workings::WorkingResult>,
    active_working_request: Option<(String, u64, crate::workings::WorkingTargetIntent)>,
    pub behavior: Behavior,
    move_timer: f32,
    /// (pos sampled, seconds since) for stuck detection.
    stuck_probe: (crate::planet::EntityPos, f32),
    mods_dir: PathBuf,
    cache_dir: PathBuf,
}

impl Agent {
    /// Connect and handshake; returns once Welcome has landed (or
    /// errors with the refusal). `name` is the agent's player name;
    /// its device identity persists under saves/.agents/<name>.
    pub fn connect(addr: std::net::SocketAddr, name: &str) -> Result<Agent, String> {
        Self::connect_with_view_distance(addr, name, AGENT_VIEW_DIST)
    }

    /// Protocol fixtures exercise behavior on a compact stage and do not need
    /// the production agent's ten-chunk pathfinding horizon.
    #[cfg(test)]
    pub(crate) fn connect_for_test(
        addr: std::net::SocketAddr,
        name: &str,
    ) -> Result<Agent, String> {
        Self::connect_with_view_distance(addr, name, 2)
    }

    fn connect_with_view_distance(
        addr: std::net::SocketAddr,
        name: &str,
        view_distance: u8,
    ) -> Result<Agent, String> {
        let id_dir = PathBuf::from("saves/.agents").join(name.to_lowercase());
        let identity = identity::LocalIdentity::load_or_create(&id_dir)
            .map_err(|e| format!("identity: {e}"))?;
        let mods_dir = PathBuf::from("mods");
        let reg = Arc::new(registry::load_validated(&mods_dir).map_err(|error| error.to_string())?);
        let hash = net::content_hash(&mods_dir);
        let client = net::Client::connect(addr, name.to_string(), hash, 0, &identity, None)
            .map_err(|e| format!("connect: {e}"))?;
        let world = World::new(0, id_dir.join("world-cache"), reg.clone());
        let half = f32::from(crate::planet::FACE_BLOCKS) * 0.5;
        let default_origin =
            crate::planet::EntityPos::new(crate::planet::Face::PosZ, half, 0.0, half)
                .expect("agent default origin is canonical");
        let default_player =
            crate::planet::EntityPos::new(crate::planet::Face::PosZ, half, 80.0, half)
                .expect("agent default player position is canonical");
        let mut agent = Agent {
            client,
            session: GuestSession::new(
                Arc::clone(&reg),
                PresentationRequirement::TerrainOnly,
                std::time::Instant::now(),
            ),
            reg,
            world,
            my_id: 0,
            player: Player::new_at(default_player),
            yaw: 0.0,
            hotbar: 0,
            inventory: Inventory::new(),
            cursor: None,
            health: 20.0,
            hunger: 20.0,
            spawn: default_origin,
            time_of_day: 0.3,
            in_world: false,
            echoes: 0,
            players: HashMap::new(),
            trail: HashMap::new(),
            follow_path: None,
            events: VecDeque::new(),
            last_discovery: None,
            last_discovery_records: None,
            last_knowledge_text: None,
            last_binding_frame: None,
            binding_revisions: HashMap::new(),
            last_alchemy_result: None,
            alchemy_revisions: HashMap::new(),
            last_preparation_result: None,
            last_working_result: None,
            active_working_request: None,
            behavior: Behavior::Idle,
            move_timer: 0.0,
            stuck_probe: (default_origin, 0.0),
            mods_dir,
            cache_dir: id_dir.join("world-cache"),
        };
        // Block for admission: the handshake is quick or refused.
        let start = std::time::Instant::now();
        while !agent.in_world {
            agent.pump(0.02);
            if let Some(refusal) = agent
                .events
                .iter()
                .find(|e| e.starts_with("refused:"))
                .cloned()
            {
                return Err(refusal);
            }
            if agent
                .session
                .admission()
                .timed_out(std::time::Instant::now())
                || start.elapsed().as_secs() > 120
            {
                return Err("timed out waiting for safe world entry".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        // Ask for a working horizon. An agent that never asks gets the old
        // fixed ring of five chunks, which is eighty blocks — it could not
        // path to anywhere it had not already been standing, because the
        // ground under the goal had never been sent to it.
        agent.client.send(&net::C2S::SetViewDistance {
            chunks: view_distance,
        });
        Ok(agent)
    }

    pub fn event(&mut self, line: String) {
        if self.events.len() > 200 {
            self.events.pop_front();
        }
        self.events.push_back(line);
    }

    pub fn send(&self, msg: &net::C2S) {
        self.client.send(msg);
    }

    fn apply_player_state(&mut self, state: net::PlayerStateSnap, initial: bool) {
        if initial {
            self.player = Player::new_at(state.pos);
            self.yaw = state.yaw;
        }
        self.spawn = state.spawn;
        self.health = state.health;
        self.hunger = state.hunger;
        self.hotbar = (state.hotbar as usize).min(HOTBAR_SLOTS - 1);
        self.inventory.slots = self.session.content().slots(&state.inventory);
        self.cursor = state
            .cursor
            .as_ref()
            .and_then(|stack| self.session.content().stack(stack));
    }

    /// One tick: apply the host's stream, advance the standing
    /// behavior, step physics, send our movement upstream.
    pub fn pump(&mut self, dt: f32) {
        if !self.client.is_connected() {
            if !self.session.admission().is_closed() {
                self.session.close();
                let message = if self.in_world {
                    "disconnected from host"
                } else {
                    "refused: disconnected during world preparation"
                };
                self.in_world = false;
                self.event(message.into());
            }
            return;
        }
        let messages = self.client.poll();
        if !messages.is_empty() {
            self.session.note_activity(std::time::Instant::now());
        }
        for msg in messages {
            self.apply(msg);
            if self.session.admission().is_closed() {
                break;
            }
        }
        self.apply_pending_chunks();
        if self.in_world {
            self.tick_behavior(dt);
            self.move_timer += dt;
            if self.move_timer >= 0.05 {
                self.move_timer = 0.0;
                self.client.send_datagram(&net::C2S::Move {
                    pos: self.player.pos,
                    yaw: self.yaw,
                    hotbar: self.hotbar as u8,
                    sprint: false,
                });
            }
        }
    }

    fn apply_pending_chunks(&mut self) {
        self.session.apply_terrain(&mut self.world, CHUNKS_PER_PUMP);
        if self.session.take_ready() {
            self.client.send(&net::C2S::EntryReady);
        }
    }

    /// Pump for `secs` of wall time at a steady cadence (macros wait
    /// on world echoes this way).
    pub fn pump_for(&mut self, secs: f32) {
        let steps = (secs / 0.02).ceil() as usize;
        for _ in 0..steps {
            self.pump(0.02);
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    fn apply(&mut self, msg: net::S2C) {
        let Some(msg) = self.session.apply_world_message(msg, &mut self.world, &mut self.time_of_day) else {
            return;
        };
        match msg {
            net::S2C::Challenge { .. } => {}
            net::S2C::ModFiles(files) => {
                let cache = PathBuf::from("saves/.agents/.remote-mods");
                match self.session.install_content(&cache, files) {
                    Ok(registry) => self.reg = registry,
                    Err(error) => {
                        self.in_world = false;
                        self.event(format!("refused: {error}"));
                        return;
                    }
                }
                self.mods_dir = cache;
                self.event("synced the host's mods".into());
            }
            net::S2C::Welcome {
                seed,
                mode,
                time,
                ire,
                palette,
                items,
                your_id,
                your_role: _,
                roster,
                spawn: _,
                world_name,
                player_state,
            } => {
                let mut world = World::new(seed, self.cache_dir.clone(), self.reg.clone());
                world.set_remote(true);
                world.mode = mode;
                world.ire = ire;
                self.my_id = your_id;
                self.session.begin(
                    ContentMap::new(Arc::clone(&self.reg), palette, items),
                    world_name,
                    player_state.pos,
                    std::time::Instant::now(),
                );
                self.session.set_roster(roster);
                self.players.clear();
                self.trail.clear();
                self.follow_path = None;
                self.world = world;
                self.time_of_day = time;
                self.apply_player_state(player_state, true);
                self.in_world = false;
            }
            net::S2C::EntryProgress { resident, total } => {
                self.event(format!("preparing entry terrain: {resident}/{total}"));
            }
            net::S2C::EntryManifest { spawn, required } => {
                if let Err(error) = self.session.manifest(spawn, required, &self.world) {
                    self.in_world = false;
                    self.event(format!("refused: {error}"));
                }
            }
            net::S2C::EntryAccepted => match self.session.accepted() {
                Ok(world) => {
                    self.in_world = true;
                    self.event(format!("joined {world}"));
                }
                Err(error) => {
                    self.in_world = false;
                    self.event(format!("refused: {error}"));
                }
            },
            net::S2C::Refused(why) => {
                self.session.close();
                self.event(format!("refused: {}", why.detail));
                self.in_world = false;
            }
            net::S2C::Chunk { face, u, v, rle } => {
                if let Some(face) = crate::planet::Face::from_u8(face)
                    && let Ok(pos) = ChunkPos::new(face, u, v)
                {
                    self.session.queue_chunk(pos, rle);
                }
            }
            net::S2C::BlockSet {
                pos,
                id,
                meta,
                salt_mass,
                soil_salinity,
            } => {
                self.session
                    .queue_block(pos, id, meta, salt_mass, soil_salinity);
            }
            net::S2C::Players(part) => {
                let Some(list) = self.session.players(part) else {
                    return;
                };
                let present: std::collections::HashSet<u32> =
                    list.iter().map(|(id, ..)| *id).collect();
                self.players.retain(|id, _| present.contains(id));
                for (id, pos, yaw, _held, _style, _implement) in list {
                    if id == self.my_id {
                        continue;
                    }
                    let name = self
                        .session.roster()
                        .get(&id)
                        .map(|presence| presence.display_name.clone())
                        .unwrap_or_else(|| format!("P{id}"));
                    self.players.insert(id, (name, pos, yaw));
                    // Breadcrumbs: a new crumb each ~0.75 blocks of
                    // travel; follow() chases the trail, not the line.
                    let t = self.trail.entry(id).or_default();
                    if t.back().is_none_or(|b| b.distance_to(pos) > 0.75) {
                        t.push_back(pos);
                        if t.len() > 512 {
                            t.pop_front();
                        }
                    }
                }
            }
            net::S2C::Mobs(part) => {
                if let Some(mobs) = self.session.mobs(part) {
                    self.world.replace_mobs(mobs);
                }
            }
            net::S2C::DiscoveryReport(record) => {
                self.event(format!(
                    "observed {}: {} ({})",
                    record.category, record.reading, record.phenomenon_id
                ));
                self.last_discovery = Some(record);
            }
            net::S2C::DiscoveryRecords {
                holder: _,
                records,
                capacity,
            } => {
                self.event(format!(
                    "read discovery records: {}/{} entries",
                    records.len(),
                    capacity
                ));
                self.last_discovery_records = Some((records, capacity));
            }
            net::S2C::KnowledgeText { instance_id, text } => {
                self.event(format!("read knowledge object {instance_id}: {text}"));
                self.last_knowledge_text = Some(text);
            }
            net::S2C::BindingFrameResult { pos, result } => {
                self.event(format!("binding frame: {}", result.message));
                self.binding_revisions.insert(pos, result.revision);
                self.last_binding_frame = Some((pos, result));
            }
            net::S2C::AlchemyResult { pos, result } => {
                self.event(format!("alchemy at {pos:?}: {}", result.cue.message));
                self.alchemy_revisions.insert(pos, result.revision);
                self.last_alchemy_result = Some((pos, result));
            }
            net::S2C::PreparationResult(result) => {
                self.event(format!("preparation: {}", result.message));
                self.last_preparation_result = Some(result);
            }
            net::S2C::PreparationState {
                modifiers,
                bodily_dross,
            } => {
                if modifiers.storm_warning
                    || modifiers.trace_sight != 0
                    || modifiers.dross_band != 0
                    || bodily_dross != 0
                {
                    self.event(format!(
                        "preparation state: trace {}, throughput {}/1000, environmental dross band {}, pattern {}, recovery {}/1000, bodily dross {bodily_dross}",
                        modifiers.trace_sight,
                        modifiers.throughput_permille,
                        modifiers.dross_band,
                        modifiers.dross_pattern,
                        modifiers.recovery_permille,
                    ));
                }
            }
            net::S2C::DrossEvent(cue) => {
                self.event(format!("dross event: {}", cue.accessible_text()));
            }
            net::S2C::AlchemyEvent(cue) => {
                self.event(format!("alchemy cue {:?}: {}", cue.kind, cue.message));
            }
            net::S2C::Hit { dmg, from: _ } => {
                self.health -= dmg;
                self.event(format!(
                    "took {dmg:.0} damage ({:.0} health left)",
                    self.health.max(0.0)
                ));
            }
            net::S2C::Give {
                item,
                count,
                durability,
                arcane_id,
                current_units,
            } => {
                if let Some(local) = self.session.content().item(item) {
                    let reg = self.reg.clone();
                    let mut stack = ItemStack::new(&reg, local, count.max(1));
                    if durability > 0 {
                        stack.durability = durability;
                    }
                    stack.arcane_id = arcane_id;
                    self.world.set_remote_arcane_item(arcane_id, current_units);
                    let name = reg.item(local).name.clone();
                    let left = self.inventory.add_stack(&reg, stack);
                    self.event(format!("received {}x {name}", count.max(1) - left));
                }
            }
            net::S2C::PlayerState(state) => {
                self.echoes += 1;
                self.apply_player_state(state, false);
            }
            net::S2C::Toast(t) => self.event(format!("toast: {t}")),
            net::S2C::Chat { from, msg } => self.event(format!("chat <{from}> {msg}")),
            net::S2C::Joined { presence } => {
                if presence.id != self.my_id {
                    self.event(format!("{} joined", presence.display_name));
                }
                self.session.joined(presence);
            }
            net::S2C::Left { id } => {
                self.players.remove(&id);
                self.trail.remove(&id);
                if let Some(presence) = self.session.left(id) {
                    self.event(format!("{} left", presence.display_name));
                }
            }
            net::S2C::SettlementDelivery {
                settlement,
                item,
                units,
                rep_per_unit,
            } => {
                // Capability E13: standing pays locally, like quest rewards.
                let _ = (settlement, item, units, rep_per_unit);
            }
            net::S2C::HeldResult(held) => {
                self.cursor = held
                    .as_ref()
                    .and_then(|stack| self.session.content().stack(stack));
            }
            net::S2C::RoleChanged { role } => {
                self.event(format!("role is now {role:?}"));
            }
            net::S2C::Sleep { sleeping, present } => {
                self.event(format!("{sleeping}/{present} sleeping"));
            }
            net::S2C::WorkingResult(result) => {
                if result.success && result.phase.is_none() {
                    self.active_working_request = None;
                }
                self.last_working_result = Some(result.clone());
                self.event(format!("working: {}", result.message));
            }
            net::S2C::WorkingEvent(cue) => {
                let path = cue
                    .path
                    .iter()
                    .map(|pos| format!("{}:{}:{}:{}", pos.face().name(), pos.u(), pos.y(), pos.v()))
                    .collect::<Vec<_>>()
                    .join(" -> ");
                self.event(format!(
                    "working cue {}: {} {:?}, warning {}, completion {}/1000, visible path {}",
                    cue.stable_id,
                    cue.working_id,
                    cue.kind,
                    cue.warning_band,
                    cue.completion_permille,
                    path
                ));
            }
            net::S2C::Bolts(part) => {
                if let Some(projectiles) = self.session.bolts(part) {
                    self.world.replace_projectiles(projectiles);
                }
            }
            net::S2C::LooseItems(part) => {
                if let Some(items) = self.session.loose_items(part) {
                    self.world.replace_loose_items(items);
                }
            }
            // The agent takes whatever ring the host grants; it has no
            // renderer, so there is no fog to keep honest.
            net::S2C::ViewDistance { .. } => {}
            // Containers, cargo, falling sand: not yet part of
            // the agent's world-model (fast follows).
            net::S2C::Container { .. }
            | net::S2C::MachineContainer { .. }
            | net::S2C::MobCargo { .. }
            | net::S2C::ImplementActivation { .. }
            | net::S2C::Falling(_)
            | net::S2C::MobHit { .. } => {}
        }
    }

    /// Advance the standing behavior and step player physics.
    fn tick_behavior(&mut self, dt: f32) {
        if !matches!(self.behavior, Behavior::Follow { .. }) {
            self.follow_path = None;
        }
        let input = match std::mem::replace(&mut self.behavior, Behavior::Idle) {
            Behavior::Idle => motion::idle(self.player.in_water),
            Behavior::GoTo { path, goal } => self.tick_goto(path, goal, dt),
            Behavior::Follow { id, distance } => self.tick_follow(id, distance, dt),
        };
        let frame = crate::planet::local_frame(self.player.pos.surface_point());
        let (east, north) = frame.chart_basis(self.player.pos);
        let (forward, right) = motion::basis(self.yaw);
        let fwd = east * forward.x + north * forward.z;
        let right = east * right.x + north * right.z;
        self.player.update(&self.world, &input, fwd, right, dt);
        self.yaw = self.player.frame_rotation.rotate_yaw(self.yaw);
    }
}
