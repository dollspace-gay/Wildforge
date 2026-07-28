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
    }

    /// Load (or create) a world and enter it.
    pub(super) fn start_world(&mut self, name: &str) {
        let mut world =
            World::load_or_create(PathBuf::from("saves").join(name), self.content.reg.clone());
        // Dev: WILDFORGE_SPAWN="x,z" overrides the spawn search.
        let (sx, sz) = std::env::var("WILDFORGE_SPAWN")
            .ok()
            .and_then(|s| {
                let (a, b) = s.split_once(',')?;
                Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
            })
            .unwrap_or_else(|| find_spawn(&world));
        let spawn_chunk = ChunkPos::of_world(sx, sz);
        for dx in -1..=1 {
            for dz in -1..=1 {
                world.ensure_chunk(ChunkPos {
                    x: spawn_chunk.x + dx,
                    z: spawn_chunk.z + dz,
                });
            }
        }
        // 3D terrain can put the "highest solid" on an overhang lip or a
        // spike; refine to a locally flat, dry column so spawning is safe.
        let (sx, sz) = {
            let mut best = (sx, sz);
            let mut best_score = i32::MAX;
            for dx in -12..=12 {
                for dz in -12..=12 {
                    let (x, z) = (sx + dx, sz + dz);
                    let h = world.surface_height(x, z);
                    if h <= SEA_LEVEL + 1 {
                        continue;
                    }
                    let mut slope = 0;
                    for (nx, nz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                        slope = slope.max((h - world.surface_height(x + nx, z + nz)).abs());
                    }
                    let score = slope * 100 + dx.abs() + dz.abs();
                    if slope <= 1 {
                        best = (x, z);
                        best_score = 0;
                        break;
                    }
                    if score < best_score {
                        best_score = score;
                        best = (x, z);
                    }
                }
                if best_score == 0 {
                    break;
                }
            }
            best
        };
        // Whatever the refinement picked, guarantee it: dry, solid
        // underfoot, above the tideline — and if this is open ocean,
        // land gets raised rather than the player getting dropped in
        // it. (The old path fell back to the unrefined column, which
        // is how spawns ended up on the seabed.)
        let spawn = world.safe_spawn(sx, sz);

        self.renderer.clear_chunks();
        // Background generators for this world's seed (heavy terrain
        // math off the main thread; guests never generate).
        self.gen_pool = Some(crate::game::streaming::GenPool::new(
            world.seed,
            self.content.reg.clone(),
        ));
        self.server = server::Server::new(world, 0.3, self.rng ^ 0x5ee1);
        self.player = Player::new(spawn);
        self.survival.spawn_point = spawn;
        self.camera.pos = spawn + Vec3::new(0.0, EYE_HEIGHT, 0.0);
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
                    self.inventory.add(&reg, item, 1);
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
                    self.inventory.add(&reg, item, n);
                }
            }
            // Auto-equip a starter set so armor pips show in shots.
            for name in ["base:leather_helmet", "base:bronze_chestplate"] {
                if let Some(item) = reg.item_id(name)
                    && let Some((slot, _)) = reg.item(item).armor
                {
                    self.survival.armor[slot as usize] = Some(ItemStack::new(&reg, item, 1));
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
            // Ensure the chunk under the restored position exists.
            let cp = ChunkPos::of_world(self.player.pos.x as i32, self.player.pos.z as i32);
            for dx in -1..=1 {
                for dz in -1..=1 {
                    self.server.world.ensure_chunk(ChunkPos {
                        x: cp.x + dx,
                        z: cp.z + dz,
                    });
                }
            }
            // A save from below the world floor (a void casualty) comes
            // back standing on whatever ground its column still has.
            if self.player.pos.y < 1.0 {
                let (px, pz) = (
                    self.player.pos.x.floor() as i32,
                    self.player.pos.z.floor() as i32,
                );
                let h = self.server.world.surface_height(px, pz);
                self.player.pos.y = h as f32 + 1.05;
                self.player.vel = Vec3::ZERO;
            }
            // And a save whose terrain changed underneath it (built
            // over, regenerated) comes back beside the hill, not in
            // it. Mid-air/mid-swim saves pass through untouched.
            let freed = self.server.world.free_position(self.player.pos);
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
        if self.content.scripts.wants("on_world_start") {
            self.content.scripts.dispatch(
                &self.server.world,
                "on_world_start",
                (name.to_string(),),
            );
            self.apply_script_cmds();
        }
        self.set_screen(Screen::Playing);
        // Everything the capture harness stages lives in demos.rs: forty
        // scenes and sixty-odd environment variables, none of which is
        // part of starting a world.
        self.apply_dev_overrides(spawn);
    }

    /// Create a fresh world folder with a random seed and enter it.
    pub(super) fn new_world_mode(&mut self, mode: &str) {
        let name = next_world_name(std::path::Path::new("saves"), &self.worlds);
        let seed = (self.rand01() * u32::MAX as f32) as u32;
        world::write_world_meta(&PathBuf::from("saves").join(&name), seed, mode, 0.0);
        self.refresh_worlds();
        self.start_world(&name);
    }

    pub(super) fn save_player(&self) {
        if !self.in_world || self.multiplayer.remote.is_some() {
            return;
        }
        use std::fmt::Write as _;
        let mut out = String::new();
        let p = self.player.pos;
        let _ = writeln!(out, "pos = [{}, {}, {}]", p.x, p.y, p.z);
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
        let _ = writeln!(out, "spawn = [{}, {}, {}]", sp.x, sp.y, sp.z);
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
        match identity::local_profile_path(&world, self.identity.device_id())
            .and_then(|path| identity::atomic_write(&path, out.as_bytes(), false))
        {
            Ok(()) => identity::finish_local_profile_migration(&world),
            Err(error) => eprintln!("identity: player profile save failed: {error}"),
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
            pos: [f32; 3],
            yaw: f32,
            pitch: f32,
            health: f32,
            hunger: f32,
            nutrition: [f32; 5],
            hotbar: usize,
            #[serde(default)]
            spawn: Option<[f32; 3]>,
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
        self.player.pos = Vec3::new(p.pos[0], p.pos[1], p.pos[2]);
        self.camera.yaw = p.yaw;
        self.camera.pitch = p.pitch;
        self.survival.health = p.health;
        self.survival.hunger = p.hunger;
        self.survival.nutrition = p.nutrition;
        self.input.hotbar_sel = p.hotbar.min(HOTBAR_SLOTS - 1);
        if let Some(sp) = p.spawn {
            self.survival.spawn_point = Vec3::new(sp[0], sp[1], sp[2]);
        }
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
        self.multiplayer.host = None; // closes connections
        self.multiplayer.host_sleeping = false;
        self.server.world.set_edit_logging(false);
        if self.multiplayer.remote.is_some() {
            self.multiplayer.remote = None;
            self.renderer.clear_chunks();
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
        if self.in_world {
            self.save_player();
            self.server.world.settle_falling();
            self.server.world.save_modified();
            self.content
                .scripts
                .save_kv(&self.server.world.save_dir_for_saving());
        }
        self.renderer.clear_chunks();
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
