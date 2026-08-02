//! The capture harness: every `WILDFORGE_*` scene a headless run can stage.
//!
//! Deterministic screenshots are how this project verifies rendering, and the
//! scenes below are the fixtures those shots need — a torch-lit room, a
//! working mill, a country's edifice, a pool mid-pour. All of it is real, and
//! none of it is part of starting a world, which is where it used to live:
//! `start_world` was two thousand lines, of which about two hundred started a
//! world.
//!
//! Nothing here runs unless the matching environment variable is set, so an
//! ordinary session pays one function call for the lot.

use super::*;
use crate::planet::{BlockPos, EntityPos, Face, SurfacePos};

/// Face-local drafting coordinates for capture scenes. Scene descriptions use
/// compact signed offsets because a room or mill is easier to read that way;
/// this adapter is the single boundary where those offsets become canonical
/// planetary addresses (including seam crossings).
#[derive(Clone, Copy)]
struct DemoChart {
    face: Face,
}

impl DemoChart {
    const fn new(face: Face) -> Self {
        Self { face }
    }

    fn surface(self, x: i32, z: i32) -> SurfacePos {
        let half = i32::from(crate::planet::FACE_BLOCKS) / 2;
        SurfacePos::canonicalized(self.face, x + half, z + half)
            .expect("a capture scene stays within one face crossing")
    }

    fn block(self, x: i32, y: i32, z: i32) -> BlockPos {
        let surface = self.surface(x, z);
        BlockPos::new(surface.face(), surface.u(), y as u8, surface.v())
            .expect("capture scene height stays inside the voxel shell")
    }

    fn block_tuple(self, pos: (i32, i32, i32)) -> BlockPos {
        self.block(pos.0, pos.1, pos.2)
    }

    fn chunk(self, x: i32, z: i32) -> ChunkPos {
        ChunkPos::from_surface(self.surface(x, z))
    }

    fn entity(self, local: Vec3) -> EntityPos {
        let half = f32::from(crate::planet::FACE_BLOCKS) * 0.5;
        EntityPos::new(self.face, half, local.y, half)
            .expect("the chart center is canonical")
            .translated(Vec3::new(local.x, 0.0, local.z))
            .expect("a capture actor crosses only nearby planet faces")
            .pos
    }
}

macro_rules! demo_set {
    ($world:expr, $chart:expr, $x:expr, $y:expr, $z:expr, $block:expr $(,)?) => {
        ($world).set_block_authored_at(
            ($chart).block($x, $y, $z),
            $block,
            "development capture scene",
        )
    };
}

macro_rules! demo_get {
    ($world:expr, $chart:expr, $x:expr, $y:expr, $z:expr $(,)?) => {
        ($world).get_block_at(($chart).block($x, $y, $z))
    };
}

macro_rules! demo_meta {
    ($world:expr, $chart:expr, $x:expr, $y:expr, $z:expr, $block:expr, $meta:expr $(,)?) => {
        ($world).set_block_meta_at(($chart).block($x, $y, $z), $block, $meta)
    };
}

macro_rules! demo_height {
    ($world:expr, $chart:expr, $x:expr, $z:expr $(,)?) => {
        ($world).surface_height_at(($chart).surface($x, $z))
    };
}

macro_rules! demo_insert {
    ($world:expr, $chart:expr, $pos:expr, $entity:expr $(,)?) => {
        ($world).insert_block_entity_authored_at(
            ($chart).block_tuple($pos),
            $entity,
            "development capture scene",
        )
    };
}

macro_rules! demo_mob {
    ($chart:expr, $species:expr, $local:expr, $yaw:expr $(,)?) => {
        crate::mobs::Mob::new_at($species, ($chart).entity($local), $yaw)
    };
}

macro_rules! demo_drop {
    ($world:expr, $chart:expr, $pos:expr, $stack:expr $(,)?) => {{
        let stack = $stack;
        if let Err(error) = ($world).record_external_stack(stack, "development capture loose item")
        {
            eprintln!("materials: capture drop source failed: {error}");
        }
        ($world).push_drop_at(($chart).block_tuple($pos), stack)
    }};
}

macro_rules! demo_anvil_put {
    ($world:expr, $chart:expr, $pos:expr, $stack:expr $(,)?) => {{
        let stack = $stack;
        let inserted = ($world).anvil_put_at(($chart).block_tuple($pos), stack);
        if inserted
            && let Err(error) =
                ($world).record_external_stack(stack, "development capture workstation")
        {
            eprintln!("materials: capture workstation source failed: {error}");
        }
        inserted
    }};
}

macro_rules! demo_ire {
    ($world:expr, $chart:expr, $x:expr, $z:expr, $amount:expr $(,)?) => {
        ($world).add_ire_at_surface(($chart).surface($x, $z), $amount)
    };
}

macro_rules! demo_bloom {
    ($world:expr, $chart:expr, $x:expr, $z:expr, $days:expr $(,)?) => {
        ($world).add_bloom_at_surface(($chart).surface($x, $z), $days)
    };
}

macro_rules! demo_standing {
    ($world:expr, $chart:expr, $x:expr, $z:expr $(,)?) => {
        ($world).regional_ire_at_surface(($chart).surface($x, $z))
    };
}

macro_rules! demo_fire {
    ($world:expr, $chart:expr, $x:expr, $y:expr, $z:expr, $mine:expr $(,)?) => {
        ($world).light_fire_at(($chart).block($x, $y, $z), $mine)
    };
}

impl Game {
    fn give_dev_item(&mut self, reg: &Registry, item: ItemId, count: u32) {
        let left = self.inventory.add(reg, item, count);
        let added = count.saturating_sub(left);
        if added != 0
            && let Err(error) = self.server.world.record_external_stack(
                ItemStack::new(reg, item, added),
                "development capture inventory",
            )
        {
            eprintln!("materials: capture inventory source failed: {error}");
        }
    }

    /// Stage whatever scene the environment asks for, once, at world start.
    ///
    /// Called immediately after the world is loaded and the player is placed,
    /// which is exactly where these blocks used to sit inline.
    pub(super) fn apply_dev_overrides(&mut self, spawn: crate::planet::EntityPos) {
        let chart = DemoChart::new(spawn.face());
        // Dev: drop a water source on a pillar ahead of spawn to watch it flow.
        if std::env::var("WILDFORGE_DEMO_WATER").is_ok() {
            let (bx, bz) = (spawn.x as i32 - 6, spawn.z as i32 - 14);
            for cx in -1..=1 {
                for cz in -1..=1 {
                    self.server
                        .world
                        .ensure_chunk(chart.chunk(bx, bz).offset(cx, cz));
                }
            }
            let by = demo_height!(self.server.world, chart, bx, bz);
            let stone = self.content.reg.block_id("base:stone").unwrap_or(AIR);
            let water = self.content.reg.block_id("base:water").unwrap_or(AIR);
            for y in by + 1..=by + 4 {
                demo_set!(self.server.world, chart, bx, y, bz, stone);
            }
            demo_set!(self.server.world, chart, bx, by + 5, bz, water);
            eprintln!(
                "demo water source at ({bx},{},{bz}), spawn {:?}",
                by + 5,
                spawn
            );
        }
        // Dev/headless: the in-world screens can only be reached by
        // playing, which a shot run cannot do. This makes the paper
        // doll and the slot grid verifiable from a capture.
        // Dev/headless: park the UI cursor, so a capture can show a
        // hover state (tooltips) that otherwise needs a real mouse.
        if let Ok(spec) = std::env::var("WILDFORGE_CURSOR")
            && let Some((x, z)) = spec.split_once(',')
            && let (Ok(x), Ok(y)) = (x.trim().parse::<f32>(), z.trim().parse::<f32>())
        {
            self.input.ui_cursor = (x, y);
            self.input.cursor_locked = true;
        }
        match std::env::var("WILDFORGE_SCREEN").as_deref() {
            Ok("inventory") => self.set_screen(Screen::Inventory),
            Ok("status") => {
                self.set_screen(Screen::Inventory);
                self.ui_state.inventory_status_open = true;
                self.ui_state.inventory_browser_open = false;
            }
            _ => {}
        }
        // Dev: force time of day (0..1; 0.75 = midnight).
        if let Ok(t) = std::env::var("WILDFORGE_TIME")
            && let Ok(t) = t.parse::<f32>()
        {
            self.server.time_of_day = t.fract();
        }
        // Dev: force the calendar day, to land on a specific moon phase
        // (day % LUNAR_DAYS; 0 = new, 4 = full at the default cycle length).
        if let Ok(d) = std::env::var("WILDFORGE_DAY")
            && let Ok(d) = d.parse::<u32>()
        {
            self.server.world.day = d;
        }
        // Planetary visual qualification needs to show a whole valley,
        // shoreline, or treeline rather than whatever happens to occupy the
        // player's eye-height foreground. Lift only automated captures into a
        // stationary creative flyover; ordinary starts and interactive play
        // are untouched.
        if self.auto_shot.is_some()
            && let Ok(height) = std::env::var("WILDFORGE_SHOT_ALTITUDE")
            && let Ok(height) = height.parse::<f32>()
        {
            self.player.pos.y = (self.player.pos.y + height.clamp(0.0, 96.0))
                .min(crate::chunk::CHUNK_Y as f32 - 3.0);
            self.player.vel = Vec3::ZERO;
            self.flying = true;
            self.camera.follow_planet(self.player.eye());
        }
        if self.auto_shot.is_some() {
            self.server.freeze_clock = true;
        }
        // Dev: WILDFORGE_HELD=<item> puts an item in the selected hotbar slot,
        // so a headless run can hold a torch — the held-light path is otherwise
        // only reachable by playing.
        if let Ok(name) = std::env::var("WILDFORGE_HELD") {
            let reg = self.content.reg.clone();
            match reg
                .item_id(&name)
                .or_else(|| reg.item_id(&format!("base:{name}")))
            {
                Some(item) => {
                    if let Some(previous) = self.inventory.slots[self.input.hotbar_sel]
                        && let Err(error) = self.server.world.record_admin_stack_deletion(previous)
                    {
                        eprintln!("materials: held-item override deletion failed: {error}");
                    }
                    let stack = ItemStack::new(&reg, item, 1);
                    if let Err(error) = self
                        .server
                        .world
                        .record_external_stack(stack, "development held-item override")
                    {
                        eprintln!("materials: held-item override source failed: {error}");
                    }
                    self.inventory.slots[self.input.hotbar_sel] = Some(stack);
                }
                None => eprintln!("WILDFORGE_HELD: no item named {name:?}"),
            }
        }
        self.load_attunements();
        // Dev: WILDFORGE_POS="x,y,z" teleports to an exact spot (reproducing
        // reported coordinates). Runs after load so it wins.
        if let Ok(s) = std::env::var("WILDFORGE_POS") {
            let p: Vec<f32> = s.split(',').filter_map(|v| v.trim().parse().ok()).collect();
            if p.len() == 3 {
                let cp = chart.chunk(p[0] as i32, p[2] as i32);
                for dx in -2..=2 {
                    for dz in -2..=2 {
                        self.server.world.ensure_chunk(cp.offset(dx, dz));
                    }
                }
                self.player.pos = self
                    .player
                    .pos
                    .relocated_local(Vec3::new(p[0], p[1], p[2]))
                    .unwrap();
                self.player.vel = Vec3::ZERO;
                self.camera.follow_planet(self.player.eye());
            }
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
                let y = demo_height!(self.server.world, chart, x, z);
                demo_set!(self.server.world, chart, x, y + 1, z, torch);
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
            let y = demo_height!(self.server.world, chart, bx, bz);
            if let Some(stone) = stone {
                // A neutral grey floor reads colored light far better than grass.
                for dx in -8..=8 {
                    for dz in -6..=8 {
                        demo_set!(self.server.world, chart, bx + dx, y, bz + dz, stone);
                    }
                }
                // Two pillars as occluders.
                for px in [-2i32, 2] {
                    for h in 1..=3 {
                        demo_set!(self.server.world, chart, bx + px, y + h, bz, stone);
                    }
                }
            }
            // Low colored lamps to either side so shadows rake across the floor.
            if let Some(b) = blue {
                demo_set!(self.server.world, chart, bx - 5, y + 2, bz, b);
            }
            if let Some(r) = red {
                demo_set!(self.server.world, chart, bx + 5, y + 2, bz, r);
            }
        }
        // Dev: an enclosed cobblestone room with a 1-wide door and a 2x2 east
        // window, for eyeballing interior lighting — the sky occlusion (walls go
        // dark away from the openings) and the cascaded-shadow sunbeam that
        // tracks across the floor through the window. `=torch` also plants one.
        if std::env::var("WILDFORGE_DEMO_ROOM").is_ok()
            && let Some(stone) = self.content.reg.block_id("base:cobblestone")
        {
            let (bx, bz) = (spawn.x as i32, spawn.z as i32);
            let fy = demo_height!(self.server.world, chart, bx, bz);
            for dx in -4..=4 {
                for dz in -4..=4 {
                    for dy in 0..=6 {
                        let shell =
                            dx == -4 || dx == 4 || dz == -4 || dz == 4 || dy == 0 || dy == 6;
                        let b = if shell { stone } else { AIR };
                        demo_set!(self.server.world, chart, bx + dx, fy + dy, bz + dz, b);
                    }
                }
            }
            // A 1-wide, 2-tall door in the +z wall.
            demo_set!(self.server.world, chart, bx, fy + 1, bz + 4, AIR);
            demo_set!(self.server.world, chart, bx, fy + 2, bz + 4, AIR);
            // A 2x2 window high in the +x (east) wall — the morning sun throws
            // a bright quad onto the floor that tracks across it.
            for wy in 3..=4 {
                for wz in -1..=0 {
                    demo_set!(self.server.world, chart, bx + 4, fy + wy, bz + wz, AIR);
                }
            }
            if std::env::var("WILDFORGE_DEMO_ROOM").as_deref() == Ok("torch")
                && let Some(torch) = self.content.reg.block_id("base:torch")
            {
                demo_set!(self.server.world, chart, bx + 2, fy + 1, bz, torch);
            }
            // Stand the player inside (this world has a saved position).
            self.player.pos = self
                .player
                .pos
                .relocated_local(Vec3::new(
                    bx as f32 + 0.5,
                    (fy + 1) as f32 + 0.2,
                    bz as f32 - 2.5,
                ))
                .unwrap();
            self.camera.follow_planet(self.player.eye());
        }
        // Dev: two pillars on a grey floor lit by a blue and a red dynamic
        // point light (sharp per-light shadows). Pair with
        // WILDFORGE_AMBIENT=0.05,0.05,0.05 for stark contrast.
        if std::env::var("WILDFORGE_DEMO_PTLIGHT").is_ok() {
            let stone = self.content.reg.block_id("base:cobblestone");
            let bx = spawn.x as i32;
            let bz = spawn.z as i32 + 5;
            let y = demo_height!(self.server.world, chart, bx, bz);
            if let Some(stone) = stone {
                for dx in -9..=9 {
                    for dz in -7..=9 {
                        demo_set!(self.server.world, chart, bx + dx, y, bz + dz, stone);
                    }
                }
                for px in [-2i32, 2] {
                    for h in 1..=3 {
                        demo_set!(self.server.world, chart, bx + px, y + h, bz, stone);
                    }
                }
            }
            let fy = (y + 2) as f32 + 0.5;
            self.presentation.demo_lights = vec![
                lights::DynLight {
                    key: lights::Key::Demo(0),
                    pos: chart
                        .entity(Vec3::new(bx as f32 - 5.0 + 0.5, fy, bz as f32 + 0.5))
                        .render_pos(),
                    range: 16.0,
                    color: Vec3::new(0.35, 0.6, 2.0),
                },
                lights::DynLight {
                    key: lights::Key::Demo(1),
                    pos: chart
                        .entity(Vec3::new(bx as f32 + 5.0 + 0.5, fy, bz as f32 + 0.5))
                        .render_pos(),
                    range: 16.0,
                    color: Vec3::new(2.0, 0.35, 0.3),
                },
            ];
        }
        // Dev: a dusk campsite for the README hero shot — torch posts
        // throwing hard shadows across the grass, a blue-glass lantern
        // staining its pool, a chest and anvil for life.
        // Dev: the whole trade arc in one clearing — stall (stocked),
        // sign, waystone, saddlebagged deer, and a boat on a dug pond.
        if std::env::var("WILDFORGE_DEMO_TRADE").is_ok() {
            let b = |n: &str| self.content.reg.block_id(n);
            let reg2 = self.content.reg.clone();
            let bx = spawn.x as i32;
            let bz = spawn.z as i32;
            for dx in [-16i32, 0, 16] {
                for dz in [-16i32, 0, 16] {
                    self.server
                        .world
                        .ensure_chunk(chart.chunk(bx + dx, bz + dz));
                }
            }
            let y = demo_height!(self.server.world, chart, bx, bz);
            // Clear and floor the clearing.
            if let Some(grass) = b("base:grass") {
                for dx in -10..=10i32 {
                    for dz in -4..=14i32 {
                        let (x, z) = (bx + dx, bz + dz);
                        demo_set!(self.server.world, chart, x, y, z, grass);
                        for h in 1..=8 {
                            if demo_get!(self.server.world, chart, x, y + h, z) != AIR {
                                demo_set!(self.server.world, chart, x, y + h, z, AIR);
                            }
                        }
                    }
                }
            }
            // The stall, stocked and priced.
            if let (Some(counter), Some(log), Some(planks)) =
                (b("base:stall_counter"), b("base:log"), b("base:planks"))
            {
                let (sx, sz) = (bx - 4, bz + 6);

                demo_set!(self.server.world, chart, sx, y + 1, sz, counter);
                for side in [-1i32, 1] {
                    demo_set!(self.server.world, chart, sx + side, y + 1, sz, log);
                    demo_set!(self.server.world, chart, sx + side, y + 2, sz, log);
                }
                for i in -1i32..=1 {
                    demo_set!(self.server.world, chart, sx + i, y + 3, sz, planks);
                }
                let mut st = crate::world::StallState {
                    owner: [7; 16],
                    owner_name: "MERI".to_string(),
                    ..Default::default()
                };
                if let (Some(salt), Some(silver)) = (
                    reg2.item_id("base:salted_meat"),
                    reg2.item_id("base:silver_ingot"),
                ) {
                    st.goods[0] = Some(ItemStack::new(&reg2, salt, 12));
                    st.price = Some(ItemStack::new(&reg2, silver, 1));
                }
                demo_insert!(
                    self.server.world,
                    chart,
                    (sx, y + 1, sz),
                    crate::world::BlockEntity::Stall(st)
                );
            }
            // A sign and a named waystone.
            if let Some(sign) = b("base:sign") {
                demo_set!(self.server.world, chart, bx, y + 1, bz + 6, sign);
                demo_insert!(
                    self.server.world,
                    chart,
                    (bx, y + 1, bz + 6),
                    crate::world::BlockEntity::Sign(crate::world::SignState {
                        lines: [
                            "SALT FAIR".to_string(),
                            "PRICES".to_string(),
                            "ASK MERI".to_string(),
                        ],
                    }),
                );
            }
            if let Some(ws) = b("base:waystone") {
                demo_set!(self.server.world, chart, bx + 3, y + 1, bz + 6, ws);
                demo_insert!(
                    self.server.world,
                    chart,
                    (bx + 3, y + 1, bz + 6),
                    crate::world::BlockEntity::Sign(crate::world::SignState {
                        lines: ["THREE PINES".to_string(), String::new(), String::new()],
                    }),
                );
            }
            // A saddlebagged deer at the hitching post.
            if let Some(di) = reg2.animal_id("base:deer") {
                let mut deer = demo_mob!(
                    chart,
                    di,
                    glam::Vec3::new(bx as f32 + 5.5, y as f32 + 1.0, bz as f32 + 7.5),
                    2.4,
                );
                deer.health = reg2.animals[di].health;
                deer.tamed = true;
                deer.calm = 100000.0;
                deer.cargo = Some(Default::default());
                self.server.world.spawn_mob(deer);
            }
            // A dug pond with a boat riding it.
            if let (Some(water), Some(dirt), Some(bi)) =
                (b("base:water"), b("base:dirt"), reg2.animal_id("base:boat"))
            {
                for dx in 6..=9i32 {
                    for dz in 0..=3i32 {
                        // A sealed bowl: solid under the water so the
                        // pond can't drain into a cave.
                        demo_set!(self.server.world, chart, bx + dx, y - 1, bz + dz, dirt);
                        demo_set!(self.server.world, chart, bx + dx, y, bz + dz, water);
                    }
                }
                let mut boat = demo_mob!(
                    chart,
                    bi,
                    glam::Vec3::new(bx as f32 + 7.5, y as f32 + 0.9, bz as f32 + 1.5),
                    0.8,
                );
                boat.health = reg2.animals[bi].health;
                boat.tamed = true;
                self.server.world.spawn_mob(boat);
            }
            // Screen shortcuts want the scene to exist first.
            match std::env::var("WILDFORGE_SCREEN").as_deref() {
                Ok("stall") => self.set_screen(Screen::Stall(chart.block(bx - 4, y + 1, bz + 6))),
                Ok("signedit") => {
                    self.ui_state.sign_lines =
                        ["SALT FAIR".to_string(), "PRICES".to_string(), String::new()];
                    self.ui_state.sign_line = 2;
                    self.set_screen(Screen::SignEdit(chart.block(bx, y + 1, bz + 6)));
                }
                Ok("mobcargo") => {
                    // Ids are sim-assigned: run one tick so the demo
                    // deer exists on the wire before we key by id.
                    let mut rng = 1u32;
                    let _ = self.server.world.tick_mobs(
                        &[crate::server::PlayerCtx {
                            id: 0,
                            pos: spawn,
                            spawn,
                            attackable: false,
                            aggro_mod: 0.0,
                        }],
                        1.0,
                        0.01,
                        &mut rng,
                    );
                    let id = self
                        .server
                        .world
                        .mobs()
                        .iter()
                        .find(|m| m.cargo.is_some() && m.id != 0)
                        .map(|m| m.id);
                    if let Some(id) = id {
                        // A little salt rides along for the screenshot.
                        if let Some(salt) = reg2.item_id("base:salt_crystal")
                            && let Some(m) = self.server.world.mob_by_id_mut(id)
                            && let Some(cargo) = m.cargo.as_mut()
                        {
                            cargo[0] = Some(ItemStack::new(&reg2, salt, 24));
                            cargo[5] = Some(ItemStack::new(&reg2, salt, 8));
                        }
                        for count in [24, 8] {
                            if let Some(salt) = reg2.item_id("base:salt_crystal")
                                && let Err(error) = self.server.world.record_external_stack(
                                    ItemStack::new(&reg2, salt, count),
                                    "development capture animal cargo",
                                )
                            {
                                eprintln!("materials: capture cargo source failed: {error}");
                            }
                        }
                        self.set_screen(Screen::MobCargo(id));
                    }
                }
                _ => {}
            }
        }
        // Dev: the wild arc in one frame — a smoking rack curing cuts
        // over a torch, and a watcher warden at the treeline.
        if std::env::var("WILDFORGE_DEMO_WILD").is_ok() {
            let b = |n: &str| self.content.reg.block_id(n);
            let reg2 = self.content.reg.clone();
            let bx = spawn.x as i32;
            let bz = spawn.z as i32;
            let y = demo_height!(self.server.world, chart, bx, bz);
            if let Some(grass) = b("base:grass") {
                for dx in -8..=8i32 {
                    for dz in -2..=16i32 {
                        let (x, z) = (bx + dx, bz + dz);
                        demo_set!(self.server.world, chart, x, y, z, grass);
                        for hh in 1..=6 {
                            if demo_get!(self.server.world, chart, x, y + hh, z) != AIR {
                                demo_set!(self.server.world, chart, x, y + hh, z, AIR);
                            }
                        }
                    }
                }
            }
            if let (Some(rack), Some(torch)) = (b("base:smoking_rack"), b("base:torch")) {
                demo_set!(self.server.world, chart, bx - 2, y + 1, bz + 4, torch);
                demo_set!(self.server.world, chart, bx - 2, y + 2, bz + 4, rack);
                let mut sm = crate::world::SmokerState::default();
                if let (Some(raw), Some(smoked)) = (
                    reg2.item_id("base:raw_venison"),
                    reg2.item_id("base:smoked_meat"),
                ) {
                    sm.meat[0] = Some(ItemStack::new(&reg2, raw, 1));
                    sm.meat[1] = Some(ItemStack::new(&reg2, smoked, 1));
                }
                demo_insert!(
                    self.server.world,
                    chart,
                    (bx - 2, y + 2, bz + 4),
                    crate::world::BlockEntity::Smoker(sm),
                );
            }
            // Aggrieved country and its watcher, mid-vigil. A torch
            // line marks the settlement's edge; the wild stands just
            // beyond it.
            for _ in 0..12 {
                demo_ire!(self.server.world, chart, bx, bz, 1.0);
            }
            if let Some(torch) = b("base:torch") {
                for dx in [0i32, 3, 6] {
                    demo_set!(self.server.world, chart, bx + dx, y + 1, bz + 11, torch);
                }
            }
            if let Some(ti) = reg2.animals.iter().position(|a| a.hostile) {
                let mut w = demo_mob!(
                    chart,
                    ti,
                    glam::Vec3::new(bx as f32 + 3.5, y as f32 + 1.0, bz as f32 + 13.5),
                    3.4,
                );
                w.health = reg2.animals[ti].health;
                w.watcher = true;
                w.watch_baseline = demo_standing!(self.server.world, chart, bx, bz);
                self.server.world.spawn_mob(w);
            }
        }
        if std::env::var("WILDFORGE_DEMO_HEART").is_ok() {
            // The three forms, alive/failing/dead, in a row — and one
            // ruined site with its ground raised ready for a seed.
            let b = |n: &str| self.content.reg.block_id(n);
            let bx = spawn.x as i32;
            let bz = spawn.z as i32;
            let y = demo_height!(self.server.world, chart, bx, bz);
            eprintln!("heart demo anchored at ({bx},{y},{bz})");
            if let Some(grass) = b("base:grass") {
                let w = &mut self.server.world;
                for dx in -18..=18i32 {
                    for dz in -20..=14i32 {
                        let (x, z) = (bx + dx, bz + dz);
                        demo_set!(w, chart, x, y, z, grass);
                        for hh in 1..=10 {
                            if demo_get!(w, chart, x, y + hh, z) != AIR {
                                demo_set!(w, chart, x, y + hh, z, AIR);
                            }
                        }
                    }
                }
            }
            let w = &mut self.server.world;
            // One of each shape, each from a different country, so a
            // capture shows the bole, the spring and the stone at all
            // three stages.
            for (col, biome) in [
                (-10i32, crate::worldgen::Biome::Jungle),
                (0, crate::worldgen::Biome::Savanna),
                (10, crate::worldgen::Biome::Arctic),
            ] {
                let form = crate::world::heart_form(biome);
                for (row, stage) in [(-4i32, 2u8), (2, 1), (8, 0)] {
                    let name = crate::world::heart_block_name(form, stage);
                    let Some(block) = b(&name) else { continue };
                    let tall = crate::world::heart_height(form);
                    for dy in 1..=tall {
                        demo_set!(w, chart, bx + col, y + dy, bz + row, block);
                    }
                }
            }
            // Ground made ready around the dead stone: the long walk's
            // last step, waiting on a seed.
            if let Some(farm) = b("base:farmland") {
                for dx in -4..=4i32 {
                    for dz in -4..=4i32 {
                        if dx * dx + dz * dz > 16 {
                            continue;
                        }
                        demo_meta!(
                            w,
                            chart,
                            bx + 10 + dx,
                            y,
                            bz + 8 + dz,
                            farm,
                            crate::world::soil::soil_meta(48, 0),
                        );
                    }
                }
            }
        }
        if std::env::var("WILDFORGE_DEMO_ECO").is_ok() {
            // The living-soil field: four fertility bands, palest dust
            // to deepest loam, wheat standing on the two rich bands —
            // the tint gradient is the shot.
            let b = |n: &str| self.content.reg.block_id(n);
            let bx = spawn.x as i32;
            let bz = spawn.z as i32;
            let y = demo_height!(self.server.world, chart, bx, bz);
            eprintln!("eco demo anchored at ({bx},{y},{bz})");
            if let (Some(grass), Some(farm)) = (b("base:grass"), b("base:farmland")) {
                let w = &mut self.server.world;
                for dx in -8..=8i32 {
                    for dz in -2..=12i32 {
                        let (x, z) = (bx + dx, bz + dz);
                        demo_set!(w, chart, x, y, z, grass);
                        for hh in 1..=8 {
                            if demo_get!(w, chart, x, y + hh, z) != AIR {
                                demo_set!(w, chart, x, y + hh, z, AIR);
                            }
                        }
                    }
                }
                // Bands run away from the camera, three columns each.
                for (band, fert) in [(0i32, 8u8), (1, 24), (2, 40), (3, 60)] {
                    for dx in 0..3i32 {
                        for dz in 2..=9i32 {
                            let x = bx - 6 + band * 3 + dx;
                            demo_meta!(
                                w,
                                chart,
                                x,
                                y,
                                bz + dz,
                                farm,
                                crate::world::soil::soil_meta(fert, 0),
                            );
                        }
                    }
                }
                if let Some(ripe) = b("base:wheat_seeds/stage2") {
                    for band in [2i32, 3] {
                        for dx in 0..3i32 {
                            for dz in 2..=9i32 {
                                if (dx + dz) % 2 == 0 {
                                    let x = bx - 6 + band * 3 + dx;
                                    demo_set!(w, chart, x, y + 1, bz + dz, ripe);
                                }
                            }
                        }
                    }
                }
                // The belly's corner: a compost heap pair by the field
                // and a plank pen with two deer (their dung on the
                // ground), plus one hungry doe loose by the dust band
                // — the raid, caught walking.
                if let (Some(heap), Some(ready)) =
                    (b("base:compost_heap"), b("base:compost_heap_ready"))
                {
                    demo_meta!(w, chart, bx + 7, y + 1, bz + 2, heap, 6);
                    demo_set!(w, chart, bx + 7, y + 1, bz + 4, ready);
                }
                if let Some(plank) = b("base:planks") {
                    for dx in -8..=-4i32 {
                        for dz in 10..=13i32 {
                            if dx == -8 || dx == -4 || dz == 10 || dz == 13 {
                                demo_set!(w, chart, bx + dx, y + 1, bz + dz, plank);
                            }
                        }
                    }
                }
            }
            let reg2 = self.content.reg.clone();
            if let Some(si) = reg2.animal_id("base:deer") {
                let w = &mut self.server.world;
                // A built demo is tended country: calm animals mind
                // the pen walls here, as they would around any base.
                for cx in -2..=2i32 {
                    for cz in -2..=2i32 {
                        let cp = chart.chunk(bx + cx * 16, bz + cz * 16);
                        w.player_touched.insert(cp);
                    }
                }
                for (dx, dz, hungry) in [
                    (-6.5f32, 11.5f32, false),
                    (-5.5, 12.5, false),
                    (7.5, 11.5, true),
                ] {
                    let mut m = demo_mob!(
                        chart,
                        si,
                        glam::Vec3::new(bx as f32 + dx, y as f32 + 1.0, bz as f32 + dz),
                        2.0,
                    );
                    m.health = reg2.animals[si].health;
                    m.tamed = !hungry;
                    if hungry {
                        m.belly = -1.0;
                    }
                    w.spawn_mob(m);
                }
                if let Some(dung) = reg2.item_id("base:dung") {
                    let stack = ItemStack::new(&reg2, dung, 1);
                    demo_drop!(w, chart, (bx - 7, y + 1, bz + 11), stack);
                }
                // The pond: dug two deep, sealed in stone, water to
                // the brim — cattails on the bank, a lily on the
                // glass, a trout below and the heron above it.
                if let (Some(stone), Some(reeds), Some(lily)) =
                    (b("base:stone"), b("base:cattail"), b("base:water_lily"))
                {
                    let water = reg2.water_block(0);
                    for dx in 3..=7i32 {
                        for dz in 10..=13i32 {
                            let (x, z) = (bx + dx, bz + dz);
                            let rim = dx == 3 || dx == 7 || dz == 10 || dz == 13;
                            for dy in [-2i32, -1] {
                                demo_set!(w, chart, x, y + dy, z, if rim { stone } else { water });
                            }
                            demo_set!(w, chart, x, y - 3, z, stone);
                            if demo_get!(w, chart, x, y, z) != AIR {
                                demo_set!(w, chart, x, y, z, AIR);
                            }
                        }
                    }
                    demo_set!(w, chart, bx + 3, y, bz + 10, reeds);
                    demo_set!(w, chart, bx + 7, y, bz + 13, reeds);
                    // The pad floats on the water surface: the first
                    // air cell above the fill.
                    demo_set!(w, chart, bx + 5, y, bz + 12, lily);
                    for (name, dx, dz, dy) in [
                        ("base:trout", 5.5f32, 11.5f32, -1.6f32),
                        ("base:heron", 5.5, 11.5, 1.0),
                    ] {
                        if let Some(si) = reg2.animal_id(name) {
                            let mut m = demo_mob!(
                                chart,
                                si,
                                glam::Vec3::new(bx as f32 + dx, y as f32 + dy, bz as f32 + dz),
                                1.2,
                            );
                            m.health = reg2.animals[si].health;
                            m.belly = 9000.0;
                            w.spawn_mob(m);
                        }
                    }
                }
                // The storm's aftermath, staged a week on: char scars
                // in the grass, flowers erupting around them, saplings
                // on the march — wrath and renewal as one event.
                if let (Some(charred), Some(mb), Some(ep), Some(sap)) = (
                    b("base:charred_soil"),
                    b("base:meadow_bloom"),
                    b("base:ember_poppy"),
                    b("base:oak_sapling"),
                ) {
                    let (ax, az) = (bx - 6, bz + 15);
                    for dx in 0..8i32 {
                        for dz in 0..6i32 {
                            let (x, z) = (ax + dx, az + dz);
                            let g = b("base:grass").unwrap();
                            demo_set!(w, chart, x, y, z, g);
                            for hh in 1..=4 {
                                if demo_get!(w, chart, x, y + hh, z) != AIR {
                                    demo_set!(w, chart, x, y + hh, z, AIR);
                                }
                            }
                            let roll = (dx * 7 + dz * 13) % 17;
                            match roll {
                                0 | 8 => {
                                    demo_set!(w, chart, x, y, z, charred);
                                }
                                2 | 9 | 14 => {
                                    demo_set!(w, chart, x, y + 1, z, mb);
                                }
                                4 | 11 => {
                                    demo_set!(w, chart, x, y + 1, z, ep);
                                }
                                6 => {
                                    demo_set!(w, chart, x, y + 1, z, sap);
                                }
                                _ => {}
                            }
                        }
                    }
                    demo_bloom!(w, chart, ax, az, 3.0);
                }
                // The grotto: a hollow cut under the pad's east edge,
                // lantern fungus glowing inside, a bat at roost and
                // its guano on the floor — the cave's corner of the
                // tour, no cave required.
                if let (Some(stone), Some(lf)) = (b("base:stone"), b("base:lantern_fungus")) {
                    for dx in 8..=12i32 {
                        for dz in -2..=2i32 {
                            for dy in -4..=-1i32 {
                                let (x, yy, z) = (bx + dx, y + dy, bz + dz);
                                let shell = dx == 12 || dz == -2 || dz == 2 || dy == -4;
                                // Open face toward the west (the pad).
                                if dx == 8 && dy >= -3 {
                                    if demo_get!(w, chart, x, yy, z) != AIR {
                                        demo_set!(w, chart, x, yy, z, AIR);
                                    }
                                    continue;
                                }
                                demo_set!(w, chart, x, yy, z, if shell { stone } else { AIR });
                            }
                        }
                    }
                    demo_set!(w, chart, bx + 11, y - 3, bz, lf);
                    demo_set!(w, chart, bx + 10, y - 3, bz + 1, lf);
                    if let Some(si) = reg2.animal_id("base:bat") {
                        let mut m = demo_mob!(
                            chart,
                            si,
                            glam::Vec3::new(bx as f32 + 10.5, y as f32 - 2.5, bz as f32 - 0.5),
                            0.0,
                        );
                        m.health = reg2.animals[si].health;
                        w.spawn_mob(m);
                    }
                    if let Some(g) = reg2.item_id("base:guano") {
                        let stack = ItemStack::new(&reg2, g, 2);
                        demo_drop!(w, chart, (bx + 10, y - 3, bz), stack);
                    }
                }
                // The neighbors: the wider roster lined up along the
                // north strip for the camera.
                for (i, name) in [
                    "base:bison",
                    "base:camel",
                    "base:musk_ox",
                    "base:antelope",
                    "base:mouflon",
                    "base:pheasant",
                    "base:duck",
                ]
                .iter()
                .enumerate()
                {
                    if let Some(si) = reg2.animal_id(name) {
                        let mut m = demo_mob!(
                            chart,
                            si,
                            glam::Vec3::new(
                                bx as f32 - 7.0 + i as f32 * 2.2,
                                y as f32 + 1.0,
                                bz as f32 - 1.0,
                            ),
                            std::f32::consts::PI,
                        );
                        m.health = reg2.animals[si].health;
                        m.tamed = true;
                        m.belly = 9000.0;
                        w.spawn_mob(m);
                    }
                }
                // Fang and carrion: a fox on stand by the field, and
                // the vultures' table set east of the pen.
                for (name, dx, dz) in [
                    ("base:fox", 8.5f32, 6.5f32),
                    ("base:carcass", -2.5, 13.5),
                    ("base:vulture", -2.5, 13.5),
                ] {
                    if let Some(si) = reg2.animal_id(name) {
                        let mut m = demo_mob!(
                            chart,
                            si,
                            glam::Vec3::new(bx as f32 + dx, y as f32 + 1.0, bz as f32 + dz),
                            3.6,
                        );
                        m.health = reg2.animals[si].health;
                        m.belly = 9000.0; // props don't eat the props
                        if name == "base:carcass" {
                            m.rot = 9000.0;
                        }
                        w.spawn_mob(m);
                    }
                }
            }
        }
        if std::env::var("WILDFORGE_DEMO_MILL").is_ok() {
            // A working millrace: an elevated pool spilling over a
            // lip, the wheel in the fall, gears walking the power
            // down to a millstone, a sawmill, and a helve hammer at
            // its anvil — plus a sail tower for the wind shot.
            let b = |n: &str| self.content.reg.block_id(n);
            let reg2 = self.content.reg.clone();
            let bx = spawn.x as i32;
            let bz = spawn.z as i32;
            let y = demo_height!(self.server.world, chart, bx, bz);
            eprintln!("mill demo anchored at ({bx},{y},{bz})");
            if let Some(grass) = b("base:grass") {
                for dx in -10..=10i32 {
                    for dz in -2..=14i32 {
                        let (x, z) = (bx + dx, bz + dz);
                        demo_set!(self.server.world, chart, x, y, z, grass);
                        for hh in 1..=10 {
                            if demo_get!(self.server.world, chart, x, y + hh, z) != AIR {
                                demo_set!(self.server.world, chart, x, y + hh, z, AIR);
                            }
                        }
                    }
                }
            }
            let (mx, mz) = (bx - 3, bz + 5);
            if let (Some(stone), Some(shaft), Some(gear)) =
                (b("base:stone"), b("base:shaft"), b("base:gear"))
            {
                let w = &mut self.server.world;
                // The raised race runs north-south so the wheel's
                // face greets a camera looking east: stone trough,
                // water pouring out the south lip under the wheel.
                for dz in 1..=8i32 {
                    for dy in 1..=2 {
                        demo_set!(w, chart, mx, y + dy, mz + dz, stone);
                    }
                    demo_set!(w, chart, mx - 1, y + 3, mz + dz, stone);
                    demo_set!(w, chart, mx + 1, y + 3, mz + dz, stone);
                }
                demo_set!(w, chart, mx, y + 3, mz + 9, stone);
                // The wall opens at the south end, downstream of the
                // wheel: the race spills there without starving the
                // cells the wheel actually rides.
                demo_set!(w, chart, mx - 1, y + 3, mz + 1, AIR);
                let water = reg2.water_block(0);
                for dz in 1..=8i32 {
                    demo_set!(w, chart, mx, y + 3, mz + dz, water);
                }
                // The wheel rides mid-race, axle running east — its
                // stream below it, its face to the camera.
                if let Some(wheel) = b("base:water_wheel") {
                    demo_set!(w, chart, mx, y + 4, mz + 3, wheel);
                    demo_insert!(
                        w,
                        chart,
                        (mx, y + 4, mz + 3),
                        crate::world::BlockEntity::Anvil(Default::default()),
                    );
                }
                // The axle line runs four clear blocks off the hub
                // before its down post, so the wheel's face stands
                // alone; stations rank along the working floor.
                for i in 1..=4 {
                    demo_set!(w, chart, mx + i, y + 4, mz + 3, shaft);
                }
                demo_set!(w, chart, mx + 5, y + 4, mz + 3, gear);
                demo_set!(w, chart, mx + 5, y + 3, mz + 3, shaft);
                demo_set!(w, chart, mx + 5, y + 2, mz + 3, shaft);
                demo_set!(w, chart, mx + 5, y + 1, mz + 3, gear);
                demo_set!(w, chart, mx + 6, y + 1, mz + 3, shaft);
                demo_set!(w, chart, mx + 7, y + 1, mz + 3, gear);
                demo_set!(w, chart, mx + 8, y + 1, mz + 3, shaft);
                demo_set!(w, chart, mx + 9, y + 1, mz + 3, gear);
                // Stations step south off their gears, facing camera.
                if let Some(mill) = b("base:millstone") {
                    demo_set!(w, chart, mx + 7, y + 1, mz + 2, mill);
                    if let Some(copper) = reg2.item_id("base:raw_copper") {
                        for _ in 0..4 {
                            demo_anvil_put!(
                                w,
                                chart,
                                (mx + 7, y + 1, mz + 2),
                                ItemStack::new(&reg2, copper, 1),
                            );
                        }
                    }
                }
                if let Some(saw) = b("base:sawmill") {
                    demo_set!(w, chart, mx + 9, y + 1, mz + 2, saw);
                    if let Some(log) = reg2.item_id("base:log") {
                        for _ in 0..3 {
                            demo_anvil_put!(
                                w,
                                chart,
                                (mx + 9, y + 1, mz + 2),
                                ItemStack::new(&reg2, log, 1),
                            );
                        }
                    }
                }
                if let (Some(helve), Some(anvil)) = (b("base:helve_hammer"), b("base:stone_anvil"))
                {
                    demo_set!(w, chart, mx + 9, y + 1, mz + 4, helve);
                    demo_set!(w, chart, mx + 9, y + 1, mz + 5, anvil);
                    if let Some(bl) = reg2.item_id("base:steel_bloom") {
                        demo_anvil_put!(
                            w,
                            chart,
                            (mx + 9, y + 1, mz + 5),
                            ItemStack::new(&reg2, bl, 1),
                        );
                    }
                }
                // The machine shop row: crude lathe west, iron lathe
                // east, the vice that lets precision cut at all.
                demo_set!(w, chart, mx + 5, y + 1, mz + 2, shaft);
                demo_set!(w, chart, mx + 5, y + 1, mz + 1, gear);
                if let (Some(lathe), Some(ilathe), Some(vice)) =
                    (b("base:lathe"), b("base:iron_lathe"), b("base:vice"))
                {
                    demo_set!(w, chart, mx + 4, y + 1, mz + 1, lathe);
                    demo_set!(w, chart, mx + 6, y + 1, mz + 1, ilathe);
                    demo_set!(w, chart, mx + 5, y + 1, mz, vice);
                    if let Some(cu) = reg2.item_id("base:copper_ingot") {
                        demo_anvil_put!(
                            w,
                            chart,
                            (mx + 4, y + 1, mz + 1),
                            ItemStack::new(&reg2, cu, 1),
                        );
                    }
                    if let Some(fe) = reg2.item_id("base:iron_ingot") {
                        demo_anvil_put!(
                            w,
                            chart,
                            (mx + 6, y + 1, mz + 1),
                            ItemStack::new(&reg2, fe, 1),
                        );
                    }
                }
                // The electric age: a generator off the shop gear,
                // arc lamps drinking its field, the steam corner,
                // and a separator on its firebrick stack.
                if let Some(dynamo) = b("base:generator") {
                    demo_set!(w, chart, mx + 7, y + 1, mz + 4, dynamo);
                    demo_insert!(
                        w,
                        chart,
                        (mx + 7, y + 1, mz + 4),
                        crate::world::BlockEntity::Anvil(Default::default()),
                    );
                }
                for (lamp, lx, lz) in [
                    ("base:arc_lamp", mx + 5, mz + 6),
                    ("base:blue_arc_lamp", mx + 7, mz + 6),
                    ("base:red_arc_lamp", mx + 9, mz + 6),
                ] {
                    if let Some(l) = b(lamp) {
                        demo_set!(w, chart, lx, y + 2, lz, l);
                        demo_set!(w, chart, lx, y + 1, lz, stone);
                    }
                }
                if let (Some(fbx), Some(boiler), Some(engine)) =
                    (b("base:firebox"), b("base:boiler"), b("base:steam_engine"))
                {
                    let (ex, ez) = (mx + 12, mz + 1);
                    demo_set!(w, chart, ex, y + 1, ez, fbx);
                    demo_set!(w, chart, ex, y + 2, ez, boiler);
                    demo_set!(w, chart, ex + 1, y + 2, ez, engine);
                    demo_insert!(
                        w,
                        chart,
                        (ex, y + 1, ez),
                        crate::world::BlockEntity::Steam(crate::world::SteamState {
                            fuel: 900.0,
                            water: crate::planet_atlas::ReservoirMass::fresh(
                                60 * crate::planet_atlas::HYDRO_UNITS_PER_BLOCK,
                            ),
                            steam_numerator_remainder: 0,
                        }),
                    );
                }
                if let (Some(fb), Some(sep)) = (b("base:firebrick"), b("base:separator")) {
                    let (px, pz) = (bx - 8, bz + 1);
                    for ly in 1..=3 {
                        for rx in -1..=1i32 {
                            for rz in -1..=1i32 {
                                if rx == 0 && rz == 0 {
                                    continue;
                                }
                                demo_set!(w, chart, px + 1 + rx, y + ly, pz + rz, fb);
                            }
                        }
                    }
                    demo_set!(w, chart, px, y + 1, pz, sep);
                    demo_insert!(
                        w,
                        chart,
                        (px, y + 1, pz),
                        crate::world::BlockEntity::Separator(crate::world::SeparatorState {
                            powder: 4,
                            fuel: 4,
                            ..Default::default()
                        }),
                    );
                }
                // Boring mill and pump join the shop floor.
                if let (Some(bore), Some(pump)) = (b("base:boring_mill"), b("base:pump")) {
                    demo_set!(w, chart, mx + 2, y + 1, mz + 1, bore);
                    demo_set!(w, chart, mx + 2, y + 1, mz, pump);
                    demo_insert!(
                        w,
                        chart,
                        (mx + 2, y + 1, mz),
                        crate::world::BlockEntity::Anvil(Default::default()),
                    );
                }
                // The sail tower: altitude is the windmill's river.
                if let Some(sail) = b("base:windmill_sail") {
                    let tx = bx + 8;
                    for ty in (y + 1)..=91 {
                        demo_set!(w, chart, tx, ty, bz + 10, stone);
                    }
                    demo_set!(w, chart, tx, 92, bz + 10, sail);
                    demo_insert!(
                        w,
                        chart,
                        (tx, 92, bz + 10),
                        crate::world::BlockEntity::Anvil(Default::default()),
                    );
                    // And one at eye level for the mesh to be judged
                    // (too low to ever turn; that's the point).
                    demo_set!(w, chart, bx + 6, y + 2, bz + 1, stone);
                    demo_set!(w, chart, bx + 6, y + 3, bz + 1, sail);
                }
            }
        }
        if std::env::var("WILDFORGE_DEMO_CAMP").is_ok() {
            let b = |n: &str| self.content.reg.block_id(n);
            let bx = spawn.x as i32;
            let bz = spawn.z as i32;
            for dx in [-8i32, 0, 8] {
                for dz in [-8i32, 0, 8] {
                    self.server
                        .world
                        .ensure_chunk(chart.chunk(bx + dx, bz + dz));
                }
            }
            if let (Some(log), Some(torch)) = (b("base:log"), b("base:torch")) {
                // A clearing: no trunks photobombing the campfire.
                for dx in -6..=6i32 {
                    for dz in -1..=12i32 {
                        let (x, z) = (bx + dx, bz + dz);
                        let y = demo_height!(self.server.world, chart, x, z);
                        for h in 1..=9 {
                            if demo_get!(self.server.world, chart, x, y + h, z) != AIR {
                                demo_set!(self.server.world, chart, x, y + h, z, AIR);
                            }
                        }
                    }
                }
                // Torch posts: a 2-log stake with the flame on top.
                for (px, pz) in [(4i32, 4i32), (-4, 6), (0, 10)] {
                    let (x, z) = (bx + px, bz + pz);
                    let y = demo_height!(self.server.world, chart, x, z);
                    demo_set!(self.server.world, chart, x, y + 1, z, log);
                    demo_set!(self.server.world, chart, x, y + 2, z, log);
                    demo_set!(self.server.world, chart, x, y + 3, z, torch);
                }
            }
            for (name, px, pz) in [("base:chest", 2i32, 7i32), ("base:stone_anvil", -2, 4)] {
                if let Some(blk) = b(name) {
                    let (x, z) = (bx + px, bz + pz);
                    let y = demo_height!(self.server.world, chart, x, z);
                    demo_set!(self.server.world, chart, x, y + 1, z, blk);
                }
            }
            let reg = self.content.reg.clone();
            if let Some(t) = reg.item_id("base:torch") {
                self.give_dev_item(&reg, t, 5);
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
                        .ensure_chunk(chart.chunk(bx + dx, bz + dz));
                }
            }
            let yf = (-7..=7)
                .flat_map(|dx| (-7..=7).map(move |dz| (dx, dz)))
                .map(|(dx, dz)| demo_height!(self.server.world, chart, bx + dx, bz + dz))
                .max()
                .unwrap_or(spawn.y as i32);
            for dx in -7..=7i32 {
                for dz in -7..=7i32 {
                    demo_set!(self.server.world, chart, bx + dx, yf, bz + dz, stone);
                    let wall = dx.abs() == 7 || dz.abs() == 7;
                    for h in 1..=8 {
                        let b = if (wall && h <= 3) || h == 4 {
                            stone
                        } else {
                            AIR
                        };
                        let b = if h > 4 { AIR } else { b };
                        demo_set!(self.server.world, chart, bx + dx, yf + h, bz + dz, b);
                    }
                }
            }
            for px in [-3i32, 3] {
                for h in 1..=3 {
                    demo_set!(self.server.world, chart, bx + px, yf + h, bz + 3, stone);
                }
            }
            for (tx, tz) in [(-6i32, -6i32), (6, -6), (0, 6)] {
                demo_set!(self.server.world, chart, bx + tx, yf + 1, bz + tz, torch);
            }
            // A red-glazed alcove: torch sealed behind a stained pane —
            // its pool outside should come out the color of the glass.
            if let Some(rg) = self.content.reg.block_id("base:red_glass") {
                let (ax, az) = (bx + 4, bz - 4);
                demo_set!(self.server.world, chart, ax, yf + 1, az, stone);
                demo_set!(self.server.world, chart, ax, yf + 2, az, torch);
                demo_set!(self.server.world, chart, ax, yf + 3, az, stone);
                demo_set!(self.server.world, chart, ax - 1, yf + 2, az, stone);
                demo_set!(self.server.world, chart, ax + 1, yf + 2, az, stone);
                demo_set!(self.server.world, chart, ax, yf + 2, az - 1, stone);
                demo_set!(self.server.world, chart, ax, yf + 2, az + 1, rg);
            }
            // Stand in the room, whatever the terrain wanted.
            let inside = Vec3::new(bx as f32 + 0.5, yf as f32 + 1.2, bz as f32 + 0.5);
            self.player.pos = self.player.pos.relocated_local(inside).unwrap();
            self.survival.spawn_point = self.player.pos;
            self.camera.follow_planet(self.player.eye());
            // A torch in slot 0: WILDFORGE_SEL=0 holds it (held-light
            // shots), WILDFORGE_SEL=8 keeps the hand empty.
            let reg = self.content.reg.clone();
            if let Some(t) = reg.item_id("base:torch") {
                self.give_dev_item(&reg, t, 5);
            }
        }

        // Dev: a flat ice rink plus a low kerb for parallax verification.
        if std::env::var("WILDFORGE_DEMO_ICE").is_ok()
            && let Some(ice) = self.content.reg.block_id("base:ice")
        {
            let bx = spawn.x as i32;
            let bz = spawn.z as i32;
            for dx in [-8i32, 0, 8] {
                for dz in [-8i32, 0, 8] {
                    self.server
                        .world
                        .ensure_chunk(chart.chunk(bx + dx, bz + dz));
                }
            }
            let yf = (-10..=10)
                .flat_map(|dx| (-10..=10).map(move |dz| (dx, dz)))
                .map(|(dx, dz)| demo_height!(self.server.world, chart, bx + dx, bz + dz))
                .max()
                .unwrap_or(spawn.y as i32);
            for dx in -10..=10i32 {
                for dz in -10..=10i32 {
                    demo_set!(self.server.world, chart, bx + dx, yf, bz + dz, ice);
                }
                demo_set!(self.server.world, chart, bx + dx, yf + 1, bz + 10, ice);
                demo_set!(self.server.world, chart, bx + dx, yf + 2, bz + 10, ice);
            }
            let strafe: f32 = std::env::var("WILDFORGE_DEMO_STRAFE")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(0.0);
            let stand = Vec3::new(bx as f32 + 0.5 + strafe, yf as f32 + 1.0, bz as f32 - 9.0);
            self.player.pos = self.player.pos.relocated_local(stand).unwrap();
            self.survival.spawn_point = self.player.pos;
            self.camera.follow_planet(self.player.eye());
            self.camera.yaw = std::f32::consts::FRAC_PI_2;
            self.camera.pitch = -0.35;
        }

        // Dev: a dark chamber with a glowglass window and a torch behind
        // it — emissive glass plus the transmission tint in one frame.
        if std::env::var("WILDFORGE_DEMO_GLOWGLASS").is_ok()
            && let Some(glow) = self.content.reg.block_id("base:glow_glass")
        {
            let stone = self.content.reg.block_id("base:stone").unwrap_or(AIR);
            let torch = self.content.reg.block_id("base:torch").unwrap_or(AIR);
            let (bx, bz) = (spawn.x as i32, spawn.z as i32 + 8);
            for dx in [-8i32, 0, 8] {
                for dz in [-8i32, 0, 8] {
                    self.server
                        .world
                        .ensure_chunk(chart.chunk(bx + dx, bz + dz));
                }
            }
            let yf = (-5..=5)
                .flat_map(|dx| (-4..=4).map(move |dz| (dx, dz)))
                .map(|(dx, dz)| demo_height!(self.server.world, chart, bx + dx, bz + dz))
                .max()
                .unwrap_or(spawn.y as i32);
            for dx in -4..=4i32 {
                for dz in -3..=3i32 {
                    for dy in 0..=4i32 {
                        let edge = dx.abs() == 4 || dz.abs() == 3 || dy == 0 || dy == 4;
                        let b = if edge { stone } else { AIR };
                        demo_set!(self.server.world, chart, bx + dx, yf + 1 + dy, bz + dz, b);
                    }
                }
            }
            // The window in the far wall, glowing green.
            for dx in -2..=2i32 {
                for dy in 2..=3i32 {
                    demo_set!(self.server.world, chart, bx + dx, yf + 1 + dy, bz + 3, glow);
                }
            }
            // A torch on the outside sill: its beam crosses the pane.
            demo_set!(self.server.world, chart, bx, yf + 2, bz + 5, torch);
            let stand = Vec3::new(bx as f32 + 0.5, yf as f32 + 1.2, bz as f32 - 1.5);
            self.player.pos = self.player.pos.relocated_local(stand).unwrap();
            self.survival.spawn_point = self.player.pos;
            self.camera.follow_planet(self.player.eye());
            self.camera.yaw = std::f32::consts::FRAC_PI_2;
            self.camera.pitch = 0.05;
        }

        // Dev: a stone wall lit from the side for authored-normal verification.
        if std::env::var("WILDFORGE_DEMO_ROCK").is_ok()
            && let Some(stone) = self.content.reg.block_id("base:stone")
        {
            let bx = spawn.x as i32;
            let bz = spawn.z as i32;
            for dx in [-8i32, 0, 8] {
                for dz in [-8i32, 0, 8] {
                    self.server
                        .world
                        .ensure_chunk(chart.chunk(bx + dx, bz + dz));
                }
            }
            let yf = (-10..=10)
                .flat_map(|dx| (-10..=10).map(move |dz| (dx, dz)))
                .map(|(dx, dz)| demo_height!(self.server.world, chart, bx + dx, bz + dz))
                .max()
                .unwrap_or(spawn.y as i32);
            for dx in -10..=10i32 {
                for dz in -10..=10i32 {
                    demo_set!(self.server.world, chart, bx + dx, yf, bz + dz, stone);
                }
                for dy in 1..=6i32 {
                    demo_set!(self.server.world, chart, bx + dx, yf + dy, bz - 8, stone);
                }
            }
            for dy in 1..=3i32 {
                demo_set!(self.server.world, chart, bx + 3, yf + dy, bz - 4, stone);
            }
            let dist: f32 = std::env::var("WILDFORGE_DEMO_DIST")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(12.0);
            let stand = Vec3::new(bx as f32 + 0.5, yf as f32 + 1.0, bz as f32 - 7.0 + dist);
            self.player.pos = self.player.pos.relocated_local(stand).unwrap();
            self.survival.spawn_point = self.player.pos;
            self.camera.follow_planet(self.player.eye());
            self.camera.yaw = -std::f32::consts::FRAC_PI_2;
            self.camera.pitch = -0.10;
        }

        // Dev: a sub-voxel surface-sand dune for octant substrate checks.
        if std::env::var("WILDFORGE_DEMO_CORNER").is_ok()
            && let Some(stone) = self.content.reg.block_id("base:cobblestone")
        {
            let bx = spawn.x as i32;
            let bz = spawn.z as i32 + 6;
            let y = demo_height!(self.server.world, chart, bx, bz);
            // Carve a clean flat arena: cobblestone floor, air above, so
            // grass and trees don't intrude on the shadow.
            for dx in -11..=11 {
                for dz in -9..=15 {
                    demo_set!(self.server.world, chart, bx + dx, y, bz + dz, stone);
                    for h in 1..=9 {
                        demo_set!(self.server.world, chart, bx + dx, y + h, bz + dz, AIR);
                    }
                }
            }
            // Wall across X at z=bz, 5 tall, with a 1-wide doorway at bx.
            for dx in -11..=11 {
                if dx == 0 {
                    continue;
                }
                for h in 1..=5 {
                    demo_set!(self.server.world, chart, bx + dx, y + h, bz, stone);
                }
            }
            // Warm light on the far side of the wall — it blares through the
            // doorway and lights the far room, leaving the near side dark.
            self.presentation.demo_lights = vec![lights::DynLight {
                key: lights::Key::Demo(0),
                pos: chart
                    .entity(Vec3::new(
                        bx as f32 + 0.5,
                        (y + 2) as f32 + 0.5,
                        bz as f32 + 5.5,
                    ))
                    .render_pos(),
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
            let y = demo_height!(self.server.world, chart, cx, cz);
            for dx in -8..=8 {
                for dz in -8..=8 {
                    demo_set!(self.server.world, chart, cx + dx, y, cz + dz, water);
                }
            }
        }
        // Dev: a warm torch and a red ruby block side by side (colored-light
        // verification — pools of warm and red that blend where they meet).
        if std::env::var("WILDFORGE_DEMO_COLORLIGHT").is_ok() {
            let place = |w: &mut World, name: &str, dx: i32, dz: i32| {
                if let Some(b) = w.reg.block_id(name) {
                    let (x, z) = (spawn.x as i32 + dx, spawn.z as i32 + dz);
                    let y = demo_height!(w, chart, x, z);
                    demo_set!(w, chart, x, y + 1, z, b);
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
                let base = demo_height!(self.server.world, chart, x, z);
                for i in 1..=h {
                    demo_set!(self.server.world, chart, x, base + i, z, stone);
                }
            }
        }
        // Dev: a ready steelworks near spawn (bloomery shell + anvil +
        // materials) for screenshots and hands-on QA.
        if std::env::var("WILDFORGE_DEMO_STEELWORKS").is_ok() {
            let b = |n: &str| self.content.reg.block_id(n);
            if let (Some(fb), Some(mouth), Some(anvil), Some(floor)) = (
                b("base:firebrick"),
                b("base:bloomery"),
                b("base:stone_anvil"),
                b("base:cobblestone"),
            ) {
                let (sx, sz) = (spawn.x as i32 + 6, spawn.z as i32 + 4);
                // Build the fixture from the player's actual ground plane.
                // `surface_height` includes leaves, which used to perch a
                // bloomery on a tree canopy on forested seeds. A thick,
                // cleared terrace is deterministic on coasts, hills, and
                // wooded starts alike.
                let floor_y = spawn.y.floor() as i32 - 1;
                for x in (spawn.x as i32 + 1)..=(spawn.x as i32 + 9) {
                    for z in (spawn.z as i32 + 1)..=(spawn.z as i32 + 15) {
                        for y in (floor_y - 2)..=floor_y {
                            demo_set!(self.server.world, chart, x, y, z, floor);
                        }
                        for y in (floor_y + 1)..=(floor_y + 14) {
                            demo_set!(self.server.world, chart, x, y, z, AIR);
                        }
                    }
                }
                let sy = floor_y + 1;
                // Core at (sx, sy, sz); mouth on its -X side.
                for ly in 0..3 {
                    for rx in -1..=1i32 {
                        for rz in -1..=1i32 {
                            if rx == 0 && rz == 0 {
                                continue;
                            }
                            demo_set!(self.server.world, chart, sx + rx, sy + ly, sz + rz, fb);
                        }
                    }
                    demo_set!(
                        self.server.world,
                        chart,
                        sx,
                        sy + ly,
                        sz,
                        crate::registry::AIR
                    );
                }
                demo_set!(self.server.world, chart, sx - 1, sy, sz, mouth);
                demo_set!(self.server.world, chart, sx - 3, sy, sz + 2, anvil);
                // A second stack, already charged and burning.
                let (lx, lz) = (sx, sz + 8);
                let ly = floor_y + 1;
                for dy in 0..3 {
                    for rx in -1..=1i32 {
                        for rz in -1..=1i32 {
                            if rx == 0 && rz == 0 {
                                continue;
                            }
                            demo_set!(self.server.world, chart, lx + rx, ly + dy, lz + rz, fb);
                        }
                    }
                    demo_set!(
                        self.server.world,
                        chart,
                        lx,
                        ly + dy,
                        lz,
                        crate::registry::AIR
                    );
                }
                demo_set!(self.server.world, chart, lx - 1, ly, lz, mouth);
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
                    demo_insert!(
                        self.server.world,
                        chart,
                        (lx - 1, ly, lz),
                        world::BlockEntity::Bloomery(st)
                    );
                    let _ = self
                        .server
                        .world
                        .light_bloomery_at(chart.block(lx - 1, ly, lz));
                }
                // A bloom resting on the anvil, ready for the hammer.
                if let Some(bl) = reg2.item_id("base:steel_bloom") {
                    demo_anvil_put!(
                        self.server.world,
                        chart,
                        (sx - 3, sy, sz + 2),
                        ItemStack::new(&reg2, bl, 1),
                    );
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
                        self.give_dev_item(&reg, item, n);
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
                let sy = demo_height!(self.server.world, chart, sx, sz);
                for rx in 0..6i32 {
                    for rz in -2..=2i32 {
                        demo_set!(self.server.world, chart, sx + rx, sy, sz + rz, dirt);
                        demo_set!(self.server.world, chart, sx + rx, sy + 1, sz + rz, layer);
                    }
                }
                // A walker crossed the field on the diagonal.
                for i in 0..5i32 {
                    self.server
                        .world
                        .tread_at(chart.block(sx + i, sy + 1, sz - 2 + i));
                }
                // A break mid-burst, sparks and all; the tick re-stamps
                // the moment so any capture frame lands mid-effect.
                let center = chart
                    .entity(Vec3::new(sx as f32 + 2.5, sy as f32 + 2.5, sz as f32 + 0.5))
                    .render_pos();
                self.presentation.demo_burst =
                    Some((center, self.content.reg.block(dirt).tiles[0]));
                self.juice_burst(center, self.content.reg.block(dirt).tiles[0], 10, 2.2);
            }
            self.survival.health = 5.0;
            self.survival.damage_flash = 0.35;
        }

        if std::env::var("WILDFORGE_DEMO_GLASSWORKS").is_ok() {
            let reg = self.content.reg.clone();
            let b = |n: &str| reg.block_id(n);
            if let (Some(fb), Some(kiln), Some(quern)) =
                (b("base:firebrick"), b("base:kiln"), b("base:quern"))
            {
                let (sx, sz) = (spawn.x as i32 + 6, spawn.z as i32 - 6);
                let sy = demo_height!(self.server.world, chart, sx, sz) + 1;
                for ly in 0..3 {
                    for rx in -1..=1i32 {
                        for rz in -1..=1i32 {
                            if rx == 0 && rz == 0 {
                                continue;
                            }
                            demo_set!(self.server.world, chart, sx + rx, sy + ly, sz + rz, fb);
                        }
                    }
                    demo_set!(
                        self.server.world,
                        chart,
                        sx,
                        sy + ly,
                        sz,
                        crate::registry::AIR
                    );
                }
                demo_set!(self.server.world, chart, sx - 1, sy, sz, kiln);
                demo_set!(self.server.world, chart, sx - 3, sy, sz + 2, quern);
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
                    demo_insert!(
                        self.server.world,
                        chart,
                        (sx - 1, sy, sz),
                        world::BlockEntity::Kiln(st)
                    );
                    let _ = self.server.world.light_kiln_at(chart.block(sx - 1, sy, sz));
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
                        self.give_dev_item(&reg, item, n);
                    }
                }
                // Torches behind stained panes: the light comes out
                // the color of the glass (stage 5's proof).
                let (tx2, tz2) = (spawn.x as i32 - 8, spawn.z as i32 + 2);
                let ty2 = demo_height!(self.server.world, chart, tx2, tz2) + 1;
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
                                demo_set!(
                                    self.server.world,
                                    chart,
                                    tx2 - 1,
                                    ty2 + dy,
                                    z + dz,
                                    stone
                                );
                                if dy != 0 || dz != 0 {
                                    demo_set!(
                                        self.server.world,
                                        chart,
                                        tx2,
                                        ty2 + dy,
                                        z + dz,
                                        stone
                                    );
                                }
                            }
                        }
                        demo_set!(self.server.world, chart, tx2, ty2, z, torch);
                        demo_set!(self.server.world, chart, tx2 + 1, ty2, z, *pane);
                    }
                }
                // A stained window row so the tint shows in shots.
                let (wx, wz) = (spawn.x as i32 - 5, spawn.z as i32);
                let wy = demo_height!(self.server.world, chart, wx, wz) + 1;
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
                        demo_set!(self.server.world, chart, wx, wy, wz + i as i32, gb);
                        demo_set!(self.server.world, chart, wx, wy + 1, wz + i as i32, gb);
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
        // Calendar overrides must move the authoritative simulation clock too.
        // Local astronomy and climate sample `World::clock`; leaving it at the
        // pre-override value makes a capture's sky disagree with its weather.
        self.server.world.clock = (f64::from(self.server.world.day)
            + f64::from(self.server.time_of_day.rem_euclid(1.0)))
            * f64::from(crate::server::DAY_LENGTH);
        if let Ok(v) = std::env::var("WILDFORGE_WEATHER") {
            self.server.world.force_local_weather(&v);
            self.presentation.weather_vis = match v.as_str() {
                "overcast" => 0.4,
                "precip" | "rain" | "snow" => 0.55,
                "storm" => 0.7,
                _ => 0.0,
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
                    let y = demo_height!(self.server.world, chart, x, z) + 1;
                    let mut m = demo_mob!(
                        chart,
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
                let y = demo_height!(self.server.world, chart, sx - 3, sz - 5) + 1;
                demo_set!(self.server.world, chart, sx - 3, y, sz - 5, os);
                let mut st = world::OfferingState::default();
                if let Some(hw) = reg.item_id("base:heartwood") {
                    st.slots[0] = Some(ItemStack::new(&reg, hw, 2));
                }
                demo_insert!(
                    self.server.world,
                    chart,
                    (sx - 3, y, sz - 5),
                    world::BlockEntity::Offering(st)
                );
            }
            if let Some(sap) = reg.block_id("base:oak_sapling") {
                let y = demo_height!(self.server.world, chart, sx + 2, sz - 6) + 1;
                demo_set!(self.server.world, chart, sx + 2, y, sz - 6, sap);
            }
            let ty = demo_height!(self.server.world, chart, sx + 6, sz - 8) + 1;
            self.server
                .world
                .grow_tree_at(chart.block(sx + 6, ty, sz - 8), "oak", 3);
            for name in ["base:bedroll", "base:oak_sapling"] {
                if let Some(item) = reg.item_id(name) {
                    self.give_dev_item(&reg, item, 1);
                }
            }
        }
        // Dev: a stocked chest next to spawn, screen open (UI verification).
        if std::env::var("WILDFORGE_DEMO_CHEST").is_ok() {
            let p = (spawn.x as i32 - 2, spawn.y as i32, spawn.z as i32);
            let reg = self.content.reg.clone();
            if let Some(cb) = reg.block_id("base:chest") {
                demo_set!(self.server.world, chart, p.0, p.1, p.2, cb);
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
                demo_insert!(self.server.world, chart, p, world::BlockEntity::Chest(st));
                self.set_screen(Screen::Chest(chart.block_tuple(p)));
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
                    let y = demo_height!(self.server.world, chart, x, z) + 1;
                    let mut m = demo_mob!(
                        chart,
                        si,
                        Vec3::new(x as f32 + 0.5, y as f32 + 0.05, z as f32 + 0.5),
                        i as f32 * 1.3,
                    );
                    m.health = self.content.reg.animals[si].health;
                    self.server.world.spawn_mob(m);
                }
            }
        }
        // Dev: a line of fliers at eye level ahead, wings mid-beat
        // (flight and wingbeat verification — the one thing you cannot
        // judge from a still of a bird standing on the ground).
        if std::env::var("WILDFORGE_DEMO_FLIGHT").is_ok() {
            for (i, name) in ["base:gull", "base:eagle", "base:vulture", "base:bat"]
                .iter()
                .enumerate()
            {
                if let Some(si) = self.content.reg.animal_id(name) {
                    let x = spawn.x as i32 - 4 + i as i32 * 3;
                    let z = spawn.z as i32 - 9;
                    let y = demo_height!(self.server.world, chart, x, z) + 4;
                    let mut m = demo_mob!(
                        chart,
                        si,
                        Vec3::new(x as f32 + 0.5, y as f32, z as f32 + 0.5),
                        std::f32::consts::FRAC_PI_2,
                    );
                    m.health = self.content.reg.animals[si].health;
                    // Staggered so one still shows the whole stroke.
                    m.anim_phase = i as f32 * 0.9;
                    self.server.world.spawn_mob(m);
                }
            }
        }
        // Dev: fly to the nearest country's heart and look at what
        // stands over it. WILDFORGE_DEMO_EDIFICE=<n> steps outward
        // through neighbouring provinces, so each family can be seen.
        if let Ok(which) = std::env::var("WILDFORGE_DEMO_EDIFICE") {
            let skip: usize = which.parse().unwrap_or(0);
            let g = &self.server.world.generator;
            let home = g.province_at(spawn.surface()).key;
            let sites: Vec<SurfacePos> = (0..6)
                .flat_map(|r: i32| {
                    (-r..=r)
                        .flat_map(move |i| [(i, -r), (i, r), (-r, i), (r, i)])
                        .collect::<Vec<_>>()
                })
                .map(|(du, dv)| g.province_center_at(g.province_offset(home, du, dv)))
                .filter(|&site| g.surface_estimate_at(site) > crate::chunk::SEA_LEVEL + 4)
                .fold(Vec::new(), |mut acc, s| {
                    // The ring walk visits (0,0) four times over.
                    if !acc.contains(&s) {
                        acc.push(s);
                    }
                    acc
                });
            if let Some(&site) = sites.get(skip) {
                let ed = crate::edifice::edifice_of(self.server.world.generator.biome_at(site));
                let center = ChunkPos::from_surface(site);
                for cx in -3..=3 {
                    for cz in -3..=3 {
                        self.server.world.ensure_chunk(center.offset(cx, cz));
                    }
                }
                let base = self.server.world.surface_height_at(site);
                // Stand well back and a little above the crest.
                let back = std::env::var("WILDFORGE_DEMO_BACK")
                    .ok()
                    .and_then(|v| v.parse::<f32>().ok())
                    .unwrap_or((ed.reach * 4).max(40) as f32);
                self.player.pos = EntityPos::new(
                    site.face(),
                    f32::from(site.u()) + 0.5,
                    base as f32 + ed.rise as f32 * 0.7,
                    f32::from(site.v()) + 0.5,
                )
                .unwrap()
                .translated(Vec3::new(0.0, 0.0, back))
                .unwrap()
                .pos;
                self.player.vel = Vec3::ZERO;
                self.camera.yaw = -std::f32::consts::FRAC_PI_2;
                self.camera.pitch = -0.22;
                self.flying = true;
                eprintln!(
                    "edifice demo: {:?} {:?} at {:?} {},{base},{}",
                    self.server.world.generator.biome_at(site),
                    ed.family,
                    site.face(),
                    site.u(),
                    site.v()
                );
            }
        }
        // Dev: a stand of trees over grass, lit at one corner, so a
        // burn can be watched running rather than inferred from a
        // test's counters. WILDFORGE_DEMO_FIRE=mine lights it as a
        // player's; anything else is the wild's.
        if let Ok(who) = std::env::var("WILDFORGE_DEMO_FIRE") {
            let b = |n: &str| self.content.reg.block_id(n);
            let (bx, bz) = (spawn.x as i32 + 10, spawn.z as i32);
            let g = demo_height!(self.server.world, chart, bx, bz);
            if let (Some(grass), Some(log), Some(leaves)) =
                (b("base:grass"), b("base:log"), b("base:leaves"))
            {
                let w = &mut self.server.world;
                // The footprint spans several chunks, and any that are
                // not loaded yet will be GENERATED over the top of
                // whatever we build here.
                for cx in -1..=1 {
                    for cz in -1..=1 {
                        w.ensure_chunk(chart.chunk(bx + cx * 16, bz + cz * 16));
                    }
                }
                for x in -8..=8 {
                    for z in -8..=8 {
                        for y in (g - 2)..g {
                            demo_set!(w, chart, bx + x, y, bz + z, grass);
                        }
                        for y in (g + 1)..(g + 9) {
                            demo_set!(w, chart, bx + x, y, bz + z, AIR);
                        }
                        demo_set!(w, chart, bx + x, g, bz + z, grass);
                    }
                }
                // A copse: trunks on a lattice under one canopy.
                for x in (-6..=6).step_by(3) {
                    for z in (-6..=6).step_by(3) {
                        for y in 1..=4 {
                            demo_set!(w, chart, bx + x, g + y, bz + z, log);
                        }
                    }
                }
                for x in -7..=7 {
                    for z in -7..=7 {
                        for y in 4..=6 {
                            demo_set!(w, chart, bx + x, g + y, bz + z, leaves);
                        }
                    }
                }
                // Wilderness unless we say otherwise, so the wild's
                // own fire is willing to touch it.
                w.player_touched.clear();
                let mine = who == "mine";
                demo_fire!(w, chart, bx - 7, g + 1, bz - 7, mine);
                eprintln!("fire demo at ({bx},{g},{bz}), mine={mine}");
            }
            self.player.pos = self
                .player
                .pos
                .relocated_local(Vec3::new(
                    bx as f32 - 2.0,
                    g as f32 + 14.0,
                    bz as f32 + 26.0,
                ))
                .unwrap();
            self.player.vel = Vec3::ZERO;
            self.camera.yaw = -std::f32::consts::FRAC_PI_2;
            self.camera.pitch = -0.42;
            self.flying = true;
        }
        // Dev: a volcano flank — a staircase with a vent at the crest,
        // so a flow can be watched settling instead of guessed at.
        if std::env::var("WILDFORGE_DEMO_LAVA").is_ok() {
            let bx = spawn.x as i32 + 6;
            let bz = spawn.z as i32;
            let y0 = demo_height!(self.server.world, chart, bx, bz) + 14;
            let b = |n: &str| self.content.reg.block_id(n);
            let Some(stone) = b("base:basalt").or_else(|| b("base:stone")) else {
                return;
            };
            let w = &mut self.server.world;
            for step in 0..14i32 {
                let top = y0 - step;
                for x in (step * 2)..(step * 2 + 2) {
                    for z in -4..=4 {
                        for fill in 0..6 {
                            demo_set!(w, chart, bx + x, top - fill, bz + z, stone);
                        }
                    }
                }
            }
            let lava = self.content.reg.lava_for_volume(8);
            for z in -2..=2 {
                for x in 0..2 {
                    demo_set!(w, chart, bx + x, y0 + 1, bz + z, lava);
                }
            }
            // Stand the viewer off the flank looking along it, so the
            // shot frames the flow rather than the inside of the hill.
            self.player.pos = self
                .player
                .pos
                .relocated_local(Vec3::new(
                    bx as f32 + 13.0,
                    y0 as f32 + 3.0,
                    bz as f32 + 22.0,
                ))
                .unwrap();
            self.player.vel = Vec3::ZERO;
            self.camera.yaw = -std::f32::consts::FRAC_PI_2;
            self.camera.pitch = -0.42;
            self.flying = true;
            eprintln!("lava demo: crest at ({bx},{y0},{bz})");
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
                demo_set!(self.server.world, chart, p.0, p.1, p.2, fb);
                demo_insert!(
                    self.server.world,
                    chart,
                    p,
                    world::BlockEntity::Furnace(world::FurnaceState {
                        input: Some(ItemStack::new(&reg, raw, 5)),
                        fuel: Some(ItemStack::new(&reg, log, 3)),
                        ..Default::default()
                    }),
                );
                self.give_dev_item(&reg, reg.item_id("base:copper_ingot").unwrap(), 7);
                self.set_screen(Screen::Furnace(chart.block_tuple(p)));
            }
        }
        if let Ok(scene) = std::env::var("WILDFORGE_PLANET_SHOT") {
            self.stage_planet_qualification(&scene);
        }
    }

    /// Deterministic scenes used by the finite-planet visual gate.
    fn stage_planet_qualification(&mut self, scene: &str) {
        if matches!(scene, "sea" | "mountain") {
            let anchor = self.qualification_ocean();
            self.config.view_dist = 14;
            let y = if scene == "sea" {
                (SEA_LEVEL + 1) as f32
            } else {
                (SEA_LEVEL + 42) as f32
            };
            self.player.pos = EntityPos::new(
                anchor.face(),
                f32::from(anchor.u()) + 0.5,
                y,
                f32::from(anchor.v()) + 0.5,
            )
            .expect("qualification altitude is inside the voxel shell");
            self.player.vel = Vec3::ZERO;
            self.survival.spawn_point = self.player.pos;
            self.flying = true;
            self.camera.yaw = 0.18;
            self.camera.pitch = -0.035;
            self.camera.follow_planet(self.player.eye());
            eprintln!(
                "planet qualification {scene}: {:?} {},{} y={y}",
                anchor.face(),
                anchor.u(),
                anchor.v()
            );
            return;
        }

        let chart = DemoChart::new(Face::PosZ);
        let (center_x, center_z, radius) = if scene == "corner" {
            (4095, 4095, 3)
        } else {
            (4095, 0, 3)
        };
        let center = chart.chunk(center_x, center_z);
        for du in -radius..=radius {
            for dv in -radius..=radius {
                self.server.world.ensure_chunk(center.offset(du, dv));
            }
        }
        self.config.view_dist = 7;

        let stone = self.content.reg.block_id("base:cobblestone").unwrap_or(AIR);
        let planks = self.content.reg.block_id("base:planks").unwrap_or(stone);
        let red = self
            .content
            .reg
            .block_id("base:red_glass")
            .unwrap_or(planks);
        let blue = self
            .content
            .reg
            .block_id("base:blue_glass")
            .unwrap_or(planks);
        let amber = self
            .content
            .reg
            .block_id("base:amber_glass")
            .unwrap_or(planks);
        let water = self.content.reg.water_for_volume(8);
        let mut edits = Vec::new();

        if scene == "corner" {
            for x in 4088..=4103 {
                for z in 4088..=4103 {
                    let surface = chart.surface(x, z);
                    let floor = match surface.face() {
                        Face::PosZ => stone,
                        Face::PosX => red,
                        Face::PosY => blue,
                        _ => amber,
                    };
                    edits.push((chart.block(x, 108, z), floor));
                    for y in 109..=124 {
                        edits.push((chart.block(x, y, z), AIR));
                    }
                }
            }
            for (x, z, block) in [
                (4093, 4093, stone),
                (4098, 4093, red),
                (4093, 4098, blue),
                (4098, 4098, amber),
            ] {
                for y in 109..=115 {
                    edits.push((chart.block(x, y, z), block));
                }
            }
            self.player.pos = chart.entity(Vec3::new(4088.5, 109.0, 4088.5));
            self.camera.yaw = std::f32::consts::FRAC_PI_4;
            self.camera.pitch = -0.14;
        } else {
            for x in 4083..=4107 {
                for z in -13..=13 {
                    edits.push((chart.block(x, 108, z), stone));
                    for y in 109..=122 {
                        edits.push((chart.block(x, y, z), AIR));
                    }
                }
            }
            match scene {
                "building" => {
                    for x in 4091..=4100 {
                        for z in -6..=6 {
                            edits.push((chart.block(x, 109, z), planks));
                            for y in 110..=115 {
                                let wall = x == 4091 || x == 4100 || z == -6 || z == 6;
                                let doorway = x == 4091 && (-1..=1).contains(&z) && y <= 112;
                                if wall && !doorway {
                                    let block = if y == 112 && (z == -6 || z == 6) {
                                        if x < 4096 { blue } else { amber }
                                    } else {
                                        planks
                                    };
                                    edits.push((chart.block(x, y, z), block));
                                }
                            }
                            edits.push((chart.block(x, 116, z), planks));
                        }
                    }
                    self.player.pos = chart.entity(Vec3::new(4084.5, 110.0, 0.5));
                    self.camera.yaw = 0.0;
                    self.camera.pitch = -0.08;
                }
                "water" => {
                    for x in 4087..=4104i32 {
                        for z in -4..=4i32 {
                            if z.abs() == 4 || x == 4087 || x == 4104 {
                                edits.push((chart.block(x, 109, z), stone));
                            } else {
                                edits.push((chart.block(x, 109, z), water));
                            }
                        }
                    }
                    self.player.pos = chart.entity(Vec3::new(4084.5, 111.0, 0.5));
                    self.camera.yaw = 0.0;
                    self.camera.pitch = -0.28;
                }
                "players" => {
                    self.player.pos = chart.entity(Vec3::new(4095.5, 109.0, -5.0));
                    self.camera.yaw = std::f32::consts::FRAC_PI_2;
                    self.camera.pitch = -0.08;
                }
                other => {
                    eprintln!("unknown WILDFORGE_PLANET_SHOT={other:?}");
                    return;
                }
            }
        }

        self.server.world.edit_batch(|world| {
            for (pos, block) in edits {
                world.set_block_authored_at(pos, block, "development qualification scene");
            }
        });
        self.player.vel = Vec3::ZERO;
        self.survival.spawn_point = self.player.pos;
        self.flying = true;
        self.camera.follow_planet(self.player.eye());
        eprintln!("planet qualification {scene}: staged at the PosZ east seam");
    }

    fn qualification_ocean(&self) -> SurfacePos {
        for face in Face::ALL {
            for u in (128..crate::planet::FACE_BLOCKS).step_by(256) {
                for v in (128..crate::planet::FACE_BLOCKS).step_by(256) {
                    let center = SurfacePos::new(face, u, v).unwrap();
                    let deep = [(0, 0), (-160, 0), (160, 0), (0, -160), (0, 160)]
                        .into_iter()
                        .all(|(du, dv)| {
                            let sample = SurfacePos::canonicalized(
                                face,
                                i32::from(u) + du,
                                i32::from(v) + dv,
                            )
                            .unwrap();
                            self.server.world.generator.biome_at(sample)
                                == crate::worldgen::Biome::Ocean
                                && self.server.world.generator.surface_estimate_at(sample)
                                    < SEA_LEVEL - 4
                        });
                    if deep {
                        return center;
                    }
                }
            }
        }
        SurfacePos::new(Face::PosZ, 4096, 4096).unwrap()
    }
}
