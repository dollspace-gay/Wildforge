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
use crate::inventory::{HOTBAR_SLOTS, Inventory, ItemStack, TOTAL_SLOTS};
use crate::physics::{self, Player};
use crate::registry::{self, BlockId, ItemId, Registry};
use crate::world::World;
use crate::{identity, mp, net};

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
        path: Vec<(i32, i32, i32)>,
        goal: (i32, i32, i32),
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
    pub spawn: Vec3,
    pub time_of_day: f32,
    pub in_world: bool,
    /// Count of PlayerState echoes applied — the click choreography
    /// sends one click, waits for one echo, and only then trusts the
    /// inventory mirror for its next decision.
    pub echoes: u64,
    block_map: Vec<BlockId>,
    item_map: Vec<Option<ItemId>>,
    /// id -> (label, pos, yaw) for every other player on the wire.
    pub players: HashMap<u32, (String, Vec3, f32)>,
    names: HashMap<u32, String>,
    /// Breadcrumbs per player: the trail follow() chases.
    trail: HashMap<u32, VecDeque<Vec3>>,
    /// Human-readable happenings, drained by the events tool.
    pub events: VecDeque<String>,
    pub behavior: Behavior,
    move_timer: f32,
    /// (pos sampled, seconds since) for stuck detection.
    stuck_probe: (Vec3, f32),
    mods_dir: PathBuf,
    cache_dir: PathBuf,
}

impl Agent {
    /// Connect and handshake; returns once Welcome has landed (or
    /// errors with the refusal). `name` is the agent's player name;
    /// its device identity persists under saves/.agents/<name>.
    pub fn connect(addr: std::net::SocketAddr, name: &str) -> Result<Agent, String> {
        let id_dir = PathBuf::from("saves/.agents").join(name.to_lowercase());
        let identity = identity::LocalIdentity::load_or_create(&id_dir)
            .map_err(|e| format!("identity: {e}"))?;
        let mods_dir = PathBuf::from("mods");
        let reg = Arc::new(registry::load(&mods_dir));
        let hash = net::content_hash(&mods_dir);
        let client = net::Client::connect(addr, name.to_string(), hash, 0, &identity, None)
            .map_err(|e| format!("connect: {e}"))?;
        let world = World::new(0, id_dir.join("world-cache"), reg.clone());
        let mut agent = Agent {
            client,
            reg,
            world,
            my_id: 0,
            player: Player::new(Vec3::new(0.0, 80.0, 0.0)),
            yaw: 0.0,
            hotbar: 0,
            inventory: Inventory::new(),
            cursor: None,
            health: 20.0,
            hunger: 20.0,
            spawn: Vec3::ZERO,
            time_of_day: 0.3,
            in_world: false,
            echoes: 0,
            block_map: Vec::new(),
            item_map: Vec::new(),
            players: HashMap::new(),
            names: HashMap::new(),
            trail: HashMap::new(),
            events: VecDeque::new(),
            behavior: Behavior::Idle,
            move_timer: 0.0,
            stuck_probe: (Vec3::ZERO, 0.0),
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
            if start.elapsed().as_secs() > 15 {
                return Err("timed out waiting for Welcome".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
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
            self.player = Player::new(state.pos);
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
        for msg in self.client.poll() {
            self.apply(msg);
        }
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
                self.in_world = true;
                self.event(format!("joined {world_name}"));
            }
            net::S2C::Refused(why) => {
                self.event(format!("refused: {}", why.detail));
                self.in_world = false;
            }
            net::S2C::Chunk { x, z, rle } => {
                self.world
                    .insert_remote_chunk(ChunkPos { x, z }, &rle, &self.block_map);
            }
            net::S2C::BlockSet { x, y, z, id, meta } => {
                let local = self
                    .block_map
                    .get(id as usize)
                    .copied()
                    .unwrap_or(self.reg.unknown_block);
                self.world.set_block_meta(x, y, z, local, meta);
                self.world.clear_pending_drops();
            }
            net::S2C::Players(list) => {
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
                    if t.back().is_none_or(|b| (*b - pos).length() > 0.75) {
                        t.push_back(pos);
                        if t.len() > 512 {
                            t.pop_front();
                        }
                    }
                }
            }
            net::S2C::Mobs(snaps) => {
                let mobs = snaps
                    .into_iter()
                    .filter(|s| (s.species as usize) < self.reg.animals.len())
                    .map(|s| {
                        let mut m = crate::mobs::Mob::new(s.species as usize, s.pos, s.yaw);
                        m.id = s.id;
                        m.growth = s.growth;
                        m.fed = s.fed;
                        m
                    })
                    .collect();
                self.world.replace_mobs(mobs);
            }
            net::S2C::TimeIre {
                time,
                ire,
                day,
                weather,
            } => {
                self.time_of_day = time;
                self.world.ire = ire;
                self.world.day = day;
                self.world.weather = crate::world::Weather::from_u8(weather);
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
            net::S2C::SignText { x, y, z, lines } => {
                self.world.insert_block_entity(
                    (x, y, z),
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
            Behavior::Idle => physics::Input {
                forward: 0.0,
                strafe: 0.0,
                jump: false,
                sprint: false,
            },
            Behavior::GoTo { path, goal } => self.tick_goto(path, goal, dt),
            Behavior::Follow { id, distance } => self.tick_follow(id, distance, dt),
        };
        let (fwd, right) = motion::basis(self.yaw);
        self.player.update(&self.world, &input, fwd, right, dt);
    }
}
