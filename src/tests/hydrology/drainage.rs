//! Drainage scenarios.

use super::*;

#[test]
fn complete_drainage_graph_is_adjacent_acyclic_and_reaches_declared_sinks() {
    let atlas = atlas();
    let side = atlas.side();
    let cells = atlas.genesis.hydrology.values();
    let mut seam_edges = BTreeSet::new();
    for (index, cell) in cells.iter().enumerate() {
        if cell.drainage_receiver == u32::MAX {
            assert!(
                cell.ocean_basin_id != 0
                    || (cell.lake_basin_id != 0 && cell.flags & HYDRO_LAKE != 0),
                "undeclared sink at {index}"
            );
            continue;
        }
        let pos = crate::planet_atlas::AtlasPos::from_index(index, side).unwrap();
        let next = crate::planet_atlas::AtlasPos::from_index(cell.drainage_receiver as usize, side)
            .unwrap();
        assert!(pos.neighbors8(side).contains(&next), "non-neighbor edge");
        if pos.face != next.face {
            seam_edges.insert((pos.face, next.face));
        }
    }
    let undirected_seams: BTreeSet<_> = seam_edges
        .iter()
        .map(|(a, b)| if a < b { (*a, *b) } else { (*b, *a) })
        .collect();
    assert_eq!(undirected_seams.len(), 12, "drainage uses every cube seam");

    for face in crate::planet::Face::ALL {
        for (u, v) in [(0, 0), (0, side - 1), (side - 1, 0), (side - 1, side - 1)] {
            let corner = crate::planet_atlas::AtlasPos { face, u, v };
            let neighbors: BTreeSet<_> = corner.neighbors8(side).into_iter().collect();
            assert_eq!(
                neighbors.len(),
                7,
                "cube vertices have seven unique Moore neighbors at {face:?} ({u}, {v}): {neighbors:?}"
            );
            let faces: BTreeSet<_> = neighbors.iter().map(|neighbor| neighbor.face).collect();
            assert!(faces.len() >= 3, "corner reaches both adjoining charts");
        }
    }

    let mut state = vec![0u8; cells.len()];
    for start in 0..cells.len() {
        if state[start] == 2 {
            continue;
        }
        let mut path = Vec::new();
        let mut at = start;
        loop {
            if state[at] == 2 {
                break;
            }
            assert_ne!(state[at], 1, "undeclared cycle through {at}");
            state[at] = 1;
            path.push(at);
            let next = cells[at].drainage_receiver;
            if next == u32::MAX {
                break;
            }
            at = next as usize;
        }
        for index in path {
            state[index] = 2;
        }
    }
}

#[test]
fn floodplain_channels_meander_but_keep_declared_endpoints() {
    let atlas = atlas();
    let side = atlas.side();
    let (index, cell) = atlas
        .genesis
        .hydrology
        .values()
        .iter()
        .enumerate()
        .find(|(index, cell)| {
            if cell.flags & crate::planet_atlas::HYDRO_FLOODPLAIN == 0
                || cell.flags & HYDRO_RIVER == 0
                || cell.drainage_receiver == u32::MAX
            {
                return false;
            }
            let here = crate::planet_atlas::AtlasPos::from_index(*index, side).unwrap();
            let receiver =
                crate::planet_atlas::AtlasPos::from_index(cell.drainage_receiver as usize, side)
                    .unwrap();
            here.face == receiver.face
        })
        .expect("fixture contains a same-face floodplain reach");
    let here = crate::planet_atlas::AtlasPos::from_index(index, side).unwrap();
    let receiver =
        crate::planet_atlas::AtlasPos::from_index(cell.drainage_receiver as usize, side).unwrap();
    let a = here.center(side);
    let b = receiver.center(side);
    let midpoint =
        crate::planet::SurfacePoint::new(a.face, (a.u + b.u) * 0.5, (a.v + b.v) * 0.5).unwrap();
    let middle = atlas.hydrology_sample(midpoint);
    assert!(middle.near_channel);
    assert!(
        middle.channel_distance_blocks > 0.25,
        "reach bows off its atlas chord"
    );
    assert!(atlas.hydrology_sample(a).channel_distance_blocks < 0.01);
    assert!(atlas.hydrology_sample(b).channel_distance_blocks < 0.01);
}

#[test]
fn runoff_accumulates_into_causal_river_geometry() {
    let atlas = atlas();
    let cells = atlas.genesis.hydrology.values();
    let mut river_cells: Vec<_> = cells
        .iter()
        .filter(|cell| cell.flags & HYDRO_RIVER != 0)
        .collect();
    for cell in cells {
        assert_eq!(
            cell.seasonal_runoff_fraction
                .iter()
                .map(|value| u32::from(*value))
                .sum::<u32>(),
            65_535
        );
        assert_eq!(
            cell.seasonal_discharge_fraction
                .iter()
                .map(|value| u32::from(*value))
                .sum::<u32>(),
            65_535
        );
    }
    assert!(
        river_cells.len() > 100,
        "a network, not isolated blue marks"
    );
    for cell in &river_cells {
        let receiver = cell.drainage_receiver;
        if receiver != u32::MAX {
            assert!(
                cells[receiver as usize].mean_discharge + 0.001 >= cell.mean_discharge,
                "tributary discharge cannot vanish downstream"
            );
            if cells[receiver as usize].flags & HYDRO_RIVER != 0 {
                assert!(
                    cell.channel_bed_elevation + 0.001
                        >= cells[receiver as usize].channel_bed_elevation,
                    "channel bed climbed downstream"
                );
                if cells[receiver as usize].flags & HYDRO_ESTUARY == 0 {
                    assert!(
                        cells[receiver as usize].salinity >= cell.salinity,
                        "dissolved load vanished downstream"
                    );
                }
            }
        }
    }
    assert!(
        river_cells
            .iter()
            .any(|cell| { cell.flags & HYDRO_INTERMITTENT != 0 && cell.baseline_water_units == 0 })
    );
    assert!(river_cells.iter().any(|cell| cell.baseline_water_units > 0));
    river_cells.sort_by(|a, b| a.mean_discharge.total_cmp(&b.mean_discharge));
    let quartile = (river_cells.len() / 4).max(1);
    let narrow = average(
        river_cells[..quartile]
            .iter()
            .map(|cell| f64::from(cell.channel_width_centiblocks)),
    );
    let wide = average(
        river_cells[river_cells.len() - quartile..]
            .iter()
            .map(|cell| f64::from(cell.channel_width_centiblocks)),
    );
    let shallow = average(
        river_cells[..quartile]
            .iter()
            .map(|cell| f64::from(cell.channel_depth_centiblocks)),
    );
    let deep = average(
        river_cells[river_cells.len() - quartile..]
            .iter()
            .map(|cell| f64::from(cell.channel_depth_centiblocks)),
    );
    assert!(wide > narrow * 1.25, "width grows with discharge");
    assert!(deep > shallow * 1.12, "depth grows with discharge");
}
