//! Separate observation and mutation capabilities for spatial machine stores.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

use super::BlockEntity;
use crate::inventory::ItemStack;
use crate::planet::BlockPos;
use crate::planet_atlas::LocalWeatherSample;
use crate::registry::{BlockId, Registry};

/// Any store of blocks that can host a multiblock machine: the main
/// [`crate::world::World`] (its [`crate::planet::BlockPos`]es on the chunk
/// grid) or a
/// [`crate::world::local_structure::LocalStructure`] (canonical local-offset
/// tuples). `match_shape`, the per-kind `validate`/`edit_region`
/// recognizers use this read contract. Machine ticks require its BlockStore
/// mutation extension, sharing recognition and firing logic between the world
/// and local structures (spec Part 2.1 groundwork).
///
/// ## Structure chronology
///
/// Prior to Part 2.1 groundwork, block behavior lived in `World`, addressed
/// by [`BlockPos`], full stop. This trait is the seam that lets the same
/// shape system and the same per-kind tick logic operate against a
/// `LocalStructure`'s own store without forking a copy.
pub trait BlockRead {
    /// A position within this store.
    type Pos: Copy + Eq + Hash;

    /// The block at `pos`, or `AIR` when the cell is empty or out of reach.
    fn get_block(&self, pos: Self::Pos) -> BlockId;

    /// The position reached by offsetting `pos` by `d`, or `None` when the
    /// offset leaves the store (the world's finite shell).
    fn offset(&self, pos: Self::Pos, d: (i32, i32, i32)) -> Option<Self::Pos>;

    /// Cell-space displacement from `from` to `to`. `None` when the two
    /// positions are not comparable (different world faces).
    fn cell_delta(&self, from: Self::Pos, to: Self::Pos) -> Option<(i32, i32, i32)>;

    /// The block-entity map, keyed by store position.
    fn block_entities(&self) -> &HashMap<Self::Pos, BlockEntity>;

    /// The registry blocks and items in this store resolve against (shared
    /// with the host world, so stores borrow its registry without a copy).
    fn reg(&self) -> &Arc<Registry>;

    /// The main-world [`BlockPos`] corresponding to a store position. The
    /// world maps a position to itself; a structure resolves the local offset
    /// through its transform.
    fn to_world(&self, pos: Self::Pos) -> Option<BlockPos>;

    /// Whether the cell three above `core` (a world position) is open sky.
    /// The world reads its light map; a chunkless structure has no light
    /// model, so it reports open sky (its machines are always unroofed).
    fn open_sky_above(&self, core: BlockPos) -> bool;

    /// The local weather at `at` (a world position). Structures always
    /// report fair weather: a structure-hosted machine is exempt from the
    /// world's storm dousing.
    fn weather_at(&self, at: BlockPos) -> LocalWeatherSample;

}

/// A machine store that can commit physical changes. Recognition and rendering
/// require only BlockRead; replicas never implement this mutation capability.
pub trait BlockStore: BlockRead {
    fn block_entities_mut(&mut self) -> &mut HashMap<Self::Pos, BlockEntity>;

    /// Swap the block at `pos` for `block_name`'s id, preserving any block
    /// entity living there.
    fn swap_block_keep_entity(&mut self, pos: Self::Pos, block_name: &str);

    /// The material ledger a store participates in, or `None` for stores
    /// without one. Structure-hosted machines are exempt from the main
    /// world's economy accounting by design (the brief's open ire/ledger
    /// question, answered as "exempt" — there is no structure-local
    /// economy to record against).
    fn material_ledger(&mut self) -> Option<&mut crate::materials::MaterialLedger>;

    /// Deliver a produced output item at a world position. The world spawns
    /// a loose drop there; a structure collects it into its own outbox (it
    /// has no loose-item world of its own, so completion stays observable
    /// via `LocalStructure.outbox`).
    fn push_drop_at(&mut self, at: crate::planet::BlockPos, stack: ItemStack);
}
