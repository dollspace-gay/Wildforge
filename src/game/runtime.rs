//! The active graphical world is either a local simulation or a guest replica.

use crate::server::Server;
use crate::world::{ReplicaWorld, WorldView};

pub(super) enum PlayRuntime {
    Local(Box<Server>),
    Guest { world: Box<ReplicaWorld>, time_of_day: f32 },
}

impl PlayRuntime {
    pub(super) fn is_guest(&self) -> bool { matches!(self, Self::Guest { .. }) }

    pub(super) fn local_session(server: Server) -> Self { Self::Local(Box::new(server)) }

    pub(super) fn set_local(&mut self, server: Server) {
        *self = Self::local_session(server);
    }

    pub(super) fn set_guest(&mut self, world: ReplicaWorld, time_of_day: f32) {
        *self = Self::Guest { world: Box::new(world), time_of_day };
    }

    pub(super) fn view(&self) -> WorldView<'_> {
        match self {
            Self::Local(server) => server.world.view(),
            Self::Guest { world, .. } => world.view(),
        }
    }

    /// Local adapters call this only after handling their remote request path.
    /// Query consumers use view(); a replica cannot supply an authoritative World.
    pub(super) fn local(&self) -> &Server {
        match self {
            Self::Local(server) => server,
            Self::Guest { .. } => panic!("local adapter used in a guest session"),
        }
    }

    /// Mutation is restricted to local simulation/operation adapters. Selecting
    /// a guest is an internal routing error, never an alternate simulation mode.
    pub(super) fn local_mut(&mut self) -> &mut Server {
        match self {
            Self::Local(server) => server,
            Self::Guest { .. } => panic!("local mutation used in a guest session"),
        }
    }

    pub(super) fn guest_mut(&mut self) -> Option<(&mut ReplicaWorld, &mut f32)> {
        match self {
            Self::Local(_) => None,
            Self::Guest { world, time_of_day } => Some((world, time_of_day)),
        }
    }

    pub(super) fn remap_loose_items(&mut self, old: &crate::registry::Registry, registry: &crate::registry::Registry) {
        match self {
            Self::Local(server) => server.world.remap_loose_items(old, registry),
            Self::Guest { world, .. } => world.remap_loose_items(old, registry),
        }
    }

    pub(super) fn replace_registry(&mut self, registry: std::sync::Arc<crate::registry::Registry>) {
        match self {
            Self::Local(server) => server.world.replace_registry(registry),
            Self::Guest { world, .. } => world.replace_registry(registry),
        }
    }

    pub(super) fn force_local_weather(&mut self, requested: &str) {
        match self {
            Self::Local(server) => server.world.force_local_weather(requested),
            Self::Guest { world, .. } => world.force_local_weather(requested),
        }
    }

    pub(super) fn finish_craft(
        &mut self, effects: crate::player_ops::craft::CraftEffects,
        position: Option<crate::planet::BlockPos>, inventory: &mut crate::inventory::Inventory,
    ) -> crate::player_ops::craft::CraftKind {
        match self {
            Self::Local(server) => effects.finish(&mut server.world, position, inventory, true, "full inventory after crafting"),
            Self::Guest { world, .. } => effects.finish_prediction(crate::world::TerrainRead::registry(world.as_ref()), inventory),
        }
    }

    pub(super) fn click_container(
        &mut self, position: crate::planet::BlockPos, cursor: &mut Option<crate::inventory::ItemStack>,
        request: crate::player_ops::container::Click,
    ) -> Result<crate::player_ops::container::Effect, crate::player_ops::container::Rejected> {
        match self {
            Self::Local(server) => server.world.click_container(position, cursor, request),
            Self::Guest { world, .. } => world.predict_container_click(position, cursor, request),
        }
    }

    pub(super) fn present_mob_feeding(&mut self, id: u32, plan: crate::player_ops::feeding::FeedPlan) -> bool {
        let mob = match self {
            Self::Local(server) => server.world.mob_by_id_mut(id),
            Self::Guest { world, .. } => world.mob_by_id_mut(id),
        };
        let Some(mob) = mob else { return false; };
        plan.apply(mob)
    }

    pub(super) fn present_ridden_mob(&mut self, id: u32, position: crate::planet::EntityPos, yaw: f32) {
        let mob = match self {
            Self::Local(server) => server.world.mob_by_id_mut(id),
            Self::Guest { world, .. } => world.mob_by_id_mut(id),
        };
        if let Some(mob) = mob {
            mob.pos = position;
            mob.vel = glam::Vec3::ZERO;
            mob.yaw = -yaw + std::f32::consts::FRAC_PI_2;
        }
    }

    pub(super) fn mark_chunk_meshed(&mut self, position: crate::chunk::ChunkPos) {
        match self {
            Self::Local(server) => server.world.mark_chunk_meshed(position),
            Self::Guest { world, .. } => world.mark_chunk_meshed(position),
        }
    }

    pub(super) fn mark_all_chunks_dirty(&mut self) {
        match self {
            Self::Local(server) => server.world.mark_all_chunks_dirty(),
            Self::Guest { world, .. } => world.mark_all_chunks_dirty(),
        }
    }

    pub(super) fn evict_chunks(
        &mut self, candidates: Vec<crate::chunk::ChunkPos>,
    ) -> (crate::world::ResidencyReport, Vec<crate::chunk::ChunkPos>) {
        match self {
            Self::Local(server) => server.world.evict_chunks(candidates),
            Self::Guest { world, .. } => {
                for &position in &candidates { world.unload_chunk(position); }
                let report = crate::world::ResidencyReport { released: candidates.len(), ..Default::default() };
                (report, candidates)
            }
        }
    }

    /// Player knowledge and script KV retain the existing local cache location.
    /// This does not grant the replica world a persistence capability.
    pub(super) fn player_sidecar_dir(&self) -> std::path::PathBuf {
        match self {
            Self::Local(server) => server.world.save_dir_for_saving(),
            Self::Guest { .. } => std::path::PathBuf::from("saves/.remote/world-cache"),
        }
    }

    pub(super) fn present_loose_item(&mut self, item: crate::entity::ItemEntity) {
        match self {
            Self::Local(server) => { server.world.spawn_loose_item(item); }
            Self::Guest { world, .. } => world.present_loose_item(item),
        }
    }

    pub(super) fn settle_spawn_at(&mut self, wanted: crate::planet::EntityPos) -> crate::planet::EntityPos {
        match self {
            Self::Local(server) => server.world.settle_spawn_at(wanted),
            Self::Guest { world, .. } => world.settle_spawn_at(wanted),
        }
    }

    pub(super) fn present_mob_hit(&mut self, id: u32) {
        let mob = match self {
            Self::Local(server) => server.world.mob_by_id_mut(id),
            Self::Guest { world, .. } => world.mob_by_id_mut(id),
        };
        if let Some(mob) = mob { mob.hurt_flash = 0.35; }
    }

    pub(super) fn time_of_day(&self) -> f32 {
        match self {
            Self::Local(server) => server.time_of_day,
            Self::Guest { time_of_day, .. } => *time_of_day,
        }
    }

    pub(super) fn time_of_day_mut(&mut self) -> &mut f32 {
        match self {
            Self::Local(server) => &mut server.time_of_day,
            Self::Guest { time_of_day, .. } => time_of_day,
        }
    }
}

impl super::Game {
    /// Old guest-only mutations had no host operation and were overwritten by
    /// snapshots. Reject before charging inventory; supported actions send their
    /// explicit request instead. This does not introduce new protocol variants.
    pub(super) fn reject_guest_action(&mut self) -> bool {
        if !self.runtime.is_guest() { return false; }
        self.toast("This action is not supported in multiplayer yet.".into());
        self.input.right_held = false;
        self.input.action_cooldown = 0.35;
        true
    }
}
