//! Persistence scenarios.

use super::*;

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
    let mut remote = ReplicaWorld::new(1_337, reg.clone(), 0.0);
    let remap: Vec<_> = (0..reg.blocks.len())
        .map(|index| crate::registry::BlockId(index as u16))
        .collect();
    remote.insert_remote_chunks([(chunk_pos, payload.as_slice())], &remap);
    assert_eq!(
        remote.chunk(chunk_pos).unwrap().hydrology_volumes(),
        expected_records
    );
    assert_eq!(
        (1..CHUNK_Y)
            .map(|y| remote.chunk(chunk_pos).unwrap().water_salt(8, y, 8))
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
