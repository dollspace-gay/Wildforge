//! Initialization coordinator for the authoritative world.

use super::{
    Arc, Generator, HashMap, HashSet, PathBuf, Registry, VecDeque, World, calendar_state,
    construction, installations, population, storage, terrain, weather_state,
    write_world_meta_full,
};

impl World {
    pub fn new(seed: u32, save_dir: PathBuf, reg: Arc<Registry>) -> World {
        Self::new_with_optional_atlas(seed, save_dir, reg, None, true)
    }

    #[cfg(test)]
    pub fn new_with_atlas(
        seed: u32,
        save_dir: PathBuf,
        reg: Arc<Registry>,
        atlas: Arc<crate::planet_atlas::PlanetAtlas>,
    ) -> World {
        Self::new_with_optional_atlas(seed, save_dir, reg, Some(atlas), true)
    }

    pub(super) fn new_with_preloaded_atlas(
        seed: u32,
        save_dir: PathBuf,
        reg: Arc<Registry>,
        atlas: Arc<crate::planet_atlas::PlanetAtlas>,
    ) -> World {
        Self::new_with_optional_atlas(seed, save_dir, reg, Some(atlas), false)
    }

    pub(super) fn new_with_optional_atlas(
        seed: u32,
        save_dir: PathBuf,
        reg: Arc<Registry>,
        planet_atlas: Option<Arc<crate::planet_atlas::PlanetAtlas>>,
        load_authority: bool,
    ) -> World {
        let weather_state = weather_state::WeatherState::new(planet_atlas.as_deref());
        let generator = planet_atlas.as_ref().map_or_else(
            || Generator::new(seed, &reg),
            |atlas| Generator::with_atlas(seed, &reg, atlas.clone()),
        );
        let authority_atlas = if load_authority {
            planet_atlas.as_ref()
        } else {
            None
        };
        let material_ledger = authority_atlas.and_then(|atlas| {
            crate::materials::MaterialLedger::load_or_initialize(&save_dir, atlas, &reg)
                .map_err(|error| {
                    eprintln!("materials: could not open ledger: {error}");
                    error
                })
                .ok()
        });
        let arcane_geography = authority_atlas.and_then(|atlas| {
            #[cfg(test)]
            let opened =
                crate::arcane_geography::ArcaneGeography::load_or_generate(&save_dir, atlas, &reg);
            #[cfg(not(test))]
            let opened = crate::arcane_geography::ArcaneGeography::load(&save_dir, atlas);
            let mut geography = opened
                .map_err(|error| {
                    eprintln!("arcane geography: could not open subledger: {error}");
                    error
                })
                .ok()?;
            if let Err(error) =
                crate::arcane_ecology::reconcile_content(atlas, &reg, &mut geography)
            {
                eprintln!("arcane ecology: could not reconcile content: {error}");
                return None;
            }
            Some(geography)
        });
        let geography_current = arcane_geography
            .as_ref()
            .and_then(|geography| geography.custody_current().ok());
        let arcane_ledger = authority_atlas
            .zip(geography_current)
            .and_then(|(atlas, current)| {
                crate::arcane::ArcaneLedger::load_or_initialize_with_geography(
                    &save_dir, atlas, &reg, current,
                )
                .map_err(|error| {
                    eprintln!("arcane: could not open ledger: {error}");
                    error
                })
                .ok()
            });
        let discovery_state = authority_atlas.and_then(|_| {
            crate::discovery::DiscoveryState::load_or_initialize(&save_dir, seed, reg.content_hash)
                .map_err(|error| {
                    eprintln!("discovery: could not open knowledge state: {error}");
                    error
                })
                .ok()
        });
        let implements_state = authority_atlas.and_then(|_| {
            crate::implements::ImplementsState::load_or_initialize(&save_dir, reg.content_hash)
                .map_err(|error| {
                    eprintln!("implements: could not open state: {error}");
                    error
                })
                .ok()
        });
        let workings_state = authority_atlas.and_then(|_| {
            crate::workings::WorkingsState::load_or_initialize(
                &save_dir,
                reg.content_hash,
                &reg.workings,
            )
            .map_err(|error| {
                eprintln!("workings: could not open transaction state: {error}");
                error
            })
            .ok()
        });
        let alchemy_state = authority_atlas.and_then(|_| {
            crate::alchemy::AlchemyState::load_or_initialize(&save_dir, reg.content_hash)
                .map_err(|error| {
                    eprintln!("alchemy: could not open state: {error}");
                    error
                })
                .ok()
        });
        let water_carriers = authority_atlas.and_then(|_| {
            crate::workings::WaterCarrierState::load_or_initialize(&save_dir)
                .map_err(|error| {
                    eprintln!("workings: could not open detailed water carriers: {error}");
                    error
                })
                .ok()
        });
        World {
            construction: construction::Construction::default(),
            calendar_state: calendar_state::CalendarState::default(),
            population: population::Population::default(),
            chunks: terrain::TerrainStore::default(),
            generator,
            planet_atlas,
            weather_state,
            common_spawn: None,
            material_ledger,
            arcane_ledger,
            arcane_geography,
            discovery_state,
            implements_state,
            workings_state,
            alchemy_state,
            water_carriers,
            dross_exposure_tick: HashMap::new(),
            palette: storage::PaletteStore::new(&save_dir, &reg),
            reg,
            seed,
            region_store: storage::RegionStore::new(save_dir.clone()),
            save_dir,
            water_queue: VecDeque::new(),
            water_queued: HashSet::new(),
            lava_queue: VecDeque::new(),
            lava_queued: HashSet::new(),
            fire_queue: VecDeque::new(),
            fire_queued: HashSet::new(),
            fluid_batch: false,
            pending_relight: HashSet::new(),
            edit_relight_batch: false,
            last_random: HashMap::new(),
            installations: installations::Installations::default(),
            pending_drops: Vec::new(),
            belt_state: HashMap::new(),
            regional_ire: HashMap::new(),
            whispers: Vec::new(),
            blessed_streak: HashMap::new(),
            player_touched: HashSet::new(),
            structure_chunks: HashSet::new(),
            gated: HashMap::new(),
            nests: HashMap::new(),
            nest_spawn_cd: HashMap::new(),
            hidden: HashMap::new(),
            bloom: HashMap::new(),
            hearts: HashMap::new(),
            bloom_spent: HashMap::new(),
            last_entry_anchor: None,
            dungeon_runs: Vec::new(),
            run_seed: 0x5EED_0000,
            mode: "survival".into(),
            camera: "first".into(),
            ire: 0.0,
            plant_ire_today: 0.0,
            log_edits: false,
            edit_log: Vec::new(),
            falling: Vec::new(),
            pending_gives: Vec::new(),
            #[cfg(test)]
            multiblock_revalidations: 0,
            #[cfg(test)]
            save_fail_chunks: HashSet::new(),
            #[cfg(test)]
            fail_loose_item_save: false,
        }
    }

    pub fn planet_atlas(&self) -> Option<Arc<crate::planet_atlas::PlanetAtlas>> {
        self.planet_atlas.clone()
    }

    /// Test-only: how many instances the 2c edit hook has revalidated.
    #[cfg(test)]
    pub(crate) fn multiblock_revalidations(&self) -> usize {
        self.multiblock_revalidations
    }

    pub fn common_spawn(&self) -> Option<crate::planet::EntityPos> {
        self.common_spawn
    }

    pub(crate) fn remap_loose_items(&mut self, old: &Registry, registry: &Registry) {
        self.population.remap_loose_items(old, registry);
    }

    /// Publish validated runtime definitions and rebuild immutable generation
    /// bindings. The content coordinator has already checked live compatibility.
    pub(crate) fn replace_registry(&mut self, registry: Arc<Registry>) {
        let old = std::mem::replace(&mut self.reg, registry);
        self.remap_from(&old);
        self.generator = self.planet_atlas().map_or_else(
            || Generator::new(self.seed, &self.reg),
            |atlas| Generator::with_atlas(self.seed, &self.reg, atlas),
        );
        if let (Some(atlas), Some(ledger)) = (self.planet_atlas(), &mut self.material_ledger)
            && (ledger.reconcile_mod_manifests(&atlas, &self.reg)
                | ledger.reconcile_saved_definitions(&self.reg))
            && let Err(error) = ledger.save()
        {
            eprintln!("materials: could not persist hot-reload manifest: {error}");
        }
    }

    /// The survival ruleset this world's mode names (capability E1). A mode
    /// string that no longer resolves falls back to survival.
    pub fn ruleset(&self) -> crate::ruleset::Ruleset {
        self.reg.ruleset_for(&self.mode)
    }

    /// Persist the camera mode in the authoritative world's metadata.
    pub fn set_camera(&mut self, camera: &str) -> std::io::Result<()> {
        self.camera = camera.to_string();
        write_world_meta_full(
            &self.save_dir,
            self.seed,
            &self.mode,
            self.ire,
            self.calendar_state.day(),
            camera,
        )
    }
}
