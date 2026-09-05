//! Stable, headless atlas maps and qualification census.

use crate::chunk::{SEA_LEVEL};
use crate::planet::{Direction4, FACE_BLOCKS, Face, PLANET_RADIUS, SurfacePos, geodesic_distance, surface_to_unit};
use crate::planet_atlas::{AtlasError, AtlasPos, BIOME_BADLANDS, BIOME_DESERT, BIOME_FOREST, BIOME_JUNGLE, BIOME_MOUNTAINS, BIOME_PLAINS, BIOME_SCRUBLAND, BIOME_TAIGA, BedrockFamily, BiomeCell, BoundaryClass, ChunkWaterCommitment, ClimateCell, CountryRecord, CountryRoute, DetailedBoundary, DynamicCell, EDAPHIC_STEEP, FREEZE_SEASONAL, FluxInbox, GeometryCell, GraftCompatibility, GroundCell, HABITAT_ALPINE, HABITAT_AQUATIC_BRACKISH, HABITAT_AQUATIC_FRESH, HABITAT_AQUATIC_SALT, HABITAT_BEACH_DUNE, HABITAT_FLOODPLAIN, HABITAT_LAKESHORE, HABITAT_NAMES, HABITAT_OASIS, HABITAT_RIPARIAN, HABITAT_SALT_MARSH, HABITAT_SPRING, HABITAT_WETLAND, HYDRO_DELTA, HYDRO_ESTUARY, HYDRO_FLOODPLAIN, HYDRO_RIVER, HYDRO_WETLAND, HydrologyCell, LocalWeather, MineralKind, PlanetAtlas, PlanetaryWeather, ResourceCell, SparseAquiferState, SpringState, SurfaceReservoirKind, TectonicCell, TerrainCell, VolcanoSource, WaterCell, weather_sample};
use crate::planet_atlas::identity::{mix64};
use glam::{DVec3};
use serde::{Deserialize, Serialize};
use std::{fs};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufWriter};
use std::path::{Path};
use std::time::{Instant};




const FACE_LAYOUT: &str =
    "3x2: pos_x,neg_x,pos_y / neg_y,pos_z,neg_z; u left-to-right, v top-to-bottom";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AtlasCensus {
    pub cells: usize,
    pub cells_by_face: BTreeMap<String, usize>,
    pub physical_area: f64,
    pub land_area: f64,
    pub ocean_area: f64,
    pub land_fraction: f64,
    pub ocean_fraction: f64,
    pub continent_and_island_count: u32,
    pub elevation_histogram: Vec<u64>,
    pub boundary_cells: BTreeMap<String, u64>,
    pub detailed_boundary_cells: BTreeMap<String, u64>,
    pub plate_count: usize,
    pub craton_count: usize,
    pub major_continent_count: usize,
    pub chosen_geology_attempt: u8,
    pub rejected_geology_attempts: usize,
    pub largest_ocean_share: f64,
    pub bedrock_areas: BTreeMap<String, f64>,
    pub oceanic_age_histogram: Vec<u64>,
    pub volcanoes_by_source: BTreeMap<String, usize>,
    pub intrusions_by_kind: BTreeMap<String, usize>,
    pub deposits_by_mineral: BTreeMap<String, usize>,
    pub deposit_tonnage_by_mineral: BTreeMap<String, u64>,
    pub progression_sites_by_continent: BTreeMap<String, usize>,
    pub climate_zone_areas: BTreeMap<String, f64>,
    pub area_mean_temperature_c: f64,
    pub minimum_temperature_c: f32,
    pub maximum_temperature_c: f32,
    pub area_mean_precipitation_mm: f64,
    pub maximum_precipitation_mm: f32,
    pub area_mean_aridity: f64,
    pub maximum_aridity: f32,
    pub biome_areas: BTreeMap<u16, f64>,
    pub biome_area_by_latitude_band: BTreeMap<String, f64>,
    pub biome_area_by_continent: BTreeMap<String, f64>,
    pub biome_area_by_elevation_band: BTreeMap<String, f64>,
    pub biome_area_by_water_availability: BTreeMap<String, f64>,
    pub habitat_areas: BTreeMap<String, f64>,
    pub drainage_edge_count: u64,
    pub river_length_blocks: f64,
    pub river_receiver_distance_histogram: Vec<u64>,
    pub named_river_count: usize,
    pub maximum_discharge: f32,
    pub maximum_channel_width_blocks: f32,
    pub maximum_stream_order: u8,
    pub ocean_basin_count: usize,
    pub lake_count: usize,
    pub lake_classes: BTreeMap<String, usize>,
    pub terminal_lake_count: usize,
    pub exorheic_lake_count: usize,
    pub watershed_count: usize,
    pub floodplain_cells: u64,
    pub wetland_cells: u64,
    pub delta_cells: u64,
    pub estuary_cells: u64,
    pub baseline_surface_water_units: u128,
    pub voxel_volume_residual: i128,
    pub freshwater_units: u128,
    pub total_water_units: u128,
    pub aquifer_storage_units: u128,
    pub resource_site_count: usize,
    pub nearest_resource_distance_bands: BTreeMap<String, u64>,
    pub max_nearest_deposit_distance_blocks: BTreeMap<String, f64>,
    pub province_count: usize,
    pub heart_count: usize,
    pub country_route_count: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AtlasExportReport {
    pub validation: String,
    pub layout: String,
    pub registered_layers: Vec<String>,
    pub exported_maps: Vec<String>,
    pub cell_count: usize,
    pub stage_elapsed_micros: BTreeMap<String, u64>,
    pub immutable_bytes: u64,
    pub dynamic_bytes: u64,
    pub water_cycle_bytes: u64,
    pub geology_bytes: u64,
    pub hydrology_bytes: u64,
    pub biome_bytes: u64,
    pub estimated_loaded_bytes: u64,
    pub peak_resident_bytes: Option<u64>,
    pub export_elapsed_micros: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct QualificationSite {
    face: String,
    atlas_u: u16,
    atlas_v: u16,
    surface_u: u16,
    surface_v: u16,
    spawn: String,
    local_noon: f32,
    evidence: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
struct QualificationSites {
    sites: BTreeMap<String, QualificationSite>,
}

#[derive(Clone, Copy)]
enum LayerKind {
    Scalar,
    Categorical,
    Direction,
}

#[derive(Clone, Copy)]
struct LayerSpec {
    id: &'static str,
    description: &'static str,
    kind: LayerKind,
}

const LAYERS: &[LayerSpec] = &[
    layer("latitude", "latitude in radians", LayerKind::Scalar),
    layer(
        "physical_area",
        "spherical cell area in block squared",
        LayerKind::Scalar,
    ),
    layer(
        "plate_id",
        "tectonic plate identifier",
        LayerKind::Categorical,
    ),
    layer(
        "boundary",
        "0 interior, 1 convergent, 2 divergent, 3 transform",
        LayerKind::Categorical,
    ),
    layer(
        "boundary_type",
        "causal connected boundary class",
        LayerKind::Categorical,
    ),
    layer(
        "boundary_strength",
        "relative plate motion magnitude",
        LayerKind::Scalar,
    ),
    layer(
        "boundary_distance",
        "distance to nearest plate boundary in cells",
        LayerKind::Scalar,
    ),
    layer(
        "boundary_strike",
        "local strike of the nearest connected boundary run",
        LayerKind::Direction,
    ),
    layer(
        "plate_velocity",
        "Euler-pole plate velocity in the local tangent frame",
        LayerKind::Direction,
    ),
    layer(
        "euler_poles",
        "explicit Euler rotation-pole locations, colored by plate identifier",
        LayerKind::Categorical,
    ),
    layer(
        "continental_crust",
        "continental crust fraction",
        LayerKind::Scalar,
    ),
    layer(
        "crust_age",
        "crust age in millions of years",
        LayerKind::Scalar,
    ),
    layer(
        "oceanic_age",
        "oceanic crust age away from spreading ridges",
        LayerKind::Scalar,
    ),
    layer(
        "crust_thickness",
        "causal crust thickness",
        LayerKind::Scalar,
    ),
    layer(
        "craton_id",
        "ancient continental craton group",
        LayerKind::Categorical,
    ),
    layer(
        "bedrock_family",
        "bedrock family identifier",
        LayerKind::Categorical,
    ),
    layer(
        "geological_province",
        "geological province identifier",
        LayerKind::Categorical,
    ),
    layer(
        "base_elevation",
        "pre-erosion elevation in blocks",
        LayerKind::Scalar,
    ),
    layer(
        "eroded_elevation",
        "committed elevation in blocks",
        LayerKind::Scalar,
    ),
    layer(
        "tectonic_contribution",
        "elevation contribution from boundary deformation",
        LayerKind::Scalar,
    ),
    layer(
        "volcanic_contribution",
        "elevation contribution from volcanic construction",
        LayerKind::Scalar,
    ),
    layer(
        "dynamic_topography",
        "long-wavelength mantle topography contribution",
        LayerKind::Scalar,
    ),
    layer(
        "landmass_id",
        "connected emerged landmass",
        LayerKind::Categorical,
    ),
    layer(
        "stratigraphic_stack",
        "causal stratigraphic stack identifier",
        LayerKind::Categorical,
    ),
    layer(
        "metamorphic_grade",
        "regional or contact metamorphic grade",
        LayerKind::Scalar,
    ),
    layer(
        "fault_intensity",
        "fault and fracture intensity",
        LayerKind::Scalar,
    ),
    layer(
        "sediment_basin",
        "sedimentary environment and basin history",
        LayerKind::Categorical,
    ),
    layer(
        "volcanic_history",
        "tectonic volcanic-history flag",
        LayerKind::Categorical,
    ),
    layer(
        "ocean_depth",
        "depth below sea level in blocks",
        LayerKind::Scalar,
    ),
    layer(
        "mean_temperature",
        "annual mean temperature in Celsius",
        LayerKind::Scalar,
    ),
    layer("seasonality", "annual temperature range", LayerKind::Scalar),
    layer(
        "ocean_temperature_anomaly",
        "surface-current coastal temperature anomaly",
        LayerKind::Scalar,
    ),
    layer(
        "continentality",
        "normalized geodesic distance from ocean influence",
        LayerKind::Scalar,
    ),
    layer(
        "mean_atmospheric_moisture",
        "equilibrium atmospheric moisture",
        LayerKind::Scalar,
    ),
    layer(
        "mean_precipitation",
        "annual precipitation",
        LayerKind::Scalar,
    ),
    layer(
        "precipitation_seasonality",
        "seasonal precipitation contrast",
        LayerKind::Scalar,
    ),
    layer(
        "potential_evapotranspiration",
        "potential evapotranspiration",
        LayerKind::Scalar,
    ),
    layer("aridity", "aridity index", LayerKind::Scalar),
    layer(
        "snow_persistence",
        "annual fraction of precipitation retained as snow",
        LayerKind::Scalar,
    ),
    layer(
        "prevailing_wind",
        "local tangent-frame wind direction",
        LayerKind::Direction,
    ),
    layer(
        "ocean_current",
        "wind-driven surface-current direction",
        LayerKind::Direction,
    ),
    layer(
        "spring_temperature",
        "spring temperature",
        LayerKind::Scalar,
    ),
    layer(
        "summer_temperature",
        "summer temperature",
        LayerKind::Scalar,
    ),
    layer(
        "autumn_temperature",
        "autumn temperature",
        LayerKind::Scalar,
    ),
    layer(
        "winter_temperature",
        "winter temperature",
        LayerKind::Scalar,
    ),
    layer(
        "spring_precipitation",
        "spring precipitation",
        LayerKind::Scalar,
    ),
    layer(
        "summer_precipitation",
        "summer precipitation",
        LayerKind::Scalar,
    ),
    layer(
        "autumn_precipitation",
        "autumn precipitation",
        LayerKind::Scalar,
    ),
    layer(
        "winter_precipitation",
        "winter precipitation",
        LayerKind::Scalar,
    ),
    layer(
        "spring_wind",
        "spring prevailing wind",
        LayerKind::Direction,
    ),
    layer(
        "summer_wind",
        "summer prevailing wind",
        LayerKind::Direction,
    ),
    layer(
        "autumn_wind",
        "autumn prevailing wind",
        LayerKind::Direction,
    ),
    layer(
        "winter_wind",
        "winter prevailing wind",
        LayerKind::Direction,
    ),
    layer(
        "drainage_receiver",
        "global downstream cell index",
        LayerKind::Categorical,
    ),
    layer(
        "watershed_id",
        "watershed identifier",
        LayerKind::Categorical,
    ),
    layer(
        "ocean_basin_id",
        "ocean basin identifier",
        LayerKind::Categorical,
    ),
    layer(
        "lake_basin_id",
        "lake basin identifier",
        LayerKind::Categorical,
    ),
    layer(
        "spill_elevation",
        "basin spill elevation",
        LayerKind::Scalar,
    ),
    layer(
        "depression_depth",
        "priority-flood depression depth",
        LayerKind::Scalar,
    ),
    layer("mean_runoff", "mean annual runoff", LayerKind::Scalar),
    layer(
        "mean_discharge",
        "accumulated channel discharge",
        LayerKind::Scalar,
    ),
    layer(
        "catchment_area",
        "upstream contributing area",
        LayerKind::Scalar,
    ),
    layer("river_id", "named river identifier", LayerKind::Categorical),
    layer("stream_order", "Strahler stream order", LayerKind::Scalar),
    layer("channel_width", "bankfull channel width", LayerKind::Scalar),
    layer("channel_depth", "bankfull channel depth", LayerKind::Scalar),
    layer(
        "channel_bed_elevation",
        "monotonic channel bed elevation",
        LayerKind::Scalar,
    ),
    layer(
        "water_surface_elevation",
        "baseline water-surface elevation",
        LayerKind::Scalar,
    ),
    layer(
        "water_body",
        "ocean, river, lake, playa, delta, estuary, or wetland",
        LayerKind::Categorical,
    ),
    layer(
        "salinity",
        "baseline salinity concentration",
        LayerKind::Scalar,
    ),
    layer(
        "sediment_energy",
        "normalized sediment transport energy",
        LayerKind::Scalar,
    ),
    layer("erosion", "hydrological erosion depth", LayerKind::Scalar),
    layer(
        "deposition",
        "hydrological deposition depth",
        LayerKind::Scalar,
    ),
    layer(
        "seasonal_water_range",
        "seasonal lake-level range",
        LayerKind::Scalar,
    ),
    layer(
        "baseline_water_volume",
        "baseline water volume in eighth-block units",
        LayerKind::Scalar,
    ),
    layer(
        "voxel_volume_residual",
        "coarse storage retained after voxel quantization",
        LayerKind::Scalar,
    ),
    layer(
        "floodplain",
        "floodplain habitat mask",
        LayerKind::Categorical,
    ),
    layer("wetland", "wetland habitat mask", LayerKind::Categorical),
    layer("delta", "deltaic deposition mask", LayerKind::Categorical),
    layer("estuary", "brackish estuary mask", LayerKind::Categorical),
    layer(
        "soil_parent_material",
        "soil parent family",
        LayerKind::Categorical,
    ),
    layer(
        "aquifer_capacity",
        "baseline aquifer capacity",
        LayerKind::Scalar,
    ),
    layer(
        "aquifer_permeability",
        "normalized aquifer permeability",
        LayerKind::Scalar,
    ),
    layer(
        "porosity",
        "normalized primary porosity seed",
        LayerKind::Scalar,
    ),
    layer(
        "groundwater_head",
        "baseline groundwater head",
        LayerKind::Scalar,
    ),
    layer("soil_depth", "soil depth in decimeters", LayerKind::Scalar),
    layer("soil_sand", "soil sand fraction", LayerKind::Scalar),
    layer("soil_silt", "soil silt fraction", LayerKind::Scalar),
    layer("soil_clay", "soil clay fraction", LayerKind::Scalar),
    layer("soil_organic", "soil organic content", LayerKind::Scalar),
    layer(
        "soil_fertility",
        "baseline soil fertility",
        LayerKind::Scalar,
    ),
    layer("soil_drainage", "soil drainage class", LayerKind::Scalar),
    layer("soil_salinity", "baseline soil salinity", LayerKind::Scalar),
    layer(
        "soil_freeze",
        "seasonal freeze and permafrost flags",
        LayerKind::Categorical,
    ),
    layer(
        "soil_erosion_susceptibility",
        "soil erosion susceptibility",
        LayerKind::Scalar,
    ),
    layer(
        "baseline_biome",
        "baseline biome identifier",
        LayerKind::Categorical,
    ),
    layer(
        "habitat_flags",
        "local habitat bit flags",
        LayerKind::Categorical,
    ),
    layer(
        "edaphic_flags",
        "soil and terrain modifier flags",
        LayerKind::Categorical,
    ),
    layer(
        "vegetation_potential",
        "water/energy-limited vegetation potential",
        LayerKind::Scalar,
    ),
    layer(
        "tree_line",
        "latitude-sensitive tree-line elevation",
        LayerKind::Scalar,
    ),
    layer(
        "succession_potential",
        "natural succession and regrowth potential",
        LayerKind::Scalar,
    ),
    layer(
        "province_id",
        "legacy alias for the geographic country identifier",
        LayerKind::Categorical,
    ),
    layer(
        "country_id",
        "geographic country identifier",
        LayerKind::Categorical,
    ),
    layer(
        "heart_assignment",
        "assigned country-heart identifier",
        LayerKind::Categorical,
    ),
    layer(
        "country_boundaries",
        "geographic country boundaries and traversable contacts",
        LayerKind::Categorical,
    ),
    layer(
        "hearts_edifices",
        "country heart sites colored by geographic form",
        LayerKind::Categorical,
    ),
    layer(
        "graft_compatibility_jungle",
        "example jungle graft: 1 compatible, 2 marginal, 3 incompatible",
        LayerKind::Categorical,
    ),
    layer(
        "animal_frog_suitability",
        "example freshwater-wetland animal suitability",
        LayerKind::Categorical,
    ),
    layer(
        "animal_seal_suitability",
        "example cold marine-coast animal suitability",
        LayerKind::Categorical,
    ),
    layer(
        "deposit_site",
        "finite deposit-site reference",
        LayerKind::Categorical,
    ),
    layer(
        "deposit_site_count",
        "number of finite deposit sites centered in the cell",
        LayerKind::Scalar,
    ),
    layer(
        "geological_sites",
        "volcano, intrusion, and deposit site overlay",
        LayerKind::Categorical,
    ),
    layer(
        "atmospheric_vapor",
        "mutable atmospheric water",
        LayerKind::Scalar,
    ),
    layer("cloud_water", "mutable cloud water", LayerKind::Scalar),
    layer("soil_moisture", "mutable soil water", LayerKind::Scalar),
    layer("snowpack", "mutable snow storage", LayerKind::Scalar),
    layer(
        "groundwater_volume",
        "mutable groundwater storage",
        LayerKind::Scalar,
    ),
    layer(
        "groundwater_head_anomaly",
        "mutable groundwater head anomaly",
        LayerKind::Scalar,
    ),
    layer(
        "surface_runoff",
        "mutable surface runoff",
        LayerKind::Scalar,
    ),
    layer(
        "groundwater_recharge",
        "water recharged into the shallow aquifer last hour",
        LayerKind::Scalar,
    ),
    layer(
        "spring_discharge",
        "pressure-driven spring discharge last hour",
        LayerKind::Scalar,
    ),
    layer(
        "water_salinity",
        "derived shallow-water salinity",
        LayerKind::Scalar,
    ),
    layer(
        "lake_storage_anomaly",
        "mutable lake storage anomaly",
        LayerKind::Scalar,
    ),
    layer(
        "ocean_storage_anomaly",
        "mutable ocean storage anomaly",
        LayerKind::Scalar,
    ),
    layer(
        "local_weather_anomaly",
        "mutable local weather anomaly",
        LayerKind::Scalar,
    ),
    layer(
        "weather_temperature_anomaly",
        "mutable near-surface temperature anomaly",
        LayerKind::Scalar,
    ),
    layer(
        "pressure_anomaly",
        "mutable pressure anomaly",
        LayerKind::Scalar,
    ),
    layer("storm_energy", "mutable storm energy", LayerKind::Scalar),
    layer(
        "precipitation_rate",
        "conservative cloud-water transfer in the last climate hour",
        LayerKind::Scalar,
    ),
    layer(
        "weather_wind_anomaly",
        "mutable local wind anomaly",
        LayerKind::Direction,
    ),
    layer(
        "fire_moisture_anomaly",
        "mutable fire moisture anomaly",
        LayerKind::Scalar,
    ),
    layer(
        "vegetation_moisture_anomaly",
        "mutable vegetation moisture anomaly",
        LayerKind::Scalar,
    ),
];

const fn layer(id: &'static str, description: &'static str, kind: LayerKind) -> LayerSpec {
    LayerSpec {
        id,
        description,
        kind,
    }
}

impl PlanetAtlas {
    pub fn census(&self) -> Result<AtlasCensus, AtlasError> {
        self.validate()?;
        let side = self.side();
        let mut cells_by_face = BTreeMap::new();
        for face in Face::ALL {
            cells_by_face.insert(face.name().to_string(), usize::from(side).pow(2));
        }
        let mut physical_area = 0.0f64;
        let mut land_area = 0.0f64;
        let mut ocean_area = 0.0f64;
        let mut elevation_histogram = vec![0u64; 16];
        let mut boundary_cells = BTreeMap::from([
            ("interior".to_string(), 0),
            ("convergent".to_string(), 0),
            ("divergent".to_string(), 0),
            ("transform".to_string(), 0),
        ]);
        let mut detailed_boundary_cells = BTreeMap::<String, u64>::new();
        let mut bedrock_areas = BTreeMap::<String, f64>::new();
        let mut oceanic_age_histogram = vec![0u64; 12];
        let mut climate_zone_areas = BTreeMap::<String, f64>::new();
        let mut temperature_area_sum = 0.0f64;
        let mut minimum_temperature = f32::INFINITY;
        let mut maximum_temperature = f32::NEG_INFINITY;
        let mut precipitation_area_sum = 0.0f64;
        let mut maximum_precipitation = 0.0f32;
        let mut aridity_area_sum = 0.0f64;
        let mut maximum_aridity = 0.0f32;
        let mut biome_areas = BTreeMap::<u16, f64>::new();
        let mut biome_area_by_latitude_band = BTreeMap::<String, f64>::new();
        let mut biome_area_by_continent = BTreeMap::<String, f64>::new();
        let mut biome_area_by_elevation_band = BTreeMap::<String, f64>::new();
        let mut biome_area_by_water_availability = BTreeMap::<String, f64>::new();
        let mut habitat_areas = BTreeMap::<String, f64>::new();
        let mut watersheds = BTreeSet::new();
        let mut lakes = BTreeSet::new();
        let mut provinces = BTreeSet::new();
        let mut hearts = BTreeSet::new();
        let mut drainage_edge_count = 0u64;
        let mut river_length_blocks = 0.0f64;
        let mut receiver_histogram = vec![0u64; 8];
        let mut aquifer_storage_units = 0u128;
        let mut freshwater_units = 0u128;
        let mut total_water_units = 0u128;
        let mut floodplain_cells = 0u64;
        let mut wetland_cells = 0u64;
        let mut delta_cells = 0u64;
        let mut estuary_cells = 0u64;
        let mut resource_sites = Vec::new();

        for (pos, geometry) in self.genesis.geometry.iter() {
            let area = f64::from(geometry.physical_area);
            physical_area += area;
            let terrain = self.genesis.terrain.get(pos).expect("matching atlas grids");
            let tectonics = self
                .genesis
                .tectonics
                .get(pos)
                .expect("matching atlas grids");
            let climate = self.genesis.climate.get(pos).expect("matching atlas grids");
            let hydrology = self
                .genesis
                .hydrology
                .get(pos)
                .expect("matching atlas grids");
            let biome = self.genesis.biomes.get(pos).expect("matching atlas grids");
            let resource = self
                .genesis
                .resources
                .get(pos)
                .expect("matching atlas grids");
            let dynamic = self.dynamic.cells.get(pos).expect("matching atlas grids");
            let water = self
                .water_cycle
                .cells
                .get(pos)
                .expect("matching water grid");
            let land = terrain.eroded_elevation > SEA_LEVEL as f32;
            if land {
                land_area += area;
            } else {
                ocean_area += area;
            }
            let elevation_bin = (((terrain.eroded_elevation - 0.0) / 256.0) * 16.0)
                .floor()
                .clamp(0.0, 15.0) as usize;
            elevation_histogram[elevation_bin] += 1;
            let boundary = match tectonics.boundary {
                BoundaryClass::Interior => "interior",
                BoundaryClass::Convergent => "convergent",
                BoundaryClass::Divergent => "divergent",
                BoundaryClass::Transform => "transform",
            };
            *boundary_cells.get_mut(boundary).expect("known boundary") += 1;
            *detailed_boundary_cells
                .entry(format!("{:?}", tectonics.boundary_detail).to_lowercase())
                .or_default() += 1;
            *bedrock_areas
                .entry(
                    BedrockFamily::from_id(tectonics.bedrock_family)
                        .label()
                        .to_string(),
                )
                .or_default() += area;
            if tectonics.continental_crust < 32_768 {
                let bin = (usize::from(tectonics.oceanic_age) / 20).min(11);
                oceanic_age_histogram[bin] += 1;
            }
            let climate_zone = if climate.aridity > 1.2 {
                "dry"
            } else if climate.mean_temperature < 0.0 {
                "polar"
            } else if climate.mean_temperature < 18.0 {
                "temperate"
            } else {
                "tropical"
            };
            *climate_zone_areas.entry(climate_zone.into()).or_default() += area;
            temperature_area_sum += f64::from(climate.mean_temperature) * area;
            minimum_temperature = minimum_temperature.min(climate.mean_temperature);
            maximum_temperature = maximum_temperature.max(climate.mean_temperature);
            precipitation_area_sum += f64::from(climate.mean_precipitation) * area;
            maximum_precipitation = maximum_precipitation.max(climate.mean_precipitation);
            aridity_area_sum += f64::from(climate.aridity) * area;
            maximum_aridity = maximum_aridity.max(climate.aridity);
            *biome_areas
                .entry(u16::from(biome.baseline_biome))
                .or_default() += area;
            let latitude_band = ((geometry.latitude_radians.to_degrees() + 90.0) / 15.0)
                .floor()
                .clamp(0.0, 11.0) as i32;
            *biome_area_by_latitude_band
                .entry(format!(
                    "biome_{}_band_{latitude_band}",
                    biome.baseline_biome
                ))
                .or_default() += area;
            if land {
                *biome_area_by_continent
                    .entry(format!(
                        "biome_{}_continent_{}",
                        biome.baseline_biome, terrain.landmass_id
                    ))
                    .or_default() += area;
            }
            let elevation_band = ((terrain.eroded_elevation - SEA_LEVEL as f32) / 32.0)
                .floor()
                .clamp(-4.0, 7.0) as i32;
            *biome_area_by_elevation_band
                .entry(format!(
                    "biome_{}_elevation_{elevation_band}",
                    biome.baseline_biome
                ))
                .or_default() += area;
            let water_class = if biome.habitat_flags
                & (HABITAT_AQUATIC_FRESH | HABITAT_AQUATIC_BRACKISH | HABITAT_AQUATIC_SALT)
                != 0
            {
                "aquatic"
            } else if biome.habitat_flags
                & (HABITAT_RIPARIAN | HABITAT_WETLAND | HABITAT_OASIS | HABITAT_SPRING)
                != 0
            {
                "water_connected"
            } else if climate.aridity >= 1.02 {
                "water_limited"
            } else {
                "mesic"
            };
            *biome_area_by_water_availability
                .entry(format!("biome_{}_{water_class}", biome.baseline_biome))
                .or_default() += area;
            for (flag, name) in HABITAT_NAMES {
                if biome.habitat_flags & *flag != 0 {
                    *habitat_areas.entry((*name).to_string()).or_default() += area;
                }
            }
            if hydrology.watershed_id != 0 {
                watersheds.insert(hydrology.watershed_id);
            }
            if hydrology.lake_basin_id != 0 {
                lakes.insert(hydrology.lake_basin_id);
            }
            if hydrology.drainage_receiver != u32::MAX {
                drainage_edge_count += 1;
                if hydrology.flags & HYDRO_RIVER != 0
                    && let Some(receiver) =
                        AtlasPos::from_index(hydrology.drainage_receiver as usize, side)
                {
                    let distance = geodesic_distance(pos.center(side), receiver.center(side));
                    river_length_blocks += distance;
                    let bin = (distance / f64::from(self.cell_blocks()))
                        .floor()
                        .clamp(0.0, 7.0) as usize;
                    receiver_histogram[bin] += 1;
                }
            }
            floodplain_cells += u64::from(u8::from(hydrology.flags & HYDRO_FLOODPLAIN != 0));
            wetland_cells += u64::from(u8::from(hydrology.flags & HYDRO_WETLAND != 0));
            delta_cells += u64::from(u8::from(hydrology.flags & HYDRO_DELTA != 0));
            estuary_cells += u64::from(u8::from(hydrology.flags & HYDRO_ESTUARY != 0));
            if biome.country_id != 0 {
                provinces.insert(u32::from(biome.country_id));
            }
            if biome.heart_assignment != 0 {
                hearts.insert(biome.heart_assignment);
            }
            if resource.deposit_site_ref != 0 {
                resource_sites.push(pos);
            }
            aquifer_storage_units += u128::from(water.groundwater.water_hu);
            freshwater_units += u128::from(dynamic.cloud_water)
                + u128::from(water.soil.water_hu)
                + u128::from(water.snow.water_hu)
                + u128::from(water.groundwater.water_hu)
                + u128::from(water.runoff.water_hu);
            total_water_units += u128::from(dynamic.atmospheric_vapor)
                + u128::from(dynamic.cloud_water)
                + u128::from(water.soil.water_hu)
                + u128::from(water.snow.water_hu)
                + u128::from(water.groundwater.water_hu)
                + u128::from(water.runoff.water_hu);
        }
        for reservoir in &self.water_cycle.reservoirs {
            total_water_units +=
                u128::from(reservoir.coarse.water_hu) + u128::from(reservoir.committed.water_hu);
            if reservoir.coarse.salinity() < 64 {
                freshwater_units += u128::from(reservoir.coarse.water_hu);
            }
        }
        for aquifer in &self.water_cycle.aquifers {
            aquifer_storage_units += u128::from(aquifer.mass.water_hu);
            total_water_units += u128::from(aquifer.mass.water_hu);
        }
        let (_, continent_and_island_count) = self
            .connected_components(&self.genesis.terrain, |cell| {
                cell.eroded_elevation > SEA_LEVEL as f32
            })?;
        let nearest_resource_distance_bands = resource_distance_bands(self, &resource_sites);
        let mut max_nearest_deposit_distance_blocks = BTreeMap::new();
        for mineral in MineralKind::ALL_TRACKED {
            let sites: Vec<_> = self
                .geology
                .deposits
                .iter()
                .filter(|site| site.mineral == mineral)
                .map(|site| site.pos)
                .collect();
            let maximum = self
                .genesis
                .terrain
                .iter()
                .filter(|(_, terrain)| terrain.eroded_elevation > SEA_LEVEL as f32)
                .map(|(pos, _)| {
                    sites
                        .iter()
                        .map(|site| geodesic_distance(pos.center(side), site.center(side)))
                        .fold(f64::INFINITY, f64::min)
                })
                .fold(0.0f64, f64::max);
            max_nearest_deposit_distance_blocks.insert(mineral.label().to_string(), maximum);
        }
        let mut volcanoes_by_source = BTreeMap::new();
        for site in &self.geology.volcanoes {
            *volcanoes_by_source
                .entry(format!("{:?}", site.source).to_lowercase())
                .or_default() += 1;
        }
        let mut intrusions_by_kind = BTreeMap::new();
        for site in &self.geology.intrusions {
            *intrusions_by_kind
                .entry(format!("{:?}", site.kind).to_lowercase())
                .or_default() += 1;
        }
        let mut deposits_by_mineral = BTreeMap::new();
        let mut deposit_tonnage_by_mineral = BTreeMap::new();
        let mut progression_sites_by_continent = BTreeMap::new();
        for site in &self.geology.deposits {
            *deposits_by_mineral
                .entry(site.mineral.label().to_string())
                .or_default() += 1;
            *deposit_tonnage_by_mineral
                .entry(site.mineral.label().to_string())
                .or_default() += site.tonnage_blocks;
            *progression_sites_by_continent
                .entry(format!(
                    "landmass_{}_{}",
                    site.landmass_id,
                    site.mineral.label()
                ))
                .or_default() += 1;
        }
        let mut lake_classes = BTreeMap::new();
        for lake in &self.hydrology.lakes {
            *lake_classes
                .entry(format!("{:?}", lake.class).to_lowercase())
                .or_default() += 1;
        }
        Ok(AtlasCensus {
            cells: self.genesis.geometry.len(),
            cells_by_face,
            physical_area,
            land_area,
            ocean_area,
            land_fraction: land_area / physical_area,
            ocean_fraction: ocean_area / physical_area,
            continent_and_island_count,
            elevation_histogram,
            boundary_cells,
            detailed_boundary_cells,
            plate_count: self.geology.plates.len(),
            craton_count: self.geology.cratons.len(),
            major_continent_count: self
                .geology
                .continents
                .iter()
                .filter(|continent| continent.major)
                .count(),
            chosen_geology_attempt: self.geology.chosen_attempt,
            rejected_geology_attempts: self.geology.rejected_attempts.len(),
            largest_ocean_share: self.geology.largest_ocean_share,
            bedrock_areas,
            oceanic_age_histogram,
            volcanoes_by_source,
            intrusions_by_kind,
            deposits_by_mineral,
            deposit_tonnage_by_mineral,
            progression_sites_by_continent,
            climate_zone_areas,
            area_mean_temperature_c: temperature_area_sum / physical_area,
            minimum_temperature_c: minimum_temperature,
            maximum_temperature_c: maximum_temperature,
            area_mean_precipitation_mm: precipitation_area_sum / physical_area,
            maximum_precipitation_mm: maximum_precipitation,
            area_mean_aridity: aridity_area_sum / physical_area,
            maximum_aridity,
            biome_areas,
            biome_area_by_latitude_band,
            biome_area_by_continent,
            biome_area_by_elevation_band,
            biome_area_by_water_availability,
            habitat_areas,
            drainage_edge_count,
            river_length_blocks,
            river_receiver_distance_histogram: receiver_histogram,
            named_river_count: self.hydrology.rivers.len(),
            maximum_discharge: self
                .hydrology
                .rivers
                .iter()
                .map(|river| river.maximum_discharge)
                .fold(0.0, f32::max),
            maximum_channel_width_blocks: self
                .hydrology
                .rivers
                .iter()
                .map(|river| river.maximum_width_blocks)
                .fold(0.0, f32::max),
            maximum_stream_order: self
                .hydrology
                .rivers
                .iter()
                .map(|river| river.stream_order)
                .max()
                .unwrap_or(0),
            ocean_basin_count: self.hydrology.oceans.len(),
            lake_count: lakes.len(),
            lake_classes,
            terminal_lake_count: self
                .hydrology
                .lakes
                .iter()
                .filter(|lake| lake.outlet.is_none())
                .count(),
            exorheic_lake_count: self
                .hydrology
                .lakes
                .iter()
                .filter(|lake| lake.outlet.is_some())
                .count(),
            watershed_count: watersheds.len(),
            floodplain_cells,
            wetland_cells,
            delta_cells,
            estuary_cells,
            baseline_surface_water_units: self.hydrology.baseline_surface_water_units,
            voxel_volume_residual: self.hydrology.voxel_volume_residual,
            freshwater_units,
            total_water_units,
            aquifer_storage_units,
            resource_site_count: resource_sites.len(),
            nearest_resource_distance_bands,
            max_nearest_deposit_distance_blocks,
            province_count: provinces.len(),
            heart_count: hearts.len(),
            country_route_count: self
                .biomes
                .countries
                .iter()
                .map(|country| country.routes.len())
                .sum::<usize>()
                / 2,
        })
    }
}

fn resource_distance_bands(atlas: &PlanetAtlas, sites: &[AtlasPos]) -> BTreeMap<String, u64> {
    let mut out = BTreeMap::from([
        ("0".into(), 0),
        ("1-512 blocks".into(), 0),
        ("513-2048 blocks".into(), 0),
        ("2049-4096 blocks".into(), 0),
        ("4097+ blocks".into(), 0),
        ("unavailable".into(), 0),
    ]);
    for (pos, _) in atlas.genesis.geometry.iter() {
        let distance = sites
            .iter()
            .map(|site| geodesic_distance(pos.center(atlas.side()), site.center(atlas.side())))
            .fold(f64::INFINITY, f64::min);
        let band = match distance {
            value if !value.is_finite() => "unavailable",
            value if value < 0.5 => "0",
            value if value <= 512.0 => "1-512 blocks",
            value if value <= 2_048.0 => "513-2048 blocks",
            value if value <= 4_096.0 => "2049-4096 blocks",
            _ => "4097+ blocks",
        };
        *out.get_mut(band).expect("known resource band") += 1;
    }
    out
}

pub fn export_diagnostics(
    atlas: &PlanetAtlas,
    output: &Path,
) -> Result<AtlasExportReport, AtlasError> {
    let started = Instant::now();
    atlas.validate()?;
    fs::create_dir_all(output)?;
    let map_dir = output.join("maps");
    fs::create_dir_all(&map_dir)?;
    let mut exported_maps = Vec::with_capacity(LAYERS.len());
    for spec in LAYERS {
        export_layer(atlas, *spec, &map_dir)?;
        exported_maps.push(format!("maps/{}.png", spec.id));
    }
    export_climate_transects(atlas, &output.join("climate-transects.csv"))?;
    export_river_profiles(atlas, &output.join("river-profiles.csv"))?;
    export_weather_examples(atlas, &map_dir, &output.join("weather-tracks.csv"))?;
    export_globe_preview(atlas, &output.join("globe_preview.png"))?;

    let census = atlas.census()?;
    let census_toml = toml::to_string_pretty(&census)
        .map_err(|error| AtlasError::Corrupt(format!("census encoding failed: {error}")))?;
    crate::persist::atomic_write(&output.join("census.toml"), census_toml.as_bytes(), false)?;
    write_census_csv(&census, &output.join("census.csv"))?;
    let manifest = toml::to_string_pretty(&atlas.manifest)
        .map_err(|error| AtlasError::Corrupt(format!("manifest encoding failed: {error}")))?;
    crate::persist::atomic_write(&output.join("manifest.toml"), manifest.as_bytes(), false)?;
    let geology = toml::to_string_pretty(&atlas.geology)
        .map_err(|error| AtlasError::Corrupt(format!("geology export failed: {error}")))?;
    crate::persist::atomic_write(&output.join("geology.toml"), geology.as_bytes(), false)?;
    let hydrology = toml::to_string_pretty(&atlas.hydrology)
        .map_err(|error| AtlasError::Corrupt(format!("hydrology export failed: {error}")))?;
    crate::persist::atomic_write(&output.join("hydrology.toml"), hydrology.as_bytes(), false)?;
    let biomes = toml::to_string_pretty(&atlas.biomes)
        .map_err(|error| AtlasError::Corrupt(format!("biome export failed: {error}")))?;
    crate::persist::atomic_write(&output.join("biomes.toml"), biomes.as_bytes(), false)?;
    export_country_adjacency(atlas, &output.join("country-adjacency.csv"))?;
    crate::persist::atomic_write(
        &output.join("water-audit.txt"),
        atlas.water_audit_text().as_bytes(),
        false,
    )?;
    let qualification_sites = qualification_sites(atlas);
    let qualification_toml = toml::to_string_pretty(&qualification_sites).map_err(|error| {
        AtlasError::Corrupt(format!("qualification-site encoding failed: {error}"))
    })?;
    crate::persist::atomic_write(
        &output.join("qualification-sites.toml"),
        qualification_toml.as_bytes(),
        false,
    )?;

    let estimated_loaded_bytes = estimated_loaded_bytes(atlas);
    let report = AtlasExportReport {
        validation: "passed".into(),
        layout: FACE_LAYOUT.into(),
        registered_layers: LAYERS.iter().map(|layer| layer.id.to_string()).collect(),
        exported_maps,
        cell_count: atlas.genesis.geometry.len(),
        stage_elapsed_micros: atlas
            .manifest
            .stages
            .iter()
            .map(|stage| (stage.id.clone(), stage.elapsed_micros))
            .collect(),
        immutable_bytes: atlas.manifest.genesis_bytes,
        dynamic_bytes: atlas.manifest.dynamic_bytes,
        water_cycle_bytes: atlas.manifest.water_cycle_bytes,
        geology_bytes: atlas.manifest.geology_bytes,
        hydrology_bytes: atlas.manifest.hydrology_bytes,
        biome_bytes: atlas.manifest.biome_bytes,
        estimated_loaded_bytes,
        peak_resident_bytes: peak_resident_bytes(),
        export_elapsed_micros: started.elapsed().as_micros().try_into().unwrap_or(u64::MAX),
    };
    let report_toml = toml::to_string_pretty(&report)
        .map_err(|error| AtlasError::Corrupt(format!("report encoding failed: {error}")))?;
    crate::persist::atomic_write(
        &output.join("validation-report.toml"),
        report_toml.as_bytes(),
        false,
    )?;
    Ok(report)
}

fn export_climate_transects(atlas: &PlanetAtlas, path: &Path) -> Result<(), AtlasError> {
    let mut candidates = Vec::new();
    for (pos, terrain) in atlas.genesis.terrain.iter() {
        if terrain.eroded_elevation <= SEA_LEVEL as f32 {
            continue;
        }
        let summit = atlas.climate_downstream(pos, 54.0);
        let lee = atlas.climate_downstream(summit, 54.0);
        let summit_elevation = atlas
            .genesis
            .terrain
            .get(summit)
            .expect("matching atlas grids")
            .eroded_elevation;
        let lee_elevation = atlas
            .genesis
            .terrain
            .get(lee)
            .expect("matching atlas grids")
            .eroded_elevation;
        let score = (summit_elevation - terrain.eroded_elevation).max(0.0)
            + (summit_elevation - lee_elevation).max(0.0);
        if score > 10.0 {
            candidates.push((score, pos));
        }
    }
    candidates.sort_by(|(a_score, a_pos), (b_score, b_pos)| {
        b_score
            .total_cmp(a_score)
            .then_with(|| a_pos.index(atlas.side()).cmp(&b_pos.index(atlas.side())))
    });
    let mut starts = Vec::new();
    for (_, candidate) in candidates {
        if starts.iter().all(|existing: &AtlasPos| {
            geodesic_distance(
                existing.center(atlas.side()),
                candidate.center(atlas.side()),
            ) > 1_000.0
        }) {
            starts.push(candidate);
            if starts.len() == 6 {
                break;
            }
        }
    }
    let mut csv = String::from(
        "transect,step,face,u,v,distance_blocks,elevation,wind_east,wind_north,moisture,precipitation,temperature,aridity\n",
    );
    for (transect, start) in starts.into_iter().enumerate() {
        let mut at = start;
        let mut seen = BTreeSet::new();
        let mut distance = 0.0;
        for step in 0..16 {
            if !seen.insert(at) {
                break;
            }
            let terrain = atlas.genesis.terrain.get(at).expect("matching atlas grids");
            let climate = atlas.genesis.climate.get(at).expect("matching atlas grids");
            csv.push_str(&format!(
                "{transect},{step},{},{},{},{distance:.3},{:.3},{:.6},{:.6},{:.3},{:.3},{:.3},{:.6}\n",
                at.face.name(),
                at.u,
                at.v,
                terrain.eroded_elevation,
                climate.seasonal_wind[1][0],
                climate.seasonal_wind[1][1],
                climate.mean_atmospheric_moisture,
                climate.seasonal_precipitation[1],
                climate.seasonal_temperature[1],
                climate.aridity,
            ));
            let next = atlas.climate_downstream(at, 54.0);
            distance += geodesic_distance(at.center(atlas.side()), next.center(atlas.side()));
            at = next;
        }
    }
    crate::persist::atomic_write(path, csv.as_bytes(), false)?;
    Ok(())
}

fn export_river_profiles(atlas: &PlanetAtlas, path: &Path) -> Result<(), AtlasError> {
    let mut selected: Vec<_> = atlas.hydrology.rivers.iter().collect();
    selected.sort_by(|a, b| {
        b.maximum_discharge
            .total_cmp(&a.maximum_discharge)
            .then_with(|| a.id.cmp(&b.id))
    });
    selected.truncate(16);
    let mut tributaries = vec![0u16; atlas.genesis.hydrology.len()];
    for cell in atlas.genesis.hydrology.values() {
        if cell.drainage_receiver != u32::MAX && cell.flags & HYDRO_RIVER != 0 {
            tributaries[cell.drainage_receiver as usize] =
                tributaries[cell.drainage_receiver as usize].saturating_add(1);
        }
    }
    let mut csv = String::from(
        "river_id,river_name,step,face,u,v,distance_blocks,terrain_elevation,bed_elevation,water_surface,discharge,width,depth,stream_order,tributaries,lake_id,ocean_id,sink_name\n",
    );
    for river in selected {
        let mut distance = 0.0;
        for (step, pos) in river.path.iter().copied().enumerate() {
            if step > 0 {
                distance += geodesic_distance(
                    river.path[step - 1].center(atlas.side()),
                    pos.center(atlas.side()),
                );
            }
            let terrain = atlas
                .genesis
                .terrain
                .get(pos)
                .expect("river profile terrain");
            let hydro = atlas
                .genesis
                .hydrology
                .get(pos)
                .expect("river profile hydrology");
            csv.push_str(&format!(
                "{},{},{step},{},{},{},{distance:.3},{:.3},{:.3},{:.3},{:.5},{:.3},{:.3},{},{},{},{},{}\n",
                river.id,
                river.name,
                pos.face.name(),
                pos.u,
                pos.v,
                terrain.eroded_elevation,
                hydro.channel_bed_elevation,
                hydro.water_surface_elevation,
                hydro.mean_discharge,
                f32::from(hydro.channel_width_centiblocks) / 100.0,
                f32::from(hydro.channel_depth_centiblocks) / 100.0,
                hydro.stream_order,
                tributaries[pos.index(atlas.side())],
                hydro.lake_basin_id,
                hydro.ocean_basin_id,
                river.sink_name,
            ));
        }
    }
    crate::persist::atomic_write(path, csv.as_bytes(), false)?;
    Ok(())
}

fn export_weather_examples(
    atlas: &PlanetAtlas,
    map_dir: &Path,
    tracks_path: &Path,
) -> Result<(), AtlasError> {
    let side = u32::from(atlas.side());
    let width = side * 3;
    let height = side * 2;
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    let mut tracks = String::from(
        "hour,storm_weight,centroid_x,centroid_y,centroid_z,clear_cells,overcast_cells,precipitation_cells,storm_cells,precipitation_units,water_drift\n",
    );
    for hour in 1..=24u64 {
        let day = hour as f64 / 24.0;
        let report = weather.complete_hour(atlas, day, hour - 1, 0.0)?;
        let mut center = DVec3::ZERO;
        let mut weight = 0.0f64;
        let mut weather_counts = [0u64; 4];
        for (pos, geometry) in atlas.genesis.geometry.iter() {
            let storm = f64::from(
                weather
                    .cells
                    .cells
                    .get(pos)
                    .expect("matching weather grids")
                    .storm_energy,
            );
            center += DVec3::from_array(geometry.unit_direction.map(f64::from)) * storm;
            weight += storm;
            let sample = weather_sample(
                atlas.genesis.climate.values()[pos.index(atlas.side())],
                weather.cells.cells.values()[pos.index(atlas.side())],
                day,
                false,
            );
            weather_counts[sample.kind as usize] += 1;
        }
        let center = if weight > 0.0 {
            (center / weight).normalize_or_zero()
        } else {
            DVec3::ZERO
        };
        tracks.push_str(&format!(
            "{hour},{weight:.3},{:.8},{:.8},{:.8},{},{},{},{},{},{}\n",
            center.x,
            center.y,
            center.z,
            weather_counts[LocalWeather::Clear as usize],
            weather_counts[LocalWeather::Overcast as usize],
            weather_counts[LocalWeather::Precipitation as usize],
            weather_counts[LocalWeather::Storm as usize],
            report.precipitation_units,
            report.unexplained_water_drift,
        ));
        if [1, 6, 12, 24].contains(&hour) {
            let mut pixels = vec![0u8; width as usize * height as usize * 3];
            for (pos, _) in atlas.genesis.geometry.iter() {
                let index = pos.index(atlas.side());
                let sample = weather_sample(
                    atlas.genesis.climate.values()[index],
                    weather.cells.cells.values()[index],
                    day,
                    false,
                );
                let color = match sample.kind {
                    LocalWeather::Clear => [50, 105, 175],
                    LocalWeather::Overcast => [125, 132, 142],
                    LocalWeather::Precipitation => [35, 175, 215],
                    LocalWeather::Storm => [130, 45, 170],
                };
                let face_index = pos.face.index() as u32;
                let x = (face_index % 3) * side + u32::from(pos.u);
                let y = (face_index / 3) * side + u32::from(pos.v);
                let target = (y * width + x) as usize * 3;
                pixels[target..target + 3].copy_from_slice(&color);
            }
            write_png(
                &map_dir.join(format!("weather_hour_{hour:02}.png")),
                width,
                height,
                &pixels,
            )?;
        }
    }
    crate::persist::atomic_write(tracks_path, tracks.as_bytes(), false)?;
    Ok(())
}

fn estimated_loaded_bytes(atlas: &PlanetAtlas) -> u64 {
    let count = atlas.genesis.geometry.len() as u64;
    count
        * (std::mem::size_of::<GeometryCell>()
            + std::mem::size_of::<TectonicCell>()
            + std::mem::size_of::<TerrainCell>()
            + std::mem::size_of::<ClimateCell>()
            + std::mem::size_of::<HydrologyCell>()
            + std::mem::size_of::<GroundCell>()
            + std::mem::size_of::<BiomeCell>()
            + std::mem::size_of::<ResourceCell>()
            + std::mem::size_of::<DynamicCell>()
            + std::mem::size_of::<WaterCell>()) as u64
        + atlas.manifest.geology_bytes
        + (atlas.water_cycle.aquifers.capacity() * std::mem::size_of::<SparseAquiferState>()) as u64
        + (atlas.water_cycle.springs.capacity() * std::mem::size_of::<SpringState>()) as u64
        + (atlas.water_cycle.inboxes.capacity() * std::mem::size_of::<FluxInbox>()) as u64
        + (atlas.water_cycle.commitments.capacity() * std::mem::size_of::<ChunkWaterCommitment>())
            as u64
        + (atlas.biomes.countries.capacity() * std::mem::size_of::<CountryRecord>()) as u64
        + atlas
            .biomes
            .countries
            .iter()
            .map(|country| {
                (country.routes.capacity() * std::mem::size_of::<CountryRoute>()) as u64
                    + country
                        .habitat_cells
                        .keys()
                        .map(|name| name.capacity() as u64)
                        .sum::<u64>()
                    + country
                        .biome_cells
                        .keys()
                        .map(|name| name.capacity() as u64)
                        .sum::<u64>()
            })
            .sum::<u64>()
}

fn export_country_adjacency(atlas: &PlanetAtlas, path: &Path) -> Result<(), AtlasError> {
    let mut out = String::from(
        "country_id,neighbor_id,pass_face,pass_u,pass_v,barrier,heart_face,heart_u,heart_v\n",
    );
    for country in &atlas.biomes.countries {
        for route in &country.routes {
            if country.id >= route.neighbor_id {
                continue;
            }
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{},{}\n",
                country.id,
                route.neighbor_id,
                route.pass.face.name(),
                route.pass.u,
                route.pass.v,
                route.barrier,
                country.heart_site.face.name(),
                country.heart_site.u,
                country.heart_site.v,
            ));
        }
    }
    crate::persist::atomic_write(path, out.as_bytes(), false)?;
    Ok(())
}

fn qualification_sites(atlas: &PlanetAtlas) -> QualificationSites {
    let mut result = QualificationSites::default();
    let mut insert = |label: &str, pos: AtlasPos, evidence: String| {
        let center = pos.center(atlas.side());
        let surface_u = center.u.round().clamp(0.0, f64::from(FACE_BLOCKS - 1)) as u16;
        let surface_v = center.v.round().clamp(0.0, f64::from(FACE_BLOCKS - 1)) as u16;
        let unit = crate::planet::surface_to_unit(center);
        let local_noon =
            (0.25 + unit.x.atan2(unit.z) / std::f64::consts::TAU).rem_euclid(1.0) as f32;
        result.sites.insert(
            label.into(),
            QualificationSite {
                face: pos.face.name().into(),
                atlas_u: pos.u,
                atlas_v: pos.v,
                surface_u,
                surface_v,
                spawn: format!("{},{surface_u},{surface_v}", pos.face.name()),
                local_noon,
                evidence,
            },
        );
    };

    let strongest = |detail: DetailedBoundary, land: Option<bool>, maximum: bool| {
        atlas
            .genesis
            .terrain
            .iter()
            .filter(|(pos, terrain)| {
                let tectonics = atlas
                    .genesis
                    .tectonics
                    .get(*pos)
                    .expect("matching atlas grids");
                tectonics.boundary_detail == detail
                    && land.is_none_or(|required| {
                        (terrain.eroded_elevation > SEA_LEVEL as f32) == required
                    })
            })
            .max_by(|(a_pos, a), (b_pos, b)| {
                let a_value = if maximum {
                    a.tectonic_contribution
                } else {
                    -a.tectonic_contribution
                };
                let b_value = if maximum {
                    b.tectonic_contribution
                } else {
                    -b.tectonic_contribution
                };
                a_value
                    .partial_cmp(&b_value)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a_pos.index(atlas.side()).cmp(&b_pos.index(atlas.side())))
            })
            .map(|(pos, terrain)| (pos, *terrain))
    };

    if let Some((pos, terrain)) =
        strongest(DetailedBoundary::ContinentalCollision, Some(true), true)
    {
        insert(
            "continent_collision_range",
            pos,
            format!(
                "continental collision; tectonic relief +{:.2} blocks",
                terrain.tectonic_contribution
            ),
        );
    }
    if let Some((pos, terrain)) = strongest(
        DetailedBoundary::OceanContinentSubduction,
        Some(false),
        false,
    ) {
        insert(
            "subduction_trench",
            pos,
            format!(
                "ocean-continent subduction; tectonic relief {:.2} blocks",
                terrain.tectonic_contribution
            ),
        );
    }
    if let Some(volcano) = atlas
        .geology
        .volcanoes
        .iter()
        .filter(|site| {
            site.source == VolcanoSource::ContinentalArc
                && atlas
                    .genesis
                    .terrain
                    .get(site.pos)
                    .is_some_and(|terrain| terrain.eroded_elevation > SEA_LEVEL as f32 + 2.0)
        })
        .max_by_key(|site| site.edifice_height_blocks)
    {
        insert(
            "coastal_volcanic_arc",
            volcano.pos,
            format!(
                "continental arc volcano {}; edifice {} blocks",
                volcano.id, volcano.edifice_height_blocks
            ),
        );
    }
    if let Some((pos, terrain)) = strongest(DetailedBoundary::ContinentalRift, Some(true), false) {
        insert(
            "continental_rift_valley",
            pos,
            format!(
                "continental rift; tectonic relief {:.2} blocks",
                terrain.tectonic_contribution
            ),
        );
    }
    if let Some(volcano) = atlas
        .geology
        .volcanoes
        .iter()
        .filter(|site| {
            site.source == VolcanoSource::IslandArc
                && atlas
                    .genesis
                    .terrain
                    .get(site.pos)
                    .is_some_and(|terrain| terrain.eroded_elevation > SEA_LEVEL as f32 + 2.0)
        })
        .max_by_key(|site| site.edifice_height_blocks)
    {
        insert(
            "island_arc",
            volcano.pos,
            format!(
                "island arc volcano {}; edifice {} blocks",
                volcano.id, volcano.edifice_height_blocks
            ),
        );
    }
    if let Some(chain_id) = atlas
        .geology
        .volcanoes
        .iter()
        .find(|site| site.source == VolcanoSource::Hotspot)
        .map(|site| site.chain_id)
    {
        let chain: Vec<_> = atlas
            .geology
            .volcanoes
            .iter()
            .filter(|site| site.source == VolcanoSource::Hotspot && site.chain_id == chain_id)
            .collect();
        if let Some(youngest) = chain.iter().min_by_key(|site| site.age_myr) {
            insert(
                "hotspot_chain_youngest",
                youngest.pos,
                format!("hotspot chain {chain_id}; age {} Myr", youngest.age_myr),
            );
        }
        if let Some(oldest) = chain.iter().max_by_key(|site| site.age_myr) {
            insert(
                "hotspot_chain_oldest",
                oldest.pos,
                format!(
                    "hotspot chain {chain_id}; age {} Myr; erosion {}",
                    oldest.age_myr, oldest.erosion
                ),
            );
        }
    }
    if let Some((pos, cell)) = atlas
        .genesis
        .tectonics
        .iter()
        .filter(|(_, cell)| {
            cell.boundary_detail == DetailedBoundary::ContinentalCollision
                && (1..=5).contains(&cell.boundary_distance)
        })
        .max_by_key(|(_, cell)| cell.fault_intensity)
    {
        insert(
            "folded_strata",
            pos,
            format!(
                "collision fold belt; strike {:?}; fault intensity {}",
                cell.boundary_strike, cell.fault_intensity
            ),
        );
    }
    if let Some(intrusion) = atlas
        .geology
        .intrusions
        .iter()
        .min_by_key(|site| site.top_depth_blocks)
    {
        insert(
            "contact_aureole",
            intrusion.pos,
            format!(
                "{:?} intrusion {}; top depth {}; contact radius {} blocks",
                intrusion.kind,
                intrusion.id,
                intrusion.top_depth_blocks,
                intrusion.contact_radius_blocks
            ),
        );
    }

    if let Some((pos, climate)) = atlas
        .genesis
        .climate
        .iter()
        .filter(|(pos, _)| {
            atlas
                .genesis
                .terrain
                .get(*pos)
                .is_some_and(|terrain| terrain.eroded_elevation > SEA_LEVEL as f32 + 2.0)
        })
        .max_by(|(_, a), (_, b)| a.mean_precipitation.total_cmp(&b.mean_precipitation))
    {
        insert(
            "wettest_land",
            pos,
            format!(
                "annual precipitation {:.1}; aridity {:.3}",
                climate.mean_precipitation, climate.aridity
            ),
        );
    }
    if let Some((pos, climate)) = atlas
        .genesis
        .climate
        .iter()
        .filter(|(pos, climate)| {
            climate.mean_temperature > 18.0
                && atlas
                    .genesis
                    .terrain
                    .get(*pos)
                    .is_some_and(|terrain| terrain.eroded_elevation > SEA_LEVEL as f32 + 2.0)
        })
        .max_by(|(_, a), (_, b)| a.aridity.total_cmp(&b.aridity))
    {
        insert(
            "driest_warm_land",
            pos,
            format!(
                "aridity {:.3}; annual precipitation {:.1}; temperature {:.1} C",
                climate.aridity, climate.mean_precipitation, climate.mean_temperature
            ),
        );
    }
    if let Some((windward, leeward, drop)) = atlas
        .genesis
        .terrain
        .iter()
        .filter(|(_, terrain)| terrain.eroded_elevation > SEA_LEVEL as f32)
        .filter_map(|(pos, _)| {
            let leeward = atlas.climate_downstream(pos, 54.0);
            let wet_climate = atlas.genesis.climate.get(pos)?;
            let dry_climate = atlas.genesis.climate.get(leeward)?;
            if wet_climate.mean_temperature < 10.0
                || dry_climate.mean_temperature < 10.0
                || wet_climate.snow_persistence > 0.08
                || dry_climate.snow_persistence > 0.08
            {
                return None;
            }
            let wet = wet_climate.seasonal_precipitation[1];
            let dry = dry_climate.seasonal_precipitation[1];
            (wet > dry).then_some((pos, leeward, wet - dry))
        })
        .max_by(|(_, _, a), (_, _, b)| a.total_cmp(b))
    {
        insert(
            "rain_shadow_windward",
            windward,
            format!("summer windward precipitation drop {drop:.1}"),
        );
        insert(
            "rain_shadow_leeward",
            leeward,
            format!("downwind of rain-shadow pair; precipitation drop {drop:.1}"),
        );
    }

    if let Some((contrast, discharge, river_pos, overlook, river_biome, overlook_biome)) = atlas
        .genesis
        .biomes
        .iter()
        .filter_map(|(river_pos, river_biome)| {
            let river_terrain = atlas.genesis.terrain.get(river_pos).expect("grid");
            let river_climate = atlas.genesis.climate.get(river_pos).expect("grid");
            let river_ground = atlas.genesis.ground.get(river_pos).expect("grid");
            if !matches!(
                river_biome.baseline_biome,
                BIOME_DESERT | BIOME_SCRUBLAND | BIOME_BADLANDS
            ) || river_biome.habitat_flags & HABITAT_RIPARIAN == 0
                || river_biome.edaphic_flags & EDAPHIC_STEEP != 0
                || river_terrain.eroded_elevation > SEA_LEVEL as f32 + 24.0
                || river_climate.mean_temperature < 16.0
                || river_climate.snow_persistence > 0.02
                || river_ground.freeze_flags & FREEZE_SEASONAL != 0
            {
                return None;
            }
            let overlook = river_pos
                .neighbors4(atlas.side())
                .into_iter()
                .filter(|neighbor| {
                    let terrain = atlas.genesis.terrain.get(*neighbor).expect("grid");
                    let biome = atlas.genesis.biomes.get(*neighbor).expect("grid");
                    terrain.eroded_elevation > SEA_LEVEL as f32 + 2.0
                        && terrain.eroded_elevation - river_terrain.eroded_elevation < 12.0
                        && biome.baseline_biome == river_biome.baseline_biome
                        && biome.edaphic_flags & EDAPHIC_STEEP == 0
                        && biome.habitat_flags
                            & (HABITAT_RIPARIAN
                                | HABITAT_WETLAND
                                | HABITAT_OASIS
                                | HABITAT_AQUATIC_FRESH
                                | HABITAT_AQUATIC_BRACKISH
                                | HABITAT_AQUATIC_SALT)
                            == 0
                })
                .min_by_key(|neighbor| {
                    atlas
                        .genesis
                        .biomes
                        .get(*neighbor)
                        .expect("grid")
                        .vegetation_potential
                })?;
            let overlook_biome = atlas.genesis.biomes.get(overlook).expect("grid");
            let contrast = river_biome
                .vegetation_potential
                .saturating_sub(overlook_biome.vegetation_potential);
            let discharge = atlas
                .genesis
                .hydrology
                .get(river_pos)
                .expect("matching grid")
                .mean_discharge;
            Some((
                contrast,
                discharge,
                river_pos,
                overlook,
                river_biome,
                overlook_biome,
            ))
        })
        .max_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| a.1.total_cmp(&b.1))
                .then_with(|| b.2.cmp(&a.2))
        })
    {
        insert(
            "desert_green_river",
            overlook,
            format!(
                "dry overlook vegetation {} toward river at {},{},{}; arid zonal biome {}; riparian vegetation {}; contrast {}; discharge {:.1}",
                overlook_biome.vegetation_potential,
                river_pos.face.name(),
                river_pos.u,
                river_pos.v,
                river_biome.baseline_biome,
                river_biome.vegetation_potential,
                contrast,
                discharge
            ),
        );
    }
    if let Some((_, _, _, contrast, spring_pos, overlook, spring_biome, overlook_biome)) = atlas
        .genesis
        .biomes
        .iter()
        .filter_map(|(spring_pos, spring_biome)| {
            let spring_terrain = atlas.genesis.terrain.get(spring_pos).expect("grid");
            let spring_climate = atlas.genesis.climate.get(spring_pos).expect("grid");
            let spring_ground = atlas.genesis.ground.get(spring_pos).expect("grid");
            if spring_biome.habitat_flags & (HABITAT_OASIS | HABITAT_SPRING)
                != (HABITAT_OASIS | HABITAT_SPRING)
                || spring_climate.mean_temperature < 16.0
                || spring_climate.snow_persistence > 0.02
                || spring_ground.freeze_flags & FREEZE_SEASONAL != 0
            {
                return None;
            }
            let overlook = spring_pos
                .neighbors4(atlas.side())
                .into_iter()
                .filter(|neighbor| {
                    let terrain = atlas.genesis.terrain.get(*neighbor).expect("grid");
                    let biome = atlas.genesis.biomes.get(*neighbor).expect("grid");
                    terrain.eroded_elevation > SEA_LEVEL as f32 + 2.0
                        && matches!(
                            biome.baseline_biome,
                            BIOME_DESERT | BIOME_SCRUBLAND | BIOME_BADLANDS
                        )
                        && biome.habitat_flags
                            & (HABITAT_OASIS
                                | HABITAT_SPRING
                                | HABITAT_RIPARIAN
                                | HABITAT_WETLAND
                                | HABITAT_AQUATIC_FRESH)
                            == 0
                })
                .min_by_key(|neighbor| {
                    atlas
                        .genesis
                        .biomes
                        .get(*neighbor)
                        .expect("grid")
                        .vegetation_potential
                })?;
            let overlook_biome = atlas.genesis.biomes.get(overlook).expect("grid");
            let overlook_terrain = atlas.genesis.terrain.get(overlook).expect("grid");
            let contrast = spring_biome
                .vegetation_potential
                .saturating_sub(overlook_biome.vegetation_potential);
            Some((
                u8::from(spring_biome.habitat_flags & HABITAT_RIPARIAN == 0),
                u8::from(
                    spring_terrain.eroded_elevation <= SEA_LEVEL as f32 + 32.0
                        && overlook_terrain.eroded_elevation <= SEA_LEVEL as f32 + 32.0,
                ),
                u8::from(
                    spring_biome.edaphic_flags & EDAPHIC_STEEP == 0
                        && overlook_biome.edaphic_flags & EDAPHIC_STEEP == 0,
                ),
                contrast,
                spring_pos,
                overlook,
                spring_biome,
                overlook_biome,
            ))
        })
        .max_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| a.2.cmp(&b.2))
                .then_with(|| a.3.cmp(&b.3))
                .then_with(|| b.4.cmp(&a.4))
        })
    {
        let ground = atlas.genesis.ground.get(spring_pos).expect("matching grid");
        insert(
            "natural_oasis_spring",
            overlook,
            format!(
                "dry overlook vegetation {} toward spring at {},{},{}; fresh shallow groundwater head {:.1}; oasis vegetation {}; contrast {}; permeability {}",
                overlook_biome.vegetation_potential,
                spring_pos.face.name(),
                spring_pos.u,
                spring_pos.v,
                ground.baseline_groundwater_head,
                spring_biome.vegetation_potential,
                contrast,
                ground.aquifer_permeability
            ),
        );
    }
    if let Some((coast_pos, coast_biome)) = atlas
        .genesis
        .biomes
        .iter()
        .filter(|(pos, biome)| {
            let climate = atlas.genesis.climate.get(*pos).expect("grid");
            matches!(
                biome.baseline_biome,
                BIOME_FOREST | BIOME_PLAINS | BIOME_TAIGA
            ) && biome.habitat_flags & HABITAT_BEACH_DUNE != 0
                && climate.mean_temperature >= 8.0
                && climate.snow_persistence < 0.08
        })
        .max_by_key(|(_, biome)| biome.vegetation_potential)
    {
        insert(
            "temperate_coast",
            coast_pos,
            format!(
                "temperate biome {}; coastal salinity {}; vegetation {}",
                coast_biome.baseline_biome,
                atlas
                    .genesis
                    .ground
                    .get(coast_pos)
                    .expect("grid")
                    .soil_salinity,
                coast_biome.vegetation_potential
            ),
        );
        if let Some((interior_pos, climate)) = atlas
            .genesis
            .climate
            .iter()
            .filter(|(pos, _)| {
                let biome = atlas.genesis.biomes.get(*pos).expect("grid");
                let climate = atlas.genesis.climate.get(*pos).expect("grid");
                biome.baseline_biome == coast_biome.baseline_biome
                    && biome.habitat_flags & HABITAT_BEACH_DUNE == 0
                    && climate.snow_persistence < 0.08
            })
            .max_by(|(_, a), (_, b)| a.continentality.total_cmp(&b.continentality))
        {
            insert(
                "temperate_continental_interior",
                interior_pos,
                format!(
                    "same zonal biome as temperate_coast; continentality {:.3}; annual range {:.1}",
                    climate.continentality, climate.seasonality
                ),
            );
        }
    }
    if let Some((forest_pos, alpine_pos)) = atlas.genesis.biomes.iter().find_map(|(pos, biome)| {
        matches!(biome.baseline_biome, BIOME_FOREST | BIOME_TAIGA)
            .then(|| {
                pos.neighbors4(atlas.side()).into_iter().find(|neighbor| {
                    atlas.genesis.biomes.get(*neighbor).is_some_and(|other| {
                        other.baseline_biome == BIOME_MOUNTAINS
                            || other.habitat_flags & HABITAT_ALPINE != 0
                    })
                })
            })
            .flatten()
            .map(|alpine| (pos, alpine))
    }) {
        insert(
            "treeline_forest",
            forest_pos,
            format!(
                "forest/taiga below local tree line {}",
                atlas
                    .genesis
                    .biomes
                    .get(forest_pos)
                    .expect("grid")
                    .tree_line_y
            ),
        );
        insert(
            "treeline_alpine",
            alpine_pos,
            format!(
                "adjacent alpine cell; elevation {:.1}; tree line {}",
                atlas
                    .genesis
                    .terrain
                    .get(alpine_pos)
                    .expect("grid")
                    .eroded_elevation,
                atlas
                    .genesis
                    .biomes
                    .get(alpine_pos)
                    .expect("grid")
                    .tree_line_y
            ),
        );
    }
    if let Some(country) = atlas.biomes.countries.iter().max_by_key(|country| {
        let climate = atlas
            .genesis
            .climate
            .get(country.heart_site)
            .expect("country heart is inside atlas");
        (
            u8::from(climate.mean_temperature >= 10.0 && climate.snow_persistence < 0.08),
            country.habitat_cells.len(),
            country.biome_cells.len(),
            country.cell_count,
        )
    }) {
        insert(
            "multi_habitat_country",
            country.heart_site,
            format!(
                "country {}; {} habitat overlays and {} zonal biomes across {} cells",
                country.id,
                country.habitat_cells.len(),
                country.biome_cells.len(),
                country.cell_count
            ),
        );
    }

    if let Some(river) = atlas.hydrology.rivers.iter().max_by(|a, b| {
        a.maximum_discharge
            .total_cmp(&b.maximum_discharge)
            .then_with(|| b.id.cmp(&a.id))
    }) {
        insert(
            "major_river_source",
            river.source,
            format!(
                "{} source; {:.0} blocks long; order {}",
                river.name, river.length_blocks, river.stream_order
            ),
        );
        insert(
            "major_river_mouth",
            river.mouth,
            format!(
                "{} mouth into {}; discharge {:.2}; width {:.1}",
                river.name, river.sink_name, river.maximum_discharge, river.maximum_width_blocks
            ),
        );
    }
    if let Some(river) = atlas.hydrology.rivers.iter().find(|river| {
        river
            .path
            .windows(2)
            .any(|edge| edge[0].face != edge[1].face)
    }) && let Some(crossing) = river
        .path
        .windows(2)
        .find(|edge| edge[0].face != edge[1].face)
    {
        insert(
            "river_face_seam",
            crossing[0],
            format!(
                "{} crosses from {} to {}",
                river.name, crossing[0].face, crossing[1].face
            ),
        );
    }
    if let Some(lake) = atlas
        .hydrology
        .lakes
        .iter()
        .find(|lake| lake.outlet.is_some())
    {
        insert(
            "through_flow_lake",
            lake.sink,
            format!(
                "{}; {:?}; surface {:.2}; spill {:.2}; outlet {}",
                lake.name,
                lake.class,
                lake.surface_elevation,
                lake.spill_elevation,
                lake.outlet.expect("filtered outlet").face
            ),
        );
    }
    if let Some(lake) = atlas
        .hydrology
        .lakes
        .iter()
        .filter(|lake| lake.outlet.is_none())
        .max_by_key(|lake| lake.salinity)
    {
        insert(
            "terminal_salt_lake",
            lake.sink,
            format!(
                "{}; {:?}; salinity {}; seasonal range {:.2}",
                lake.name, lake.class, lake.salinity, lake.seasonal_level_range
            ),
        );
    }
    if let Some((pos, cell)) = atlas
        .genesis
        .hydrology
        .iter()
        .filter(|(pos, cell)| {
            let climate = atlas.genesis.climate.get(*pos).expect("grid");
            cell.flags & (HYDRO_DELTA | HYDRO_ESTUARY) != 0
                && climate.mean_temperature >= 10.0
                && climate.snow_persistence < 0.08
        })
        .max_by(|(_, a), (_, b)| a.mean_discharge.total_cmp(&b.mean_discharge))
    {
        insert(
            "delta_or_estuary",
            pos,
            format!(
                "{:?}; discharge {:.2}; salinity {}",
                cell.water_body, cell.mean_discharge, cell.salinity
            ),
        );
    }

    // One accepted local-weather hour supplies two positions from the same
    // authoritative state for in-world simultaneous-weather captures.
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    if weather.complete_hour(atlas, 0.0, 0, 0.0).is_ok() {
        let local_relief = |pos: AtlasPos, elevation: f32| {
            Direction4::ALL
                .into_iter()
                .filter_map(|direction| {
                    atlas
                        .genesis
                        .terrain
                        .get(pos.step(direction, atlas.side()).pos)
                })
                .map(|neighbor| (neighbor.eroded_elevation - elevation).abs())
                .fold(0.0f32, f32::max)
        };
        // Qualification sites are also capture sites. Prefer a broad, gentle
        // shelf where falling weather is readable; retain a wet-land fallback
        // so diagnostics still prove locality on unusually rugged seeds.
        let preferred_rain = atlas
            .genesis
            .terrain
            .iter()
            .filter_map(|(pos, terrain)| {
                let cell = *weather.cells.cells.get(pos)?;
                let relief = local_relief(pos, terrain.eroded_elevation);
                (terrain.eroded_elevation > SEA_LEVEL as f32 + 4.0
                    && terrain.eroded_elevation < SEA_LEVEL as f32 + 48.0
                    && relief <= 6.0
                    && cell.precipitation_rate > 0)
                    .then_some((pos, cell, relief))
            })
            .max_by_key(|(_, cell, _)| cell.precipitation_rate);
        let rain = preferred_rain.or_else(|| {
            atlas
                .genesis
                .terrain
                .iter()
                .filter(|(_, terrain)| terrain.eroded_elevation > SEA_LEVEL as f32 + 2.0)
                .filter_map(|(pos, terrain)| {
                    Some((
                        pos,
                        *weather.cells.cells.get(pos)?,
                        local_relief(pos, terrain.eroded_elevation),
                    ))
                })
                .filter(|(_, cell, _)| cell.precipitation_rate > 0)
                .max_by_key(|(_, cell, _)| cell.precipitation_rate)
        });
        if let Some((rain_pos, rain_cell, relief)) = rain {
            insert(
                "weather_raining_hour_1",
                rain_pos,
                format!(
                    "same hour as weather_clear_hour_1; precipitation units {}; storm energy {}; local relief {:.1} blocks",
                    rain_cell.precipitation_rate, rain_cell.storm_energy, relief
                ),
            );
            if let Some((clear_pos, clear_cell)) = atlas
                .genesis
                .terrain
                .iter()
                .filter(|(pos, terrain)| {
                    terrain.eroded_elevation > SEA_LEVEL as f32 + 2.0
                        && geodesic_distance(
                            pos.center(atlas.side()),
                            rain_pos.center(atlas.side()),
                        ) > 3_000.0
                })
                .filter_map(|(pos, _)| {
                    let cell = weather.cells.cells.get(pos)?;
                    (cell.precipitation_rate == 0).then_some((pos, cell))
                })
                .min_by_key(|(_, cell)| cell.cloud_water)
            {
                insert(
                    "weather_clear_hour_1",
                    clear_pos,
                    format!(
                        "same hour as weather_raining_hour_1; cloud water {}; storm energy {}",
                        clear_cell.cloud_water, clear_cell.storm_energy
                    ),
                );
            }
        }
    }
    result
}

fn peak_resident_bytes() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let kib = status.lines().find_map(|line| {
        let value = line.strip_prefix("VmHWM:")?;
        value.split_whitespace().next()?.parse::<u64>().ok()
    })?;
    Some(kib * 1024)
}

fn write_census_csv(census: &AtlasCensus, path: &Path) -> Result<(), AtlasError> {
    let mut out = String::from("metric,key,value\n");
    out.push_str(&format!("planet,cells,{}\n", census.cells));
    out.push_str(&format!(
        "planet,physical_area,{:.3}\n",
        census.physical_area
    ));
    out.push_str(&format!(
        "planet,land_fraction,{:.8}\n",
        census.land_fraction
    ));
    out.push_str(&format!(
        "planet,ocean_fraction,{:.8}\n",
        census.ocean_fraction
    ));
    out.push_str(&format!("geology,plates,{}\n", census.plate_count));
    out.push_str(&format!("geology,cratons,{}\n", census.craton_count));
    out.push_str(&format!(
        "geology,major_continents,{}\n",
        census.major_continent_count
    ));
    out.push_str(&format!(
        "geology,largest_ocean_share,{:.8}\n",
        census.largest_ocean_share
    ));
    for (face, cells) in &census.cells_by_face {
        out.push_str(&format!("face,{face},{cells}\n"));
    }
    for (biome, area) in &census.biome_areas {
        out.push_str(&format!("biome,{biome},{area:.3}\n"));
    }
    for (zone, area) in &census.climate_zone_areas {
        out.push_str(&format!("climate,{zone},{area:.3}\n"));
    }
    out.push_str(&format!(
        "climate,area_mean_temperature_c,{:.4}\nclimate,minimum_temperature_c,{:.4}\nclimate,maximum_temperature_c,{:.4}\nclimate,area_mean_precipitation_mm,{:.4}\nclimate,maximum_precipitation_mm,{:.4}\nclimate,area_mean_aridity,{:.6}\nclimate,maximum_aridity,{:.6}\n",
        census.area_mean_temperature_c,
        census.minimum_temperature_c,
        census.maximum_temperature_c,
        census.area_mean_precipitation_mm,
        census.maximum_precipitation_mm,
        census.area_mean_aridity,
        census.maximum_aridity,
    ));
    out.push_str(&format!(
        "hydrology,ocean_basins,{}\nhydrology,named_rivers,{}\nhydrology,lakes,{}\nhydrology,watersheds,{}\nhydrology,maximum_discharge,{:.5}\nhydrology,maximum_channel_width_blocks,{:.3}\nhydrology,maximum_stream_order,{}\nhydrology,floodplain_cells,{}\nhydrology,wetland_cells,{}\nhydrology,delta_cells,{}\nhydrology,estuary_cells,{}\nhydrology,baseline_surface_water_units,{}\nhydrology,voxel_volume_residual,{}\n",
        census.ocean_basin_count,
        census.named_river_count,
        census.lake_count,
        census.watershed_count,
        census.maximum_discharge,
        census.maximum_channel_width_blocks,
        census.maximum_stream_order,
        census.floodplain_cells,
        census.wetland_cells,
        census.delta_cells,
        census.estuary_cells,
        census.baseline_surface_water_units,
        census.voxel_volume_residual,
    ));
    for (class, count) in &census.lake_classes {
        out.push_str(&format!("lake_class,{class},{count}\n"));
    }
    for (mineral, count) in &census.deposits_by_mineral {
        out.push_str(&format!("deposit_sites,{mineral},{count}\n"));
    }
    for (mineral, tonnage) in &census.deposit_tonnage_by_mineral {
        out.push_str(&format!("deposit_tonnage,{mineral},{tonnage}\n"));
    }
    for (mineral, distance) in &census.max_nearest_deposit_distance_blocks {
        out.push_str(&format!(
            "deposit_max_distance_blocks,{mineral},{distance:.3}\n"
        ));
    }
    crate::persist::atomic_write(path, out.as_bytes(), false)?;
    Ok(())
}

fn export_layer(atlas: &PlanetAtlas, spec: LayerSpec, output: &Path) -> Result<(), AtlasError> {
    let side = u32::from(atlas.side());
    let width = side * 3;
    let height = side * 2;
    let values: Vec<f64> = atlas
        .genesis
        .geometry
        .iter()
        .map(|(pos, _)| layer_value(atlas, spec.id, pos))
        .collect();
    let (minimum, maximum) = values.iter().fold(
        (f64::INFINITY, f64::NEG_INFINITY),
        |(minimum, maximum), value| (minimum.min(*value), maximum.max(*value)),
    );
    let mut pixels = vec![0u8; width as usize * height as usize * 3];
    for (pos, _) in atlas.genesis.geometry.iter() {
        let face_index = pos.face.index() as u32;
        let origin_x = (face_index % 3) * side;
        let origin_y = (face_index / 3) * side;
        let target =
            ((origin_y + u32::from(pos.v)) * width + origin_x + u32::from(pos.u)) as usize * 3;
        let value = values[pos.index(atlas.side())];
        let color = match spec.kind {
            LayerKind::Scalar => scalar_color(value, minimum, maximum),
            LayerKind::Categorical => categorical_color(value as u64),
            LayerKind::Direction => direction_color(value),
        };
        pixels[target..target + 3].copy_from_slice(&color);
    }
    write_png(
        &output.join(format!("{}.png", spec.id)),
        width,
        height,
        &pixels,
    )?;
    let legend = format!(
        "layer = {}\ndescription = {}\nlayout = {}\nkind = {}\nminimum = {:.9}\nmaximum = {:.9}\ncolor = scalar: navy-cyan-yellow-red; categorical: stable id hash (zero black); direction: cyclic hue\n",
        spec.id,
        spec.description,
        FACE_LAYOUT,
        match spec.kind {
            LayerKind::Scalar => "scalar",
            LayerKind::Categorical => "categorical",
            LayerKind::Direction => "direction",
        },
        minimum,
        maximum
    );
    crate::persist::atomic_write(
        &output.join(format!("{}.legend.txt", spec.id)),
        legend.as_bytes(),
        false,
    )?;
    Ok(())
}

fn layer_value(atlas: &PlanetAtlas, id: &str, pos: AtlasPos) -> f64 {
    let geometry = atlas
        .genesis
        .geometry
        .get(pos)
        .expect("registered geometry");
    let tectonics = atlas
        .genesis
        .tectonics
        .get(pos)
        .expect("registered tectonics");
    let terrain = atlas.genesis.terrain.get(pos).expect("registered terrain");
    let climate = atlas.genesis.climate.get(pos).expect("registered climate");
    let hydrology = atlas
        .genesis
        .hydrology
        .get(pos)
        .expect("registered hydrology");
    let ground = atlas.genesis.ground.get(pos).expect("registered ground");
    let biome = atlas.genesis.biomes.get(pos).expect("registered biome");
    let resource = atlas
        .genesis
        .resources
        .get(pos)
        .expect("registered resource");
    let dynamic = atlas.dynamic.cells.get(pos).expect("registered dynamic");
    let water = atlas.water_cycle.cells.get(pos).expect("registered water");
    match id {
        "latitude" => f64::from(geometry.latitude_radians),
        "physical_area" => f64::from(geometry.physical_area),
        "plate_id" => f64::from(tectonics.plate_id),
        "boundary" => tectonics.boundary as u8 as f64,
        "boundary_type" => tectonics.boundary_detail as u8 as f64,
        "boundary_strength" => f64::from(tectonics.boundary_strength),
        "boundary_distance" => f64::from(tectonics.boundary_distance),
        "boundary_strike" => {
            let global = DVec3::new(
                f64::from(tectonics.boundary_strike[0]),
                f64::from(tectonics.boundary_strike[1]),
                f64::from(tectonics.boundary_strike[2]),
            )
            .normalize_or_zero();
            let frame = crate::planet::local_frame(pos.center(atlas.side()));
            frame.north.dot(global).atan2(frame.east.dot(global))
        }
        "plate_velocity" => {
            let plate = &atlas.geology.plates[usize::from(tectonics.plate_id)];
            let pole = DVec3::new(
                f64::from(plate.euler_pole[0]),
                f64::from(plate.euler_pole[1]),
                f64::from(plate.euler_pole[2]),
            );
            let point = DVec3::new(
                f64::from(geometry.unit_direction[0]),
                f64::from(geometry.unit_direction[1]),
                f64::from(geometry.unit_direction[2]),
            );
            let velocity = pole.cross(point).normalize_or_zero();
            let frame = crate::planet::local_frame(pos.center(atlas.side()));
            frame.north.dot(velocity).atan2(frame.east.dot(velocity))
        }
        "euler_poles" => {
            let point = DVec3::new(
                f64::from(geometry.unit_direction[0]),
                f64::from(geometry.unit_direction[1]),
                f64::from(geometry.unit_direction[2]),
            );
            let angular_radius = f64::from(atlas.cell_blocks()) * 1.5 / PLANET_RADIUS;
            atlas
                .geology
                .plates
                .iter()
                .filter_map(|plate| {
                    let pole = DVec3::new(
                        f64::from(plate.euler_pole[0]),
                        f64::from(plate.euler_pole[1]),
                        f64::from(plate.euler_pole[2]),
                    );
                    let dot = pole.dot(point);
                    (dot >= angular_radius.cos()).then_some((dot, plate.id))
                })
                .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
                .map_or(0.0, |(_, id)| f64::from(id) + 1.0)
        }
        "continental_crust" => f64::from(tectonics.continental_crust),
        "crust_age" => f64::from(tectonics.crust_age),
        "oceanic_age" => f64::from(tectonics.oceanic_age),
        "crust_thickness" => f64::from(tectonics.crust_thickness),
        "craton_id" => f64::from(tectonics.craton_id),
        "bedrock_family" => f64::from(tectonics.bedrock_family),
        "geological_province" => f64::from(tectonics.geological_province),
        "base_elevation" => f64::from(terrain.base_elevation),
        "eroded_elevation" => f64::from(terrain.eroded_elevation),
        "tectonic_contribution" => f64::from(terrain.tectonic_contribution),
        "volcanic_contribution" => f64::from(terrain.volcanic_contribution),
        "dynamic_topography" => f64::from(terrain.dynamic_topography),
        "landmass_id" => f64::from(terrain.landmass_id),
        "stratigraphic_stack" => f64::from(tectonics.stratigraphic_stack),
        "metamorphic_grade" => f64::from(tectonics.metamorphic_grade),
        "fault_intensity" => f64::from(tectonics.fault_intensity),
        "sediment_basin" => tectonics.sediment_basin as u8 as f64,
        "volcanic_history" => f64::from(tectonics.volcanic_history),
        "ocean_depth" => f64::from((SEA_LEVEL as f32 - terrain.eroded_elevation).max(0.0)),
        "mean_temperature" => f64::from(climate.mean_temperature),
        "seasonality" => f64::from(climate.seasonality),
        "ocean_temperature_anomaly" => f64::from(climate.ocean_temperature_anomaly),
        "continentality" => f64::from(climate.continentality),
        "mean_atmospheric_moisture" => f64::from(climate.mean_atmospheric_moisture),
        "mean_precipitation" => f64::from(climate.mean_precipitation),
        "precipitation_seasonality" => f64::from(climate.precipitation_seasonality),
        "potential_evapotranspiration" => f64::from(climate.potential_evapotranspiration),
        "aridity" => f64::from(climate.aridity),
        "snow_persistence" => f64::from(climate.snow_persistence),
        "prevailing_wind" => {
            f64::from(climate.prevailing_wind[1].atan2(climate.prevailing_wind[0]))
        }
        "ocean_current" => f64::from(climate.ocean_current[1].atan2(climate.ocean_current[0])),
        "spring_temperature" => f64::from(climate.seasonal_temperature[0]),
        "summer_temperature" => f64::from(climate.seasonal_temperature[1]),
        "autumn_temperature" => f64::from(climate.seasonal_temperature[2]),
        "winter_temperature" => f64::from(climate.seasonal_temperature[3]),
        "spring_precipitation" => f64::from(climate.seasonal_precipitation[0]),
        "summer_precipitation" => f64::from(climate.seasonal_precipitation[1]),
        "autumn_precipitation" => f64::from(climate.seasonal_precipitation[2]),
        "winter_precipitation" => f64::from(climate.seasonal_precipitation[3]),
        "spring_wind" => f64::from(climate.seasonal_wind[0][1].atan2(climate.seasonal_wind[0][0])),
        "summer_wind" => f64::from(climate.seasonal_wind[1][1].atan2(climate.seasonal_wind[1][0])),
        "autumn_wind" => f64::from(climate.seasonal_wind[2][1].atan2(climate.seasonal_wind[2][0])),
        "winter_wind" => f64::from(climate.seasonal_wind[3][1].atan2(climate.seasonal_wind[3][0])),
        "drainage_receiver" => f64::from(hydrology.drainage_receiver),
        "watershed_id" => f64::from(hydrology.watershed_id),
        "ocean_basin_id" => f64::from(hydrology.ocean_basin_id),
        "lake_basin_id" => f64::from(hydrology.lake_basin_id),
        "spill_elevation" => f64::from(hydrology.spill_elevation),
        "depression_depth" => {
            f64::from((hydrology.filled_elevation - terrain.eroded_elevation).max(0.0))
        }
        "mean_runoff" => f64::from(hydrology.mean_runoff),
        "mean_discharge" => f64::from(hydrology.mean_discharge),
        "catchment_area" => f64::from(hydrology.catchment_area),
        "river_id" => f64::from(hydrology.river_id),
        "stream_order" => f64::from(hydrology.stream_order),
        "channel_width" => f64::from(hydrology.channel_width_centiblocks) / 100.0,
        "channel_depth" => f64::from(hydrology.channel_depth_centiblocks) / 100.0,
        "channel_bed_elevation" => f64::from(hydrology.channel_bed_elevation),
        "water_surface_elevation" => f64::from(hydrology.water_surface_elevation),
        "water_body" => hydrology.water_body as u8 as f64,
        "salinity" => f64::from(hydrology.salinity),
        "sediment_energy" => f64::from(hydrology.sediment_energy),
        "erosion" => f64::from(hydrology.erosion_centiblocks) / 100.0,
        "deposition" => f64::from(hydrology.deposition_centiblocks) / 100.0,
        "seasonal_water_range" => f64::from(hydrology.seasonal_level_range_centiblocks) / 100.0,
        "baseline_water_volume" => hydrology.baseline_water_units as f64,
        "voxel_volume_residual" => f64::from(hydrology.voxel_volume_residual),
        "floodplain" => f64::from(u8::from(hydrology.flags & HYDRO_FLOODPLAIN != 0)),
        "wetland" => f64::from(u8::from(hydrology.flags & HYDRO_WETLAND != 0)),
        "delta" => f64::from(u8::from(hydrology.flags & HYDRO_DELTA != 0)),
        "estuary" => f64::from(u8::from(hydrology.flags & HYDRO_ESTUARY != 0)),
        "soil_parent_material" => f64::from(ground.soil_parent_material),
        "aquifer_capacity" => f64::from(ground.aquifer_capacity),
        "aquifer_permeability" => f64::from(ground.aquifer_permeability),
        "porosity" => f64::from(ground.porosity),
        "groundwater_head" => f64::from(ground.baseline_groundwater_head),
        "soil_depth" => f64::from(ground.soil_depth_decimeters),
        "soil_sand" => f64::from(ground.sand),
        "soil_silt" => f64::from(ground.silt),
        "soil_clay" => f64::from(
            255u8
                .saturating_sub(ground.sand)
                .saturating_sub(ground.silt),
        ),
        "soil_organic" => f64::from(ground.organic),
        "soil_fertility" => f64::from(ground.baseline_fertility),
        "soil_drainage" => f64::from(ground.drainage),
        "soil_salinity" => f64::from(ground.soil_salinity),
        "soil_freeze" => f64::from(ground.freeze_flags),
        "soil_erosion_susceptibility" => f64::from(ground.erosion_susceptibility),
        "baseline_biome" => f64::from(biome.baseline_biome),
        "habitat_flags" => f64::from(biome.habitat_flags),
        "edaphic_flags" => f64::from(biome.edaphic_flags),
        "vegetation_potential" => f64::from(biome.vegetation_potential),
        "tree_line" => f64::from(biome.tree_line_y),
        "succession_potential" => f64::from(biome.succession_potential),
        "province_id" | "country_id" => f64::from(biome.country_id),
        "heart_assignment" => f64::from(biome.heart_assignment),
        "country_boundaries" => {
            if biome.country_id == 0 {
                0.0
            } else {
                let boundary = pos.neighbors4(atlas.side()).into_iter().any(|neighbor| {
                    atlas
                        .genesis
                        .biomes
                        .get(neighbor)
                        .is_some_and(|cell| cell.country_id != biome.country_id)
                });
                if boundary {
                    f64::from(biome.country_id)
                } else {
                    0.0
                }
            }
        }
        "hearts_edifices" => atlas.country(biome.country_id).map_or(0.0, |country| {
            if country.heart_site == pos {
                f64::from(country.heart_form) + 1.0
            } else {
                0.0
            }
        }),
        "graft_compatibility_jungle" => {
            let center = pos.center(atlas.side());
            let surface = SurfacePos::new(
                center.face,
                center.u.floor().clamp(0.0, f64::from(FACE_BLOCKS - 1)) as u16,
                center.v.floor().clamp(0.0, f64::from(FACE_BLOCKS - 1)) as u16,
            )
            .expect("atlas cell center lies on its face");
            match atlas.graft_compatibility_at(surface, BIOME_JUNGLE) {
                GraftCompatibility::Compatible => 1.0,
                GraftCompatibility::Marginal => 2.0,
                GraftCompatibility::Incompatible => 3.0,
            }
        }
        "animal_frog_suitability" => f64::from(u8::from(
            (-2.0..=34.0).contains(&climate.mean_temperature)
                && biome.habitat_flags
                    & (HABITAT_WETLAND
                        | HABITAT_RIPARIAN
                        | HABITAT_FLOODPLAIN
                        | HABITAT_LAKESHORE
                        | HABITAT_AQUATIC_FRESH)
                    != 0
                && biome.habitat_flags
                    & (HABITAT_AQUATIC_BRACKISH | HABITAT_AQUATIC_SALT | HABITAT_SALT_MARSH)
                    == 0,
        )),
        "animal_seal_suitability" => f64::from(u8::from(
            climate.mean_temperature <= 8.0
                && biome.habitat_flags
                    & (HABITAT_AQUATIC_SALT | HABITAT_BEACH_DUNE | HABITAT_SALT_MARSH)
                    != 0,
        )),
        "deposit_site" => f64::from(resource.deposit_site_ref),
        "deposit_site_count" => f64::from(resource.deposit_site_count),
        "geological_sites" => {
            if resource.deposit_site_ref != 0 {
                30_000.0 + f64::from(resource.deposit_site_ref)
            } else if let Some(site) = atlas.geology.volcanoes.iter().find(|site| site.pos == pos) {
                10_000.0 + f64::from(site.id)
            } else if let Some(site) = atlas.geology.intrusions.iter().find(|site| site.pos == pos)
            {
                20_000.0 + f64::from(site.id)
            } else {
                0.0
            }
        }
        "atmospheric_vapor" => f64::from(dynamic.atmospheric_vapor),
        "cloud_water" => f64::from(dynamic.cloud_water),
        "soil_moisture" => water.soil.water_hu as f64,
        "snowpack" => water.snow.water_hu as f64,
        "groundwater_volume" => water.groundwater.water_hu as f64,
        "groundwater_head_anomaly" => f64::from(water.groundwater_head_milliblocks),
        "surface_runoff" => water.runoff.water_hu as f64,
        "groundwater_recharge" => f64::from(water.last_recharge_hu),
        "spring_discharge" => f64::from(water.last_spring_hu),
        "water_salinity" => f64::from(if water.runoff.water_hu != 0 {
            water.runoff.salinity()
        } else {
            water.groundwater.salinity()
        }),
        "lake_storage_anomaly" => hydrology
            .lake_basin_id
            .checked_sub(1)
            .and_then(|_| {
                atlas.water_cycle.reservoirs.iter().find(|reservoir| {
                    reservoir.id
                        == surface_reservoir_id(SurfaceReservoirKind::Lake, hydrology.lake_basin_id)
                })
            })
            .map_or(0.0, |reservoir| {
                reservoir.coarse.water_hu as f64 - reservoir.initial_total_hu as f64
            }),
        "ocean_storage_anomaly" => atlas
            .water_cycle
            .reservoirs
            .iter()
            .find(|reservoir| {
                reservoir.id
                    == surface_reservoir_id(
                        SurfaceReservoirKind::Ocean,
                        u32::from(hydrology.ocean_basin_id),
                    )
            })
            .map_or(0.0, |reservoir| {
                reservoir.coarse.water_hu as f64 - reservoir.initial_total_hu as f64
            }),
        "local_weather_anomaly" => f64::from(dynamic.local_weather_anomaly),
        "weather_temperature_anomaly" => f64::from(dynamic.weather_temperature_anomaly),
        "pressure_anomaly" => f64::from(dynamic.pressure_anomaly),
        "storm_energy" => f64::from(dynamic.storm_energy),
        "precipitation_rate" => f64::from(dynamic.precipitation_rate),
        "weather_wind_anomaly" => {
            f64::from(f32::from(dynamic.wind_anomaly[1]).atan2(f32::from(dynamic.wind_anomaly[0])))
        }
        "fire_moisture_anomaly" => f64::from(dynamic.fire_moisture_anomaly),
        "vegetation_moisture_anomaly" => f64::from(dynamic.vegetation_moisture_anomaly),
        _ => 0.0,
    }
}

fn scalar_color(value: f64, minimum: f64, maximum: f64) -> [u8; 3] {
    let t = if maximum > minimum {
        ((value - minimum) / (maximum - minimum)).clamp(0.0, 1.0)
    } else {
        0.5
    };
    if t < 1.0 / 3.0 {
        lerp_color([10, 22, 72], [20, 190, 210], t * 3.0)
    } else if t < 2.0 / 3.0 {
        lerp_color([20, 190, 210], [245, 220, 70], (t - 1.0 / 3.0) * 3.0)
    } else {
        lerp_color([245, 220, 70], [180, 25, 35], (t - 2.0 / 3.0) * 3.0)
    }
}

fn categorical_color(value: u64) -> [u8; 3] {
    if value == 0 || value == u64::from(u32::MAX) {
        return [5, 7, 12];
    }
    let mixed = mix64(value ^ 0x4154_4c41_534d_4150);
    [
        48 + (mixed & 0x9f) as u8,
        48 + ((mixed >> 16) & 0x9f) as u8,
        48 + ((mixed >> 32) & 0x9f) as u8,
    ]
}

fn direction_color(angle: f64) -> [u8; 3] {
    let phase = ((angle / std::f64::consts::TAU) + 1.0).fract() * 6.0;
    let segment = phase.floor() as u8;
    let x = ((1.0 - (phase % 2.0 - 1.0).abs()) * 220.0) as u8;
    match segment {
        0 => [235, x, 25],
        1 => [x, 235, 25],
        2 => [25, 235, x],
        3 => [25, x, 235],
        4 => [x, 25, 235],
        _ => [235, 25, x],
    }
}

fn lerp_color(a: [u8; 3], b: [u8; 3], t: f64) -> [u8; 3] {
    std::array::from_fn(|index| {
        (f64::from(a[index]) + (f64::from(b[index]) - f64::from(a[index])) * t) as u8
    })
}

fn write_png(path: &Path, width: u32, height: u32, pixels: &[u8]) -> Result<(), AtlasError> {
    let file = fs::File::create(path)?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_source_gamma(png::ScaledFloat::new(1.0 / 2.2));
    let mut writer = encoder.write_header().map_err(|error| {
        AtlasError::Corrupt(format!("PNG header for {}: {error}", path.display()))
    })?;
    writer.write_image_data(pixels).map_err(|error| {
        AtlasError::Corrupt(format!("PNG data for {}: {error}", path.display()))
    })?;
    Ok(())
}

fn export_globe_preview(atlas: &PlanetAtlas, path: &Path) -> Result<(), AtlasError> {
    const SIDE: u32 = 512;
    let mut pixels = vec![0u8; SIDE as usize * SIDE as usize * 3];
    let mut depth = vec![f32::NEG_INFINITY; SIDE as usize * SIDE as usize];
    for (pos, geometry) in atlas.genesis.geometry.iter() {
        let direction = geometry.unit_direction;
        // A fixed oblique view shows latitude and more than one cube face.
        let view_x = direction[0] * 0.866 + direction[2] * 0.5;
        let view_z = -direction[0] * 0.354 + direction[1] * 0.707 + direction[2] * 0.612;
        let view_y = direction[0] * 0.354 + direction[1] * 0.707 - direction[2] * 0.612;
        if view_z <= 0.0 {
            continue;
        }
        let x = (((view_x * 0.47 + 0.5) * SIDE as f32) as i32).clamp(0, SIDE as i32 - 1);
        let y = (((-view_y * 0.47 + 0.5) * SIDE as f32) as i32).clamp(0, SIDE as i32 - 1);
        let biome = atlas.genesis.biomes.get(pos).expect("matching atlas grids");
        let terrain = atlas
            .genesis
            .terrain
            .get(pos)
            .expect("matching atlas grids");
        let color = if terrain.eroded_elevation <= SEA_LEVEL as f32 {
            [22, 72, 145]
        } else {
            categorical_color(u64::from(biome.baseline_biome) + 1)
        };
        for dy in -1..=1 {
            for dx in -1..=1 {
                let px = (x + dx).clamp(0, SIDE as i32 - 1) as u32;
                let py = (y + dy).clamp(0, SIDE as i32 - 1) as u32;
                let index = (py * SIDE + px) as usize;
                if view_z > depth[index] {
                    depth[index] = view_z;
                    pixels[index * 3..index * 3 + 3].copy_from_slice(&color);
                }
            }
        }
    }
    write_png(path, SIDE, SIDE, &pixels)
}
