//! Material retrogen chunks transaction coordination.

use crate::chunk::CHUNK_X;
use crate::chunk::CHUNK_Y;
use crate::chunk::CHUNK_Z;
use crate::chunk::ChunkPos;
use crate::world::World;

impl World {
    pub(super) fn apply_loaded_material_retrogen(&mut self, pos: ChunkPos) {
        if !self.chunks.contains_key(&pos) {
            return;
        }
        // Any authored edit, structure stamp, or block entity makes the whole
        // chunk ineligible. This is deliberately conservative: host rock is
        // plentiful; player trust is not.
        if self.player_touched.contains(&pos)
            || self.structure_chunks.contains(&pos)
            || self.installations.keys().any(|at| at.chunk() == pos)
        {
            return;
        }
        let pending = self
            .reg
            .ores
            .iter()
            .filter(|ore| ore.mod_id != "base")
            .filter(|ore| {
                self.material_ledger
                    .as_ref()
                    .is_some_and(|ledger| ledger.retrogen_pending_for(&ore.resource_key, pos))
            })
            .map(|ore| (ore.resource_key.clone(), ore.block, ore.replaces))
            .collect::<Vec<_>>();
        if pending.is_empty() {
            return;
        }
        let reference = self.generator.generate(pos, &self.reg);
        let mut changed = false;
        if let Some(chunk) = self.chunks.get_mut(&pos) {
            for (_, ore_block, host) in &pending {
                for y in 1..CHUNK_Y {
                    for z in 0..CHUNK_Z {
                        for x in 0..CHUNK_X {
                            if reference.get(x, y, z) == *ore_block && chunk.get(x, y, z) == *host {
                                chunk.set(x, y, z, *ore_block);
                                changed = true;
                            }
                        }
                    }
                }
            }
            if changed {
                chunk.modified = true;
                chunk.dirty = true;
            }
        }
        // Voxel first, ledger marker second. A crash between them simply
        // reruns the deterministic pass; already replaced ore is unchanged.
        if changed && let Err(error) = self.save_chunk(pos) {
            eprintln!("materials: retrogen chunk write failed for {pos:?}: {error}");
            return;
        }
        if let Some(ledger) = &mut self.material_ledger
            && let Err(error) = ledger.mark_retrogen_chunk(
                pending.into_iter().map(|(resource_key, _, _)| resource_key),
                pos,
            )
        {
            eprintln!("materials: retrogen marker write failed for {pos:?}: {error}");
        }
    }
}
