//! Shared physical mining and held-placement operations.
//! Adapters admit reach, player overlap, input timing, and script requests.

use crate::inventory::ItemStack;
use crate::planet::BlockPos;
use crate::planet_atlas::WaterClass;
use crate::registry::{BlockId, ItemId, Registry};
use crate::world::{BlockBreak, World};

pub(crate) struct MinedBlock {
    pub(crate) result: BlockBreak,
    pub(crate) sheared: bool,
}

pub(crate) fn mine(world: &mut World, position: BlockPos, held: Option<ItemId>, creative: bool) -> Option<MinedBlock> {
    let block = world.get_block_at(position);
    let sheared = held.is_some_and(|item| world.reg.item(item).shears)
        && world.reg.block(block).name.contains("leaves");
    let result = world.break_block_at(position, held, !creative && !sheared, !creative)?;
    Some(MinedBlock { result, sheared })
}

#[derive(Clone, Copy)]
pub(crate) enum Placement {
    Block(BlockId),
    Water(WaterClass),
    Lava(BlockId),
}

impl Placement {
    /// Resolve the same held item for both physical and bucket placement.
    pub(crate) fn from_stack(registry: &Registry, selected: Option<ItemStack>) -> Option<Self> {
        let stack = selected?;
        let item = Some(stack.item);
        if item == registry.item_id("base:bucket_water") {
            Some(Self::Water(WaterClass::Fresh))
        } else if item == registry.item_id("base:bucket_brackish") {
            Some(Self::Water(WaterClass::Brackish))
        } else if item == registry.item_id("base:bucket_salt") {
            Some(Self::Water(WaterClass::Salt))
        } else if item == registry.item_id("base:bucket_lava") {
            Some(Self::Lava(registry.item(stack.item).places.unwrap_or_else(|| registry.lava_for_volume(8))))
        } else {
            registry.item(stack.item).places.map(Self::Block)
        }
    }

    pub(crate) fn is_bucket(self) -> bool { matches!(self, Self::Water(_) | Self::Lava(_)) }

    /// Effects occur before the adapter spends inventory. The world remains
    /// the authority for material journals, water custody, and block side effects.
    pub(crate) fn apply(self, world: &mut World, position: BlockPos, selected: Option<ItemStack>, creative: bool) -> bool {
        match self {
            Self::Water(class) => world.place_portable_water_at(position, class),
            Self::Lava(block) => world.place_block_at(position, block),
            Self::Block(block) if creative => world.place_block_at(position, block),
            Self::Block(_) => selected.is_some_and(|stack| world.place_item_block_at(position, stack)),
        }
    }
}
