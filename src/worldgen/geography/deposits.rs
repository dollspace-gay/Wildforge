//! Pure deposit locations and dimensions; materialization stays in generation.

use super::Geography;
use crate::chunk::ChunkPos;

impl Geography {
    /// The kimberlite pipe rolled for a chunk, if any: (local cx, cz,
    /// breaches_surface). Roughly one chunk in four hundred; the pipe
    /// fits inside its chunk's footprint by construction.
    pub fn pipe_at(&self, pos: ChunkPos) -> Option<(usize, usize, bool)> {
        if let Some(atlas) = &self.atlas {
            let site =
                atlas.deposit_center_in_chunk(pos, crate::planet_atlas::MineralKind::Diamond)?;
            let hash = self.chunk_hash(0x8d1a ^ site.id, pos);
            return Some((
                6 + ((hash >> 8) % 5) as usize,
                6 + ((hash >> 16) % 5) as usize,
                (hash >> 24) % 10 < 3,
            ));
        }
        let h = self.chunk_hash(0x8d1a, pos);
        // Treasure-band rarity (economy plan): a pipe is a multi-km
        // expedition and a famous site, not a backyard curiosity —
        // median nearest ~2.3 km (was 1/397, ~150 blocks).
        if !h.is_multiple_of(90_000) {
            return None;
        }
        let cx = 6 + ((h >> 8) % 5) as usize;
        let cz = 6 + ((h >> 16) % 5) as usize;
        Some((cx, cz, (h >> 24) % 10 < 3))
    }

    /// The geode rolled for a chunk, if any: (local cx, cz, cy, r).
    pub fn geode_at(&self, pos: ChunkPos) -> Option<(usize, usize, i32, i32)> {
        if let Some(atlas) = &self.atlas {
            let site =
                atlas.deposit_center_in_chunk(pos, crate::planet_atlas::MineralKind::Geode)?;
            let hash = self.chunk_hash(0x6e0d ^ site.id, pos);
            let raw_y = 46 + ((hash >> 24) % 26) as i32;
            let ground = atlas
                .genesis
                .ground
                .get(site.pos)
                .expect("a validated geode deposit has a ground cell");
            // The mining path materializes finite groundwater whenever a
            // player opens permeable rock below the head. Keep the same
            // deterministic depth roll, but lift a geode's discovery band
            // just above that immutable head when the host is an active
            // aquifer. If the dry band would breach the terrain, plant_geode
            // naturally rejects it because its heart is no longer host rock.
            let dry_y = if ground.aquifer_permeability >= 8_192 {
                ground.baseline_groundwater_head.ceil() as i32 + 1
            } else {
                raw_y
            };
            return Some((
                5 + ((hash >> 8) % 7) as usize,
                5 + ((hash >> 16) % 7) as usize,
                raw_y.max(dry_y),
                3 + ((hash >> 5) % 3) as i32,
            ));
        }
        let h = self.chunk_hash(0x6e0d, pos);
        // Uncommon local luxury (economy plan): median nearest ~200
        // blocks (was 1/89, ~70).
        if !h.is_multiple_of(700) {
            return None;
        }
        let cx = 5 + ((h >> 8) % 7) as usize;
        let cz = 5 + ((h >> 16) % 7) as usize;
        let cy = 46 + ((h >> 24) % 26) as i32;
        let r = 3 + ((h >> 5) % 3) as i32;
        Some((cx, cz, cy, r))
    }
}
