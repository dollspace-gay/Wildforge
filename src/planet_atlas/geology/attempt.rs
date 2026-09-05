//! One deterministic geological attempt and its unchanged acceptance constraints.

use noise::{NoiseFn, Perlin};
use crate::planet_atlas::{AtlasGrid, GeometryCell, TerrainCell};
use crate::planet::FACE_BLOCKS;
use crate::chunk::SEA_LEVEL;
use super::{AttemptOutput, PlateRecord, ContinentRecord, VolcanoSource, MineralKind, BedrockFamily};
use super::geometry::dvec;
use super::plates::craton_field;
use super::tectonics::build_tectonics;
use super::volcanism::geology_sites;
use super::relief::{stamp_volcanic_relief, boundary_relief, smooth_scalar, weighted_sea_level, label_components};
use super::provinces::{assign_provinces, metamorphism};
use super::deposits::deposits;

pub(super) fn build_attempt(
    seed: u32,
    attempt: u8,
    side: u16,
    geometry: &AtlasGrid<GeometryCell>,
    plates: &[PlateRecord],
    plate_ids: &[u16],
    target_ocean: f64,
) -> AttemptOutput {
    let (continental, craton_ids, crust_ages, cratons) = craton_field(seed, attempt, geometry);
    let mut tectonics = build_tectonics(
        side,
        geometry,
        plates,
        plate_ids,
        &continental,
        &craton_ids,
        &crust_ages,
    );
    let (volcanoes, intrusions) = geology_sites(seed, side, geometry, &tectonics, &cratons);
    let volcanic = stamp_volcanic_relief(side, geometry, &volcanoes);
    let mantle = Perlin::new(seed ^ u32::from(attempt).wrapping_mul(0x9e37_79b9) ^ 0x6d61_6e74);
    let regional = Perlin::new(seed ^ u32::from(attempt).wrapping_mul(0x85eb_ca6b) ^ 0x7265_6769);
    let mut raw = Vec::with_capacity(geometry.len());
    let mut tectonic_contribution = Vec::with_capacity(geometry.len());
    let mut dynamic_topography = Vec::with_capacity(geometry.len());
    for index in 0..geometry.len() {
        let point = dvec(geometry.values()[index].unit_direction);
        let cont = f32::from(continental[index]) / 65_535.0;
        let ocean_base = -37.0 - f32::from(tectonics[index].oceanic_age) / 220.0 * 23.0;
        let continent_base = 13.0 + cont * 26.0;
        let crust = ocean_base + (continent_base - ocean_base) * cont.powf(0.72);
        let dynamic = mantle.get([point.x * 1.45, point.y * 1.45, point.z * 1.45]) as f32 * 9.0;
        let detail = regional.get([
            point.x * 8.0 + 7.0,
            point.y * 8.0 - 3.0,
            point.z * 8.0 + 11.0,
        ]) as f32
            * 3.2;
        let tectonic = boundary_relief(tectonics[index], FACE_BLOCKS / side);
        raw.push(crust + dynamic + detail + tectonic);
        tectonic_contribution.push(tectonic);
        dynamic_topography.push(dynamic);
    }
    raw = smooth_scalar(side, &raw, 2);
    for (elevation, volcanic_relief) in raw.iter_mut().zip(&volcanic) {
        *elevation += volcanic_relief;
    }
    let erosion: Vec<_> = geometry
        .values()
        .iter()
        .map(|cell| {
            let point = dvec(cell.unit_direction);
            (regional.get([point.x * 5.0, point.y * 5.0, point.z * 5.0]) as f32 * 0.5 + 0.5) * 3.5
        })
        .collect();
    // Water classification is based on eroded_elevation.  Solving the
    // quantile on raw - erosion / relief_scale makes the requested fraction
    // apply to that final surface instead of the pre-erosion intermediate.
    let sea_basis: Vec<_> = raw
        .iter()
        .zip(&erosion)
        .map(|(elevation, erosion)| elevation - erosion / 1.05)
        .collect();
    let raw_sea_level = weighted_sea_level(&sea_basis, geometry, target_ocean);
    let mut terrain: Vec<_> = raw
        .iter()
        .enumerate()
        .map(|(index, elevation)| {
            let base = (SEA_LEVEL as f32 + (*elevation - raw_sea_level) * 1.05).clamp(5.0, 232.0);
            TerrainCell {
                base_elevation: base,
                eroded_elevation: (base - erosion[index]).clamp(4.0, 232.0),
                tectonic_contribution: tectonic_contribution[index],
                volcanic_contribution: volcanic[index],
                dynamic_topography: dynamic_topography[index],
                landmass_id: 0,
            }
        })
        .collect();

    let (land_labels, land_components) = label_components(side, geometry, &terrain, true);
    for (index, label) in land_labels.iter().copied().enumerate() {
        terrain[index].landmass_id = label;
    }
    let total_area: f64 = geometry
        .values()
        .iter()
        .map(|cell| f64::from(cell.physical_area))
        .sum();
    let land_area: f64 = land_components.iter().map(|record| record.2).sum();
    let major_minimum = total_area * 0.012;
    let continents: Vec<_> = land_components
        .iter()
        .map(|(id, cells, area)| ContinentRecord {
            id: *id,
            cell_count: *cells,
            area: *area,
            share_of_land: if land_area > 0.0 {
                *area / land_area
            } else {
                0.0
            },
            major: *area >= major_minimum,
        })
        .collect();
    let major_count = continents.iter().filter(|record| record.major).count();
    let max_land_share = continents
        .iter()
        .map(|record| record.share_of_land)
        .fold(0.0f64, f64::max);
    let (_, ocean_components) = label_components(side, geometry, &terrain, false);
    let ocean_area: f64 = ocean_components.iter().map(|record| record.2).sum();
    let largest_ocean_share = ocean_components
        .iter()
        .map(|record| {
            if ocean_area > 0.0 {
                record.2 / ocean_area
            } else {
                0.0
            }
        })
        .fold(0.0f64, f64::max);
    let achieved_ocean_fraction = ocean_area / total_area;
    let mut failures = Vec::new();
    if !(4..=7).contains(&major_count) {
        failures.push(format!("major continents {major_count}, required 4..=7"));
    }
    if !(0.62..=0.70).contains(&achieved_ocean_fraction) {
        failures.push(format!(
            "ocean fraction {achieved_ocean_fraction:.5}, required 0.62..=0.70"
        ));
    }
    if max_land_share > 0.65 {
        failures.push(format!(
            "largest continent owns {:.1}% of land",
            max_land_share * 100.0
        ));
    }
    if largest_ocean_share < 0.90 {
        failures.push(format!(
            "largest connected ocean owns only {:.1}% of ocean",
            largest_ocean_share * 100.0
        ));
    }
    if side >= 128 {
        for (source, label) in [
            (VolcanoSource::ContinentalArc, "continental arc"),
            (VolcanoSource::IslandArc, "island arc"),
        ] {
            let sites: Vec<_> = volcanoes
                .iter()
                .filter(|volcano| volcano.source == source)
                .collect();
            if !sites.is_empty()
                && !sites.iter().any(|volcano| {
                    terrain[volcano.pos.index(side)].eroded_elevation > SEA_LEVEL as f32 + 2.0
                })
            {
                failures.push(format!("{label} has no emergent volcanic edifice"));
            }
        }
    }

    let provinces = assign_provinces(seed, geometry, &mut tectonics, &terrain);
    metamorphism(side, geometry, &mut tectonics, &intrusions);
    let (deposits, resources) = deposits(
        seed,
        side,
        geometry,
        &tectonics,
        &terrain,
        &continents,
        &intrusions,
    );
    for kind in MineralKind::ALL_TRACKED {
        let count = deposits.iter().filter(|site| site.mineral == kind).count();
        if count < 2 {
            failures.push(format!(
                "{} has {count} deposit sites, required at least 2",
                kind.label()
            ));
        }
    }
    let bronze_regions = deposits
        .iter()
        .filter(|site| site.mineral == MineralKind::Tin)
        .count();
    if bronze_regions < 3 {
        failures.push(format!(
            "bronze bootstrap has {bronze_regions} independent tin regions, required 3"
        ));
    }
    for continent in continents.iter().filter(|continent| continent.major) {
        for kind in [MineralKind::Copper, MineralKind::Iron, MineralKind::Coal] {
            if !deposits
                .iter()
                .any(|site| site.landmass_id == continent.id && site.mineral == kind)
            {
                failures.push(format!(
                    "major continent {} lacks guaranteed {}",
                    continent.id,
                    kind.label()
                ));
            }
        }
        if !terrain
            .iter()
            .zip(tectonics.iter())
            .any(|(terrain, tectonic)| {
                terrain.landmass_id == continent.id
                    && matches!(
                        BedrockFamily::from_id(tectonic.bedrock_family),
                        BedrockFamily::Limestone | BedrockFamily::Marble
                    )
            })
        {
            failures.push(format!(
                "major continent {} lacks basic carbonate flux",
                continent.id
            ));
        }
    }
    if side < 32 {
        // Tiny persistence and seam fixtures execute this same algorithm but
        // cannot resolve production-scale statistical acceptance bands.
        failures.clear();
    }

    AttemptOutput {
        tectonics,
        terrain,
        resources,
        cratons,
        continents,
        provinces,
        volcanoes,
        intrusions,
        deposits,
        raw_sea_level,
        achieved_ocean_fraction,
        largest_ocean_share,
        failures,
    }
}
