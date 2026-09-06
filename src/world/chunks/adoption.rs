//! Adoption chunks transaction coordination.

use super::EDIFICE_CLEARANCE;
use crate::chunk::CHUNK_X;
use crate::chunk::CHUNK_Y;
use crate::chunk::CHUNK_Z;
use crate::chunk::Chunk;
use crate::chunk::ChunkPos;
use crate::world::ChunkRead;
use crate::world::ChunkRevision;
use crate::world::World;

impl World {
    pub fn ensure_chunk(&mut self, pos: ChunkPos) -> bool {
        match self.try_ensure_chunk(pos) {
            Ok(adopted) => adopted,
            Err(error) => {
                eprintln!("world: cannot read chunk {pos:?}; saved data left intact: {error}");
                false
            }
        }
    }

    /// Prepare synchronously when entry requires residency and must retain
    /// the actual read failure. False means the chunk is already resident.
    pub(crate) fn try_ensure_chunk(&mut self, pos: ChunkPos) -> std::io::Result<bool> {
        if self.chunks.contains_key(&pos) {
            return Ok(false);
        }
        let (chunk, fresh) = self.prepare_chunk(pos, None)?;
        self.adopt_chunk(pos, chunk, fresh);
        Ok(true)
    }

    /// Adopt a chunk generated elsewhere (a background worker). A
    /// saved copy on disk always wins over the worker's fresh terrain,
    /// and an already-present chunk drops the offering — generation is
    /// pure, so a worker chunk equals what ensure_chunk would build.
    pub fn adopt_generated(&mut self, pos: ChunkPos, chunk: Chunk) -> bool {
        if self.chunks.contains_key(&pos) {
            return false;
        }
        let (chunk, fresh) = match self.prepare_chunk(pos, Some(chunk)) {
            Ok(prepared) => prepared,
            Err(error) => {
                eprintln!("world: cannot read chunk {pos:?}; saved data left intact: {error}");
                return false;
            }
        };
        self.adopt_chunk(pos, chunk, fresh);
        true
    }

    pub(super) fn prepare_chunk(
        &self,
        pos: ChunkPos,
        generated: Option<Chunk>,
    ) -> std::io::Result<(Chunk, bool)> {
        match self.try_load_chunk(pos)? {
            ChunkRead::Present(chunk) => Ok((chunk, false)),
            source @ (ChunkRead::Missing | ChunkRead::LegacyPlaceholder) => {
                if matches!(source, ChunkRead::LegacyPlaceholder) {
                    eprintln!("world: repairing legacy all-placeholder chunk {pos:?}");
                }
                Ok((
                    generated.unwrap_or_else(|| self.generator.generate(pos, &self.reg)),
                    true,
                ))
            }
        }
    }

    /// Reject prepared terrain invalidated by a save, including a chunk that
    /// has since been unloaded. The authoritative thread owns all writes.
    pub(crate) fn adopt_prepared_at_revision(
        &mut self,
        pos: ChunkPos,
        chunk: Chunk,
        fresh: bool,
        revision: &ChunkRevision,
    ) -> bool {
        if !revision.is_current(pos, &self.chunk_loader()) {
            return false;
        }
        self.adopt_prepared(pos, chunk, fresh)
    }

    /// Adopt only after the caller has validated the saved-terrain revision.
    pub(super) fn adopt_prepared(&mut self, pos: ChunkPos, chunk: Chunk, fresh: bool) -> bool {
        if self.chunks.contains_key(&pos) {
            return false;
        }
        self.adopt_chunk(pos, chunk, fresh);
        true
    }

    /// The main-thread half of chunk arrival: bedrock heal, insert,
    /// structures, wildlife, stamps, seam wake, light, reconcile.
    pub(super) fn adopt_chunk(&mut self, pos: ChunkPos, mut chunk: Chunk, fresh: bool) {
        // The Deep (capability E10) adopts bare: no bedrock floor, no
        // water seeding, no ruins, no country hearts, no ecology, and no
        // material reservations — a dungeon run is pure stamped rooms in
        // void, and its geography has no planetary cells to reconcile.
        if pos.face().is_deep() {
            self.chunks.insert(pos, chunk);
            return;
        }
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
        if fresh {
            self.commit_fresh_chunk_water(pos, &mut chunk);
        }
        self.chunks.insert(pos, chunk);
        if !fresh {
            self.apply_loaded_material_retrogen(pos);
        }
        self.apply_loaded_water_inboxes();
        // Ruins place once, at first generation; placement marks the chunk
        // modified so it saves and never regenerates.
        if fresh {
            self.seed_structures(pos);
        }
        self.reconcile_arcane_ecology_chunk(pos);
        self.reconcile_dross_scars_chunk(pos);
        // Reserve the final physical voxels. In particular, ruins can replace
        // host rock: reserving before their stamp left phantom ore underground.
        if let (Some(atlas), Some(ledger), Some(chunk)) = (
            &self.planet_atlas,
            &mut self.material_ledger,
            self.chunks.get(&pos),
        ) && let Err(error) = ledger.reserve_fresh_chunk(atlas, &self.reg, pos, chunk)
        {
            // Do not hide a manifest gap. The deterministic chunk remains
            // inspectable while audit reports the missing reservation.
            eprintln!("materials: failed to reserve chunk {pos:?}: {error}");
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
            for key in self.generator.province_keys_near(center, 24.0) {
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
                    let at =
                        crate::planet::BlockPos::new(site.face(), site.u(), base as u8, site.v())
                            .expect("heart base is inside the world");
                    self.register_heart(key, at);
                }
            }
        }
        // Wildlife rolls once per chunk, ever (the mark persists with the
        // world so hunted animals stay gone across sessions).
        if self.population.record_seeded(pos) {
            self.seed_wildlife(pos);
        }
        // A chunk seen for the first time is up to date; one loaded
        // from disk keeps its old stamp (the gap below reads it).
        let stamp = self.last_random.get(&pos).copied();
        self.last_random
            .entry(pos)
            .or_insert(self.calendar_state.clock());
        self.wake_seams(pos);
        // A chunk back from disk may hold water saved mid-flow (or
        // stranded by older, unsealed worldgen): set it settling again.
        if !fresh {
            self.wake_stale_fluids(pos);
        }
        self.relight_and_cascade(pos);
        // The world lived while this chunk was away: catch it up.
        if let Some(stamp) = stamp {
            let gap = self.calendar_state.clock() - stamp;
            if gap > 60.0 {
                self.reconcile_chunk(pos, gap);
                self.last_random.insert(pos, self.calendar_state.clock());
            }
        }
    }
}
