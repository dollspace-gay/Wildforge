//! Installation access coordinator for the authoritative world.

use super::{BlockEntity, BlockPos, ItemStack, World};

impl World {
    #[cfg(test)]
    pub fn block_entity(&self, pos: &(i32, i32, i32)) -> Option<&BlockEntity> {
        crate::planet::BlockPos::of_world(pos.0, pos.1, pos.2)
            .and_then(|pos| self.installations.get(&pos))
    }

    #[cfg(test)]
    pub fn block_entity_mut(&mut self, pos: &(i32, i32, i32)) -> Option<&mut BlockEntity> {
        let pos = crate::planet::BlockPos::of_world(pos.0, pos.1, pos.2)?;
        self.installations.get_mut(&pos)
    }

    pub(crate) fn click_container(
        &mut self, position: BlockPos, cursor: &mut Option<ItemStack>,
        request: crate::player_ops::container::Click,
    ) -> Result<crate::player_ops::container::Effect, crate::player_ops::container::Rejected> {
        self.installations.click(&self.reg, position, cursor, request)
    }

    pub fn block_entity_at(&self, pos: &crate::planet::BlockPos) -> Option<&BlockEntity> {
        self.installations.get(pos)
    }

    pub fn block_entity_mut_at(
        &mut self,
        pos: &crate::planet::BlockPos,
    ) -> Option<&mut BlockEntity> {
        self.installations.get_mut(pos)
    }

    #[cfg(test)]
    pub fn insert_block_entity(
        &mut self,
        pos: (i32, i32, i32),
        entity: BlockEntity,
    ) -> Option<BlockEntity> {
        let pos = crate::planet::BlockPos::of_world(pos.0, pos.1, pos.2)?;
        self.installations.insert(pos, entity)
    }

    pub fn insert_block_entity_at(
        &mut self,
        pos: crate::planet::BlockPos,
        entity: BlockEntity,
    ) -> Option<BlockEntity> {
        self.installations.insert(pos, entity)
    }

    /// Insert a development-authored machine/container while keeping every
    /// finite stack in its buffers visible to the material ledger.
    pub fn insert_block_entity_authored_at(
        &mut self,
        pos: crate::planet::BlockPos,
        entity: BlockEntity,
        source: &str,
    ) -> Option<BlockEntity> {
        let old = self.installations.remove(&pos);
        if let Some(previous) = old.as_ref()
            && let Err(error) = self.record_admin_block_entity_deletion(previous)
        {
            eprintln!("materials: authored block-entity replacement failed: {error}");
        }
        if let Err(error) = self.record_external_block_entity_contents(&entity, source) {
            eprintln!("materials: authored block-entity source failed: {error}");
        }
        self.installations.insert(pos, entity);
        old
    }

    pub fn ensure_block_entity_at(
        &mut self,
        pos: crate::planet::BlockPos,
        default: BlockEntity,
    ) -> &mut BlockEntity {
        self.installations.entry(pos).or_insert(default)
    }

    #[cfg(test)]
    pub fn has_block_entity(&self, pos: &(i32, i32, i32)) -> bool {
        crate::planet::BlockPos::of_world(pos.0, pos.1, pos.2)
            .is_some_and(|pos| self.installations.contains_key(&pos))
    }

    pub fn block_entities(&self) -> impl Iterator<Item = (&crate::planet::BlockPos, &BlockEntity)> {
        self.installations.iter()
    }

    /// Live nest spawn-gate records (capability E9), for tests and tooling.
    #[cfg(test)]
    pub fn nests(&self) -> impl Iterator<Item = (&crate::planet::BlockPos, usize)> {
        self.nests.iter().map(|(pos, nest)| (pos, *nest))
    }
}
