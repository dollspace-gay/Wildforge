//! Ecology custody scenarios.

use super::*;

#[test]
fn resonant_mineral_break_reconciles_physical_and_arcane_ledgers_together() {
    let mut world = ecology_world("resonant-mineral-dual-ledger", 10_104);
    let surface = world
        .arcane_geography
        .as_ref()
        .unwrap()
        .dynamic
        .ecology
        .sites
        .first()
        .and_then(|site| site.surface())
        .unwrap();
    world.ensure_chunk(ChunkPos::from_surface(surface));
    let pos = crate::planet::BlockPos::new(surface.face(), surface.u(), 220, surface.v()).unwrap();
    let mineral = b(&world.reg, "base:choirstone");
    world.set_block_authored_at(pos, mineral, "dual-ledger fixture");
    let before = world.material_ledger.as_ref().unwrap().audit();
    assert_eq!(before.placed.get("choirstone"), Some(&1_200));
    let pick = it(&world.reg, "base:iron_pickaxe");
    let drop = world
        .break_block_at(pos, Some(pick), true, false)
        .expect("resonant mineral extraction")
        .drop
        .unwrap();
    assert_ne!(drop.arcane_id, 0);
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_current_total(drop.arcane_id)
            .is_some_and(|units| units > 0)
    );
    let materials = world.material_ledger.as_ref().unwrap().audit();
    assert_eq!(materials.placed.get("choirstone").copied().unwrap_or(0), 0);
    assert_eq!(materials.circulating.get("choirstone"), Some(&1_200));
    assert!(materials.is_balanced());
    let geography = world.arcane_geography.as_ref().unwrap();
    assert_eq!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&crate::arcane::ArcaneOwner::Geography)
            .unwrap()
            .current,
        geography.custody_current().unwrap()
    );
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .is_balanced()
    );
}

#[test]
fn plausible_grazer_bite_collapses_magical_forage_without_rematerializing_charge() {
    let mut world = ecology_world("magical-forage-bite", 10_106);
    let pos = materialize_ecology_site(&mut world, "base:rainbell");
    let total_before = world
        .arcane_geography
        .as_ref()
        .unwrap()
        .audit()
        .unwrap()
        .accounted_total;
    world.apply_bite_at(pos);
    assert_eq!(world.get_block_at(pos), AIR);
    let geography = world.arcane_geography.as_ref().unwrap();
    let site = geography
        .dynamic
        .ecology
        .sites
        .iter()
        .find(|site| site.block_pos() == Some(pos))
        .unwrap();
    assert!(matches!(
        site.stage,
        crate::arcane_ecology::EcologyStage::Collapsed
            | crate::arcane_ecology::EcologyStage::Harvested
    ));
    assert_eq!(geography.audit().unwrap().accounted_total, total_before);
    assert_eq!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&crate::arcane::ArcaneOwner::Geography)
            .unwrap()
            .current,
        geography.custody_current().unwrap()
    );
    let dir = world.save_dir_for_test().to_path_buf();
    let reg = world.reg.clone();
    save_world(&mut world);
    drop(world);
    let mut loaded = World::load_or_create(dir, reg).unwrap();
    loaded.ensure_chunk(pos.chunk());
    assert_eq!(loaded.get_block_at(pos), AIR);
}

#[test]
fn arcane_ire_and_dross_four_states_persist_independently() {
    let reg = base_reg();
    let mut readings = std::collections::BTreeSet::new();
    for (angry, polluted) in [(false, false), (false, true), (true, false), (true, true)] {
        let dir = tmp_dir(&format!("arcane-separation-{angry}-{polluted}"));
        let atlas = std::sync::Arc::new(
            crate::planet_atlas::PlanetAtlas::fixture(8_811 + angry as u32, 8).unwrap(),
        );
        atlas.write_new(&dir).unwrap();
        let site = atlas.biomes.countries[0].heart_site.center(atlas.side());
        let surface =
            crate::planet::SurfacePos::new(site.face, site.u.floor() as u16, site.v.floor() as u16)
                .unwrap();
        let at = crate::planet::BlockPos::new(surface.face(), surface.u(), 1, surface.v()).unwrap();
        let region = atlas.atlas_pos(surface);
        let mut world =
            World::new_with_atlas(8_811 + angry as u32, dir.clone(), reg.clone(), atlas);
        world.ire = if angry { 80.0 } else { 0.0 };
        if polluted {
            let mut ember = ItemStack::new(&reg, it(&reg, "base:ember"), 1);
            world
                .bind_arcane_stack_at(at, &mut ember, "separation fixture")
                .unwrap();
            world.retire_arcane_stack_at(at, ember, "separation pollution");
        }
        save_world(&mut world);
        let loaded = World::load_or_create(dir, reg.clone()).unwrap();
        let bands = loaded.arcane_ledger.as_ref().unwrap().local_bands(region);
        assert_eq!(loaded.ire, if angry { 80.0 } else { 0.0 });
        assert_eq!(bands[1] > 0, polluted);
        assert!(
            loaded
                .arcane_ledger
                .as_ref()
                .unwrap()
                .audit()
                .unwrap()
                .is_balanced()
        );
        readings.insert((loaded.ire as u8, bands[1]));
    }
    assert_eq!(readings.len(), 4);
}

#[test]
fn dross_changes_ire_only_when_it_causes_real_ecology_population_loss() {
    let mut world = ecology_world("dross-habitat-harm-ire", 10_108);
    let (site_index, tolerance) = {
        let geography = world.arcane_geography.as_ref().unwrap();
        geography
            .dynamic
            .ecology
            .sites
            .iter()
            .enumerate()
            .find_map(|(index, site)| {
                let definition = world.reg.arcane_ecology.get(&site.content_id)?;
                (site.charge_total() > u64::from(definition.dross_tolerance))
                    .then_some((index, definition.dross_tolerance))
            })
            .expect("fixture needs one charged ecology site")
    };
    let needed = u64::from(tolerance).saturating_add(1);
    let (population_before, completed_before) = {
        let geography = world.arcane_geography.as_mut().unwrap();
        let site = &mut geography.dynamic.ecology.sites[site_index];
        site.population = 1;
        site.stage = crate::arcane_ecology::EcologyStage::Mature;
        let mut remaining = needed;
        for slot in 0..6 {
            let moved = u64::from(site.charge[slot]).min(remaining);
            site.charge[slot] -= moved as u32;
            site.dross[slot] = site.dross[slot].checked_add(moved as u32).unwrap();
            remaining -= moved;
        }
        assert_eq!(remaining, 0);
        (site.population, geography.dynamic.ecology.completed_days)
    };
    let ire_before = world.ire;
    assert_eq!(
        world.ire, ire_before,
        "moving Current into Dross changed Ire"
    );
    world.set_calendar_day(u32::try_from(completed_before.saturating_add(1)).unwrap());

    world.tick_arcane_ecology(usize::MAX).unwrap();

    let geography = world.arcane_geography.as_ref().unwrap();
    let site = &geography.dynamic.ecology.sites[site_index];
    assert!(site.population < population_before);
    assert!(
        world.ire > ire_before,
        "accounted Dross killed habitat population without invoking Ire"
    );
    assert!(geography.audit().unwrap().is_balanced());
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .is_balanced()
    );
}
