//! Ecology harvest scenarios.

use super::*;

#[test]
fn lantern_reed_light_is_driven_by_its_stored_current() {
    let mut world = ecology_world("lantern-current-state", 10_099);
    let pos = materialize_ecology_site(&mut world, "base:lantern_reed");
    let lit = world.reg.block_id("base:lantern_reed").unwrap();
    let dim = world.reg.block_id("base:lantern_reed_dim").unwrap();

    let original_charge = {
        let site = world
            .arcane_geography
            .as_mut()
            .unwrap()
            .dynamic
            .ecology
            .sites
            .iter_mut()
            .find(|site| site.block_pos() == Some(pos))
            .unwrap();
        let charge = site.charge;
        assert!(
            site.charge_total() > 0,
            "genesis lantern has stored Current"
        );
        site.charge = [0; 6];
        charge
    };
    world.refresh_arcane_ecology_for_test();
    assert_eq!(world.get_block_at(pos), dim);
    assert_eq!(world.reg.block(dim).light_emit, 0);
    assert_eq!(world.light_at_pos(pos).0, 0);

    world
        .arcane_geography
        .as_mut()
        .unwrap()
        .dynamic
        .ecology
        .sites
        .iter_mut()
        .find(|site| site.block_pos() == Some(pos))
        .unwrap()
        .charge = original_charge;
    world.refresh_arcane_ecology_for_test();
    assert_eq!(world.get_block_at(pos), lit);
    assert_eq!(world.reg.block(lit).light_emit, 4);
    assert_eq!(world.light_at_pos(pos).0, 4);
}

#[test]
fn simultaneous_wellglass_harvest_resolves_once_and_both_custodies_balance() {
    // Seed 76 has qualified glass-heath habitat in the deliberately tiny
    // fixture; not every finite test planet should be forced to have one.
    let mut world = ecology_world("wellglass-host-once", 76);
    let pos = materialize_ecology_site(&mut world, "base:wellglass_bud");
    let site_id = {
        let geography = world.arcane_geography.as_mut().unwrap();
        let site = geography
            .dynamic
            .ecology
            .sites
            .iter_mut()
            .find(|site| site.block_pos() == Some(pos))
            .unwrap();
        site.crystal_stage = 3;
        site.id
    };
    let before = world
        .arcane_geography
        .as_ref()
        .unwrap()
        .dynamic
        .ecology
        .sites
        .iter()
        .find(|site| site.id == site_id)
        .unwrap()
        .charge_total();
    assert!(before > 0);
    let pick = it(&world.reg, "base:iron_pickaxe");
    let ire_before = world.ire;
    let first = world
        .break_block_at(pos, Some(pick), true, true)
        .expect("host accepts first mature harvest");
    assert!(
        world.ire > ire_before,
        "taking, not Current arithmetic, raises Ire"
    );
    let shard = first.drop.expect("wellglass shard");
    assert_ne!(shard.arcane_id, 0);
    assert_eq!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .item_current_total(shard.arcane_id),
        Some(before)
    );
    assert_eq!(world.get_block_at(pos), b(&world.reg, "base:wellglass_bud"));
    assert!(
        world.break_block_at(pos, Some(pick), true, true).is_none(),
        "second same-state host command must not fall through to generic loot"
    );
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

#[test]
fn replanting_a_collapsed_site_restores_it_and_credits_tending() {
    let mut world = ecology_world("ecology-common-seed-restoration", 10_105);
    let pos = materialize_ecology_site(&mut world, "base:rainbell");
    {
        let geography = world.arcane_geography.as_mut().unwrap();
        let site = geography
            .dynamic
            .ecology
            .sites
            .iter_mut()
            .find(|site| site.block_pos() == Some(pos))
            .unwrap();
        site.population = 0;
        site.seed_bank = 0;
        site.stage = crate::arcane_ecology::EcologyStage::Collapsed;
    }
    world.set_block_at(pos, AIR);
    world.ire = 10.0;
    let seed = ItemStack::new(&world.reg, it(&world.reg, "base:rainbell_dew"), 1);
    assert!(world.place_item_block_at(pos, seed));
    let site = world
        .arcane_geography
        .as_ref()
        .unwrap()
        .dynamic
        .ecology
        .sites
        .iter()
        .find(|site| site.block_pos() == Some(pos))
        .unwrap();
    assert_eq!(
        site.ownership,
        crate::arcane_ecology::EcologyOwnership::Genesis
    );
    assert_eq!(site.stage, crate::arcane_ecology::EcologyStage::Recovering);
    assert_eq!(site.population, 1);
    assert!(site.seed_bank > 0);
    assert!(
        world.ire < 10.0,
        "successful establishment uses the tending hook"
    );
}

#[test]
fn ashlace_harvest_keeps_sequestered_dross_on_the_item_and_composting_returns_it() {
    let mut world = ecology_world("ashlace-item-dross", 10_102);
    let pos = materialize_ecology_site(&mut world, "base:ashlace");
    {
        let geography = world.arcane_geography.as_mut().unwrap();
        let site_index = geography
            .dynamic
            .ecology
            .sites
            .iter()
            .position(|site| site.block_pos() == Some(pos))
            .unwrap();
        let cell_index = geography.dynamic.ecology.sites[site_index]
            .atlas_pos
            .index(geography.manifest.side);
        let slot = geography.dynamic.cells[cell_index]
            .ambient
            .iter()
            .enumerate()
            .max_by_key(|(_, units)| **units)
            .map(|(slot, _)| slot)
            .unwrap();
        let moved = geography.dynamic.cells[cell_index].ambient[slot].min(90);
        geography.dynamic.cells[cell_index].ambient[slot] -= moved;
        geography.dynamic.ecology.sites[site_index].dross[slot] += u32::from(moved);
        geography.dynamic.ecology.sites[site_index].charge = [0; 6];
    }
    let drop = world
        .break_block_at(pos, None, true, false)
        .unwrap()
        .drop
        .unwrap();
    let ledger = world.arcane_ledger.as_ref().unwrap();
    let dross = ledger.item_dross_total(drop.arcane_id);
    assert!(dross > 0);
    assert!(
        ledger
            .account(&crate::arcane::ArcaneOwner::Item(drop.arcane_id))
            .is_none(),
        "sequestered dross must not be relabeled as clean bound Current"
    );
    assert_eq!(crate::world::soil::compost_value("base:ashlace_tissue"), 1);
    let total_before = ledger.audit().unwrap().total;
    world.retire_arcane_stack_at(pos, drop, "ashlace tissue composted");
    let ledger = world.arcane_ledger.as_ref().unwrap();
    assert_eq!(ledger.item_current_total(drop.arcane_id), None);
    assert_eq!(ledger.audit().unwrap().total, total_before);
    assert!(ledger.audit().unwrap().is_balanced());
}

#[test]
fn rainbell_dew_moves_real_soil_water_into_circulating_custody() {
    let mut world = ecology_world("rainbell-real-water", 10_103);
    let pos = materialize_ecology_site(&mut world, "base:rainbell");
    let before = world.live_water_audit().unwrap();
    let soil_before = world.ecology_soil_water_hu_at(pos.surface()).unwrap();
    assert!(
        soil_before >= crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL,
        "wetland fixture needs one dew measure of live soil water"
    );
    let drop = world
        .break_block_at(pos, None, true, false)
        .expect("water-backed rainbell harvest")
        .drop
        .unwrap();
    assert_eq!(drop.item, it(&world.reg, "base:rainbell_dew"));
    let after = world.live_water_audit().unwrap();
    assert_eq!(after.current_water_hu, before.current_water_hu);
    assert_eq!(
        after.unexplained_water_delta_hu, before.unexplained_water_delta_hu,
        "dew harvest must introduce no new water-audit delta"
    );
    assert_eq!(
        after.industrial.water_hu - before.industrial.water_hu,
        crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL
    );
    assert_eq!(
        soil_before - world.ecology_soil_water_hu_at(pos.surface()).unwrap(),
        crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL
    );

    let planted = pos.offset(1, 0, 0).unwrap();
    world.set_block_at(planted, AIR);
    assert!(world.place_item_block_at(planted, drop));
    let returned = world.live_water_audit().unwrap();
    assert_eq!(returned.current_water_hu, before.current_water_hu);
    assert_eq!(
        returned.unexplained_water_delta_hu,
        before.unexplained_water_delta_hu
    );
    assert_eq!(returned.industrial.water_hu, before.industrial.water_hu);
    assert_eq!(
        world.ecology_soil_water_hu_at(pos.surface()).unwrap(),
        soil_before,
        "planting the dew closes its local real-water loop"
    );
}

#[test]
fn explosive_wellglass_loss_destroys_the_bud_and_cannot_rematerialize_charge() {
    let mut world = ecology_world("wellglass-explosion-persistence", 10_107);
    let pos = materialize_ecology_site(&mut world, "base:wellglass_bud");
    let site_id = world
        .arcane_geography
        .as_ref()
        .unwrap()
        .dynamic
        .ecology
        .sites
        .iter()
        .find(|site| site.block_pos() == Some(pos))
        .unwrap()
        .id;
    let total_before = world
        .arcane_geography
        .as_ref()
        .unwrap()
        .audit()
        .unwrap()
        .accounted_total;
    world
        .break_block_at(pos, None, false, false)
        .expect("explosion-style loss settles the persistent site");
    assert_ne!(world.get_block_at(pos), b(&world.reg, "base:wellglass_bud"));
    let geography = world.arcane_geography.as_ref().unwrap();
    let site = geography
        .dynamic
        .ecology
        .sites
        .iter()
        .find(|site| site.id == site_id)
        .unwrap();
    assert_eq!(site.stage, crate::arcane_ecology::EcologyStage::Harvested);
    assert_eq!(site.seed_bank, 0);
    assert_eq!(site.charge_total(), 0);
    assert_eq!(geography.audit().unwrap().accounted_total, total_before);
    let dir = world.save_dir_for_test().to_path_buf();
    let reg = world.reg.clone();
    save_world(&mut world);
    drop(world);
    let mut loaded = World::load_or_create(dir, reg).unwrap();
    loaded.ensure_chunk(pos.chunk());
    assert_ne!(
        loaded.get_block_at(pos),
        b(&loaded.reg, "base:wellglass_bud")
    );
    let geography = loaded.arcane_geography.as_ref().unwrap();
    let site = geography
        .dynamic
        .ecology
        .sites
        .iter()
        .find(|site| site.id == site_id)
        .unwrap();
    assert_eq!(site.stage, crate::arcane_ecology::EcologyStage::Harvested);
    assert_eq!(site.charge_total(), 0);
    assert_eq!(geography.audit().unwrap().accounted_total, total_before);
}

#[test]
fn magical_harvests_and_bonus_drops_are_funded_before_delivery() {
    use crate::arcane::ArcaneOwner;

    let reg = base_reg();
    let mut world = World::load_or_create(
        tmp_dir("funded-magical-harvests").join("world"),
        reg.clone(),
    )
    .unwrap();
    world.ensure_chunk(tchunk(0, 0));
    let at = crate::planet::BlockPos::of_world(3, 150, 3).unwrap();
    world.set_block_at(at, b(&reg, "base:lantern_fungus"));
    let drop = world
        .break_block_at(at, None, true, false)
        .unwrap()
        .drop
        .unwrap();
    assert_eq!(drop.item, it(&reg, "base:lantern_fungus"));
    assert_ne!(drop.arcane_id, 0);
    assert_eq!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&ArcaneOwner::Item(drop.arcane_id))
            .unwrap()
            .current
            .total(),
        192
    );
    let placed_at = crate::planet::BlockPos::of_world(4, 200, 3).unwrap();
    let ambient_before = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .audit()
        .unwrap()
        .reservoirs[&crate::arcane::Reservoir::Ambient];
    assert!(world.place_item_block_at(placed_at, drop));
    assert_eq!(
        world.get_block_at(placed_at),
        b(&reg, "base:lantern_fungus")
    );
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&ArcaneOwner::Item(drop.arcane_id))
            .is_none()
    );
    assert_eq!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .reservoirs[&crate::arcane::Reservoir::Ambient],
        ambient_before + 192
    );

    let gold_quartz = b(&reg, "base:gold_quartz");
    assert!(
        reg.block(gold_quartz).arcane.is_none(),
        "this fixture proves the output item, not the source block, controls binding"
    );
    let mut rng = 0;
    let bonus = world
        .roll_bonus_drop_at(at, gold_quartz, &mut rng)
        .expect("seed zero rolls below the declared half chance");
    assert_eq!(bonus.item, it(&reg, "base:quartz_shard"));
    assert_ne!(bonus.arcane_id, 0);
    assert_eq!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&ArcaneOwner::Item(bonus.arcane_id))
            .unwrap()
            .current
            .total(),
        384
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
