//! Scar sites scenarios.

use super::*;

#[test]
fn scar_manifestation_preserves_structure_persists_once_and_excavates_exactly() {
    let (mut world, root) = dross_world("dross-scar-host-lifecycle", 0xd205_5201);
    let region = AtlasPos {
        face: Face::PosZ,
        u: 8,
        v: 8,
    };
    let surface = surface_atlas_center(&world, region);
    world.ensure_chunk(ChunkPos::from_surface(surface));
    let scar_pos = crate::planet::BlockPos::new(
        surface.face(),
        surface.u(),
        (world.surface_height_at(surface) + 1) as u8,
        surface.v(),
    )
    .unwrap();
    let structure = b(&world.reg, "base:planks");
    // Exact occupancy remains a second line of defense even when this fixture
    // deliberately leaves the chunk outside the authored/touched index.
    world.set_block_at(scar_pos, structure);
    world.set_block_meta_at(scar_pos, structure, 173);
    seed_dense_soil_dross(&mut world, region, 1_200, DrossBand::Seep);

    complete_next_dross_hour(&mut world);
    let site = world
        .arcane_geography
        .as_ref()
        .unwrap()
        .dynamic
        .dross_state
        .scars
        .values()
        .next()
        .cloned()
        .expect("seep produced one canonical persistent site");
    assert_eq!(site.region, region);
    assert!(!site.content_id.is_empty());
    assert_eq!(world.get_block_at(scar_pos), structure);
    assert_eq!(world.get_meta_at(scar_pos), 173);
    let manifestation = site
        .materialized_at
        .expect("bounded handler search found an empty supported cell");
    assert_ne!(manifestation, scar_pos);
    let scar_block = world
        .reg
        .resolve_dross_scar(&site.content_id, site.kind)
        .unwrap()
        .block;
    assert_eq!(world.get_block_at(manifestation), scar_block);
    assert_eq!(
        world
            .arcane_geography
            .as_ref()
            .unwrap()
            .dynamic
            .dross_state
            .materialized
            .get(&manifestation),
        Some(&site.id)
    );
    let scar_current = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .account(&ArcaneOwner::Scar(site.id))
        .unwrap()
        .current
        .clone();
    assert!(scar_current.total() > 0);

    save_world(&mut world);
    drop(world);
    let mut loaded = World::load_or_create(root.clone(), base_reg()).unwrap();
    loaded.ensure_chunk(ChunkPos::from_surface(surface));
    let state = &loaded
        .arcane_geography
        .as_ref()
        .unwrap()
        .dynamic
        .dross_state;
    assert_eq!(state.scars.len(), 1);
    assert_eq!(state.materialized.len(), 1);
    assert_eq!(
        world_block_name(&loaded, manifestation),
        loaded.reg.block(scar_block).name
    );
    assert_eq!(loaded.get_block_at(scar_pos), structure);
    assert_eq!(loaded.get_meta_at(scar_pos), 173);

    let broken = loaded
        .break_block_at(manifestation, None, true, false)
        .expect("ordinary excavation removes a recoverable scar");
    let fragment = broken.drop.expect("scar excavation returns one fragment");
    assert_eq!(fragment.item, it(&loaded.reg, "base:scar_fragment"));
    assert_eq!(fragment.count, 1);
    assert_ne!(fragment.arcane_id, 0);
    assert_eq!(
        loaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&ArcaneOwner::ItemDross(fragment.arcane_id))
            .unwrap()
            .current,
        scar_current
    );
    assert!(
        loaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&ArcaneOwner::Scar(site.id))
            .is_none_or(|account| account.current.is_empty())
    );
    let site = &loaded
        .arcane_geography
        .as_ref()
        .unwrap()
        .dynamic
        .dross_state
        .scars[&site.id];
    let resolved_scar_id = site.id;
    assert!(site.resolved_step.is_some());
    assert!(site.materialized_at.is_none());
    let replacement = loaded.get_block_at(manifestation);
    assert_ne!(replacement, scar_block);
    assert!(
        replacement == AIR || loaded.reg.is_water(replacement),
        "excavation may expose air or physically backed seep water, not {:?}",
        loaded.reg.block(replacement).name
    );
    assert!(
        loaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .is_balanced()
    );

    loaded.retire_arcane_stack_at(manifestation, fragment, "burned in fire");
    assert!(
        loaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&ArcaneOwner::ItemDross(fragment.arcane_id))
            .is_none_or(|account| account.current.is_empty())
    );
    assert_eq!(
        loaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&ArcaneOwner::Dross {
                region,
                medium: DrossMedium::Air,
            })
            .unwrap()
            .current,
        scar_current,
        "burning a recovered scar changes its carrier instead of deleting it"
    );
    assert!(
        loaded
            .arcane_geography
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .is_balanced()
    );

    save_world(&mut loaded);
    drop(loaded);
    let mut reopened = World::load_or_create(root, base_reg()).unwrap();
    reopened.ensure_chunk(ChunkPos::from_surface(surface));
    assert_ne!(reopened.get_block_at(manifestation), scar_block);
    assert_eq!(reopened.get_block_at(scar_pos), structure);
    assert_eq!(reopened.get_meta_at(scar_pos), 173);
    let state = &reopened
        .arcane_geography
        .as_ref()
        .unwrap()
        .dynamic
        .dross_state;
    assert!(state.materialized.is_empty());
    assert_eq!(
        state.scars.len(),
        1,
        "resolved evidence remains bounded and durable"
    );
    assert!(state.scars[&resolved_scar_id].resolved_step.is_some());
    assert!(
        reopened
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&ArcaneOwner::Scar(resolved_scar_id))
            .is_none_or(|account| account.current.is_empty())
    );
}

#[test]
fn touched_chunk_prevents_first_scar_site_and_preserves_authored_bytes() {
    let (mut world, _) = dross_world("dross-touched-chunk-protection", 0xd205_5206);
    let region = AtlasPos {
        face: Face::PosZ,
        u: 8,
        v: 8,
    };
    let surface = surface_atlas_center(&world, region);
    world.ensure_chunk(ChunkPos::from_surface(surface));
    let at = crate::planet::BlockPos::new(
        surface.face(),
        surface.u(),
        (world.surface_height_at(surface) + 1) as u8,
        surface.v(),
    )
    .unwrap();
    let structure = b(&world.reg, "base:planks");
    world.set_block_authored_at(at, structure, "authored scar exclusion fixture");
    world.set_block_meta_at(at, structure, 211);
    seed_dense_soil_dross(&mut world, region, 1_200, DrossBand::Seep);

    complete_next_dross_hour(&mut world);

    assert_eq!(world.get_block_at(at), structure);
    assert_eq!(world.get_meta_at(at), 211);
    let state = &world.arcane_geography.as_ref().unwrap().dynamic.dross_state;
    assert!(state.scars.is_empty());
    assert!(state.materialized.is_empty());
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .accounts
            .keys()
            .all(|owner| !matches!(owner, ArcaneOwner::Scar(_)))
    );
}

#[test]
fn closed_scar_handlers_choose_bounded_physical_habitats() {
    use crate::dross::{DrossProvenance, ScarKind, ScarSite};

    let qualify = |tag: &str,
                   content_id: &str,
                   kind: ScarKind,
                   anchor_support: &str,
                   preferred_support: &str| {
        let (mut world, _) = dross_world(tag, 0xd205_5207);
        let region = AtlasPos {
            face: Face::PosZ,
            u: 8,
            v: 8,
        };
        let surface = surface_atlas_center(&world, region);
        let chunk = ChunkPos::from_surface(surface);
        world.insert_empty_chunks_for_test([chunk]);
        let anchor =
            crate::planet::BlockPos::new(surface.face(), surface.u(), 0, surface.v()).unwrap();
        let preferred = anchor.offset(2, 0, 0).unwrap();
        assert_eq!(preferred.chunk(), chunk);
        for (du, dv) in [
            (0, 0),
            (1, 0),
            (0, 1),
            (-1, 0),
            (0, -1),
            (1, 1),
            (-1, 1),
            (-1, -1),
            (1, -1),
            (2, 0),
            (0, 2),
            (-2, 0),
            (0, -2),
        ] {
            let support = anchor.offset(du, 0, dv).unwrap();
            world.set_block_at(support, AIR);
            world.set_block_at(support.offset(0, 1, 0).unwrap(), AIR);
        }
        world.set_block_at(anchor, b(&world.reg, anchor_support));
        world.set_block_at(preferred, b(&world.reg, preferred_support));
        world
            .arcane_geography
            .as_mut()
            .unwrap()
            .dynamic
            .dross_state
            .scars
            .insert(
                1,
                ScarSite {
                    id: 1,
                    region,
                    kind,
                    content_id: content_id.into(),
                    site_slot: 0,
                    created_step: 1,
                    last_changed_step: 1,
                    breach_count: 0,
                    materialized_at: None,
                    resolved_step: None,
                    actor_hint: None,
                    installation_hint: None,
                    provenance: DrossProvenance::default(),
                },
            );

        world.refresh_dross_scars_for_test();

        let site = &world
            .arcane_geography
            .as_ref()
            .unwrap()
            .dynamic
            .dross_state
            .scars[&1];
        let materialized = site
            .materialized_at
            .expect("closed placement handler found a bounded site");
        if kind == ScarKind::WetFilm {
            let below = materialized.offset(0, -1, 0).unwrap();
            assert!(
                world.reg.is_water(world.get_block_at(below))
                    || [(1, 0), (-1, 0), (0, 1), (0, -1)]
                        .into_iter()
                        .filter_map(|(du, dv)| below.offset(du, 0, dv))
                        .any(|neighbor| world.reg.is_water(world.get_block_at(neighbor))),
                "water-margin handler selected a dry unsupported cell"
            );
        } else {
            assert_eq!(materialized, preferred.offset(0, 1, 0).unwrap());
        }
        assert_eq!(
            world.get_block_at(materialized),
            world
                .reg
                .resolve_dross_scar(content_id, kind)
                .unwrap()
                .block
        );
        assert_eq!(world.get_block_at(anchor), b(&world.reg, anchor_support));
        assert_eq!(
            world.get_block_at(preferred),
            b(&world.reg, preferred_support)
        );
    };

    qualify(
        "dross-handler-water-margin",
        "base:scar_wet_film",
        ScarKind::WetFilm,
        "base:stone",
        "base:water",
    );
    qualify(
        "dross-handler-filament",
        "base:scar_forest_threads",
        ScarKind::ForestThreads,
        "base:stone",
        "base:log",
    );
    qualify(
        "dross-handler-mineral",
        "base:scar_industrial_scale",
        ScarKind::IndustrialScale,
        "base:log",
        "base:stone",
    );
}
