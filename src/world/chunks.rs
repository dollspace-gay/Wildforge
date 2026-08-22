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

    /// Adopt a worker result whose saved-vs-generated decision was already
    /// made off-thread. Unlike `adopt_generated`, this path performs no cold
    /// disk read and is safe inside a live host/client pump.
    pub fn adopt_prepared(&mut self, pos: ChunkPos, chunk: Chunk, fresh: bool) -> bool {
        if self.chunks.contains_key(&pos) || self.remote {
            return false;
        }
        self.adopt_chunk(pos, chunk, fresh);
        true
    }

    /// The main-thread half of chunk arrival: bedrock heal, insert,
    /// structures, wildlife, stamps, seam wake, light, reconcile.
    fn adopt_chunk(&mut self, pos: ChunkPos, mut chunk: Chunk, fresh: bool) {
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

    /// Materialize the one sparse representative block for each persistent
    /// ecology site in this chunk. A saved site wins over generation order;
    /// player-touched terrain wins over retrogen/materialization.
    fn reconcile_arcane_ecology_chunk(&mut self, chunk: ChunkPos) {
        let Some(geography) = self.arcane_geography.as_ref() else {
            return;
        };
        let candidates = geography
            .dynamic
            .ecology
            .sites
            .iter()
            .filter_map(|site| {
                let surface = site.surface()?;
                (ChunkPos::from_surface(surface) == chunk).then(|| {
                    (
                        site.id,
                        site.content_id.clone(),
                        surface,
                        site.materialized_y,
                        site.stage,
                        site.ownership,
                        site.charge_total(),
                    )
                })
            })
            .collect::<Vec<_>>();
        for (site_id, content_id, surface, saved_y, stage, ownership, charge) in candidates {
            let Some(block) = self.reg.block_id(&content_id) else {
                continue;
            };
            let state_variant = (content_id == "base:lantern_reed")
                .then(|| self.reg.block_id("base:lantern_reed_dim"))
                .flatten();
            let display_block = if charge == 0 {
                state_variant.unwrap_or(block)
            } else {
                block
            };
            let y = if saved_y != 0 {
                i32::from(saved_y)
            } else {
                if ownership != crate::arcane_ecology::EcologyOwnership::Cultivated
                    && self.player_touched.contains(&chunk)
                {
                    continue;
                }
                let definition = self.reg.block(block).arcane_ecology.as_ref();
                let cave = definition.is_some_and(|definition| {
                    definition
                        .habitat
                        .iter()
                        .any(|tag| matches!(tag.as_str(), "cave" | "moist_cave" | "subsurface"))
                });
                let top = self.surface_height_at(surface).clamp(2, CHUNK_Y as i32 - 2);
                if cave {
                    (2..top)
                        .rev()
                        .find(|height| {
                            let at = crate::planet::BlockPos::new(
                                surface.face(),
                                surface.u(),
                                *height as u8,
                                surface.v(),
                            )
                            .expect("ecology cave candidate is inside shell");
                            at.offset(0, -1, 0).is_some_and(|below| {
                                self.get_block_at(at) == crate::registry::AIR
                                    && self.reg.is_solid(self.get_block_at(below))
                            })
                        })
                        .unwrap_or(top + 1)
                } else {
                    top + 1
                }
            };
            if !(1..CHUNK_Y as i32 - 1).contains(&y) {
                continue;
            }
            let at =
                crate::planet::BlockPos::new(surface.face(), surface.u(), y as u8, surface.v())
                    .expect("ecology materialization position is canonical");
            let present = self.get_block_at(at);
            let absent = matches!(
                stage,
                crate::arcane_ecology::EcologyStage::Collapsed
                    | crate::arcane_ecology::EcologyStage::Harvested
                    | crate::arcane_ecology::EcologyStage::Dormant
            );
            if absent {
                if present == block || state_variant == Some(present) {
                    self.set_block_at(at, crate::registry::AIR);
                }
                continue;
            }
            if present != display_block {
                // Ecological representatives never overwrite water, ordinary
                // plants, ruins, or player blocks. A skipped natural site can
                // recover after habitat repair; it does not reroll elsewhere.
                if present != crate::registry::AIR
                    && present != block
                    && state_variant != Some(present)
                {
                    continue;
                }
                self.set_block_at(at, display_block);
            }
            if let Some(site) = self.arcane_geography.as_mut().and_then(|geography| {
                geography
                    .dynamic
                    .ecology
                    .sites
                    .iter_mut()
                    .find(|site| site.id == site_id)
            }) {
                site.materialized_y = y as u8;
            }
        }
    }

    pub(super) fn refresh_loaded_arcane_ecology(&mut self) {
        let chunks = self.chunks.keys().copied().collect::<Vec<_>>();
        for chunk in chunks {
            self.reconcile_arcane_ecology_chunk(chunk);
        }
    }

    fn reconcile_dross_scars_chunk(&mut self, chunk: ChunkPos) {
        let Some(atlas) = self.planet_atlas.as_ref() else {
            return;
        };
        let Some(geography) = self.arcane_geography.as_ref() else {
            return;
        };
        let candidates = geography
            .dynamic
            .dross_state
            .scars
            .values()
            .filter(|site| site.resolved_step.is_none())
            .filter_map(|site| {
                let center = site.region.center(atlas.side());
                let surface = crate::planet::SurfacePos::new(
                    center.face,
                    center
                        .u
                        .floor()
                        .clamp(0.0, f64::from(crate::planet::FACE_BLOCKS - 1))
                        as u16,
                    center
                        .v
                        .floor()
                        .clamp(0.0, f64::from(crate::planet::FACE_BLOCKS - 1))
                        as u16,
                )
                .ok()?;
                const SLOT_OFFSETS: [(u16, u16); crate::dross::MAX_ACTIVE_SCARS_PER_REGION] = [
                    (0, 0),
                    (4, 0),
                    (12, 0),
                    (0, 4),
                    (0, 12),
                    (4, 4),
                    (12, 12),
                    (4, 12),
                ];
                let origin = ChunkPos::from_surface(surface).block_origin();
                let (offset_u, offset_v) = SLOT_OFFSETS[usize::from(site.site_slot)];
                let local_u = (surface.u() % CHUNK_X as u16 + offset_u) % CHUNK_X as u16;
                let local_v = (surface.v() % CHUNK_Z as u16 + offset_v) % CHUNK_Z as u16;
                let surface = crate::planet::SurfacePos::new(
                    origin.face(),
                    origin.u().saturating_add(local_u),
                    origin.v().saturating_add(local_v),
                )
                .ok()?;
                let owned_chunk = site
                    .materialized_at
                    .map_or_else(|| ChunkPos::from_surface(surface), BlockPos::chunk);
                (owned_chunk == chunk).then_some((
                    site.id,
                    site.kind,
                    site.content_id.clone(),
                    surface,
                    site.materialized_at,
                ))
            })
            .collect::<Vec<_>>();
        for (scar_id, kind, content_id, surface, saved) in candidates {
            if saved.is_none() && self.player_touched.contains(&chunk) {
                // A persisted scar selected before the chunk was authored may
                // still be waiting to materialize. The later construction
                // record wins; loading the chunk must not bypass that history.
                continue;
            }
            let Some((block, handler)) = self
                .reg
                .resolve_dross_scar(&content_id, kind)
                .map(|definition| (definition.block, definition.handler))
            else {
                continue;
            };
            let at = if let Some(saved) = saved {
                saved
            } else {
                const LOCAL_OFFSETS: [(i32, i32); 13] = [
                    (0, 0),
                    (1, 0),
                    (0, 1),
                    (-1, 0),
                    (0, -1),
                    (1, 1),
                    (-1, 1),
                    (-1, -1),
                    (1, -1),
                    (2, 0),
                    (0, 2),
                    (-2, 0),
                    (0, -2),
                ];
                let mut selected = None;
                for (du, dv) in LOCAL_OFFSETS {
                    let u = i32::from(surface.u()) + du;
                    let v = i32::from(surface.v()) + dv;
                    if !(0..i32::from(crate::planet::FACE_BLOCKS)).contains(&u)
                        || !(0..i32::from(crate::planet::FACE_BLOCKS)).contains(&v)
                    {
                        continue;
                    }
                    let candidate =
                        crate::planet::SurfacePos::new(surface.face(), u as u16, v as u16)
                            .expect("bounded scar candidate is canonical");
                    if ChunkPos::from_surface(candidate) != chunk {
                        continue;
                    }
                    let surface_y = self.surface_height_at(candidate).saturating_add(1);
                    let y = if handler == crate::dross::ScarHandler::WaterMarginFilm {
                        (1..CHUNK_Y as i32 - 1)
                            .rev()
                            .find(|height| {
                                let at = BlockPos::new(
                                    candidate.face(),
                                    candidate.u(),
                                    *height as u8,
                                    candidate.v(),
                                )
                                .expect("bounded water-margin candidate is canonical");
                                at.offset(0, -1, 0).is_some_and(|below| {
                                    self.reg.is_water(self.get_block_at(below))
                                        && matches!(self.get_block_at(at), crate::registry::AIR)
                                })
                            })
                            .unwrap_or(surface_y)
                    } else {
                        surface_y
                    };
                    if !(1..CHUNK_Y as i32 - 1).contains(&y) {
                        continue;
                    }
                    let candidate =
                        BlockPos::new(candidate.face(), candidate.u(), y as u8, candidate.v())
                            .expect("scar surface candidate is canonical");
                    let present = self.get_block_at(candidate);
                    if (present != crate::registry::AIR && present != block)
                        || self.arcane_geography.as_ref().is_some_and(|geography| {
                            geography
                                .dynamic
                                .dross_state
                                .materialized
                                .get(&candidate)
                                .is_some_and(|owner| *owner != scar_id)
                        })
                    {
                        continue;
                    }
                    let Some(below) = candidate.offset(0, -1, 0) else {
                        continue;
                    };
                    let support = self.get_block_at(below);
                    let support_definition = self.reg.block(support);
                    if !self.reg.is_solid(support)
                        && !self.reg.is_water(support)
                        && !support_definition.cross
                    {
                        continue;
                    }
                    let water_margin = self.reg.is_water(support)
                        || [(1, 0), (-1, 0), (0, 1), (0, -1)]
                            .into_iter()
                            .filter_map(|(du, dv)| below.offset(du, 0, dv))
                            .any(|neighbor| self.reg.is_water(self.get_block_at(neighbor)));
                    let organic = support_definition.cross
                        || support_definition.burns != 0
                        || ["grass", "dirt", "leaves", "log", "moss"]
                            .iter()
                            .any(|part| support_definition.name.contains(part));
                    let mineral = self.reg.is_solid(support)
                        && !support_definition.cross
                        && support_definition.burns == 0;
                    let score = match handler {
                        crate::dross::ScarHandler::WaterMarginFilm => u8::from(water_margin) * 3,
                        crate::dross::ScarHandler::FilamentGrowth => u8::from(organic) * 3,
                        crate::dross::ScarHandler::MineralCrust => u8::from(mineral) * 3,
                        crate::dross::ScarHandler::SurfaceOverlay => 1,
                    };
                    if selected
                        .as_ref()
                        .is_none_or(|(_, best_score)| score > *best_score)
                    {
                        selected = Some((candidate, score));
                    }
                }
                let Some((selected, _)) = selected else {
                    continue;
                };
                selected
            };
            let present = self.get_block_at(at);
            if present != crate::registry::AIR && present != block {
                // A structural, inventory, fluid, plant, or other authored
                // block always wins. Scars occupy space; they never replace it.
                continue;
            }
            if present != block {
                self.set_block_at(at, block);
            }
            if let Some(geography) = self.arcane_geography.as_mut()
                && let Some(site) = geography.dynamic.dross_state.scars.get_mut(&scar_id)
            {
                if let Some(previous) = site.materialized_at
                    && previous != at
                {
                    geography.dynamic.dross_state.materialized.remove(&previous);
                }
                site.materialized_at = Some(at);
                site.last_changed_step = geography.dynamic.dross_state.completed_steps;
                geography
                    .dynamic
                    .dross_state
                    .materialized
                    .insert(at, scar_id);
            }
        }
    }

    pub(super) fn refresh_loaded_dross_scars(&mut self) {
        let chunks = self.chunks.keys().copied().collect::<Vec<_>>();
        for chunk in chunks {
            self.reconcile_dross_scars_chunk(chunk);
        }
    }

    #[cfg(test)]
    pub(crate) fn refresh_arcane_ecology_for_test(&mut self) {
        self.refresh_loaded_arcane_ecology();
    }

    #[cfg(test)]
    pub(crate) fn refresh_dross_scars_for_test(&mut self) {
        self.refresh_loaded_dross_scars();
    }

    fn apply_loaded_material_retrogen(&mut self, pos: ChunkPos) {
        if !self.chunks.contains_key(&pos) {
            return;
        }
        // Any authored edit, structure stamp, or block entity makes the whole
        // chunk ineligible. This is deliberately conservative: host rock is
        // plentiful; player trust is not.
        if self.player_touched.contains(&pos)
            || self.structure_chunks.contains(&pos)
            || self.block_entities.keys().any(|at| at.chunk() == pos)
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
        if changed {
            // This direct write may be the first chunk saved after a mod was
            // added. Land the naming palette first so a crash/reload cannot
            // interpret the new ore's numeric id through the old palette.
            if self.palette_stale {
                if let Err(error) = self.write_palette() {
                    eprintln!("materials: retrogen palette write failed: {error}");
                    return;
                }
                self.palette_stale = false;
                self.load_remap = self.read_palette_remap();
            }
            if let Err(error) = self.save_chunk(pos) {
                eprintln!("materials: retrogen chunk write failed for {pos:?}: {error}");
                return;
            }
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

    fn commit_fresh_chunk_water(&mut self, pos: ChunkPos, chunk: &mut Chunk) {
        let reg = self.reg.clone();
        let (Some(atlas), Some(weather)) = (&self.planet_atlas, &mut self.planetary_weather) else {
            return;
        };
        let existing = weather
            .water
            .commitments
            .iter()
            .filter(|commitment| commitment.chunk == pos)
            .copied()
            .collect::<Vec<_>>();
        let mut records = chunk.hydrology_volumes().to_vec();
        for record in &mut records {
            let wanted_hu = i128::from(record.baseline_hu)
                .saturating_sub(i128::from(record.residual_hu))
                .clamp(0, i128::from(u64::MAX)) as u64;
            let parcel = if let Some(commitment) = existing
                .iter()
                .find(|commitment| commitment.reservoir == record.reservoir)
            {
                commitment.mass
            } else {
                let Some(reservoir) = weather.water.reservoir_mut(record.reservoir) else {
                    eprintln!(
                        "water: chunk {:?} references missing reservoir {}",
                        pos, record.reservoir
                    );
                    continue;
                };
                // The generated chunk already measured the local salinity of
                // every voxel. Debit that exact salt mass from the named
                // basin instead of taking a basin-average parcel, otherwise
                // a fresh river chunk changes salinity merely by loading.
                // Voxel fluid states are quantized in 32-HU visible units.
                // If a dynamically lowered reservoir cannot fund the
                // immutable baseline, leave its sub-level remainder coarse
                // and materialize only water it actually owns.
                let funded_hu = reservoir.coarse.water_hu.min(wanted_hu)
                    / crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL
                    * crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL;
                let wanted = crate::planet_atlas::ReservoirMass {
                    water_hu: funded_hu,
                    salt_mass: if funded_hu == wanted_hu {
                        record.salt_mass
                    } else {
                        0
                    },
                };
                let parcel = if funded_hu == wanted_hu {
                    reservoir
                        .coarse
                        .take_exact(wanted)
                        .unwrap_or_else(|| reservoir.coarse.take(funded_hu))
                } else {
                    reservoir.coarse.take(funded_hu)
                };
                if parcel.water_hu != wanted_hu {
                    eprintln!(
                        "water: reservoir {} supplied {} of {} HU for chunk {:?}",
                        record.reservoir, parcel.water_hu, wanted_hu, pos
                    );
                }
                if weather
                    .water
                    .credit_detailed_to(Some(record.reservoir), parcel)
                    .is_err()
                {
                    weather
                        .water
                        .reservoir_mut(record.reservoir)
                        .expect("source reservoir still exists")
                        .coarse
                        .add_assign(parcel)
                        .expect("rolled-back water commitment fits");
                    continue;
                }
                weather
                    .water
                    .commitments
                    .push(crate::planet_atlas::ChunkWaterCommitment {
                        chunk: pos,
                        reservoir: record.reservoir,
                        mass: parcel,
                    });
                parcel
            };
            record.salt_mass = parcel.salt_mass;
            record.residual_hu = i128::from(record.baseline_hu)
                .saturating_sub(i128::from(parcel.water_hu))
                .clamp(i128::from(i64::MIN), i128::from(i64::MAX))
                as i64;

            let mut cells = Vec::new();
            for lx in 0..CHUNK_X {
                for lz in 0..CHUNK_Z {
                    let surface = crate::planet::SurfacePos::new(
                        pos.face(),
                        pos.u() * CHUNK_X as u16 + lx as u16,
                        pos.v() * CHUNK_Z as u16 + lz as u16,
                    )
                    .expect("chunk column is canonical");
                    let hydro = atlas.hydrology_sample(surface.center());
                    let reservoir = if hydro.ocean_basin_id != 0 {
                        Some(crate::planet_atlas::surface_reservoir_id(
                            crate::planet_atlas::SurfaceReservoirKind::Ocean,
                            u32::from(hydro.ocean_basin_id),
                        ))
                    } else if hydro.lake_basin_id != 0 {
                        Some(crate::planet_atlas::surface_reservoir_id(
                            crate::planet_atlas::SurfaceReservoirKind::Lake,
                            hydro.lake_basin_id,
                        ))
                    } else if hydro.river_id != 0 {
                        Some(crate::planet_atlas::surface_reservoir_id(
                            crate::planet_atlas::SurfaceReservoirKind::River,
                            hydro.river_id,
                        ))
                    } else {
                        None
                    };
                    if reservoir != Some(record.reservoir) {
                        continue;
                    }
                    for y in 1..CHUNK_Y {
                        let block = chunk.get(lx, y, lz);
                        if let Some(units) = reg.water_volume(block) {
                            cells.push((lx, y, lz, units, false));
                        } else if reg.block(block).name == "base:ice" {
                            cells.push((lx, y, lz, 8, true));
                        }
                    }
                }
            }
            let represented_hu = cells.iter().fold(0u64, |total, cell| {
                total.saturating_add(
                    u64::from(cell.3)
                        .saturating_mul(crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL),
                )
            });
            if represented_hu != parcel.water_hu {
                cells.sort_by_key(|&(x, y, z, _, _)| (y, x, z));
                let mut remaining_units =
                    parcel.water_hu / crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL;
                for cell in &mut cells {
                    let (x, y, z, units, was_ice) = *cell;
                    let kept = u64::from(units).min(remaining_units) as u8;
                    remaining_units -= u64::from(kept);
                    cell.3 = kept;
                    let block = if kept == 0 {
                        crate::registry::AIR
                    } else if was_ice && kept == 8 {
                        chunk.get(x, y, z)
                    } else {
                        reg.water_for_volume(kept)
                    };
                    chunk.set(x, y, z, block);
                    if kept == 0 {
                        chunk.set_water_salt(x, y, z, 0);
                        chunk.set_meta(x, y, z, 0);
                    }
                }
                debug_assert_eq!(remaining_units, 0);
                cells.retain(|cell| cell.3 != 0);
            }
            if !cells.is_empty() {
                let existing_total = cells.iter().fold(0u64, |total, &(x, y, z, _, _)| {
                    total.saturating_add(u64::from(chunk.water_salt(x, y, z)))
                });
                // Usually these totals are identical and the atlas-authored
                // per-column concentrations remain byte-for-byte unchanged.
                // A recovered/legacy commitment can differ, so apportion its
                // exact total by the existing local weights rather than
                // flattening the whole chunk to one concentration.
                if existing_total != parcel.salt_mass {
                    let count = cells.len() as u64;
                    let mut previous_allocation = 0u64;
                    let mut cumulative_weight = 0u64;
                    let mut allocations = Vec::with_capacity(cells.len());
                    for (index, &(x, y, z, _, _)) in cells.iter().enumerate() {
                        cumulative_weight =
                            cumulative_weight.saturating_add(u64::from(chunk.water_salt(x, y, z)));
                        let cumulative_allocation = if existing_total == 0 {
                            (index as u64 + 1).saturating_mul(parcel.salt_mass) / count
                        } else {
                            (u128::from(parcel.salt_mass) * u128::from(cumulative_weight)
                                / u128::from(existing_total)) as u64
                        };
                        allocations.push(
                            cumulative_allocation
                                .saturating_sub(previous_allocation)
                                .min(u64::from(u16::MAX)),
                        );
                        previous_allocation = cumulative_allocation;
                    }
                    let mut remainder = parcel
                        .salt_mass
                        .saturating_sub(allocations.iter().copied().sum::<u64>());
                    for allocation in &mut allocations {
                        let extra = remainder.min(u64::from(u16::MAX) - *allocation);
                        *allocation += extra;
                        remainder -= extra;
                        if remainder == 0 {
                            break;
                        }
                    }
                    debug_assert_eq!(remainder, 0);
                    for (&(x, y, z, _, _), salt) in cells.iter().zip(allocations) {
                        let salt = salt as u16;
                        chunk.set_water_salt(x, y, z, salt);
                        chunk.set_meta(x, y, z, (u64::from(salt) / 256).min(255) as u8);
                    }
                }
            }
        }
        weather
            .water
            .commitments
            .sort_by_key(|commitment| (commitment.chunk, commitment.reservoir));
        chunk.set_hydrology_volumes(records);
    }

    // ---------------- ruins ----------------

    /// Deterministic per-chunk structure roll (at most one per chunk).
    pub(super) fn seed_structures(&mut self, pos: ChunkPos) {
        // A chunk already claimed by a structure or piece assembly never
        // rolls its own: multi-chunk assemblies reserve every touched chunk
        // in `structure_chunks` *before* `ensure_chunk`, so a neighbor that
        // arrives mid-walk (and any re-generation) early-returns here.
        if self.structure_chunks.contains(&pos) {
            return;
        }
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
            // A fixed-template ruin won this chunk; skip assemblies.
            return;
        }
        // Piece assemblies (spec Part 2.3). At most one structure roll wins
        // per chunk; the ruin loop already returned if one landed, so the
        // origin chunk is unreserved here.
        for (ai, asm) in reg.assemblies.iter().enumerate() {
            if !asm.biomes.contains(&biome) {
                continue;
            }
            let h = self.mob_hash_at(center, 9000 + 0x10000 + ai as u32);
            if !h.is_multiple_of(asm.rarity) {
                continue;
            }
            let (markers, _) = self.place_assembly(asm.clone(), pos, h);
            // Spec 2.4/2.5 seam — first consumer: `spawn:npc:<id>` markers
            // place their NPC at the resolved world position. The NPC id is
            // `mod:npc` qualified; an unknown id is silently skipped (the
            // piece stays, its occupant just isn't there).
            for marker in markers {
                if let Some(npc_name) = marker.kind.strip_prefix("spawn:npc:")
                    && let Some(ni) = reg.npc_id(npc_name)
                {
                    let at = marker.at;
                    let surface = at.surface();
                    let y = self.surface_height_at(surface) as u8;
                    if let Ok(pos) = crate::planet::EntityPos::new(
                        surface.face(),
                        f32::from(surface.u()) + 0.5,
                        f32::from(y) + 1.05,
                        f32::from(surface.v()) + 0.5,
                    ) {
                        self.spawn_npc_at(ni, pos);
                    }
                }
                // Spec 2.5: `feature:<id>` markers place a sealed block at
                // the resolved position, locked until the player's KV flag
                // reads the gate's `value`. The gate id is `mod:gate`
                // qualified; an unknown id is silently skipped so a missing
                // def cannot leave a permanent unbreakable wall.
                if let Some(gate_name) = marker.kind.strip_prefix("feature:")
                    && let Some(gate) = reg.gate_id(gate_name)
                {
                    self.place_gate_at(gate, marker.at);
                }
                // Capability E9: `spawn:nest:<id>` markers place a nest
                // spawn-gate block at the resolved position. Placing the
                // block through the ordinary block path records the nest
                // automatically; an unknown id is silently skipped.
                if let Some(nest_name) = marker.kind.strip_prefix("spawn:nest:")
                    && let Some(nest_index) = reg
                        .nests
                        .iter()
                        .position(|nest| nest.id == nest_name || nest.id == format!("base:{nest_name}"))
                {
                    let nest = &reg.nests[nest_index];
                    self.set_block_at(marker.at, nest.block);
                }
            }
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
        self.structure_chunks.insert(origin.chunk());
        let chest_block = reg.block_id("base:chest");
        let mut rng = seed ^ 0x5f37_59df;
        let mut inherited_stacks = Vec::new();
        let mut inherited_placements = Vec::new();
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
                                    let mut loot = self.roll_loot(table, n as u32, &mut rng);
                                    if st.name == "base:observational_outpost" {
                                        for name in [
                                            "base:etched_tablet",
                                            "base:maker_calibration_plate",
                                            "base:spent_charm_fitting",
                                            "base:broken_focus",
                                            "base:sealed_dross_ampoule",
                                            "base:site_survey_marks",
                                            "base:failed_containment_fragment",
                                        ] {
                                            if let Some(item) = reg.item_id(name) {
                                                loot.push(ItemStack::new(&reg, item, 1));
                                            }
                                        }
                                    }
                                    for (i, mut stck) in loot.into_iter().enumerate() {
                                        if i < CHEST_SLOTS {
                                            if self.arcane_ledger.is_some()
                                                && let Err(error) = self.bind_arcane_stack_at(
                                                    pos,
                                                    &mut stck,
                                                    "ruin inheritance",
                                                )
                                            {
                                                eprintln!(
                                                    "arcane: ruin loot could not bind: {error}"
                                                );
                                                continue;
                                            }
                                            if self.discovery_state.is_some()
                                                && let Err(error) =
                                                    self.bind_discovery_stack_at(pos, &mut stck)
                                            {
                                                eprintln!(
                                                    "discovery: ruin artifact could not bind: {error}"
                                                );
                                                continue;
                                            }
                                            // Scatter through the chest.
                                            let preferred =
                                                (i * 7 + (rng % 5) as usize) % CHEST_SLOTS;
                                            let slot = (0..CHEST_SLOTS)
                                                .map(|offset| (preferred + offset) % CHEST_SLOTS)
                                                .find(|slot| state.slots[*slot].is_none())
                                                .unwrap_or(preferred);
                                            inherited_stacks.push(stck);
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
                                let materials = reg.block(*b).materials.clone();
                                if !materials.is_empty() {
                                    inherited_placements.push((pos, materials));
                                }
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
        if let Some(ledger) = &mut self.material_ledger
            && let Err(error) = ledger.record_external_world_content(
                &reg,
                &inherited_stacks,
                &inherited_placements,
                "pre-genesis ruin inheritance",
            )
        {
            eprintln!("materials: ruin inheritance accounting failed: {error}");
        }
    }

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
                    let instances = if self.reg.item(e.item).arcane.is_some()
                        || self.reg.item(e.item).discovery.is_some()
                    {
                        n.max(1)
                    } else {
                        1
                    };
                    for _ in 0..instances {
                        let count = if instances == 1 { n.max(1) } else { 1 };
                        let mut stack = ItemStack::new(&self.reg, e.item, count);
                        if let Some(frac) = e.durability_frac {
                            let max = self.reg.item(e.item).durability;
                            if max > 0 {
                                stack.durability = ((max as f32 * frac) as u32).max(1);
                            }
                        }
                        out.push(stack);
                    }
                    break;
                }
                pick -= e.weight;
            }
        }
        out
    }

    // ---------------- gravity blocks ----------------
}
