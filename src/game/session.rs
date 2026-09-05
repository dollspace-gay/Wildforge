//! World/session lifecycle, player persistence, and client configuration.

use crate::entity::ItemEntity;
use crate::identity;
use crate::inventory::HOTBAR_SLOTS;
use crate::inventory::Inventory;
use crate::inventory::ItemStack;
use crate::inventory::TOTAL_SLOTS;
use crate::physics::Player;
use crate::server;
use crate::world;
use crate::world::World;
use glam::Vec3;
use std::path::PathBuf;
use super::Game;
use super::MAX_AIR;
use super::combat;
use super::navigation::Screen;

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

    pub(super) fn finish_world_entry(
        &mut self,
        name: &str,
        world: World,
        spawn: crate::planet::EntityPos,
        material_policy_notices: Vec<String>,
    ) {
        // Background generators for this world's seed (heavy terrain
        // math off the main thread; guests never generate).
        let jobs = match crate::terrain_jobs::TerrainJobs::new(
            crate::terrain_jobs::TerrainContext::new(
                world.seed,
                world.planet_atlas(),
                world.chunk_loader(),
            ),
            crate::terrain_jobs::WorkerPolicy::Interactive,
        ) {
            Ok(jobs) => jobs,
            Err(error) => {
                eprintln!("world: terrain workers could not start: {error}");
                self.set_screen(Screen::Title);
                self.toast(format!("Could not enter world: {error}"));
                return;
            }
        };
        let meshes = match crate::game::mesh_jobs::MeshPool::new() {
            Ok(meshes) => meshes,
            Err(error) => {
                eprintln!("world: mesh workers could not start: {error}");
                self.set_screen(Screen::Title);
                self.toast(format!("Could not enter world: {error}"));
                return;
            }
        };
        self.renderer.clear_chunks();
        self.gen_pool = Some(jobs);
        self.mesh_pool = Some(meshes);
        self.server = server::Server::new(world, 0.3, self.rng ^ 0x5ee1);
        self.player = Player::new_at(spawn);
        self.survival.spawn_point = self.player.pos;
        self.combat = combat::CombatState::new();
        self.combat.stamina = self.stamina_max();
        self.camera.follow_planet(self.player.eye());
        self.camera.yaw = -std::f32::consts::FRAC_PI_2;
        self.camera.pitch = 0.0;
        // A world carries its chosen view (`camera` line in world.toml).
        // Guests keep the default first-person view: Tab is the roster there,
        // so a guest could never toggle back out of a host's orbit setting.
        if self.multiplayer.remote.is_none() {
            self.camera.mode = crate::camera::CameraMode::parse(&self.server.world.camera);
        }
        self.inventory = Inventory::new();
        self.survival.armor = [None; 5];
        self.interaction.bow_draw = 0.0;
        if let Ok(extra) = std::env::var("WILDFORGE_GIVE") {
            let reg = self.content.reg.clone();
            // Named items land first (hotbar slots), then the kit.
            for name in extra.split(',').filter(|s| s.contains(':')) {
                if let Some(item) = reg.item_id(name.trim()) {
                    let mut stack = ItemStack::new(&reg, item, 1);
                    if let Some(at) = self.player.pos.block()
                        && let Err(error) = self.server.world.bind_arcane_stack_at(
                            at,
                            &mut stack,
                            "development kit",
                        )
                    {
                        eprintln!("arcane: development kit item rejected: {error}");
                        continue;
                    }
                    let left = self.inventory.add_stack(&reg, stack);
                    if left == 0
                        && let Err(error) = self
                            .server
                            .world
                            .record_external_stack(stack, "development kit")
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
        self.server.world.clear_loose_items();
        self.interaction.breaking = None;
        self.survival.health = self.max_health();
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
        let _ = writeln!(
            out,
            "level = {}\nxp = {}\nskill_points = {}\nallocated = {:?}\nrespecs = {}",
            self.skills.level,
            self.skills.xp,
            self.skills.points,
            self.skills.allocated,
            self.skills.respecs
        );
        for (source, count) in &self.skills.source_counts {
            let _ = writeln!(out, "[[skill_xp]]\nsource = \"{source}\"\ncount = {count}");
        }
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
                    "[[slot]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}\narcane_id = {}",
                    self.content.reg.item(s.item).name,
                    s.count,
                    s.durability,
                    s.arcane_id
                );
            }
        }
        for (i, s) in self.survival.armor.iter().enumerate() {
            if let Some(s) = s {
                let _ = writeln!(
                    out,
                    "[[armor]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}\narcane_id = {}",
                    self.content.reg.item(s.item).name,
                    s.count,
                    s.durability,
                    s.arcane_id
                );
            }
        }
        for (i, loadout) in self.survival.loadouts.iter().enumerate() {
            if loadout.is_empty() {
                continue;
            }
            let _ = writeln!(out, "[[loadout]]\nindex = {i}");
            for component in &loadout.components {
                let _ = writeln!(
                    out,
                    "[[loadout.component]]\nitem = \"{}\"\ncount = {}\ndurability = {}\narcane_id = {}",
                    self.content.reg.item(component.stack.item).name,
                    component.stack.count,
                    component.stack.durability,
                    component.stack.arcane_id
                );
            }
        }
        for preset in &self.survival.loadout_presets {
            if preset.slots.iter().all(Option::is_none) {
                continue;
            }
            let _ = writeln!(out, "[[loadout_preset]]\nname = \"{}\"", preset.name);
            for (i, slot) in preset.slots.iter().enumerate() {
                let Some(slot) = slot else { continue };
                let _ = writeln!(
                    out,
                    "[[loadout_preset.slot]]\nindex = {i}\nframe = \"{}\"",
                    slot.frame
                );
                for component in &slot.components {
                    let _ = writeln!(
                        out,
                        "[[loadout_preset.slot.component]]\nitem = \"{component}\""
                    );
                }
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
            stable_id: u64,
            pos: crate::planet::EntityPos,
            vel: [f32; 3],
            item: String,
            count: u32,
            age: f32,
            durability: u32,
            arcane_id: u64,
        }
        #[derive(Serialize)]
        struct File {
            version: u32,
            drop: Vec<StoredDrop>,
        }
        let drop = self
            .server
            .world
            .loose_items()
            .iter()
            .map(|entity| StoredDrop {
                stable_id: entity.stable_id,
                pos: entity.pos,
                vel: entity.vel.to_array(),
                item: self.content.reg.item(entity.item).name.clone(),
                count: entity.count,
                age: entity.age,
                durability: entity.durability,
                arcane_id: entity.arcane_id,
            })
            .collect();
        let text =
            toml::to_string_pretty(&File { version: 3, drop }).map_err(std::io::Error::other)?;
        crate::identity::atomic_write(&world.join("loose-items.toml"), text.as_bytes(), false)
    }

    fn load_loose_items(&mut self, world: &std::path::Path) {
        use serde::Deserialize;

        // World::load_or_create owns the v3 host-authoritative format. This
        // reader remains only as a migration fallback for older session
        // worlds that reached the game before world-side adoption.
        if !self.server.world.loose_items().is_empty() {
            return;
        }

        #[derive(Deserialize)]
        struct StoredDrop {
            #[serde(default)]
            stable_id: u64,
            pos: crate::planet::EntityPos,
            vel: [f32; 3],
            item: String,
            count: u32,
            age: f32,
            durability: u32,
            #[serde(default)]
            arcane_id: u64,
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
        if !(1..=3).contains(&file.version) {
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
            entity.stable_id = stored.stable_id;
            entity.age = stored.age.max(0.0);
            entity.durability = stored
                .durability
                .min(self.content.reg.item(item).durability);
            entity.arcane_id = stored.arcane_id;
            if let Some(at) = stored.pos.block()
                && self.content.reg.item(item).charm_def.is_some()
            {
                let mut stack = ItemStack {
                    item,
                    count: stored.count,
                    durability: entity.durability,
                    arcane_id: stored.arcane_id,
                };
                if self
                    .server
                    .world
                    .ensure_charm_instance_at(
                        at,
                        &mut stack,
                        "explicit planetary loose-item charm migration",
                    )
                    .is_ok()
                {
                    entity.arcane_id = stack.arcane_id;
                }
            }
            self.server.world.spawn_loose_item(entity);
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
            #[serde(default)]
            arcane_id: u64,
        }
        #[derive(Deserialize)]
        struct SkillXpCount {
            source: String,
            count: u32,
        }
        #[derive(Deserialize)]
        struct LoadoutT {
            index: usize,
            #[serde(default)]
            component: Vec<SlotT>,
        }
        #[derive(Deserialize)]
        struct LoadoutPresetSlotT {
            index: usize,
            frame: String,
            #[serde(default)]
            component: Vec<String>,
        }
        #[derive(Deserialize)]
        struct LoadoutPresetT {
            #[serde(default)]
            name: String,
            #[serde(default)]
            slot: Vec<LoadoutPresetSlotT>,
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
            #[serde(default)]
            level: u32,
            #[serde(default)]
            xp: f64,
            #[serde(default)]
            skill_points: u32,
            #[serde(default)]
            allocated: Vec<String>,
            #[serde(default)]
            respecs: u32,
            #[serde(default)]
            skill_xp: Vec<SkillXpCount>,
            #[serde(default)]
            loadout: Vec<LoadoutT>,
            #[serde(default)]
            loadout_preset: Vec<LoadoutPresetT>,
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
        if p.level > 0 {
            self.skills.level = p.level;
        }
        self.skills.xp = p.xp;
        self.skills.points = p.skill_points;
        self.skills.allocated = p.allocated;
        self.skills.respecs = p.respecs;
        for entry in p.skill_xp {
            self.skills.source_counts.insert(entry.source, entry.count);
        }
        for s in p.slot {
            if s.index < TOTAL_SLOTS
                && let Some(item) = self.content.reg.item_id(&s.item)
            {
                self.inventory.slots[s.index] = Some(ItemStack {
                    item,
                    count: s.count,
                    durability: s.durability,
                    arcane_id: s.arcane_id,
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
                    arcane_id: s.arcane_id,
                });
            }
        }
        for entry in p.loadout {
            if entry.index >= 5 {
                continue;
            }
            let mut loadout = crate::equipment::Loadout::default();
            for component in entry.component {
                let Some(item) = self.content.reg.item_id(&component.item) else {
                    continue;
                };
                loadout.components.push(crate::equipment::SlottedComponent {
                    slot_type: self
                        .content
                        .reg
                        .item(item)
                        .component
                        .clone()
                        .unwrap_or_default(),
                    stack: ItemStack {
                        item,
                        count: component.count,
                        durability: component.durability,
                        arcane_id: component.arcane_id,
                    },
                });
            }
            self.survival.loadouts[entry.index] = loadout;
        }
        for entry in p.loadout_preset {
            let mut slots: [Option<crate::equipment::PresetSlot>; 4] = Default::default();
            for slot in entry.slot {
                if slot.index >= 4 {
                    continue;
                }
                slots[slot.index] = Some(crate::equipment::PresetSlot {
                    frame: slot.frame,
                    components: slot.component,
                });
            }
            self.survival
                .loadout_presets
                .push(crate::equipment::LoadoutPreset {
                    name: entry.name,
                    slots,
                });
        }
        if let Some(at) = self.player.pos.block() {
            let migrated = self.server.world.migrate_legacy_player_charms(
                at,
                &mut self.inventory,
                &mut self.survival.armor,
                &mut self.ui_state.held_stack,
                "local player",
            );
            if migrated != 0
                && let Err(error) = self.save_player()
            {
                eprintln!(
                    "implements: migrated {migrated} local charms but profile save failed: {error}"
                );
            }
        }
        if let Ok(player_id) = identity::local_player_id(dir, self.identity.device_id()) {
            match self
                .server
                .world
                .resume_pending_inventory_workings(player_id.0, &mut self.inventory)
            {
                Ok(ids) if !ids.is_empty() => match self.save_player() {
                    Ok(()) => {
                        for id in ids {
                            if let Err(error) = self.server.world.finish_inventory_working(id) {
                                eprintln!("workings: resumed Fieldmend could not finish: {error}");
                            }
                        }
                    }
                    Err(error) => eprintln!(
                        "workings: resumed Fieldmend remains pending because its profile checkpoint failed: {error}"
                    ),
                },
                Ok(_) => {}
                Err(error) => {
                    eprintln!("workings: pending local Fieldmend is inconsistent: {error}")
                }
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
            self.server.world.clear_loose_items();
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
        self.server.world.clear_loose_items();
        self.in_world = false;
        self.refresh_worlds();
        self.set_screen(Screen::Title);
    }
}
