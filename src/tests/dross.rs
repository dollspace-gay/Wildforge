use super::*;

use crate::alchemy::{PreparationModifiers, PreparationPhysiology};
use crate::arcane::{
    ArcaneAuthority, ArcaneOwner, ArcaneTransaction, BASE_RESONANCES, DrossMedium,
};
use crate::dross::{DrossBand, DrossCellState};
use crate::planet::{Face, SurfacePos};
use crate::planet_atlas::AtlasPos;

fn dross_world(tag: &str, seed: u32) -> (World, std::path::PathBuf) {
    let dir = tmp_dir(tag);
    let atlas = std::sync::Arc::new(crate::planet_atlas::PlanetAtlas::fixture(seed, 16).unwrap());
    atlas.write_new(&dir).unwrap();
    (
        World::new_with_atlas(seed, dir.clone(), base_reg(), atlas),
        dir,
    )
}

fn surface_atlas_center(world: &World, region: AtlasPos) -> SurfacePos {
    let atlas = world.planet_atlas().unwrap();
    let center = region.center(atlas.side());
    SurfacePos::new(
        center.face,
        center
            .u
            .floor()
            .clamp(0.0, f64::from(crate::planet::FACE_BLOCKS - 1)) as u16,
        center
            .v
            .floor()
            .clamp(0.0, f64::from(crate::planet::FACE_BLOCKS - 1)) as u16,
    )
    .unwrap()
}

/// Re-label existing Geography Current inside one cell. This preserves the
/// parent ledger's exact owner and resonance totals while creating a bounded
/// environmental test burden.
fn seed_dense_soil_dross(world: &mut World, region: AtlasPos, requested: u64, band: DrossBand) {
    let geography = world.arcane_geography.as_mut().unwrap();
    geography
        .dynamic
        .dross_state
        .ensure_cells(geography.dynamic.cells.len());
    let index = region.index(geography.manifest.side);
    let mut remaining = requested;
    for slot in 0..6 {
        let ambient = u64::from(geography.dynamic.cells[index].ambient[slot]).min(remaining);
        geography.dynamic.cells[index].ambient[slot] -= ambient as u16;
        geography.dynamic.cells[index].dross[slot] += ambient as u16;
        remaining -= ambient;
        let deep = u64::from(geography.dynamic.cells[index].deep[slot]).min(remaining);
        geography.dynamic.cells[index].deep[slot] -= deep as u16;
        geography.dynamic.cells[index].dross[slot] += deep as u16;
        remaining -= deep;
        if remaining == 0 {
            break;
        }
    }
    assert_eq!(
        remaining, 0,
        "fixture cell lacked {requested} Current units"
    );
    geography.dynamic.dross_state.cells[index] = DrossCellState {
        band,
        band_since_step: geography.dynamic.dross_state.completed_steps,
        ..DrossCellState::default()
    };
}

fn complete_next_dross_hour(world: &mut World) {
    let next = world
        .arcane_geography
        .as_ref()
        .unwrap()
        .dynamic
        .dross_state
        .completed_steps
        .saturating_add(1);
    world
        .planetary_weather_for_test_mut()
        .unwrap()
        .completed_hours = next;
    while world
        .arcane_geography
        .as_ref()
        .unwrap()
        .dynamic
        .dross_state
        .completed_steps
        < next
    {
        world.tick_dross(31).unwrap();
    }
}

fn world_block_name(world: &World, pos: crate::planet::BlockPos) -> String {
    world.reg.block(world.get_block_at(pos)).name.clone()
}

mod exposure;
mod imports;
mod scar_sites;
