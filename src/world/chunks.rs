//! Chunk loading/generation, structures, loot, and remote chunk insertion.

use super::*;

impl World {
    pub fn ensure_chunk(&mut self, pos: ChunkPos) -> bool {
        if self.chunks.contains_key(&pos) {
            return false;
        }
        if self.remote {
            return false; // guests receive chunks, they don't make them
        }
        let loaded = self.try_load_chunk(pos);
        let fresh = loaded.is_none();
        let chunk = loaded.unwrap_or_else(|| self.generator.generate(pos, &self.reg));
        self.adopt_chunk(pos, chunk, fresh);
        true
    }

    /// Adopt a chunk generated elsewhere (a background worker). A
    /// saved copy on disk always wins over the worker's fresh terrain,
    /// and an already-present chunk drops the offering — generation is
    /// pure, so a worker chunk equals what ensure_chunk would build.
    pub fn adopt_generated(&mut self, pos: ChunkPos, chunk: Chunk) -> bool {
        if self.chunks.contains_key(&pos) || self.remote {
            return false;
        }
        if let Some(saved) = self.try_load_chunk(pos) {
            self.adopt_chunk(pos, saved, false);
        } else {
            self.adopt_chunk(pos, chunk, true);
        }
        true
    }

    /// The main-thread half of chunk arrival: bedrock heal, insert,
    /// structures, wildlife, stamps, seam wake, light, reconcile.
    fn adopt_chunk(&mut self, pos: ChunkPos, mut chunk: Chunk, fresh: bool) {
        // The floor reseals on load: any hole in the bedrock (a
        // creative dig, an old bug) heals when the chunk comes back.
        // Idempotent — set() doesn't mark the chunk modified.
        if let Some(root) = self.reg.block_id("base:bedrock") {
            for lx in 0..CHUNK_X {
                for lz in 0..CHUNK_Z {
                    if chunk.get(lx, 0, lz) != root {
                        chunk.set(lx, 0, lz, root);
                    }
                }
            }
        }
        self.chunks.insert(pos, chunk);
        // Ruins place once, at first generation; placement marks the chunk
        // modified so it saves and never regenerates.
        if fresh {
            self.seed_structures(pos);
        }
        // Wildlife rolls once per chunk, ever (the mark persists with the
        // world so hunted animals stay gone across sessions).
        if self.mob_seeded.insert((pos.x, pos.z)) {
            self.seed_wildlife(pos);
        }
        // A chunk seen for the first time is up to date; one loaded
        // from disk keeps its old stamp (the gap below reads it).
        let stamp = self.last_random.get(&(pos.x, pos.z)).copied();
        self.last_random.entry((pos.x, pos.z)).or_insert(self.clock);
        self.wake_seams(pos);
        // A chunk back from disk may hold water saved mid-flow (or
        // stranded by older, unsealed worldgen): set it settling again.
        if !fresh {
            self.wake_stale_fluids(pos);
        }
        self.relight_and_cascade(pos);
        // The world lived while this chunk was away: catch it up.
        if let Some(stamp) = stamp {
            let gap = self.clock - stamp;
            if gap > 60.0 {
                self.reconcile_chunk(pos, gap);
                self.last_random.insert((pos.x, pos.z), self.clock);
            }
        }
    }

    // ---------------- ruins ----------------

    /// Deterministic per-chunk structure roll (at most one per chunk).
    pub(super) fn seed_structures(&mut self, pos: ChunkPos) {
        let reg = self.reg.clone();
        let (cx, cz) = (pos.x * CHUNK_X as i32, pos.z * CHUNK_Z as i32);
        let biome = self.generator.biome(cx + 8, cz + 8).name().to_lowercase();
        for (si, st) in reg.structures.iter().enumerate() {
            if !st.biomes.contains(&biome) {
                continue;
            }
            let h = self.mob_hash(pos.x, pos.z, 9000 + si as u32);
            if !h.is_multiple_of(st.rarity) {
                continue;
            }
            let w = st.layers[0].first().map(|r| r.len()).unwrap_or(0) as i32;
            let d = st.layers[0].len() as i32;
            if w == 0 || w > 14 || d > 14 {
                continue;
            }
            let ox = cx + 1 + ((h >> 8) as i32).rem_euclid((15 - w).max(1));
            let oz = cz + 1 + ((h >> 16) as i32).rem_euclid((15 - d).max(1));
            let surface = self.surface_height(ox + w / 2, oz + d / 2);
            if surface <= SEA_LEVEL + 1 || surface >= CHUNK_Y as i32 - 24 {
                continue;
            }
            let y0 = match st.buried {
                None => surface,
                Some((min, max)) => {
                    let depth = min + (h >> 4).rem_euclid((max - min + 1) as u32) as i32;
                    (surface - depth).max(6)
                }
            };
            self.place_structure(si, ox, y0, oz, h);
            break;
        }
    }

    /// Stamp a structure template into the world (clipped writes via
    /// set_block; chests get rolled loot and belong to the wild).
    pub fn place_structure(&mut self, si: usize, x0: i32, y0: i32, z0: i32, seed: u32) {
        let reg = self.reg.clone();
        let Some(st) = reg.structures.get(si).cloned() else {
            return;
        };
        let chest_block = reg.block_id("base:chest");
        let mut rng = seed ^ 0x5f37_59df;
        for (ly, layer) in st.layers.iter().enumerate() {
            for (lz, row) in layer.iter().enumerate() {
                for (lx, ch) in row.chars().enumerate() {
                    let (x, y, z) = (x0 + lx as i32, y0 + ly as i32, z0 + lz as i32);
                    match ch {
                        '.' => {}
                        '~' => self.set_block(x, y, z, AIR),
                        'C' => {
                            if let Some(cb) = chest_block {
                                self.set_block(x, y, z, cb);
                                let mut state = ChestState {
                                    wild_owned: true,
                                    ..Default::default()
                                };
                                if let Some(table) = &st.loot {
                                    let n = 3 + (rng % 3) as usize;
                                    for (i, stck) in self
                                        .roll_loot(table, n as u32, &mut rng)
                                        .into_iter()
                                        .enumerate()
                                    {
                                        if i < CHEST_SLOTS {
                                            // Scatter through the chest.
                                            let slot = (i * 7 + (rng % 5) as usize) % CHEST_SLOTS;
                                            state.slots[slot] = Some(stck);
                                        }
                                    }
                                }
                                self.block_entities
                                    .insert((x, y, z), BlockEntity::Chest(state));
                            }
                        }
                        c => {
                            if let Some(b) = st.palette.get(&c) {
                                self.set_block(x, y, z, *b);
                            }
                        }
                    }
                }
            }
        }
        // Buried ruins leave a hint on the surface: a chimney stub.
        if st.buried.is_some()
            && let Some(cob) = reg.block_id("base:cobblestone")
        {
            let hx = x0 + 1;
            let hz = z0 + 1;
            let sy = self.surface_height(hx, hz);
            self.set_block(hx, sy + 1, hz, cob);
            self.set_block(hx, sy + 2, hz, cob);
        }
    }

    /// Weighted rolls from a loot table.
    pub fn roll_loot(&self, table: &str, rolls: u32, rng: &mut u32) -> Vec<ItemStack> {
        let Some(entries) = self.reg.loots.get(table) else {
            return Vec::new();
        };
        let total: u32 = entries.iter().map(|e| e.weight).sum();
        if total == 0 {
            return Vec::new();
        }
        let mut out = Vec::new();
        for _ in 0..rolls {
            *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
            let mut pick = (*rng >> 8) % total;
            for e in entries {
                if pick < e.weight {
                    *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                    let span = e.count.1.saturating_sub(e.count.0) + 1;
                    let n = e.count.0 + (*rng >> 8) % span;
                    let mut stack = ItemStack::new(&self.reg, e.item, n.max(1));
                    if let Some(frac) = e.durability_frac {
                        let max = self.reg.item(e.item).durability;
                        if max > 0 {
                            stack.durability = ((max as f32 * frac) as u32).max(1);
                        }
                    }
                    out.push(stack);
                    break;
                }
                pick -= e.weight;
            }
        }
        out
    }

    // ---------------- gravity blocks ----------------
}
