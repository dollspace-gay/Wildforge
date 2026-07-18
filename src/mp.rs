//! Host-side multiplayer session: applies guest requests through the
//! same authoritative code paths local play uses, and broadcasts the
//! world back. Shared by the windowed host (pause menu → OPEN TO
//! FRIENDS) and the headless `--server` — one code path.

use std::collections::{HashMap, HashSet};

use glam::Vec3;

use crate::chunk::ChunkPos;
use crate::inventory::{ItemStack, click_stack};
use crate::net::{self, C2S, HostEvent, MobSnap, S2C, StackSnap};
use crate::server::Server;
use crate::world::{BlockEntity, World};

pub struct Guest {
    pub name: String,
    pub pos: Vec3,
    pub yaw: f32,
    /// Cursor stack while a container is open (host-side, authoritative).
    pub held: Option<ItemStack>,
    pub container: Option<(i32, i32, i32)>,
    pub sleeping: bool,
    sent_chunks: HashSet<(i32, i32)>,
    edits: u32,
    edit_window: f32,
}

/// Presentation the host player should see.
pub enum HostFx {
    Chat { from: String, msg: String },
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
    snapshot_timer: f32,
    state_timer: f32,
}

const REACH: f32 = 7.0;
const EDITS_PER_SEC: u32 = 10;

impl HostSession {
    pub fn start(world_name: String) -> std::io::Result<HostSession> {
        Self::start_on(world_name, net::GAME_PORT)
    }

    /// Tests and second-hosts bind an OS-assigned port with 0.
    pub fn start_on(world_name: String, port: u16) -> std::io::Result<HostSession> {
        Ok(HostSession {
            net: net::Host::start(world_name.clone(), port)?,
            guests: HashMap::new(),
            content_hash: net::content_hash(std::path::Path::new("mods")),
            world_name,
            snapshot_timer: 0.0,
            state_timer: 0.0,
        })
    }

    pub fn player_count(&self) -> usize {
        self.guests.len() + 1
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
        host: Option<(Vec3, f32, bool)>,
        dt: f32,
    ) -> Vec<HostFx> {
        let host_pos = host.map(|(p, _, _)| p).unwrap_or(Vec3::new(0.5, 80.0, 0.5));
        let host_yaw = host.map(|(_, y, _)| y).unwrap_or(0.0);
        let host_sleeping = host.map(|(_, _, s)| s).unwrap_or(false);
        let mut fx = Vec::new();
        for ev in self.net.poll() {
            match ev {
                HostEvent::Joined { id, name, content_hash } => {
                    self.on_join(server, id, name, content_hash, &mut fx);
                }
                HostEvent::Left { id } => {
                    if let Some(g) = self.guests.remove(&id) {
                        self.net.broadcast(&S2C::Left { id });
                        fx.push(HostFx::Left(g.name));
                    }
                }
                HostEvent::Msg { id, msg } => {
                    self.on_msg(server, id, msg, &mut fx);
                }
            }
        }

        // Rate-limit windows.
        for g in self.guests.values_mut() {
            g.edit_window += dt;
            if g.edit_window >= 1.0 {
                g.edit_window = 0.0;
                g.edits = 0;
            }
        }

        // Authoritative block edits out.
        if !server.world.edit_log.is_empty() {
            for (x, y, z, b) in std::mem::take(&mut server.world.edit_log) {
                self.net.broadcast(&S2C::BlockSet { x, y, z, id: b.0 });
            }
        }
        // Items owed to guests (arrow recovery, mining, mob drops).
        for (owner, item) in std::mem::take(&mut server.world.pending_gives) {
            self.net.send(owner, &S2C::Give { item: item.0, count: 1, durability: 0 });
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
                players.push((0u32, host_pos, host_yaw));
            }
            for (id, g) in &self.guests {
                players.push((*id, g.pos, g.yaw));
            }
            self.net.broadcast_datagram(&S2C::Players(players));
            let mobs: Vec<MobSnap> = server
                .world
                .mobs
                .iter()
                .map(|m| MobSnap {
                    species: m.species as u16,
                    pos: m.pos,
                    yaw: m.yaw,
                    growth: m.growth,
                    hurt: m.hurt_flash,
                })
                .collect();
            self.net.broadcast_datagram(&S2C::Mobs(mobs));
            let bolts: Vec<net::BoltSnap> = server
                .world
                .projectiles
                .iter()
                .map(|p| net::BoltSnap { pos: p.pos, tile: p.tile, age: p.age })
                .collect();
            self.net.broadcast_datagram(&S2C::Bolts(bolts));
        }
        self.state_timer += dt;
        if self.state_timer >= 1.0 {
            self.state_timer = 0.0;
            self.net.broadcast(&S2C::TimeIre {
                time: server.time_of_day,
                ire: server.world.ire,
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
                server.time_of_day = 0.3;
                for g in self.guests.values_mut() {
                    g.sleeping = false;
                }
                self.net.broadcast(&S2C::TimeIre {
                    time: server.time_of_day,
                    ire: server.world.ire,
                });
                self.net.broadcast(&S2C::Toast("Dawn. The camp wakes.".into()));
                fx.push(HostFx::AllSlept);
            }
        }
        fx
    }

    fn on_join(
        &mut self,
        server: &Server,
        id: u32,
        name: String,
        content_hash: u64,
        fx: &mut Vec<HostFx>,
    ) {
        let reg = &server.world.reg;
        if content_hash != self.content_hash {
            // Stream the mods dir so the guest can match us exactly.
            let files = net::collect_mod_files(std::path::Path::new("mods"));
            self.net.send(id, &S2C::ModFiles(files));
        }
        let palette: Vec<String> = reg.blocks.iter().map(|b| b.name.clone()).collect();
        let items: Vec<String> = reg.items.iter().map(|i| i.name.clone()).collect();
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
                spawn: Vec3::new(0.5, 80.0, 0.5), // corrected by first chunks
                world_name: self.world_name.clone(),
            },
        );
        self.net.broadcast(&S2C::Joined { id, name: name.clone() });
        self.guests.insert(
            id,
            Guest {
                name: name.clone(),
                pos: Vec3::new(0.5, 80.0, 0.5),
                yaw: 0.0,
                held: None,
                container: None,
                sleeping: false,
                sent_chunks: HashSet::new(),
                edits: 0,
                edit_window: 0.0,
            },
        );
        fx.push(HostFx::Joined(name));
    }

    fn on_msg(&mut self, server: &mut Server, id: u32, msg: C2S, fx: &mut Vec<HostFx>) {
        let Some(guest) = self.guests.get_mut(&id) else { return };
        match msg {
            C2S::Hello { .. } | C2S::Bye => {}
            C2S::Move { pos, yaw } => {
                guest.pos = pos;
                guest.yaw = yaw;
            }
            C2S::Break { x, y, z } => {
                let p = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                if (p - guest.pos).length() > REACH || guest.edits >= EDITS_PER_SEC {
                    return;
                }
                guest.edits += 1;
                let b = server.world.get_block(x, y, z);
                if b == crate::registry::AIR || server.world.reg.block(b).hardness.is_none() {
                    return;
                }
                // Drops go straight to the breaker over the wire.
                if let Some((item, n)) = server.world.reg.drops_for(b, None) {
                    for _ in 0..n {
                        server.world.pending_gives.push((id, item));
                    }
                }
                let cost = server.world.ire_for_block(b);
                server.world.add_ire(cost);
                server.world.set_block(x, y, z, crate::registry::AIR);
            }
            C2S::Place { x, y, z, block } => {
                let p = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                if (p - guest.pos).length() > REACH || guest.edits >= EDITS_PER_SEC {
                    return;
                }
                let Some(def) = server.world.reg.blocks.get(block as usize) else { return };
                if server.world.get_block(x, y, z) != crate::registry::AIR {
                    return;
                }
                let _ = def;
                guest.edits += 1;
                server.world.set_block(x, y, z, crate::registry::BlockId(block));
            }
            C2S::AttackMob { index, dmg, from } => {
                let i = index as usize;
                let dmg = dmg.clamp(0.0, 16.0);
                let gpos = guest.pos;
                if let Some(m) = server.world.mobs.get_mut(i) {
                    if (m.pos - gpos).length() <= REACH {
                        let def = server.world.reg.animals[m.species].clone();
                        m.hurt(&def, dmg, from);
                        m.last_hit_by = id;
                        if !def.hostile {
                            server.world.add_ire(2.0);
                        }
                    }
                }
            }
            C2S::FireArrow { pos, vel, dmg, tile, recover } => {
                if (pos - guest.pos).length() > 3.0 {
                    return;
                }
                let arrow = server.world.reg.item_id("base:arrow");
                server.world.projectiles.push(crate::mobs::Projectile {
                    pos,
                    vel: vel.clamp_length_max(40.0),
                    tile,
                    damage: dmg.clamp(0.0, 12.0),
                    age: 0.0,
                    from_player: true,
                    drop_item: if recover { arrow } else { None },
                    owner: id,
                });
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
                    _ => return,
                };
                let entry =
                    server.world.block_entities.entry((x, y, z)).or_insert_with(|| match kind {
                        0 => BlockEntity::Chest(Default::default()),
                        1 => BlockEntity::Furnace(Default::default()),
                        _ => BlockEntity::Offering(Default::default()),
                    });
                if let BlockEntity::Chest(c) = entry {
                    if c.wild_owned {
                        c.wild_owned = false;
                        server.world.add_ire(1.0);
                        self.net
                            .send(id, &S2C::Toast("The wild keeps its trophies.".into()));
                    }
                }
                if let Some(g) = self.guests.get_mut(&id) {
                    g.container = Some((x, y, z));
                }
                self.send_container(server, id, (x, y, z));
            }
            C2S::ContainerClick { x, y, z, slot, right } => {
                self.container_click(server, id, (x, y, z), slot as usize, right);
            }
            C2S::TakeHeld => {
                if let Some(g) = self.guests.get_mut(&id) {
                    if let Some(h) = g.held.take() {
                        for _ in 0..h.count {
                            server.world.pending_gives.push((id, h.item));
                        }
                        // Durability rides only single stacks; tools are 1x.
                        if let Some(pos) = g.container {
                            self.send_container(server, id, pos);
                        }
                    }
                }
            }
            C2S::OfferHeld { item, count, durability } => {
                if let Some(g) = self.guests.get_mut(&id) {
                    if g.held.is_none()
                        && (item as usize) < server.world.reg.items.len()
                        && count > 0
                        && count <= 64
                    {
                        g.held = Some(ItemStack {
                            item: crate::registry::ItemId(item),
                            count,
                            durability,
                        });
                        if let Some(pos) = g.container {
                            self.send_container(server, id, pos);
                        }
                    }
                }
            }
            C2S::SleepRequest => {
                guest.sleeping = true;
            }
            C2S::SleepCancel => {
                guest.sleeping = false;
            }
            C2S::Chat(msg) => {
                let msg: String = msg.chars().take(200).collect();
                let from = guest.name.clone();
                self.net.broadcast(&S2C::Chat { from: from.clone(), msg: msg.clone() });
                fx.push(HostFx::Chat { from, msg });
            }
        }
    }

    fn container_click(
        &mut self,
        server: &mut Server,
        id: u32,
        pos: (i32, i32, i32),
        slot: usize,
        right: bool,
    ) {
        let reg = server.world.reg.clone();
        let Some(guest) = self.guests.get_mut(&id) else { return };
        let Some(entity) = server.world.block_entities.get_mut(&pos) else { return };
        match entity {
            BlockEntity::Chest(c) => {
                if slot < c.slots.len() {
                    let (ns, nh) = click_stack(&reg, c.slots[slot], guest.held, right);
                    c.slots[slot] = ns;
                    guest.held = nh;
                }
            }
            BlockEntity::Offering(o) => {
                if slot < o.slots.len() {
                    let (ns, nh) = click_stack(&reg, o.slots[slot], guest.held, right);
                    o.slots[slot] = ns;
                    guest.held = nh;
                }
            }
            BlockEntity::Furnace(f) => match slot {
                0 | 1 => {
                    let cur = if slot == 0 { f.input } else { f.fuel };
                    let (ns, nh) = click_stack(&reg, cur, guest.held, right);
                    if slot == 0 {
                        if f.input.map(|s| s.item) != ns.map(|s| s.item) {
                            f.progress = 0.0;
                        }
                        f.input = ns;
                    } else {
                        f.fuel = ns;
                    }
                    guest.held = nh;
                }
                _ => {
                    if let Some(out) = f.output {
                        match guest.held {
                            None => {
                                guest.held = Some(out);
                                f.output = None;
                            }
                            Some(h)
                                if h.item == out.item
                                    && h.count + out.count <= reg.item(h.item).max_stack =>
                            {
                                guest.held =
                                    Some(ItemStack { count: h.count + out.count, ..h });
                                f.output = None;
                            }
                            _ => {}
                        }
                    }
                }
            },
        }
        self.send_container(server, id, pos);
    }

    fn send_container(&mut self, server: &Server, id: u32, pos: (i32, i32, i32)) {
        let Some(entity) = server.world.block_entities.get(&pos) else { return };
        let snap = |s: &Option<ItemStack>| {
            s.map(|s| StackSnap { item: s.item.0, count: s.count, durability: s.durability })
        };
        let (kind, slots): (u8, Vec<Option<StackSnap>>) = match entity {
            BlockEntity::Chest(c) => (0, c.slots.iter().map(snap).collect()),
            BlockEntity::Furnace(f) => {
                (1, vec![snap(&f.input), snap(&f.fuel), snap(&f.output)])
            }
            BlockEntity::Offering(o) => (2, o.slots.iter().map(snap).collect()),
        };
        let held = self.guests.get(&id).and_then(|g| snap(&g.held));
        self.net.send(
            id,
            &S2C::Container { x: pos.0, y: pos.1, z: pos.2, kind, slots, held },
        );
    }
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
