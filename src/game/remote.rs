//! Guest connection setup and remote snapshot application.

use crate::atlas;
use crate::audio;
use crate::audio::Sfx;
use crate::chunk::CHUNK_X;
use crate::chunk::ChunkPos;
use crate::identity;
use crate::inventory::HOTBAR_SLOTS;
use crate::inventory::ItemStack;
use crate::mesher;
use crate::net;
use crate::physics::Player;
use crate::server;
use crate::world;
use crate::world::World;
use glam::Vec3;
use std::path::PathBuf;
use std::sync::Arc;
use super::Game;
use super::Lerp;
use super::Remote;
use super::navigation::Screen;
use crate::client_session::{ContentMap, GuestSession, PresentationRequirement};

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
        content: &ContentMap,
        state: net::PlayerStateSnap,
        initial: bool,
    ) {
        if initial {
            self.player = Player::new_at(state.pos);
            // Dev: WILDFORGE_POS frames multiplayer captures too —
            // movement is client-stated, so the host accepts it.
            if let Ok(s) = std::env::var("WILDFORGE_POS") {
                let p: Vec<f32> = s.split(',').filter_map(|v| v.trim().parse().ok()).collect();
                let face = std::env::var("WILDFORGE_FACE")
                    .ok()
                    .as_deref()
                    .and_then(crate::planet::Face::from_name)
                    .unwrap_or(state.pos.face());
                if p.len() == 3
                    && let Ok(pos) =
                        crate::planet::EntityPos::from_local(face, Vec3::new(p[0], p[1], p[2]))
                {
                    self.player = Player::new_at(pos);
                }
            }
            self.camera.follow_planet(self.player.eye());
            self.camera.yaw = state.yaw;
            self.camera.pitch = state.pitch;
        }
        self.survival.spawn_point = state.spawn;
        self.inventory.slots = content.slots(&state.inventory);
        self.survival.armor = content.slots(&state.armor);
        self.ui_state.held_stack = state.cursor.as_ref().and_then(|stack| content.stack(stack));
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
        if let Err(error) = self.content.reg.validate() {
            eprintln!("join: {error}");
            self.multiplayer.join_status = format!("FAILED: {error}").to_uppercase();
            return;
        }
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
                    my_id: 0,
                    role: identity::Role::Player,
                    session: GuestSession::with_client(
                        Arc::clone(&self.content.reg),
                        PresentationRequirement::FirstFrame,
                        std::time::Instant::now(),
                        client,
                    ),
                    players: Default::default(),
                    player_positions: Default::default(),
                    player_held: Default::default(),
                    player_implement: Default::default(),
                    player_style: Default::default(),
                    sleeping: false,
                    player_lerp: Default::default(),
                    player_age: 0.0,
                    player_interval: 0.05,
                    mob_lerp: Default::default(),
                    mob_age: 0.0,
                    mob_interval: 0.05,
                    // Until the host answers, assume the old fixed ring.
                    granted_view_dist: 5,
                    asked_view_dist: 0,
                    wants: Default::default(),
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
                    r.session.send(&net::C2S::RequestChunk {
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
        if !r.session.is_connected() {
            r.session.close();
            if self.in_world {
                self.toast("Disconnected from host.".to_string());
                self.quit_to_title();
            } else {
                self.multiplayer.join_status = "DISCONNECTED DURING WORLD PREPARATION".into();
                self.multiplayer.remote = None;
            }
            return;
        }
        let msgs = r.session.poll();
        if !msgs.is_empty() {
            r.session.note_activity(std::time::Instant::now());
        } else if !self.in_world && r.session.admission().timed_out(std::time::Instant::now()) {
            self.multiplayer.join_status = "WORLD PREPARATION TIMED OUT".into();
            self.multiplayer.remote = None;
            return;
        }
        for msg in msgs {
            let Some(msg) = r.session.apply_world_message(
                msg, &mut self.server.world, &mut self.server.time_of_day,
            ) else {
                continue;
            };
            match msg {
                // Consumed by the shared replica-domain dispatch above.
                net::S2C::TimeIre { .. } | net::S2C::WeatherCells { .. }
                | net::S2C::ArcaneCue { .. } | net::S2C::ArcaneItems { .. }
                | net::S2C::SignText { .. } | net::S2C::SwitchState { .. } => {}
                net::S2C::Challenge { .. } => {}
                net::S2C::ModFiles(files) => {
                    let cache = PathBuf::from("saves/.remote/mods");
                    match r.session.install_content(&cache, files) {
                        Ok(registry) => self.content.reg = registry,
                        Err(error) => {
                            self.multiplayer.join_status = format!("FAILED: {error}").to_uppercase();
                            return;
                        }
                    }
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
                    self.mesh_pool = match crate::game::mesh_jobs::MeshPool::new() {
                        Ok(meshes) => Some(meshes),
                        Err(error) => {
                            eprintln!("guest: mesh workers could not start: {error}");
                            self.mesh_pool = None;
                            self.renderer.clear_chunks();
                            self.in_world = false;
                            self.set_screen(Screen::Title);
                            self.toast(format!("Could not enter host world: {error}"));
                            return; // local RemoteSession drops and disconnects
                        }
                    };
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
                    let content = ContentMap::new(Arc::clone(&self.content.reg), palette, items);
                    self.server = server::Server::new(world, time, 7);
                    self.renderer.clear_chunks();
                    self.apply_remote_player_state(&content, player_state, true);
                    self.creative = mode == "creative";
                    self.in_world = false;
                    r.session.begin(
                        content,
                        world_name,
                        self.player.pos,
                        std::time::Instant::now(),
                    );
                    r.session.set_roster(roster);
                    r.players.clear();
                    r.player_positions.clear();
                    r.player_held.clear();
                    r.player_implement.clear();
                    r.player_style.clear();
                    r.player_lerp.clear();
                    r.mob_lerp.clear();
                    r.player_age = 0.0;
                    r.mob_age = 0.0;
                    r.wants.clear();
                    self.multiplayer.join_status = "PREPARING SAFE WORLD ENTRY...".into();
                }
                net::S2C::EntryManifest { spawn, required } => {
                    if let Err(error) = r.session.manifest(spawn, required, &self.server.world) {
                        self.multiplayer.join_status = format!("FAILED: {error}").to_uppercase();
                        self.multiplayer.remote = None;
                        return;
                    }
                }
                net::S2C::EntryProgress { resident, total } => {
                    self.multiplayer.join_status =
                        format!("PREPARING SAFE WORLD ENTRY... {resident}/{total}");
                }
                net::S2C::EntryAccepted => {
                    let world_name = match r.session.accepted() {
                        Ok(name) => name,
                        Err(error) => {
                            self.multiplayer.join_status =
                                format!("FAILED: {error}").to_uppercase();
                            self.multiplayer.remote = None;
                            return;
                        }
                    };
                    self.in_world = true;
                    self.set_screen(Screen::Playing);
                    self.multiplayer.join_status.clear();
                    self.toast(format!("Joined {}.", world_name.to_uppercase()));
                }
                net::S2C::Refused(why) => {
                    r.session.close();
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
                    if !self.server.world.has_chunk(pos) && !r.session.has_queued_chunk(pos) {
                        // Proactively pushed chunks are pending too; marking
                        // them wanted prevents the repair scan from asking for
                        // duplicates before paced adoption reaches them.
                        r.wants.insert(pos);
                        r.session.queue_chunk(pos, rle);
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
                        .session
                        .queue_block(pos, id, meta, salt_mass, soil_salinity);
                    let old = self.server.world.get_block_at(pos);
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
                            self.presentation.burst(center, self.content.reg.block(old).tiles[0], 8, 2.0);
                        }
                    }
                }
                net::S2C::Players(part) => {
                    let Some(list) = r.session.players(part) else {
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
                            .session.roster()
                            .get(&id)
                            .map(presence_label)
                            .unwrap_or_else(|| format!("P{id}"));
                        r.players.insert(id, (name, cur.0, cur.1));
                    }
                    r.player_interval = r.player_age.clamp(0.03, 0.3);
                    r.player_age = 0.0;
                }
                net::S2C::Mobs(part) => {
                    let Some(mut mobs) = r.session.mobs(part) else {
                        continue;
                    };
                    let t = (r.mob_age / r.mob_interval.max(0.001)).clamp(0.0, 1.0);
                    let mut lerps = std::collections::HashMap::new();
                    for mob in &mut mobs {
                        let render_pos = mob.pos.render_pos();
                        let (cur, phase) = match r.mob_lerp.get(&mob.id) {
                            Some(lerp) if mob.id != 0 => (lerp.at(t), lerp.phase),
                            _ => ((render_pos, mob.yaw), 0.0),
                        };
                        lerps.insert(
                            mob.id,
                            Lerp {
                                from: cur.0,
                                to: render_pos,
                                from_yaw: cur.1,
                                to_yaw: mob.yaw,
                                phase,
                            },
                        );
                        mob.present_replica_at(cur.1, phase);
                    }
                    self.server.world.replace_mobs(mobs);
                    r.mob_lerp = lerps; // dead mobs' spans fall away
                    r.mob_interval = r.mob_age.clamp(0.03, 0.3);
                    r.mob_age = 0.0;
                }
                net::S2C::ViewDistance { chunks } => {
                    r.granted_view_dist = chunks.max(1) as i32;
                }
                net::S2C::Falling(part) => {
                    if let Some(falling) = r.session.falling(part) {
                        self.server.world.replace_falling_blocks(falling);
                    }
                }
                net::S2C::Bolts(part) => {
                    if let Some(projectiles) = r.session.bolts(part) {
                        self.server.world.replace_projectiles(projectiles);
                    }
                }
                net::S2C::LooseItems(part) => {
                    if let Some(items) = r.session.loose_items(part) {
                        self.server.world.replace_loose_items(items);
                    }
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
                    self.present_implement_activation(
                        pos,
                        cue,
                        visual,
                        Some(r.session.content().items()),
                    );
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
                net::S2C::Hit { dmg, from } => self.hurt_player_from_wild(dmg, from, None),
                net::S2C::MobHit { id, dmg, crit } => {
                    // The host's authoritative damage for the guest's swing;
                    // float the number over the mob the snapshot still shows.
                    let at = self.server.world.mob_by_id(id).map(|m| m.pos);
                    if let Some(at) = at {
                        self.spawn_damage_number(at, dmg, crit);
                    }
                }
                net::S2C::Give {
                    item,
                    count,
                    durability,
                    arcane_id,
                    current_units,
                } => {
                    if let Some(local) = r.session.content().item(item) {
                        let reg = self.content.reg.clone();
                        let mut stack = ItemStack::new(&reg, local, count.max(1));
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
                    self.apply_remote_player_state(r.session.content(), state, false);
                }
                net::S2C::SettlementDelivery {
                    settlement,
                    item,
                    units,
                    rep_per_unit,
                } => {
                    // Capability E13: the host's depot accepted the goods;
                    // standing pays locally, exactly like quest rewards.
                    let rep = units * rep_per_unit;
                    let ns = self.player_namespace();
                    let rep_key = self
                        .content
                        .reg
                        .settlements
                        .iter()
                        .find(|s| s.id == settlement)
                        .map(|s| s.rep_key.clone())
                        .unwrap_or_else(|| format!("rep_{settlement}"));
                    self.content
                        .scripts
                        .kv
                        .borrow_mut()
                        .entry(ns)
                        .or_default()
                        .entry(rep_key)
                        .and_modify(|current: &mut String| {
                            *current = (current.parse::<u32>().unwrap_or(0) + rep).to_string();
                        })
                        .or_insert_with(|| rep.to_string());
                    self.toast(format!(
                        "{settlement} appreciates the {item} (+{rep} standing)."
                    ));
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
                    let conv = |s: &Option<net::StackSnap>| -> Option<ItemStack> {
                        s.as_ref()
                            .and_then(|stack| r.session.content().stack(stack))
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
                            6 => Screen::Stall(pos),
                            _ => Screen::Offering(pos),
                        });
                    }
                }
                net::S2C::MachineContainer {
                    pos,
                    machine,
                    slots,
                    aux,
                } => {
                    // Capability E7: the machine id remaps like the item
                    // palette; the handler's layout rebuilds the instance.
                    let reg = self.content.reg.clone();
                    let conv = |s: &Option<net::StackSnap>| -> Option<ItemStack> {
                        s.as_ref()
                            .and_then(|stack| r.session.content().stack(stack))
                    };
                    let Some(kind) = reg.machine_kind(&machine) else {
                        return;
                    };
                    let Some(def) = reg.machine(kind) else {
                        return;
                    };
                    let handler = def.handler;
                    let mut m = world::MachineInstance {
                        kind,
                        lit: aux.first().copied().unwrap_or(0.0) > 0.5,
                        progress: aux.get(1).copied().unwrap_or(0.0) * def.fire_secs,
                        ..Default::default()
                    };
                    match handler {
                        crate::machines::MachineHandler::Kiln => {
                            for (i, sl) in slots.iter().enumerate().take(9) {
                                let st = conv(sl);
                                match i {
                                    0..=3 => m.charge[i] = st,
                                    4 => m.reagent = st,
                                    _ => m.fuel[i - 5] = st,
                                }
                            }
                        }
                        crate::machines::MachineHandler::Workbench => {}
                        _ => {
                            for (i, s) in slots.iter().enumerate().take(8) {
                                if i < 4 {
                                    m.charge[i] = conv(s);
                                } else {
                                    m.fuel[i - 4] = conv(s);
                                }
                            }
                        }
                    }
                    self.server
                        .world
                        .insert_block_entity_at(pos, world::BlockEntity::Multiblock(m));
                    if matches!(self.ui_state.screen, Screen::Playing) {
                        self.set_screen(match handler {
                            crate::machines::MachineHandler::Kiln => Screen::Kiln(pos),
                            crate::machines::MachineHandler::Workbench => Screen::Workbench(pos),
                            _ => Screen::Bloomery(pos),
                        });
                    }
                }
                net::S2C::HeldResult(held) => {
                    // The authoritative cursor after our click replaces
                    // the local prediction (identical on agreement).
                    self.ui_state.held_stack = held
                        .as_ref()
                        .and_then(|stack| r.session.content().stack(stack));
                }
                net::S2C::Sleep { sleeping, present } => {
                    self.toast(format!("{sleeping}/{present} sleeping..."));
                }
                net::S2C::Toast(msg) => self.toast(msg),
                net::S2C::Chat { from, msg } => self.toast(format!("{from}: {msg}")),
                net::S2C::Joined { presence } => {
                    if presence.id != r.my_id {
                        self.toast(format!("{} joined.", presence.display_name));
                    }
                    r.session.joined(presence);
                }
                net::S2C::Left { id } => {
                    r.players.remove(&id);
                    r.player_positions.remove(&id);
                    r.player_lerp.remove(&id);
                    r.player_held.remove(&id);
                    r.player_implement.remove(&id);
                    r.player_style.remove(&id);
                    if let Some(presence) = r.session.left(id) {
                        self.toast(format!("{} left.", presence_label(&presence)));
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
        for position in r
            .session
            .apply_terrain(&mut self.server.world, REMOTE_CHUNKS_PER_FRAME)
        {
            r.wants.remove(&position);
        }
        if let Some(center) = r.session.admission().frame_needed()
            && [(-1, 0), (1, 0), (0, -1), (0, 1)]
                .iter()
                .all(|(du, dv)| self.server.world.has_chunk(center.offset(*du, *dv)))
        {
            let mesh = mesher::mesh_chunk(&self.server.world, center, &self.content.tile_variants);
            self.renderer.upload_chunk(center, &mesh);
            self.presentation.lights.chunk_meshed(center, mesh.emitters);
            self.server.world.mark_chunk_meshed(center);
            if let Err(error) = r.session.frame_ready() {
                self.multiplayer.join_status = format!("FAILED: {error}").to_uppercase();
                return;
            }
        }
        if r.session.take_ready() {
            r.session.send(&net::C2S::EntryReady);
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
                r.session.send_datagram(&net::C2S::Move {
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
                r.session
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

pub(super) fn presence_label(presence: &net::PlayerPresence) -> String {
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
