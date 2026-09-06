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

use crate::game::Game;
use crate::inventory::ItemStack;
use crate::registry::{ItemId, Registry};
use crate::planet::{BlockPos, EntityPos, Face, SurfacePos};
use crate::chunk::ChunkPos;
use glam::Vec3;

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
            && let Err(error) = self.runtime.local_mut().world.record_external_stack(
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
        self.stage_capture_water(spawn, chart);
        self.stage_capture_presentation();
        self.stage_capture_magic_ecology(spawn, chart);
        self.stage_capture_colored_shadows(spawn, chart);
        self.stage_capture_room(spawn, chart);
        self.stage_capture_bounce_room(spawn, chart);
        self.stage_capture_point_lights(spawn, chart);
        self.stage_capture_trade(spawn, chart);
        self.stage_capture_wildlife(spawn, chart);
        self.stage_capture_heart(spawn, chart);
        self.stage_capture_ecology(spawn, chart);
        self.stage_capture_mill(spawn, chart);
        self.stage_capture_camp(spawn, chart);
        self.stage_capture_torch_room(spawn, chart);
        self.stage_capture_ice(spawn, chart);
        self.stage_capture_glowglass(spawn, chart);
        self.stage_capture_rock(spawn, chart);
        self.stage_capture_corner(spawn, chart);
        self.stage_capture_pool(spawn, chart);
        self.stage_capture_colored_light(spawn, chart);
        self.stage_capture_pillars(spawn, chart);
        self.stage_capture_steelworks(spawn, chart);
        self.stage_capture_juice(spawn, chart);
        self.stage_capture_glassworks(spawn, chart);
        self.stage_capture_environment();
        self.stage_capture_wardens(spawn, chart);
        self.stage_capture_steward(spawn, chart);
        self.stage_capture_chest(spawn, chart);
        self.stage_capture_inventory();
        self.stage_capture_mobs(spawn, chart);
        self.stage_capture_flight(spawn, chart);
        self.stage_capture_edifice(spawn);
        self.stage_capture_fire(spawn, chart);
        if self.stage_capture_lava(spawn, chart) { return; }
        self.stage_capture_furnace(spawn, chart);
        if (std::env::var("WILDFORGE_DEMO_IMPLEMENTS").is_ok()
            || std::env::var("WILDFORGE_DEMO_WORKINGS").is_ok())
            && let Err(error) = self.stage_implements_demo(spawn)
        {
            eprintln!("implements demo could not be staged: {error}");
        }
        if std::env::var("WILDFORGE_DEMO_ALCHEMY").is_ok()
            && let Err(error) = self.stage_alchemy_demo(spawn)
        {
            eprintln!("alchemy demo could not be staged: {error}");
        }
        if let Ok(scene) = std::env::var("WILDFORGE_PLANET_SHOT") {
            self.stage_planet_qualification(&scene);
        }
    }

}

mod flow;
mod overrides;
mod ecology;
mod lighting;
mod bounce;
mod trade;
mod stewardship;
mod mill;
mod materials;
mod industry;
mod animals;
mod storage;
mod landmarks;
mod alchemy;
mod implements;
mod planet;
