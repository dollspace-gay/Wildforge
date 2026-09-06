//! Entity and structure observations for rendering and selection.

use super::{Source, WorldView};
use crate::entity::ItemEntity;
use crate::mobs::{Mob, Projectile};
use crate::world::local_structure::{LocalStructure, LocalStructureId};
use crate::world::{FallingBlock, SceneRead};

impl<'a> WorldView<'a> {
    pub(crate) fn mobs(&self) -> &'a [Mob] {
        match self.source {
            Source::Authority(world) => world.mobs(),
            Source::Replica(world) => world.mobs(),
        }
    }
    pub(crate) fn mob(&self, index: usize) -> Option<&'a Mob> {
        self.mobs().get(index)
    }
    pub(crate) fn mob_by_id(&self, id: u32) -> Option<&'a Mob> {
        self.mobs().iter().find(|mob| mob.id == id)
    }
    pub(crate) fn mob_count(&self) -> usize {
        self.mobs().len()
    }
    pub(crate) fn projectiles(&self) -> &'a [Projectile] {
        match self.source {
            Source::Authority(world) => world.projectiles(),
            Source::Replica(world) => world.projectiles(),
        }
    }
    pub(crate) fn loose_items(&self) -> &'a [ItemEntity] {
        match self.source {
            Source::Authority(world) => world.loose_items(),
            Source::Replica(world) => world.loose_items(),
        }
    }
    pub(crate) fn falling_blocks(&self) -> &'a [FallingBlock] {
        match self.source {
            Source::Authority(world) => world.falling_blocks(),
            Source::Replica(world) => world.falling_blocks(),
        }
    }
    pub(super) fn structures(&self) -> &'a [LocalStructure] {
        match self.source {
            Source::Authority(world) => world.local_structures(),
            Source::Replica(world) => world.local_structures(),
        }
    }
    pub(crate) fn local_structure(&self, id: LocalStructureId) -> Option<&'a LocalStructure> {
        self.structures()
            .iter()
            .find(|structure| structure.id == id)
    }
    pub(crate) fn npc_by_mob(&self, id: u32) -> Option<&'a crate::npc::NpcInstance> {
        match self.source {
            Source::Authority(world) => world.npc_by_mob(id),
            // The protocol streams the companion mob, not the private NPC state.
            Source::Replica(_) => None,
        }
    }
    pub(crate) fn gate_at(&self, position: crate::planet::BlockPos) -> Option<usize> {
        match self.source {
            Source::Authority(world) => world.gate_at(position),
            // Dungeon gate registries are host state, absent from guest snapshots.
            Source::Replica(_) => None,
        }
    }
}

impl WorldView<'_> {
    pub(crate) fn dungeon_checkpoint_for(
        &self,
        position: crate::planet::EntityPos,
    ) -> Option<crate::planet::EntityPos> {
        match self.source {
            Source::Authority(world) => world.dungeon_checkpoint_for(position),
            Source::Replica(_) => None,
        }
    }
}
