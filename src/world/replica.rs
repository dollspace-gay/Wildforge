//! An independently owned guest world. It cannot generate, simulate, or save.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::chunk::{Chunk, ChunkPos};
use crate::worldgen::Geography;
use crate::entity::ItemEntity;
use crate::mobs::{Mob, Projectile};
use crate::planet::{BlockPos, SurfacePos};
use crate::planet_atlas::LocalWeatherSample;
use crate::registry::{AIR, BlockId, Registry};
mod scene;

use super::terrain::TerrainStore;
use super::{BlockEntity, ReplicaObservations, ReplicationTarget, TerrainRead};

/// Host-owned values copied into resident presentation state. The private
/// storage deliberately contains no generator, save path, simulation queues,
/// identity allocators, or finite-material/Current ledgers.
pub(crate) struct ReplicaWorld {
    registry: Arc<Registry>,
    terrain: TerrainStore,
    geography: Geography,
    observations: ReplicaObservations,
    block_entities: HashMap<BlockPos, BlockEntity>,
    mobs: Vec<Mob>,
    projectiles: Vec<Projectile>,
    loose_items: Vec<ItemEntity>,
    seed: u32,
    mode: String,
    clock: f64,
    weather_override: Option<LocalWeatherSample>,
    falling: Vec<super::FallingBlock>,
    day: u32,
    ire: f32,
}

impl ReplicaWorld {
    pub(crate) fn new(seed: u32, registry: Arc<Registry>, ire: f32) -> Self {
        Self {
            registry,
            terrain: TerrainStore::default(),
            geography: Geography::new(seed, None),
            observations: ReplicaObservations::default(),
            block_entities: HashMap::new(),
            mobs: Vec::new(),
            projectiles: Vec::new(),
            loose_items: Vec::new(),
            seed,
            mode: "survival".into(),
            clock: 0.0,
            weather_override: None,
            falling: Vec::new(),
            day: 0,
            ire,
        }
    }

    pub(crate) fn day(&self) -> u32 { self.day }
    pub(crate) fn ire(&self) -> f32 { self.ire }
    pub(crate) fn mobs(&self) -> &[Mob] { &self.mobs }
    pub(crate) fn projectiles(&self) -> &[Projectile] { &self.projectiles }
    pub(crate) fn loose_items(&self) -> &[ItemEntity] { &self.loose_items }

    pub(crate) fn replace_mobs(&mut self, mobs: Vec<Mob>) { self.mobs = mobs; }
    pub(crate) fn replace_projectiles(&mut self, projectiles: Vec<Projectile>) {
        self.projectiles = projectiles;
    }
    pub(crate) fn replace_loose_items(&mut self, items: Vec<ItemEntity>) { self.loose_items = items; }

    pub(crate) fn remote_arcane_cue(&self) -> [u8; 2] { self.observations.arcane_bands() }
    pub(crate) fn remote_arcane_dominant(&self) -> u8 { self.observations.arcane_dominant() }
    pub(crate) fn arcane_ecology(&self) -> Option<crate::arcane_ecology::EcologyObservation> {
        self.observations.ecology()
    }
    pub(crate) fn inspectable_item_current(&self, id: u64) -> Option<u64> {
        self.observations.charge(id)
    }
    pub(crate) fn set_remote_arcane_item(&mut self, id: u64, units: u64) {
        self.observations.set_charge(id, units);
    }

    pub(crate) fn weather_at_surface(&self, position: SurfacePos) -> LocalWeatherSample {
        self.observations.weather_at(position).unwrap_or_else(|| {
            let temperature = crate::climate::seasonal_temperature(
                self.geography.climate_at(position).t, position, f64::from(self.day),
            );
            super::calendar_view::fallback_weather(temperature, self.weather_override)
        })
    }

    pub(crate) fn biome_here_at(&self, position: SurfacePos) -> crate::worldgen::Biome {
        if self.is_open_water_at(position) {
            crate::worldgen::Biome::Ocean
        } else {
            // Heart graft state is authoritative and has never been streamed.
            self.geography.biome_at(position)
        }
    }

    fn apply_block(
        &mut self,
        position: BlockPos,
        block: BlockId,
        metadata: u8,
        salt_mass: u16,
        soil_salinity: u8,
        relight: &mut HashSet<ChunkPos>,
    ) {
        let Some(old) = self.terrain.write_state(position, block, metadata, salt_mass, soil_salinity) else {
            return;
        };
        self.terrain.dirty_edit_neighbors(position);
        if let Some((above, _)) = self.terrain.unsupported_above(&self.registry, position, block) {
            self.apply_block(above, AIR, 0, 0, 0, relight);
        }
        let fluid_level_only = self.registry.is_fluid(old)
            && self.registry.is_fluid(block)
            && self.registry.is_lava(old) == self.registry.is_lava(block);
        if !fluid_level_only { relight.insert(position.chunk()); }
        if old != block { self.block_entities.remove(&position); }
    }
}

impl TerrainRead for ReplicaWorld {
    fn registry(&self) -> &Arc<Registry> { &self.registry }
    fn chunk(&self, position: ChunkPos) -> Option<&Chunk> { self.terrain.get(&position) }
    // Settlement reveal records are host state and have never been replicated.
    // The guest has no locally authored hidden-cell overlay.
    fn is_hidden(&self, _position: BlockPos) -> bool { false }
    fn hidden_in_chunk(&self, _position: ChunkPos) -> Vec<BlockPos> { Vec::new() }
}

impl ReplicationTarget for ReplicaWorld {
    fn observations_mut(&mut self) -> &mut ReplicaObservations { &mut self.observations }

    fn receive_clock(&mut self, ire: f32, day: u32) {
        self.ire = ire;
        self.day = day;
    }

    fn receive_block_entity(&mut self, position: BlockPos, entity: BlockEntity) {
        self.block_entities.insert(position, entity);
    }

    fn insert_remote_chunks<'a>(
        &mut self,
        chunks: impl IntoIterator<Item = (ChunkPos, &'a [u8])>,
        remap: &[BlockId],
    ) {
        self.terrain.insert_remote_chunks(&self.registry, chunks, remap);
    }

    fn apply_remote_block_states(
        &mut self,
        updates: impl IntoIterator<Item = (BlockPos, BlockId, u8, u16, u8)>,
    ) {
        let mut relight = HashSet::new();
        for (position, block, metadata, salt_mass, soil_salinity) in updates {
            self.apply_block(position, block, metadata, salt_mass, soil_salinity, &mut relight);
        }
        self.terrain.relight_chunks_and_cascade(&self.registry, relight);
    }
}
