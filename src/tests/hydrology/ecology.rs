//! Ecology scenarios.

use super::*;

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
