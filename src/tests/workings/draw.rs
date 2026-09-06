//! Draw scenarios.

use super::*;

#[test]
fn draw_moves_one_exact_salted_parcel_without_overflow_or_loss() {
    let (mut world, wand_pos, wand) = workings_world("workings-draw");
    let current_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let from = wand_pos.offset(2, 1, 0).unwrap();
    let to = wand_pos.offset(3, 1, 0).unwrap();
    world.set_block_water_at(from, world.reg.water_block(0), 80, 20_480);
    world.set_block_at(to, AIR);
    let before = world.water_mass_at(from).unwrap();
    let total_water = before.water_hu;
    let total_salt = before.salt_mass;

    let started = world
        .begin_draw_working(
            [8; 16],
            "draw-fixture",
            wand_pos,
            wand.arcane_id,
            from,
            to,
            32,
            false,
        )
        .unwrap();
    world.complete_working(started.stable_id).unwrap();

    let source_after = world.water_mass_at(from).unwrap();
    let destination_after = world.water_mass_at(to).unwrap();
    assert_eq!(
        source_after.water_hu + destination_after.water_hu,
        total_water
    );
    assert_eq!(
        source_after.salt_mass + destination_after.salt_mass,
        total_salt
    );
    assert_eq!(destination_after.water_hu, 32);
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        current_before
    );
}

#[test]
fn draw_and_ordinary_flow_conserve_exact_heat_dross_and_fixed_point_remainders() {
    let reg = base_reg();
    let dir = tmp_dir("workings-draw-carriers");
    let atlas = std::sync::Arc::new(crate::planet_atlas::PlanetAtlas::fixture(8_805, 16).unwrap());
    atlas.write_new(&dir).unwrap();
    let mut world = World::new_with_atlas(8_805, dir.clone(), reg, atlas.clone());
    let (frame, _) = crate::tests::implements::install_frame_fixture(&mut world);
    let (wand, _) = crate::tests::implements::assemble_fixture_wand(&mut world, frame);
    let wand_pos = frame.offset(0, 2, 0).unwrap();
    world.set_block_at(wand_pos, AIR);
    let from = wand_pos.offset(2, 0, 0).unwrap();
    let to = wand_pos.offset(3, 0, 0).unwrap();
    let flowed = wand_pos.offset(4, 0, 0).unwrap();
    world.set_block_water_at(from, world.reg.water_block(0), 80, 20_480);
    world.set_block_at(to, AIR);
    world.set_block_at(flowed, AIR);
    let original_carrier = crate::workings::WaterCarrier {
        // Deliberately indivisible by both 256 HU and the 32 HU parcel.
        thermal_millic_hu: 4_383_491,
        dross_subunits: 1_027,
    };
    world
        .water_carriers
        .as_mut()
        .unwrap()
        .cells
        .insert(from, original_carrier);

    let started = world
        .begin_draw_working(
            [18; 16],
            "draw-carrier-fixture",
            wand_pos,
            wand.arcane_id,
            from,
            to,
            32,
            false,
        )
        .unwrap();
    world.complete_working(started.stable_id).unwrap();

    let source_after_mass = world.water_mass_at(from).unwrap();
    let destination_after_mass = world.water_mass_at(to).unwrap();
    let source_after = world.water_carrier_at(from, source_after_mass);
    let destination_after = world.water_carrier_at(to, destination_after_mass);
    assert_eq!(
        source_after
            .thermal_millic_hu
            .checked_add(destination_after.thermal_millic_hu),
        Some(original_carrier.thermal_millic_hu)
    );
    assert_eq!(
        source_after
            .dross_subunits
            .checked_add(destination_after.dross_subunits),
        Some(original_carrier.dross_subunits)
    );
    assert_ne!(source_after.dross_subunits % 256, 0);

    assert!(world.move_water_units(to, flowed, 1));
    let flowed_mass = world.water_mass_at(flowed).unwrap();
    let flowed_carrier = world.water_carrier_at(flowed, flowed_mass);
    assert_eq!(flowed_carrier, destination_after);
    assert_eq!(world.water_mass_at(to), None);
    assert!(
        !world
            .water_carriers
            .as_ref()
            .unwrap()
            .cells
            .contains_key(&to),
        "ordinary flow must not leave an orphan carrier"
    );

    save_world(&mut world);
    let persisted = crate::workings::WaterCarrierState::load_or_initialize(&dir).unwrap();
    assert_eq!(persisted.cells.get(&from), Some(&source_after));
    assert_eq!(persisted.cells.get(&flowed), Some(&flowed_carrier));
    assert_eq!(
        persisted
            .cells
            .values()
            .map(|carrier| i128::from(carrier.thermal_millic_hu))
            .sum::<i128>(),
        i128::from(original_carrier.thermal_millic_hu)
    );
    assert_eq!(
        persisted
            .cells
            .values()
            .map(|carrier| u128::from(carrier.dross_subunits))
            .sum::<u128>(),
        u128::from(original_carrier.dross_subunits)
    );

    // Loading the authoritative sidecar is the restart boundary. Chunk
    // persistence itself has separate generated-chunk coverage; this fixture
    // deliberately installs empty in-memory chunks for speed.
    let reloaded = crate::workings::WaterCarrierState::load_or_initialize(&dir).unwrap();
    assert_eq!(reloaded.cells, persisted.cells);
}
