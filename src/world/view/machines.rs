//! Read-only block-entity and machine-recognition queries.

use std::collections::{HashMap, hash_map};
use std::sync::Arc;

use super::{Source, WorldView};
use crate::planet::BlockPos;
use crate::planet_atlas::LocalWeatherSample;
use crate::registry::{BlockId, Registry};
use crate::world::multiblock::BlockRead;
use crate::world::{BlockEntity, SignState, TerrainRead};

impl<'a> WorldView<'a> {
    fn block_entity_map(&self) -> &'a HashMap<BlockPos, BlockEntity> {
        match self.source {
            Source::Authority(world) => BlockRead::block_entities(world),
            Source::Replica(world) => BlockRead::block_entities(world),
        }
    }
    pub(crate) fn block_entity_at(&self, position: &BlockPos) -> Option<&'a BlockEntity> {
        self.block_entity_map().get(position)
    }
    pub(crate) fn block_entities(&self) -> hash_map::Iter<'a, BlockPos, BlockEntity> {
        self.block_entity_map().iter()
    }
    pub(crate) fn sign_texts(&self) -> impl Iterator<Item = (BlockPos, &'a SignState)> {
        self.block_entity_map()
            .iter()
            .filter_map(|(position, entity)| match entity {
                BlockEntity::Sign(sign) => Some((*position, sign)),
                _ => None,
            })
    }
    pub(crate) fn check_bloomery_at(&self, position: BlockPos) -> Option<BlockPos> {
        crate::world::machines::check_machine_at(self, "base:bloomery", position)
    }
    pub(crate) fn check_forge_at(&self, position: BlockPos) -> Option<BlockPos> {
        crate::world::machines::check_machine_at(self, "base:forge", position)
    }
    pub(crate) fn check_kiln_at(&self, position: BlockPos) -> Option<BlockPos> {
        crate::world::machines::check_machine_at(self, "base:kiln", position)
    }
    pub(crate) fn check_glassworks_at(&self, position: BlockPos) -> Option<BlockPos> {
        crate::world::machines::check_glassworks_at(self, position)
    }
    pub(crate) fn check_stall_at(&self, position: BlockPos) -> bool {
        crate::world::machines::check_stall_at(self, position)
    }
    pub(crate) fn slot_category_at(&self, position: BlockPos) -> Option<&'static str> {
        crate::world::machines::slot_of_instance_at(self, position).map(|(_, category)| category)
    }
}

impl BlockRead for WorldView<'_> {
    type Pos = BlockPos;
    fn get_block(&self, position: BlockPos) -> BlockId {
        self.get_block_at(position)
    }
    fn offset(&self, position: BlockPos, delta: (i32, i32, i32)) -> Option<BlockPos> {
        position.offset(delta.0, delta.1, delta.2)
    }
    fn cell_delta(&self, from: BlockPos, to: BlockPos) -> Option<(i32, i32, i32)> {
        crate::world::block_store::planetary_delta(from, to)
    }
    fn block_entities(&self) -> &HashMap<BlockPos, BlockEntity> {
        self.block_entity_map()
    }
    fn reg(&self) -> &Arc<Registry> {
        self.registry()
    }
    fn to_world(&self, position: BlockPos) -> Option<BlockPos> {
        Some(position)
    }
    fn open_sky_above(&self, core: BlockPos) -> bool {
        core.offset(0, 3, 0)
            .is_some_and(|above| self.light_at_pos(above).1 == 15)
    }
    fn weather_at(&self, position: BlockPos) -> LocalWeatherSample {
        self.weather_at_surface(position.surface())
    }
}

impl WorldView<'_> {
    pub(crate) fn is_nudge_mechanism_at(&self, position: crate::planet::BlockPos) -> bool {
        self.registry()
            .block(self.get_block_at(position))
            .interaction
            .as_deref()
            == Some("firebox")
            && matches!(
                self.block_entity_at(&position),
                Some(crate::world::BlockEntity::Steam(_))
            )
    }
}
