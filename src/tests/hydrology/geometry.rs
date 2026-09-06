//! Geometry scenarios.

use super::*;

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
