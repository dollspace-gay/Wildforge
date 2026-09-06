//! Explicit whole-planet generation and stage evidence coordination.

use crate::planet::{FACE_BLOCKS, PLANET_RADIUS, surface_to_unit};
use crate::planet_atlas::codec::FILE_HEADER_BYTES;
use crate::planet_atlas::codec::dynamic::encode_dynamic;
use crate::planet_atlas::codec::fingerprints::{
    fingerprint_biomes, fingerprint_climate, fingerprint_geometry, fingerprint_ground,
    fingerprint_hydrology, fingerprint_resources, fingerprint_tectonics, fingerprint_terrain,
};
use crate::planet_atlas::codec::genesis::encode_genesis;
use crate::planet_atlas::codec::models::{
    encode_biomes, encode_geology, encode_history, encode_hydrology,
};
use crate::planet_atlas::grid::{atlas_count, cell_area, generate_grid};
use crate::planet_atlas::identity::{stable_hash, unit_noise};
use crate::planet_atlas::manifest::layer_versions;
use crate::planet_atlas::validation::{
    validate_biomes, validate_climate, validate_geometry, validate_ground, validate_hydrology,
    validate_layers, validate_tectonics, validate_terrain,
};
use crate::planet_atlas::{
    ATLAS_ALGORITHM_VERSION, ATLAS_DYNAMIC_VERSION, ATLAS_FORMAT_VERSION, ATLAS_HISTORY_VERSION,
    AXIAL_TILT_DEGREES, AtlasConfig, AtlasError, AtlasManifest, AtlasProgress, AtlasStage,
    BIOME_SCHEMA_VERSION, CancellationToken, DynamicCell, DynamicLayers, GEOLOGY_SCHEMA_VERSION,
    GenesisLayers, GeologyOutput, GeometryCell, HYDRO_UNITS_PER_VISIBLE_LEVEL,
    HYDROLOGY_SCHEMA_VERSION, HistoryLayers, HydrologyOutput, PRIME_MERIDIAN, PlanetAtlas,
    ROTATION_AXIS, ReservoirMass, StageRecord, WATER_CYCLE_SCHEMA_VERSION, dynamic_water_total,
    encode_water_cycle, generate_biomes_and_countries, generate_climate, generate_geology,
    generate_ground_layer, generate_hydrology, initial_water_cycle, route_placer_deposits,
};
use noise::Perlin;
use std::time::Instant;

impl PlanetAtlas {
    pub fn generate(
        seed: u32,
        content_hash: u64,
        config: AtlasConfig,
        cancel: &CancellationToken,
        mut progress: impl FnMut(AtlasProgress),
    ) -> Result<Self, AtlasError> {
        let side = config.side;
        let count = atlas_count(side)?;
        let mut stages = Vec::new();
        let mut announce = |stage: AtlasStage| -> Result<Instant, AtlasError> {
            if cancel.is_cancelled() {
                return Err(AtlasError::Cancelled);
            }
            let completed_stages = AtlasStage::ALL
                .iter()
                .position(|candidate| *candidate == stage)
                .unwrap_or(0);
            progress(AtlasProgress {
                stage,
                completed_stages,
                total_stages: AtlasStage::ALL.len(),
            });
            Ok(Instant::now())
        };
        let mut record = |stage: AtlasStage, started: Instant, checksum: u64| {
            stages.push(StageRecord {
                id: stage.id().to_string(),
                schema_version: 1,
                algorithm_version: ATLAS_ALGORITHM_VERSION,
                checksum,
                elapsed_micros: started.elapsed().as_micros().try_into().unwrap_or(u64::MAX),
            });
        };

        let started = announce(AtlasStage::Topology)?;
        let geometry = generate_grid(side, config.mode, |pos| {
            let unit = surface_to_unit(pos.center(side));
            GeometryCell {
                unit_direction: [unit.x as f32, unit.y as f32, unit.z as f32],
                latitude_radians: unit.y.asin() as f32,
                physical_area: cell_area(pos, side),
            }
        })?;
        validate_geometry(&geometry)?;
        record(
            AtlasStage::Topology,
            started,
            fingerprint_geometry(&geometry),
        );

        let started = announce(AtlasStage::Tectonics)?;
        let GeologyOutput {
            tectonics,
            terrain: geological_terrain,
            resources: mut geological_resources,
            model: mut geology,
        } = generate_geology(seed, side, &geometry, cancel)?;
        validate_tectonics(&tectonics)?;
        record(
            AtlasStage::Tectonics,
            started,
            fingerprint_tectonics(&tectonics),
        );

        let started = announce(AtlasStage::Elevation)?;
        let mut terrain = geological_terrain;
        validate_terrain(&terrain)?;
        record(
            AtlasStage::Elevation,
            started,
            fingerprint_terrain(&terrain),
        );

        let started = announce(AtlasStage::Climate)?;
        let (climate, climate_report) = generate_climate(seed, side, &geometry, &terrain, cancel)?;
        validate_climate(&climate)?;
        record(AtlasStage::Climate, started, fingerprint_climate(&climate));

        let started = announce(AtlasStage::Drainage)?;
        let HydrologyOutput {
            terrain: hydrological_terrain,
            cells: hydrology,
            model: hydrology_model,
        } = generate_hydrology(
            seed, side, &geometry, &tectonics, &terrain, &climate, cancel,
        )?;
        terrain = hydrological_terrain;
        route_placer_deposits(
            side,
            &terrain,
            &tectonics,
            &hydrology,
            &mut geological_resources,
            &mut geology,
        );
        validate_hydrology(&hydrology)?;
        record(
            AtlasStage::Drainage,
            started,
            fingerprint_hydrology(&hydrology),
        );

        let started = announce(AtlasStage::Hydrology)?;
        record(
            AtlasStage::Hydrology,
            started,
            fingerprint_hydrology(&hydrology),
        );

        let started = announce(AtlasStage::Ground)?;
        let ground = generate_ground_layer(
            seed,
            side,
            config.mode,
            &tectonics,
            &terrain,
            &climate,
            &hydrology,
        )?;
        validate_ground(&ground)?;
        record(AtlasStage::Ground, started, fingerprint_ground(&ground));

        let started = announce(AtlasStage::Biomes)?;
        let (biomes, biome_model) = generate_biomes_and_countries(
            seed,
            side,
            config.mode,
            &geometry,
            &tectonics,
            &terrain,
            &climate,
            &hydrology,
            &ground,
        )?;
        validate_biomes(&biomes)?;
        record(AtlasStage::Biomes, started, fingerprint_biomes(&biomes));

        let started = announce(AtlasStage::Resources)?;
        let resources = geological_resources;
        record(
            AtlasStage::Resources,
            started,
            fingerprint_resources(&resources),
        );

        let weather_noise = Perlin::new(seed ^ 0x5745_4154);
        let dynamic_cells = generate_grid(side, config.mode, |pos| {
            let climate_cell = climate.get(pos).expect("matching atlas grids");
            let atmospheric_vapor = ((climate_cell.mean_atmospheric_moisture * 180.0).max(1.0)
                as u32)
                .saturating_mul(HYDRO_UNITS_PER_VISIBLE_LEVEL as u32);
            let cloudiness =
                ((unit_noise(&weather_noise, pos.center(side), 4.2, [17.0, -29.0, 7.0]) + 1.0)
                    * 0.5)
                    .clamp(0.0, 1.0)
                    .powi(4);
            DynamicCell {
                atmospheric_vapor,
                cloud_water: (atmospheric_vapor as f32 * 0.20 * cloudiness) as u32,
                ..DynamicCell::default()
            }
        })?;

        let genesis = GenesisLayers {
            geometry,
            tectonics,
            terrain,
            climate,
            hydrology,
            ground,
            biomes,
            resources,
        };
        let dynamic = DynamicLayers {
            completed_climate_hours: 0,
            cells: dynamic_cells,
        };
        let atmospheric_mass = ReservoirMass::fresh(dynamic_water_total(&dynamic) as u64);
        let water_cycle = initial_water_cycle(side, &genesis, &hydrology_model, atmospheric_mass)?;
        let history = HistoryLayers {
            version: ATLAS_HISTORY_VERSION,
            ..HistoryLayers::default()
        };

        let started = announce(AtlasStage::Validation)?;
        validate_layers(side, &genesis, &dynamic)?;
        let (genesis_checksum, genesis_bytes) = {
            let payload = encode_genesis(&genesis)?;
            (
                stable_hash(&payload),
                (FILE_HEADER_BYTES + payload.len()) as u64,
            )
        };
        let (dynamic_checksum, dynamic_bytes) = {
            let payload = encode_dynamic(&dynamic)?;
            (
                stable_hash(&payload),
                (FILE_HEADER_BYTES + payload.len()) as u64,
            )
        };
        let (water_cycle_checksum, water_cycle_bytes) = {
            let payload = encode_water_cycle(&water_cycle)?;
            (
                stable_hash(&payload),
                (FILE_HEADER_BYTES + payload.len()) as u64,
            )
        };
        let (history_checksum, _history_bytes) = {
            let payload = encode_history(&history)?;
            (stable_hash(payload.as_bytes()), payload.len() as u64)
        };
        let (geology_checksum, geology_bytes) = {
            let payload = encode_geology(&geology)?;
            (stable_hash(payload.as_bytes()), payload.len() as u64)
        };
        let (hydrology_checksum, hydrology_bytes) = {
            let payload = encode_hydrology(&hydrology_model)?;
            (stable_hash(payload.as_bytes()), payload.len() as u64)
        };
        let (biome_checksum, biome_bytes) = {
            let payload = encode_biomes(&biome_model)?;
            (stable_hash(payload.as_bytes()), payload.len() as u64)
        };
        record(AtlasStage::Validation, started, genesis_checksum);

        let manifest = AtlasManifest {
            format_version: ATLAS_FORMAT_VERSION,
            seed,
            topology: crate::planet::WORLD_TOPOLOGY.to_string(),
            face_blocks: FACE_BLOCKS,
            atlas_cell_blocks: FACE_BLOCKS / side,
            atlas_face_side: side,
            atlas_cell_count: count.try_into().expect("atlas cell count fits u32"),
            topology_version: 1,
            atlas_algorithm_version: ATLAS_ALGORITHM_VERSION,
            dynamic_schema_version: ATLAS_DYNAMIC_VERSION,
            water_cycle_schema_version: WATER_CYCLE_SCHEMA_VERSION,
            history_schema_version: ATLAS_HISTORY_VERSION,
            geology_schema_version: GEOLOGY_SCHEMA_VERSION,
            hydrology_schema_version: HYDROLOGY_SCHEMA_VERSION,
            biome_schema_version: BIOME_SCHEMA_VERSION,
            planet_radius: PLANET_RADIUS,
            rotation_axis: ROTATION_AXIS,
            prime_meridian: PRIME_MERIDIAN,
            axial_tilt_degrees: AXIAL_TILT_DEGREES,
            climate_convergence_iterations: climate_report.iterations,
            climate_max_residual: climate_report.max_residual,
            climate_moisture_budget_error: climate_report.max_moisture_budget_error,
            content_hash,
            genesis_checksum,
            dynamic_checksum,
            water_cycle_checksum,
            history_checksum,
            geology_checksum,
            hydrology_checksum,
            biome_checksum,
            arcane_schema_version: 0,
            arcane_algorithm_version: 0,
            arcane_unit_scale: 0,
            arcane_genesis_total: 0,
            arcane_last_clean_total: 0,
            arcane_reservoir_totals: [0; 6],
            arcane_registry_hash: 0,
            arcane_ledger_checksum: 0,
            arcane_delta_checksum: 0,
            arcane_geography_schema_version: 0,
            arcane_geography_algorithm_version: 0,
            arcane_geography_dynamic_version: 0,
            arcane_geography_genesis_total: 0,
            arcane_geography_immutable_checksum: 0,
            arcane_geography_dynamic_checksum: 0,
            arcane_geography_site_catalog_checksum: 0,
            arcane_geography_last_authoritative_time: 0,
            genesis_bytes,
            dynamic_bytes,
            water_cycle_bytes,
            geology_bytes,
            hydrology_bytes,
            biome_bytes,
            complete: true,
            stages,
            layer_versions: layer_versions(),
        };
        let atlas = Self {
            manifest,
            genesis,
            dynamic,
            water_cycle,
            history,
            geology,
            hydrology: hydrology_model,
            biomes: biome_model,
        };
        atlas.validate()?;
        Ok(atlas)
    }

    #[cfg(test)]
    pub fn fixture(seed: u32, side: u16) -> Result<Self, AtlasError> {
        Self::generate(
            seed,
            0,
            AtlasConfig::fixture(side),
            &CancellationToken::default(),
            |_| {},
        )
    }
}
