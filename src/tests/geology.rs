//! Qualification tests for the causal spherical geology model.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use glam::{DQuat, DVec3};

use super::*;
use crate::planet::{Direction4, FACE_BLOCKS, Face, SurfacePos};
use crate::planet_atlas::{AtlasPos, DetailedBoundary, MineralKind, PlanetAtlas, VolcanoSource};
use crate::worldgen::Generator;

fn geology(seed: u32) -> PlanetAtlas {
    PlanetAtlas::fixture(seed, 32).unwrap()
}

#[test]
fn geological_seed_suite_meets_the_planetary_acceptance_bands() {
    let mut island_seeds = 0;
    let mut polar_land = false;
    let mut polar_ocean = false;
    let mut seam_coasts = 0;
    for seed in [3, 17, 91, 1_337] {
        let atlas = geology(seed);
        assert!((16..=22).contains(&atlas.geology.plates.len()));
        assert!((6..=9).contains(&atlas.geology.cratons.len()));
        let major: Vec<_> = atlas
            .geology
            .continents
            .iter()
            .filter(|continent| continent.major)
            .collect();
        assert!(
            (4..=7).contains(&major.len()),
            "seed {seed}: {} major continents",
            major.len()
        );
        assert!((0.62..=0.70).contains(&atlas.geology.achieved_ocean_fraction));
        assert!(atlas.geology.largest_ocean_share >= 0.90);
        assert!(
            major
                .iter()
                .all(|continent| continent.share_of_land <= 0.65)
        );
        assert!(atlas.geology.chosen_attempt < 8);
        assert_eq!(
            atlas.geology.rejected_attempts.len(),
            usize::from(atlas.geology.chosen_attempt)
        );
        island_seeds += usize::from(atlas.geology.continents.len() > major.len());
        for (pos, geometry) in atlas.genesis.geometry.iter() {
            let land = atlas.genesis.terrain.get(pos).unwrap().eroded_elevation > SEA_LEVEL as f32;
            if geometry.latitude_radians.abs() > 1.15 {
                polar_land |= land;
                polar_ocean |= !land;
            }
            for direction in [Direction4::East, Direction4::North] {
                let neighbor = pos.step(direction, atlas.side()).pos;
                if neighbor.face != pos.face {
                    let neighbor_land = atlas
                        .genesis
                        .terrain
                        .get(neighbor)
                        .unwrap()
                        .eroded_elevation
                        > SEA_LEVEL as f32;
                    seam_coasts += usize::from(land != neighbor_land);
                }
            }
        }
    }
    assert!(
        island_seeds >= 2,
        "islands occurred on only {island_seeds}/4 seeds"
    );
    assert!(polar_land && polar_ocean);
    assert!(seam_coasts > 0, "coastlines never crossed a cube-face seam");
}

#[test]
fn plate_partition_and_boundary_edges_are_closed_and_reciprocal() {
    let atlas = geology(91);
    let mut boundary_edges = 0usize;
    let mut seam_edges = 0usize;
    for (pos, cell) in atlas.genesis.tectonics.iter() {
        assert!(usize::from(cell.plate_id) < atlas.geology.plates.len());
        for direction in [Direction4::East, Direction4::North] {
            let neighbor = pos.step(direction, atlas.side()).pos;
            let other = atlas.genesis.tectonics.get(neighbor).unwrap();
            if other.plate_id == cell.plate_id {
                continue;
            }
            boundary_edges += 1;
            seam_edges += usize::from(pos.face != neighbor.face);
            let forward = atlas.boundary_between(pos, neighbor).unwrap();
            let reverse = atlas.boundary_between(neighbor, pos).unwrap();
            assert_eq!(forward.detail, reverse.detail);
            assert!((forward.strength - reverse.strength).abs() < 0.08);
        }
    }
    assert!(boundary_edges > 100);
    assert!(
        seam_edges > 0,
        "plate boundaries must cross cube-face seams"
    );
}

#[test]
fn relative_plate_motion_classification_is_rotationally_invariant() {
    let atlas = geology(91);
    let rotation = DQuat::from_axis_angle(DVec3::new(0.3, -0.7, 0.4).normalize(), 1.173);
    let rotated_plates: Vec<_> = atlas
        .geology
        .plates
        .iter()
        .cloned()
        .map(|mut plate| {
            let site = rotation
                * DVec3::new(
                    f64::from(plate.site_unit[0]),
                    f64::from(plate.site_unit[1]),
                    f64::from(plate.site_unit[2]),
                );
            let pole = rotation
                * DVec3::new(
                    f64::from(plate.euler_pole[0]),
                    f64::from(plate.euler_pole[1]),
                    f64::from(plate.euler_pole[2]),
                );
            plate.site_unit = site.as_vec3().to_array();
            plate.euler_pole = pole.as_vec3().to_array();
            plate
        })
        .collect();
    let mut compared = 0;
    for (a, a_cell) in atlas.genesis.tectonics.iter() {
        for b in a.neighbors4(atlas.side()) {
            let b_cell = atlas.genesis.tectonics.get(b).unwrap();
            if a_cell.plate_id == b_cell.plate_id {
                continue;
            }
            let a_point = DVec3::from_array(
                atlas
                    .genesis
                    .geometry
                    .get(a)
                    .unwrap()
                    .unit_direction
                    .map(f64::from),
            );
            let b_point = DVec3::from_array(
                atlas
                    .genesis
                    .geometry
                    .get(b)
                    .unwrap()
                    .unit_direction
                    .map(f64::from),
            );
            let point = (a_point + b_point).normalize();
            let (first, second, first_continental, second_continental) =
                if a_cell.plate_id < b_cell.plate_id {
                    (
                        a_cell.plate_id,
                        b_cell.plate_id,
                        a_cell.continental_crust >= 32_768,
                        b_cell.continental_crust >= 32_768,
                    )
                } else {
                    (
                        b_cell.plate_id,
                        a_cell.plate_id,
                        b_cell.continental_crust >= 32_768,
                        a_cell.continental_crust >= 32_768,
                    )
                };
            let baseline = crate::planet_atlas::classify_pair_rotation_probe(
                first,
                second,
                first_continental,
                second_continental,
                point,
                &atlas.geology.plates,
            );
            let rotated = crate::planet_atlas::classify_pair_rotation_probe(
                first,
                second,
                first_continental,
                second_continental,
                rotation * point,
                &rotated_plates,
            );
            assert_eq!(baseline.0, rotated.0);
            assert!((baseline.1 - rotated.1).abs() < 1.0e-5);
            compared += 1;
            if compared == 256 {
                break;
            }
        }
        if compared == 256 {
            break;
        }
    }
    assert_eq!(compared, 256);
}

#[test]
fn connected_boundary_runs_do_not_flicker_cell_by_cell() {
    let atlas = geology(17);
    let mut boundary = 0usize;
    let mut isolated = 0usize;
    for (pos, cell) in atlas.genesis.tectonics.iter() {
        if cell.boundary_detail == DetailedBoundary::Interior {
            continue;
        }
        boundary += 1;
        let agreeing = pos
            .neighbors8(atlas.side())
            .into_iter()
            .filter(|neighbor| {
                atlas
                    .genesis
                    .tectonics
                    .get(*neighbor)
                    .unwrap()
                    .boundary_detail
                    == cell.boundary_detail
            })
            .count();
        isolated += usize::from(agreeing == 0);
    }
    assert!(boundary > 100);
    assert!(
        isolated * 100 <= boundary,
        "{isolated}/{boundary} isolated classes"
    );
}

#[test]
fn relief_records_the_motion_that_caused_it() {
    let atlas = geology(1_337);
    let mut collision = 0;
    let mut subduction = 0;
    let mut ridges = 0;
    for (pos, cell) in atlas.genesis.tectonics.iter() {
        if cell.boundary_distance != 0 {
            continue;
        }
        let terrain = atlas.genesis.terrain.get(pos).unwrap();
        match cell.boundary_detail {
            DetailedBoundary::ContinentalCollision => {
                collision += 1;
                assert!(terrain.tectonic_contribution > 0.0);
            }
            DetailedBoundary::OceanContinentSubduction => {
                subduction += 1;
                if cell.continental_crust >= 32_768 {
                    assert!(terrain.tectonic_contribution > 0.0);
                } else {
                    assert!(terrain.tectonic_contribution < 0.0);
                }
            }
            DetailedBoundary::OceanRidge if cell.continental_crust < 32_768 => {
                ridges += 1;
                assert!(cell.oceanic_age <= 6);
                assert!(terrain.tectonic_contribution > 0.0);
            }
            _ => {}
        }
    }
    assert!(collision > 0 && subduction > 0 && ridges > 0);
}

#[test]
fn volcanic_arcs_exist_on_the_correct_side_of_eligible_boundaries() {
    for seed in [3, 17, 91, 1_337] {
        let atlas = geology(seed);
        for (detail, source) in [
            (
                DetailedBoundary::OceanContinentSubduction,
                VolcanoSource::ContinentalArc,
            ),
            (
                DetailedBoundary::OceanOceanSubduction,
                VolcanoSource::IslandArc,
            ),
            (DetailedBoundary::ContinentalRift, VolcanoSource::Rift),
            (DetailedBoundary::OceanRidge, VolcanoSource::OceanRidge),
        ] {
            let eligible: Vec<_> = atlas
                .genesis
                .tectonics
                .iter()
                .filter(|(_, cell)| {
                    cell.boundary_distance == 0
                        && cell.boundary_detail == detail
                        && (detail != DetailedBoundary::OceanContinentSubduction
                            || cell.continental_crust >= 32_768)
                        && (detail != DetailedBoundary::OceanOceanSubduction
                            || (cell.continental_crust < 32_768
                                && cell.plate_id < cell.neighbor_plate))
                        && (detail != DetailedBoundary::ContinentalRift
                            || cell.continental_crust >= 32_768)
                        && (detail != DetailedBoundary::OceanRidge
                            || cell.continental_crust < 32_768)
                })
                .map(|(pos, _)| pos)
                .collect();
            if eligible.is_empty() {
                continue;
            }
            let sites: Vec<_> = atlas
                .geology
                .volcanoes
                .iter()
                .filter(|volcano| volcano.source == source)
                .collect();
            assert!(!sites.is_empty(), "seed {seed} lacks {source:?}");
            assert!(sites.iter().any(|volcano| {
                eligible.iter().any(|boundary| {
                    crate::planet::geodesic_distance(
                        volcano.pos.center(atlas.side()),
                        boundary.center(atlas.side()),
                    ) <= f64::from(atlas.cell_blocks()) * 3.5
                })
            }));
        }
        for volcano in &atlas.geology.volcanoes {
            let crust = atlas
                .genesis
                .tectonics
                .get(volcano.pos)
                .unwrap()
                .continental_crust;
            match volcano.source {
                VolcanoSource::ContinentalArc | VolcanoSource::Rift => assert!(crust >= 32_768),
                VolcanoSource::IslandArc | VolcanoSource::OceanRidge => {
                    assert!(crust < 32_768)
                }
                VolcanoSource::Hotspot => {}
            }
        }
        for source in [VolcanoSource::ContinentalArc, VolcanoSource::IslandArc] {
            let sites: Vec<_> = atlas
                .geology
                .volcanoes
                .iter()
                .filter(|volcano| volcano.source == source)
                .collect();
            if atlas.side() >= 128 && !sites.is_empty() {
                assert!(sites.iter().any(|volcano| {
                    atlas
                        .genesis
                        .terrain
                        .get(volcano.pos)
                        .unwrap()
                        .eroded_elevation
                        > SEA_LEVEL as f32 + 2.0
                }));
            }
        }
    }
}

#[test]
fn hotspot_chains_age_and_erode_along_plate_motion() {
    let atlas = geology(1_337);
    let mut chains = BTreeMap::<u16, Vec<_>>::new();
    for volcano in &atlas.geology.volcanoes {
        if volcano.source == VolcanoSource::Hotspot {
            chains.entry(volcano.chain_id).or_default().push(volcano);
        }
    }
    assert!((4..=6).contains(&chains.len()));
    for chain in chains.values_mut() {
        chain.sort_by_key(|volcano| volcano.age_myr);
        assert_eq!(chain.len(), 5);
        for pair in chain.windows(2) {
            assert!(pair[1].age_myr > pair[0].age_myr);
            assert!(pair[1].erosion > pair[0].erosion);
            assert_eq!(pair[0].plate_id, pair[1].plate_id);
        }
    }
}

#[test]
fn terrain_has_no_seam_privilege_or_single_cell_tectonic_spikes() {
    let atlas = geology(3);
    let mut face_means = Vec::new();
    for face in Face::ALL {
        let values: Vec<_> = atlas
            .genesis
            .terrain
            .iter()
            .filter(|(pos, _)| pos.face == face)
            .map(|(_, cell)| cell.eroded_elevation)
            .collect();
        face_means.push(values.iter().sum::<f32>() / values.len() as f32);
    }
    let min = face_means.iter().copied().fold(f32::INFINITY, f32::min);
    let max = face_means.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    assert!(max - min < 35.0, "face means span {} blocks", max - min);
    for (pos, cell) in atlas.genesis.terrain.iter() {
        for neighbor in pos.neighbors4(atlas.side()) {
            let other = atlas.genesis.terrain.get(neighbor).unwrap();
            if cell.volcanic_contribution > 0.0 || other.volcanic_contribution > 0.0 {
                continue;
            }
            assert!(
                (cell.eroded_elevation - other.eroded_elevation).abs() < 70.0,
                "single-cell spike at {pos:?}"
            );
        }
    }
    let elevations: Vec<_> = atlas
        .genesis
        .terrain
        .values()
        .iter()
        .map(|cell| cell.eroded_elevation)
        .collect();
    assert!(
        elevations
            .iter()
            .filter(|e| **e < SEA_LEVEL as f32 - 18.0)
            .count()
            > 100
    );
    assert!(
        elevations
            .iter()
            .filter(|e| **e > SEA_LEVEL as f32 && **e < SEA_LEVEL as f32 + 20.0)
            .count()
            > 100
    );
    assert!(elevations.iter().any(|e| *e > SEA_LEVEL as f32 + 45.0));
}

#[test]
fn intrusions_cause_contact_metamorphism_and_not_random_coordinates() {
    let atlas = geology(17);
    assert!(!atlas.geology.intrusions.is_empty());
    for intrusion in atlas.geology.intrusions.iter().take(12) {
        assert!(
            atlas
                .genesis
                .tectonics
                .get(intrusion.pos)
                .unwrap()
                .metamorphic_grade
                > 0
        );
    }
    assert!(
        atlas
            .genesis
            .tectonics
            .values()
            .iter()
            .any(|cell| cell.metamorphic_grade == 0)
    );
}

#[test]
fn deposits_obey_hosts_budgets_and_progression_guarantees() {
    for seed in [17, 91, 1_337] {
        let atlas = geology(seed);
        let ids: BTreeSet<_> = atlas.geology.deposits.iter().map(|site| site.id).collect();
        assert_eq!(ids.len(), atlas.geology.deposits.len());
        for site in &atlas.geology.deposits {
            assert!(atlas.deposit_host_is_valid(site), "invalid host: {site:?}");
            assert!(
                site.tonnage_blocks
                    >= u64::from(site.max_blocks_per_chunk)
                        * u64::from(site.eligible_chunk_upper_bound)
            );
        }
        for mineral in MineralKind::ALL_TRACKED {
            assert!(
                atlas
                    .geology
                    .deposits
                    .iter()
                    .filter(|site| site.mineral == mineral)
                    .count()
                    >= 2,
                "seed {seed} lacks redundant {}",
                mineral.label()
            );
        }
        assert!(
            atlas
                .geology
                .deposits
                .iter()
                .filter(|site| site.mineral == MineralKind::Tin)
                .count()
                >= 3,
            "seed {seed} lacks three independent bronze bootstrap regions"
        );
        for continent in atlas.geology.continents.iter().filter(|c| c.major) {
            for mineral in [MineralKind::Copper, MineralKind::Iron, MineralKind::Coal] {
                assert!(
                    atlas.geology.deposits.iter().any(|site| {
                        site.landmass_id == continent.id && site.mineral == mineral
                    })
                );
            }
            assert!(
                atlas
                    .genesis
                    .terrain
                    .values()
                    .iter()
                    .zip(atlas.genesis.tectonics.values())
                    .any(|(terrain, tectonic)| {
                        terrain.landmass_id == continent.id
                            && matches!(
                                crate::planet_atlas::BedrockFamily::from_id(
                                    tectonic.bedrock_family
                                ),
                                crate::planet_atlas::BedrockFamily::Limestone
                                    | crate::planet_atlas::BedrockFamily::Marble
                            )
                    }),
                "seed {seed} major continent {} lacks basic carbonate flux",
                continent.id
            );
        }
    }
}

#[test]
fn voxel_ore_materialization_is_manifest_gated_and_bounded() {
    let reg = base_reg();
    let atlas = Arc::new(geology(91));
    let generator = Generator::with_atlas(91, &reg, atlas.clone());
    let site = atlas
        .geology
        .deposits
        .iter()
        .find(|site| site.mineral == MineralKind::Copper)
        .unwrap();
    let surface = SurfacePos::new(
        site.pos.face,
        site.pos.u * atlas.cell_blocks() + atlas.cell_blocks() / 2,
        site.pos.v * atlas.cell_blocks() + atlas.cell_blocks() / 2,
    )
    .unwrap();
    let chunk_pos = crate::planet::ChunkPos::from_surface(surface);
    let allowance = atlas.deposit_allowance(chunk_pos, MineralKind::Copper);
    let chunk = generator.generate(chunk_pos, &reg);
    let mut copper = 0u32;
    for x in 0..crate::chunk::CHUNK_X {
        for y in 0..crate::chunk::CHUNK_Y {
            for z in 0..crate::chunk::CHUNK_Z {
                if reg.block(chunk.get(x, y, z)).name.contains("copper_ore") {
                    copper += 1;
                }
            }
        }
    }
    assert!(
        copper <= allowance,
        "{copper} materialized over allowance {allowance}"
    );

    let outside = (0..atlas.genesis.geometry.len())
        .filter_map(|index| AtlasPos::from_index(index, atlas.side()))
        .map(|pos| {
            SurfacePos::new(
                pos.face,
                pos.u * atlas.cell_blocks() + atlas.cell_blocks() / 2,
                pos.v * atlas.cell_blocks() + atlas.cell_blocks() / 2,
            )
            .unwrap()
        })
        .map(crate::planet::ChunkPos::from_surface)
        .find(|chunk| atlas.deposit_allowance(*chunk, MineralKind::Copper) == 0)
        .unwrap();
    let chunk = generator.generate(outside, &reg);
    for block in chunk.raw() {
        assert!(
            !reg.block(crate::registry::BlockId(block))
                .name
                .contains("copper_ore")
        );
    }
}

#[test]
fn prospecting_reads_named_persisted_geology() {
    let reg = base_reg();
    let atlas = Arc::new(geology(1_337));
    let generator = Generator::with_atlas(1_337, &reg, atlas.clone());
    let volcano = &atlas.geology.volcanoes[0];
    let surface = SurfacePos::new(
        volcano.pos.face,
        volcano.pos.u * atlas.cell_blocks() + atlas.cell_blocks() / 2,
        volcano.pos.v * atlas.cell_blocks() + atlas.cell_blocks() / 2,
    )
    .unwrap();
    let reading = generator.prospect_at(surface);
    assert!(reading.province_name.is_some());
    assert!(reading.bedrock.is_some());
    assert_eq!(reading.volcano.unwrap().distance, 0);
}

#[test]
fn geology_manifest_corruption_is_refused() {
    let root = tmp_dir("geology-corrupt");
    geology(17).write_new(&root).unwrap();
    let path = root.join("planet/geology.wfg");
    let mut bytes = std::fs::read(&path).unwrap();
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0x41;
    std::fs::write(path, bytes).unwrap();
    let error = PlanetAtlas::load_fixture(&root).unwrap_err().to_string();
    assert!(error.contains("geology manifest"));
}

#[test]
fn fine_chunk_relief_stays_inside_the_atlas_envelope() {
    let reg = base_reg();
    let atlas = Arc::new(geology(3));
    let generator = Generator::with_atlas(3, &reg, atlas.clone());
    for face in Face::ALL {
        for (u, v) in [(1_024, 1_024), (4_096, 4_096), (7_100, 2_300)] {
            let pos = SurfacePos::new(face, u, v).unwrap();
            let coarse = atlas.terrain_sample(pos.center()).eroded_elevation
                - atlas.sampled_volcanic_relief(pos.center())
                + atlas.exact_volcanic_relief(pos.center());
            let fine = generator.pre_hydrology_surface_estimate_at(pos);
            assert!(
                (fine - coarse).abs() <= 2.51,
                "fine relief escaped at {pos:?}: coarse {coarse:.2}, fine {fine:.2}, delta {:.2}",
                (fine - coarse).abs()
            );
            let density_surface = generator.density_surface_estimate_at(pos) as f32;
            assert!(
                (density_surface - fine).abs() <= 7.0,
                "density surface escaped at {pos:?}: offset {fine:.2}, density {density_surface:.2}"
            );
        }
    }
}

#[test]
fn strata_cross_face_seams_and_fold_across_boundary_strike() {
    let reg = base_reg();
    let atlas = Arc::new(geology(1_337));
    let generator = Generator::with_atlas(1_337, &reg, atlas.clone());
    let mut continuous_seams = 0;
    for face in Face::ALL {
        for direction in [
            Direction4::East,
            Direction4::North,
            Direction4::West,
            Direction4::South,
        ] {
            for varying in [1_500u16, 4_096, 6_700] {
                let (u, v) = match direction {
                    Direction4::East => (FACE_BLOCKS - 1, varying),
                    Direction4::North => (varying, FACE_BLOCKS - 1),
                    Direction4::West => (0, varying),
                    Direction4::South => (varying, 0),
                };
                let here = SurfacePos::new(face, u, v).unwrap();
                let across = crate::planet::step4(here, direction).pos;
                let a = atlas.geology_sample(here.center());
                let b = atlas.geology_sample(across.center());
                if a.stratigraphic_stack == b.stratigraphic_stack
                    && a.geological_province == b.geological_province
                {
                    continuous_seams += 1;
                    let bands_a = generator.strata_bands_probe(here);
                    let bands_b = generator.strata_bands_probe(across);
                    assert!(
                        bands_a
                            .iter()
                            .zip(bands_b)
                            .all(|(left, right)| (*left - right).abs() <= 3)
                    );
                }
            }
        }
    }
    assert!(continuous_seams > 0);

    let mut along_change = 0.0f32;
    let mut across_change = 0.0f32;
    let mut samples = 0;
    for (pos, cell) in atlas.genesis.tectonics.iter() {
        if cell.boundary_detail != DetailedBoundary::ContinentalCollision
            || !(1..=5).contains(&cell.boundary_distance)
        {
            continue;
        }
        let center = pos.center(atlas.side());
        let surface = SurfacePos::new(
            center.face,
            center.u.floor() as u16,
            center.v.floor() as u16,
        )
        .unwrap();
        let strike = atlas.geology_sample(center).boundary_strike;
        if strike.length_squared() < 0.5 {
            continue;
        }
        let directions = [
            (Direction4::East, glam::Vec2::X),
            (Direction4::North, glam::Vec2::Y),
            (Direction4::West, -glam::Vec2::X),
            (Direction4::South, -glam::Vec2::Y),
        ];
        let along = directions
            .iter()
            .max_by(|a, b| {
                a.1.dot(strike)
                    .abs()
                    .partial_cmp(&b.1.dot(strike).abs())
                    .unwrap()
            })
            .unwrap()
            .0;
        let across = directions
            .iter()
            .min_by(|a, b| {
                a.1.dot(strike)
                    .abs()
                    .partial_cmp(&b.1.dot(strike).abs())
                    .unwrap()
            })
            .unwrap()
            .0;
        let base = generator.strata_bands_probe(surface)[2];
        let along_pos = pos.step(along, atlas.side()).pos.center(atlas.side());
        let across_pos = pos.step(across, atlas.side()).pos.center(atlas.side());
        let to_surface = |point: crate::planet::SurfacePoint| {
            SurfacePos::new(point.face, point.u.floor() as u16, point.v.floor() as u16).unwrap()
        };
        along_change +=
            (generator.strata_bands_probe(to_surface(along_pos))[2] - base).abs() as f32;
        across_change +=
            (generator.strata_bands_probe(to_surface(across_pos))[2] - base).abs() as f32;
        samples += 1;
    }
    assert!(samples > 10);
    assert!(
        across_change > along_change,
        "fold bands changed {along_change} along strike and {across_change} across it"
    );
}
