//! Read-only presentation and received entity/mesh state for a guest world.

use std::collections::HashMap;
use std::sync::Arc;

use super::ReplicaWorld;
use crate::chunk::ChunkPos;
use crate::inventory::ItemStack;
use crate::mobs::{Mob, Projectile};
use crate::planet::{BlockPos, SurfacePos};
use crate::planet_atlas::LocalWeatherSample;
use crate::registry::{BlockId, Registry};
use crate::world::calendar_view::CalendarView;
use crate::world::local_structure::LocalStructure;
use crate::world::multiblock::BlockRead;
use crate::world::{BlockEntity, FallingBlock, SceneRead, TerrainRead};

impl ReplicaWorld {
    pub(crate) fn configure_entry(&mut self, mode: String, time: f32) {
        self.mode = mode;
        // Preserve the former guest Server::new clock. TimeIre changes the
        // displayed day/time; its existing protocol handling does not tick it.
        self.clock = (f64::from(self.day) + f64::from(time.rem_euclid(1.0)))
            * f64::from(crate::server::DAY_LENGTH);
    }

    pub(in crate::world) fn calendar_view(&self) -> CalendarView {
        CalendarView::new(self.day, self.clock, false)
    }

    pub(crate) fn seed(&self) -> u32 { self.seed }
    pub(crate) fn mode(&self) -> &str { &self.mode }
    pub(crate) fn chunk_count(&self) -> usize { self.terrain.len() }
    pub(crate) fn dirty_chunks(&self) -> Vec<ChunkPos> { self.terrain.dirty_positions() }
    pub(crate) fn chunks_outside_all(&self, centers: &[ChunkPos], radius: i32) -> Vec<ChunkPos> {
        self.terrain.outside(centers, radius)
    }
    pub(crate) fn mark_chunk_meshed(&mut self, position: ChunkPos) {
        self.terrain.mark_dirty(position, false);
    }
    pub(crate) fn mark_all_chunks_dirty(&mut self) { self.terrain.mark_all_dirty(); }
    pub(crate) fn unload_chunk(&mut self, position: ChunkPos) { self.terrain.remove(&position); }

    pub(crate) fn remap_loose_items(&mut self, old: &Registry, registry: &Registry) {
        crate::entity::remap_items(&mut self.loose_items, old, registry);
    }

    pub(crate) fn replace_registry(&mut self, registry: Arc<Registry>) {
        let old = std::mem::replace(&mut self.registry, registry);
        self.terrain.remap_from(&old, &self.registry);
    }

    pub(crate) fn predict_container_click(
        &mut self, position: BlockPos, cursor: &mut Option<ItemStack>,
        request: crate::player_ops::container::Click,
    ) -> Result<crate::player_ops::container::Effect, crate::player_ops::container::Rejected> {
        let entity = self.block_entities.get_mut(&position)
            .ok_or(crate::player_ops::container::Rejected::Missing)?;
        crate::player_ops::container::click(&self.registry, entity, cursor, request)
    }

    pub(crate) fn falling_blocks(&self) -> &[FallingBlock] { &self.falling }
    pub(crate) fn replace_falling_blocks(&mut self, falling: Vec<FallingBlock>) {
        self.falling = falling;
    }
    pub(crate) fn mob_by_id_mut(&mut self, id: u32) -> Option<&mut Mob> {
        self.mobs.iter_mut().find(|mob| mob.id == id)
    }
    pub(crate) fn for_each_mob_mut(&mut self, update: impl FnMut(&mut Mob)) {
        self.mobs.iter_mut().for_each(update);
    }
    pub(crate) fn for_each_projectile_mut(&mut self, update: impl FnMut(&mut Projectile)) {
        self.projectiles.iter_mut().for_each(update);
    }

    pub(crate) fn implement_tooltip(&self, stack: ItemStack, exact: bool) -> Vec<String> {
        if stack.arcane_id == 0 { return Vec::new(); }
        let clean = self.observations.charge(stack.arcane_id).unwrap_or(0);
        self.observations.implement(stack.arcane_id)
            .map_or_else(Vec::new, |state| state.tooltip(clean, exact))
    }

    pub(crate) fn implement_visual(&self, stack: ItemStack) -> Option<crate::implements::ImplementVisual> {
        let kind = &self.observations.implement(stack.arcane_id)?.kind;
        crate::world::item_presentation::implement_visual(
            &self.registry, kind, self.observations.charge(stack.arcane_id),
        )
    }

    pub(crate) fn apparatus_cues_near(
        &self, observer: crate::planet::EntityPos, radius: f32,
    ) -> Vec<crate::implements::ApparatusCue> {
        self.observations.apparatus_near(observer, radius)
    }

    pub(crate) fn prospect_at(&self, position: SurfacePos) -> crate::worldgen::ProspectReading {
        self.geography.prospect_at(position)
    }

    pub(crate) fn force_local_weather(&mut self, requested: &str) {
        self.weather_override = Some(crate::world::calendar_view::forced_weather(requested));
    }

    pub(crate) fn season_at_surface(&self, position: SurfacePos) -> usize {
        self.calendar_view().season_at(position)
    }
}

impl SceneRead for ReplicaWorld {
    fn local_structures(&self) -> &[LocalStructure] {
        // The protocol has no local-structure snapshots. The old guest world
        // also had no locally authored structures to select or render.
        &[]
    }
}

impl BlockRead for ReplicaWorld {
    type Pos = BlockPos;

    fn get_block(&self, position: BlockPos) -> BlockId { self.get_block_at(position) }
    fn offset(&self, position: BlockPos, delta: (i32, i32, i32)) -> Option<BlockPos> {
        position.offset(delta.0, delta.1, delta.2)
    }
    fn cell_delta(&self, from: BlockPos, to: BlockPos) -> Option<(i32, i32, i32)> {
        crate::world::block_store::planetary_delta(from, to)
    }
    fn block_entities(&self) -> &HashMap<BlockPos, BlockEntity> { &self.block_entities }
    fn reg(&self) -> &Arc<Registry> { &self.registry }
    fn to_world(&self, position: BlockPos) -> Option<BlockPos> { Some(position) }
    fn open_sky_above(&self, core: BlockPos) -> bool {
        core.offset(0, 3, 0).is_some_and(|above| self.light_at_pos(above).1 == 15)
    }
    fn weather_at(&self, position: BlockPos) -> LocalWeatherSample {
        self.weather_at_surface(position.surface())
    }
}

impl ReplicaWorld {
    pub(crate) fn clock(&self) -> f64 { self.clock }
    pub(crate) fn present_loose_item(&mut self, item: crate::entity::ItemEntity) {
        self.loose_items.push(item);
    }
}

impl ReplicaWorld {
    pub(crate) fn settle_spawn_at(&mut self, want: crate::planet::EntityPos) -> crate::planet::EntityPos {
        super::super::standing::settle(self, want)
    }
}

impl ReplicaWorld {
    pub(crate) fn seed_bearing_at(&self, from: crate::planet::EntityPos) -> String {
        crate::world::country_view::seed_bearing(&self.geography, from, |_| false)
    }
    pub(crate) fn heart_report_at(&self, position: SurfacePos) -> String {
        crate::world::country_view::heart_report(&self.geography, &self.registry, position, None)
    }
}
