//! Guest connection setup and remote snapshot application.

use super::*;

impl Game {
    /// The name this client will present to a multiplayer host, plus whether
    /// it came from the explicitly enabled ATProto profile preference.
    pub(super) fn selected_multiplayer_name(&self) -> (String, bool) {
        if let Some(name) = self
            .atproto_account
            .as_ref()
            .filter(|account| account.use_social_display_name)
            .and_then(|account| account.profile_display_name.as_deref())
            .and_then(|name| identity::DisplayName::parse(name).ok())
        {
            return (name.to_string(), true);
        }
        (self.config.display_name.clone(), false)
    }

    fn apply_remote_player_state(
        &mut self,
        remote: &Remote,
        state: net::PlayerStateSnap,
        initial: bool,
    ) {
        if initial {
            self.player = Player::new_at(state.pos);
            // Dev: WILDFORGE_POS frames multiplayer captures too —
            // movement is client-stated, so the host accepts it.
            if let Ok(s) = std::env::var("WILDFORGE_POS") {
                let p: Vec<f32> = s.split(',').filter_map(|v| v.trim().parse().ok()).collect();
                if p.len() == 3
                    && let Ok(pos) = state.pos.relocated_local(Vec3::new(p[0], p[1], p[2]))
                {
                    self.player = Player::new_at(pos);
                }
            }
            self.camera.follow_planet(self.player.eye());
            self.camera.yaw = state.yaw;
            self.camera.pitch = state.pitch;
        }
        self.survival.spawn_point = state.spawn;
        self.inventory = Inventory::new();
        for (index, stack) in state.inventory.into_iter().enumerate() {
            if index >= TOTAL_SLOTS {
                break;
            }
            self.inventory.slots[index] = stack.and_then(|stack| {
                Some(ItemStack {
                    item: *remote.item_map.get(stack.item as usize)?.as_ref()?,
                    count: stack.count,
                    durability: stack.durability,
                    arcane_id: stack.arcane_id,
                })
            });
        }
        self.survival.armor = [None; 5];
        for (index, stack) in state.armor.into_iter().enumerate() {
            if index >= self.survival.armor.len() {
                break;
            }
            self.survival.armor[index] = stack.and_then(|stack| {
                Some(ItemStack {
                    item: *remote.item_map.get(stack.item as usize)?.as_ref()?,
                    count: stack.count,
                    durability: stack.durability,
                    arcane_id: stack.arcane_id,
                })
            });
        }
        self.ui_state.held_stack = state.cursor.and_then(|stack| {
            Some(ItemStack {
                item: *remote.item_map.get(stack.item as usize)?.as_ref()?,
                count: stack.count,
                durability: stack.durability,
                arcane_id: stack.arcane_id,
            })
        });
        self.survival.health = state.health;
        self.survival.hunger = state.hunger;
        self.survival.nutrition = state.nutrition;
        self.input.hotbar_sel = (state.hotbar as usize).min(HOTBAR_SLOTS - 1);
    }

    /// Join a host: blocks briefly for the QUIC handshake, then the
    /// Welcome/ModFiles flow finishes in remote_pump.
    pub(super) fn request_join(
        &mut self,
        addr: std::net::SocketAddr,
        advertised_policy: Option<identity::IdentityPolicy>,
    ) {
        let could_disclose_atproto = self.atproto_account.is_some()
            && advertised_policy != Some(identity::IdentityPolicy::Local);
        if could_disclose_atproto && self.multiplayer.pending_join_disclosure != Some(addr) {
            self.multiplayer.pending_join_disclosure = Some(addr);
            self.multiplayer.join_status =
                "SERVER MAY RESOLVE YOUR PUBLIC DID - CLICK AGAIN".into();
            return;
        }
        self.multiplayer.pending_join_disclosure = None;
        self.multiplayer.join_status = "CONNECTING...".into();
        self.join_server(addr);
    }

    pub(super) fn join_server(&mut self, addr: std::net::SocketAddr) {
        let (name, _) = self.selected_multiplayer_name();
        let hash = net::content_hash(std::path::Path::new("mods"));
        match net::Client::connect(
            addr,
            name,
            hash,
            self.style.pack(),
            &self.identity,
            self.atproto_account.as_ref(),
        ) {
            Ok(client) => {
                let policy = match client.identity_policy {
                    identity::IdentityPolicy::AtprotoRequired => {
                        "VERIFIED ATPROTO REQUIRED; SERVER CAN RESOLVE YOUR PUBLIC PROFILE"
                    }
                    identity::IdentityPolicy::AtprotoOptional => {
                        "LOCAL OR ATPROTO IDENTITY ACCEPTED"
                    }
                    identity::IdentityPolicy::Local => "LOCAL IDENTITIES ACCEPTED",
                };
                let admission = match client.admission_policy {
                    identity::AdmissionPolicy::Open => "OPEN ADMISSION",
                    identity::AdmissionPolicy::Allowlist => "ALLOWLIST ONLY",
                };
                self.multiplayer.remote = Some(Remote {
                    client,
                    my_id: 0,
                    role: identity::Role::Player,
                    block_map: Vec::new(),
                    item_map: Vec::new(),
                    host_block: Default::default(),
                    players: Default::default(),
                    player_positions: Default::default(),
                    player_held: Default::default(),
                    player_implement: Default::default(),
                    player_style: Default::default(),
                    names: Default::default(),
                    sleeping: false,
                    player_lerp: Default::default(),
                    player_age: 0.0,
                    player_interval: 0.05,
                    mob_lerp: Default::default(),
                    mob_age: 0.0,
                    mob_interval: 0.05,
                    players_rx: Default::default(),
                    mobs_rx: Default::default(),
                    bolts_rx: Default::default(),
                    loose_items_rx: Default::default(),
                    falling_rx: Default::default(),
                    // Until the host answers, assume the old fixed ring.
                    granted_view_dist: 5,
                    asked_view_dist: 0,
                    wants: Default::default(),
                    entry_required: Default::default(),
                    entry_manifest_received: false,
                    entry_ready_sent: false,
                    entry_world_name: None,
                    entry_center: None,
                    entry_center_meshed: false,
                    pending_entry_chunks: Default::default(),
                    entry_activity: std::time::Instant::now(),
                });
                self.multiplayer.join_status = format!("{policy} - {admission} - SYNCING...");
            }
            Err(e) => {
                self.multiplayer.join_status = format!("FAILED: {e}").to_uppercase();
            }
        }
    }

    /// Ask the host for chunks we should have and do not.
    ///
    /// Nearest first, a few per frame, bounded by the granted radius. This is
    /// what closes the hole a guest used to leave behind by walking away and
    /// coming back: the host remembers what it sent forever, so without this
    /// the ground never returned.
    fn request_missing_chunks(&mut self, r: &mut Remote) {
        const ASK_PER_FRAME: usize = 4;
        let vd = r.granted_view_dist.min(self.config.view_dist);
        let Some(center) = self.player.pos.chunk() else {
            return;
        };
        let mut asked = 0;
        for ring in 0..=vd {
            for dx in -ring..=ring {
                for dz in -ring..=ring {
                    if dx.abs().max(dz.abs()) != ring {
                        continue;
                    }
                    let pos = center.offset(dx, dz);
                    if pos.distance(center) > f64::from(vd * CHUNK_X as i32) + 1.0 {
                        continue;
                    }
                    if self.server.world.has_chunk(pos) || !r.wants.insert(pos) {
                        continue;
                    }
                    r.client.send(&net::C2S::RequestChunk {
                        face: pos.face() as u8,
                        u: pos.u(),
                        v: pos.v(),
                    });
                    asked += 1;
                    if asked >= ASK_PER_FRAME {
                        return;
                    }
                }
            }
        }
    }

    /// Everything a guest does per frame: apply the host's stream, send
    /// our movement. The local Server never advances in remote mode.
    pub(super) fn remote_pump(&mut self, dt: f32) {
        let Some(mut r) = self.multiplayer.remote.take() else {
            return;
        };
        if !r.client.is_connected() {
            if self.in_world {
                self.toast("Disconnected from host.".to_string());
                self.quit_to_title();
            } else {
                self.multiplayer.join_status = "DISCONNECTED DURING WORLD PREPARATION".into();
                self.multiplayer.remote = None;
            }
            return;
        }
        let msgs = r.client.poll();
        if !msgs.is_empty() {
            r.entry_activity = std::time::Instant::now();
        } else if !self.in_world && r.entry_activity.elapsed().as_secs() > 15 {
            self.multiplayer.join_status = "WORLD PREPARATION TIMED OUT".into();
            self.multiplayer.remote = None;
            return;
        }
        let mut block_updates = Vec::new();
        for msg in msgs {
            match msg {
                net::S2C::Challenge { .. } => {}
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
                    let mut atlas = atlas::build_atlas(
                        &self.content.reg.tex_files,
                        &atlas::pack_chain(&self.active_pack_id()),
                        &self.content.reg.tex_names,
                    );
                    let season = self
                        .server
                        .world
                        .season_at_surface(self.player.pos.surface());
                    atlas::season_tint(&mut atlas.color, atlas.px, season);
                    self.presentation.atlas_season = season;
                    self.content.pack_warnings = atlas.warnings;
                    self.renderer.set_atlas(
                        &atlas.color,
                        &atlas.material,
                        &atlas.normal,
                        atlas.px,
                        atlas.interior_base,
                        &atlas.layer_params,
                    );
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
                    your_role,
                    roster,
                    spawn: _,
                    world_name,
                    player_state,
                } => {
                    let mut world = World::new(
                        seed,
                        PathBuf::from("saves/.remote/world-cache"),
                        self.content.reg.clone(),
                    );
                    world.set_remote(true);
                    self.gen_pool = None; // chunks come by wire
                    self.mesh_pool = Some(crate::game::streaming::MeshPool::new());
                    world.mode = mode.clone();
                    world.ire = ire;
                    r.my_id = your_id;
                    r.role = your_role;
                    if roster
                        .iter()
                        .find(|presence| presence.id == your_id)
                        .is_some_and(|presence| presence.cached_verification)
                    {
                        self.toast(
                            "ATProto verified from the server's bounded outage cache.".into(),
                        );
                    }
                    r.names = roster
                        .into_iter()
                        .map(|presence| (presence.id, presence_label(&presence)))
                        .collect();
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
                    self.apply_remote_player_state(&r, player_state, true);
                    self.creative = mode == "creative";
                    self.in_world = false;
                    r.entry_required.clear();
                    r.entry_manifest_received = false;
                    r.entry_ready_sent = false;
                    r.entry_world_name = Some(world_name);
                    r.entry_center = None;
                    r.entry_center_meshed = false;
                    r.pending_entry_chunks.clear();
                    self.multiplayer.join_status = "PREPARING SAFE WORLD ENTRY...".into();
                }
                net::S2C::EntryManifest { spawn, required } => {
                    if spawn != self.player.pos {
                        self.multiplayer.join_status =
                            "FAILED: ENTRY MANIFEST DID NOT MATCH WELCOME SPAWN".into();
                        self.multiplayer.remote = None;
                        return;
                    }
                    r.entry_required = required.into_iter().collect();
                    r.entry_manifest_received = true;
                    r.entry_center = spawn.chunk();
                }
                net::S2C::EntryProgress { resident, total } => {
                    self.multiplayer.join_status =
                        format!("PREPARING SAFE WORLD ENTRY... {resident}/{total}");
                }
                net::S2C::EntryAccepted => {
                    if !r.entry_ready_sent || !r.entry_required.is_empty() {
                        self.multiplayer.join_status =
                            "FAILED: HOST ACCEPTED ENTRY BEFORE TERRAIN WAS READY".into();
                        self.multiplayer.remote = None;
                        return;
                    }
                    if !r.entry_center_meshed {
                        self.multiplayer.join_status =
                            "FAILED: HOST ACCEPTED ENTRY BEFORE THE FIRST FRAME WAS READY".into();
                        self.multiplayer.remote = None;
                        return;
                    }
                    self.in_world = true;
                    self.set_screen(Screen::Playing);
                    self.multiplayer.join_status.clear();
                    let world_name = r.entry_world_name.take().unwrap_or_else(|| "world".into());
                    self.toast(format!("Joined {}.", world_name.to_uppercase()));
                }
                net::S2C::Refused(why) => {
                    if self.in_world {
                        // Kicked mid-game: a clean exit, not a broken
                        // half-local world.
                        self.toast(format!("Removed by host: {}", why.detail));
                        self.quit_to_title();
                    } else {
                        self.multiplayer.join_status =
                            format!("REFUSED: {}", why.detail).to_uppercase();
                        self.multiplayer.remote = None;
                    }
                    return;
                }
                net::S2C::Chunk { face, u, v, rle } => {
                    let Some(face) = crate::planet::Face::from_u8(face) else {
                        continue;
                    };
                    let Ok(pos) = ChunkPos::new(face, u, v) else {
                        continue;
                    };
                    // Never decode an arbitrarily large host burst inline.
                    // A prepared host can encode the whole view faster than a
                    // software-rendered client presents frames; inserting all
                    // of those chunks here froze the UI immediately after
                    // EntryAccepted. The paced adoption stage below is shared
                    // by admission and ordinary view expansion.
                    if !self.server.world.has_chunk(pos)
                        && !r
                            .pending_entry_chunks
                            .iter()
                            .any(|(queued, _)| *queued == pos)
                    {
                        // Proactively pushed chunks are pending too; marking
                        // them wanted prevents the repair scan from asking for
                        // duplicates before paced adoption reaches them.
                        r.wants.insert(pos);
                        r.pending_entry_chunks.push_back((pos, rle));
                    }
                }
                net::S2C::BlockSet {
                    pos,
                    id,
                    meta,
                    salt_mass,
                    soil_salinity,
                } => {
                    let local = r
                        .block_map
                        .get(id as usize)
                        .copied()
                        .unwrap_or(self.content.reg.unknown_block);
                    let old = self.server.world.get_block_at(pos);
                    block_updates.push((pos, local, meta, salt_mass, soil_salinity));
                    // Someone broke something: the world crumbles for
                    // everyone watching.
                    if local == crate::registry::AIR
                        && old != crate::registry::AIR
                        && self.content.reg.block(old).hardness.is_some()
                    {
                        let center = Vec3::new(
                            pos.surface().centered_u() as f32 + 0.5,
                            pos.y() as f32 + 0.5,
                            pos.surface().centered_v() as f32 + 0.5,
                        );
                        if (center - self.camera.pos).length() < 40.0 {
                            self.juice_burst(center, self.content.reg.block(old).tiles[0], 8, 2.0);
                        }
                    }
                }
                net::S2C::Players(part) => {
                    let Some(list) = r.players_rx.accept(part) else {
                        continue;
                    };
                    // Anyone the host stopped mentioning has walked out of
                    // our reach; drop them rather than leaving a statue.
                    let present: std::collections::HashSet<u32> =
                        list.iter().map(|(id, ..)| *id).collect();
                    r.players.retain(|id, _| present.contains(id));
                    r.player_positions.retain(|id, _| present.contains(id));
                    r.player_lerp.retain(|id, _| present.contains(id));
                    r.player_held.retain(|id, _| present.contains(id));
                    r.player_implement.retain(|id, _| present.contains(id));
                    r.player_style.retain(|id, _| present.contains(id));
                    // New span: from wherever each player currently
                    // renders, toward the fresh snapshot.
                    let t = (r.player_age / r.player_interval.max(0.001)).clamp(0.0, 1.0);
                    for (id, pos, yaw, held, pstyle, implement) in list {
                        if id == r.my_id {
                            continue;
                        }
                        r.player_held.insert(id, held);
                        if let Some(visual) = implement {
                            r.player_implement.insert(id, visual);
                        } else {
                            r.player_implement.remove(&id);
                        }
                        r.player_style.insert(id, pstyle);
                        r.player_positions.insert(id, pos);
                        let render_pos = pos.render_pos();
                        let cur = match r.player_lerp.get(&id) {
                            Some(l) => l.at(t),
                            None => (render_pos, yaw),
                        };
                        r.player_lerp.insert(
                            id,
                            Lerp {
                                from: cur.0,
                                to: render_pos,
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
                net::S2C::Mobs(part) => {
                    let Some(snaps) = r.mobs_rx.accept(part) else {
                        continue;
                    };
                    let t = (r.mob_age / r.mob_interval.max(0.001)).clamp(0.0, 1.0);
                    let mut lerps = std::collections::HashMap::new();
                    let mobs = snaps
                        .into_iter()
                        .filter(|s| (s.species as usize) < self.content.reg.animals.len())
                        .map(|s| {
                            let render_pos = s.pos.render_pos();
                            let (cur, phase) = match r.mob_lerp.get(&s.id) {
                                Some(l) if s.id != 0 => (l.at(t), l.phase),
                                _ => ((render_pos, s.yaw), 0.0),
                            };
                            lerps.insert(
                                s.id,
                                Lerp {
                                    from: cur.0,
                                    to: render_pos,
                                    from_yaw: cur.1,
                                    to_yaw: s.yaw,
                                    phase,
                                },
                            );
                            let mut m = mobs::Mob::new_at(s.species as usize, s.pos, cur.1);
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
                net::S2C::ViewDistance { chunks } => {
                    r.granted_view_dist = chunks.max(1) as i32;
                }
                net::S2C::Falling(part) => {
                    let Some(snaps) = r.falling_rx.accept(part) else {
                        continue;
                    };
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
                net::S2C::Bolts(part) => {
                    let Some(snaps) = r.bolts_rx.accept(part) else {
                        continue;
                    };
                    let projectiles = snaps
                        .into_iter()
                        .map(|s| mobs::Projectile {
                            stable_id: s.id,
                            pos: s.pos,
                            // Dead-reckoned between snapshots below.
                            vel: s.vel,
                            tile: s.tile,
                            damage: 0.0,
                            age: s.age,
                            from_player: false,
                            drop_item: None,
                            preparation_payload: None,
                            owner: 0,
                        })
                        .collect();
                    self.server.world.replace_projectiles(projectiles);
                }
                net::S2C::LooseItems(part) => {
                    let Some(snaps) = r.loose_items_rx.accept(part) else {
                        continue;
                    };
                    let items = snaps
                        .into_iter()
                        .filter_map(|snap| {
                            let item = (*r.item_map.get(snap.item as usize)?)?;
                            let mut entity = ItemEntity::new(snap.pos, snap.vel, item, snap.count);
                            entity.stable_id = snap.id;
                            entity.age = snap.age;
                            entity.durability =
                                snap.durability.min(self.content.reg.item(item).durability);
                            entity.arcane_id = snap.arcane_id;
                            Some(entity)
                        })
                        .collect();
                    self.server.world.replace_loose_items(items);
                }
                net::S2C::TimeIre { time, ire, day } => {
                    self.server.time_of_day = time;
                    self.server.world.ire = ire;
                    self.server.world.day = day;
                }
                net::S2C::WeatherCells { side, cells } => {
                    self.server.world.set_remote_weather(side, cells);
                }
                net::S2C::ArcaneCue {
                    bands,
                    dominant,
                    ecology,
                } => {
                    self.server
                        .world
                        .set_remote_arcane_cue(bands, dominant, ecology);
                }
                net::S2C::ArcaneItems {
                    reset,
                    charges,
                    implements,
                    apparatus,
                } => {
                    if reset {
                        self.server.world.clear_remote_implement_snapshot();
                    }
                    self.server.world.extend_remote_arcane_items(charges);
                    self.server.world.extend_remote_implements(implements);
                    self.server.world.extend_remote_apparatus(apparatus);
                }
                net::S2C::DiscoveryReport(record) => {
                    self.present_discovery_record(&record);
                }
                net::S2C::DiscoveryRecords {
                    holder,
                    records,
                    capacity,
                } => self.receive_discovery_catalogue(holder, records, capacity),
                net::S2C::KnowledgeText {
                    instance_id: _,
                    text,
                } => self.toast(text),
                net::S2C::BindingFrameResult { pos, result } => {
                    self.interaction
                        .binding_revisions
                        .insert(pos, result.revision);
                    let cue = result.cue;
                    self.presentation.swing = 1.0;
                    self.toast(result.message);
                    for line in result.lines.into_iter().take(3) {
                        self.toast(line);
                    }
                    self.sfx(match cue {
                        crate::implements::ImplementCue::Use => Sfx::ImplementUse,
                        crate::implements::ImplementCue::Transfer => Sfx::ImplementTransfer,
                        crate::implements::ImplementCue::Strain => Sfx::ImplementStrain,
                        crate::implements::ImplementCue::Empty => Sfx::ImplementEmpty,
                        crate::implements::ImplementCue::Failure => Sfx::ImplementFailure,
                    });
                }
                net::S2C::AlchemyResult { pos, result } => {
                    self.interaction
                        .alchemy_revisions
                        .insert(pos, result.revision);
                    self.present_alchemy_cue(result.cue);
                }
                net::S2C::PreparationResult(result) => {
                    self.present_alchemy_cue(result.cue);
                }
                net::S2C::PreparationState {
                    modifiers,
                    bodily_dross,
                } => {
                    let old_dross_band = self.survival.preparation_modifiers.dross_band;
                    self.survival.preparation_modifiers = modifiers;
                    self.survival.bodily_dross = bodily_dross;
                    if modifiers.dross_band > old_dross_band && modifiers.dross_band != 0 {
                        self.sfx(Sfx::DrossWarning(modifiers.dross_band));
                        let (band, pattern) =
                            super::status::dross_warning_text(modifiers.dross_band);
                        self.toast(format!("DROSS {band} — {pattern}"));
                    }
                }
                net::S2C::DrossEvent(cue) => self.present_dross_cue(cue),
                net::S2C::AlchemyEvent(cue) => self.present_alchemy_cue(cue),
                net::S2C::ImplementActivation {
                    actor,
                    pos,
                    cue,
                    visual,
                } => {
                    let item_map = r.item_map.clone();
                    self.present_implement_activation(pos, cue, visual, Some(&item_map));
                    // The next player snapshot remains authoritative for the
                    // held model; this short-lived event only drives the
                    // visible settling gesture and local envelope.
                    if actor != r.my_id {
                        r.player_age = r.player_age.min(r.player_interval * 0.5);
                    }
                }
                net::S2C::WorkingResult(result) => {
                    if result.success {
                        if let Some(channel) = self.interaction.working.as_mut()
                            && result.phase.is_some()
                        {
                            channel.stable_id = result.stable_id;
                        }
                        if result.phase.is_none() {
                            self.interaction.working = None;
                        }
                    } else {
                        self.interaction.working = None;
                    }
                    self.toast(result.message);
                    self.sfx(if result.success {
                        Sfx::ImplementUse
                    } else {
                        Sfx::ImplementFailure
                    });
                }
                net::S2C::WorkingEvent(cue) => self.present_working_cue(cue),
                net::S2C::Hit { dmg, from } => self.hurt_player_from_wild(dmg, from),
                net::S2C::Give {
                    item,
                    count,
                    durability,
                    arcane_id,
                    current_units,
                } => {
                    if let Some(Some(local)) = r.item_map.get(item as usize) {
                        let reg = self.content.reg.clone();
                        let mut stack = ItemStack::new(&reg, *local, count.max(1));
                        if durability > 0 {
                            stack.durability = durability;
                        }
                        stack.arcane_id = arcane_id;
                        self.server
                            .world
                            .set_remote_arcane_item(arcane_id, current_units);
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
                net::S2C::PlayerState(state) => {
                    self.apply_remote_player_state(&r, state, false);
                }
                net::S2C::SignText { pos, lines } => {
                    self.server.world.insert_block_entity_at(
                        pos,
                        world::BlockEntity::Sign(world::SignState { lines }),
                    );
                }
                net::S2C::MobCargo { id, slots } => {
                    // The host's pack truth: mirror it onto the local
                    // snapshot mob and open the screen if we asked.
                    let mut cargo: Box<[Option<ItemStack>; 12]> = Default::default();
                    for (i, sl) in slots.iter().enumerate().take(12) {
                        cargo[i] = sl.as_ref().map(|sn| ItemStack {
                            item: crate::registry::ItemId(sn.item),
                            count: sn.count,
                            durability: sn.durability,
                            arcane_id: sn.arcane_id,
                        });
                    }
                    if let Some(m) = self.server.world.mob_by_id_mut(id) {
                        m.cargo = Some(cargo);
                    }
                    if matches!(self.ui_state.screen, Screen::Playing) {
                        self.set_screen(Screen::MobCargo(id));
                    }
                }
                net::S2C::Container {
                    pos,
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
                            arcane_id: s.arcane_id,
                        })
                    };
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
                            let mut k = world::MachineInstance {
                                kind: world::multiblock::MachineKind::Kiln,
                                lit: aux.first().copied().unwrap_or(0.0) > 0.5,
                                progress: aux.get(1).copied().unwrap_or(0.0)
                                    * world::KILN_FIRE_SECS,
                                ..Default::default()
                            };
                            for (i, sl) in slots.iter().enumerate().take(9) {
                                let st = conv(sl);
                                match i {
                                    0..=3 => k.charge[i] = st,
                                    4 => k.reagent = st,
                                    _ => k.fuel[i - 5] = st,
                                }
                            }
                            world::BlockEntity::Multiblock(k)
                        }
                        6 => {
                            let mut st = world::StallState::default();
                            for (i, sl) in slots.iter().enumerate().take(13) {
                                let stk = conv(sl);
                                match i {
                                    0..=5 => st.goods[i] = stk,
                                    6 => st.price = stk,
                                    _ => st.till[i - 7] = stk,
                                }
                            }
                            // aux[0] carries "you own this" — marked
                            // with a sentinel owner so the UI knows.
                            if aux.first().copied().unwrap_or(0.0) > 0.5 {
                                st.owner = [1; 16];
                            }
                            world::BlockEntity::Stall(st)
                        }
                        3 | 5 => {
                            let secs = if kind == 5 {
                                world::FORGE_FIRE_SECS
                            } else {
                                world::BLOOMERY_FIRE_SECS
                            };
                            let mkind = if kind == 5 {
                                world::multiblock::MachineKind::Forge
                            } else {
                                world::multiblock::MachineKind::Bloomery
                            };
                            let mut b = world::MachineInstance {
                                kind: mkind,
                                lit: aux.first().copied().unwrap_or(0.0) > 0.5,
                                progress: aux.get(1).copied().unwrap_or(0.0) * secs,
                                ..Default::default()
                            };
                            for (i, s) in slots.iter().enumerate().take(8) {
                                if i < 4 {
                                    b.charge[i] = conv(s);
                                } else {
                                    b.fuel[i - 4] = conv(s);
                                }
                            }
                            world::BlockEntity::Multiblock(b)
                        }
                        _ => {
                            let mut o = world::OfferingState::default();
                            for (i, s) in slots.iter().enumerate().take(3) {
                                o.slots[i] = conv(s);
                            }
                            world::BlockEntity::Offering(o)
                        }
                    };
                    self.server.world.insert_block_entity_at(pos, entity);
                    if matches!(self.ui_state.screen, Screen::Playing) {
                        self.set_screen(match kind {
                            0 => Screen::Chest(pos),
                            1 => Screen::Furnace(pos),
                            3 => Screen::Bloomery(pos),
                            4 => Screen::Kiln(pos),
                            5 => Screen::Bloomery(pos),
                            6 => Screen::Stall(pos),
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
                            arcane_id: s.arcane_id,
                        })
                    });
                }
                net::S2C::Sleep { sleeping, present } => {
                    self.toast(format!("{sleeping}/{present} sleeping..."));
                }
                net::S2C::Toast(msg) => self.toast(msg),
                net::S2C::Chat { from, msg } => self.toast(format!("{from}: {msg}")),
                net::S2C::Joined { presence } => {
                    r.names.insert(presence.id, presence_label(&presence));
                    if presence.id != r.my_id {
                        self.toast(format!("{} joined.", presence.display_name));
                    }
                }
                net::S2C::Left { id } => {
                    r.players.remove(&id);
                    r.player_positions.remove(&id);
                    r.player_lerp.remove(&id);
                    if let Some(n) = r.names.remove(&id) {
                        self.toast(format!("{n} left."));
                    }
                }
                net::S2C::RoleChanged { role } => {
                    r.role = role;
                    self.toast(format!("Your server role is now {role:?}."));
                }
            }
        }
        // Decode terrain at a fixed cadence. During admission this brings in
        // the exact safety set; afterward it prevents a fast host's full-view
        // burst from monopolizing the render/input thread. Once all nine entry
        // chunks are resident, build the spawn chunk's first visible mesh
        // before claiming readiness; Welcome by itself never exposes a blank
        // world.
        const REMOTE_CHUNKS_PER_FRAME: usize = 8;
        let mut terrain_batch = Vec::with_capacity(REMOTE_CHUNKS_PER_FRAME);
        for _ in 0..REMOTE_CHUNKS_PER_FRAME {
            let Some((position, rle)) = r.pending_entry_chunks.pop_front() else {
                break;
            };
            r.wants.remove(&position);
            r.entry_required.remove(&position);
            terrain_batch.push((position, rle));
        }
        if !terrain_batch.is_empty() {
            self.server.world.insert_remote_chunks(
                terrain_batch
                    .iter()
                    .map(|(position, rle)| (*position, rle.as_slice())),
                &r.block_map,
            );
        }
        if !block_updates.is_empty() {
            self.server.world.apply_remote_block_states(block_updates);
        }
        if r.entry_manifest_received
            && r.entry_required.is_empty()
            && !r.entry_ready_sent
            && let Some(center) = r.entry_center
            && [(-1, 0), (1, 0), (0, -1), (0, 1)]
                .iter()
                .all(|(du, dv)| self.server.world.has_chunk(center.offset(*du, *dv)))
        {
            let mesh = mesher::mesh_chunk(&self.server.world, center, &self.content.tile_variants);
            self.renderer.upload_chunk(center, &mesh);
            self.presentation.lights.chunk_meshed(center, mesh.emitters);
            self.server.world.mark_chunk_meshed(center);
            r.entry_center_meshed = true;
            r.client.send(&net::C2S::EntryReady);
            r.entry_ready_sent = true;
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
                let (_, y) = l.at(t);
                m.yaw = y;
                let d = l.to - l.from;
                let hspeed = Vec3::new(d.x, 0.0, d.z).length() / r.mob_interval.max(0.03);
                l.phase += hspeed * dt * 3.2; // same feel as the local tick
                m.anim_phase = l.phase;
                m.hurt_flash = (m.hurt_flash - dt).max(0.0);
            }
        });
        self.server.world.for_each_projectile_mut(|p| {
            if let Ok(moved) = p.pos.translated(p.vel * dt) {
                p.pos = moved.pos;
                p.vel = moved.rotation.rotate_vec3(p.vel);
            }
            p.age += dt;
        });
        // Our movement upstream at 20 Hz.
        if self.in_world {
            self.multiplayer.move_timer += dt;
            if self.multiplayer.move_timer >= 0.05 {
                self.multiplayer.move_timer = 0.0;
                r.client.send_datagram(&net::C2S::Move {
                    pos: self.player.pos,
                    yaw: self.camera.yaw,
                    hotbar: self.input.hotbar_sel as u8,
                    sprint: self.input.keys.sprint,
                });
            }
            // Tell the host how far we want to see, whenever that changes.
            // The host clamps and answers; until it does we keep the ring
            // we were given.
            let want = self.config.view_dist;
            if want != r.asked_view_dist {
                r.asked_view_dist = want;
                r.client
                    .send(&net::C2S::SetViewDistance { chunks: want as u8 });
            }
            // Ask for terrain we are missing inside the granted radius.
            // The host pushes a ring as we walk, but ground we evicted and
            // came back to is ours to ask for — it believes we still have it.
            self.request_missing_chunks(&mut r);
        }
        self.multiplayer.remote = Some(r);
    }
}

fn presence_label(presence: &net::PlayerPresence) -> String {
    let handle = presence
        .handle
        .as_deref()
        .map(|handle| format!(" @{handle}"))
        .unwrap_or_default();
    if presence.cached_verification {
        format!("{}{handle} [VERIFIED/CACHED]", presence.display_name)
    } else if presence.verified {
        format!("{}{handle} [VERIFIED]", presence.display_name)
    } else {
        presence.display_name.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roster_labels_only_show_an_explicitly_disclosed_handle() {
        let private = net::PlayerPresence {
            id: 1,
            display_name: "MOSS".into(),
            verified: true,
            cached_verification: false,
            handle: None,
        };
        assert_eq!(presence_label(&private), "MOSS [VERIFIED]");

        let public = net::PlayerPresence {
            handle: Some("moss.example".into()),
            ..private
        };
        assert_eq!(presence_label(&public), "MOSS @moss.example [VERIFIED]");
    }
}
