//! World metadata, palettes, save paths, and persistence codecs.

use super::*;

/// Stamps are only meaningful against the DAY_LENGTH they were written
/// under. Bump this whenever that changes and old stamps are discarded
/// rather than misread as an absence.
const STAMPS_MAGIC: &[u8] = b"WFS3-PLANET-1200";

impl World {
    /// Load a world from disk (reads seed + palette) or create a fresh one.
    pub fn load_or_create(save_dir: PathBuf, mut reg: Arc<Registry>) -> std::io::Result<World> {
        if !reg.material_errors.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "content material accounting failed:\n{}",
                    reg.material_errors.join("\n")
                ),
            ));
        }
        if !reg.arcane_errors.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "content arcane accounting failed:\n{}",
                    reg.arcane_errors.join("\n")
                ),
            ));
        }
        let mut existing = load_world_meta(&save_dir)?;
        let seed = existing.as_ref().map(|meta| meta.seed).unwrap_or_else(|| {
            std::env::var("WILDFORGE_SEED")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or_else(|| {
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs() as u32)
                        .unwrap_or(1337)
                })
        });
        if existing.is_none() {
            #[cfg(not(test))]
            create_world_atomic(
                &save_dir,
                seed,
                "survival",
                crate::planet_atlas::genesis_content_hash(std::path::Path::new("mods")),
                Arc::clone(&reg),
                &crate::planet_atlas::CancellationToken::default(),
                |progress| match progress {
                    WorldCreationProgress::Atlas(progress) => {
                        eprintln!("world creation: {}", progress.stage.label())
                    }
                    WorldCreationProgress::Arcane(progress) => {
                        eprintln!("world creation: {}", progress.stage.label())
                    }
                    WorldCreationProgress::Homeland {
                        stage,
                        completed,
                        total,
                    } => eprintln!("world creation: {stage} {completed}/{total}"),
                },
            )?;
            #[cfg(test)]
            create_world_fixture_atomic(
                &save_dir,
                seed,
                "survival",
                8,
                &crate::planet_atlas::CancellationToken::default(),
                |_| {},
            )?;
            existing = load_world_meta(&save_dir)?;
        }
        let (mode, ire, day, camera) = existing
            .map(|meta| (meta.mode, meta.ire, meta.day, meta.camera))
            .unwrap_or_else(|| ("survival".to_string(), 0.0, 0, "first".to_string()));
        #[cfg(not(test))]
        let atlas =
            crate::planet_atlas::PlanetAtlas::load(&save_dir).map_err(std::io::Error::other)?;
        #[cfg(test)]
        let atlas = match crate::planet_atlas::PlanetAtlas::load_fixture(&save_dir) {
            Ok(atlas) => atlas,
            Err(crate::planet_atlas::AtlasError::Io(error))
                if error.kind() == std::io::ErrorKind::NotFound =>
            {
                // Existing test worlds made through `World::new` predate the
                // atlas fixture. Production builds refuse this state.
                let fixture = crate::planet_atlas::PlanetAtlas::fixture(seed, 8)
                    .map_err(std::io::Error::other)?;
                fixture
                    .write_new(&save_dir)
                    .map_err(std::io::Error::other)?;
                fixture
            }
            Err(error) => return Err(std::io::Error::other(error)),
        };
        if let Ok(ledger) = crate::materials::MaterialLedger::load(&save_dir) {
            let added = Arc::make_mut(&mut reg).install_saved_placeholders(&save_dir, &ledger)?;
            ledger.validate_saved_definitions(&reg)?;
            if added != 0 {
                eprintln!(
                    "materials: restored {added} named placeholder block definitions for removed mods"
                );
            }
        }
        if let Ok(ledger) = crate::arcane::ArcaneLedger::load(&save_dir) {
            let added = Arc::make_mut(&mut reg).install_saved_arcane_placeholders(&ledger);
            if added != 0 {
                eprintln!("arcane: restored {added} named charged placeholders for removed mods");
            }
        }
        // A finite world must never continue with accounting silently
        // disabled. This also leaves the independently written backup intact
        // for an operator-led recovery instead of inventing replacement mass.
        let material_ledger =
            crate::materials::MaterialLedger::load_or_initialize(&save_dir, &atlas, &reg)?;
        #[cfg(test)]
        let mut arcane_geography =
            crate::arcane_geography::ArcaneGeography::load_or_generate(&save_dir, &atlas, &reg)
                .map_err(std::io::Error::other)?;
        #[cfg(not(test))]
        let mut arcane_geography =
            crate::arcane_geography::ArcaneGeography::load(&save_dir, &atlas)
                .map_err(std::io::Error::other)?;
        let ecology_added =
            crate::arcane_ecology::reconcile_content(&atlas, &reg, &mut arcane_geography)
                .map_err(std::io::Error::other)?;
        if ecology_added != 0 {
            arcane_geography
                .save_dynamic(&save_dir)
                .map_err(std::io::Error::other)?;
            eprintln!(
                "arcane ecology: reserved {ecology_added} deterministic existing-world sites"
            );
        }
        let geography_current = arcane_geography
            .custody_current()
            .map_err(std::io::Error::other)?;
        let arcane_ledger = crate::arcane::ArcaneLedger::load_or_initialize_with_geography(
            &save_dir,
            &atlas,
            &reg,
            geography_current,
        )
        .map_err(std::io::Error::other)?;
        let discovery_state =
            crate::discovery::DiscoveryState::load_or_initialize(&save_dir, seed, reg.content_hash)
                .map_err(std::io::Error::other)?;
        let implements_state =
            crate::implements::ImplementsState::load_or_initialize(&save_dir, reg.content_hash)
                .map_err(std::io::Error::other)?;
        let workings_state = crate::workings::WorkingsState::load_or_initialize(
            &save_dir,
            reg.content_hash,
            &reg.workings,
        )
        .map_err(std::io::Error::other)?;
        let alchemy_state =
            crate::alchemy::AlchemyState::load_or_initialize(&save_dir, reg.content_hash)
                .map_err(std::io::Error::other)?;
        write_world_meta_full(&save_dir, seed, &mode, ire, day, &camera)?;
        let mut w = World::new_with_preloaded_atlas(seed, save_dir, reg, Arc::new(atlas));
        w.material_ledger = Some(material_ledger);
        w.arcane_ledger = Some(arcane_ledger);
        w.arcane_geography = Some(arcane_geography);
        w.discovery_state = Some(discovery_state);
        w.implements_state = Some(implements_state);
        w.workings_state = Some(workings_state);
        w.alchemy_state = Some(alchemy_state);
        w.mode = mode;
        w.camera = camera;
        w.ire = ire;
        w.day = day;
        w.clock = day as f64 * crate::server::DAY_LENGTH as f64;
        w.load_remap = w.read_palette_remap();
        w.palette_stale = !w.palette_matches_registry();
        // The arcane journal commits before sparse item-owner files. If a
        // process stopped between those two durable writes, roll an
        // unmaterialized account back or remove a stale pre-commit stack
        // before any player, container, drop, or mob can observe it.
        if let Some(ledger) = &mut w.arcane_ledger {
            let active_workings = w
                .workings_state
                .as_ref()
                .map_or_else(std::collections::BTreeSet::new, |state| state.active_ids());
            let workings_max = w
                .workings_state
                .as_ref()
                .map_or(0, crate::workings::WorkingsState::max_working_id);
            ledger.reconcile_working_id_floor(workings_max);
            ledger
                .reconcile_transient_owners(&active_workings)
                .map_err(std::io::Error::other)?;
            let active_alchemy = w
                .alchemy_state
                .as_ref()
                .map_or_else(std::collections::BTreeSet::new, |state| {
                    state.active_arcane_ids()
                });
            ledger
                .reconcile_alchemy_owners(&active_alchemy)
                .map_err(std::io::Error::other)?;
            ledger
                .reconcile_durable_item_owners(&w.save_dir)
                .map_err(std::io::Error::other)?;
        }
        if let (Some(ledger), Some(implements)) =
            (w.arcane_ledger.as_ref(), w.implements_state.as_mut())
        {
            let recovered = implements
                .reconcile_ledger(ledger)
                .map_err(std::io::Error::other)?;
            if recovered != 0 {
                implements.save().map_err(std::io::Error::other)?;
                eprintln!(
                    "implements: removed {recovered} construction records rolled back after an interrupted physical-owner save"
                );
            }
        }
        w.load_entities();
        w.migrate_loaded_entity_charms();
        w.load_mobs();
        w.migrate_loaded_mob_charms();
        w.load_loose_items();
        w.load_stamps();
        w.load_templates();
        w.load_local_structures();
        let interrupted = w
            .interrupt_loaded_wand_workings()
            .map_err(std::io::Error::other)?;
        if interrupted != 0 {
            eprintln!(
                "workings: interrupted {interrupted} held wand channel(s) whose session ended at restart"
            );
        }
        w.replay_pending_material_operation()?;
        let replayed = w.replay_pending_workings().map_err(std::io::Error::other)?;
        if replayed != 0 {
            eprintln!("workings: completed {replayed} crash-safe pending effect(s)");
        }
        Ok(w)
    }

    fn replay_pending_material_operation(&mut self) -> std::io::Result<()> {
        let pending = self
            .material_ledger
            .as_ref()
            .map(crate::materials::MaterialLedger::pending_operation)
            .transpose()?
            .flatten();
        let Some(operation) = pending else {
            return Ok(());
        };
        self.ensure_chunk(operation.pos.chunk());
        let current = self
            .reg
            .block(self.get_block_at(operation.pos))
            .name
            .clone();
        let ledger = self
            .material_ledger
            .as_mut()
            .ok_or_else(|| std::io::Error::other("material journal exists without a ledger"))?;
        if current == operation.after {
            ledger.apply_operation(&operation)?;
            ledger.finish_operation()?;
        } else if current == operation.before_block_name() {
            // The voxel write never landed, so the un-applied operation is a
            // harmless aborted transaction.
            ledger.finish_operation()?;
        } else {
            return Err(std::io::Error::other(format!(
                "material journal {} expected voxel {} or {}, found {}; restore the chunk/ledger backup",
                operation.id,
                operation.before_block_name(),
                operation.after,
                current
            )));
        }
        Ok(())
    }

    /// Per-chunk random-tick stamps: compact (x, z, time) triples.
    ///
    /// The times are clock seconds, and the clock's unit is DAY_LENGTH.
    /// When that changed, every stamp from an older save started
    /// reading as days of absence — so the whole explored world would
    /// have fired its catch-up burst (crops jumping, snow phasing) the
    /// first time it loaded. The header says whose clock these are;
    /// anything older is dropped, which costs only the catch-up itself.
    pub(super) fn load_stamps(&mut self) {
        let Ok(buf) = fs::read(self.save_dir.join("stamps")) else {
            return;
        };
        let Some(body) = buf.strip_prefix(STAMPS_MAGIC) else {
            return;
        };
        for rec in body.chunks_exact(13) {
            let Some(face) = crate::planet::Face::from_u8(rec[0]) else {
                continue;
            };
            let u = u16::from_le_bytes(rec[1..3].try_into().unwrap());
            let v = u16::from_le_bytes(rec[3..5].try_into().unwrap());
            let Ok(pos) = ChunkPos::new(face, u, v) else {
                continue;
            };
            let t = f64::from_le_bytes(rec[5..13].try_into().unwrap());
            self.last_random.insert(pos, t);
        }
    }

    pub(super) fn save_stamps(&self) -> std::io::Result<()> {
        let mut buf = Vec::with_capacity(STAMPS_MAGIC.len() + self.last_random.len() * 13);
        buf.extend_from_slice(STAMPS_MAGIC);
        for (pos, t) in &self.last_random {
            buf.push(pos.face() as u8);
            buf.extend_from_slice(&pos.u().to_le_bytes());
            buf.extend_from_slice(&pos.v().to_le_bytes());
            buf.extend_from_slice(&t.to_le_bytes());
        }
        atomic_replace(&self.save_dir.join("stamps"), &buf)
    }

    /// Map every stored numeric id to a current runtime id via string names.
    pub(super) fn read_palette_remap(&self) -> Vec<BlockId> {
        let Ok(text) = fs::read_to_string(self.save_dir.join("palette")) else {
            // WFC3 saves from before named palettes stored the then-current
            // registry ids directly. Treating a missing palette as an empty
            // remap turns every cell, including air, into base:unknown and
            // produces solid chunk-height magenta pillars.
            return self.identity_palette_remap();
        };
        let mut remap = Vec::new();
        for line in text.lines() {
            let Some((num, name)) = line.split_once(' ') else {
                continue;
            };
            let Ok(num) = num.parse::<usize>() else {
                continue;
            };
            if remap.len() <= num {
                remap.resize(num + 1, self.reg.unknown_block);
            }
            remap[num] = self
                .reg
                .block_id(name.trim())
                .unwrap_or(self.reg.unknown_block);
        }
        if remap.is_empty() {
            self.identity_palette_remap()
        } else {
            remap
        }
    }

    fn identity_palette_remap(&self) -> Vec<BlockId> {
        (0..self.reg.blocks.len())
            .map(|id| BlockId(id as u16))
            .collect()
    }

    /// The palette this registry would write: one `id name` line per block.
    fn palette_text(&self) -> String {
        let mut out = String::new();
        for (i, b) in self.reg.blocks.iter().enumerate() {
            out.push_str(&format!("{i} {}\n", b.name));
        }
        out
    }

    /// Does the saved palette already describe this registry? When it does,
    /// every chunk file on disk is written in ids we still understand, so a
    /// chunk that loads unedited does not need saving again. When it does
    /// NOT (a mod arrived, the game updated), chunk files carry stale ids
    /// and every one we touch has to be rewritten under the new palette.
    pub(super) fn palette_matches_registry(&self) -> bool {
        fs::read_to_string(self.save_dir.join("palette")).is_ok_and(|on_disk| {
            // A fresh world has no chunks yet, so a missing palette is not
            // a mismatch — but an unreadable one we treat as stale.
            on_disk == self.palette_text()
        })
    }

    /// Write the current registry as this world's palette (runtime ids are
    /// stored ids from now on).
    pub(super) fn write_palette(&self) -> std::io::Result<()> {
        atomic_replace(
            &self.save_dir.join("palette"),
            self.palette_text().as_bytes(),
        )
    }
}

pub(super) fn atomic_replace(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    crate::identity::atomic_write(path, bytes, false)
}

pub(super) fn remove_if_exists(path: &std::path::Path) -> std::io::Result<()> {
    crate::persist::remove_if_exists(path)
}

pub(super) fn replace_or_remove(
    path: &std::path::Path,
    bytes: Option<&[u8]>,
) -> std::io::Result<()> {
    match bytes {
        Some(bytes) => atomic_replace(path, bytes),
        None => remove_if_exists(path),
    }
}
