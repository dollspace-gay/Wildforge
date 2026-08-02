//! World/session lifecycle, player persistence, and client configuration.

use super::*;

impl Game {
    pub(super) fn apply_config(&mut self) {
        self.camera.sens = self.config.sensitivity;
        self.camera.fovy = self.config.fov.to_radians();
        if let Some(a) = &mut self.audio {
            a.volume = self.config.volume;
        }
        self.config.save();
    }

    pub(super) fn refresh_worlds(&mut self) {
        self.worlds = world::list_worlds(std::path::Path::new("saves"));
        let inspection = world::inspect_worlds(std::path::Path::new("saves"));
        self.world_details = inspection
            .iter()
            .filter(|entry| entry.playable)
            .map(|entry| (entry.name.clone(), entry.status.clone()))
            .collect();
        self.world_problems = inspection
            .into_iter()
            .filter(|entry| !entry.playable)
            .map(|entry| (entry.name, entry.status))
            .collect();
    }

    /// Load/prepare a world off-thread. `Screen::CreatingWorld` remains
    /// responsive until both the common homeland and a saved player's local
    /// safety region are resident; only then does `finish_world_entry` create
    /// the player session.
    pub(super) fn start_world(&mut self, name: &str) {
        self.start_world_with_origin(name, false);
    }

    fn start_created_world(&mut self, name: &str) {
        self.start_world_with_origin(name, true);
    }

    fn start_world_with_origin(&mut self, name: &str, created_here: bool) {
        if self.ui_state.world_entry.is_some() {
            return;
        }
        let save_dir = PathBuf::from("saves").join(name);
        let reg = self.content.reg.clone();
        let profile_path = identity::local_profile_path(&save_dir, self.identity.device_id()).ok();
        // Dev: WILDFORGE_SPAWN="face,u,v" bypasses the persisted common
        // homeland so visual fixtures can still pin an exact atlas site.
        let override_wanted = std::env::var("WILDFORGE_SPAWN").ok().and_then(|s| {
            let mut fields = s.split(',').map(str::trim);
            let face = crate::planet::Face::from_name(fields.next()?)?;
            let u = fields.next()?.parse().ok()?;
            let v = fields.next()?.parse().ok()?;
            fields.next().is_none().then_some(())?;
            crate::planet::SurfacePos::new(face, u, v).ok()
        });
        let cancel = crate::planet_atlas::CancellationToken::default();
        let worker_cancel = cancel.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        let complete_name = name.to_owned();
        std::thread::spawn(move || {
            let result = (|| -> Result<_, String> {
                let mut world = World::load_or_create(save_dir, reg)
                    .map_err(|error| format!("could not open world: {error}"))?;
                let material_policy_notices = world
                    .material_ledger
                    .as_ref()
                    .map_or_else(Vec::new, |ledger| ledger.retrogen_notices());
                let progress_sender = sender.clone();
                let spawn = if let Some(wanted) = override_wanted {
                    let chunks = crate::world::player_entry_chunks(wanted);
                    for (index, position) in chunks.iter().copied().enumerate() {
                        world.ensure_chunk(position);
                        let _ = progress_sender.send(WorldEntryEvent::Progress {
                            stage: "LOADING DEVELOPMENT ENTRY".into(),
                            completed: index + 1,
                            total: chunks.len(),
                        });
                    }
                    world.safe_spawn_at(wanted)
                } else {
                    world
                        .prepare_common_spawn(|stage, completed, total| {
                            let _ = progress_sender.send(WorldEntryEvent::Progress {
                                stage: stage.to_uppercase(),
                                completed,
                                total,
                            });
                        })
                        .map_err(|error| format!("could not prepare a homeland: {error}"))?
                };
                if worker_cancel.is_cancelled() {
                    return Err("world entry cancelled".into());
                }
                if let Some(saved) = profile_path.as_deref().and_then(saved_profile_position) {
                    let chunks = crate::world::player_entry_chunks(saved.surface());
                    for (index, position) in chunks.iter().copied().enumerate() {
                        world.ensure_chunk(position);
                        let _ = progress_sender.send(WorldEntryEvent::Progress {
                            stage: "LOADING SAVED DOORSTEP".into(),
                            completed: index + 1,
                            total: chunks.len(),
                        });
                    }
                }
                if worker_cancel.is_cancelled() {
                    return Err("world entry cancelled".into());
                }
                Ok((world, spawn, material_policy_notices))
            })();
            let _ = sender.send(WorldEntryEvent::Complete {
                name: complete_name,
                result: Box::new(result),
            });
        });
        self.ui_state.world_entry = Some(WorldEntryTask {
            receiver,
            cancel,
            created_here,
        });
        self.ui_state.creation_status = "OPENING PLANET".into();
        self.ui_state.creation_progress = (0, 1);
        self.set_screen(Screen::CreatingWorld);
    }

    fn finish_world_entry(
        &mut self,
        name: &str,
        world: World,
        spawn: crate::planet::EntityPos,
        material_policy_notices: Vec<String>,
    ) {
        self.renderer.clear_chunks();
        // Background generators for this world's seed (heavy terrain
        // math off the main thread; guests never generate).
        self.gen_pool = Some(crate::game::streaming::GenPool::new(
            world.seed,
            self.content.reg.clone(),
            world.planet_atlas(),
            world.chunk_loader(),
        ));
        self.mesh_pool = Some(crate::game::streaming::MeshPool::new());
        self.server = server::Server::new(world, 0.3, self.rng ^ 0x5ee1);
        self.player = Player::new_at(spawn);
        self.survival.spawn_point = self.player.pos;
        self.camera.follow_planet(self.player.eye());
        self.camera.yaw = -std::f32::consts::FRAC_PI_2;
        self.camera.pitch = 0.0;
        self.inventory = Inventory::new();
        self.survival.armor = [None; 5];
        self.interaction.bow_draw = 0.0;
        if let Ok(extra) = std::env::var("WILDFORGE_GIVE") {
            let reg = self.content.reg.clone();
            // Named items land first (hotbar slots), then the kit.
            for name in extra.split(',').filter(|s| s.contains(':')) {
                if let Some(item) = reg.item_id(name.trim()) {
                    let left = self.inventory.add(&reg, item, 1);
                    if left == 0
                        && let Some(ledger) = &mut self.server.world.material_ledger
                        && let Err(error) = ledger.record_external_stack(
                            &reg,
                            ItemStack::new(&reg, item, 1),
                            "development kit",
                        )
                    {
                        eprintln!("materials: development kit accounting failed: {error}");
                    }
                }
            }
            for (name, n) in [
                ("base:dirt", 64),
                ("base:cobblestone", 32),
                ("base:log", 8),
                ("base:planks", 12),
                ("base:stick", 8),
                ("base:wood_pickaxe", 1),
                ("base:potato", 5),
                ("base:bread", 3),
                ("base:bronze_sword", 1),
                ("base:hunting_bow", 1),
                ("base:arrow", 16),
                ("base:leather_chestplate", 1),
            ] {
                if let Some(item) = reg.item_id(name) {
                    let left = self.inventory.add(&reg, item, n);
                    let added = n - left;
                    if added != 0
                        && let Some(ledger) = &mut self.server.world.material_ledger
                        && let Err(error) = ledger.record_external_stack(
                            &reg,
                            ItemStack::new(&reg, item, added),
                            "development kit",
                        )
                    {
                        eprintln!("materials: development kit accounting failed: {error}");
                    }
                }
            }
            // Auto-equip a starter set so armor pips show in shots.
            for name in ["base:leather_helmet", "base:bronze_chestplate"] {
                if let Some(item) = reg.item_id(name)
                    && let Some((slot, _)) = reg.item(item).armor
                {
                    self.survival.armor[slot as usize] = Some(ItemStack::new(&reg, item, 1));
                    if let Some(ledger) = &mut self.server.world.material_ledger
                        && let Err(error) = ledger.record_external_stack(
                            &reg,
                            ItemStack::new(&reg, item, 1),
                            "development kit auto-equip",
                        )
                    {
                        eprintln!("materials: development armor accounting failed: {error}");
                    }
                }
            }
        }
        self.ui_state.held_stack = None;
        self.interaction.craft_grid = [None; 9];
        self.interaction.items.clear();
        self.interaction.breaking = None;
        self.survival.health = MAX_HEALTH;
        self.survival.killed_by_wild = false;
        self.survival.hunger = 20.0;
        self.survival.nutrition = [0.0; 5];
        self.survival.eating = 0.0;
        self.survival.exhaustion_regen = 0.0;
        self.survival.starve_timer = 0.0;
        self.survival.drown_timer = 0.0;
        self.survival.air = MAX_AIR;
        self.survival.since_damage = 100.0;
        self.survival.damage_flash = 0.0;
        self.survival.fall_start = None;
        self.server.time_of_day = 0.3;
        self.input.hotbar_sel = 0;
        let (_, mode, _) = world::read_world_meta(&PathBuf::from("saves").join(name));
        self.creative = mode == "creative";
        self.flying = false;
        self.in_world = true;
        if self.load_player(&PathBuf::from("saves").join(name)) {
            // The entry worker loaded this exact 3x3 before handing the
            // World to the session. Repairs below therefore inspect resident
            // terrain and cannot turn first-frame setup into cold generation.
            // A malformed/development profile below the sealed shell is
            // settled onto valid ground; there is no planetary void mechanic.
            if self.player.pos.y < 1.0 {
                self.player.pos = self.server.world.settle_spawn_at(self.player.pos);
                self.player.vel = Vec3::ZERO;
            }
            // And a save whose terrain changed underneath it (built
            // over, regenerated) comes back beside the hill, not in
            // it. Mid-air/mid-swim saves pass through untouched.
            let freed = self.server.world.free_position_at(self.player.pos);
            if freed != self.player.pos {
                self.player.pos = freed;
                self.player.vel = Vec3::ZERO;
            }
        }
        // Dev: pick the hotbar slot screenshots hold up (after the
        // profile load so it isn't overwritten).
        if let Ok(s) = std::env::var("WILDFORGE_SEL")
            && let Ok(i) = s.parse::<usize>()
        {
            self.input.hotbar_sel = i.min(HOTBAR_SLOTS - 1);
        }
        self.server.sync_tier();
        self.content
            .scripts
            .load_kv(&PathBuf::from("saves").join(name));
        self.load_loose_items(&PathBuf::from("saves").join(name));
        if self.content.scripts.wants("on_world_start") {
            self.content.scripts.dispatch(
                &self.server.world,
                "on_world_start",
                (name.to_string(),),
            );
            self.apply_script_cmds();
        }
        self.set_screen(Screen::Playing);
        for notice in material_policy_notices {
            self.toast(notice);
        }
        // Everything the capture harness stages lives in demos.rs: forty
        // scenes and sixty-odd environment variables, none of which is
        // part of starting a world.
        self.apply_dev_overrides(spawn);
    }

    pub(super) fn open_new_world(&mut self, mode: &str) {
        self.ui_state.new_world_mode = mode.to_string();
        self.roll_new_world_seed();
        self.ui_state.new_world_status.clear();
        self.set_screen(Screen::NewWorld);
    }

    pub(super) fn roll_new_world_seed(&mut self) {
        let seed = (self.rand01() * u32::MAX as f32) as u32;
        self.ui_state.new_world_seed = seed.to_string();
    }

    /// Create a fresh world folder with the player-visible seed and enter it.
    pub(super) fn create_new_world(&mut self) {
        if self.ui_state.world_creation.is_some() {
            return;
        }
        let Ok(seed) = self.ui_state.new_world_seed.parse::<u32>() else {
            self.ui_state.new_world_status = "SEED MUST BE AN INTEGER FROM 0 TO 4294967295".into();
            return;
        };
        self.ui_state.new_world_status.clear();
        let mode = self.ui_state.new_world_mode.clone();
        let name = next_world_name(std::path::Path::new("saves"), &self.worlds);
        let destination = PathBuf::from("saves").join(&name);
        let content_hash = crate::planet_atlas::genesis_content_hash(std::path::Path::new("mods"));
        let cancel = crate::planet_atlas::CancellationToken::default();
        let worker_cancel = cancel.clone();
        let worker_reg = self.content.reg.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        let complete_name = name.clone();
        std::thread::spawn(move || {
            let progress_sender = sender.clone();
            let result = world::create_world_atomic(
                &destination,
                seed,
                &mode,
                content_hash,
                worker_reg,
                &worker_cancel,
                move |progress| {
                    let _ = progress_sender.send(WorldCreationEvent::Progress(progress));
                },
            )
            .map_err(|error| error.to_string());
            let _ = sender.send(WorldCreationEvent::Complete {
                name: complete_name,
                result,
            });
        });
        self.ui_state.world_creation = Some(WorldCreationTask { receiver, cancel });
        self.ui_state.creation_status = "SHAPING PLANET".into();
        self.ui_state.creation_progress = (0, crate::planet_atlas::AtlasStage::ALL.len());
        self.set_screen(Screen::CreatingWorld);
    }

    pub(super) fn cancel_world_creation(&mut self) {
        if let Some(task) = &self.ui_state.world_creation {
            task.cancel.cancel();
            self.ui_state.creation_status = "CANCELLING PLANET CREATION".into();
        } else if let Some(task) = &self.ui_state.world_entry {
            task.cancel.cancel();
            self.ui_state.creation_status = "CANCELLING WORLD ENTRY".into();
        }
    }

    pub(super) fn poll_world_creation(&mut self) {
        let events: Vec<_> = self
            .ui_state
            .world_creation
            .as_ref()
            .map(|task| task.receiver.try_iter().collect())
            .unwrap_or_default();
        for event in events {
            match event {
                WorldCreationEvent::Progress(progress) => match progress {
                    world::WorldCreationProgress::Atlas(progress) => {
                        self.ui_state.creation_status = progress.stage.label().into();
                        self.ui_state.creation_progress =
                            (progress.completed_stages, progress.total_stages);
                    }
                    world::WorldCreationProgress::Homeland {
                        stage,
                        completed,
                        total,
                    } => {
                        self.ui_state.creation_status = stage.to_uppercase();
                        self.ui_state.creation_progress = (completed, total);
                    }
                },
                WorldCreationEvent::Complete { name, result } => {
                    self.ui_state.world_creation = None;
                    match result {
                        Ok(()) => {
                            self.refresh_worlds();
                            self.start_created_world(&name);
                        }
                        Err(error) => {
                            if error.contains("cancel") {
                                self.set_screen(Screen::NewWorld);
                                self.toast("Planet creation cancelled".into());
                            } else {
                                eprintln!("world: creation of {name} failed: {error}");
                                self.ui_state.new_world_status =
                                    format!("CREATION FAILED: {error}");
                                self.set_screen(Screen::NewWorld);
                                self.toast(format!("Could not create world: {error}"));
                            }
                        }
                    }
                }
            }
        }

        let entry_events: Vec<_> = self
            .ui_state
            .world_entry
            .as_ref()
            .map(|task| task.receiver.try_iter().collect())
            .unwrap_or_default();
        for event in entry_events {
            match event {
                WorldEntryEvent::Progress {
                    stage,
                    completed,
                    total,
                } => {
                    self.ui_state.creation_status = stage;
                    self.ui_state.creation_progress = (completed, total);
                }
                WorldEntryEvent::Complete { name, result } => {
                    let created_here = self
                        .ui_state
                        .world_entry
                        .as_ref()
                        .is_some_and(|task| task.created_here);
                    self.ui_state.world_entry = None;
                    match *result {
                        Ok((world, spawn, notices)) => {
                            self.finish_world_entry(&name, world, spawn, notices);
                        }
                        Err(error) => {
                            if error.contains("cancel") {
                                self.set_screen(if created_here {
                                    Screen::NewWorld
                                } else {
                                    Screen::Title
                                });
                                self.toast("World entry cancelled".into());
                            } else {
                                eprintln!("world: entry into {name} failed: {error}");
                                if created_here {
                                    self.ui_state.new_world_status =
                                        format!("WORLD WAS CREATED, BUT ENTRY FAILED: {error}");
                                    self.set_screen(Screen::NewWorld);
                                } else {
                                    self.set_screen(Screen::Title);
                                }
                                self.toast(format!("Could not enter world: {error}"));
                            }
                        }
                    }
                }
            }
        }
    }

    pub(super) fn save_player(&self) -> std::io::Result<()> {
        if !self.in_world || self.multiplayer.remote.is_some() {
            return Ok(());
        }
        use std::fmt::Write as _;
        let mut out = String::new();
        let p = self.player.pos;
        let _ = writeln!(out, "version = 2");
        let _ = writeln!(
            out,
            "face = {}\nu = {}\ny = {}\nv = {}",
            p.face() as u8,
            p.u(),
            p.y(),
            p.v()
        );
        let _ = writeln!(
            out,
            "yaw = {}\npitch = {}",
            self.camera.yaw, self.camera.pitch
        );
        let _ = writeln!(
            out,
            "health = {}\nhunger = {}",
            self.survival.health, self.survival.hunger
        );
        let _ = writeln!(out, "nutrition = {:?}", self.survival.nutrition);
        let _ = writeln!(out, "hotbar = {}", self.input.hotbar_sel);
        let sp = self.survival.spawn_point;
        let _ = writeln!(
            out,
            "spawn_face = {}\nspawn_u = {}\nspawn_y = {}\nspawn_v = {}",
            sp.face() as u8,
            sp.u(),
            sp.y(),
            sp.v()
        );
        for (i, s) in self.inventory.slots.iter().enumerate() {
            if let Some(s) = s {
                let _ = writeln!(
                    out,
                    "[[slot]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}",
                    self.content.reg.item(s.item).name,
                    s.count,
                    s.durability
                );
            }
        }
        for (i, s) in self.survival.armor.iter().enumerate() {
            if let Some(s) = s {
                let _ = writeln!(
                    out,
                    "[[armor]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}",
                    self.content.reg.item(s.item).name,
                    s.count,
                    s.durability
                );
            }
        }
        let world = self.server.world.save_dir_for_saving();
        let path = identity::local_profile_path(&world, self.identity.device_id())?;
        identity::atomic_write(&path, out.as_bytes(), false)?;
        identity::finish_local_profile_migration(&world);
        Ok(())
    }

    /// Persist every local-session component and keep enough context for a
    /// player-facing error. A remote guest owns none of this state.
    pub(super) fn save_session(&mut self) -> Result<String, String> {
        if self.multiplayer.remote.is_some() {
            return Ok("remote session has no local world state".into());
        }
        let mut failures = Vec::new();
        if let Err(error) = self.save_player() {
            failures.push(format!("player profile: {error}"));
        }
        let world_dir = self.server.world.save_dir_for_saving();
        if let Err(error) = self.save_loose_items(&world_dir) {
            failures.push(format!("loose items: {error}"));
        }
        self.server.world.settle_falling();
        let world_report = self.server.world.save_modified();
        if !world_report.is_ok() {
            failures.push(format!("world: {}", world_report.summary()));
        }
        if let Err(error) = self.content.scripts.save_kv(&world_dir) {
            failures.push(format!(
                "mod storage ({}): {error}",
                world_dir.join("modstore.toml").display()
            ));
        }
        if failures.is_empty() {
            Ok(world_report.summary())
        } else {
            Err(failures.join("; "))
        }
    }

    fn save_loose_items(&self, world: &std::path::Path) -> std::io::Result<()> {
        use serde::Serialize;

        #[derive(Serialize)]
        struct StoredDrop {
            pos: crate::planet::EntityPos,
            vel: [f32; 3],
            item: String,
            count: u32,
            age: f32,
            durability: u32,
        }
        #[derive(Serialize)]
        struct File {
            version: u32,
            drop: Vec<StoredDrop>,
        }
        let drop = self
            .interaction
            .items
            .iter()
            .map(|entity| StoredDrop {
                pos: entity.pos,
                vel: entity.vel.to_array(),
                item: self.content.reg.item(entity.item).name.clone(),
                count: entity.count,
                age: entity.age,
                durability: entity.durability,
            })
            .collect();
        let text =
            toml::to_string_pretty(&File { version: 1, drop }).map_err(std::io::Error::other)?;
        crate::identity::atomic_write(&world.join("loose-items.toml"), text.as_bytes(), false)
    }

    fn load_loose_items(&mut self, world: &std::path::Path) {
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct StoredDrop {
            pos: crate::planet::EntityPos,
            vel: [f32; 3],
            item: String,
            count: u32,
            age: f32,
            durability: u32,
        }
        #[derive(Deserialize)]
        struct File {
            version: u32,
            #[serde(default)]
            drop: Vec<StoredDrop>,
        }
        let Ok(text) = std::fs::read_to_string(world.join("loose-items.toml")) else {
            return;
        };
        let Ok(file) = toml::from_str::<File>(&text) else {
            eprintln!("items: could not parse loose-items.toml; file left untouched");
            return;
        };
        if file.version != 1 {
            eprintln!(
                "items: unsupported loose item save version {}",
                file.version
            );
            return;
        }
        for stored in file.drop {
            let Some(item) = self.content.reg.item_id(&stored.item) else {
                eprintln!("items: retained unknown loose item name {}", stored.item);
                continue;
            };
            if stored.count == 0
                || !stored.age.is_finite()
                || stored.vel.iter().any(|value| !value.is_finite())
            {
                continue;
            }
            let mut entity =
                ItemEntity::new(stored.pos, Vec3::from_array(stored.vel), item, stored.count);
            entity.age = stored.age.max(0.0);
            entity.durability = stored
                .durability
                .min(self.content.reg.item(item).durability);
            self.interaction.items.push(entity);
        }
    }

    pub(super) fn load_player(&mut self, dir: &std::path::Path) -> bool {
        use serde::Deserialize;
        #[derive(Deserialize)]
        struct SlotT {
            index: usize,
            item: String,
            count: u32,
            durability: u32,
        }
        #[derive(Deserialize)]
        struct P {
            version: u32,
            face: u8,
            u: f32,
            y: f32,
            v: f32,
            yaw: f32,
            pitch: f32,
            health: f32,
            hunger: f32,
            nutrition: [f32; 5],
            hotbar: usize,
            spawn_face: u8,
            spawn_u: f32,
            spawn_y: f32,
            spawn_v: f32,
            #[serde(default, alias = "inventory")]
            slot: Vec<SlotT>,
            #[serde(default)]
            armor: Vec<SlotT>,
        }
        let path = match identity::local_profile_path(dir, self.identity.device_id()) {
            Ok(path) => path,
            Err(error) => {
                eprintln!("identity: player profile migration failed: {error}");
                return false;
            }
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return false;
        };
        let Ok(p) = toml::from_str::<P>(&text) else {
            return false;
        };
        if p.version != 2 {
            return false;
        }
        let Some(face) = crate::planet::Face::from_u8(p.face) else {
            return false;
        };
        let Ok(pos) = crate::planet::EntityPos::new(face, p.u, p.y, p.v) else {
            return false;
        };
        let Some(spawn_face) = crate::planet::Face::from_u8(p.spawn_face) else {
            return false;
        };
        let Ok(spawn) = crate::planet::EntityPos::new(spawn_face, p.spawn_u, p.spawn_y, p.spawn_v)
        else {
            return false;
        };
        self.player.pos = pos;
        self.camera.yaw = p.yaw;
        self.camera.pitch = p.pitch;
        self.survival.health = p.health;
        self.survival.hunger = p.hunger;
        self.survival.nutrition = p.nutrition;
        self.input.hotbar_sel = p.hotbar.min(HOTBAR_SLOTS - 1);
        self.survival.spawn_point = spawn;
        for s in p.slot {
            if s.index < TOTAL_SLOTS
                && let Some(item) = self.content.reg.item_id(&s.item)
            {
                self.inventory.slots[s.index] = Some(ItemStack {
                    item,
                    count: s.count,
                    durability: s.durability,
                });
            }
        }
        for s in p.armor {
            if s.index < 5
                && let Some(item) = self.content.reg.item_id(&s.item)
            {
                self.survival.armor[s.index] = Some(ItemStack {
                    item,
                    count: s.count,
                    durability: s.durability,
                });
            }
        }
        true
    }

    pub(super) fn quit_to_title(&mut self) {
        if self.multiplayer.remote.is_some() {
            self.multiplayer.remote = None;
            self.multiplayer.host = None;
            self.multiplayer.host_sleeping = false;
            self.server.world.set_edit_logging(false);
            self.renderer.clear_chunks();
            self.gen_pool = None;
            self.mesh_pool = None;
            self.server = server::Server::new(
                World::new(0, PathBuf::from("saves/.none"), self.content.reg.clone()),
                0.3,
                1,
            );
            self.interaction.items.clear();
            self.in_world = false;
            self.refresh_worlds();
            self.set_screen(Screen::Title);
            return;
        }
        if self.in_world
            && let Err(error) = self.save_session()
        {
            eprintln!("world: save and quit cancelled: {error}");
            self.toast(format!("Could not save; still in world: {error}"));
            return;
        }
        self.multiplayer.host = None; // closes connections after durable save
        self.multiplayer.host_sleeping = false;
        self.server.world.set_edit_logging(false);
        self.renderer.clear_chunks();
        self.gen_pool = None;
        self.mesh_pool = None;
        self.server = server::Server::new(
            World::new(0, PathBuf::from("saves/.none"), self.content.reg.clone()),
            0.3,
            1,
        );
        self.interaction.items.clear();
        self.in_world = false;
        self.refresh_worlds();
        self.set_screen(Screen::Title);
    }
}

fn saved_profile_position(path: &std::path::Path) -> Option<crate::planet::EntityPos> {
    #[derive(serde::Deserialize)]
    struct Position {
        version: u32,
        face: u8,
        u: f32,
        y: f32,
        v: f32,
    }
    let text = std::fs::read_to_string(path).ok()?;
    let saved: Position = toml::from_str(&text).ok()?;
    if saved.version != 2 {
        return None;
    }
    let face = crate::planet::Face::from_u8(saved.face)?;
    crate::planet::EntityPos::new(face, saved.u, saved.y, saved.v).ok()
}
