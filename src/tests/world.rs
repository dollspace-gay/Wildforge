//! World persistence, block mutation, ticks, fluids, lighting, and weather.

use super::*;
use crate::world::{ReplicaWorld, ReplicationTarget, TerrainRead};
use std::collections::HashMap;

fn ecology_world(tag: &str, seed: u32) -> World {
    let dir = tmp_dir(tag);
    let atlas = std::sync::Arc::new(crate::planet_atlas::PlanetAtlas::fixture(seed, 16).unwrap());
    atlas.write_new(&dir).unwrap();
    World::new_with_atlas(seed, dir, base_reg(), atlas)
}

fn materialize_ecology_site(world: &mut World, content: &str) -> crate::planet::BlockPos {
    let surface = world
        .arcane_geography
        .as_ref()
        .unwrap()
        .dynamic
        .ecology
        .sites
        .iter()
        .find(|site| site.content_id == content)
        .and_then(|site| site.surface())
        .unwrap_or_else(|| panic!("fixture has no {content} site"));
    world.ensure_chunk(ChunkPos::from_surface(surface));
    let geography = world.arcane_geography.as_mut().unwrap();
    let site = geography
        .dynamic
        .ecology
        .sites
        .iter_mut()
        .find(|site| site.content_id == content && site.surface() == Some(surface))
        .unwrap();
    site.stage = crate::arcane_ecology::EcologyStage::Mature;
    if site.materialized_y == 0 {
        site.materialized_y = 80;
    }
    let pos = site.block_pos().unwrap();
    let block = world.reg.block_id(content).unwrap();
    world.set_block_at(pos, block);
    pos
}

/// A deterministic offering-fixture planet: worlds created through a bare
/// `load_or_create` seed from the wall clock, which made these tests roll a
/// different planet every CI run.
fn charged_offering_world(name: &str, seed: u32) -> (Arc<Registry>, World) {
    let reg = base_reg();
    let root = tmp_dir(name).join("world");
    crate::world::create_world_fixture_atomic(
        &root,
        seed,
        "survival",
        8,
        &crate::planet_atlas::CancellationToken::default(),
        |_| {},
    )
    .unwrap();
    let world = World::load_or_create(root, reg.clone()).unwrap();
    (reg, world)
}

/// The first country heart whose reserve satisfies `keep`, as the surface of
/// its heart site, its country id, and its current total.
fn country_heart_holding(
    world: &World,
    keep: impl Fn(u64) -> bool,
) -> Option<(crate::planet::SurfacePos, u16, u64)> {
    let atlas = world.planet_atlas().unwrap();
    atlas.biomes.countries.iter().find_map(|candidate| {
        let site = candidate.heart_site.center(atlas.side());
        let surface =
            crate::planet::SurfacePos::new(site.face, site.u.floor() as u16, site.v.floor() as u16)
                .ok()?;
        let country = atlas.country_at(surface)?.id;
        let total = world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&crate::arcane::ArcaneOwner::Heart(country))?
            .current
            .total();
        keep(total).then_some((surface, country, total))
    })
}

/// A patch of forest floor: grass under a stand of leaves, with the
/// chunk left wild unless the caller says otherwise.
fn kindling(name: &str) -> (World, std::sync::Arc<Registry>, i32) {
    let reg = base_reg();
    let mut w = test_world_with(name, reg.clone());
    let grass = b(&reg, "base:grass");
    let leaves = b(&reg, "base:leaves");
    // One flat level for the whole patch, captured BEFORE anything is
    // built on it — leaves are solid, so surface_height stops meaning
    // "the ground" the moment a canopy goes up.
    let ground = w.surface_height(4, 4);
    let dirt = b(&reg, "base:dirt");
    for x in 0..8 {
        for z in 0..8 {
            // Flatten it. Natural terrain is not level, and a patch
            // built at one height across a slope leaves half the fuel
            // buried in rock and half hanging in the air.
            for y in (ground - 3)..ground {
                w.set_block(x, y, z, dirt);
            }
            for y in (ground + 1)..(ground + 6) {
                w.set_block(x, y, z, AIR);
            }
            w.set_block(x, ground, z, grass);
            w.set_block(x, ground + 2, z, leaves);
        }
    }
    // Wilderness: an edit marks a chunk touched, so undo that.
    w.player_touched.clear();
    (w, reg, ground)
}

fn burn(w: &mut World, rounds: usize) {
    let mut rng = 99u32;
    for _ in 0..rounds {
        w.tick_fire(512, &mut rng);
    }
}

mod block_edits;
mod calendar;
mod discovery;
mod ecology_custody;
mod ecology_harvest;
mod fire;
mod funded_offerings;
mod growth;
mod ire_gains_decay_tiers_and_persistence;
mod lava;
mod lighting;
mod performance;
mod ponds;
mod regional_accounting;
mod replication;
mod save_codecs;
mod save_lifecycle;
mod seasons;
mod spawn;
mod torch_needs_ground_and_pops_without_it;
mod voxel_shell;
mod water_balance;
mod water_residency;
mod water_seams;
mod water_weather;
mod world_menu;
