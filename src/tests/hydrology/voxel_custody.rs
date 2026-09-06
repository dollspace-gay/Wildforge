//! Voxel custody scenarios.

use super::*;

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
