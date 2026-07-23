//! Guest connection setup and remote snapshot application.

use super::*;

impl Game {
    /// Join a host: blocks briefly for the QUIC handshake, then the
    /// Welcome/ModFiles flow finishes in remote_pump.
    pub(super) fn join_server(&mut self, addr: std::net::SocketAddr) {
        let name = whoami();
        let hash = net::content_hash(std::path::Path::new("mods"));
        match net::Client::connect(addr, name, hash, self.style.pack()) {
            Ok(client) => {
                self.multiplayer.remote = Some(Remote {
                    client,
                    my_id: 0,
                    block_map: Vec::new(),
                    item_map: Vec::new(),
                    host_block: Default::default(),
                    players: Default::default(),
                    player_held: Default::default(),
                    player_style: Default::default(),
                    names: Default::default(),
                    sleeping: false,
                    player_lerp: Default::default(),
                    player_age: 0.0,
                    player_interval: 0.05,
                    mob_lerp: Default::default(),
                    mob_age: 0.0,
                    mob_interval: 0.05,
                });
                self.multiplayer.join_status = "CONNECTED - SYNCING...".to_string();
            }
            Err(e) => {
                self.multiplayer.join_status = format!("FAILED: {e}").to_uppercase();
            }
        }
    }

    /// Everything a guest does per frame: apply the host's stream, send
    /// our movement. The local Server never advances in remote mode.
    pub(super) fn remote_pump(&mut self, dt: f32) {
        let Some(mut r) = self.multiplayer.remote.take() else {
            return;
        };
        if !r.client.is_connected() && self.in_world {
            self.toast("Disconnected from host.".to_string());
            self.quit_to_title();
            return; // remote dropped
        }
        let msgs = r.client.poll();
        for msg in msgs {
            match msg {
                net::S2C::ModFiles(files) => {
                    // The host's content, cached and loaded as ours.
                    let cache = PathBuf::from("saves/.remote/mods");
                    let _ = std::fs::remove_dir_all(&cache);
                    for (rel, bytes) in files {
                        if rel.contains("..") {
                            continue; // no path escapes
                        }
                        let p = cache.join(rel);
                        if let Some(parent) = p.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        let _ = std::fs::write(p, bytes);
                    }
                    self.content.reg = Arc::new(registry::load(&cache));
                    let (mut data, px, warns) = atlas::build_atlas(
                        &self.content.reg.tex_files,
                        pack_source_of(&self.active_pack_id()),
                        &self.content.reg.tex_names,
                    );
                    atlas::season_tint(&mut data, px, self.server.world.season());
                    self.presentation.atlas_season = self.server.world.season();
                    self.content.pack_warnings = warns;
                    self.renderer.set_atlas(&data, px);
                    self.toast("Synced the host's mods.".to_string());
                }
                net::S2C::Welcome {
                    seed,
                    mode,
                    time,
                    ire,
                    palette,
                    items,
                    your_id,
                    spawn,
                    world_name,
                } => {
                    let mut world = World::new(
                        seed,
                        PathBuf::from("saves/.remote/profile"),
                        self.content.reg.clone(),
                    );
                    world.set_remote(true);
                    world.mode = mode.clone();
                    world.ire = ire;
                    r.my_id = your_id;
                    r.block_map = mp::block_remap(&world, &palette);
                    r.item_map = mp::item_remap(&world, &items);
                    r.host_block = r
                        .block_map
                        .iter()
                        .enumerate()
                        .map(|(host, local)| (local.0, host as u16))
                        .collect();
                    self.server = server::Server::new(world, time, 7);
                    self.renderer.clear_chunks();
                    self.player = Player::new(spawn);
                    self.survival.spawn_point = spawn;
                    self.camera.pos = spawn + Vec3::new(0.0, EYE_HEIGHT, 0.0);
                    self.inventory = Inventory::new();
                    self.survival.armor = [None; 5];
                    self.survival.health = MAX_HEALTH;
                    self.survival.hunger = 20.0;
                    self.creative = mode == "creative";
                    self.in_world = true;
                    self.load_player(&PathBuf::from("saves/.remote/profile"));
                    self.set_screen(Screen::Playing);
                    self.toast(format!("Joined {}.", world_name.to_uppercase()));
                }
                net::S2C::Refused(why) => {
                    if self.in_world {
                        // Kicked mid-game: a clean exit, not a broken
                        // half-local world.
                        self.toast(format!("Removed by host: {why}"));
                        self.quit_to_title();
                    } else {
                        self.multiplayer.join_status = format!("REFUSED: {why}").to_uppercase();
                        self.multiplayer.remote = None;
                    }
                    return;
                }
                net::S2C::Chunk { x, z, rle } => {
                    self.server
                        .world
                        .insert_remote_chunk(ChunkPos { x, z }, &rle, &r.block_map);
                }
                net::S2C::BlockSet { x, y, z, id } => {
                    let local = r
                        .block_map
                        .get(id as usize)
                        .copied()
                        .unwrap_or(self.content.reg.unknown_block);
                    let old = self.server.world.get_block(x, y, z);
                    self.server.world.set_block(x, y, z, local);
                    self.server.world.clear_pending_drops();
                    // Someone broke something: the world crumbles for
                    // everyone watching.
                    if local == crate::registry::AIR
                        && old != crate::registry::AIR
                        && self.content.reg.block(old).hardness.is_some()
                    {
                        let center = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                        if (center - self.camera.pos).length() < 40.0 {
                            self.juice_burst(center, self.content.reg.block(old).tiles[0], 8, 2.0);
                        }
                    }
                }
                net::S2C::Players(list) => {
                    // New span: from wherever each player currently
                    // renders, toward the fresh snapshot.
                    let t = (r.player_age / r.player_interval.max(0.001)).clamp(0.0, 1.0);
                    for (id, pos, yaw, held, pstyle) in list {
                        if id == r.my_id {
                            continue;
                        }
                        r.player_held.insert(id, held);
                        r.player_style.insert(id, pstyle);
                        let cur = match r.player_lerp.get(&id) {
                            Some(l) => l.at(t),
                            None => (pos, yaw),
                        };
                        r.player_lerp.insert(
                            id,
                            Lerp {
                                from: cur.0,
                                to: pos,
                                from_yaw: cur.1,
                                to_yaw: yaw,
                                phase: 0.0,
                            },
                        );
                        let name = r
                            .names
                            .get(&id)
                            .cloned()
                            .unwrap_or_else(|| format!("P{id}"));
                        r.players.insert(id, (name, cur.0, cur.1));
                    }
                    r.player_interval = r.player_age.clamp(0.03, 0.3);
                    r.player_age = 0.0;
                }
                net::S2C::Mobs(snaps) => {
                    let t = (r.mob_age / r.mob_interval.max(0.001)).clamp(0.0, 1.0);
                    let mut lerps = std::collections::HashMap::new();
                    let mobs = snaps
                        .into_iter()
                        .filter(|s| (s.species as usize) < self.content.reg.animals.len())
                        .map(|s| {
                            let (cur, phase) = match r.mob_lerp.get(&s.id) {
                                Some(l) if s.id != 0 => (l.at(t), l.phase),
                                _ => ((s.pos, s.yaw), 0.0),
                            };
                            lerps.insert(
                                s.id,
                                Lerp {
                                    from: cur.0,
                                    to: s.pos,
                                    from_yaw: cur.1,
                                    to_yaw: s.yaw,
                                    phase,
                                },
                            );
                            let mut m = mobs::Mob::new(s.species as usize, cur.0, cur.1);
                            m.id = s.id;
                            m.growth = s.growth;
                            m.hurt_flash = s.hurt;
                            m.fed = s.fed; // "won't take food" — gates guest feeding
                            m.health = 1.0;
                            m.anim_phase = phase;
                            m
                        })
                        .collect();
                    self.server.world.replace_mobs(mobs);
                    r.mob_lerp = lerps; // dead mobs' spans fall away
                    r.mob_interval = r.mob_age.clamp(0.03, 0.3);
                    r.mob_age = 0.0;
                }
                net::S2C::Falling(snaps) => {
                    let falling = snaps
                        .into_iter()
                        .map(|f| world::FallingBlock {
                            pos: f.pos,
                            vel: 0.0,
                            block: r
                                .block_map
                                .get(f.block as usize)
                                .copied()
                                .unwrap_or(self.content.reg.unknown_block),
                        })
                        .collect();
                    self.server.world.replace_falling_blocks(falling);
                }
                net::S2C::Bolts(snaps) => {
                    let projectiles = snaps
                        .into_iter()
                        .map(|s| mobs::Projectile {
                            pos: s.pos,
                            // Dead-reckoned between snapshots below.
                            vel: s.vel,
                            tile: s.tile,
                            damage: 0.0,
                            age: s.age,
                            from_player: false,
                            drop_item: None,
                            owner: 0,
                        })
                        .collect();
                    self.server.world.replace_projectiles(projectiles);
                }
                net::S2C::TimeIre {
                    time,
                    ire,
                    day,
                    weather,
                } => {
                    self.server.time_of_day = time;
                    self.server.world.ire = ire;
                    self.server.world.day = day;
                    self.server.world.weather = world::Weather::from_u8(weather);
                }
                net::S2C::Hit { dmg, from } => self.hurt_player_from_wild(dmg, from),
                net::S2C::Give {
                    item,
                    count,
                    durability,
                } => {
                    if let Some(Some(local)) = r.item_map.get(item as usize) {
                        let reg = self.content.reg.clone();
                        let mut stack = ItemStack::new(&reg, *local, count.max(1));
                        if durability > 0 {
                            stack.durability = durability;
                        }
                        let left = self.inventory.add_stack(&reg, stack);
                        if left == 0 {
                            // Guests harvest over the wire; the ramp
                            // climbs for them too.
                            if self.presentation.juice {
                                self.presentation.pickup_streak.0 =
                                    (self.presentation.pickup_streak.0 + 1).min(24);
                                self.presentation.pickup_streak.1 = 1.5;
                                let p = audio::pickup_pitch(self.presentation.pickup_streak.0 - 1);
                                self.sfx(Sfx::Pickup2(p));
                            } else {
                                self.sfx(Sfx::Pickup);
                            }
                        }
                    }
                }
                net::S2C::Container {
                    x,
                    y,
                    z,
                    kind,
                    slots,
                    aux,
                } => {
                    let reg = self.content.reg.clone();
                    let conv = |s: &Option<net::StackSnap>| -> Option<ItemStack> {
                        let s = s.as_ref()?;
                        let local = (*r.item_map.get(s.item as usize)?)?;
                        Some(ItemStack {
                            item: local,
                            count: s.count,
                            durability: s.durability,
                        })
                    };
                    let pos = (x, y, z);
                    let entity = match kind {
                        0 => {
                            let mut c = world::ChestState::default();
                            for (i, s) in slots.iter().enumerate().take(world::CHEST_SLOTS) {
                                c.slots[i] = conv(s);
                            }
                            world::BlockEntity::Chest(c)
                        }
                        1 => {
                            let f = world::FurnaceState {
                                input: slots.first().and_then(&conv),
                                fuel: slots.get(1).and_then(&conv),
                                output: slots.get(2).and_then(&conv),
                                progress: aux.first().copied().unwrap_or(0.0),
                                burn_left: aux.get(1).copied().unwrap_or(0.0),
                                burn_total: aux.get(2).copied().unwrap_or(0.0),
                                ..Default::default()
                            };
                            world::BlockEntity::Furnace(f)
                        }
                        4 => {
                            let mut k = world::KilnState {
                                lit: aux.first().copied().unwrap_or(0.0) > 0.5,
                                progress: aux.get(1).copied().unwrap_or(0.0)
                                    * world::KILN_FIRE_SECS,
                                ..Default::default()
                            };
                            for (i, sl) in slots.iter().enumerate().take(9) {
                                let st = conv(sl);
                                match i {
                                    0..=3 => k.sand[i] = st,
                                    4 => k.powder = st,
                                    _ => k.fuel[i - 5] = st,
                                }
                            }
                            world::BlockEntity::Kiln(k)
                        }
                        3 => {
                            let mut b = world::BloomeryState {
                                lit: aux.first().copied().unwrap_or(0.0) > 0.5,
                                progress: aux.get(1).copied().unwrap_or(0.0)
                                    * world::BLOOMERY_FIRE_SECS,
                                ..Default::default()
                            };
                            for (i, s) in slots.iter().enumerate().take(8) {
                                if i < 4 {
                                    b.charge[i] = conv(s);
                                } else {
                                    b.fuel[i - 4] = conv(s);
                                }
                            }
                            world::BlockEntity::Bloomery(b)
                        }
                        _ => {
                            let mut o = world::OfferingState::default();
                            for (i, s) in slots.iter().enumerate().take(3) {
                                o.slots[i] = conv(s);
                            }
                            world::BlockEntity::Offering(o)
                        }
                    };
                    self.server.world.insert_block_entity(pos, entity);
                    if matches!(self.ui_state.screen, Screen::Playing) {
                        self.set_screen(match kind {
                            0 => Screen::Chest(pos),
                            1 => Screen::Furnace(pos),
                            3 => Screen::Bloomery(pos),
                            4 => Screen::Kiln(pos),
                            _ => Screen::Offering(pos),
                        });
                    }
                    let _ = reg;
                }
                net::S2C::HeldResult(held) => {
                    // The authoritative cursor after our click replaces
                    // the local prediction (identical on agreement).
                    self.ui_state.held_stack = held.and_then(|s| {
                        let local = (*r.item_map.get(s.item as usize)?)?;
                        Some(ItemStack {
                            item: local,
                            count: s.count,
                            durability: s.durability,
                        })
                    });
                }
                net::S2C::Sleep { sleeping, present } => {
                    self.toast(format!("{sleeping}/{present} sleeping..."));
                }
                net::S2C::Toast(msg) => self.toast(msg),
                net::S2C::Chat { from, msg } => self.toast(format!("{from}: {msg}")),
                net::S2C::Joined { id, name } => {
                    r.names.insert(id, name.clone());
                    if id != r.my_id {
                        self.toast(format!("{name} joined."));
                    }
                }
                net::S2C::Left { id } => {
                    r.players.remove(&id);
                    r.player_lerp.remove(&id);
                    if let Some(n) = r.names.remove(&id) {
                        self.toast(format!("{n} left."));
                    }
                }
            }
        }
        // Snapshot smoothing: glide players and mobs along their spans,
        // dead-reckon bolts, advance walk cycles from apparent speed.
        r.player_age += dt;
        r.mob_age += dt;
        let t = (r.player_age / r.player_interval.max(0.001)).clamp(0.0, 1.0);
        for (id, entry) in r.players.iter_mut() {
            if let Some(l) = r.player_lerp.get(id) {
                let (p, y) = l.at(t);
                entry.1 = p;
                entry.2 = y;
            }
        }
        let t = (r.mob_age / r.mob_interval.max(0.001)).clamp(0.0, 1.0);
        self.server.world.for_each_mob_mut(|m| {
            if let Some(l) = r.mob_lerp.get_mut(&m.id) {
                let (p, y) = l.at(t);
                m.pos = p;
                m.yaw = y;
                let d = l.to - l.from;
                let hspeed = Vec3::new(d.x, 0.0, d.z).length() / r.mob_interval.max(0.03);
                l.phase += hspeed * dt * 3.2; // same feel as the local tick
                m.anim_phase = l.phase;
                m.hurt_flash = (m.hurt_flash - dt).max(0.0);
            }
        });
        self.server.world.for_each_projectile_mut(|p| {
            p.pos += p.vel * dt;
            p.age += dt;
        });
        // Our movement upstream at 20 Hz.
        if self.in_world {
            self.multiplayer.move_timer += dt;
            if self.multiplayer.move_timer >= 0.05 {
                self.multiplayer.move_timer = 0.0;
                let held = self.inventory.slots[self.input.hotbar_sel]
                    .and_then(|st| r.item_map.iter().position(|m| *m == Some(st.item)))
                    .map(|i| i as u16)
                    .unwrap_or(u16::MAX);
                r.client.send_datagram(&net::C2S::Move {
                    pos: self.player.pos,
                    yaw: self.camera.yaw,
                    held,
                });
            }
        }
        self.multiplayer.remote = Some(r);
    }
}
