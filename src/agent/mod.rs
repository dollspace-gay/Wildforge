//! The agent: a headless guest that plays by the player protocol.
//!
//! Agents are guests, not gods (docs/agent-mcp-plan.md): this module
//! connects over the same QUIC handshake as a windowed player, keeps
//! the same streamed world mirror, moves with the same AABB physics,
//! and asks the host for everything the same way — same reach checks,
//! same rate limits, same shared ire. The layers above (perception,
//! motion, work, mcp) only ever act through what a player could do.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use glam::Vec3;

use crate::chunk::ChunkPos;
use crate::inventory::{HOTBAR_SLOTS, Inventory, ItemStack, TOTAL_SLOTS};
use crate::physics::{self, Player};
use crate::registry::{self, BlockId, ItemId, Registry};
use crate::world::World;
use crate::{identity, mp, net};

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
    block_map: Vec<BlockId>,
    item_map: Vec<Option<ItemId>>,
    /// id -> (label, pos, yaw) for every other player on the wire.
    pub players: HashMap<u32, (String, crate::planet::EntityPos, f32)>,
    names: HashMap<u32, String>,
    /// Breadcrumbs per player: the trail follow() chases.
    trail: HashMap<u32, VecDeque<crate::planet::EntityPos>>,
    /// Snapshots arrive split when they outgrow one datagram.
    players_rx: net::SnapshotAssembler<(u32, crate::planet::EntityPos, f32, u16, u32)>,
    mobs_rx: net::SnapshotAssembler<net::MobSnap>,
    /// Human-readable happenings, drained by the events tool.
    pub events: VecDeque<String>,
    pub behavior: Behavior,
    pending_chunks: VecDeque<(ChunkPos, Vec<u8>)>,
    entry_required: HashSet<ChunkPos>,
    entry_manifest_received: bool,
    entry_ready_sent: bool,
    entry_world_name: Option<String>,
    entry_activity: std::time::Instant,
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
        let reg = Arc::new(registry::load(&mods_dir));
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
            block_map: Vec::new(),
            item_map: Vec::new(),
            players: HashMap::new(),
            names: HashMap::new(),
            trail: HashMap::new(),
            players_rx: Default::default(),
            mobs_rx: Default::default(),
            events: VecDeque::new(),
            behavior: Behavior::Idle,
            pending_chunks: VecDeque::new(),
            entry_required: HashSet::new(),
            entry_manifest_received: false,
            entry_ready_sent: false,
            entry_world_name: None,
            entry_activity: std::time::Instant::now(),
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
            if agent.entry_activity.elapsed().as_secs() > 15 || start.elapsed().as_secs() > 120 {
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

    fn local_item(&self, wire: u16) -> Option<ItemId> {
        *self.item_map.get(wire as usize)?
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
        let mut inv = Inventory::new();
        for (i, s) in state.inventory.into_iter().enumerate() {
            if i >= TOTAL_SLOTS {
                break;
            }
            inv.slots[i] = s.and_then(|s| {
                Some(ItemStack {
                    item: self.local_item(s.item)?,
                    count: s.count,
                    durability: s.durability,
                })
            });
        }
        self.inventory = inv;
        self.cursor = state.cursor.and_then(|s| {
            Some(ItemStack {
                item: self.local_item(s.item)?,
                count: s.count,
                durability: s.durability,
            })
        });
    }

    /// One tick: apply the host's stream, advance the standing
    /// behavior, step physics, send our movement upstream.
    pub fn pump(&mut self, dt: f32) {
        if !self.client.is_connected() && self.in_world {
            self.in_world = false;
            self.event("disconnected from host".into());
            return;
        }
        let messages = self.client.poll();
        if !messages.is_empty() {
            self.entry_activity = std::time::Instant::now();
        }
        for msg in messages {
            match msg {
                net::S2C::Chunk { face, u, v, rle } => {
                    if let Some(face) = crate::planet::Face::from_u8(face)
                        && let Ok(pos) = ChunkPos::new(face, u, v)
                    {
                        self.pending_chunks.push_back((pos, rle));
                    }
                }
                other => self.apply(other),
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
        if !self.pending_chunks.is_empty() {
            let chunks: Vec<_> = self
                .pending_chunks
                .drain(..self.pending_chunks.len().min(CHUNKS_PER_PUMP))
                .collect();
            self.world.insert_remote_chunks(
                chunks.iter().map(|(pos, rle)| (*pos, rle.as_slice())),
                &self.block_map,
            );
            for (pos, _) in chunks {
                self.entry_required.remove(&pos);
            }
        }
        if self.entry_manifest_received && self.entry_required.is_empty() && !self.entry_ready_sent
        {
            self.client.send(&net::C2S::EntryReady);
            self.entry_ready_sent = true;
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
        match msg {
            net::S2C::Challenge { .. } => {}
            net::S2C::ModFiles(files) => {
                // The host's content becomes ours, same as a windowed
                // guest: cached, loaded, remapped on Welcome.
                let cache = PathBuf::from("saves/.agents/.remote-mods");
                let _ = std::fs::remove_dir_all(&cache);
                for (rel, bytes) in files {
                    if rel.contains("..") {
                        continue;
                    }
                    let p = cache.join(rel);
                    if let Some(parent) = p.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let _ = std::fs::write(p, bytes);
                }
                self.reg = Arc::new(registry::load(&cache));
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
                self.names = roster.into_iter().map(|p| (p.id, p.display_name)).collect();
                self.block_map = mp::block_remap(&world, &palette);
                self.item_map = mp::item_remap(&world, &items);
                self.world = world;
                self.time_of_day = time;
                self.apply_player_state(player_state, true);
                self.in_world = false;
                self.entry_required.clear();
                self.entry_manifest_received = false;
                self.entry_ready_sent = false;
                self.entry_world_name = Some(world_name);
            }
            net::S2C::EntryProgress { resident, total } => {
                self.event(format!("preparing entry terrain: {resident}/{total}"));
            }
            net::S2C::EntryManifest { spawn, required } => {
                if spawn != self.player.pos {
                    self.event("refused: host entry manifest did not match Welcome spawn".into());
                    return;
                }
                self.entry_required = required.into_iter().collect();
                self.entry_manifest_received = true;
            }
            net::S2C::EntryAccepted => {
                if !self.entry_ready_sent || !self.entry_required.is_empty() {
                    self.event("refused: host accepted entry before terrain was decoded".into());
                    return;
                }
                self.in_world = true;
                let world = self
                    .entry_world_name
                    .take()
                    .unwrap_or_else(|| "world".into());
                self.event(format!("joined {world}"));
            }
            net::S2C::Refused(why) => {
                self.event(format!("refused: {}", why.detail));
                self.in_world = false;
            }
            net::S2C::Chunk { face, u, v, rle } => {
                if let Some(face) = crate::planet::Face::from_u8(face)
                    && let Ok(pos) = ChunkPos::new(face, u, v)
                {
                    self.world.insert_remote_chunk(pos, &rle, &self.block_map);
                }
            }
            net::S2C::BlockSet {
                pos,
                id,
                meta,
                salt_mass,
                soil_salinity,
            } => {
                let local = self
                    .block_map
                    .get(id as usize)
                    .copied()
                    .unwrap_or(self.reg.unknown_block);
                self.world
                    .set_block_state_at(pos, local, meta, salt_mass, soil_salinity);
                self.world.clear_pending_drops();
            }
            net::S2C::Players(part) => {
                let Some(list) = self.players_rx.accept(part) else {
                    return;
                };
                let present: std::collections::HashSet<u32> =
                    list.iter().map(|(id, ..)| *id).collect();
                self.players.retain(|id, _| present.contains(id));
                for (id, pos, yaw, _held, _style) in list {
                    if id == self.my_id {
                        continue;
                    }
                    let name = self
                        .names
                        .get(&id)
                        .cloned()
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
                let Some(snaps) = self.mobs_rx.accept(part) else {
                    return;
                };
                let mobs = snaps
                    .into_iter()
                    .filter(|s| (s.species as usize) < self.reg.animals.len())
                    .map(|s| {
                        let mut m = crate::mobs::Mob::new_at(s.species as usize, s.pos, s.yaw);
                        m.id = s.id;
                        m.growth = s.growth;
                        m.fed = s.fed;
                        m
                    })
                    .collect();
                self.world.replace_mobs(mobs);
            }
            net::S2C::TimeIre { time, ire, day } => {
                self.time_of_day = time;
                self.world.ire = ire;
                self.world.day = day;
            }
            net::S2C::WeatherCells { side, cells } => {
                self.world.set_remote_weather(side, cells);
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
            } => {
                if let Some(local) = self.local_item(item) {
                    let reg = self.reg.clone();
                    let mut stack = ItemStack::new(&reg, local, count.max(1));
                    if durability > 0 {
                        stack.durability = durability;
                    }
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
                self.names.insert(presence.id, presence.display_name);
            }
            net::S2C::Left { id } => {
                self.players.remove(&id);
                self.trail.remove(&id);
                if let Some(n) = self.names.remove(&id) {
                    self.event(format!("{n} left"));
                }
            }
            net::S2C::SignText { pos, lines } => {
                self.world.insert_block_entity_at(
                    pos,
                    crate::world::BlockEntity::Sign(crate::world::SignState { lines }),
                );
            }
            net::S2C::HeldResult(held) => {
                self.cursor = held.and_then(|s| {
                    Some(ItemStack {
                        item: self.local_item(s.item)?,
                        count: s.count,
                        durability: s.durability,
                    })
                });
            }
            net::S2C::RoleChanged { role } => {
                self.event(format!("role is now {role:?}"));
            }
            net::S2C::Sleep { sleeping, present } => {
                self.event(format!("{sleeping}/{present} sleeping"));
            }
            // The agent takes whatever ring the host grants; it has no
            // renderer, so there is no fog to keep honest.
            net::S2C::ViewDistance { .. } => {}
            // Containers, cargo, bolts, falling sand: not yet part of
            // the agent's world-model (fast follows).
            net::S2C::Container { .. }
            | net::S2C::MobCargo { .. }
            | net::S2C::Bolts(_)
            | net::S2C::Falling(_) => {}
        }
    }

    /// Advance the standing behavior and step player physics.
    fn tick_behavior(&mut self, dt: f32) {
        let input = match std::mem::replace(&mut self.behavior, Behavior::Idle) {
            Behavior::Idle => motion::idle(self.player.in_water),
            Behavior::GoTo { path, goal } => self.tick_goto(path, goal, dt),
            Behavior::Follow { id, distance } => self.tick_follow(id, distance, dt),
        };
        let (fwd, right) = motion::basis(self.yaw);
        self.player.update(&self.world, &input, fwd, right, dt);
    }
}
