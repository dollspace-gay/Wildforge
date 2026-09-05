//! Host IDs are resolved once against the content used by a guest session.

use std::fmt;
use std::sync::Arc;

use crate::entity::ItemEntity;
use crate::inventory::ItemStack;
use crate::net::{LooseItemSnap, StackSnap};
use crate::registry::{BlockId, ItemId, Registry};

pub(crate) struct ContentMap {
    registry: Arc<Registry>,
    block_names: Vec<String>,
    item_names: Vec<String>,
    blocks: Vec<BlockId>,
    items: Vec<Option<ItemId>>,
}

impl fmt::Debug for ContentMap {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ContentMap")
            .field("blocks", &self.blocks.len())
            .field("items", &self.items.len())
            .field("content_hash", &self.registry.content_hash)
            .finish()
    }
}

impl ContentMap {
    pub(crate) fn empty(registry: Arc<Registry>) -> Self {
        Self::new(registry, Vec::new(), Vec::new())
    }

    pub(crate) fn new(registry: Arc<Registry>, blocks: Vec<String>, items: Vec<String>) -> Self {
        let (block_ids, item_ids) = resolve(&registry, &blocks, &items);
        Self {
            blocks: block_ids,
            items: item_ids,
            block_names: blocks,
            item_names: items,
            registry,
        }
    }

    /// Keep host identities while replacing the local content that resolves them.
    pub(crate) fn rebind(&mut self, registry: Arc<Registry>) {
        let (blocks, items) = resolve(&registry, &self.block_names, &self.item_names);
        self.blocks = blocks;
        self.items = items;
        self.registry = registry;
    }

    pub(super) fn registry(&self) -> &Registry {
        &self.registry
    }

    pub(crate) fn block(&self, wire: u16) -> BlockId {
        self.blocks
            .get(usize::from(wire))
            .copied()
            .unwrap_or(self.registry.unknown_block)
    }

    pub(crate) fn blocks(&self) -> &[BlockId] {
        &self.blocks
    }

    pub(crate) fn item(&self, wire: u16) -> Option<ItemId> {
        self.items.get(usize::from(wire)).copied().flatten()
    }

    /// Presentation may resolve several fields of a host implement visual.
    pub(crate) fn items(&self) -> &[Option<ItemId>] {
        &self.items
    }

    pub(crate) fn stack(&self, wire: &StackSnap) -> Option<ItemStack> {
        Some(ItemStack {
            item: self.item(wire.item)?,
            count: wire.count,
            durability: wire.durability,
            arcane_id: wire.arcane_id,
        })
    }

    pub(crate) fn slots<const N: usize>(
        &self,
        wires: &[Option<StackSnap>],
    ) -> [Option<ItemStack>; N] {
        std::array::from_fn(|index| {
            wires
                .get(index)
                .and_then(Option::as_ref)
                .and_then(|wire| self.stack(wire))
        })
    }

    pub(crate) fn loose_item(&self, wire: &LooseItemSnap) -> Option<ItemEntity> {
        let item = self.item(wire.item)?;
        let mut entity = ItemEntity::new(wire.pos, wire.vel, item, wire.count);
        entity.stable_id = wire.id;
        entity.age = wire.age;
        entity.durability = wire.durability.min(self.registry.item(item).durability);
        entity.arcane_id = wire.arcane_id;
        Some(entity)
    }
}

fn resolve(
    registry: &Registry,
    blocks: &[String],
    items: &[String],
) -> (Vec<BlockId>, Vec<Option<ItemId>>) {
    (
        blocks
            .iter()
            .map(|name| registry.block_id(name).unwrap_or(registry.unknown_block))
            .collect(),
        items.iter().map(|name| registry.item_id(name)).collect(),
    )
}

#[cfg(test)]
#[path = "palette_tests.rs"]
mod tests;
