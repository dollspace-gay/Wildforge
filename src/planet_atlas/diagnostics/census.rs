//! Census coordination and sparse model/resource summaries.

use crate::chunk::{SEA_LEVEL};
use crate::planet::{Face, geodesic_distance};
use crate::planet_atlas::{AtlasError, AtlasPos, MineralKind, PlanetAtlas};
use crate::planet_atlas::diagnostics::{AtlasCensus};
use std::collections::{BTreeMap};
use super::census_cells::DenseCensus;
impl PlanetAtlas {
    pub fn census(&self) -> Result<AtlasCensus, AtlasError> {
        self.validate()?;
        let side = self.side();
        let mut cells_by_face = BTreeMap::new();
        for face in Face::ALL {
            cells_by_face.insert(face.name().to_string(), usize::from(side).pow(2));
        }
        let DenseCensus {
            physical_area,
            land_area,
            ocean_area,
            elevation_histogram,
            boundary_cells,
            detailed_boundary_cells,
            bedrock_areas,
            oceanic_age_histogram,
            climate_zone_areas,
            temperature_area_sum,
            minimum_temperature,
            maximum_temperature,
            precipitation_area_sum,
            maximum_precipitation,
            aridity_area_sum,
            maximum_aridity,
            biome_areas,
            biome_area_by_latitude_band,
            biome_area_by_continent,
            biome_area_by_elevation_band,
            biome_area_by_water_availability,
            habitat_areas,
            watersheds,
            lakes,
            provinces,
            hearts,
            drainage_edge_count,
            river_length_blocks,
            receiver_histogram,
            aquifer_storage_units,
            freshwater_units,
            total_water_units,
            floodplain_cells,
            wetland_cells,
            delta_cells,
            estuary_cells,
            resource_sites,
        } = DenseCensus::collect(self);
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
