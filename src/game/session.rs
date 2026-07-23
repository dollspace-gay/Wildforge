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
        let sy = world.surface_height(sx, sz) + 1;
        let spawn = Vec3::new(sx as f32 + 0.5, sy as f32 + 0.2, sz as f32 + 0.5);

        self.renderer.clear_chunks();
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
        // Dev: drop a water source on a pillar ahead of spawn to watch it flow.
        if std::env::var("WILDFORGE_DEMO_WATER").is_ok() {
            let (bx, bz) = (spawn.x as i32 - 6, spawn.z as i32 - 14);
            for cx in -1..=1 {
                for cz in -1..=1 {
                    self.server.world.ensure_chunk(crate::chunk::ChunkPos {
                        x: bx.div_euclid(16) + cx,
                        z: bz.div_euclid(16) + cz,
                    });
                }
            }
            let by = self.server.world.surface_height(bx, bz);
            let stone = self.content.reg.block_id("base:stone").unwrap_or(AIR);
            let water = self.content.reg.block_id("base:water").unwrap_or(AIR);
            for y in by + 1..=by + 4 {
                self.server.world.set_block(bx, y, bz, stone);
            }
            self.server.world.set_block(bx, by + 5, bz, water);
            eprintln!(
                "demo water source at ({bx},{},{bz}), spawn {:?}",
                by + 5,
                spawn
            );
        }
        self.set_screen(Screen::Playing);
        // Dev: force time of day (0..1; 0.75 = midnight).
        if let Ok(t) = std::env::var("WILDFORGE_TIME")
            && let Ok(t) = t.parse::<f32>()
        {
            self.server.time_of_day = t.fract();
        }
        // Dev: force camera look ("yaw,pitch" in radians) for framed captures.
        self.apply_look_env();
        // Dev: WILDFORGE_SCREEN=inventory opens the pack in-world for
        // layout screenshots (menu screens are handled at startup).
        if std::env::var("WILDFORGE_SCREEN").as_deref() == Ok("inventory") {
            self.set_screen(Screen::Inventory);
        }
        // Dev: a ring of torches near spawn (lighting verification).
        if std::env::var("WILDFORGE_DEMO_TORCH").is_ok()
            && let Some(torch) = self.content.reg.block_id("base:torch")
        {
            for (dx, dz) in [(3, 0), (-3, 2), (0, 4), (2, -4)] {
                let (x, z) = (spawn.x as i32 + dx, spawn.z as i32 + dz);
                let y = self.server.world.surface_height(x, z);
                self.server.world.set_block(x, y + 1, z, torch);
            }
        }
        // Dev: two pillars flanked by a blue and a red lamp — colored-shadow
        // test (each pillar should cast a blue shadow away from the blue lamp
        // and a red one away from the red lamp, purple where both reach).
        if std::env::var("WILDFORGE_DEMO_COLORSHADOW").is_ok() {
            let blue = self.content.reg.block_id("base:blue_lamp");
            let red = self.content.reg.block_id("base:red_lamp");
            let stone = self.content.reg.block_id("base:cobblestone");
            let bx = spawn.x as i32;
            let bz = spawn.z as i32 + 4;
            let y = self.server.world.surface_height(bx, bz);
            if let Some(stone) = stone {
                // A neutral grey floor reads colored light far better than grass.
                for dx in -8..=8 {
                    for dz in -6..=8 {
                        self.server.world.set_block(bx + dx, y, bz + dz, stone);
                    }
                }
                // Two pillars as occluders.
                for px in [-2i32, 2] {
                    for h in 1..=3 {
                        self.server.world.set_block(bx + px, y + h, bz, stone);
                    }
                }
            }
            // Low colored lamps to either side so shadows rake across the floor.
            if let Some(b) = blue {
                self.server.world.set_block(bx - 5, y + 2, bz, b);
            }
            if let Some(r) = red {
                self.server.world.set_block(bx + 5, y + 2, bz, r);
            }
        }
        // Dev: two pillars on a grey floor lit by a blue and a red dynamic
        // point light (sharp per-light shadows). Pair with
        // WILDFORGE_AMBIENT=0.05,0.05,0.05 for stark contrast.
        if std::env::var("WILDFORGE_DEMO_PTLIGHT").is_ok() {
            let stone = self.content.reg.block_id("base:cobblestone");
            let bx = spawn.x as i32;
            let bz = spawn.z as i32 + 5;
            let y = self.server.world.surface_height(bx, bz);
            if let Some(stone) = stone {
                for dx in -9..=9 {
                    for dz in -7..=9 {
                        self.server.world.set_block(bx + dx, y, bz + dz, stone);
                    }
                }
                for px in [-2i32, 2] {
                    for h in 1..=3 {
                        self.server.world.set_block(bx + px, y + h, bz, stone);
                    }
                }
            }
            let fy = (y + 2) as f32 + 0.5;
            self.presentation.demo_lights = vec![
                lights::DynLight {
                    key: lights::Key::Demo(0),
                    pos: Vec3::new(bx as f32 - 5.0 + 0.5, fy, bz as f32 + 0.5),
                    range: 16.0,
                    color: Vec3::new(0.35, 0.6, 2.0),
                },
                lights::DynLight {
                    key: lights::Key::Demo(1),
                    pos: Vec3::new(bx as f32 + 5.0 + 0.5, fy, bz as f32 + 0.5),
                    range: 16.0,
                    color: Vec3::new(2.0, 0.35, 0.3),
                },
            ];
        }
        // Dev: a dusk campsite for the README hero shot — torch posts
        // throwing hard shadows across the grass, a blue-glass lantern
        // staining its pool, a chest and anvil for life.
        if std::env::var("WILDFORGE_DEMO_CAMP").is_ok() {
            let b = |n: &str| self.content.reg.block_id(n);
            let bx = spawn.x as i32;
            let bz = spawn.z as i32;
            for dx in [-8i32, 0, 8] {
                for dz in [-8i32, 0, 8] {
                    self.server
                        .world
                        .ensure_chunk(ChunkPos::of_world(bx + dx, bz + dz));
                }
            }
            if let (Some(log), Some(torch)) = (b("base:log"), b("base:torch")) {
                // A clearing: no trunks photobombing the campfire.
                for dx in -6..=6i32 {
                    for dz in -1..=12i32 {
                        let (x, z) = (bx + dx, bz + dz);
                        let y = self.server.world.surface_height(x, z);
                        for h in 1..=9 {
                            if self.server.world.get_block(x, y + h, z) != AIR {
                                self.server.world.set_block(x, y + h, z, AIR);
                            }
                        }
                    }
                }
                // Torch posts: a 2-log stake with the flame on top.
                for (px, pz) in [(4i32, 4i32), (-4, 6), (0, 10)] {
                    let (x, z) = (bx + px, bz + pz);
                    let y = self.server.world.surface_height(x, z);
                    self.server.world.set_block(x, y + 1, z, log);
                    self.server.world.set_block(x, y + 2, z, log);
                    self.server.world.set_block(x, y + 3, z, torch);
                }
            }
            for (name, px, pz) in [("base:chest", 2i32, 7i32), ("base:stone_anvil", -2, 4)] {
                if let Some(blk) = b(name) {
                    let (x, z) = (bx + px, bz + pz);
                    let y = self.server.world.surface_height(x, z);
                    self.server.world.set_block(x, y + 1, z, blk);
                }
            }
            let reg = self.content.reg.clone();
            if let Some(t) = reg.item_id("base:torch") {
                self.inventory.add(&reg, t, 5);
            }
        }

        // Dev: an enclosed torch-lit room — the full static pipeline
        // (mesher emitters -> promotion -> cached cube shadows), with two
        // pillars to throw hard shadows and a red-glazed alcove (stained
        // transmission). Real torch blocks, no demo lights. Built on the
        // footprint's highest ground so hills never poke through.
        if std::env::var("WILDFORGE_DEMO_TORCHROOM").is_ok()
            && let (Some(stone), Some(torch)) = (
                self.content.reg.block_id("base:cobblestone"),
                self.content.reg.block_id("base:torch"),
            )
        {
            let bx = spawn.x as i32;
            let bz = spawn.z as i32;
            // The footprint may straddle chunks that don't exist yet —
            // writes into missing chunks vanish, leaving open walls.
            for dx in [-8i32, 0, 8] {
                for dz in [-8i32, 0, 8] {
                    self.server
                        .world
                        .ensure_chunk(ChunkPos::of_world(bx + dx, bz + dz));
                }
            }
            let yf = (-7..=7)
                .flat_map(|dx| (-7..=7).map(move |dz| (dx, dz)))
                .map(|(dx, dz)| self.server.world.surface_height(bx + dx, bz + dz))
                .max()
                .unwrap_or(spawn.y as i32);
            for dx in -7..=7i32 {
                for dz in -7..=7i32 {
                    self.server.world.set_block(bx + dx, yf, bz + dz, stone);
                    let wall = dx.abs() == 7 || dz.abs() == 7;
                    for h in 1..=8 {
                        let b = if (wall && h <= 3) || h == 4 {
                            stone
                        } else {
                            AIR
                        };
                        let b = if h > 4 { AIR } else { b };
                        self.server.world.set_block(bx + dx, yf + h, bz + dz, b);
                    }
                }
            }
            for px in [-3i32, 3] {
                for h in 1..=3 {
                    self.server.world.set_block(bx + px, yf + h, bz + 3, stone);
                }
            }
            for (tx, tz) in [(-6i32, -6i32), (6, -6), (0, 6)] {
                self.server.world.set_block(bx + tx, yf + 1, bz + tz, torch);
            }
            // A red-glazed alcove: torch sealed behind a stained pane —
            // its pool outside should come out the color of the glass.
            if let Some(rg) = self.content.reg.block_id("base:red_glass") {
                let (ax, az) = (bx + 4, bz - 4);
                self.server.world.set_block(ax, yf + 1, az, stone);
                self.server.world.set_block(ax, yf + 2, az, torch);
                self.server.world.set_block(ax, yf + 3, az, stone);
                self.server.world.set_block(ax - 1, yf + 2, az, stone);
                self.server.world.set_block(ax + 1, yf + 2, az, stone);
                self.server.world.set_block(ax, yf + 2, az - 1, stone);
                self.server.world.set_block(ax, yf + 2, az + 1, rg);
            }
            // Stand in the room, whatever the terrain wanted.
            let inside = Vec3::new(bx as f32 + 0.5, yf as f32 + 1.2, bz as f32 + 0.5);
            self.player.pos = inside;
            self.survival.spawn_point = inside;
            self.camera.pos = inside + Vec3::new(0.0, EYE_HEIGHT, 0.0);
            // A torch in slot 0: WILDFORGE_SEL=0 holds it (held-light
            // shots), WILDFORGE_SEL=8 keeps the hand empty.
            let reg = self.content.reg.clone();
            if let Some(t) = reg.item_id("base:torch") {
                self.inventory.add(&reg, t, 5);
            }
        }

        // Dev: a warm light behind a wall with a doorway — light blares through
        // the gap onto the near floor while the wall and the corners beside it
        // stay dark. Pair with WILDFORGE_AMBIENT=0.03,0.03,0.04.
        if std::env::var("WILDFORGE_DEMO_CORNER").is_ok()
            && let Some(stone) = self.content.reg.block_id("base:cobblestone")
        {
            let bx = spawn.x as i32;
            let bz = spawn.z as i32 + 6;
            let y = self.server.world.surface_height(bx, bz);
            // Carve a clean flat arena: cobblestone floor, air above, so
            // grass and trees don't intrude on the shadow.
            for dx in -11..=11 {
                for dz in -9..=15 {
                    self.server.world.set_block(bx + dx, y, bz + dz, stone);
                    for h in 1..=9 {
                        self.server.world.set_block(bx + dx, y + h, bz + dz, AIR);
                    }
                }
            }
            // Wall across X at z=bz, 5 tall, with a 1-wide doorway at bx.
            for dx in -11..=11 {
                if dx == 0 {
                    continue;
                }
                for h in 1..=5 {
                    self.server.world.set_block(bx + dx, y + h, bz, stone);
                }
            }
            // Warm light on the far side of the wall — it blares through the
            // doorway and lights the far room, leaving the near side dark.
            self.presentation.demo_lights = vec![lights::DynLight {
                key: lights::Key::Demo(0),
                pos: Vec3::new(bx as f32 + 0.5, (y + 2) as f32 + 0.5, bz as f32 + 5.5),
                range: 24.0,
                color: Vec3::new(2.4, 1.7, 0.8),
            }];
        }
        // Dev: a flat water pool ahead of spawn (specular-glint verification).
        if std::env::var("WILDFORGE_DEMO_POOL").is_ok()
            && let Some(water) = self.content.reg.block_id("base:water")
        {
            let cx = spawn.x as i32;
            let cz = spawn.z as i32 + 10;
            let y = self.server.world.surface_height(cx, cz);
            for dx in -8..=8 {
                for dz in -8..=8 {
                    self.server.world.set_block(cx + dx, y, cz + dz, water);
                }
            }
        }
        // Dev: a warm torch and a red ruby block side by side (colored-light
        // verification — pools of warm and red that blend where they meet).
        if std::env::var("WILDFORGE_DEMO_COLORLIGHT").is_ok() {
            let place = |w: &mut World, name: &str, dx: i32, dz: i32| {
                if let Some(b) = w.reg.block_id(name) {
                    let (x, z) = (spawn.x as i32 + dx, spawn.z as i32 + dz);
                    let y = w.surface_height(x, z);
                    w.set_block(x, y + 1, z, b);
                }
            };
            place(&mut self.server.world, "base:torch", -2, 5);
            place(&mut self.server.world, "gems:ruby_block", 2, 5);
        }
        // Dev: a few tall pillars near spawn (shadow-casting verification).
        if std::env::var("WILDFORGE_DEMO_PILLARS").is_ok()
            && let Some(stone) = self.content.reg.block_id("base:cobblestone")
        {
            for (dx, dz, h) in [(4, 2, 6), (7, -3, 8), (-2, 6, 5), (10, 4, 7)] {
                let (x, z) = (spawn.x as i32 + dx, spawn.z as i32 + dz);
                let base = self.server.world.surface_height(x, z);
                for i in 1..=h {
                    self.server.world.set_block(x, base + i, z, stone);
                }
            }
        }
        // Dev: a ready steelworks near spawn (bloomery shell + anvil +
        // materials) for screenshots and hands-on QA.
        if std::env::var("WILDFORGE_DEMO_STEELWORKS").is_ok() {
            let b = |n: &str| self.content.reg.block_id(n);
            if let (Some(fb), Some(mouth), Some(anvil)) = (
                b("base:firebrick"),
                b("base:bloomery"),
                b("base:stone_anvil"),
            ) {
                let (sx, sz) = (spawn.x as i32 + 6, spawn.z as i32 + 4);
                let sy = self.server.world.surface_height(sx, sz) + 1;
                // Core at (sx, sy, sz); mouth on its -X side.
                for ly in 0..3 {
                    for rx in -1..=1i32 {
                        for rz in -1..=1i32 {
                            if rx == 0 && rz == 0 {
                                continue;
                            }
                            self.server.world.set_block(sx + rx, sy + ly, sz + rz, fb);
                        }
                    }
                    self.server
                        .world
                        .set_block(sx, sy + ly, sz, crate::registry::AIR);
                }
                self.server.world.set_block(sx - 1, sy, sz, mouth);
                self.server.world.set_block(sx - 3, sy, sz + 2, anvil);
                // A second stack, already charged and burning.
                let (lx, lz) = (sx, sz + 8);
                let ly = self.server.world.surface_height(lx, lz) + 1;
                for dy in 0..3 {
                    for rx in -1..=1i32 {
                        for rz in -1..=1i32 {
                            if rx == 0 && rz == 0 {
                                continue;
                            }
                            self.server.world.set_block(lx + rx, ly + dy, lz + rz, fb);
                        }
                    }
                    self.server
                        .world
                        .set_block(lx, ly + dy, lz, crate::registry::AIR);
                }
                self.server.world.set_block(lx - 1, ly, lz, mouth);
                let reg2 = self.content.reg.clone();
                if let (Some(iron), Some(coal)) = (
                    reg2.item_id("base:iron_ingot"),
                    reg2.item_id("base:charcoal"),
                ) {
                    let mut st = world::BloomeryState::default();
                    for i in 0..4 {
                        st.charge[i] = Some(ItemStack::new(&reg2, iron, 2));
                        st.fuel[i] = Some(ItemStack::new(&reg2, coal, 2));
                    }
                    self.server
                        .world
                        .insert_block_entity((lx - 1, ly, lz), world::BlockEntity::Bloomery(st));
                    let _ = self.server.world.light_bloomery(lx - 1, ly, lz);
                }
                // A bloom resting on the anvil, ready for the hammer.
                if let Some(bl) = reg2.item_id("base:steel_bloom") {
                    self.server
                        .world
                        .anvil_put((sx - 3, sy, sz + 2), ItemStack::new(&reg2, bl, 1));
                }
                let reg = self.content.reg.clone();
                for (name, n) in [
                    ("base:iron_ingot", 8),
                    ("base:charcoal", 8),
                    ("base:ember", 2),
                    ("base:smith_hammer", 1),
                    ("base:steel_bloom", 2),
                    ("base:log", 8),
                    ("base:dirt", 32),
                ] {
                    if let Some(item) = reg.item_id(name) {
                        self.inventory.add(&reg, item, n);
                    }
                }
            }
        }
        // Dev: a glassworks yard - kiln stack, quern, minerals, sand.
        // Dev: stage the juice layer for screenshots — a trodden snow
        // trail, low health (heart wobble + vignette), and a debris
        // burst frozen mid-flight.
        if std::env::var("WILDFORGE_DEMO_JUICE").is_ok() {
            let b = |n: &str| self.content.reg.block_id(n);
            if let (Some(layer), Some(dirt)) = (b("base:snow_layer"), b("base:dirt")) {
                let (sx, sz) = (spawn.x as i32 + 4, spawn.z as i32 - 2);
                let sy = self.server.world.surface_height(sx, sz);
                for rx in 0..6i32 {
                    for rz in -2..=2i32 {
                        self.server.world.set_block(sx + rx, sy, sz + rz, dirt);
                        self.server.world.set_block(sx + rx, sy + 1, sz + rz, layer);
                    }
                }
                // A walker crossed the field on the diagonal.
                for i in 0..5i32 {
                    self.server.world.tread(sx + i, sy + 1, sz - 2 + i);
                }
                // A break mid-burst, sparks and all; the tick re-stamps
                // the moment so any capture frame lands mid-effect.
                let center = Vec3::new(sx as f32 + 2.5, sy as f32 + 2.5, sz as f32 + 0.5);
                self.presentation.demo_burst =
                    Some((center, self.content.reg.block(dirt).tiles[0]));
                self.juice_burst(center, self.content.reg.block(dirt).tiles[0], 10, 2.2);
            }
            self.survival.health = 5.0;
            self.survival.damage_flash = 0.35;
        }

        if std::env::var("WILDFORGE_DEMO_GLASSWORKS").is_ok() {
            let b = |n: &str| self.content.reg.block_id(n);
            if let (Some(fb), Some(kiln), Some(quern)) =
                (b("base:firebrick"), b("base:kiln"), b("base:quern"))
            {
                let (sx, sz) = (spawn.x as i32 + 6, spawn.z as i32 - 6);
                let sy = self.server.world.surface_height(sx, sz) + 1;
                for ly in 0..3 {
                    for rx in -1..=1i32 {
                        for rz in -1..=1i32 {
                            if rx == 0 && rz == 0 {
                                continue;
                            }
                            self.server.world.set_block(sx + rx, sy + ly, sz + rz, fb);
                        }
                    }
                    self.server
                        .world
                        .set_block(sx, sy + ly, sz, crate::registry::AIR);
                }
                self.server.world.set_block(sx - 1, sy, sz, kiln);
                self.server.world.set_block(sx - 3, sy, sz + 2, quern);
                let reg = self.content.reg.clone();
                if let (Some(sand), Some(coal), Some(pow)) = (
                    reg.item_id("base:sand"),
                    reg.item_id("base:charcoal"),
                    reg.item_id("base:cobalt_powder"),
                ) {
                    let mut st = world::KilnState::default();
                    for i in 0..4 {
                        st.sand[i] = Some(ItemStack::new(&reg, sand, 2));
                        st.fuel[i] = Some(ItemStack::new(&reg, coal, 2));
                    }
                    st.powder = Some(ItemStack::new(&reg, pow, 1));
                    self.server
                        .world
                        .insert_block_entity((sx - 1, sy, sz), world::BlockEntity::Kiln(st));
                    let _ = self.server.world.light_kiln(sx - 1, sy, sz);
                }
                for (name, n) in [
                    ("base:sand", 16),
                    ("base:raw_cobalt", 4),
                    ("base:raw_cinnabar", 4),
                    ("base:charcoal", 8),
                    ("base:ember", 2),
                    ("base:blue_glass", 8),
                    ("base:glass", 8),
                ] {
                    if let Some(item) = reg.item_id(name) {
                        self.inventory.add(&reg, item, n);
                    }
                }
                // Torches behind stained panes: the light comes out
                // the color of the glass (stage 5's proof).
                let (tx2, tz2) = (spawn.x as i32 - 8, spawn.z as i32 + 2);
                let ty2 = self.server.world.surface_height(tx2, tz2) + 1;
                if let (Some(stone), Some(torch), Some(rg), Some(bg)) = (
                    b("base:stone"),
                    b("base:torch"),
                    b("base:red_glass"),
                    b("base:blue_glass"),
                ) {
                    for (i, pane) in [rg, bg].iter().enumerate() {
                        let z = tz2 + i as i32 * 3;
                        // A stone alcove holding a torch, glazed shut.
                        for dy in -1..=1i32 {
                            for dz in -1..=1i32 {
                                self.server
                                    .world
                                    .set_block(tx2 - 1, ty2 + dy, z + dz, stone);
                                if dy != 0 || dz != 0 {
                                    self.server.world.set_block(tx2, ty2 + dy, z + dz, stone);
                                }
                            }
                        }
                        self.server.world.set_block(tx2, ty2, z, torch);
                        self.server.world.set_block(tx2 + 1, ty2, z, *pane);
                    }
                }
                // A stained window row so the tint shows in shots.
                let (wx, wz) = (spawn.x as i32 - 5, spawn.z as i32);
                let wy = self.server.world.surface_height(wx, wz) + 1;
                for (i, g) in [
                    "base:glass",
                    "base:teal_glass",
                    "base:amber_glass",
                    "base:blue_glass",
                    "base:red_glass",
                    "base:violet_glass",
                ]
                .iter()
                .enumerate()
                {
                    if let Some(gb) = b(g) {
                        self.server.world.set_block(wx, wy, wz + i as i32, gb);
                        self.server.world.set_block(wx, wy + 1, wz + i as i32, gb);
                    }
                }
            }
        }
        // Dev: WILDFORGE_IRE=N forces the wild's ire (spawn testing).
        if let Ok(v) = std::env::var("WILDFORGE_IRE")
            && let Ok(v) = v.parse::<f32>()
        {
            self.server.world.ire = v.clamp(0.0, 100.0);
            self.server.sync_tier();
        }
        // Dev: force the calendar and the sky.
        if let Ok(v) = std::env::var("WILDFORGE_DAY")
            && let Ok(v) = v.parse::<u32>()
        {
            self.server.world.day = v;
        }
        if let Ok(v) = std::env::var("WILDFORGE_SEASON")
            && let Ok(v) = v.parse::<u32>()
        {
            self.server.world.day = (v % 4) * world::SEASON_DAYS;
        }
        if let Ok(v) = std::env::var("WILDFORGE_WEATHER") {
            self.server.world.weather = world::Weather::from_name(&v);
            self.server.world.weather_timer = 1.0e9; // pinned for the session
            self.presentation.weather_vis = match self.server.world.weather {
                world::Weather::Clear => 0.0,
                world::Weather::Overcast => 0.4,
                world::Weather::Precip => 0.55,
                world::Weather::Storm => 0.7,
            };
        }
        // Dev: a row of wardens near spawn (rendering/combat verification).
        if std::env::var("WILDFORGE_DEMO_WARDENS").is_ok() {
            for (i, name) in [
                "base:thornling",
                "base:dryad",
                "base:emberkin",
                "base:gravelurk",
                "base:wrathwood",
            ]
            .iter()
            .enumerate()
            {
                if let Some(si) = self.content.reg.animal_id(name) {
                    let x = spawn.x as i32 - 4 + i as i32 * 3;
                    let z = spawn.z as i32 - 7;
                    let y = self.server.world.surface_height(x, z) + 1;
                    let mut m = mobs::Mob::new(
                        si,
                        Vec3::new(x as f32 + 0.5, y as f32 + 0.05, z as f32 + 0.5),
                        0.0,
                    );
                    m.health = self.content.reg.animals[si].health;
                    self.server.world.spawn_mob(m);
                }
            }
        }
        // Dev: stewardship showcase — offering stone with gifts, a planted
        // sapling, and a grown oak (verification).
        if std::env::var("WILDFORGE_DEMO_STEWARD").is_ok() {
            let reg = self.content.reg.clone();
            let (sx, sz) = (spawn.x as i32, spawn.z as i32);
            if let Some(os) = reg.block_id("base:offering_stone") {
                let y = self.server.world.surface_height(sx - 3, sz - 5) + 1;
                self.server.world.set_block(sx - 3, y, sz - 5, os);
                let mut st = world::OfferingState::default();
                if let Some(hw) = reg.item_id("base:heartwood") {
                    st.slots[0] = Some(ItemStack::new(&reg, hw, 2));
                }
                self.server
                    .world
                    .insert_block_entity((sx - 3, y, sz - 5), world::BlockEntity::Offering(st));
            }
            if let Some(sap) = reg.block_id("base:oak_sapling") {
                let y = self.server.world.surface_height(sx + 2, sz - 6) + 1;
                self.server.world.set_block(sx + 2, y, sz - 6, sap);
            }
            let ty = self.server.world.surface_height(sx + 6, sz - 8) + 1;
            self.server.world.grow_tree(sx + 6, ty, sz - 8, "oak", 3);
            for name in ["base:bedroll", "base:oak_sapling"] {
                if let Some(item) = reg.item_id(name) {
                    self.inventory.add(&reg, item, 1);
                }
            }
        }
        // Dev: a stocked chest next to spawn, screen open (UI verification).
        if std::env::var("WILDFORGE_DEMO_CHEST").is_ok() {
            let p = (spawn.x as i32 - 2, spawn.y as i32, spawn.z as i32);
            let reg = self.content.reg.clone();
            if let Some(cb) = reg.block_id("base:chest") {
                self.server.world.set_block(p.0, p.1, p.2, cb);
                let mut st = world::ChestState::default();
                for (i, (name, n)) in [
                    ("base:bread", 5),
                    ("base:torch", 12),
                    ("base:bronze_sword", 1),
                ]
                .iter()
                .enumerate()
                {
                    if let Some(item) = reg.item_id(name) {
                        st.slots[i * 4] = Some(ItemStack::new(&reg, item, *n));
                    }
                }
                self.server
                    .world
                    .insert_block_entity(p, world::BlockEntity::Chest(st));
                self.set_screen(Screen::Chest(p));
            }
        }
        // Dev/headless: open the inventory for UI verification.
        if std::env::var("WILDFORGE_SCREEN").as_deref() == Ok("inventory") {
            self.interaction.craft_size = 2;
            self.set_screen(Screen::Inventory);
        }
        // Dev: a small menagerie near spawn (rendering/combat verification).
        if std::env::var("WILDFORGE_DEMO_MOBS").is_ok() {
            for (i, name) in [
                "base:deer",
                "base:boar",
                "base:goat",
                "base:grouse",
                "base:rabbit",
            ]
            .iter()
            .enumerate()
            {
                if let Some(si) = self.content.reg.animal_id(name) {
                    let x = spawn.x as i32 - 3 + i as i32 * 2;
                    let z = spawn.z as i32 - 6;
                    let y = self.server.world.surface_height(x, z) + 1;
                    let mut m = mobs::Mob::new(
                        si,
                        Vec3::new(x as f32 + 0.5, y as f32 + 0.05, z as f32 + 0.5),
                        i as f32 * 1.3,
                    );
                    m.health = self.content.reg.animals[si].health;
                    self.server.world.spawn_mob(m);
                }
            }
        }
        // Dev: a stocked furnace next to spawn, screen open (UI verification).
        if std::env::var("WILDFORGE_DEMO_FURNACE").is_ok() {
            let p = (spawn.x as i32 + 2, spawn.y as i32, spawn.z as i32);
            let reg = self.content.reg.clone();
            if let (Some(fb), Some(raw), Some(log)) = (
                reg.block_id("base:furnace"),
                reg.item_id("base:raw_copper"),
                reg.item_id("base:log"),
            ) {
                self.server.world.set_block(p.0, p.1, p.2, fb);
                self.server.world.insert_block_entity(
                    p,
                    world::BlockEntity::Furnace(world::FurnaceState {
                        input: Some(ItemStack::new(&reg, raw, 5)),
                        fuel: Some(ItemStack::new(&reg, log, 3)),
                        ..Default::default()
                    }),
                );
                self.inventory
                    .add(&reg, reg.item_id("base:copper_ingot").unwrap(), 7);
                self.set_screen(Screen::Furnace(p));
            }
        }
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
        if !self.in_world {
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
        let _ = std::fs::write(
            self.server.world.save_dir_for_saving().join("player.toml"),
            out,
        );
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
            #[serde(default)]
            slot: Vec<SlotT>,
            #[serde(default)]
            armor: Vec<SlotT>,
        }
        let Ok(text) = std::fs::read_to_string(dir.join("player.toml")) else {
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
            self.save_player(); // guest profile under saves/.remote/profile
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
