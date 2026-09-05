//! Block installations and their transient work/cadence state.

use std::collections::{HashMap, hash_map};
use crate::inventory::ItemStack;
use crate::planet::BlockPos;
use crate::registry::Registry;
use super::BlockEntity;

#[derive(Default)]
pub(super) struct Installations {
    entries: HashMap<BlockPos, BlockEntity>,
    work: HashMap<BlockPos, f32>,
    perish_accum: f32,
    industrial_ire_accum: f32,
}

impl Installations {
    pub(super) fn get(&self, pos: &BlockPos) -> Option<&BlockEntity> { self.entries.get(pos) }
    pub(super) fn get_mut(&mut self, pos: &BlockPos) -> Option<&mut BlockEntity> { self.entries.get_mut(pos) }
    pub(super) fn insert(&mut self, pos: BlockPos, entity: BlockEntity) -> Option<BlockEntity> { self.entries.insert(pos, entity) }
    pub(super) fn remove(&mut self, pos: &BlockPos) -> Option<BlockEntity> { self.entries.remove(pos) }
    pub(super) fn contains_key(&self, pos: &BlockPos) -> bool { self.entries.contains_key(pos) }
    pub(super) fn entry(&mut self, pos: BlockPos) -> hash_map::Entry<'_, BlockPos, BlockEntity> { self.entries.entry(pos) }
    pub(super) fn iter(&self) -> hash_map::Iter<'_, BlockPos, BlockEntity> { self.entries.iter() }
    pub(super) fn iter_mut(&mut self) -> hash_map::IterMut<'_, BlockPos, BlockEntity> { self.entries.iter_mut() }
    pub(super) fn keys(&self) -> hash_map::Keys<'_, BlockPos, BlockEntity> { self.entries.keys() }
    /// The shared BlockRead/BlockStore adapters borrow the actual owner.
    pub(super) fn entries(&self) -> &HashMap<BlockPos, BlockEntity> { &self.entries }
    pub(super) fn entries_mut(&mut self) -> &mut HashMap<BlockPos, BlockEntity> { &mut self.entries }

    pub(super) fn click(
        &mut self, registry: &Registry, pos: BlockPos, cursor: &mut Option<ItemStack>,
        request: crate::player_ops::container::Click,
    ) -> Result<crate::player_ops::container::Effect, crate::player_ops::container::Rejected> {
        let entity = self.entries.get_mut(&pos).ok_or(crate::player_ops::container::Rejected::Missing)?;
        crate::player_ops::container::click(registry, entity, cursor, request)
    }

    pub(super) fn work_at(&self, pos: BlockPos) -> f32 { self.work.get(&pos).copied().unwrap_or(0.0) }
    pub(super) fn accumulate_work(&mut self, pos: BlockPos, amount: f32) -> f32 {
        let bank = self.work.entry(pos).or_insert(0.0);
        *bank += amount;
        *bank
    }
    pub(super) fn reset_work(&mut self, pos: BlockPos) { *self.work.entry(pos).or_insert(0.0) = 0.0; }
    pub(super) fn consume_work(&mut self, pos: BlockPos, amount: f32) { *self.work.entry(pos).or_insert(0.0) -= amount; }
    pub(super) fn forget_work(&mut self, pos: BlockPos) { self.work.remove(&pos); }
    pub(super) fn wheel_momentum(&mut self, pos: BlockPos, wet: bool, dt: f32, spindown: f32) -> f32 {
        let bank = self.work.entry(pos).or_insert(0.0);
        if wet { *bank = spindown; } else { *bank = (*bank - dt).max(0.0); }
        if *bank > 0.0 { 1.0 } else { 0.0 }
    }

    pub(super) fn perish_cycle(&mut self, dt: f32, period: f32) -> bool {
        self.perish_accum += dt;
        if self.perish_accum < period { return false; }
        self.perish_accum -= period;
        true
    }
    pub(super) fn industrial_cycle(&mut self, dt: f32) -> Option<f32> {
        self.industrial_ire_accum += dt;
        if self.industrial_ire_accum < 1.0 { return None; }
        Some(std::mem::take(&mut self.industrial_ire_accum))
    }
}
