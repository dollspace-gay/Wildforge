//! Basin-only queries agree with complete sampling, including mixed models.

use std::sync::OnceLock;

use super::HydrologyBasinIds;
use crate::planet::{Direction4, Face, SurfacePos};
use crate::planet_atlas::{AtlasPos, HYDRO_FLOODPLAIN, HYDRO_RIVER, PlanetAtlas, WaterBodyKind};

fn atlas() -> &'static PlanetAtlas {
    static ATLAS: OnceLock<PlanetAtlas> = OnceLock::new();
    ATLAS.get_or_init(|| PlanetAtlas::fixture(6_015, 8).unwrap())
}

fn land_atlas() -> PlanetAtlas {
    let mut result = atlas().clone();
    for cell in result.genesis.hydrology.values_mut() {
        cell.water_body = WaterBodyKind::Land;
        cell.flags = 0;
        cell.ocean_basin_id = 0;
        cell.lake_basin_id = 0;
        cell.drainage_receiver = u32::MAX;
    }
    result
}

fn ids(ocean_basin_id: u16, lake_basin_id: u32) -> HydrologyBasinIds {
    HydrologyBasinIds {
        ocean_basin_id,
        lake_basin_id,
    }
}

fn full_ids(atlas: &PlanetAtlas, surface: SurfacePos) -> HydrologyBasinIds {
    let sample = atlas.hydrology_sample(surface.center());
    ids(sample.ocean_basin_id, sample.lake_basin_id)
}

fn center(atlas: &PlanetAtlas, cell: AtlasPos) -> SurfacePos {
    let point = cell.center(atlas.side());
    SurfacePos::new(point.face, point.u.floor() as u16, point.v.floor() as u16).unwrap()
}

fn river(
    atlas: &mut PlanetAtlas,
    cell: AtlasPos,
    receiver: AtlasPos,
    basin_ids: HydrologyBasinIds,
) {
    let receiver_index = receiver.index(atlas.side()) as u32;
    let hydro = atlas.genesis.hydrology.get_mut(cell).unwrap();
    hydro.water_body = WaterBodyKind::River;
    hydro.flags = HYDRO_RIVER;
    hydro.drainage_receiver = receiver_index;
    hydro.ocean_basin_id = basin_ids.ocean_basin_id();
    hydro.lake_basin_id = basin_ids.lake_basin_id();
    hydro.channel_width_centiblocks = 60_000;
    hydro.mean_discharge = 10.0;
}

#[test]
fn generated_basin_ids_match_full_sampling_at_cell_chunk_and_face_boundaries() {
    let atlas = atlas();
    let cell_blocks = atlas.cell_blocks();
    let mut points = Vec::new();
    for (cell, _) in atlas.genesis.hydrology.iter() {
        let u = cell.u * cell_blocks;
        let v = cell.v * cell_blocks;
        for du in [0, cell_blocks / 2, cell_blocks - 1] {
            for dv in [0, cell_blocks / 2, cell_blocks - 1] {
                points.push(SurfacePos::new(cell.face, u + du, v + dv).unwrap());
            }
        }
        // Both sides of a 16-block chunk boundary within an atlas cell.
        for du in [15, 16] {
            points.push(SurfacePos::new(cell.face, u + du, v + cell_blocks / 2).unwrap());
        }
    }
    let mut query = atlas.hydrology_basin_query();
    for surface in points.iter().copied().chain(points.iter().rev().copied()) {
        assert_eq!(
            query.at(surface),
            full_ids(atlas, surface),
            "basin identity changed at {surface:?}"
        );
    }
}

#[test]
fn named_surface_basins_return_home_ids_even_beside_mixed_rivers() {
    for (body, expected) in [
        (WaterBodyKind::Ocean, ids(19, 0)),
        (WaterBodyKind::Lake, ids(0, 71)),
        (WaterBodyKind::Playa, ids(0, 81)),
        // Preserve both fields even when a model contains both identities.
        (WaterBodyKind::Ocean, ids(23, 91)),
    ] {
        let mut atlas = land_atlas();
        let home = AtlasPos::new(Face::PosX, 3, 3, atlas.side()).unwrap();
        let neighbor = home.step(Direction4::West, atlas.side()).pos;
        river(&mut atlas, neighbor, home, ids(99, 101));
        let cell = atlas.genesis.hydrology.get_mut(home).unwrap();
        cell.water_body = body;
        cell.ocean_basin_id = expected.ocean_basin_id();
        cell.lake_basin_id = expected.lake_basin_id();
        let mut query = atlas.hydrology_basin_query();
        for du in [0, atlas.cell_blocks() / 2, atlas.cell_blocks() - 1] {
            let surface = SurfacePos::new(
                home.face,
                home.u * atlas.cell_blocks() + du,
                home.v * atlas.cell_blocks() + atlas.cell_blocks() / 2,
            )
            .unwrap();
            assert_eq!(full_ids(&atlas, surface), expected);
            assert_eq!(query.at(surface), expected);
        }
    }
}

#[test]
fn non_river_home_ids_do_not_create_a_basin_in_the_full_sample() {
    let mut atlas = land_atlas();
    let home = AtlasPos::new(Face::PosX, 3, 3, atlas.side()).unwrap();
    let cell = atlas.genesis.hydrology.get_mut(home).unwrap();
    cell.ocean_basin_id = 19;
    cell.lake_basin_id = 71;
    let surface = center(&atlas, home);
    assert_eq!(full_ids(&atlas, surface), ids(0, 0));
    assert_eq!(atlas.hydrology_basin_query().at(surface), ids(0, 0));
}

#[test]
fn mixed_river_winners_vary_within_one_cached_home_cell() {
    let mut atlas = land_atlas();
    let home = AtlasPos::new(Face::PosX, 3, 3, atlas.side()).unwrap();
    let west = home.step(Direction4::West, atlas.side()).pos;
    let north = home.step(Direction4::North, atlas.side()).pos;
    river(&mut atlas, west, home, ids(7, 71));
    river(&mut atlas, north, home, ids(0, 0));
    let middle = center(&atlas, home);
    let west_point = SurfacePos::new(home.face, home.u * atlas.cell_blocks(), middle.v()).unwrap();
    let north_point = SurfacePos::new(
        home.face,
        middle.u(),
        (home.v + 1) * atlas.cell_blocks() - 1,
    )
    .unwrap();
    let cases = [(west_point, ids(7, 71)), (north_point, ids(0, 0))];
    let mut query = atlas.hydrology_basin_query();
    for (surface, expected) in cases.into_iter().chain(cases.into_iter().rev()) {
        assert_eq!(atlas.atlas_pos(surface), home);
        assert_eq!(full_ids(&atlas, surface), expected);
        assert_eq!(query.at(surface), expected);
    }
}

#[test]
fn mixed_data_keeps_threshold_discharge_and_meander_selection() {
    for width in [1, 80, 60_000] {
        for discharge in [0.0, 10.0, 1_000.0] {
            for meander in [false, true] {
                let mut atlas = land_atlas();
                let home = AtlasPos::new(Face::PosX, 3, 3, atlas.side()).unwrap();
                let west = home.step(Direction4::West, atlas.side()).pos;
                let north = home.step(Direction4::North, atlas.side()).pos;
                river(&mut atlas, west, home, ids(0, 71));
                river(&mut atlas, north, home, ids(19, 0));
                let west_cell = atlas.genesis.hydrology.get_mut(west).unwrap();
                west_cell.channel_width_centiblocks = width;
                west_cell.mean_discharge = discharge;
                if meander {
                    west_cell.flags |= HYDRO_FLOODPLAIN;
                }
                let mut query = atlas.hydrology_basin_query();
                for du in [0, 1, 255, 511, 512, 767, 1_022, 1_023] {
                    for dv in [0, 1, 255, 511, 512, 767, 1_022, 1_023] {
                        let surface = SurfacePos::new(
                            home.face,
                            home.u * atlas.cell_blocks() + du,
                            home.v * atlas.cell_blocks() + dv,
                        )
                        .unwrap();
                        assert_eq!(query.at(surface), full_ids(&atlas, surface));
                    }
                }
            }
        }
    }
}

#[test]
fn mixed_river_basin_ids_cross_all_face_edges_and_corners() {
    for face in Face::ALL {
        for (u, v, direction) in [
            (0, 3, Direction4::West),
            (7, 3, Direction4::East),
            (3, 0, Direction4::South),
            (3, 7, Direction4::North),
            (0, 0, Direction4::West),
            (0, 7, Direction4::North),
            (7, 0, Direction4::South),
            (7, 7, Direction4::East),
        ] {
            let mut atlas = land_atlas();
            let home = AtlasPos::new(face, u, v, atlas.side()).unwrap();
            let across = home.step(direction, atlas.side()).pos;
            assert_ne!(home.face, across.face);
            river(&mut atlas, home, across, ids(23, 107));
            let surface = center(&atlas, home);
            // The segment begins at this point, independently fixing its
            // winning basin identity on either side of the chart seam.
            assert_eq!(full_ids(&atlas, surface), ids(23, 107));
            let mut query = atlas.hydrology_basin_query();
            assert_eq!(query.at(surface), ids(23, 107));
            let across_surface = center(&atlas, across);
            assert_eq!(full_ids(&atlas, across_surface), ids(23, 107));
            assert_eq!(query.at(across_surface), ids(23, 107));
        }
    }
}

#[test]
fn a_new_query_observes_atlas_changes_after_the_previous_borrow_ends() {
    let mut atlas = land_atlas();
    let home = AtlasPos::new(Face::PosX, 3, 3, atlas.side()).unwrap();
    let surface = center(&atlas, home);
    assert_eq!(atlas.hydrology_basin_query().at(surface), ids(0, 0));
    let cell = atlas.genesis.hydrology.get_mut(home).unwrap();
    cell.water_body = WaterBodyKind::Lake;
    cell.lake_basin_id = 71;
    assert_eq!(full_ids(&atlas, surface), ids(0, 71));
    assert_eq!(atlas.hydrology_basin_query().at(surface), ids(0, 71));
}
