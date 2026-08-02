use std::sync::Arc;

use glam::Vec3;

use super::*;
use crate::planet::{
    Direction4, FACE_BLOCKS, Face, QuarterTurn, SurfacePoint, canonicalize_surface_point,
};
use crate::planet_atlas::{
    ATLAS_CELL_BLOCKS, ATLAS_CELL_COUNT, ATLAS_DYNAMIC_FILE_BYTES,
    ATLAS_ESTIMATED_GENERATION_PEAK_BYTES, ATLAS_ESTIMATED_LOADED_BYTES, ATLAS_FACE_SIDE,
    ATLAS_IMMUTABLE_FILE_BYTES, AtlasConfig, AtlasGrid, AtlasPos, CancellationToken,
    GenerationMode, PlanetAtlas, export_diagnostics,
};

const _: () = {
    assert!(ATLAS_IMMUTABLE_FILE_BYTES < 128 * 1024 * 1024);
    assert!(ATLAS_DYNAMIC_FILE_BYTES < 64 * 1024 * 1024);
    assert!(ATLAS_ESTIMATED_LOADED_BYTES < 512 * 1024 * 1024);
    assert!(ATLAS_ESTIMATED_GENERATION_PEAK_BYTES < 512 * 1024 * 1024);
};

#[test]
fn production_atlas_dimensions_are_fixed() {
    assert_eq!(ATLAS_CELL_BLOCKS, 32);
    assert_eq!(ATLAS_FACE_SIDE, 256);
    assert_eq!(ATLAS_CELL_COUNT, 393_216);
}

#[test]
fn serial_and_parallel_atlas_generation_are_identical() {
    let cancel = CancellationToken::default();
    let serial = PlanetAtlas::generate(
        0x51eed,
        0xabc,
        AtlasConfig {
            side: 8,
            mode: GenerationMode::Serial,
        },
        &cancel,
        |_| {},
    )
    .unwrap();
    let parallel = PlanetAtlas::generate(
        0x51eed,
        0xabc,
        AtlasConfig {
            side: 8,
            mode: GenerationMode::Parallel,
        },
        &cancel,
        |_| {},
    )
    .unwrap();
    assert_eq!(serial.genesis, parallel.genesis);
    assert_eq!(serial.dynamic, parallel.dynamic);
    assert_eq!(serial.geology, parallel.geology);
    assert_eq!(serial.hydrology, parallel.hydrology);
    assert_eq!(
        serial.manifest.genesis_checksum,
        parallel.manifest.genesis_checksum
    );
    assert_eq!(
        serial.manifest.geology_checksum,
        parallel.manifest.geology_checksum
    );
    assert_eq!(
        serial.manifest.hydrology_checksum,
        parallel.manifest.hydrology_checksum
    );
    assert_eq!(
        serial.manifest.dynamic_checksum,
        parallel.manifest.dynamic_checksum
    );
    let serial_stages: Vec<_> = serial
        .manifest
        .stages
        .iter()
        .map(|stage| stage.checksum)
        .collect();
    let parallel_stages: Vec<_> = parallel
        .manifest
        .stages
        .iter()
        .map(|stage| stage.checksum)
        .collect();
    assert_eq!(serial_stages, parallel_stages);
}

#[test]
fn atlas_round_trips_every_owned_layer() {
    let root = tmp_dir("atlas-round-trip");
    let atlas = PlanetAtlas::fixture(77, 8).unwrap();
    atlas.write_new(&root).unwrap();
    let loaded = PlanetAtlas::load_fixture(&root).unwrap();
    assert_eq!(loaded, atlas);
}

#[test]
fn corrupt_genesis_fails_instead_of_regenerating() {
    let root = tmp_dir("atlas-corrupt-genesis");
    let atlas = PlanetAtlas::fixture(12, 4).unwrap();
    atlas.write_new(&root).unwrap();
    let path = root.join("planet/genesis.wfa");
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[40] ^= 0x5a;
    std::fs::write(path, bytes).unwrap();
    let error = PlanetAtlas::load_fixture(&root).unwrap_err().to_string();
    assert!(error.contains("checksum") || error.contains("genesis"));
}

#[test]
fn corrupt_dynamic_state_without_backup_fails_closed() {
    let root = tmp_dir("atlas-dynamic-recovery");
    let atlas = PlanetAtlas::fixture(91, 4).unwrap();
    atlas.write_new(&root).unwrap();
    std::fs::write(root.join("planet/dynamic.wfd"), b"broken").unwrap();
    let error = PlanetAtlas::load_fixture(&root).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("refusing to mint or destroy water")
    );
}

#[test]
fn corrupt_dynamic_primary_restores_the_last_valid_backup() {
    let root = tmp_dir("atlas-dynamic-backup-recovery");
    let atlas = PlanetAtlas::fixture(92, 4).unwrap();
    atlas.write_new(&root).unwrap();
    let mut first = atlas.dynamic.clone();
    first.completed_climate_hours = 17;
    first.cells.values_mut()[0].atmospheric_vapor -= 11;
    first.cells.values_mut()[0].cloud_water += 11;
    let mut first_water = atlas.water_cycle.clone();
    first_water.completed_surface_hours = 17;
    atlas
        .save_dynamic_snapshot(&root, &first, &first_water)
        .unwrap();
    let mut second = first.clone();
    second.completed_climate_hours = 18;
    second.cells.values_mut()[0].atmospheric_vapor -= 7;
    second.cells.values_mut()[0].cloud_water += 7;
    let mut second_water = first_water.clone();
    second_water.completed_surface_hours = 18;
    atlas
        .save_dynamic_snapshot(&root, &second, &second_water)
        .unwrap();

    std::fs::write(root.join("planet/dynamic.wfd"), b"broken").unwrap();
    let loaded = PlanetAtlas::load_fixture(&root).unwrap();
    assert_eq!(loaded.dynamic, first);
    assert_eq!(loaded.water_cycle, first_water);
    // The manifest now commits the restored payload; a second load proves it
    // without reaching into the private codec.
    assert_eq!(PlanetAtlas::load_fixture(&root).unwrap().dynamic, first);
}

#[test]
fn unknown_atlas_versions_are_refused() {
    let root = tmp_dir("atlas-newer-version");
    PlanetAtlas::fixture(8, 4)
        .unwrap()
        .write_new(&root)
        .unwrap();
    let path = root.join("planet/manifest.toml");
    let current = format!(
        "format_version = {}",
        crate::planet_atlas::ATLAS_FORMAT_VERSION
    );
    let text = std::fs::read_to_string(&path)
        .unwrap()
        .replace(&current, "format_version = 999");
    std::fs::write(path, text).unwrap();
    let error = PlanetAtlas::load_fixture(&root).unwrap_err().to_string();
    assert!(error.contains("unsupported"));
}

#[test]
fn saved_genesis_survives_an_older_algorithm_manifest() {
    let root = tmp_dir("atlas-algorithm-migration");
    let atlas = PlanetAtlas::fixture(17, 4).unwrap();
    atlas.write_new(&root).unwrap();
    let path = root.join("planet/manifest.toml");
    let text = std::fs::read_to_string(&path)
        .unwrap()
        .replace("atlas_algorithm_version = 4", "atlas_algorithm_version = 3");
    std::fs::write(path, text).unwrap();
    let loaded = PlanetAtlas::load_fixture(&root).unwrap();
    assert_eq!(
        loaded.manifest.genesis_checksum,
        atlas.manifest.genesis_checksum
    );
    assert_eq!(loaded.genesis, atlas.genesis);
}

#[test]
fn truncated_payload_fails_before_record_decoding() {
    let root = tmp_dir("atlas-truncated");
    PlanetAtlas::fixture(18, 4)
        .unwrap()
        .write_new(&root)
        .unwrap();
    let path = root.join("planet/genesis.wfa");
    let mut bytes = std::fs::read(&path).unwrap();
    bytes.truncate(47);
    std::fs::write(path, bytes).unwrap();
    let error = PlanetAtlas::load_fixture(&root).unwrap_err().to_string();
    assert!(error.contains("length") || error.contains("dimensions"));
}

#[test]
fn cancellation_never_publishes_a_selectable_world() {
    let root = tmp_dir("atlas-cancel-world");
    let destination = root.join("cancelled");
    let cancel = CancellationToken::default();
    cancel.cancel();
    let error =
        crate::world::create_world_fixture_atomic(&destination, 44, "survival", 4, &cancel, |_| {})
            .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::Other);
    assert!(!destination.exists());
    assert!(crate::world::list_worlds(&root).is_empty());
}

#[test]
#[ignore = "production world creation probe; run explicitly for release qualification"]
fn production_world_creation_publishes_and_reopens() {
    let seed = std::env::var("WILDFORGE_CREATION_SEED")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(42);
    let root = tmp_dir("production-world-creation");
    let destination = root.join("world1");
    let reg = base_reg();
    let content_hash = crate::planet_atlas::genesis_content_hash(Path::new("mods"));
    crate::world::create_world_atomic(
        &destination,
        seed,
        "survival",
        content_hash,
        reg.clone(),
        &CancellationToken::default(),
        |_| {},
    )
    .unwrap();
    assert_eq!(
        crate::world::list_worlds(&root),
        vec![("world1".into(), seed)]
    );
    let mut reopened = World::load_or_create(destination, reg).unwrap();
    reopened.prepare_common_spawn(|_, _, _| {}).unwrap();
    let audit = reopened.material_ledger.as_ref().unwrap().audit();
    assert!(audit.is_balanced());
    assert!(audit.is_qualified());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_metadata_only_directory_is_not_selectable() {
    let root = tmp_dir("atlas-partial-hidden");
    crate::world::write_world_meta(&root.join("partial"), 4, "survival", 0.0).unwrap();
    assert!(crate::world::list_worlds(&root).is_empty());
}

#[test]
fn components_and_neighbors_cross_face_seams() {
    let atlas = PlanetAtlas::fixture(2, 8).unwrap();
    let source = AtlasPos {
        face: Face::PosX,
        u: 7,
        v: 3,
    };
    let across = source.step(Direction4::East, atlas.side()).pos;
    assert_ne!(source.face, across.face);
    assert!(across.neighbors4(atlas.side()).contains(&source));
    let mut included = AtlasGrid::filled(atlas.side(), false).unwrap();
    *included.get_mut(source).unwrap() = true;
    *included.get_mut(across).unwrap() = true;
    let (labels, count) = atlas
        .connected_components(&included, |value| *value)
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(labels.get(source), labels.get(across));
}

#[test]
fn every_atlas_edge_is_reciprocal_and_scalar_samples_agree() {
    let atlas = PlanetAtlas::fixture(39, 8).unwrap();
    let side = atlas.side();
    let cell_blocks = f64::from(atlas.cell_blocks());
    for face in Face::ALL {
        for direction in Direction4::ALL {
            for varying in 0..side {
                let source = match direction {
                    Direction4::East => AtlasPos {
                        face,
                        u: side - 1,
                        v: varying,
                    },
                    Direction4::North => AtlasPos {
                        face,
                        u: varying,
                        v: side - 1,
                    },
                    Direction4::West => AtlasPos {
                        face,
                        u: 0,
                        v: varying,
                    },
                    Direction4::South => AtlasPos {
                        face,
                        u: varying,
                        v: 0,
                    },
                };
                let crossed = source.step(direction, side);
                let returned = crossed.pos.step(crossed.direction.opposite(), side);
                assert_eq!(returned.pos, source);
                assert_eq!(returned.rotation.then(crossed.rotation).turns(), 0);

                let source_point = match direction {
                    Direction4::East => SurfacePoint {
                        face,
                        u: f64::from(FACE_BLOCKS),
                        v: (f64::from(varying) + 0.5) * cell_blocks,
                    },
                    Direction4::North => SurfacePoint {
                        face,
                        u: (f64::from(varying) + 0.5) * cell_blocks,
                        v: f64::from(FACE_BLOCKS),
                    },
                    Direction4::West => SurfacePoint {
                        face,
                        u: 0.0,
                        v: (f64::from(varying) + 0.5) * cell_blocks,
                    },
                    Direction4::South => SurfacePoint {
                        face,
                        u: (f64::from(varying) + 0.5) * cell_blocks,
                        v: 0.0,
                    },
                };
                let destination_point = match crossed.direction.opposite() {
                    Direction4::West => SurfacePoint {
                        face: crossed.pos.face,
                        u: 0.0,
                        v: (f64::from(crossed.pos.v) + 0.5) * cell_blocks,
                    },
                    Direction4::East => SurfacePoint {
                        face: crossed.pos.face,
                        u: f64::from(FACE_BLOCKS),
                        v: (f64::from(crossed.pos.v) + 0.5) * cell_blocks,
                    },
                    Direction4::South => SurfacePoint {
                        face: crossed.pos.face,
                        u: (f64::from(crossed.pos.u) + 0.5) * cell_blocks,
                        v: 0.0,
                    },
                    Direction4::North => SurfacePoint {
                        face: crossed.pos.face,
                        u: (f64::from(crossed.pos.u) + 0.5) * cell_blocks,
                        v: f64::from(FACE_BLOCKS),
                    },
                };
                let sample = |point| {
                    atlas.sample_scalar(point, |pos| {
                        atlas.genesis.geometry.get(pos).unwrap().latitude_radians
                    })
                };
                assert!(
                    (sample(source_point) - sample(destination_point)).abs() < 0.0001,
                    "scalar mismatch at {face}/{direction:?}/{varying}"
                );
            }
        }
    }
}

#[test]
fn connected_components_cross_a_cube_corner() {
    let atlas = PlanetAtlas::fixture(40, 8).unwrap();
    let corner = AtlasPos {
        face: Face::PosX,
        u: atlas.side() - 1,
        v: atlas.side() - 1,
    };
    let east = corner.step(Direction4::East, atlas.side()).pos;
    let north = corner.step(Direction4::North, atlas.side()).pos;
    assert_ne!(east.face, north.face);
    let mut included = AtlasGrid::filled(atlas.side(), false).unwrap();
    for pos in [corner, east, north] {
        *included.get_mut(pos).unwrap() = true;
    }
    let (_, count) = atlas
        .connected_components(&included, |value| *value)
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn scalar_and_tangent_sampling_are_continuous_at_a_seam() {
    let atlas = PlanetAtlas::fixture(5, 16).unwrap();
    let before = SurfacePoint {
        face: Face::PosX,
        u: f64::from(FACE_BLOCKS) - 0.01,
        v: 3100.25,
    };
    let after = canonicalize_surface_point(Face::PosX, f64::from(FACE_BLOCKS) + 0.01, 3100.25)
        .unwrap()
        .point;
    let scalar = |point| {
        atlas.sample_scalar(point, |pos| {
            atlas.genesis.geometry.get(pos).unwrap().latitude_radians
        })
    };
    assert!((scalar(before) - scalar(after)).abs() < 0.01);

    let (source, direction, step) = Face::ALL
        .into_iter()
        .flat_map(|face| Direction4::ALL.map(move |direction| (face, direction)))
        .find_map(|(face, direction)| {
            let (u, v) = match direction {
                Direction4::East => (atlas.side() - 1, atlas.side() / 2),
                Direction4::North => (atlas.side() / 2, atlas.side() - 1),
                Direction4::West => (0, atlas.side() / 2),
                Direction4::South => (atlas.side() / 2, 0),
            };
            let source = AtlasPos { face, u, v };
            let step = source.step(direction, atlas.side());
            (step.rotation.turns() != 0).then_some((source, direction, step))
        })
        .expect("cube topology has rotated seams");
    let middle = f64::from(FACE_BLOCKS) * 0.43;
    let (before, outside_u, outside_v) = match direction {
        Direction4::East => (
            SurfacePoint {
                face: source.face,
                u: f64::from(FACE_BLOCKS) - 0.01,
                v: middle,
            },
            f64::from(FACE_BLOCKS) + 0.01,
            middle,
        ),
        Direction4::North => (
            SurfacePoint {
                face: source.face,
                u: middle,
                v: f64::from(FACE_BLOCKS) - 0.01,
            },
            middle,
            f64::from(FACE_BLOCKS) + 0.01,
        ),
        Direction4::West => (
            SurfacePoint {
                face: source.face,
                u: 0.01,
                v: middle,
            },
            -0.01,
            middle,
        ),
        Direction4::South => (
            SurfacePoint {
                face: source.face,
                u: middle,
                v: 0.01,
            },
            middle,
            -0.01,
        ),
    };
    let after = canonicalize_surface_point(source.face, outside_u, outside_v)
        .unwrap()
        .point;
    let source_vector = Vec3::X;
    let destination_vector = step.rotation.rotate_vec3(source_vector);
    let field = |pos: AtlasPos| {
        if pos.face == source.face {
            [source_vector.x, source_vector.z]
        } else if pos.face == step.pos.face {
            [destination_vector.x, destination_vector.z]
        } else {
            [0.0, 0.0]
        }
    };
    let sampled_before = atlas.sample_tangent_vector(before, field);
    let sampled_after = atlas.sample_tangent_vector(after, field);
    let after_in_source = QuarterTurn::new(4 - step.rotation.turns()).rotate_vec3(Vec3::new(
        sampled_after.x,
        0.0,
        sampled_after.y,
    ));
    assert!(sampled_before.dot(glam::Vec2::new(after_in_source.x, after_in_source.z)) > 0.999);
}

#[test]
fn radius_queries_use_the_same_cells_from_either_side_of_a_seam() {
    let atlas = PlanetAtlas::fixture(21, 16).unwrap();
    let source = SurfacePoint {
        face: Face::PosX,
        u: f64::from(FACE_BLOCKS),
        v: 3700.0,
    };
    let across = canonicalize_surface_point(source.face, source.u + 0.01, source.v)
        .unwrap()
        .point;
    let destination = SurfacePoint {
        face: across.face,
        u: 0.0,
        v: across.v,
    };
    let from_source: std::collections::BTreeSet<_> =
        atlas.radius_query(source, 700.0).into_iter().collect();
    let from_destination: std::collections::BTreeSet<_> =
        atlas.radius_query(destination, 700.0).into_iter().collect();
    assert_eq!(from_source, from_destination);
}

#[test]
fn authoritative_chunk_height_consumes_atlas_terrain() {
    let reg = base_reg();
    let low = PlanetAtlas::fixture(28, 8).unwrap();
    let mut high = low.clone();
    for face in Face::ALL {
        for v in 0..high.side() {
            for u in 0..high.side() {
                let cell = high
                    .genesis
                    .terrain
                    .get_mut(AtlasPos { face, u, v })
                    .unwrap();
                cell.base_elevation += 24.0;
                cell.eroded_elevation += 24.0;
            }
        }
    }
    let dry_land = low
        .genesis
        .terrain
        .iter()
        .find(|(pos, terrain)| {
            let center = pos.center(low.side());
            terrain.eroded_elevation > crate::chunk::SEA_LEVEL as f32 + 8.0
                && !low.hydrology_sample(center).near_channel
        })
        .map(|(pos, _)| pos)
        .expect("fixture has unconstrained dry land");
    let center = dry_land.center(8);
    let point = crate::planet::SurfacePos::new(
        center.face,
        center.u.floor() as u16,
        center.v.floor() as u16,
    )
    .unwrap();
    let low_generator = crate::worldgen::Generator::with_atlas(28, &reg, Arc::new(low));
    let high_generator = crate::worldgen::Generator::with_atlas(28, &reg, Arc::new(high));
    assert!(
        high_generator.surface_estimate_at(point) - low_generator.surface_estimate_at(point) >= 20
    );
}

#[test]
fn exporter_covers_every_registered_layer_and_all_faces() {
    let root = tmp_dir("atlas-export");
    let atlas = PlanetAtlas::fixture(101, 4).unwrap();
    let report = export_diagnostics(&atlas, &root).unwrap();
    assert_eq!(report.cell_count, 6 * 4 * 4);
    assert_eq!(report.registered_layers.len(), report.exported_maps.len());
    for layer in &report.registered_layers {
        assert!(root.join(format!("maps/{layer}.png")).is_file());
        assert!(root.join(format!("maps/{layer}.legend.txt")).is_file());
    }
    let census = atlas.census().unwrap();
    assert_eq!(census.cells_by_face.values().sum::<usize>(), census.cells);
    assert!((census.land_fraction + census.ocean_fraction - 1.0).abs() < 1e-9);
    assert!(root.join("globe_preview.png").is_file());
    assert!(root.join("validation-report.toml").is_file());
    assert!(root.join("geology.toml").is_file());
    assert!(root.join("hydrology.toml").is_file());
    assert!(root.join("biomes.toml").is_file());
    assert!(root.join("country-adjacency.csv").is_file());
    assert!(root.join("river-profiles.csv").is_file());
    assert!(root.join("qualification-sites.toml").is_file());
    assert!(root.join("climate-transects.csv").is_file());
    assert!(root.join("weather-tracks.csv").is_file());
    for hour in [1, 6, 12, 24] {
        assert!(
            root.join(format!("maps/weather_hour_{hour:02}.png"))
                .is_file()
        );
    }
}

#[test]
fn dynamic_scans_are_sliced_and_complete_exactly_once() {
    let mut atlas = PlanetAtlas::fixture(72, 4).unwrap();
    let mut scan = crate::planet_atlas::DynamicScan::default();
    let mut visited = std::collections::BTreeSet::new();
    let mut completed = false;
    while !completed {
        completed = scan.step(&mut atlas, 7, |pos, _| {
            assert!(visited.insert(pos));
        });
    }
    assert_eq!(visited.len(), atlas.dynamic.cells.len());
}

#[test]
fn genesis_content_hash_covers_built_in_and_mod_content() {
    let root = tmp_dir("atlas-content-hash");
    let empty = crate::planet_atlas::genesis_content_hash(&root);
    std::fs::write(root.join("content.toml"), "value = 1").unwrap();
    let changed = crate::planet_atlas::genesis_content_hash(&root);
    assert_ne!(empty, changed);
    assert_eq!(changed, crate::planet_atlas::genesis_content_hash(&root));
}

#[test]
fn chunk_generation_order_does_not_change_atlas_or_chunks() {
    let reg = base_reg();
    let atlas = Arc::new(PlanetAtlas::fixture(303, 8).unwrap());
    let generator = crate::worldgen::Generator::with_atlas(303, &reg, atlas.clone());
    let a = ChunkPos::new(Face::PosZ, 250, 250).unwrap();
    let b = a.offset(1, 0);
    let first_a = generator.generate(a, &reg);
    let first_b = generator.generate(b, &reg);
    let second_b = generator.generate(b, &reg);
    let second_a = generator.generate(a, &reg);
    for x in 0..crate::chunk::CHUNK_X {
        for z in 0..crate::chunk::CHUNK_Z {
            for y in 0..crate::chunk::CHUNK_Y {
                assert_eq!(first_a.get(x, y, z), second_a.get(x, y, z));
                assert_eq!(first_b.get(x, y, z), second_b.get(x, y, z));
            }
        }
    }
    assert_eq!(
        atlas.manifest.genesis_checksum,
        PlanetAtlas::fixture(303, 8)
            .unwrap()
            .manifest
            .genesis_checksum
    );
}

#[test]
fn soils_and_habitat_overlays_obey_their_physical_causes() {
    use crate::planet_atlas::{
        FREEZE_PERMAFROST, HABITAT_AQUATIC_BRACKISH, HABITAT_AQUATIC_FRESH, HABITAT_AQUATIC_SALT,
        HABITAT_OASIS, HABITAT_PERMAFROST, HABITAT_SPRING,
    };

    let atlas = PlanetAtlas::fixture(8_701, 64).unwrap();
    let aquatic_mask = HABITAT_AQUATIC_FRESH | HABITAT_AQUATIC_BRACKISH | HABITAT_AQUATIC_SALT;
    let mut springs = 0usize;
    let mut shallow_fresh = 0usize;
    let mut shallow_max_permeability = 0u16;
    let mut terrestrial = 0usize;
    for index in 0..atlas.genesis.ground.len() {
        let ground = atlas.genesis.ground.values()[index];
        let biome = atlas.genesis.biomes.values()[index];
        let terrain = atlas.genesis.terrain.values()[index];
        let climate = atlas.genesis.climate.values()[index];
        assert!(u16::from(ground.sand) + u16::from(ground.silt) <= 245);
        assert!((1..=80).contains(&ground.soil_depth_decimeters));
        let aquatic = biome.habitat_flags & aquatic_mask;
        if aquatic == 0
            && ground.baseline_groundwater_head >= terrain.eroded_elevation - 2.0
            && ground.soil_salinity < 64
        {
            shallow_fresh += 1;
            shallow_max_permeability = shallow_max_permeability.max(ground.aquifer_permeability);
        }
        assert!(
            aquatic.count_ones() <= 1,
            "water salinity classes are exclusive"
        );
        if terrain.eroded_elevation > crate::chunk::SEA_LEVEL as f32 {
            terrestrial += 1;
        }
        if biome.habitat_flags & HABITAT_SPRING != 0 {
            springs += 1;
            assert_eq!(aquatic, 0, "a spring outlet is not a standing water body");
            assert!(
                ground.baseline_groundwater_head >= terrain.eroded_elevation - 2.0,
                "springs require a shallow water table"
            );
            assert!(
                ground.soil_salinity < 64,
                "springs require fresh groundwater"
            );
        }
        if biome.habitat_flags & HABITAT_OASIS != 0 {
            assert_ne!(biome.habitat_flags & HABITAT_SPRING, 0);
            assert!(climate.aridity >= 1.02 || climate.mean_precipitation < 620.0);
        }
        assert_eq!(
            biome.habitat_flags & HABITAT_PERMAFROST != 0,
            ground.freeze_flags & FREEZE_PERMAFROST != 0,
            "permafrost habitat is a soil/climate fact"
        );
    }
    assert!(terrestrial > 0);
    assert!(
        springs > 0,
        "fixture contains groundwater-fed spring outlets (shallow={shallow_fresh}, max permeability={shallow_max_permeability})"
    );
}

#[test]
fn legacy_riparian_flags_require_a_physical_river_at_query_time() {
    use crate::planet_atlas::{HABITAT_RIPARIAN, HYDRO_FLOODPLAIN, HYDRO_RIVER};

    let mut atlas = PlanetAtlas::fixture(8_705, 16).unwrap();
    let dry_index = atlas
        .genesis
        .hydrology
        .values()
        .iter()
        .enumerate()
        .find_map(|(index, hydro)| {
            (hydro.flags & (HYDRO_RIVER | HYDRO_FLOODPLAIN) == 0
                && hydro.water_body == crate::planet_atlas::WaterBodyKind::Land)
                .then_some(index)
        })
        .expect("fixture contains land without a river");
    atlas.genesis.biomes.values_mut()[dry_index].habitat_flags |= HABITAT_RIPARIAN;
    let atlas_pos = AtlasPos::from_index(dry_index, atlas.side()).unwrap();
    let center = atlas_pos.center(atlas.side());
    let surface = crate::planet::SurfacePos::new(
        center.face,
        center.u.floor() as u16,
        center.v.floor() as u16,
    )
    .unwrap();

    assert_eq!(
        atlas.biome_sample(surface).habitat_flags & HABITAT_RIPARIAN,
        0,
        "legacy discharge-only riparian flag leaked through without a channel"
    );
}

#[test]
fn zonal_biomes_and_local_habitats_follow_their_causes_across_seams() {
    use crate::planet_atlas::{
        BIOME_ARCTIC, BIOME_BADLANDS, BIOME_DESERT, BIOME_FOREST, BIOME_JUNGLE, BIOME_MOUNTAINS,
        BIOME_SCRUBLAND, BIOME_TAIGA, BIOME_TUNDRA, HABITAT_AQUATIC_FRESH, HABITAT_ESTUARY_DELTA,
        HABITAT_RIPARIAN, HABITAT_WETLAND, HYDRO_DELTA, HYDRO_ESTUARY, HYDRO_RIVER, HYDRO_WETLAND,
    };

    let atlas = PlanetAtlas::fixture(8_705, 64).unwrap();
    let side = atlas.side();
    let slope = |pos: AtlasPos| {
        let elevation = atlas.genesis.terrain.get(pos).unwrap().eroded_elevation;
        pos.neighbors4(side)
            .into_iter()
            .map(|neighbor| {
                (atlas
                    .genesis
                    .terrain
                    .get(neighbor)
                    .unwrap()
                    .eroded_elevation
                    - elevation)
                    .abs()
            })
            .fold(0.0f32, f32::max)
    };
    let mut tropical_wet = 0usize;
    let mut hot_arid = 0usize;
    let mut temperate_moist = 0usize;
    let mut taiga = 0usize;
    let mut tundra = 0usize;
    let mut arctic = 0usize;
    let mut alpine = 0usize;
    let mut low_latitude_tree_lines = Vec::new();
    let mut high_latitude_tree_lines = Vec::new();
    let mut coherent = 0usize;
    let mut comparable = 0usize;
    let mut arid_riparian = Vec::new();
    let mut arid_dry = Vec::new();
    let mut seam_river_corridors = 0usize;
    let mut deltas = 0usize;
    let mut estuaries = 0usize;

    for (pos, biome) in atlas.genesis.biomes.iter() {
        let climate = *atlas.genesis.climate.get(pos).unwrap();
        let terrain = *atlas.genesis.terrain.get(pos).unwrap();
        let hydro = *atlas.genesis.hydrology.get(pos).unwrap();
        let ground = *atlas.genesis.ground.get(pos).unwrap();
        let latitude = atlas.genesis.geometry.get(pos).unwrap().latitude_radians;
        let warmest = climate
            .seasonal_temperature
            .into_iter()
            .fold(f32::NEG_INFINITY, f32::max);
        let terrestrial = terrain.eroded_elevation > crate::chunk::SEA_LEVEL as f32;
        let below_tree_line = terrain.eroded_elevation <= f32::from(biome.tree_line_y) + 6.0
            && !(terrain.eroded_elevation > 112.0 && slope(pos) > 17.0);
        if terrestrial
            && below_tree_line
            && climate.mean_temperature >= 21.0
            && climate.mean_precipitation >= 1_550.0
            && climate.aridity < 0.82
            && climate.precipitation_seasonality < 0.75
        {
            tropical_wet += 1;
            assert_eq!(biome.baseline_biome, BIOME_JUNGLE);
        }
        if terrestrial
            && below_tree_line
            && climate.mean_temperature >= 13.0
            && (climate.aridity >= 1.65 || climate.mean_precipitation < 310.0)
        {
            hot_arid += 1;
            assert_eq!(biome.baseline_biome, BIOME_DESERT);
        }
        if terrestrial
            && below_tree_line
            && (6.5..18.0).contains(&climate.mean_temperature)
            && climate.mean_precipitation >= 820.0
            && climate.aridity < 1.0
        {
            temperate_moist += 1;
            assert_eq!(biome.baseline_biome, BIOME_FOREST);
        }
        if terrestrial
            && below_tree_line
            && (-5.0..6.5).contains(&climate.mean_temperature)
            && warmest >= 8.0
        {
            taiga += 1;
            assert_eq!(biome.baseline_biome, BIOME_TAIGA);
        }
        if terrestrial && below_tree_line && (0.0..8.0).contains(&warmest) {
            tundra += 1;
            assert_eq!(biome.baseline_biome, BIOME_TUNDRA);
        }
        if terrestrial && below_tree_line && warmest < 0.0 {
            arctic += 1;
            assert_eq!(biome.baseline_biome, BIOME_ARCTIC);
        }
        if latitude.abs().to_degrees() < 20.0 {
            low_latitude_tree_lines.push(biome.tree_line_y);
        } else if latitude.abs().to_degrees() > 60.0 {
            high_latitude_tree_lines.push(biome.tree_line_y);
        }
        if terrestrial && biome.baseline_biome == BIOME_MOUNTAINS {
            alpine += 1;
            assert!(
                terrain.eroded_elevation > f32::from(biome.tree_line_y) + 6.0
                    || (terrain.eroded_elevation > 112.0 && slope(pos) > 17.0),
                "mountain classification follows treeline or steep high relief"
            );
        }

        let arid = climate.aridity >= 1.02 || climate.mean_precipitation < 620.0;
        if arid && biome.habitat_flags & HABITAT_RIPARIAN != 0 {
            arid_riparian.push(biome.vegetation_potential);
            assert!(
                hydro.flags & HYDRO_RIVER != 0,
                "riparian habitat at {pos:?} has no physical river channel"
            );
        } else if arid
            && terrain.eroded_elevation > crate::chunk::SEA_LEVEL as f32
            && biome.habitat_flags & HABITAT_AQUATIC_FRESH == 0
        {
            arid_dry.push(biome.vegetation_potential);
        }
        if biome.habitat_flags & HABITAT_WETLAND != 0 {
            assert!(
                hydro.flags & HYDRO_WETLAND != 0
                    || (ground.baseline_groundwater_head >= terrain.eroded_elevation - 2.0
                        && ground.soil_salinity < 64
                        && ground.drainage < 92),
                "wetland at {pos:?} has neither saturated hydrology nor a shallow, poorly drained water table"
            );
        }
        assert_eq!(
            biome.habitat_flags & HABITAT_ESTUARY_DELTA != 0,
            hydro.flags & (HYDRO_DELTA | HYDRO_ESTUARY) != 0,
            "delta/estuary overlay is owned by hydrology"
        );
        deltas += usize::from(hydro.flags & HYDRO_DELTA != 0);
        estuaries += usize::from(hydro.flags & HYDRO_ESTUARY != 0);

        for direction in [Direction4::East, Direction4::North] {
            let step = pos.step(direction, side);
            let other = atlas.genesis.biomes.get(step.pos).unwrap();
            if biome.baseline_biome != crate::planet_atlas::BIOME_OCEAN
                && other.baseline_biome != crate::planet_atlas::BIOME_OCEAN
            {
                comparable += 1;
                coherent += usize::from(biome.baseline_biome == other.baseline_biome);
            }
            if step.pos.face != pos.face
                && biome.habitat_flags & HABITAT_RIPARIAN != 0
                && other.habitat_flags & HABITAT_RIPARIAN != 0
            {
                seam_river_corridors += 1;
            }
        }
    }

    for (name, count) in [
        ("tropical wet", tropical_wet),
        ("hot arid", hot_arid),
        ("temperate moist", temperate_moist),
        ("taiga-capable cold", taiga),
        ("tundra", tundra),
        ("arctic", arctic),
        ("alpine", alpine),
    ] {
        assert!(count > 0, "qualification fixture contains {name} cells");
    }
    assert!(
        coherent as f64 / comparable as f64 > 0.70,
        "neighboring terrestrial cells form broad zones ({coherent}/{comparable})"
    );
    assert!(!arid_riparian.is_empty() && !arid_dry.is_empty());
    let mean = |values: &[u8]| {
        values.iter().map(|value| u64::from(*value)).sum::<u64>() as f64 / values.len() as f64
    };
    assert!(
        mean(&high_latitude_tree_lines) < mean(&low_latitude_tree_lines),
        "treeline descends toward the poles ({:.1} versus {:.1})",
        mean(&high_latitude_tree_lines),
        mean(&low_latitude_tree_lines)
    );
    assert!(
        mean(&arid_riparian) > mean(&arid_dry),
        "river water greens arid country ({:.1} versus {:.1})",
        mean(&arid_riparian),
        mean(&arid_dry)
    );
    assert!(
        seam_river_corridors > 0,
        "at least one riparian corridor continues through a cube-face seam"
    );
    assert!(
        deltas > 0 && estuaries > 0,
        "fixture contains both depositional deltas and tidal estuaries"
    );
    assert!(
        atlas
            .genesis
            .biomes
            .values()
            .iter()
            .any(|cell| matches!(cell.baseline_biome, BIOME_SCRUBLAND | BIOME_BADLANDS))
    );
}

#[test]
fn saline_coastal_habitat_materializes_tolerant_cover_without_trees() {
    use crate::planet_atlas::HABITAT_SALT_MARSH;

    let reg = base_reg();
    let mut atlas = PlanetAtlas::fixture(8_705, 64).unwrap();
    let site = atlas
        .genesis
        .terrain
        .iter()
        .find(|(pos, terrain)| {
            terrain.eroded_elevation > crate::chunk::SEA_LEVEL as f32 + 2.0
                && terrain.eroded_elevation < crate::chunk::SEA_LEVEL as f32 + 24.0
                && atlas.genesis.biomes.get(*pos).is_some_and(|biome| {
                    biome.habitat_flags
                        & (crate::planet_atlas::HABITAT_AQUATIC_FRESH
                            | crate::planet_atlas::HABITAT_AQUATIC_BRACKISH
                            | crate::planet_atlas::HABITAT_AQUATIC_SALT)
                        == 0
                })
        })
        .map(|(pos, _)| pos)
        .expect("fixture contains low coastal land");
    let biome = atlas.genesis.biomes.get_mut(site).unwrap();
    biome.baseline_biome = crate::planet_atlas::BIOME_PLAINS;
    biome.edaphic_flags = 0;
    biome.habitat_flags = HABITAT_SALT_MARSH;
    biome.vegetation_potential = 120;
    biome.tree_line_y = u8::MAX;
    let ground = atlas.genesis.ground.get_mut(site).unwrap();
    ground.soil_salinity = 180;
    ground.freeze_flags = 0;
    let atlas = Arc::new(atlas);
    let center = site.center(atlas.side());
    let surface = crate::planet::SurfacePos::new(
        center.face,
        center.u.floor() as u16,
        center.v.floor() as u16,
    )
    .unwrap();
    let generator = crate::worldgen::Generator::with_atlas(8_705, &reg, atlas);
    let chunk = generator.generate(ChunkPos::from_surface(surface), &reg);
    let mud = b(&reg, "base:mud");
    let halite = b(&reg, "base:halite");
    let grass = b(&reg, "base:grass");
    let log = b(&reg, "base:log");
    let mut tolerant_cover = 0usize;
    let mut trees = 0usize;
    for x in 0..crate::chunk::CHUNK_X {
        for z in 0..crate::chunk::CHUNK_Z {
            for y in 0..crate::chunk::CHUNK_Y {
                let block = chunk.get(x, y, z);
                tolerant_cover += usize::from(block == mud || block == halite || block == grass);
                trees += usize::from(block == log);
            }
        }
    }
    assert!(
        tolerant_cover > 0,
        "the saline shore has visible marsh cover"
    );
    assert_eq!(trees, 0, "severe salinity excludes ordinary trees");
}

#[test]
fn countries_cover_land_once_and_routes_touch_real_boundaries() {
    let atlas = PlanetAtlas::fixture(8_702, 32).unwrap();
    let side = atlas.side();
    let mut dense = vec![0u32; atlas.biomes.countries.len()];
    for index in 0..atlas.genesis.terrain.len() {
        let terrain = atlas.genesis.terrain.values()[index];
        let cell = atlas.genesis.biomes.values()[index];
        let terrestrial = terrain.eroded_elevation > crate::chunk::SEA_LEVEL as f32
            && cell.habitat_flags
                & (crate::planet_atlas::HABITAT_AQUATIC_FRESH
                    | crate::planet_atlas::HABITAT_AQUATIC_BRACKISH
                    | crate::planet_atlas::HABITAT_AQUATIC_SALT)
                == 0;
        assert!(!terrestrial || cell.country_id != 0);
        assert!(terrain.eroded_elevation > crate::chunk::SEA_LEVEL as f32 || cell.country_id == 0);
        assert_eq!(cell.country_id, cell.heart_assignment);
        if let Some(slot) = cell.country_id.checked_sub(1) {
            dense[usize::from(slot)] += 1;
        }
    }
    for country in &atlas.biomes.countries {
        assert_eq!(dense[usize::from(country.id - 1)], country.cell_count);
        let heart = atlas.genesis.biomes.get(country.heart_site).unwrap();
        assert_eq!(heart.country_id, country.id);
        assert_eq!(
            heart.habitat_flags
                & (crate::planet_atlas::HABITAT_AQUATIC_FRESH
                    | crate::planet_atlas::HABITAT_AQUATIC_BRACKISH
                    | crate::planet_atlas::HABITAT_AQUATIC_SALT),
            0,
            "heart sites stand on dry terrestrial cells"
        );
        for route in &country.routes {
            let reciprocal = atlas.country(route.neighbor_id).unwrap();
            assert!(
                reciprocal
                    .routes
                    .iter()
                    .any(|back| back.neighbor_id == country.id)
            );
            let pass_owner = atlas.genesis.biomes.get(route.pass).unwrap().country_id;
            assert!([country.id, route.neighbor_id].contains(&pass_owner));
            assert!(route.pass.neighbors4(side).into_iter().any(|neighbor| {
                let owner = atlas.genesis.biomes.get(neighbor).unwrap().country_id;
                [country.id, route.neighbor_id].contains(&owner) && owner != pass_owner
            }));
        }
    }
}

#[test]
fn geographic_boundaries_follow_barriers_without_a_cube_seam_bias() {
    use std::collections::{BTreeMap, VecDeque};

    use crate::planet_atlas::HYDRO_RIVER;

    let atlas = PlanetAtlas::fixture(8_707, 64).unwrap();
    let side = atlas.side();
    let barrier = |from: AtlasPos, to: AtlasPos| {
        let ai = from.index(side);
        let bi = to.index(side);
        let a = atlas.genesis.terrain.values()[ai];
        let b = atlas.genesis.terrain.values()[bi];
        let ah = atlas.genesis.hydrology.values()[ai];
        let bh = atlas.genesis.hydrology.values()[bi];
        let watershed =
            if ah.watershed_id != 0 && bh.watershed_id != 0 && ah.watershed_id != bh.watershed_id {
                92.0
            } else {
                0.0
            };
        let crest = (a.eroded_elevation - b.eroded_elevation).abs() * 0.9
            + (f32::from(atlas.genesis.ground.values()[ai].erosion_susceptibility)
                + f32::from(atlas.genesis.ground.values()[bi].erosion_susceptibility))
                * 0.045;
        let river = if (ah.flags | bh.flags) & HYDRO_RIVER != 0
            && (ah.stream_order.max(bh.stream_order) >= 2
                || ah.mean_discharge.max(bh.mean_discharge) > 45.0)
        {
            36.0
        } else {
            0.0
        };
        watershed + crest + river
    };
    let mut boundary_score = 0.0f64;
    let mut boundary_edges = 0usize;
    let mut interior_score = 0.0f64;
    let mut interior_edges = 0usize;
    let mut seam_edges = 0usize;
    let mut seam_boundaries = 0usize;
    let mut ordinary_edges = 0usize;
    let mut ordinary_boundaries = 0usize;
    let mut seam_divides = 0usize;
    let mut ordinary_divides = 0usize;
    let mut seam_landmass_splits = 0usize;
    let mut ordinary_landmass_splits = 0usize;
    let mut seam_distance = 0.0f64;
    let mut ordinary_distance = 0.0f64;

    for (pos, cell) in atlas.genesis.biomes.iter() {
        if cell.country_id == 0 {
            continue;
        }
        for direction in [Direction4::East, Direction4::North] {
            let step = pos.step(direction, side);
            let other = atlas.genesis.biomes.get(step.pos).unwrap();
            if other.country_id == 0 {
                continue;
            }
            let boundary = other.country_id != cell.country_id;
            let score = f64::from(barrier(pos, step.pos));
            if boundary {
                boundary_edges += 1;
                boundary_score += score;
            } else {
                interior_edges += 1;
                interior_score += score;
            }
            if step.pos.face != pos.face {
                seam_edges += 1;
                seam_distance +=
                    crate::planet::geodesic_distance(pos.center(side), step.pos.center(side));
                seam_boundaries += usize::from(boundary);
                seam_divides += usize::from(
                    atlas.genesis.hydrology.get(pos).unwrap().watershed_id
                        != atlas.genesis.hydrology.get(step.pos).unwrap().watershed_id,
                );
                seam_landmass_splits += usize::from(
                    atlas.genesis.terrain.get(pos).unwrap().landmass_id
                        != atlas.genesis.terrain.get(step.pos).unwrap().landmass_id,
                );
            } else if match direction {
                Direction4::East => pos.u <= 1 || pos.u >= side - 3,
                Direction4::North => pos.v <= 1 || pos.v >= side - 3,
                _ => false,
            } {
                // Compare the seam to the in-face edges immediately beside
                // it. Cube-map cells have different physical edge lengths at
                // face centres and corners; using the whole face as the
                // control would mistake that real geometry for seam cost.
                ordinary_edges += 1;
                ordinary_distance +=
                    crate::planet::geodesic_distance(pos.center(side), step.pos.center(side));
                ordinary_boundaries += usize::from(boundary);
                ordinary_divides += usize::from(
                    atlas.genesis.hydrology.get(pos).unwrap().watershed_id
                        != atlas.genesis.hydrology.get(step.pos).unwrap().watershed_id,
                );
                ordinary_landmass_splits += usize::from(
                    atlas.genesis.terrain.get(pos).unwrap().landmass_id
                        != atlas.genesis.terrain.get(step.pos).unwrap().landmass_id,
                );
            }
        }
    }
    let boundary_mean = boundary_score / boundary_edges as f64;
    let interior_mean = interior_score / interior_edges as f64;
    assert!(
        boundary_mean > interior_mean,
        "country edges prefer divides, crests, and major rivers ({boundary_mean:.1} versus {interior_mean:.1})"
    );
    let seam_rate = seam_boundaries as f64 / seam_edges as f64;
    let ordinary_rate = ordinary_boundaries as f64 / ordinary_edges as f64;
    let mut aggregate = (
        seam_boundaries,
        seam_edges,
        ordinary_boundaries,
        ordinary_edges,
    );
    for seed in [8_708, 8_709, 8_710] {
        let sample = PlanetAtlas::fixture(seed, 64).unwrap();
        for (pos, cell) in sample.genesis.biomes.iter() {
            if cell.country_id == 0 {
                continue;
            }
            for direction in [Direction4::East, Direction4::North] {
                let step = pos.step(direction, sample.side());
                let other = sample.genesis.biomes.get(step.pos).unwrap();
                if other.country_id == 0 {
                    continue;
                }
                let boundary = cell.country_id != other.country_id;
                if step.pos.face != pos.face {
                    aggregate.0 += usize::from(boundary);
                    aggregate.1 += 1;
                } else if match direction {
                    Direction4::East => pos.u <= 1 || pos.u >= sample.side() - 3,
                    Direction4::North => pos.v <= 1 || pos.v >= sample.side() - 3,
                    _ => false,
                } {
                    aggregate.2 += usize::from(boundary);
                    aggregate.3 += 1;
                }
            }
        }
    }
    let aggregate_seam_rate = aggregate.0 as f64 / aggregate.1 as f64;
    let aggregate_control_rate = aggregate.2 as f64 / aggregate.3 as f64;
    assert!(
        aggregate_seam_rate <= aggregate_control_rate * 1.75 + 0.015,
        "cube seams do not statistically attract borders ({aggregate_seam_rate:.3} versus {aggregate_control_rate:.3}; example {seam_rate:.3} versus {ordinary_rate:.3}; watershed divides {:.3} versus {:.3}; landmass splits {:.3} versus {:.3}; step distance {:.1} versus {:.1}; example n={seam_edges}/{ordinary_edges})",
        seam_divides as f64 / seam_edges as f64,
        ordinary_divides as f64 / ordinary_edges as f64,
        seam_landmass_splits as f64 / seam_edges as f64,
        ordinary_landmass_splits as f64 / ordinary_edges as f64,
        seam_distance / seam_edges as f64,
        ordinary_distance / ordinary_edges as f64,
    );

    // A country's cells must form one traversable component and never jump
    // between islands. This is stronger than merely counting every cell.
    let mut cells = BTreeMap::<u16, Vec<AtlasPos>>::new();
    for (pos, cell) in atlas.genesis.biomes.iter() {
        if cell.country_id != 0 {
            cells.entry(cell.country_id).or_default().push(pos);
        }
    }
    for country in &atlas.biomes.countries {
        let owned = &cells[&country.id];
        let expected_landmass = atlas.genesis.terrain.get(owned[0]).unwrap().landmass_id;
        assert!(owned.iter().all(|pos| {
            atlas.genesis.terrain.get(*pos).unwrap().landmass_id == expected_landmass
        }));
        let mut seen = std::collections::BTreeSet::new();
        let mut queue = VecDeque::from([owned[0]]);
        seen.insert(owned[0]);
        while let Some(pos) = queue.pop_front() {
            for neighbor in pos.neighbors4(side) {
                if atlas.genesis.biomes.get(neighbor).unwrap().country_id == country.id
                    && seen.insert(neighbor)
                {
                    queue.push_back(neighbor);
                }
            }
        }
        assert_eq!(
            seen.len(),
            owned.len(),
            "country {} is one geographic component",
            country.id
        );
        assert_eq!(country.principal_landmass, expected_landmass);
        assert!(
            atlas
                .genesis
                .terrain
                .get(country.heart_site)
                .unwrap()
                .eroded_elevation
                > crate::chunk::SEA_LEVEL as f32
        );
        let heart_cell = atlas.genesis.biomes.get(country.heart_site).unwrap();
        if country.habitat_cells.get("wetland").copied().unwrap_or(0) > country.cell_count / 3 {
            assert_eq!(country.heart_form, crate::planet_atlas::BIOME_SWAMP);
        } else {
            assert_eq!(country.heart_form, country.dominant_biome);
        }
        assert_eq!(heart_cell.country_id, country.id);
    }
}

#[test]
fn every_atlas_country_materializes_its_registered_heart_and_edifice() {
    let reg = base_reg();
    let atlas = Arc::new(PlanetAtlas::fixture(8_711, 16).unwrap());
    let countries = atlas.biomes.countries.clone();
    let mut world = World::new_with_atlas(
        8_711,
        tmp_dir("atlas-country-hearts"),
        reg.clone(),
        atlas.clone(),
    );
    assert!(!countries.is_empty());
    for country in countries {
        let point = country.heart_site.center(atlas.side());
        let site = crate::planet::SurfacePos::new(
            point.face,
            point.u.floor() as u16,
            point.v.floor() as u16,
        )
        .unwrap();
        let expected_biome = crate::worldgen::Biome::from_index(country.heart_form).unwrap();
        assert_eq!(
            world.generator.province_at(site).biome,
            expected_biome,
            "country {} raises the form selected for its heart geography",
            country.id
        );
        let edifice = crate::edifice::edifice_of(expected_biome);
        let chunk_radius =
            (edifice.reach + crate::chunk::CHUNK_X as i32 - 1) / crate::chunk::CHUNK_X as i32 + 1;
        let home = crate::planet::ChunkPos::from_surface(site);
        for du in -chunk_radius..=chunk_radius {
            for dv in -chunk_radius..=chunk_radius {
                world.ensure_chunk(home.offset(du, dv));
            }
        }
        let heart = world.heart_at_surface(site).unwrap_or_else(|| {
            panic!(
                "country {} generated no reachable registered heart at {:?}",
                country.id, site
            )
        });
        let form = crate::world::heart_form(expected_biome);
        assert_eq!(
            reg.block(world.get_block_at(heart.pos)).name,
            crate::world::heart_block_name(form, heart.stage)
        );

        let shell = edifice.shell;
        let crown = edifice.crown;
        let mut monument_block = false;
        'columns: for du in -edifice.reach..=edifice.reach {
            for dv in -edifice.reach..=edifice.reach {
                let Ok(column) = crate::planet::SurfacePos::canonicalized(
                    site.face(),
                    i32::from(site.u()) + du,
                    i32::from(site.v()) + dv,
                ) else {
                    continue;
                };
                for y in 1..crate::chunk::CHUNK_Y as i32 {
                    let at = crate::planet::BlockPos::new(
                        column.face(),
                        column.u(),
                        y as u8,
                        column.v(),
                    )
                    .unwrap();
                    let name = &reg.block(world.get_block_at(at)).name;
                    if name == shell || name == crown {
                        monument_block = true;
                        break 'columns;
                    }
                }
            }
        }
        assert!(
            monument_block,
            "country {} materialized its {:?} edifice",
            country.id, edifice.family
        );
    }
}

#[test]
fn oasis_visibility_tracks_groundwater_drawdown_and_recovery() {
    let mut atlas = PlanetAtlas::fixture(8_703, 16).unwrap();
    let (pos, _) = atlas
        .genesis
        .terrain
        .iter()
        .find(|(_, terrain)| terrain.eroded_elevation > crate::chunk::SEA_LEVEL as f32)
        .expect("fixture contains land");
    atlas.genesis.biomes.get_mut(pos).unwrap().habitat_flags |=
        crate::planet_atlas::HABITAT_OASIS | crate::planet_atlas::HABITAT_SPRING;
    let baseline = atlas
        .genesis
        .ground
        .get(pos)
        .unwrap()
        .baseline_groundwater_head;
    atlas
        .water_cycle
        .cells
        .get_mut(pos)
        .unwrap()
        .groundwater_head_milliblocks = (baseline * 1_000.0).round() as i32;
    let center = pos.center(atlas.side());
    let surface = crate::planet::SurfacePos::new(
        center.face,
        center.u.floor() as u16,
        center.v.floor() as u16,
    )
    .unwrap();
    assert_ne!(
        atlas.biome_sample(surface).habitat_flags & crate::planet_atlas::HABITAT_OASIS,
        0
    );
    atlas
        .water_cycle
        .cells
        .get_mut(pos)
        .unwrap()
        .groundwater_head_milliblocks = (baseline * 1_000.0).round() as i32 - 1_501;
    assert_eq!(
        atlas.biome_sample(surface).habitat_flags & crate::planet_atlas::HABITAT_OASIS,
        0,
        "pumping below the local water table dries the oasis"
    );
    atlas
        .water_cycle
        .cells
        .get_mut(pos)
        .unwrap()
        .groundwater_head_milliblocks = (baseline * 1_000.0).round() as i32;
    assert_ne!(
        atlas.biome_sample(surface).habitat_flags & crate::planet_atlas::HABITAT_OASIS,
        0,
        "recharge restores the oasis predicate"
    );
}
