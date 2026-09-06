//! Entry graphical session adapter.

use crate::inventory::HOTBAR_SLOTS;
use crate::inventory::Inventory;
use crate::inventory::ItemStack;
use crate::physics::Player;
use crate::server;
use crate::world;
use crate::world::World;
use glam::Vec3;
use std::path::PathBuf;
use crate::game::Game;
use crate::game::MAX_AIR;
use crate::game::combat;
use crate::game::navigation::Screen;

impl Game {
    pub(in crate::game) fn finish_world_entry(
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
        self.runtime.set_local(server::Server::new(world, 0.3, self.rng ^ 0x5ee1));
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
            self.camera.mode = crate::camera::CameraMode::parse(&self.runtime.local().world.camera);
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
                        && let Err(error) = self.runtime.local_mut().world.bind_arcane_stack_at(
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
                        && let Err(error) = self.runtime.local_mut().world.record_external_stack(stack, "development kit")
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
                        && let Some(ledger) = &mut self.runtime.local_mut().world.material_ledger
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
                    if let Some(ledger) = &mut self.runtime.local_mut().world.material_ledger
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
        self.runtime.local_mut().world.clear_loose_items();
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
        *self.runtime.time_of_day_mut() = 0.3;
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
                self.player.pos = self.runtime.local_mut().world.settle_spawn_at(self.player.pos);
                self.player.vel = Vec3::ZERO;
            }
            // And a save whose terrain changed underneath it (built
            // over, regenerated) comes back beside the hill, not in
            // it. Mid-air/mid-swim saves pass through untouched.
            let freed = self.runtime.local_mut().world.free_position_at(self.player.pos);
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
        self.runtime.local_mut().sync_tier();
        self.content
            .scripts
            .load_kv(&PathBuf::from("saves").join(name));
        self.load_loose_items(&PathBuf::from("saves").join(name));
        if self.content.scripts.wants("on_world_start") {
            self.content.scripts.dispatch_view(
                &self.runtime.view(),
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
}
