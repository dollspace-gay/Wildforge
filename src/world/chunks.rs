//! Chunk loading/generation, structures, loot, and remote chunk insertion.

use super::*;

/// How far above the surface a heart's own column is still searched
/// when the ledger goes looking for it. An edifice can bury the site
/// under courses of stone or lift a canopy over it; the spirit is
/// still down there.
const EDIFICE_CLEARANCE: i32 = 48;

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
        // A heart standing in this chunk joins the ledger. The site is
        // deterministic, so a chunk loaded from an old save registers
        // its country's spirit the same way a fresh one does — and a
        // stage already recorded wins (a dead heart stays dead).
        {
            let center = crate::planet::SurfacePos::new(
                pos.face(),
                pos.u() * CHUNK_X as u16 + CHUNK_X as u16 / 2,
                pos.v() * CHUNK_Z as u16 + CHUNK_Z as u16 / 2,
            )
            .expect("chunk center is canonical");
            let home = self.generator.province_at(center).key;
            let mut seen = Vec::new();
            for du in -2..=2 {
                for dv in -2..=2 {
                    let key = self.generator.province_offset(home, du, dv);
                    if seen.contains(&key) {
                        continue;
                    }
                    seen.push(key);
                    let site = self.generator.province_center_at(key);
                    if ChunkPos::from_surface(site) != pos {
                        continue;
                    }
                    // Find the site's base. A bole is solid, so the
                    // surface scan lands on its CROWN — walk down to
                    // the foot, which is the block the ledger keys on.
                    let is_heart = |w: &World, y: i32| {
                        crate::planet::BlockPos::new(site.face(), site.u(), y as u8, site.v())
                            .is_ok_and(|at| {
                                w.reg
                                    .block(w.get_block_at(at))
                                    .name
                                    .starts_with("base:heart_")
                            })
                    };
                    // Search a band around the surface rather than
                    // demanding the heart BE the surface block. Anything
                    // standing over the site — an edifice, or a roof a
                    // player put there — used to mean the country
                    // registered no heart at all: not a dead one, none.
                    // Wardens kept spawning and offerings kept being
                    // accepted while the whole arc quietly did not
                    // happen there.
                    let top = self.surface_height_at(site);
                    if let Some(crown) = (2..=(top + EDIFICE_CLEARANCE).min(CHUNK_Y as i32 - 1))
                        .rev()
                        .find(|&y| is_heart(self, y))
                    {
                        let mut base = crown;
                        while base > 1 && is_heart(self, base - 1) {
                            base -= 1;
                        }
                        let at = crate::planet::BlockPos::new(
                            site.face(),
                            site.u(),
                            base as u8,
                            site.v(),
                        )
                        .expect("heart base is inside the world");
                        self.register_heart(key, at);
                    }
                }
            }
        }
        // Wildlife rolls once per chunk, ever (the mark persists with the
        // world so hunted animals stay gone across sessions).
        if self.mob_seeded.insert(pos) {
            self.seed_wildlife(pos);
        }
        // A chunk seen for the first time is up to date; one loaded
        // from disk keeps its old stamp (the gap below reads it).
        let stamp = self.last_random.get(&pos).copied();
        self.last_random.entry(pos).or_insert(self.clock);
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
                self.last_random.insert(pos, self.clock);
            }
        }
    }

    // ---------------- ruins ----------------

    /// Deterministic per-chunk structure roll (at most one per chunk).
    pub(super) fn seed_structures(&mut self, pos: ChunkPos) {
        let reg = self.reg.clone();
        let center = crate::planet::SurfacePos::new(
            pos.face(),
            pos.u() * CHUNK_X as u16 + CHUNK_X as u16 / 2,
            pos.v() * CHUNK_Z as u16 + CHUNK_Z as u16 / 2,
        )
        .expect("chunk center is canonical");
        let biome = self.generator.biome_at(center).name().to_lowercase();
        for (si, st) in reg.structures.iter().enumerate() {
            if !st.biomes.contains(&biome) {
                continue;
            }
            let h = self.mob_hash_at(center, 9000 + si as u32);
            // The takers' cities stand in barren country because the
            // barrenness is the receipt: they are twice as common on
            // ground that stopped giving. (Worldgen cannot know which
            // hearts a player will kill, so it reads the signature —
            // exhausted, thin-soiled country — instead.)
            let barren = matches!(
                self.generator.biome_at(center),
                crate::worldgen::Biome::Badlands
                    | crate::worldgen::Biome::Scrubland
                    | crate::worldgen::Biome::Tundra
                    | crate::worldgen::Biome::Desert
            );
            let rarity = if barren {
                (st.rarity / 2).max(1)
            } else {
                st.rarity
            };
            if !h.is_multiple_of(rarity) {
                continue;
            }
            let w = st.layers[0].first().map(|r| r.len()).unwrap_or(0) as i32;
            let d = st.layers[0].len() as i32;
            if w == 0 || w > 14 || d > 14 {
                continue;
            }
            let origin_surface = crate::planet::SurfacePos::new(
                pos.face(),
                pos.u() * CHUNK_X as u16
                    + (1 + ((h >> 8) as i32).rem_euclid((15 - w).max(1))) as u16,
                pos.v() * CHUNK_Z as u16
                    + (1 + ((h >> 16) as i32).rem_euclid((15 - d).max(1))) as u16,
            )
            .expect("structure origin is inside its chunk");
            let sample = crate::planet::SurfacePos::canonicalized(
                origin_surface.face(),
                i32::from(origin_surface.u()) + w / 2,
                i32::from(origin_surface.v()) + d / 2,
            )
            .expect("structure center canonicalizes");
            let surface_y = self.surface_height_at(sample);
            if surface_y <= SEA_LEVEL + 1 || surface_y >= CHUNK_Y as i32 - 24 {
                continue;
            }
            let y0 = match st.buried {
                None => surface_y,
                Some((min, max)) => {
                    let depth = min + (h >> 4).rem_euclid((max - min + 1) as u32) as i32;
                    (surface_y - depth).max(6)
                }
            };
            let origin = BlockPos::new(
                origin_surface.face(),
                origin_surface.u(),
                y0 as u8,
                origin_surface.v(),
            )
            .expect("structure base is inside the world");
            self.place_structure_at(si, origin, h);
            break;
        }
    }

    /// Stamp a structure template into the world (clipped writes via
    /// set_block; chests get rolled loot and belong to the wild).
    pub fn place_structure_at(&mut self, si: usize, origin: BlockPos, seed: u32) {
        let reg = self.reg.clone();
        let Some(st) = reg.structures.get(si).cloned() else {
            return;
        };
        let chest_block = reg.block_id("base:chest");
        let mut rng = seed ^ 0x5f37_59df;
        for (ly, layer) in st.layers.iter().enumerate() {
            for (lz, row) in layer.iter().enumerate() {
                for (lx, ch) in row.chars().enumerate() {
                    let Some(pos) = origin.offset(lx as i32, ly as i32, lz as i32) else {
                        continue;
                    };
                    match ch {
                        '.' => {}
                        '~' => self.set_block_at(pos, AIR),
                        'C' => {
                            if let Some(cb) = chest_block {
                                self.set_block_at(pos, cb);
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
                                self.block_entities.insert(pos, BlockEntity::Chest(state));
                            }
                        }
                        c => {
                            if let Some(b) = st.palette.get(&c) {
                                self.set_block_at(pos, *b);
                            }
                        }
                    }
                }
            }
        }
        // Buried ruins leave a hint on the surface: a chimney stub.
        if st.buried.is_some()
            && let Some(cob) = reg.block_id("base:cobblestone")
            && let Some(hint) = origin.offset(1, 0, 1)
        {
            let sy = self.surface_height_at(hint.surface());
            if let Ok(base) = BlockPos::new(hint.face(), hint.u(), (sy + 1) as u8, hint.v()) {
                self.set_block_at(base, cob);
                if let Some(top) = base.offset(0, 1, 0) {
                    self.set_block_at(top, cob);
                }
            }
        }
    }

    #[cfg(test)]
    #[cfg(test)]
    pub fn place_structure(&mut self, si: usize, x: i32, y: i32, z: i32, seed: u32) {
        if let Some(origin) = BlockPos::of_world(x, y, z) {
            self.place_structure_at(si, origin, seed);
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
