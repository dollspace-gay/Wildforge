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
    pub container: Option<(i32, i32, i32)>,
    pub sleeping: bool,
    /// Wire item id in hand (u16::MAX = empty), from Move packets.
    pub held: u16,
    /// Packed appearance Style, from the Hello.
    pub style: u32,
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
    /// Names kicked this session: refused if they reconnect.
    banned: HashSet<String>,
    snapshot_timer: f32,
    state_timer: f32,
    container_timer: f32,
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
                    name,
                    content_hash,
                    style,
                } => {
                    self.on_join(server, id, name, content_hash, style, &mut fx);
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

        // Rate-limit windows + movement interpolation clocks.
        for g in self.guests.values_mut() {
            g.edit_window += dt;
            if g.edit_window >= 1.0 {
                g.edit_window = 0.0;
                g.edits = 0;
            }
            g.net_age += dt;
        }

        // Authoritative block edits out.
        if !server.world.edit_log.is_empty() {
            for (x, y, z, b) in std::mem::take(&mut server.world.edit_log) {
                self.net.broadcast(&S2C::BlockSet { x, y, z, id: b.0 });
            }
        }
        // Items owed to guests (arrow recovery, mining, mob drops,
        // brush finds) — full stacks so durability crosses the wire.
        for (owner, s) in std::mem::take(&mut server.world.pending_gives) {
            self.net.send(
                owner,
                &S2C::Give {
                    item: s.item.0,
                    count: s.count,
                    durability: s.durability,
                },
            );
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
                .mobs
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
                .projectiles
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
                .falling
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

    fn on_join(
        &mut self,
        server: &Server,
        id: u32,
        name: String,
        content_hash: u64,
        style: u32,
        fx: &mut Vec<HostFx>,
    ) {
        if self.banned.contains(&name) {
            self.net.send(id, &S2C::Refused("kicked by host".into()));
            self.net.kick(id);
            return;
        }
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
        self.net.broadcast(&S2C::Joined {
            id,
            name: name.clone(),
        });
        self.guests.insert(
            id,
            Guest {
                name: name.clone(),
                pos: Vec3::new(0.5, 80.0, 0.5),
                yaw: 0.0,
                container: None,
                sleeping: false,
                held: u16::MAX,
                style,
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

    /// Kick a guest and refuse them for the rest of the session.
    pub fn kick_guest(&mut self, id: u32) -> Option<String> {
        let g = self.guests.remove(&id)?;
        self.banned.insert(g.name.clone());
        self.net.send(id, &S2C::Refused("kicked by host".into()));
        self.net.broadcast(&S2C::Left { id });
        self.net.kick(id);
        Some(g.name)
    }

    fn on_msg(&mut self, server: &mut Server, id: u32, msg: C2S, fx: &mut Vec<HostFx>) {
        let Some(guest) = self.guests.get_mut(&id) else {
            return;
        };
        match msg {
            C2S::Hello { .. } | C2S::Bye => {}
            C2S::Move { pos, yaw, held } => {
                guest.render_from = guest.render_pos();
                guest.net_interval = guest.net_age.clamp(0.03, 0.3);
                guest.net_age = 0.0;
                guest.pos = pos;
                guest.yaw = yaw;
                guest.held = held;
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
                let b = server.world.get_block(x, y, z);
                if b == crate::registry::AIR || server.world.reg.block(b).hardness.is_none() {
                    return;
                }
                // Drops go straight to the breaker over the wire.
                if let Some((item, n)) = server.world.reg.drops_for(b, None) {
                    let stack = ItemStack::new(&server.world.reg, item, n);
                    server.world.pending_gives.push((id, stack));
                }
                let cost = server.world.ire_for_block(b);
                server.world.add_ire(cost);
                server.world.set_block(x, y, z, crate::registry::AIR);
            }
            C2S::Scoop { x, y, z } => {
                let p = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                if (p - guest.pos).length() > REACH || guest.edits >= EDITS_PER_SEC {
                    return;
                }
                // Only a full cell fills a bucket — partials would let
                // a guest mint water out of films.
                let b = server.world.get_block(x, y, z);
                if server.world.reg.water_volume(b) != Some(8) {
                    return;
                }
                guest.edits += 1;
                server.world.set_block(x, y, z, crate::registry::AIR);
            }
            C2S::Place { x, y, z, block } => {
                let p = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                if (p - guest.pos).length() > REACH || guest.edits >= EDITS_PER_SEC {
                    return;
                }
                let Some(def) = server.world.reg.blocks.get(block as usize) else {
                    return;
                };
                if server.world.get_block(x, y, z) != crate::registry::AIR {
                    return;
                }
                let _ = def;
                guest.edits += 1;
                server
                    .world
                    .set_block(x, y, z, crate::registry::BlockId(block));
            }
            C2S::AttackMob {
                id: mob_id,
                dmg,
                from,
            } => {
                // Stable ids: snapshots lag the sim, so an index would
                // race deaths/spawns and strike the wrong creature.
                let dmg = dmg.clamp(0.0, 16.0);
                let gpos = guest.pos;
                if let Some(m) = server.world.mobs.iter_mut().find(|m| m.id == mob_id)
                    && (m.pos - gpos).length() <= REACH
                {
                    let def = server.world.reg.animals[m.species].clone();
                    m.hurt(&def, dmg, from);
                    m.last_hit_by = id;
                    if !def.hostile {
                        server.world.add_ire(2.0);
                    }
                }
            }
            C2S::FeedMob { id: mob_id } => {
                let gpos = guest.pos;
                if let Some(m) = server.world.mobs.iter_mut().find(|m| m.id == mob_id) {
                    let def = &server.world.reg.animals[m.species];
                    if (m.pos - gpos).length() <= REACH
                        && !def.hostile
                        && def.breed_food.is_some()
                        && m.growth >= 1.0
                        && m.breed_cd <= 0.0
                        && !m.fed
                    {
                        m.fed = true;
                        m.calm = 30.0;
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
                let mut r = server.rng;
                let found = server.world.brush_block(x, y, z, &mut r);
                server.rng = r;
                if let Some(stack) = found {
                    server.world.pending_gives.push((id, stack));
                }
            }
            C2S::FireArrow {
                pos,
                vel,
                dmg,
                tile,
                recover,
            } => {
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
                    Some("bloomery") => 3,
                    Some("kiln") => 4,
                    _ => return,
                };
                let entry = server
                    .world
                    .block_entities
                    .entry((x, y, z))
                    .or_insert_with(|| match kind {
                        0 => BlockEntity::Chest(Default::default()),
                        1 => BlockEntity::Furnace(Default::default()),
                        3 => BlockEntity::Bloomery(Default::default()),
                        4 => BlockEntity::Kiln(Default::default()),
                        _ => BlockEntity::Offering(Default::default()),
                    });
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
                held,
            } => {
                // The cursor is guest-owned (trusted-friends model, like
                // the guest's whole inventory); a click is a transaction
                // between it and the host-owned container.
                let held = held.and_then(|h| {
                    ((h.item as usize) < server.world.reg.items.len()
                        && h.count > 0
                        && h.count <= 99)
                        .then_some(ItemStack {
                            item: crate::registry::ItemId(h.item),
                            count: h.count,
                            durability: h.durability,
                        })
                });
                self.container_click(server, id, (x, y, z), slot as usize, right, held);
            }
            C2S::CloseContainer => {
                guest.container = None;
            }
            C2S::LightBloomery { x, y, z } => {
                let p = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                if (p - guest.pos).length() > REACH {
                    return;
                }
                let b = server.world.get_block(x, y, z);
                let res = match server.world.reg.block(b).interaction.as_deref() {
                    Some("kiln") => server.world.light_kiln(x, y, z),
                    _ => server.world.light_bloomery(x, y, z),
                };
                match res {
                    Ok(()) => self.send_container(server, id, (x, y, z)),
                    Err(e) => self.net.send(id, &S2C::Toast(e.into())),
                }
            }
            C2S::LightClamp { x, y, z } => {
                let p = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                if (p - guest.pos).length() > REACH {
                    return;
                }
                match server.world.try_light_clamp(x, y, z) {
                    Ok(n) => self
                        .net
                        .send(id, &S2C::Toast(format!("The clamp smolders ({n} logs)."))),
                    Err(e) => self.net.send(id, &S2C::Toast(e.into())),
                }
            }
            C2S::AnvilPut { x, y, z, item } => {
                let p = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                if (p - guest.pos).length() > REACH
                    || (item as usize) >= server.world.reg.items.len()
                {
                    return;
                }
                let stack = ItemStack::new(&server.world.reg, crate::registry::ItemId(item), 1);
                if !server.world.anvil_put((x, y, z), stack) {
                    // Rejected: the trusted-consumed item goes back.
                    server.world.pending_gives.push((id, stack));
                }
            }
            C2S::AnvilStrike { x, y, z } => {
                let p = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                if (p - guest.pos).length() > REACH {
                    return;
                }
                if let Some(out) = server.world.anvil_strike((x, y, z)) {
                    server.world.pending_gives.push((id, out));
                }
            }
            C2S::AnvilTake { x, y, z } => {
                let p = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                if (p - guest.pos).length() > REACH {
                    return;
                }
                if let Some(b) = server.world.anvil_take((x, y, z)) {
                    server.world.pending_gives.push((id, b));
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
                self.net.broadcast(&S2C::Chat {
                    from: from.clone(),
                    msg: msg.clone(),
                });
                fx.push(HostFx::Chat { from, msg });
            }
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
        held: Option<ItemStack>,
    ) {
        let reg = server.world.reg.clone();
        if self
            .guests
            .get(&id)
            .is_none_or(|g| g.container != Some(pos))
        {
            return;
        }
        let Some(entity) = server.world.block_entities.get_mut(&pos) else {
            return;
        };
        let mut held = held;
        match entity {
            BlockEntity::Bloomery(bl) => {
                // Sealed while firing; charge takes ore-chain items,
                // the bank takes its fuel. Taking out is always fine.
                if !bl.lit && slot < 8 {
                    let chain = server.world.reg.bloomery.first().cloned();
                    let want = chain.map(|c| if slot < 4 { c.charge } else { c.fuel });
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
                    let base = server.world.reg.kiln_base;
                    let powders: Vec<crate::registry::ItemId> =
                        server.world.reg.kiln.iter().map(|(p, _)| *p).collect();
                    let ok_put = |it: crate::registry::ItemId| match slot {
                        0..=3 => base.map(|(sa, _, _)| sa) == Some(it),
                        4 => powders.contains(&it),
                        _ => base.map(|(_, fu, _)| fu) == Some(it),
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
        self.net.send(id, &S2C::HeldResult(snap));
        self.send_container(server, id, pos);
    }

    fn send_container(&mut self, server: &Server, id: u32, pos: (i32, i32, i32)) {
        let Some(entity) = server.world.block_entities.get(&pos) else {
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
