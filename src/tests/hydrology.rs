use std::collections::BTreeSet;
use std::sync::{Arc, OnceLock};

use super::*;
use crate::chunk::{CHUNK_X, CHUNK_Y, CHUNK_Z, ChunkPos, SEA_LEVEL};
use crate::planet::{FACE_BLOCKS, SurfacePos, geodesic_distance};
use crate::planet_atlas::{
    HYDRO_DELTA, HYDRO_ESTUARY, HYDRO_INTERMITTENT, HYDRO_LAKE, HYDRO_PERENNIAL, HYDRO_RIVER,
    LakeClass, PlanetAtlas, PlanetaryWeather, ReservoirMass, SurfaceReservoirKind,
    dynamic_water_total, surface_reservoir_id,
};

fn atlas() -> &'static Arc<PlanetAtlas> {
    static ATLAS: OnceLock<Arc<PlanetAtlas>> = OnceLock::new();
    ATLAS.get_or_init(|| Arc::new(PlanetAtlas::fixture(1_337, 64).unwrap()))
}

fn average(values: impl Iterator<Item = f64>) -> f64 {
    let values: Vec<_> = values.collect();
    values.iter().sum::<f64>() / values.len().max(1) as f64
}

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

#[test]
fn wet_country_has_denser_more_perennial_water_than_arid_country() {
    let atlas = atlas();
    let mut wet = (0u64, 0u64, 0u64);
    let mut dry = (0u64, 0u64, 0u64);
    for index in 0..atlas.genesis.hydrology.len() {
        let terrain = atlas.genesis.terrain.values()[index];
        if terrain.eroded_elevation <= SEA_LEVEL as f32 {
            continue;
        }
        let climate = atlas.genesis.climate.values()[index];
        let hydro = atlas.genesis.hydrology.values()[index];
        let bucket = if climate.aridity < 0.72 {
            &mut wet
        } else if climate.aridity > 1.35 {
            &mut dry
        } else {
            continue;
        };
        bucket.0 += 1;
        bucket.1 += u64::from(u8::from(hydro.flags & HYDRO_RIVER != 0));
        bucket.2 += u64::from(u8::from(hydro.flags & HYDRO_PERENNIAL != 0));
    }
    let wet_density = wet.1 as f64 / wet.0.max(1) as f64;
    let dry_density = dry.1 as f64 / dry.0.max(1) as f64;
    let wet_perennial = wet.2 as f64 / wet.1.max(1) as f64;
    let dry_perennial = dry.2 as f64 / dry.1.max(1) as f64;
    assert!(
        wet_density > dry_density,
        "wet={wet_density} dry={dry_density}"
    );
    assert!(
        wet_perennial > dry_perennial,
        "wet perennial={wet_perennial} dry={dry_perennial}"
    );
}

#[test]
fn mountain_rain_shadows_reduce_runoff_and_perennial_flow() {
    let atlas = atlas();
    let mut windward_runoff = Vec::new();
    let mut leeward_runoff = Vec::new();
    let mut windward_perennial = 0u64;
    let mut leeward_perennial = 0u64;
    for (pos, terrain) in atlas.genesis.terrain.iter() {
        if terrain.eroded_elevation <= SEA_LEVEL as f32 {
            continue;
        }
        let summit = atlas.climate_downstream(pos, 54.0);
        let lee = atlas.climate_downstream(summit, 54.0);
        let summit_elevation = atlas.genesis.terrain.get(summit).unwrap().eroded_elevation;
        let lee_elevation = atlas.genesis.terrain.get(lee).unwrap().eroded_elevation;
        if summit_elevation > terrain.eroded_elevation + 8.0
            && lee_elevation < summit_elevation - 5.0
        {
            let wet = atlas.genesis.hydrology.get(pos).unwrap();
            let dry = atlas.genesis.hydrology.get(lee).unwrap();
            windward_runoff.push(f64::from(wet.mean_runoff));
            leeward_runoff.push(f64::from(dry.mean_runoff));
            windward_perennial += u64::from(u8::from(wet.flags & HYDRO_PERENNIAL != 0));
            leeward_perennial += u64::from(u8::from(dry.flags & HYDRO_PERENNIAL != 0));
        }
    }
    assert!(windward_runoff.len() >= 12);
    let wet = average(windward_runoff.into_iter());
    let dry = average(leeward_runoff.into_iter());
    assert!(wet > dry * 1.08, "windward runoff={wet} leeward={dry}");
    assert!(windward_perennial >= leeward_perennial);
}

#[test]
fn oceans_lakes_sills_salinity_and_hypsometry_are_closed() {
    let atlas = atlas();
    let ocean_cells: u64 = atlas
        .hydrology
        .oceans
        .iter()
        .map(|ocean| u64::from(ocean.cell_count))
        .sum();
    let ocean_share = ocean_cells as f64 / atlas.genesis.hydrology.len() as f64;
    assert!((0.35..=0.80).contains(&ocean_share));
    let ocean_area: f64 = atlas.hydrology.oceans.iter().map(|ocean| ocean.area).sum();
    let dominant = atlas
        .hydrology
        .oceans
        .iter()
        .find(|ocean| ocean.id == atlas.hydrology.dominant_ocean_id)
        .unwrap();
    assert!(dominant.area / ocean_area >= 0.90);
    assert!(atlas.hydrology.oceans.iter().all(|ocean| {
        ocean.salinity >= 192
            && ocean
                .volume_elevation_curve
                .windows(2)
                .all(|pair| pair[1].volume_units >= pair[0].volume_units)
    }));
    assert!(atlas.hydrology.lakes.len() >= 3);
    assert!(
        atlas
            .hydrology
            .lakes
            .iter()
            .any(|lake| lake.outlet.is_some())
    );
    assert!(atlas.hydrology.lakes.iter().any(|lake| {
        lake.outlet.is_none() && (lake.salinity >= 64 || lake.class == LakeClass::SeasonalPlaya)
    }));
    for lake in &atlas.hydrology.lakes {
        assert!(lake.cell_count > 0 && lake.catchment_area > 0.0);
        assert!(!lake.volume_elevation_curve.is_empty());
        assert!(lake.volume_elevation_curve.windows(2).all(|pair| {
            pair[1].elevation >= pair[0].elevation && pair[1].volume_units >= pair[0].volume_units
        }));
        assert!(lake.surface_elevation <= lake.spill_elevation + 0.001);
        assert!(
            (lake.baseline_inflow - lake.baseline_evaporation - lake.baseline_outflow).abs()
                <= lake.baseline_inflow.max(1.0) * 1.0e-8
        );
        if let Some(outlet) = lake.outlet {
            assert!(lake.sink.neighbors8(atlas.side()).contains(&outlet) || lake.cell_count > 1);
        }
        match lake.class {
            LakeClass::ThroughFlowFresh => {
                assert!(lake.outlet.is_some() && lake.salinity < 64)
            }
            LakeClass::TerminalFresh => {
                assert!(lake.outlet.is_none() && lake.salinity < 64)
            }
            LakeClass::SalineTerminal => {
                assert!(lake.outlet.is_none() && lake.salinity >= 64)
            }
            LakeClass::SeasonalPlaya => {
                assert!(lake.outlet.is_none() && lake.baseline_volume_units == 0)
            }
            LakeClass::Rift | LakeClass::VolcanicCrater | LakeClass::GlacialAlpine => {}
        }
    }
    assert!(atlas.hydrology.lakes.iter().any(|lake| {
        lake.outlet.is_none()
            && matches!(
                lake.class,
                LakeClass::SalineTerminal | LakeClass::SeasonalPlaya
            )
            && atlas
                .genesis
                .climate
                .get(lake.sink)
                .is_some_and(|climate| climate.aridity > 1.0)
    }));
}

#[test]
fn terminal_lake_concentration_precipitates_salt_without_losing_it() {
    let atlas = atlas();
    let lake = atlas
        .hydrology
        .lakes
        .iter()
        .find(|lake| {
            matches!(
                lake.class,
                LakeClass::TerminalFresh | LakeClass::SalineTerminal | LakeClass::SeasonalPlaya
            )
        })
        .expect("the qualification atlas has a terminal lake");
    let id = surface_reservoir_id(SurfaceReservoirKind::Lake, lake.id);
    let mut weather = PlanetaryWeather::new(atlas.dynamic.clone(), atlas.water_cycle.clone());
    if weather.water.reservoir_mut(id).unwrap().coarse.water_hu == 0 {
        let source = weather
            .water
            .reservoirs
            .iter()
            .find(|reservoir| reservoir.id != id && reservoir.coarse.water_hu >= 4_096)
            .unwrap()
            .id;
        assert_eq!(
            weather.breach_surface_reservoir(source, id, 4_096).water_hu,
            4_096
        );
    }
    let reservoir = weather.water.reservoir_mut(id).unwrap();
    let saturated = reservoir.coarse.water_hu.saturating_mul(250);
    let added = saturated.saturating_sub(reservoir.coarse.salt_mass);
    reservoir.coarse.salt_mass = saturated;
    weather.water.ledger.initial_salt_mass =
        weather.water.ledger.initial_salt_mass.saturating_add(added);
    let before = weather.water.audit(ReservoirMass::fresh(
        dynamic_water_total(&weather.cells) as u64
    ));
    weather.complete_hour(atlas, 20.0, 0, 0.0).unwrap();
    let after = weather.water.audit(ReservoirMass::fresh(
        dynamic_water_total(&weather.cells) as u64
    ));
    assert!(
        after.precipitated_salt_mass > before.precipitated_salt_mass,
        "supersaturated terminal water leaves an audited salt precipitate"
    );
    assert_eq!(after.current_salt_mass, before.current_salt_mass);
    assert_eq!(after.unexplained_salt_delta, 0);
}

#[test]
fn deltas_estuaries_and_resistant_valleys_follow_energy_and_substrate() {
    let atlas = atlas();
    let hydro = atlas.genesis.hydrology.values();
    let mean_cell_area = atlas
        .genesis
        .geometry
        .values()
        .iter()
        .map(|cell| f64::from(cell.physical_area))
        .sum::<f64>()
        / atlas.genesis.geometry.len() as f64;
    assert!(hydro.iter().any(|cell| cell.flags & HYDRO_DELTA != 0));
    assert!(hydro.iter().any(|cell| cell.flags & HYDRO_ESTUARY != 0));
    assert!(
        hydro
            .iter()
            .any(|cell| { cell.flags & crate::planet_atlas::HYDRO_FLOODPLAIN != 0 })
    );
    assert!(
        hydro
            .iter()
            .any(|cell| cell.flags & crate::planet_atlas::HYDRO_WATERFALL != 0)
    );
    assert!(
        hydro
            .iter()
            .any(|cell| cell.flags & crate::planet_atlas::HYDRO_WETLAND != 0)
    );
    let mut soft = Vec::new();
    let mut hard = Vec::new();
    for (index, cell) in hydro.iter().copied().enumerate() {
        let equivalent_discharge = f64::from(cell.mean_discharge) / mean_cell_area;
        if cell.flags & HYDRO_RIVER == 0 || !(1.0..=20.0).contains(&equivalent_discharge) {
            continue;
        }
        match crate::planet_atlas::BedrockFamily::from_id(
            atlas.genesis.tectonics.values()[index].bedrock_family,
        ) {
            crate::planet_atlas::BedrockFamily::Shale
            | crate::planet_atlas::BedrockFamily::Sandstone => {
                soft.push(f64::from(cell.channel_width_centiblocks))
            }
            crate::planet_atlas::BedrockFamily::Quartzite
            | crate::planet_atlas::BedrockFamily::Ultramafic
            | crate::planet_atlas::BedrockFamily::Granite => {
                hard.push(f64::from(cell.channel_width_centiblocks))
            }
            _ => {}
        }
    }
    assert!(!soft.is_empty() && !hard.is_empty());
    assert!(average(soft.into_iter()) > average(hard.into_iter()));
}

#[test]
fn gold_and_monazite_placers_follow_sources_into_depositional_reaches() {
    let atlas = atlas();
    let placers: Vec<_> = atlas
        .geology
        .deposits
        .iter()
        .filter(|deposit| {
            matches!(
                deposit.mineral,
                crate::planet_atlas::MineralKind::Gold
                    | crate::planet_atlas::MineralKind::RareEarth
            ) && deposit.depth_min == SEA_LEVEL as u16
                && deposit.max_blocks_per_chunk == 2
        })
        .collect();
    assert!(
        !placers.is_empty(),
        "the fixture routes at least one placer"
    );
    assert!(
        placers
            .iter()
            .any(|deposit| { deposit.mineral == crate::planet_atlas::MineralKind::RareEarth })
    );
    for placer in placers {
        let source = atlas
            .geology
            .deposits
            .iter()
            .find(|source| source.id == placer.source_body_id && source.mineral == placer.mineral)
            .expect("placer retains its geological source deposit");
        let target = atlas.genesis.hydrology.get(placer.pos).unwrap();
        assert_eq!(
            source.tonnage_blocks + placer.tonnage_blocks,
            u64::from(source.max_blocks_per_chunk + 1)
                * u64::from(source.eligible_chunk_upper_bound),
            "placer tonnage is removed from, not added beside, its source"
        );
        assert!(
            target.flags
                & (crate::planet_atlas::HYDRO_FLOODPLAIN | crate::planet_atlas::HYDRO_DELTA)
                != 0
        );
        let mut at = source.pos;
        let mut reached = false;
        for _ in 0..256 {
            if at == placer.pos {
                reached = true;
                break;
            }
            let cell = atlas.genesis.hydrology.get(at).unwrap();
            let Some(next) = (cell.drainage_receiver != u32::MAX)
                .then(|| {
                    crate::planet_atlas::AtlasPos::from_index(
                        cell.drainage_receiver as usize,
                        atlas.side(),
                    )
                })
                .flatten()
            else {
                break;
            };
            at = next;
        }
        assert!(reached, "placer is downstream from its source");
    }
}

fn surface_at(pos: crate::planet_atlas::AtlasPos, side: u16) -> SurfacePos {
    let center = pos.center(side);
    SurfacePos::new(
        center.face,
        center.u.floor().clamp(0.0, f64::from(FACE_BLOCKS - 1)) as u16,
        center.v.floor().clamp(0.0, f64::from(FACE_BLOCKS - 1)) as u16,
    )
    .unwrap()
}

#[test]
fn voxel_water_carries_salinity_and_exact_chunk_residual_accounting() {
    let atlas = atlas().clone();
    let reg = base_reg();
    let generator = crate::worldgen::Generator::with_atlas(1_337, &reg, atlas.clone());
    let river = atlas
        .hydrology
        .rivers
        .iter()
        .max_by(|a, b| a.maximum_width_blocks.total_cmp(&b.maximum_width_blocks))
        .unwrap();
    let surface = surface_at(river.mouth, atlas.side());
    let pos = ChunkPos::from_surface(surface);
    let chunk = generator.generate(pos, &reg);
    assert!(!chunk.hydrology_volumes().is_empty());
    let actual_units = (0..CHUNK_X)
        .flat_map(|x| (0..CHUNK_Z).map(move |z| (x, z)))
        .flat_map(|(x, z)| (1..CHUNK_Y).map(move |y| (x, y, z)))
        .map(|(x, y, z)| {
            let block = chunk.get(x, y, z);
            reg.water_volume(block).map_or_else(
                || u64::from(reg.block(block).name == "base:ice") * 8,
                u64::from,
            )
        })
        .sum::<u64>() as i128
        * i128::from(crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL);
    let accounted = chunk
        .hydrology_volumes()
        .iter()
        .map(|record| i128::from(record.baseline_hu) - i128::from(record.residual_hu))
        .sum::<i128>();
    assert_eq!(accounted, actual_units);
    let mut saw_water = false;
    for x in 0..CHUNK_X {
        for z in 0..CHUNK_Z {
            for y in 1..CHUNK_Y {
                if reg.is_water(chunk.get(x, y, z)) {
                    saw_water = true;
                    assert_eq!(
                        chunk.meta(x, y, z),
                        atlas
                            .hydrology_sample(
                                SurfacePos::new(
                                    pos.face(),
                                    pos.u() * CHUNK_X as u16 + x as u16,
                                    pos.v() * CHUNK_Z as u16 + z as u16,
                                )
                                .unwrap()
                                .center(),
                            )
                            .salinity
                    );
                }
            }
        }
    }
    assert!(saw_water);
}

#[test]
fn compact_river_voxel_entitlements_fit_the_named_reservoir() {
    let atlas = atlas().clone();
    let reg = base_reg();
    let side = atlas.side();
    let cells = atlas.genesis.hydrology.values();
    let mut candidates = Vec::new();
    for river in &atlas.hydrology.rivers {
        let members = cells
            .iter()
            .enumerate()
            .filter(|(_, cell)| cell.river_id == river.id && cell.baseline_water_units > 0)
            .collect::<Vec<_>>();
        let Some(&(first_index, _)) = members.first() else {
            continue;
        };
        let face = crate::planet_atlas::AtlasPos::from_index(first_index, side)
            .unwrap()
            .face;
        let mut min_u = f64::INFINITY;
        let mut max_u = f64::NEG_INFINITY;
        let mut min_v = f64::INFINITY;
        let mut max_v = f64::NEG_INFINITY;
        let mut width = 0.0f32;
        let mut same_face = true;
        for &(index, cell) in &members {
            let here = crate::planet_atlas::AtlasPos::from_index(index, side).unwrap();
            let Some(receiver) =
                crate::planet_atlas::AtlasPos::from_index(cell.drainage_receiver as usize, side)
            else {
                same_face = false;
                break;
            };
            if here.face != face || receiver.face != face {
                same_face = false;
                break;
            }
            for point in [here.center(side), receiver.center(side)] {
                min_u = min_u.min(point.u);
                max_u = max_u.max(point.u);
                min_v = min_v.min(point.v);
                max_v = max_v.max(point.v);
            }
            width = width.max(f32::from(cell.channel_width_centiblocks) / 100.0);
        }
        if !same_face {
            continue;
        }
        let margin = f64::from(width * 2.2 + 28.0);
        min_u = (min_u - margin).max(0.0);
        max_u = (max_u + margin).min(f64::from(FACE_BLOCKS - 1));
        min_v = (min_v - margin).max(0.0);
        max_v = (max_v + margin).min(f64::from(FACE_BLOCKS - 1));
        let chunk_count = ((max_u as u16 / CHUNK_X as u16) - (min_u as u16 / CHUNK_X as u16) + 1)
            as usize
            * (((max_v as u16 / CHUNK_Z as u16) - (min_v as u16 / CHUNK_Z as u16) + 1) as usize);
        candidates.push((chunk_count, river.id, face, min_u, max_u, min_v, max_v));
    }
    candidates.sort_by_key(|candidate| candidate.0);
    let (chunk_count, river_id, face, min_u, max_u, min_v, max_v) = candidates
        .into_iter()
        .next()
        .expect("fixture contains a compact wet same-face river");
    assert!(chunk_count < 2_000, "fixture probe stays bounded");

    let generator = crate::worldgen::Generator::with_atlas(1_337, &reg, atlas.clone());
    let mut requested_hu = 0u64;
    for u in (min_u as u16 / CHUNK_X as u16)..=(max_u as u16 / CHUNK_X as u16) {
        for v in (min_v as u16 / CHUNK_Z as u16)..=(max_v as u16 / CHUNK_Z as u16) {
            let pos = ChunkPos::new(face, u, v).unwrap();
            let chunk = generator.generate(pos, &reg);
            requested_hu = requested_hu.saturating_add(
                chunk
                    .hydrology_volumes()
                    .iter()
                    .filter(|record| {
                        record.reservoir
                            == surface_reservoir_id(SurfaceReservoirKind::River, river_id)
                    })
                    .map(|record| {
                        (i128::from(record.baseline_hu) - i128::from(record.residual_hu))
                            .clamp(0, i128::from(u64::MAX)) as u64
                    })
                    .sum::<u64>(),
            );
        }
    }
    let reservoir_id = surface_reservoir_id(SurfaceReservoirKind::River, river_id);
    let available_hu = atlas
        .water_cycle
        .reservoirs
        .iter()
        .find(|reservoir| reservoir.id == reservoir_id)
        .expect("named river has a finite reservoir")
        .initial_total_hu;
    assert!(
        requested_hu <= available_hu,
        "river {river_id} materializes {requested_hu} HU across {chunk_count} chunks but owns only {available_hu} HU"
    );
}

#[test]
fn depleted_reservoir_never_materializes_unowned_voxel_water() {
    let atlas = atlas().clone();
    let reg = base_reg();
    let river = atlas
        .hydrology
        .rivers
        .iter()
        .max_by(|a, b| a.maximum_width_blocks.total_cmp(&b.maximum_width_blocks))
        .unwrap();
    let pos = ChunkPos::from_surface(surface_at(river.mouth, atlas.side()));
    let reservoir_id = surface_reservoir_id(SurfaceReservoirKind::River, river.id);
    let generated =
        crate::worldgen::Generator::with_atlas(1_337, &reg, atlas.clone()).generate(pos, &reg);
    let wanted_hu = generated
        .hydrology_volumes()
        .iter()
        .find(|record| record.reservoir == reservoir_id)
        .map(|record| i128::from(record.baseline_hu) - i128::from(record.residual_hu))
        .unwrap() as u64;
    assert!(wanted_hu >= 64);
    let available_hu = wanted_hu / 2 + 17;
    let funded_hu = available_hu / crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL
        * crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL;

    let mut world = World::new_with_atlas(
        1_337,
        tmp_dir("hydrology-depleted-materialization"),
        reg.clone(),
        atlas.clone(),
    );
    let reservoir = world
        .planetary_weather_for_test_mut()
        .unwrap()
        .water
        .reservoir_mut(reservoir_id)
        .unwrap();
    let salinity = reservoir.coarse.salinity();
    reservoir.coarse = ReservoirMass::with_salinity(available_hu, salinity);
    world.ensure_chunk(pos);

    let weather = world.planetary_weather_for_test().unwrap();
    let commitment = weather
        .water
        .commitments
        .iter()
        .find(|commitment| commitment.chunk == pos && commitment.reservoir == reservoir_id)
        .unwrap();
    assert_eq!(commitment.mass.water_hu, funded_hu);
    assert_eq!(
        weather
            .water
            .reservoirs
            .iter()
            .find(|reservoir| reservoir.id == reservoir_id)
            .unwrap()
            .coarse
            .water_hu,
        available_hu - funded_hu,
        "sub-visible remainder stays coarse"
    );

    let chunk = &world.chunks()[&pos];
    let mut represented_hu = 0u64;
    for x in 0..CHUNK_X {
        for z in 0..CHUNK_Z {
            let surface = SurfacePos::new(
                pos.face(),
                pos.u() * CHUNK_X as u16 + x as u16,
                pos.v() * CHUNK_Z as u16 + z as u16,
            )
            .unwrap();
            let sample = atlas.hydrology_sample(surface.center());
            if sample.river_id != river.id {
                continue;
            }
            for y in 1..CHUNK_Y {
                let block = chunk.get(x, y, z);
                let units = reg.water_volume(block).map_or_else(
                    || u64::from(reg.block(block).name == "base:ice") * 8,
                    u64::from,
                );
                represented_hu += units * crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL;
            }
        }
    }
    assert_eq!(represented_hu, commitment.mass.water_hu);
}

#[test]
#[ignore = "operator probe for a production save named by WILDFORGE_PROBE_WORLD"]
fn production_river_entitlement_probe() {
    let root = std::env::var_os("WILDFORGE_PROBE_WORLD")
        .map(std::path::PathBuf::from)
        .expect("set WILDFORGE_PROBE_WORLD to a production world directory");
    let river_id = std::env::var("WILDFORGE_PROBE_RIVER")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(641);
    let atlas = Arc::new(PlanetAtlas::load(&root).unwrap());
    let reservoir_id = surface_reservoir_id(SurfaceReservoirKind::River, river_id);
    let reservoir = atlas
        .water_cycle
        .reservoirs
        .iter()
        .find(|reservoir| reservoir.id == reservoir_id)
        .unwrap();
    let initial_total_hu = reservoir.initial_total_hu;
    let mut faces = std::collections::BTreeMap::new();
    let mut atlas_hu = 0u64;
    let mut members = 0usize;
    let mut bounds = None::<(crate::planet::Face, f64, f64, f64, f64, f32)>;
    for (index, cell) in atlas.genesis.hydrology.values().iter().enumerate() {
        if cell.river_id != river_id || cell.baseline_water_units == 0 {
            continue;
        }
        let pos = crate::planet_atlas::AtlasPos::from_index(index, atlas.side()).unwrap();
        let receiver = crate::planet_atlas::AtlasPos::from_index(
            cell.drainage_receiver as usize,
            atlas.side(),
        );
        eprintln!(
            "member={pos:?} receiver={receiver:?} width={} depth={} surface={} bed={} units={}",
            f32::from(cell.channel_width_centiblocks) / 100.0,
            f32::from(cell.channel_depth_centiblocks) / 100.0,
            cell.water_surface_elevation,
            cell.channel_bed_elevation,
            cell.baseline_water_units,
        );
        if let Some(receiver) = receiver
            && receiver.face == pos.face
        {
            let a = pos.center(atlas.side());
            let b = receiver.center(atlas.side());
            let width = f32::from(cell.channel_width_centiblocks) / 100.0;
            bounds = Some(bounds.map_or(
                (
                    pos.face,
                    a.u.min(b.u),
                    a.u.max(b.u),
                    a.v.min(b.v),
                    a.v.max(b.v),
                    width,
                ),
                |(face, min_u, max_u, min_v, max_v, old_width)| {
                    assert_eq!(face, pos.face, "probe river crosses a face");
                    (
                        face,
                        min_u.min(a.u).min(b.u),
                        max_u.max(a.u).max(b.u),
                        min_v.min(a.v).min(b.v),
                        max_v.max(a.v).max(b.v),
                        old_width.max(width),
                    )
                },
            ));
        }
        *faces.entry(pos.face).or_insert(0usize) += 1;
        atlas_hu = atlas_hu.saturating_add(
            cell.baseline_water_units
                .saturating_mul(crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL),
        );
        members += 1;
    }
    eprintln!(
        "river={river_id} members={members} faces={faces:?} atlas_hu={atlas_hu} initial={} coarse={} committed={}",
        reservoir.initial_total_hu, reservoir.coarse.water_hu, reservoir.committed.water_hu
    );
    assert_eq!(atlas_hu, reservoir.initial_total_hu);

    let (face, min_u, max_u, min_v, max_v, width) = bounds.unwrap();
    let margin = f64::from(width * 2.2 + 28.0);
    let min_u = (min_u - margin).max(0.0) as u16 / CHUNK_X as u16;
    let max_u = (max_u + margin).min(f64::from(FACE_BLOCKS - 1)) as u16 / CHUNK_X as u16;
    let min_v = (min_v - margin).max(0.0) as u16 / CHUNK_Z as u16;
    let max_v = (max_v + margin).min(f64::from(FACE_BLOCKS - 1)) as u16 / CHUNK_Z as u16;
    let reg = base_reg();
    let generator =
        crate::worldgen::Generator::with_atlas(atlas.manifest.seed, &reg, atlas.clone());
    let mut requested_hu = 0u64;
    for u in min_u..=max_u {
        for v in min_v..=max_v {
            let chunk = generator.generate(ChunkPos::new(face, u, v).unwrap(), &reg);
            requested_hu += chunk
                .hydrology_volumes()
                .iter()
                .filter(|record| record.reservoir == reservoir_id)
                .map(|record| {
                    (i128::from(record.baseline_hu) - i128::from(record.residual_hu)) as u64
                })
                .sum::<u64>();
        }
    }
    eprintln!(
        "raster chunks={} requested_hu={requested_hu} available_hu={}",
        usize::from(max_u - min_u + 1) * usize::from(max_v - min_v + 1),
        initial_total_hu,
    );
    assert!(requested_hu <= initial_total_hu);
}

#[test]
fn chunk_save_and_load_preserve_hydrology_residuals_and_exact_salt() {
    let atlas = atlas().clone();
    let reg = base_reg();
    let root = tmp_dir("hydrology-wfc7-roundtrip");
    atlas.write_new(&root).unwrap();
    let river = atlas
        .hydrology
        .rivers
        .iter()
        .max_by(|a, b| a.maximum_width_blocks.total_cmp(&b.maximum_width_blocks))
        .unwrap();
    let surface = surface_at(river.mouth, atlas.side());
    let chunk_pos = ChunkPos::from_surface(surface);
    let mut world = World::new_with_atlas(1_337, root.clone(), reg.clone(), atlas);
    world.ensure_chunk(chunk_pos);
    let expected_records = world.chunks()[&chunk_pos].hydrology_volumes().to_vec();
    let expected_meta: Vec<_> = (1..CHUNK_Y)
        .map(|y| world.chunks()[&chunk_pos].meta(8, y, 8))
        .collect();
    let expected_salt: Vec<_> = (1..CHUNK_Y)
        .map(|y| world.chunks()[&chunk_pos].water_salt(8, y, 8))
        .collect();
    assert!(!expected_records.is_empty());
    let payload = world.chunk_rle(chunk_pos).unwrap();
    let mut remote = World::new(1_337, tmp_dir("hydrology-wfc6-remote"), reg.clone());
    remote.set_remote(true);
    let remap: Vec<_> = (0..reg.blocks.len())
        .map(|index| crate::registry::BlockId(index as u16))
        .collect();
    remote.insert_remote_chunk(chunk_pos, &payload, &remap);
    assert_eq!(
        remote.chunks()[&chunk_pos].hydrology_volumes(),
        expected_records
    );
    assert_eq!(
        (1..CHUNK_Y)
            .map(|y| remote.chunks()[&chunk_pos].water_salt(8, y, 8))
            .collect::<Vec<_>>(),
        expected_salt
    );
    save_world(&mut world);
    drop(world);

    let mut loaded = World::load_or_create(root, reg).unwrap();
    loaded.ensure_chunk(chunk_pos);
    assert_eq!(
        loaded.chunks()[&chunk_pos].hydrology_volumes(),
        expected_records
    );
    assert_eq!(
        (1..CHUNK_Y)
            .map(|y| loaded.chunks()[&chunk_pos].meta(8, y, 8))
            .collect::<Vec<_>>(),
        expected_meta
    );
    assert_eq!(
        (1..CHUNK_Y)
            .map(|y| loaded.chunks()[&chunk_pos].water_salt(8, y, 8))
            .collect::<Vec<_>>(),
        expected_salt
    );
}

#[test]
fn aquatic_ecology_reads_live_depth_and_atlas_temperature_flow_and_salinity() {
    let atlas = atlas().clone();
    let reg = base_reg();
    let (river_index, _) = atlas
        .genesis
        .hydrology
        .values()
        .iter()
        .enumerate()
        .filter(|(_, cell)| cell.flags & HYDRO_RIVER != 0 && cell.channel_depth_centiblocks >= 200)
        .max_by_key(|(_, cell)| cell.channel_width_centiblocks)
        .expect("the fixture has a fish-sized major river");
    let river_cell = crate::planet_atlas::AtlasPos::from_index(river_index, atlas.side()).unwrap();
    let river = atlas
        .hydrology
        .rivers
        .iter()
        .find(|river| river.id == atlas.genesis.hydrology.values()[river_index].river_id)
        .expect("the channel belongs to a named river");
    assert!(river.maximum_width_blocks >= 2.0);
    let surface = surface_at(river_cell, atlas.side());
    let expected = atlas.hydrology_sample(surface.center());
    assert!(expected.discharge > 0.0);

    let mut world = World::new_with_atlas(
        1_337,
        tmp_dir("hydrology-aquatic-habitat"),
        reg.clone(),
        atlas,
    );
    world.ensure_chunk(ChunkPos::from_surface(surface));
    let habitat = world
        .aquatic_habitat_at(surface)
        .expect("the mapped river materializes as a water column");
    assert!(habitat.depth_blocks >= 2);
    assert!(habitat.temperature_c.is_finite());
    assert_eq!(habitat.salinity, expected.salinity);
    assert!((habitat.discharge - expected.discharge).abs() < 0.001);

    let trout = reg.animals[reg.animal_id("base:trout").unwrap()]
        .aquatic
        .unwrap();
    let cod = reg.animals[reg.animal_id("base:cod").unwrap()]
        .aquatic
        .unwrap();
    assert!(trout.discharge[0] > 0.0 && trout.salinity[1] < 64);
    assert!(cod.salinity[0] >= 64);
}

#[test]
fn river_geometry_is_shared_across_chunks_and_cube_faces() {
    let atlas = atlas();
    let crossing = atlas
        .hydrology
        .rivers
        .iter()
        .flat_map(|river| river.path.windows(2))
        .find(|edge| edge[0].face != edge[1].face)
        .expect("a named river crosses a cube-face seam");
    let a = surface_at(crossing[0], atlas.side());
    let b = surface_at(crossing[1], atlas.side());
    let sa = atlas.hydrology_sample(a.center());
    let sb = atlas.hydrology_sample(b.center());
    assert!(sa.near_channel && sb.near_channel);
    assert!(sa.water_surface_elevation.is_some() && sb.water_surface_elevation.is_some());
    assert!(sa.channel_bed_elevation + 0.001 >= sb.channel_bed_elevation);
    assert!(geodesic_distance(a.center(), b.center()) < 220.0);

    let reg = base_reg();
    let mut world = World::new_with_atlas(
        1_337,
        tmp_dir("hydrology-face-river"),
        reg.clone(),
        atlas.clone(),
    );
    world.ensure_chunk(ChunkPos::from_surface(a));
    world.ensure_chunk(ChunkPos::from_surface(b));
    let water_column = |world: &World, surface: SurfacePos| {
        let top = (1..CHUNK_Y)
            .rev()
            .find(|y| {
                crate::planet::BlockPos::new(surface.face(), surface.u(), *y as u8, surface.v())
                    .is_ok_and(|pos| reg.is_water(world.get_block_at(pos)))
            })
            .expect("river endpoint materializes as water");
        let depth = world.aquatic_habitat_at(surface).unwrap().depth_blocks as usize;
        (top, top + 1 - depth)
    };
    let (top_a, bed_a) = water_column(&world, a);
    let (top_b, bed_b) = water_column(&world, b);
    assert!(
        top_a >= top_b,
        "voxel water surface does not climb downstream"
    );
    assert!(
        bed_a >= bed_b,
        "voxel channel bed does not climb across a face seam"
    );
}

#[test]
fn chunk_order_does_not_change_channels_salinity_or_residuals() {
    let atlas = atlas().clone();
    let reg = base_reg();
    let generator = crate::worldgen::Generator::with_atlas(1_337, &reg, atlas.clone());
    let river = atlas.hydrology.rivers.first().unwrap();
    let center = ChunkPos::from_surface(surface_at(river.mouth, atlas.side()));
    let mouth_index = atlas
        .genesis
        .hydrology
        .values()
        .iter()
        .position(|cell| cell.flags & (HYDRO_DELTA | HYDRO_ESTUARY) != 0)
        .unwrap();
    let mouth = ChunkPos::from_surface(surface_at(
        crate::planet_atlas::AtlasPos::from_index(mouth_index, atlas.side()).unwrap(),
        atlas.side(),
    ));
    let lake = ChunkPos::from_surface(surface_at(
        atlas.hydrology.lakes.first().unwrap().sink,
        atlas.side(),
    ));
    let mut positions = vec![
        center,
        center.offset(1, 0),
        center.offset(0, 1),
        mouth,
        lake,
    ];
    positions.sort();
    positions.dedup();
    let forward: Vec<_> = positions
        .iter()
        .map(|pos| generator.generate(*pos, &reg))
        .collect();
    let reverse: Vec<_> = positions
        .iter()
        .rev()
        .map(|pos| generator.generate(*pos, &reg))
        .collect();
    for index in 0..positions.len() {
        let a = &forward[index];
        let b = &reverse[positions.len() - 1 - index];
        assert_eq!(a.raw(), b.raw());
        assert_eq!(a.hydrology_volumes(), b.hydrology_volumes());
        for x in 0..CHUNK_X {
            for z in 0..CHUNK_Z {
                for y in 0..CHUNK_Y {
                    assert_eq!(a.meta(x, y, z), b.meta(x, y, z));
                }
            }
        }
    }
}

#[test]
fn generated_atlas_river_banks_hold_their_finite_water_at_rest() {
    let atlas = atlas().clone();
    let reg = base_reg();
    let (index, _) = atlas
        .genesis
        .hydrology
        .values()
        .iter()
        .enumerate()
        .filter(|(_, cell)| cell.flags & HYDRO_RIVER != 0 && cell.channel_depth_centiblocks >= 200)
        .max_by_key(|(_, cell)| cell.channel_width_centiblocks)
        .expect("fixture contains a major river");
    let surface = surface_at(
        crate::planet_atlas::AtlasPos::from_index(index, atlas.side()).unwrap(),
        atlas.side(),
    );
    let center = ChunkPos::from_surface(surface);
    let mut world = World::new_with_atlas(1_337, tmp_dir("hydrology-resting-banks"), reg, atlas);
    for du in -1..=1 {
        for dv in -1..=1 {
            world.ensure_chunk(center.offset(du, dv));
        }
    }
    let before = total_water(&world);
    let mut quiet = false;
    for _ in 0..300 {
        if !world.tick_water(100_000) {
            quiet = true;
            break;
        }
    }
    assert!(
        quiet,
        "generated banks settle instead of continuously leaking"
    );
    assert_eq!(
        total_water(&world),
        before,
        "settling conserves finite water"
    );
}

#[test]
fn atlas_names_rivers_lakes_seas_and_watersheds_deterministically() {
    let atlas = atlas();
    for river in atlas.hydrology.rivers.iter().take(8) {
        let surface = surface_at(river.mouth, atlas.side());
        assert!(atlas.hydrological_name_at(surface).is_some());
    }
    assert!(
        atlas
            .hydrology
            .lakes
            .iter()
            .all(|lake| !lake.name.trim().is_empty())
    );
    assert!(
        atlas
            .hydrology
            .oceans
            .iter()
            .all(|ocean| !ocean.name.trim().is_empty())
    );
    assert!(
        atlas
            .hydrology
            .watersheds
            .iter()
            .all(|watershed| !watershed.name.trim().is_empty())
    );
}

/// Operator/visual qualification probe. Run serially to keep atlas creation
/// inside the documented memory envelope:
/// `WILDFORGE_ATLAS_OUTPUT=/tmp/wildforge-hydrology CARGO_BUILD_JOBS=1 cargo
/// test --locked --release --lib
/// tests::hydrology::export_production_hydrology_qualification -- --ignored
/// --exact --nocapture --test-threads=1`
#[test]
#[ignore]
fn export_production_hydrology_qualification() {
    let peak_rss_kib = || {
        std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|status| {
                status.lines().find_map(|line| {
                    line.strip_prefix("VmHWM:")?
                        .split_whitespace()
                        .next()?
                        .parse::<u64>()
                        .ok()
                })
            })
    };
    let output = std::env::var_os("WILDFORGE_ATLAS_OUTPUT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| tmp_dir("production-hydrology-qualification"));
    std::fs::create_dir_all(&output).unwrap();
    let started = std::time::Instant::now();
    let atlas = PlanetAtlas::generate(
        1_337,
        0,
        crate::planet_atlas::AtlasConfig {
            side: crate::planet_atlas::ATLAS_FACE_SIDE,
            mode: crate::planet_atlas::GenerationMode::Serial,
        },
        &crate::planet_atlas::CancellationToken::default(),
        |_| {},
    )
    .unwrap();
    let generated = started.elapsed();
    let generation_peak_rss_kib = peak_rss_kib();
    atlas.write_new(&output).unwrap();
    let report = crate::planet_atlas::export_diagnostics(&atlas, &output).unwrap();
    let export_peak_rss_kib = peak_rss_kib();
    println!(
        "output={} generation={generated:?} generation_peak_rss_kib={generation_peak_rss_kib:?} export_peak_rss_kib={export_peak_rss_kib:?} cells={} maps={} hydrology_bytes={} oceans={} lakes={} rivers={} watersheds={}",
        output.display(),
        report.cell_count,
        report.exported_maps.len(),
        report.hydrology_bytes,
        atlas.hydrology.oceans.len(),
        atlas.hydrology.lakes.len(),
        atlas.hydrology.rivers.len(),
        atlas.hydrology.watersheds.len()
    );
}
