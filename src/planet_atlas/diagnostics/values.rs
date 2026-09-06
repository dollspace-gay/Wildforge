//! Read-only mapping from registered layer names to atlas values.

use crate::chunk::SEA_LEVEL;
use crate::planet::{FACE_BLOCKS, PLANET_RADIUS, SurfacePos};
use crate::planet_atlas::{
    AtlasPos, BIOME_JUNGLE, GraftCompatibility, HABITAT_AQUATIC_BRACKISH, HABITAT_AQUATIC_FRESH,
    HABITAT_AQUATIC_SALT, HABITAT_BEACH_DUNE, HABITAT_FLOODPLAIN, HABITAT_LAKESHORE,
    HABITAT_RIPARIAN, HABITAT_SALT_MARSH, HABITAT_WETLAND, HYDRO_DELTA, HYDRO_ESTUARY,
    HYDRO_FLOODPLAIN, HYDRO_WETLAND, PlanetAtlas, SurfaceReservoirKind, surface_reservoir_id,
};
use glam::DVec3;

pub(in crate::planet_atlas::diagnostics) fn layer_value(
    atlas: &PlanetAtlas,
    id: &str,
    pos: AtlasPos,
) -> f64 {
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
