//! Pricing scenarios.

use super::*;

#[test]
fn strained_ground_measurably_increases_real_wand_working_waste() {
    use crate::dross::DrossBand;

    let reserve = |tag: &str, band: DrossBand| {
        let (mut world, source, wand) = workings_world(tag);
        let region = world.planet_atlas().unwrap().atlas_pos(source.surface());
        let side = world.planet_atlas().unwrap().side();
        world
            .arcane_geography
            .as_mut()
            .unwrap()
            .dynamic
            .dross_state
            .cells[region.index(side)]
        .band = band;
        let started = world
            .begin_gleam_working(
                [0x31; 16],
                "environmental strain qualification",
                source,
                wand.arcane_id,
                source,
                2,
                20,
                false,
            )
            .unwrap();
        let transaction = &world.workings_state.as_ref().unwrap().active[&started.stable_id];
        (
            transaction.reserved_current.total(),
            transaction.dross_current.total(),
            transaction.strain.strain,
        )
    };

    let clear = reserve("workings-clear-ground-strain", DrossBand::Clear);
    let strained = reserve("workings-strained-ground-strain", DrossBand::Strained);
    assert_eq!(
        clear.0, strained.0,
        "the compared workings priced different charge"
    );
    assert!(
        strained.1 > clear.1 && strained.2 > clear.2,
        "strained ground reserved dross {:?} versus {:?} on clear ground",
        strained,
        clear
    );
}

#[test]
fn storm_cordial_prices_real_wand_current_and_keeps_its_warning_visible() {
    use crate::alchemy::ActivePreparationStatus;

    let actor = [70; 16];
    let (mut ordinary, ordinary_source, ordinary_wand) =
        workings_world("workings-storm-cordial-ordinary");
    let (mut storm, storm_source, storm_wand) = workings_world("workings-storm-cordial-active");
    let definition = storm.reg.preparations["base:storm_cordial"].clone();
    let now = (storm.clock().max(0.0) * 20.0).round() as u64;
    let (source_batch, status_id) = {
        let state = storm.alchemy_state.as_mut().unwrap();
        (
            state.allocate_batch_id().unwrap(),
            state.allocate_status_id().unwrap(),
        )
    };
    storm.alchemy_state.as_mut().unwrap().statuses.insert(
        actor,
        vec![ActivePreparationStatus {
            status_id,
            preparation_id: definition.id.clone(),
            definition_version: definition.version,
            source_batch,
            actor,
            dose_volume_units: definition.dose_units,
            active_current: Default::default(),
            dross_current: Default::default(),
            started_tick: now,
            last_tick: now,
            due_tick: now.saturating_add(definition.effect.duration_ticks),
            recovery_until_tick: now
                .saturating_add(definition.effect.duration_ticks)
                .saturating_add(definition.effect.recovery_ticks),
            stack_group: definition.stack_group.clone(),
            completed_units: 0,
            refresh_count: 0,
            overdose_until_tick: 0,
        }],
    );

    let modifiers = storm.preparation_modifiers(actor);
    assert_eq!(
        modifiers.throughput_permille,
        definition.effect.throughput_permille
    );
    assert_eq!(modifiers.drain_permille, definition.effect.drain_permille);
    assert_eq!(
        modifiers.overdraw_permille,
        definition.effect.overdraw_permille
    );
    assert!(modifiers.storm_warning);

    let ordinary_started = ordinary
        .begin_trace_working(
            actor,
            "ordinary trace fixture",
            ordinary_source,
            ordinary_wand.arcane_id,
            ordinary_source.offset(1, 0, 0).unwrap(),
            20,
            false,
        )
        .unwrap();
    let storm_started = storm
        .begin_trace_working(
            actor,
            "storm cordial trace fixture",
            storm_source,
            storm_wand.arcane_id,
            storm_source.offset(1, 0, 0).unwrap(),
            20,
            false,
        )
        .unwrap();
    let ordinary_charge = ordinary.workings_state.as_ref().unwrap().active
        [&ordinary_started.stable_id]
        .reserved_current
        .total();
    let storm_transaction =
        &storm.workings_state.as_ref().unwrap().active[&storm_started.stable_id];
    assert_eq!(
        storm_transaction.reserved_current.total(),
        ordinary_charge
            .saturating_mul(u64::from(definition.effect.drain_permille))
            .div_ceil(1_000),
        "Storm Cordial advertised extra drain without pricing it into the real reservation"
    );
    assert!(storm_transaction.reserved_current.total() > ordinary_charge);

    storm.set_simulation_clock((now + 41) as f64 / 20.0);
    let ticked = storm
        .tick_preparation_statuses(actor, storm_source, Default::default())
        .unwrap();
    assert!(ticked.modifiers.storm_warning);
    assert!(ticked.cues.iter().any(|cue| {
        cue.kind == crate::alchemy::AlchemyCueKind::Pulse
            && cue.message.contains("drain")
            && cue.message.contains("overdraw")
    }));
}

#[test]
fn base_roster_is_eight_wand_workings_and_four_rituals() {
    let reg = base_reg();
    assert_eq!(reg.workings.len(), 12);
    assert_eq!(
        reg.workings
            .values()
            .filter(|definition| definition.mode == crate::workings::DeliveryMode::Wand)
            .count(),
        8
    );
    assert_eq!(
        reg.workings
            .values()
            .filter(|definition| definition.mode == crate::workings::DeliveryMode::Ritual)
            .count(),
        4
    );
    assert_eq!(
        reg.workings
            .values()
            .map(|definition| definition.handler)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        12
    );
    assert!(reg.workings.values().all(|definition| {
        !definition.label.trim().is_empty()
            && definition.description.split_whitespace().count() >= 8
    }));
}

#[test]
fn every_magical_route_keeps_a_bounded_niche_below_bulk_technology() {
    let reg = base_reg();
    let get = |id: &str| reg.workings.get(id).unwrap();
    let draw = get("base:draw");
    let fastest_draw_hu_per_second = draw.max_volume as f32
        / (crate::workings::MIN_WAND_SETTLE_SECONDS + crate::workings::WAND_RECOVERY_SECONDS);
    let pump_hu_per_second =
        crate::planet_atlas::HYDRO_UNITS_PER_BLOCK as f32 / crate::world::PUMP_STROKE_SECS;
    assert!(fastest_draw_hu_per_second < pump_hu_per_second);
    assert!(draw.max_volume < crate::planet_atlas::HYDRO_UNITS_PER_BLOCK as u32);

    let gleam = get("base:gleam");
    assert!(gleam.charge_per_second > 0);
    assert_ne!(reg.block(b(&reg, "base:torch")).light_rgb, [0; 3]);

    let nudge = get("base:nudge");
    assert_eq!(nudge.max_targets, 1);
    assert!(nudge.range <= 6 && nudge.max_magnitude <= 4);

    let rootwake = get("base:rootwake");
    assert_eq!(rootwake.max_targets, 1);
    assert!(rootwake.physical.iter().any(|need| need == "water"));
    assert!(rootwake.physical.iter().any(|need| need == "nutrients"));

    let fieldmend = get("base:fieldmend");
    assert!(fieldmend.max_magnitude <= 16);
    assert!(
        fieldmend
            .physical
            .iter()
            .any(|need| need == "matching_repair_material")
    );
    assert!(fieldmend.dross >= 6 && fieldmend.wear >= 7);

    let holdfast = get("base:holdfast");
    assert_eq!(holdfast.max_targets, 1);
    assert!(holdfast.charge_per_second > 0);

    let rooting_bed = get("base:rooting_bed");
    assert!(rooting_bed.max_magnitude <= 16 && rooting_bed.max_targets <= 16);
    let ward = get("base:ward_boundary");
    assert!(ward.charge_per_second > 0 && ward.max_targets <= 64);
    let circle = get("base:transfer_circle");
    assert!(circle.range <= 2 && circle.max_targets == 2);
    assert!(reg.workings.values().all(|working| {
        working.charge > 0
            && working.dross > 0
            && working.wear > 0
            && (working.mode != crate::workings::DeliveryMode::Ritual
                || !working.physical.is_empty())
    }));
}

#[test]
fn valid_fixture_mod_working_executes_its_own_approved_shell() {
    let mods = tmp_dir("workings-valid-mod");
    let pack = mods.join("gentle");
    std::fs::create_dir_all(&pack).unwrap();
    std::fs::write(
        pack.join("mod.toml"),
        "id = \"gentle\"\nname = \"Gentle Workings\"\nversion = \"1\"\nworld_api = 2\ndepends = [\"base\"]\n",
    )
    .unwrap();
    std::fs::write(
        pack.join("workings.toml"),
        r#"
schema_version = 1

[[working]]
id = "patient_trace"
label = "Patient Trace"
handler = "trace"
mode = "wand"
focus = "base:echo"
charge = 9
charge_per_second = 1
dross = 1
safe_throughput = 12
range = 8
max_magnitude = 1
max_targets = 1
max_duration_ticks = 1200
target = ["visible_arcane"]
interruption = "end_continuous"
disposition = "split"
wear = 1
description = "A fixture-mod sensing shell executed only by the native bounded Trace handler."
"#,
    )
    .unwrap();
    let registry = crate::registry::load(&mods);
    assert!(
        registry.arcane_errors.is_empty(),
        "{:?}",
        registry.arcane_errors
    );
    let definition = registry.workings.get("gentle:patient_trace").unwrap();
    assert_eq!(definition.provider, "gentle");
    assert_eq!(definition.handler, crate::workings::WorkingHandler::Trace);

    let world_dir = tmp_dir("workings-valid-mod-world");
    let atlas = std::sync::Arc::new(crate::planet_atlas::PlanetAtlas::fixture(8_805, 16).unwrap());
    atlas.write_new(&world_dir).unwrap();
    let mut world = World::new_with_atlas(8_805, world_dir, std::sync::Arc::new(registry), atlas);
    let (frame, _) = crate::tests::implements::install_frame_fixture(&mut world);
    let (wand, _) = crate::tests::implements::assemble_fixture_wand(&mut world, frame);
    charge_fixture_vessel(&mut world, wand.arcane_id, 256);
    let source = frame.offset(0, 2, 0).unwrap();
    world.set_block_at(source, AIR);
    let mut inventory = Inventory::new();
    inventory.slots[1] = Some(fitted_lens_stack(&mut world));
    let started = world
        .begin_wand_working(
            [29; 16],
            "fixture-mod-worker",
            source,
            wand.arcane_id,
            "gentle:patient_trace",
            crate::workings::WorkingTargetIntent::None,
            Some(&inventory),
            false,
        )
        .unwrap();
    let transaction = &world.workings_state.as_ref().unwrap().active[&started.stable_id];
    assert_eq!(transaction.definition.id, "gentle:patient_trace");
    assert_eq!(transaction.definition.provider, "gentle");
    assert_eq!(transaction.reserved_current.total(), 39);
    settle_channel(&mut world, started.stable_id);
    world.release_working(started.stable_id).unwrap();
    let event = world
        .workings_state
        .as_ref()
        .unwrap()
        .history
        .back()
        .unwrap();
    assert_eq!(event.working_id, "gentle:patient_trace");
}

#[test]
fn visible_apparatus_damage_increases_strain_and_eventually_refuses() {
    let (mut sound_world, sound_source, sound_wand) = workings_world("workings-sound-wand");
    let sound = sound_world
        .begin_gleam_working(
            [8; 16],
            "sound-wand-worker",
            sound_source,
            sound_wand.arcane_id,
            sound_source,
            2,
            20,
            false,
        )
        .unwrap();
    let sound_tx = sound_world.workings_state.as_ref().unwrap().active[&sound.stable_id].clone();

    let (mut damaged_world, damaged_source, damaged_wand) = workings_world("workings-damaged-wand");
    {
        let instance = damaged_world
            .implements_state
            .as_mut()
            .unwrap()
            .instances
            .get_mut(&damaged_wand.arcane_id)
            .unwrap();
        instance.wear = 750;
        instance.strain = 5_000;
    }
    let visible = damaged_world.implement_tooltip(damaged_wand, true);
    assert!(visible.iter().any(|line| line == "Condition: critical"));
    assert!(
        visible
            .iter()
            .any(|line| line.contains("wear 750; strain 5000"))
    );
    let damaged = damaged_world
        .begin_gleam_working(
            [8; 16],
            "damaged-wand-worker",
            damaged_source,
            damaged_wand.arcane_id,
            damaged_source,
            2,
            20,
            false,
        )
        .unwrap();
    let damaged_tx =
        damaged_world.workings_state.as_ref().unwrap().active[&damaged.stable_id].clone();
    assert!(damaged_tx.strain.strain > sound_tx.strain.strain);
    assert!(damaged_tx.dross_current.total() > sound_tx.dross_current.total());
    assert!(damaged.warning_band >= sound.warning_band);

    damaged_world.cancel_working(damaged.stable_id).unwrap();
    damaged_world
        .implements_state
        .as_mut()
        .unwrap()
        .instances
        .get_mut(&damaged_wand.arcane_id)
        .unwrap()
        .wear = crate::implements::MAX_WAND_WEAR;
    assert!(
        damaged_world
            .begin_gleam_working(
                [8; 16],
                "broken-wand-worker",
                damaged_source,
                damaged_wand.arcane_id,
                damaged_source,
                2,
                20,
                false,
            )
            .is_err()
    );
}
