//! Deterministic pipes, geodes, and data-defined ore deposits.

use super::Generator;
use crate::chunk::{CHUNK_X, CHUNK_Y, CHUNK_Z, Chunk, ChunkPos};
use crate::registry::AIR;
use std::collections::HashMap;
use super::{GeodeBand, geode_band_at, hash2};
use crate::registry::Registry;

impl Generator {


    /// A kimberlite pipe: a carrot of deep rock punched up through
    /// every stratum — wide near the top, a thread at depth. Most are
    /// blind (topped below the surface, found by mining); the ones
    /// that breach weather into a blue-ground stain, the prospector's
    /// tell. Diamonds only ever live inside these (the ore feature
    /// replaces kimberlite and nothing else).
    pub(super) fn plant_pipe(&self, c: &mut Chunk, pos: ChunkPos, heights: &[[i32; CHUNK_Z]; CHUNK_X]) {
        let Some((cx, cz, breach)) = self.pipe_at(pos) else {
            return;
        };
        let h = self.chunk_hash(0x8d1a, pos);
        let surf = heights[cx][cz];
        let top_y = if breach {
            surf
        } else {
            (surf - 6 - ((h >> 26) % 12) as i32).max(20)
        };
        for y in 2..=top_y {
            let t = y as f32 / top_y as f32;
            let r = 1.2 + t * t * 3.6;
            let ri = r.ceil() as i32;
            for dx in -ri..=ri {
                for dz in -ri..=ri {
                    if ((dx * dx + dz * dz) as f32) > r * r {
                        continue;
                    }
                    let (lx, lz) = (cx as i32 + dx, cz as i32 + dz);
                    if !(0..CHUNK_X as i32).contains(&lx) || !(0..CHUNK_Z as i32).contains(&lz) {
                        continue;
                    }
                    if self.is_rock(c.get(lx as usize, y as usize, lz as usize)) {
                        c.set(lx as usize, y as usize, lz as usize, self.kimberlite);
                    }
                }
            }
        }
        if breach {
            // Blue ground: the weathered pipe stains the topsoil.
            for dx in -5i32..=5 {
                for dz in -5i32..=5 {
                    let (lx, lz) = (cx as i32 + dx, cz as i32 + dz);
                    if !(0..CHUNK_X as i32).contains(&lx) || !(0..CHUNK_Z as i32).contains(&lz) {
                        continue;
                    }
                    if dx * dx + dz * dz <= 20
                        && hash2(
                            self.seed ^ 0xb1e ^ (pos.face() as u32).wrapping_mul(0x9e37_79b9),
                            pos.u() as i32 * 16 + lx,
                            pos.v() as i32 * 16 + lz,
                        )
                        .is_multiple_of(2)
                    {
                        let top = heights[lx as usize][lz as usize];
                        if top > 0 {
                            c.set(lx as usize, top as usize, lz as usize, self.kimberlite);
                        }
                    }
                }
            }
        }
    }



    /// A limestone geode: a rough quartz shell around an amethyst
    /// lining around a void — crack one open with a torch in hand.
    pub(super) fn plant_geode(&self, c: &mut Chunk, pos: ChunkPos) {
        let Some((cx, cz, cy, r)) = self.geode_at(pos) else {
            return;
        };
        // Only real limestone country hosts them.
        let heart = c.get(cx, cy as usize, cz);
        if heart != self.limestone && heart != self.marble {
            return;
        }
        let h = self.chunk_hash(0x6e0d, pos);
        for dx in -r..=r {
            for dy in -r..=r {
                for dz in -r..=r {
                    let d2 = dx * dx + dy * dy + dz * dz;
                    if d2 > r * r {
                        continue;
                    }
                    let (lx, y, lz) = (cx as i32 + dx, cy + dy, cz as i32 + dz);
                    if !(0..CHUNK_X as i32).contains(&lx)
                        || !(0..CHUNK_Z as i32).contains(&lz)
                        || y < 2
                        || y >= CHUNK_Y as i32 - 1
                    {
                        continue;
                    }
                    if !self.is_rock(c.get(lx as usize, y as usize, lz as usize)) {
                        continue;
                    }
                    // Include the exact inner-radius lattice. With a strict
                    // inequality, the nominal one-cell shell breaks into
                    // disconnected islands for valid integer radii (most
                    // dramatically at r=4). The inclusive boundary is the
                    // smallest correction that makes every production
                    // radius 3..=5 quartz shell six-connected.
                    let b = match geode_band_at(d2, r)
                        .expect("the geode loop already clipped the outer radius")
                    {
                        GeodeBand::Shell => self.quartz_block,
                        GeodeBand::Lining => {
                            if hash2(h, dx * 31 + dy, dz * 17 + dy).is_multiple_of(3) {
                                self.quartz_block
                            } else {
                                self.amethyst_block
                            }
                        }
                        GeodeBand::Heart => AIR,
                    };
                    c.set(lx as usize, y as usize, lz as usize, b);
                }
            }
        }
    }

    /// Data-driven ore veins from mod features, deterministic per chunk.
    pub(super) fn plant_ores(&self, c: &mut Chunk, pos: ChunkPos, reg: &Registry) {
        let mut materialized = HashMap::<crate::planet_atlas::MineralKind, u32>::new();
        for (fi, ore) in reg.ores.iter().enumerate() {
            let post_creation_mod = ore.mod_id != "base"
                && self
                    .atlas
                    .as_ref()
                    .is_some_and(|atlas| atlas.manifest.content_hash != reg.content_hash);
            if post_creation_mod
                && ore.retrogen != crate::registry::RetrogenPolicy::UntouchedHostOnly
            {
                continue;
            }
            let name = &reg.block(ore.block).name;
            let mineral = crate::planet_atlas::MineralKind::from_block_name(name);
            let allowance = self
                .atlas
                .as_ref()
                .map_or(u32::MAX, |atlas| atlas.deposit_allowance(pos, mineral));
            if allowance == 0 {
                continue;
            }
            let gangue = name.contains("quartz_vein");
            let mut rng = self.chunk_hash((fi as u32).wrapping_mul(0x9e37), pos);
            let mut next = || {
                rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                rng >> 8
            };
            for _ in 0..ore.per_chunk {
                if ore.chance < 1.0 && (next() as f32 / (1 << 24) as f32) >= ore.chance {
                    continue;
                }
                let (mut x, mut y, mut z) = (
                    (next() % CHUNK_X as u32) as i32,
                    ore.y_min + (next() % (ore.y_max - ore.y_min).max(1) as u32) as i32,
                    (next() % CHUNK_Z as u32) as i32,
                );
                for _ in 0..ore.vein_size {
                    if x >= 0
                        && x < CHUNK_X as i32
                        && y > 0
                        && y < CHUNK_Y as i32
                        && z >= 0
                        && z < CHUNK_Z as i32
                        && c.get(x as usize, y as usize, z as usize) == ore.replaces
                        && (gangue || materialized.get(&mineral).copied().unwrap_or(0) < allowance)
                    {
                        c.set(x as usize, y as usize, z as usize, ore.block);
                        if !gangue {
                            *materialized.entry(mineral).or_default() += 1;
                        }
                    }
                    match ore.shape {
                        // Round pockets: drift any direction.
                        crate::registry::VeinShape::Walk => match next() % 6 {
                            0 => x += 1,
                            1 => x -= 1,
                            2 => y += 1,
                            3 => y -= 1,
                            4 => z += 1,
                            _ => z -= 1,
                        },
                        // Flat lenses: spread wide, climb grudgingly.
                        crate::registry::VeinShape::Seam => match next() % 9 {
                            0 | 1 => x += 1,
                            2 | 3 => x -= 1,
                            4 | 5 => z += 1,
                            6 | 7 => z -= 1,
                            _ => y += if next() % 2 == 0 { 1 } else { -1 },
                        },
                        // Near-vertical streaks: climb hard, wander little.
                        crate::registry::VeinShape::Streak => match next() % 6 {
                            0..=2 => y += 1,
                            3 => y -= 1,
                            4 => x += if next() % 2 == 0 { 1 } else { -1 },
                            _ => z += if next() % 2 == 0 { 1 } else { -1 },
                        },
                    }
                }
            }
        }
    }

}
