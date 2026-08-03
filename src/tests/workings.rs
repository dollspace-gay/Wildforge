use super::*;

fn settle_channel(world: &mut World, id: u64) -> crate::workings::WorkingResult {
    world.clock += f64::from(crate::workings::MIN_WAND_SETTLE_SECONDS) + 0.01;
    world.activate_working(id).unwrap()
}

fn workings_world(tag: &str) -> (World, crate::planet::BlockPos, ItemStack) {
    let mut world = super::implements::embodied_implements_world(tag);
    let (frame, _) = super::implements::install_frame_fixture(&mut world);
    let (wand, _) = super::implements::assemble_fixture_wand(&mut world, frame);
    let player_source = frame.offset(0, 2, 0).unwrap();
    world.set_block_at(player_source, AIR);
    (world, player_source, wand)
}

fn ritual_world(
    tag: &str,
) -> (
    World,
    crate::planet::BlockPos,
    crate::planet::BlockPos,
    crate::planet::BlockPos,
    ItemStack,
) {
    use crate::implements::FrameAction;

    let mut world = super::implements::embodied_implements_world(tag);
    let (controller, first_vessel) = super::implements::install_frame_fixture(&mut world);
    world.ensure_block_entity_at(
        controller,
        crate::world::BlockEntity::BindingFrame(Default::default()),
    );
    let wand = super::implements::assemble_fixture_wand(&mut world, controller).0;

    // Calibrate a second vessel through a separate complete physical frame,
    // then move that exact stable item beside the ritual controller.
    let second_frame = controller.offset(8, 0, 0).unwrap();
    let second_vessel_fixture = second_frame.offset(-1, 0, 0).unwrap();
    for (pos, block) in [
        (second_frame, b(&world.reg, "base:binding_frame")),
        (
            second_frame.offset(1, 0, 0).unwrap(),
            b(&world.reg, "base:focus_mount"),
        ),
        (
            second_frame.offset(0, 0, 1).unwrap(),
            b(&world.reg, "base:arcane_conductor"),
        ),
        (
            second_frame.offset(0, 0, -1).unwrap(),
            b(&world.reg, "base:containment_post"),
        ),
    ] {
        world.set_block_authored_at(pos, block, "second ritual fixture");
    }
    assert!(world.place_item_block_at(
        second_vessel_fixture,
        ItemStack::new(&world.reg, it(&world.reg, "base:charge_vessel"), 1),
    ));
    world
        .operate_binding_frame(
            second_frame,
            &mut Inventory::new(),
            0,
            FrameAction::Calibrate,
            None,
            "ritual-fixture",
        )
        .unwrap();
    let second_id = match world.block_entity_at(&second_vessel_fixture).unwrap() {
        crate::world::BlockEntity::ChargeVessel(vessel) => vessel.vessel.unwrap().arcane_id,
        _ => panic!("second ritual fixture did not embody its vessel"),
    };
    assert_ne!(second_id, 0);
    let pick = it(&world.reg, "base:bronze_pickaxe");
    assert!(
        world
            .break_block_at(second_vessel_fixture, Some(pick), true, false)
            .is_some()
    );
    let second_stack = world
        .take_pending_drops()
        .into_iter()
        .find_map(|(_, stack)| (stack.arcane_id == second_id).then_some(stack))
        .unwrap();
    let second_vessel = controller.offset(-2, 0, 0).unwrap();
    assert!(world.place_item_block_at(second_vessel, second_stack));
    assert!(world.binding_frame_layout(controller).valid);
    (world, controller, first_vessel, second_vessel, wand)
}

fn vessel_stack_at(world: &World, pos: crate::planet::BlockPos) -> ItemStack {
    match world.block_entity_at(&pos).unwrap() {
        crate::world::BlockEntity::ChargeVessel(vessel) => vessel.vessel.unwrap(),
        _ => panic!("fixture vessel has the wrong physical state"),
    }
}

fn charge_fixture_vessel(world: &mut World, vessel_id: u64, desired_clean: u64) {
    use crate::arcane::{ArcaneAuthority, ArcaneOwner, ArcaneTransaction};

    let destination = ArcaneOwner::Item(vessel_id);
    let clean = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .item_clean_total(vessel_id)
        .unwrap_or_default();
    let needed = desired_clean.saturating_sub(clean);
    if needed == 0 {
        return;
    }
    let source = ArcaneOwner::Deep;
    let (source_version, destination_version, current) = {
        let ledger = world.arcane_ledger.as_ref().unwrap();
        let account = ledger.account(&source).unwrap();
        let mut available = account.current.clone();
        let current = available.take_units(needed, std::iter::empty()).unwrap();
        (account.version, ledger.version_of(&destination), current)
    };
    let ledger = world.arcane_ledger.as_mut().unwrap();
    let transaction = ArcaneTransaction::transfer(
        ledger.system_transaction_id().unwrap(),
        source,
        source_version,
        destination,
        destination_version,
        current,
        ArcaneAuthority::System,
        "ritual test fixture charged a real bounded vessel",
    );
    ledger.commit(transaction).unwrap();
}

fn fitted_lens_stack(world: &mut World) -> ItemStack {
    let item = it(&world.reg, "base:tuning_lens");
    let definition = world.reg.item(item).clone();
    let arcane = definition.arcane.as_ref().unwrap();
    let arcane_id = world
        .arcane_ledger
        .as_mut()
        .unwrap()
        .bind_new_item(
            crate::arcane::ArcaneOwner::Deep,
            arcane,
            &definition.name,
            "workings fitted tuning-lens fixture",
        )
        .unwrap();
    ItemStack {
        arcane_id,
        ..ItemStack::new(&world.reg, item, 1)
    }
}

fn move_fixture_current_to_deep(
    world: &mut World,
    owner: crate::arcane::ArcaneOwner,
    retained_units: u64,
) {
    use crate::arcane::{ArcaneAuthority, ArcaneOwner, ArcaneTransaction};

    let Some(account) = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .account(&owner)
        .cloned()
    else {
        return;
    };
    let amount = account.current.total().saturating_sub(retained_units);
    if amount == 0 {
        return;
    }
    let mut available = account.current.clone();
    let moved = available.take_units(amount, std::iter::empty()).unwrap();
    let destination = ArcaneOwner::Deep;
    let ledger = world.arcane_ledger.as_mut().unwrap();
    let transaction = ArcaneTransaction::transfer(
        ledger.system_transaction_id().unwrap(),
        owner,
        account.version,
        destination.clone(),
        ledger.version_of(&destination),
        moved,
        ArcaneAuthority::System,
        "workings fixture isolated an unsafe local draw",
    );
    ledger.commit(transaction).unwrap();
}

fn persistent_workings_world(
    tag: &str,
) -> (
    World,
    std::path::PathBuf,
    crate::planet::BlockPos,
    ItemStack,
) {
    let reg = base_reg();
    let dir = tmp_dir(tag);
    let atlas = std::sync::Arc::new(crate::planet_atlas::PlanetAtlas::fixture(8_806, 16).unwrap());
    atlas.write_new(&dir).unwrap();
    let mut world = World::new_with_atlas(8_806, dir.clone(), reg, atlas);
    let frame = bp(8, 100, 8);
    let center = frame.chunk();
    for chunk in (-1..=1).flat_map(|du| (-1..=1).map(move |dv| center.offset(du, dv))) {
        world.ensure_chunk(chunk);
    }
    for (pos, block) in [
        (frame, b(&world.reg, "base:binding_frame")),
        (bp(9, 100, 8), b(&world.reg, "base:focus_mount")),
        (bp(8, 100, 9), b(&world.reg, "base:arcane_conductor")),
        (bp(8, 100, 7), b(&world.reg, "base:containment_post")),
    ] {
        world.set_block_authored_at(pos, block, "persistent workings fixture");
    }
    assert!(world.place_item_block_at(
        bp(7, 100, 8),
        ItemStack::new(&world.reg, it(&world.reg, "base:charge_vessel"), 1),
    ));
    let wand = super::implements::assemble_fixture_wand(&mut world, frame).0;
    let source = frame.offset(0, 2, 0).unwrap();
    world.set_block_at(source, AIR);
    save_world(&mut world);
    (world, dir, source, wand)
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
    let (frame, _) = super::implements::install_frame_fixture(&mut world);
    let (wand, _) = super::implements::assemble_fixture_wand(&mut world, frame);
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
fn trace_requires_the_real_fitted_lens_and_one_wand_cannot_double_channel() {
    let (mut world, source, wand) = workings_world("workings-trace-lens-authority");
    let empty = Inventory::new();
    let request = crate::workings::WorkingTargetIntent::None;
    let refusal = world
        .begin_wand_working(
            [41; 16],
            "lensless-worker",
            source,
            wand.arcane_id,
            "base:trace",
            request,
            Some(&empty),
            false,
        )
        .unwrap_err();
    assert!(refusal.contains("fitted tuning lens"), "{refusal}");

    let mut inventory = Inventory::new();
    inventory.slots[2] = Some(fitted_lens_stack(&mut world));
    let started = world
        .begin_wand_working(
            [41; 16],
            "equipped-worker",
            source,
            wand.arcane_id,
            "base:trace",
            request,
            Some(&inventory),
            false,
        )
        .unwrap();
    let other_target = source.offset(1, 0, 0).unwrap();
    world.set_block_at(other_target, AIR);
    let conflict = world
        .begin_gleam_working(
            [42; 16],
            "competing-worker",
            source,
            wand.arcane_id,
            other_target,
            1,
            20,
            false,
        )
        .unwrap_err();
    assert!(conflict.contains("apparatus or target is already reserved"));
    world.cancel_working(started.stable_id).unwrap();
    let refusal = world
        .begin_wand_working(
            [41; 16],
            "focus-worker",
            source,
            wand.arcane_id,
            "base:gleam",
            crate::workings::WorkingTargetIntent::Block {
                pos: other_target,
                adjacent: None,
            },
            Some(&inventory),
            false,
        )
        .unwrap_err();
    assert!(refusal.contains("physical apparatus focus"));
}

#[test]
fn forced_wand_draw_crosses_the_safe_floor_visibly_and_is_audited() {
    use crate::arcane::ArcaneOwner;

    fn prepared(tag: &str) -> (World, crate::planet::BlockPos, ItemStack) {
        let (mut world, source, wand) = workings_world(tag);
        let region = world.planet_atlas().unwrap().atlas_pos(source.surface());
        move_fixture_current_to_deep(
            &mut world,
            ArcaneOwner::Item(wand.arcane_id),
            crate::implements::STRUCTURAL_SPARK_UNITS,
        );
        move_fixture_current_to_deep(&mut world, ArcaneOwner::Ambient(region), 0);
        (world, source, wand)
    }

    let (mut safe_world, safe_source, safe_wand) = prepared("workings-safe-draw");
    let safe_total = safe_world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .audit()
        .unwrap()
        .total;
    let safe = safe_world
        .begin_gleam_working(
            [31; 16],
            "safe-draw",
            safe_source,
            safe_wand.arcane_id,
            safe_source,
            1,
            20,
            false,
        )
        .unwrap();
    let safe_transaction =
        safe_world.workings_state.as_ref().unwrap().active[&safe.stable_id].clone();
    let safe_region = safe_world
        .planet_atlas()
        .unwrap()
        .atlas_pos(safe_source.surface());
    assert!(!safe_transaction.forced);
    assert_eq!(
        safe_world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&ArcaneOwner::Ambient(safe_region))
            .unwrap()
            .current
            .total(),
        64,
        "an ordinary draw must preserve the measured local floor"
    );

    let (mut forced_world, forced_source, forced_wand) = prepared("workings-forced-draw");
    let forced_total = forced_world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .audit()
        .unwrap()
        .total;
    let forced = forced_world
        .begin_gleam_working(
            [32; 16],
            "forced-draw",
            forced_source,
            forced_wand.arcane_id,
            forced_source,
            1,
            20,
            true,
        )
        .unwrap();
    let forced_transaction =
        forced_world.workings_state.as_ref().unwrap().active[&forced.stable_id].clone();
    let forced_region = forced_world
        .planet_atlas()
        .unwrap()
        .atlas_pos(forced_source.surface());
    assert!(forced_transaction.forced);
    assert!(forced_transaction.strain.strain > safe_transaction.strain.strain);
    assert!(forced_transaction.dross_current.total() > safe_transaction.dross_current.total());
    assert!(forced_transaction.strain.warning_band >= 2);
    assert!(
        forced_world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&ArcaneOwner::Ambient(forced_region))
            .is_none(),
        "forced draw must actually cross rather than silently refill the floor"
    );

    settle_channel(&mut forced_world, forced.stable_id);
    forced_world.release_working(forced.stable_id).unwrap();
    let audit = forced_world
        .workings_state
        .as_ref()
        .unwrap()
        .history
        .back()
        .unwrap();
    assert!(audit.forced);
    assert_eq!(audit.warning_band, forced_transaction.strain.warning_band);
    assert_eq!(
        forced_world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        forced_total
    );
    settle_channel(&mut safe_world, safe.stable_id);
    safe_world.release_working(safe.stable_id).unwrap();
    assert_eq!(
        safe_world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        safe_total
    );

    let mut refusals = Vec::new();
    for tag in ["workings-forced-refusal-a", "workings-forced-refusal-b"] {
        let (mut world, source, wand) = prepared(tag);
        let total = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
        let instance = world
            .implements_state
            .as_mut()
            .unwrap()
            .instances
            .get_mut(&wand.arcane_id)
            .unwrap();
        instance.wear = crate::implements::MAX_WAND_WEAR - 1;
        instance.strain = crate::implements::MAX_WAND_STRAIN - 1;
        let refusal = world
            .begin_gleam_working(
                [44; 16],
                "forced-refusal",
                source,
                wand.arcane_id,
                source,
                8,
                20,
                true,
            )
            .unwrap_err();
        assert!(refusal.contains("forced overdraw"), "{refusal}");
        assert!(world.workings_state.as_ref().unwrap().active.is_empty());
        assert_eq!(
            world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
            total
        );
        refusals.push(refusal);
    }
    assert_eq!(refusals[0], refusals[1]);
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

#[test]
fn kindle_reserves_current_then_uses_ordinary_player_fire_provenance() {
    let (mut world, source, wand) = workings_world("workings-kindle");
    let fuel = source.offset(2, 0, 0).unwrap();
    let fire_cell = fuel.offset(0, 1, 0).unwrap();
    world.set_block_authored_at(fuel, b(&world.reg, "base:log"), "kindle fixture fuel");
    world.set_block_at(fire_cell, AIR);
    let before_total = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;

    let started = world
        .begin_kindle_working(
            [7; 16],
            "kindle-fixture",
            source,
            wand.arcane_id,
            fuel,
            fire_cell,
            false,
        )
        .unwrap();
    let reserved = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .account(&crate::arcane::ArcaneOwner::Working(started.stable_id))
        .unwrap()
        .current
        .clone();
    assert_eq!(
        reserved,
        world.workings_state.as_ref().unwrap().active[&started.stable_id].reserved_current
    );

    world.complete_working(started.stable_id).unwrap();
    assert_eq!(world.get_block_at(fire_cell), b(&world.reg, "base:fire"));
    assert_ne!(world.get_meta_at(fire_cell) & 0x80, 0);
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&crate::arcane::ArcaneOwner::Working(started.stable_id))
            .is_none()
    );
    let after = world.arcane_ledger.as_ref().unwrap().audit().unwrap();
    assert_eq!(after.total, before_total);
    assert_eq!(after.unexplained_delta, 0);
    let event = world
        .workings_state
        .as_ref()
        .unwrap()
        .history
        .iter()
        .find(|event| event.id == started.stable_id)
        .unwrap();
    assert_eq!(
        event.actor, [7; 16],
        "harmful fire keeps stable player identity"
    );
}

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
    let (frame, _) = super::implements::install_frame_fixture(&mut world);
    let (wand, _) = super::implements::assemble_fixture_wand(&mut world, frame);
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

#[test]
fn rootwake_advances_one_stage_and_debits_real_water_and_fertility() {
    let (mut world, wand_pos, wand) = workings_world("workings-rootwake");
    let current_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let plant = wand_pos.offset(2, 1, 0).unwrap();
    let soil = plant.offset(0, -1, 0).unwrap();
    let water = plant.offset(1, 0, 0).unwrap();
    let farmland = b(&world.reg, "base:farmland");
    let seed = b(&world.reg, "base:wheat_seeds");
    let next = world.reg.block(seed).crop_next.unwrap();
    world.set_block_meta_at(soil, farmland, crate::world::soil::soil_meta(42, 0));
    world.set_block_at(plant, seed);
    world.set_block_water_at(water, world.reg.water_block(0), 0, 0);
    let water_before = world.water_mass_at(water).unwrap();
    let fertility_before = world.fertility_at_pos(soil);

    let started = world
        .begin_rootwake_working(
            [9; 16],
            "rootwake-fixture",
            wand_pos,
            wand.arcane_id,
            plant,
            false,
        )
        .unwrap();
    let legitimate = world.workings_state.as_ref().unwrap().active[&started.stable_id].clone();
    let mut input_free_forgery = legitimate.clone();
    input_free_forgery.physical_debits.clear();
    assert!(
        input_free_forgery.validate().is_err(),
        "a forged biological effect must not omit its water and nutrient debits"
    );
    let mut cross_capability_forgery = legitimate;
    cross_capability_forgery.effect = crate::workings::WorkingEffect::PointLight {
        source: wand_pos,
        target: plant,
        intensity: 1,
        expires_tick: 1,
    };
    assert!(
        cross_capability_forgery.validate().is_err(),
        "a Rootwake shell must not obtain a light or arbitrary mutation capability"
    );
    world.complete_working(started.stable_id).unwrap();

    assert_eq!(world.get_block_at(plant), next);
    assert_eq!(world.fertility_at_pos(soil), fertility_before - 1);
    assert_eq!(
        world.water_mass_at(water).unwrap().water_hu,
        water_before.water_hu - crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        current_before
    );
}

#[test]
fn rootwake_cannot_override_dead_hearts_winter_or_missing_habitat_inputs() {
    let (mut dead_world, source, wand) = workings_world("workings-rootwake-dead-heart");
    let plant = source.offset(2, 1, 0).unwrap();
    let soil = plant.offset(0, -1, 0).unwrap();
    let water = plant.offset(1, 0, 0).unwrap();
    dead_world.set_block_at(soil, b(&dead_world.reg, "base:dirt"));
    dead_world.set_block_at(plant, b(&dead_world.reg, "base:berry_bush"));
    dead_world.set_block_water_at(water, dead_world.reg.water_block(0), 0, 0);
    let province = dead_world.generator.province_at(plant.surface()).key;
    dead_world.hearts.insert(
        province,
        crate::world::Heart {
            pos: plant,
            stage: 0,
            strain: 100.0,
            rooting: 0.0,
            graft: None,
            drift: 0.0,
            regrow: 0.0,
        },
    );
    let current_before = dead_world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .audit()
        .unwrap()
        .total;
    let water_before = dead_world.water_mass_at(water).unwrap();
    let refusal = dead_world
        .begin_rootwake_working(
            [42; 16],
            "dead-heart-worker",
            source,
            wand.arcane_id,
            plant,
            false,
        )
        .unwrap_err();
    assert!(refusal.contains("dead heart"), "{refusal}");
    assert_eq!(
        dead_world.get_block_at(plant),
        b(&dead_world.reg, "base:berry_bush")
    );
    assert_eq!(dead_world.water_mass_at(water).unwrap(), water_before);
    assert_eq!(
        dead_world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        current_before
    );

    let (mut winter_world, winter_source, winter_wand) = workings_world("workings-rootwake-winter");
    let winter_plant = winter_source.offset(2, 1, 0).unwrap();
    let winter_soil = winter_plant.offset(0, -1, 0).unwrap();
    let winter_water = winter_plant.offset(1, 0, 0).unwrap();
    winter_world.set_block_meta_at(
        winter_soil,
        b(&winter_world.reg, "base:farmland"),
        crate::world::soil::soil_meta(42, 0),
    );
    winter_world.set_block_at(winter_plant, b(&winter_world.reg, "base:wheat_seeds"));
    winter_world.set_block_water_at(winter_water, winter_world.reg.water_block(0), 0, 0);
    winter_world.long_winter = true;
    let winter_current_before = winter_world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .audit()
        .unwrap()
        .total;
    let winter_water_before = winter_world.water_mass_at(winter_water).unwrap();
    let winter_fertility_before = winter_world.fertility_at_pos(winter_soil);
    let refusal = winter_world
        .begin_rootwake_working(
            [43; 16],
            "winter-worker",
            winter_source,
            winter_wand.arcane_id,
            winter_plant,
            false,
        )
        .unwrap_err();
    assert!(refusal.contains("Winter stops"), "{refusal}");
    assert_eq!(
        winter_world.water_mass_at(winter_water).unwrap(),
        winter_water_before
    );
    assert_eq!(
        winter_world.fertility_at_pos(winter_soil),
        winter_fertility_before
    );
    assert_eq!(
        winter_world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        winter_current_before
    );

    winter_world.long_winter = false;
    winter_world.set_block_at(winter_water, AIR);
    let refusal = winter_world
        .begin_rootwake_working(
            [43; 16],
            "dry-habitat-worker",
            winter_source,
            winter_wand.arcane_id,
            winter_plant,
            false,
        )
        .unwrap_err();
    assert!(
        refusal.contains("real adjacent water reservoir"),
        "{refusal}"
    );
    assert_eq!(
        winter_world.fertility_at_pos(winter_soil),
        winter_fertility_before
    );
    assert_eq!(
        winter_world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        winter_current_before
    );
}

#[test]
fn cancellation_returns_clean_current_once_and_disorders_a_declared_remainder() {
    let (mut world, source, wand) = workings_world("workings-cancel");
    let target = source.offset(1, 0, 0).unwrap();
    let wand_owner = crate::arcane::ArcaneOwner::Item(wand.arcane_id);
    let before = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .account(&wand_owner)
        .unwrap()
        .current
        .total();
    let started = world
        .begin_trace_working(
            [10; 16],
            "cancel-fixture",
            source,
            wand.arcane_id,
            target,
            20,
            false,
        )
        .unwrap();
    world.cancel_working(started.stable_id).unwrap();
    assert!(world.cancel_working(started.stable_id).is_err());
    let after = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .account(&wand_owner)
        .unwrap()
        .current
        .total();
    assert!(after < before);
    assert!(world.workings_state.as_ref().unwrap().active.is_empty());
    assert_eq!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .unexplained_delta,
        0
    );
}

#[test]
fn early_release_cancels_instead_of_bypassing_host_settle_time() {
    let (mut world, source, wand) = workings_world("workings-early-release");
    let fuel = source.offset(2, 0, 0).unwrap();
    let fire_cell = fuel.offset(0, 1, 0).unwrap();
    world.set_block_authored_at(fuel, b(&world.reg, "base:log"), "early-release fuel");
    world.set_block_at(fire_cell, AIR);
    let total_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let started = world
        .begin_wand_working(
            [11; 16],
            "impatient-worker",
            source,
            wand.arcane_id,
            "base:kindle",
            crate::workings::WorkingTargetIntent::Block {
                pos: fuel,
                adjacent: Some(fire_cell),
            },
            None,
            false,
        )
        .unwrap();

    let result = world.release_working(started.stable_id).unwrap();
    assert_eq!(result.cue, crate::workings::WorkingCueKind::Cancel);
    assert_eq!(world.get_block_at(fire_cell), AIR);
    assert_eq!(
        world
            .workings_state
            .as_ref()
            .unwrap()
            .history
            .iter()
            .filter(|event| event.id == started.stable_id)
            .count(),
        1
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        total_before
    );
}

#[test]
fn trace_reports_only_bounded_local_drift_leakage_and_recent_working_evidence() {
    let (mut world, source, wand) = workings_world("workings-trace-reading");
    let current_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let fuel = source.offset(2, 0, 0).unwrap();
    let fire_cell = fuel.offset(0, 1, 0).unwrap();
    world.set_block_authored_at(fuel, b(&world.reg, "base:log"), "trace fixture fuel");
    world.set_block_at(fire_cell, AIR);
    let prior = world
        .begin_kindle_working(
            [21; 16],
            "prior-worker",
            source,
            wand.arcane_id,
            fuel,
            fire_cell,
            false,
        )
        .unwrap();
    world.complete_working(prior.stable_id).unwrap();

    let started = world
        .begin_trace_working(
            [22; 16],
            "trace-worker",
            source,
            wand.arcane_id,
            fuel,
            20 * 30,
            false,
        )
        .unwrap();
    let reading = settle_channel(&mut world, started.stable_id);
    assert!(reading.message.contains("Trace resolves"));
    assert!(reading.message.contains("drift"));
    assert!(reading.message.contains("one faint recent working trace"));
    assert!(reading.message.contains("charge leak"));
    assert!(reading.message.contains("uncertainty 5%"));
    assert!(!reading.message.contains("prior-worker"));
    assert!(!reading.message.contains("Kindle"));
    assert!(!reading.message.contains("base:kindle"));

    let history = &world.workings_state.as_ref().unwrap().history;
    let prior_event = history
        .iter()
        .find(|event| event.id == prior.stable_id)
        .unwrap();
    assert_eq!(prior_event.source, source);
    assert_eq!(prior_event.path, vec![fuel, fire_cell]);
    world.release_working(started.stable_id).unwrap();
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        current_before
    );
}

#[test]
fn nudge_uses_one_locked_projectile_and_ordinary_velocity_without_teleporting() {
    let (mut world, source, wand) = workings_world("workings-nudge");
    let current_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let wand_source = source.offset(0, 2, 0).unwrap();
    world.set_block_at(wand_source, AIR);
    let projectile_pos = wand_source
        .entity_center()
        .translated(glam::Vec3::new(2.0, 0.25, 0.0))
        .unwrap()
        .pos;
    world.spawn_projectile(crate::mobs::Projectile {
        stable_id: 777,
        pos: projectile_pos,
        vel: glam::Vec3::new(0.0, 0.0, 1.0),
        tile: 0,
        damage: 1.0,
        age: 0.0,
        from_player: true,
        drop_item: None,
        owner: 0,
    });
    let position_before = world.projectiles()[0].pos;
    let velocity_before = world.projectiles()[0].vel;
    let started = world
        .begin_nudge_working(
            [23; 16],
            "nudge-worker",
            wand_source,
            wand.arcane_id,
            777,
            2,
            false,
        )
        .unwrap();
    assert!(
        world
            .begin_nudge_working(
                [24; 16],
                "competing-worker",
                wand_source,
                wand.arcane_id,
                777,
                1,
                false,
            )
            .unwrap_err()
            .contains("already reserved")
    );
    world.complete_working(started.stable_id).unwrap();
    let projectile = &world.projectiles()[0];
    assert_eq!(
        projectile.pos, position_before,
        "Nudge is not teleportation"
    );
    assert_ne!(projectile.vel, velocity_before);
    assert!(projectile.vel.length() <= 40.0);
    assert_eq!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .unexplained_delta,
        0
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        current_before
    );
}

#[test]
fn nudge_operates_one_real_firebox_latch_and_machine_simulation_obeys_it() {
    let (mut world, source, wand) = workings_world("workings-nudge-mechanism");
    let firebox = source.offset(2, 0, 0).unwrap();
    let boiler = firebox.offset(0, 1, 0).unwrap();
    assert!(world.place_block_at(firebox, b(&world.reg, "base:firebox")));
    assert!(world.place_block_at(boiler, b(&world.reg, "base:boiler")));
    let crate::world::BlockEntity::Steam(steam) = world.block_entity_mut_at(&firebox).unwrap()
    else {
        panic!("placed firebox did not receive ordinary machine authority");
    };
    steam.fuel = 20.0;
    steam.water.water_hu = crate::planet_atlas::HYDRO_UNITS_PER_BLOCK;

    let closed = world
        .begin_nudge_mechanism_working(
            [33; 16],
            "nudge-latch",
            source,
            wand.arcane_id,
            firebox,
            false,
        )
        .unwrap();
    world.complete_working(closed.stable_id).unwrap();
    assert!(matches!(
        world.block_entity_at(&firebox),
        Some(crate::world::BlockEntity::Steam(steam)) if steam.draft_closed
    ));
    let fuel_before = match world.block_entity_at(&firebox).unwrap() {
        crate::world::BlockEntity::Steam(steam) => steam.fuel,
        _ => unreachable!(),
    };
    world.tick_entities(1.0);
    let fuel_closed = match world.block_entity_at(&firebox).unwrap() {
        crate::world::BlockEntity::Steam(steam) => steam.fuel,
        _ => unreachable!(),
    };
    assert_eq!(
        fuel_closed, fuel_before,
        "a closed physical draft must bank fuel"
    );
    assert!(
        matches!(
            world.block_entity_at(&firebox),
            Some(crate::world::BlockEntity::Steam(steam)) if steam.draft_closed
        ),
        "the ordinary lit/unlit visual swap must preserve the latch"
    );

    let dir = world.save_dir_for_saving();
    save_world(&mut world);
    drop(world);
    let mut world = World::load_or_create(dir, base_reg()).unwrap();
    world.ensure_chunk(firebox.chunk());
    assert!(
        matches!(
            world.block_entity_at(&firebox),
            Some(crate::world::BlockEntity::Steam(steam)) if steam.draft_closed
        ),
        "the embodied draft latch must survive a process restart"
    );

    let opened = world
        .begin_nudge_mechanism_working(
            [33; 16],
            "nudge-latch",
            source,
            wand.arcane_id,
            firebox,
            false,
        )
        .unwrap();
    world.complete_working(opened.stable_id).unwrap();
    assert!(matches!(
        world.block_entity_at(&firebox),
        Some(crate::world::BlockEntity::Steam(steam)) if !steam.draft_closed
    ));
    world.tick_entities(1.0);
    let fuel_open = match world.block_entity_at(&firebox).unwrap() {
        crate::world::BlockEntity::Steam(steam) => steam.fuel,
        _ => unreachable!(),
    };
    assert!(
        fuel_open < fuel_closed,
        "opening the draft must resume ordinary steam work"
    );
}

#[test]
fn nudge_targets_one_host_owned_dropped_stack_through_ordinary_physics() {
    let (mut world, source, wand) = workings_world("workings-nudge-dropped-item");
    let pos = source
        .entity_center()
        .translated(glam::Vec3::new(2.0, 0.5, 0.0))
        .unwrap()
        .pos;
    let drop = crate::entity::ItemEntity::new(
        pos,
        glam::Vec3::new(0.0, 1.0, 0.25),
        it(&world.reg, "base:stone"),
        64,
    );
    let drop_id = world.spawn_loose_item(drop);
    assert_ne!(drop_id, 0);

    let started = world
        .begin_nudge_working(
            [34; 16],
            "nudge-drop",
            source,
            wand.arcane_id,
            drop_id,
            1,
            false,
        )
        .unwrap();
    let reserved = &world.workings_state.as_ref().unwrap().active[&started.stable_id];
    assert!(matches!(
        reserved.effect,
        crate::workings::WorkingEffect::Impulse {
            entity_kind: crate::workings::NudgeEntityKind::DroppedItem,
            ..
        }
    ));
    let distance = match reserved.effect {
        crate::workings::WorkingEffect::Impulse { target, .. } => source
            .entity_center()
            .local_delta_to(target.entity_center())
            .length()
            .ceil() as u16,
        _ => unreachable!(),
    };
    assert_eq!(
        reserved.definition.quote(4, distance, 0).unwrap().charge,
        reserved.reserved_current.total(),
        "a full stack must pay the declared highest bounded mass band"
    );

    // The target is not frozen while the wand settles. Release refreshes the
    // exact write-ahead velocity, then applies only an impulse.
    world.tick_entities(0.1);
    let before_release = world
        .loose_items()
        .iter()
        .find(|item| item.stable_id == drop_id)
        .unwrap()
        .clone();
    settle_channel(&mut world, started.stable_id);
    world.release_working(started.stable_id).unwrap();
    let after = world
        .loose_items()
        .iter()
        .find(|item| item.stable_id == drop_id)
        .unwrap();
    assert_eq!(
        after.pos, before_release.pos,
        "Nudge may not teleport a drop"
    );
    assert_ne!(after.vel, before_release.vel);
    assert!(after.vel.length() <= 40.0);
    assert_eq!(after.item, before_release.item);
    assert_eq!(after.count, 64);
}

#[test]
fn fieldmend_consumes_matching_matter_leaves_residue_and_cannot_upgrade() {
    let (mut world, source, wand) = workings_world("workings-fieldmend");
    let actor = [25; 16];
    let tool = it(&world.reg, "base:bronze_pickaxe");
    let repair = it(&world.reg, "base:bronze_pickaxe/forge_scrap");
    let residue = it(&world.reg, "base:bronze_pickaxe/primitive_scale");
    let mut inventory = Inventory::new();
    let mut damaged = ItemStack::new(&world.reg, tool, 1);
    damaged.durability -= 20;
    inventory.slots[1] = Some(damaged);
    inventory.slots[2] = Some(ItemStack::new(&world.reg, repair, 1));
    let material_before = world.material_ledger.as_ref().unwrap().audit();
    let current_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;

    let started = world
        .begin_fieldmend_working(
            actor,
            "fieldmend-worker",
            source,
            wand.arcane_id,
            &inventory,
            1,
            2,
            16,
            false,
        )
        .unwrap();
    settle_channel(&mut world, started.stable_id);
    let landed = world
        .complete_inventory_working(started.stable_id, &mut inventory)
        .unwrap();
    assert_eq!(
        landed.phase,
        Some(crate::workings::WorkingPhase::PendingApply)
    );
    assert_eq!(inventory.slots[1].unwrap().item, tool);
    assert_eq!(
        inventory.slots[1].unwrap().durability,
        damaged.durability + 16
    );
    assert_eq!(inventory.slots[2].unwrap().item, residue);
    world.finish_inventory_working(started.stable_id).unwrap();
    assert!(world.finish_inventory_working(started.stable_id).is_err());

    let material_after = world.material_ledger.as_ref().unwrap().audit();
    assert!(material_after.is_balanced());
    assert_ne!(
        material_after.consumption_loss, material_before.consumption_loss,
        "inefficient field repair must account for lost matching matter"
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        current_before
    );

    let mut wrong = Inventory::new();
    wrong.slots[1] = Some(damaged);
    wrong.slots[2] = Some(ItemStack::new(
        &world.reg,
        it(&world.reg, "base:copper_pickaxe/forge_scrap"),
        1,
    ));
    assert!(
        world
            .begin_fieldmend_working(
                actor,
                "fieldmend-worker",
                source,
                wand.arcane_id,
                &wrong,
                1,
                2,
                16,
                false,
            )
            .unwrap_err()
            .contains("needs one separated stack")
    );
    let mut pristine = Inventory::new();
    pristine.slots[1] = Some(ItemStack::new(&world.reg, tool, 1));
    pristine.slots[2] = Some(ItemStack::new(&world.reg, repair, 1));
    assert!(
        world
            .begin_fieldmend_working(
                actor,
                "fieldmend-worker",
                source,
                wand.arcane_id,
                &pristine,
                1,
                2,
                16,
                false,
            )
            .is_err()
    );
}

#[test]
fn holdfast_only_slows_real_elapsed_age_and_ends_on_schedule() {
    let (mut world, source, wand) = workings_world("workings-holdfast");
    let actor = [26; 16];
    let mut inventory = Inventory::new();
    inventory.slots[1] = Some(ItemStack::new(&world.reg, it(&world.reg, "base:bread"), 1));
    let initial = inventory.slots[1].unwrap();
    let started = world
        .begin_holdfast_working(
            actor,
            "holdfast-worker",
            source,
            wand.arcane_id,
            &inventory,
            1,
            20 * 60,
            false,
        )
        .unwrap();
    settle_channel(&mut world, started.stable_id);
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&crate::arcane::ArcaneOwner::Working(started.stable_id))
            .is_some()
    );
    let first = world.holdfast_age_step(actor, 1, initial, 20, 20);
    assert_eq!(first, 5);
    let aged = ItemStack {
        durability: initial.durability - first,
        ..initial
    };
    let second = world.holdfast_age_step(actor, 1, aged, 20, 20);
    assert_eq!(second, 5);
    let effect = &world.workings_state.as_ref().unwrap().active[&started.stable_id].effect;
    assert!(matches!(
        effect,
        crate::workings::WorkingEffect::Preserve {
            elapsed_ticks: 40,
            age_advance_ticks: 10,
            ..
        }
    ));
    assert!(second <= 20, "Holdfast may never reverse elapsed age");

    world.clock = 61.0;
    let outcomes = world.tick_workings();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].0.stable_id, started.stable_id);
    assert_eq!(
        outcomes[0].0.cue,
        crate::workings::WorkingCueKind::Complete,
        "{}",
        outcomes[0].0.message
    );
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&crate::arcane::ArcaneOwner::Working(started.stable_id))
            .is_none()
    );
    assert_eq!(
        world.holdfast_age_step(actor, 1, aged, 20, 20),
        20,
        "an ended binding must grant no permanent passive benefit"
    );
}

#[test]
fn holdfast_slows_a_real_mounted_heart_seed_and_expired_seed_stays_dead() {
    let (mut world, source, wand) = workings_world("workings-holdfast-mounted-seed");
    let mount = source.offset(2, 0, 0).unwrap();
    assert!(world.place_block_at(mount, b(&world.reg, "base:experiment_apparatus")));
    let seed_item = it(&world.reg, "base:forest_seed");
    let seed = ItemStack::new(&world.reg, seed_item, 1);
    let crate::world::BlockEntity::DiscoveryApparatus(apparatus) =
        world.block_entity_mut_at(&mount).unwrap()
    else {
        panic!("the physical sample mount has no authoritative state")
    };
    apparatus.sample = Some(seed);
    let total_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;

    let started = world
        .begin_wand_working(
            [43; 16],
            "mounted-seed-worker",
            source,
            wand.arcane_id,
            "base:holdfast",
            crate::workings::WorkingTargetIntent::Block {
                pos: mount,
                adjacent: None,
            },
            None,
            false,
        )
        .unwrap();
    settle_channel(&mut world, started.stable_id);
    let cue = world
        .working_cues()
        .into_iter()
        .find(|cue| cue.stable_id == started.stable_id)
        .unwrap();
    assert_eq!(cue.path, vec![source, mount]);

    world.tick_entities(20.1);
    let preserved = match world.block_entity_at(&mount).unwrap() {
        crate::world::BlockEntity::DiscoveryApparatus(apparatus) => apparatus.sample.unwrap(),
        _ => unreachable!(),
    };
    assert_eq!(seed.durability - preserved.durability, 3);

    world.release_working(started.stable_id).unwrap();
    world.tick_entities(20.0);
    let ordinary = match world.block_entity_at(&mount).unwrap() {
        crate::world::BlockEntity::DiscoveryApparatus(apparatus) => apparatus.sample.unwrap(),
        _ => unreachable!(),
    };
    assert_eq!(preserved.durability - ordinary.durability, 10);
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        total_before
    );

    let expired = ItemStack {
        durability: 0,
        ..seed
    };
    let refusal = world
        .plant_heart_seed_stack_at(source, expired)
        .expect("an expired heart cutting must not root");
    assert!(refusal.contains("living interval has spent itself"));
}

#[test]
fn holdfast_slows_real_botanical_charge_leakage_and_only_spends_while_working() {
    use crate::arcane::ArcaneOwner;

    let (mut world, source, wand) = workings_world("workings-holdfast-botanical");
    let actor = [35; 16];
    let item = it(&world.reg, "base:rainbell_dew");
    let definition = world.reg.item(item).clone();
    let arcane = definition.arcane.clone().unwrap();
    let item_id = world
        .arcane_ledger
        .as_mut()
        .unwrap()
        .bind_new_item(
            ArcaneOwner::Deep,
            &arcane,
            &definition.name,
            "Holdfast charged botanical fixture",
        )
        .unwrap();
    let mut specimen = ItemStack::new(&world.reg, item, 1);
    specimen.arcane_id = item_id;
    let mut inventory = Inventory::new();
    inventory.slots[2] = Some(specimen);
    let total_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;

    let started = world
        .begin_holdfast_working(
            actor,
            "holdfast-botanical",
            source,
            wand.arcane_id,
            &inventory,
            2,
            20 * 60,
            false,
        )
        .unwrap();
    settle_channel(&mut world, started.stable_id);
    let slowed = world
        .leak_fragile_item_charge(actor, 2, specimen, source, 20)
        .unwrap();
    assert_eq!(slowed, 2, "480 permille instability leaks 5 units normally");
    let transaction = &world.workings_state.as_ref().unwrap().active[&started.stable_id];
    assert!(matches!(
        transaction.effect,
        crate::workings::WorkingEffect::Preserve {
            preservation_kind: crate::workings::PreservationKind::ChargeLeakage,
            elapsed_ticks: 5,
            age_advance_ticks: 2,
            charge_spent_units: 60,
            ..
        }
    ));
    let reserved_clean = transaction.return_current.total();
    assert!(reserved_clean > 60);

    world.release_working(started.stable_id).unwrap();
    let ordinary = world
        .leak_fragile_item_charge(actor, 2, specimen, source, 20)
        .unwrap();
    assert_eq!(ordinary, 5, "ended Holdfast must leave no passive benefit");
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        total_before,
        "preservation, leakage, refunds, and dross must conserve Current"
    );
}

#[test]
fn gleam_is_a_releasable_temporary_cue_and_never_places_a_light_block() {
    let (mut world, source, wand) = workings_world("workings-gleam");
    let target = source.offset(3, 0, 0).unwrap();
    world.set_block_at(target, AIR);
    let before = world.get_block_at(target);
    let current_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let started = world
        .begin_gleam_working(
            [27; 16],
            "gleam-worker",
            source,
            wand.arcane_id,
            target,
            6,
            20 * 30,
            false,
        )
        .unwrap();
    settle_channel(&mut world, started.stable_id);
    let cue = world
        .working_cues()
        .into_iter()
        .find(|cue| cue.stable_id == started.stable_id)
        .unwrap();
    assert_eq!(cue.handler, crate::workings::WorkingHandler::Gleam);
    assert_eq!(cue.path, vec![source, target]);
    assert_eq!(world.get_block_at(target), before);
    world.release_working(started.stable_id).unwrap();
    assert!(
        world
            .working_cues()
            .iter()
            .all(|cue| cue.stable_id != started.stable_id)
    );
    assert_eq!(world.get_block_at(target), before);
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        current_before
    );

    let ranged = world
        .begin_gleam_working(
            [27; 16],
            "gleam-range-worker",
            source,
            wand.arcane_id,
            target,
            2,
            20 * 30,
            false,
        )
        .unwrap();
    settle_channel(&mut world, ranged.stable_id);
    let far_actor = source.offset(20, 0, 0).unwrap();
    assert!(!world.wand_working_reachable_from(ranged.stable_id, far_actor));
    world.interrupt_working(ranged.stable_id).unwrap();

    let (mut depleted_world, depleted_source, depleted_wand) =
        workings_world("workings-gleam-depletion");
    let depleted_target = depleted_source.offset(2, 0, 0).unwrap();
    depleted_world.set_block_at(depleted_target, AIR);
    let depleted_total = depleted_world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .audit()
        .unwrap()
        .total;
    let depleted = depleted_world
        .begin_gleam_working(
            [27; 16],
            "gleam-depletion-worker",
            depleted_source,
            depleted_wand.arcane_id,
            depleted_target,
            2,
            20,
            false,
        )
        .unwrap();
    settle_channel(&mut depleted_world, depleted.stable_id);
    let due = depleted_world.workings_state.as_ref().unwrap().active[&depleted.stable_id].due_tick;
    depleted_world.clock = due as f64 / 20.0 + 0.01;
    let outcomes = depleted_world.tick_workings();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].0.stable_id, depleted.stable_id);
    assert_eq!(outcomes[0].0.cue, crate::workings::WorkingCueKind::Complete);
    assert_eq!(
        depleted_world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        depleted_total
    );
}

#[test]
fn wand_authority_rejects_through_wall_and_out_of_range_targets() {
    let (mut world, source, wand) = workings_world("workings-wand-los");
    let wall = source.offset(1, 0, 0).unwrap();
    let hidden = source.offset(2, 0, 0).unwrap();
    world.set_block_at(wall, b(&world.reg, "base:stone"));
    world.set_block_at(hidden, b(&world.reg, "base:log"));
    assert!(
        world
            .begin_trace_working(
                [28; 16],
                "los-worker",
                source,
                wand.arcane_id,
                hidden,
                20,
                false,
            )
            .unwrap_err()
            .contains("line of sight")
    );
    let far = source.offset(12, 0, 0).unwrap();
    world.set_block_at(wall, AIR);
    world.set_block_at(far, AIR);
    assert!(
        world
            .begin_gleam_working(
                [28; 16],
                "range-worker",
                source,
                wand.arcane_id,
                far,
                1,
                20,
                false,
            )
            .unwrap_err()
            .contains("bounded reach")
    );
}

#[test]
fn transfer_circle_moves_only_exact_adjacent_charge_with_lower_bounded_loss() {
    let (mut world, controller, first_pos, second_pos, _) =
        ritual_world("workings-transfer-circle");
    let first = vessel_stack_at(&world, first_pos);
    let second = vessel_stack_at(&world, second_pos);
    charge_fixture_vessel(&mut world, first.arcane_id, 128);
    super::implements::drain_fixture_item_to_spark(&mut world, second_pos, second.arcane_id);
    let total_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let first_before = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .item_current_total(first.arcane_id)
        .unwrap();
    let second_before = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .item_current_total(second.arcane_id)
        .unwrap();
    assert!(first_before > second_before);

    let started = world
        .begin_transfer_circle_ritual([30; 16], "circle-worker", controller, 32)
        .unwrap();
    let payload = match &world.workings_state.as_ref().unwrap().active[&started.stable_id].effect {
        crate::workings::WorkingEffect::TransferCurrent { current, .. } => current.total(),
        _ => panic!("transfer circle reserved the wrong typed effect"),
    };
    settle_channel(&mut world, started.stable_id);
    world.clock = 5.0;
    let outcomes = world.tick_workings();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].0.cue, crate::workings::WorkingCueKind::Complete);
    let second_after = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .item_current_total(second.arcane_id)
        .unwrap();
    assert_eq!(second_after, second_before + payload);
    assert_eq!(
        vessel_stack_at(&world, first_pos).arcane_id,
        first.arcane_id
    );
    assert_eq!(
        vessel_stack_at(&world, second_pos).arcane_id,
        second.arcane_id
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        total_before
    );
    assert!(payload <= 32);
}

#[test]
fn rooting_bed_advances_loaded_or_unloaded_only_after_reserving_real_budgets() {
    let (mut world, controller, first_pos, _, _) = ritual_world("workings-rooting-bed");
    let source_vessel = vessel_stack_at(&world, first_pos);
    charge_fixture_vessel(&mut world, source_vessel.arcane_id, 512);
    let post = controller.offset(1, 0, -1).unwrap();
    world.set_block_authored_at(
        post,
        b(&world.reg, "base:containment_post"),
        "rooting focus post",
    );
    let plant = controller.offset(2, 0, 2).unwrap();
    let soil = plant.offset(0, -1, 0).unwrap();
    let water = plant.offset(-1, 0, 0).unwrap();
    let seed = b(&world.reg, "base:wheat_seeds");
    let next = world.reg.block(seed).crop_next.unwrap();
    world.set_block_meta_at(
        soil,
        b(&world.reg, "base:farmland"),
        crate::world::soil::soil_meta(42, 0),
    );
    world.set_block_at(plant, seed);
    world.set_block_water_at(water, world.reg.water_block(0), 0, 0);
    let water_before = world.water_mass_at(water).unwrap().water_hu;
    let fertility_before = world.fertility_at_pos(soil);
    let current_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;

    let started = world
        .begin_rooting_bed_ritual([31; 16], "bed-worker", controller)
        .unwrap();
    assert_eq!(
        world.get_block_at(plant),
        seed,
        "reservation is not early growth"
    );
    assert_eq!(world.water_mass_at(water).unwrap().water_hu, water_before);
    assert_eq!(world.fertility_at_pos(soil), fertility_before);
    settle_channel(&mut world, started.stable_id);
    save_world(&mut world);
    world.unload_chunk(plant.chunk());
    assert!(world.chunk(plant.chunk()).is_none());
    world.clock = 13.0;
    let outcomes = world.tick_workings();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].0.stable_id, started.stable_id);
    assert_eq!(
        outcomes[0].0.cue,
        crate::workings::WorkingCueKind::Complete,
        "{}",
        outcomes[0].0.message
    );
    assert_eq!(world.get_block_at(plant), next);
    assert_eq!(
        world.water_mass_at(water).unwrap().water_hu,
        water_before - 32
    );
    assert_eq!(world.fertility_at_pos(soil), fertility_before - 1);
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        current_before
    );
}

#[test]
fn active_unloaded_ritual_resumes_after_process_restart_and_settles_once() {
    let (mut world, controller, first_pos, _, _) = ritual_world("workings-ritual-restart");
    let dir = world.save_dir_for_saving();
    let source_vessel = vessel_stack_at(&world, first_pos);
    charge_fixture_vessel(&mut world, source_vessel.arcane_id, 512);
    let post = controller.offset(1, 0, -1).unwrap();
    world.set_block_authored_at(
        post,
        b(&world.reg, "base:containment_post"),
        "restart rooting focus post",
    );
    let plant = controller.offset(2, 0, 2).unwrap();
    let soil = plant.offset(0, -1, 0).unwrap();
    let water = plant.offset(-1, 0, 0).unwrap();
    let seed = b(&world.reg, "base:wheat_seeds");
    let next = world.reg.block(seed).crop_next.unwrap();
    world.set_block_meta_at(
        soil,
        b(&world.reg, "base:farmland"),
        crate::world::soil::soil_meta(42, 0),
    );
    world.set_block_at(plant, seed);
    world.set_block_water_at(water, world.reg.water_block(0), 0, 0);
    save_world(&mut world);

    let water_before = world.water_mass_at(water).unwrap().water_hu;
    let fertility_before = world.fertility_at_pos(soil);
    let current_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let started = world
        .begin_rooting_bed_ritual([44; 16], "restart-bed-worker", controller)
        .unwrap();
    settle_channel(&mut world, started.stable_id);
    let due_tick = world.workings_state.as_ref().unwrap().active[&started.stable_id].due_tick;
    drop(world);

    let mut reloaded = World::load_or_create(dir, base_reg()).unwrap();
    let resumed = &reloaded.workings_state.as_ref().unwrap().active[&started.stable_id];
    assert_eq!(resumed.phase, crate::workings::WorkingPhase::Active);
    assert_eq!(resumed.due_tick, due_tick);
    assert!(reloaded.chunk(plant.chunk()).is_none());
    reloaded.clock = due_tick as f64 / 20.0 + 0.1;
    let outcomes = reloaded.tick_workings();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].0.stable_id, started.stable_id);
    assert_eq!(outcomes[0].0.cue, crate::workings::WorkingCueKind::Complete);
    assert_eq!(reloaded.get_block_at(plant), next);
    assert_eq!(
        reloaded.water_mass_at(water).unwrap().water_hu,
        water_before - 32
    );
    assert_eq!(reloaded.fertility_at_pos(soil), fertility_before - 1);
    assert_eq!(
        reloaded
            .workings_state
            .as_ref()
            .unwrap()
            .history
            .iter()
            .filter(|event| event.id == started.stable_id)
            .count(),
        1
    );
    assert!(reloaded.complete_working(started.stable_id).is_err());
    assert_eq!(
        reloaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        current_before
    );
}

#[test]
fn ward_is_closed_supplied_ire_costed_and_breaks_without_rewriting_construction() {
    let (mut world, controller, first_pos, _, _) = ritual_world("workings-ward");
    let source_vessel = vessel_stack_at(&world, first_pos);
    charge_fixture_vessel(&mut world, source_vessel.arcane_id, 768);
    let conductor = b(&world.reg, "base:arcane_conductor");
    let mut boundary = Vec::new();
    for du in -3i32..=3 {
        for dv in -3i32..=3 {
            if du.abs().max(dv.abs()) == 3 {
                let pos = controller.offset(du, 0, dv).unwrap();
                world.set_block_authored_at(pos, conductor, "ward fixture boundary");
                boundary.push(pos);
            }
        }
    }
    world.add_ire(80.0);
    let ire_before = world.ire;
    let current_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let started = world
        .begin_ward_boundary_ritual([32; 16], "ward-worker", controller)
        .unwrap();
    settle_channel(&mut world, started.stable_id);
    assert!(world.resist_supernatural_pressure_at(controller, "warden", 1));
    let transaction = &world.workings_state.as_ref().unwrap().active[&started.stable_id];
    assert!(matches!(
        transaction.effect,
        crate::workings::WorkingEffect::Ward {
            pressure_units: 5,
            ire_before_millipoints: 80_000,
            ire_after_millipoints: 80_000,
            ..
        }
    ));
    assert_eq!(world.ire, ire_before, "a ward never lowers stored Ire");

    let inside = controller.offset(0, 2, 0).unwrap();
    world.set_block_at(inside, AIR);
    world.spawn_projectile(crate::mobs::Projectile {
        stable_id: 88_001,
        pos: inside.entity_center(),
        vel: glam::Vec3::ZERO,
        tile: 0,
        damage: 2.0,
        age: 0.0,
        from_player: false,
        drop_item: None,
        owner: 0,
    });
    let players = [crate::server::PlayerCtx {
        id: 7,
        pos: inside.entity_center(),
        spawn: inside.entity_center(),
        attackable: true,
        aggro_mod: 0.0,
        quiet_charm: None,
    }];
    assert!(world.tick_projectiles(&players, 0.05).is_empty());
    assert!(world.projectiles().is_empty());
    let projectile_pressure =
        match &world.workings_state.as_ref().unwrap().active[&started.stable_id].effect {
            crate::workings::WorkingEffect::Ward {
                pressure_kind,
                pressure_units,
                ..
            } => {
                assert_eq!(pressure_kind, "projectile");
                *pressure_units
            }
            _ => unreachable!(),
        };
    assert!(projectile_pressure > 5);
    world.spawn_projectile(crate::mobs::Projectile {
        stable_id: 88_002,
        pos: inside.entity_center(),
        vel: glam::Vec3::ZERO,
        tile: 0,
        damage: 2.0,
        age: 0.0,
        from_player: true,
        drop_item: None,
        owner: 7,
    });
    assert!(world.tick_projectiles(&players, 0.05).is_empty());
    assert_eq!(
        world.projectiles().len(),
        1,
        "a ward may not stop an ordinary player's projectile"
    );
    world.replace_projectiles(Vec::new());

    // Real planetary wake and dross state applies pressure once the next
    // authoritative Current transport step lands. Neither environmental
    // reservoir is consumed by the ward.
    let region = world
        .planet_atlas()
        .unwrap()
        .atlas_pos(controller.surface());
    let geography = world.arcane_geography.as_mut().unwrap();
    let geographic_total_before = geography.audit().unwrap().accounted_total;
    assert!(geography.foul_ambient(region, 128).unwrap() > 0);
    geography.begin_wake(vec![region], 64, 64, 1).unwrap();
    let next_step = geography.dynamic.completed_steps + 1;
    world.clock = (next_step * crate::arcane_geography::ARCANE_GEOGRAPHY_SIMULATION_SECONDS) as f64;
    for _ in 0..8 {
        world.tick_arcane_geography(4_096).unwrap();
        if world
            .arcane_geography
            .as_ref()
            .unwrap()
            .dynamic
            .completed_steps
            >= next_step
        {
            break;
        }
    }
    let transaction = &world.workings_state.as_ref().unwrap().active[&started.stable_id];
    assert!(matches!(
        transaction.effect,
        crate::workings::WorkingEffect::Ward {
            ref pressure_kind,
            pressure_units,
            ..
        } if pressure_kind == "dross" && pressure_units > projectile_pressure
    ));
    assert_eq!(
        world
            .arcane_geography
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .accounted_total,
        geographic_total_before
    );
    assert_eq!(world.ire, ire_before);

    let broken = boundary[0];
    let untouched = boundary[1];
    world.set_block_at(broken, AIR);
    assert!(!world.resist_supernatural_pressure_at(controller, "warden", 1));
    let outcomes = world.tick_workings();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].0.cue, crate::workings::WorkingCueKind::Strain);
    assert_eq!(world.get_block_at(broken), AIR);
    assert_eq!(world.get_block_at(untouched), conductor);
    assert_eq!(world.ire, ire_before);
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        current_before
    );
}

#[test]
fn ward_overload_is_deterministic_exhausts_supply_and_never_changes_ire() {
    let (mut world, controller, first_pos, _, _) = ritual_world("workings-ward-overload");
    let source_vessel = vessel_stack_at(&world, first_pos);
    charge_fixture_vessel(&mut world, source_vessel.arcane_id, 768);
    let conductor = b(&world.reg, "base:arcane_conductor");
    let mut boundary = Vec::new();
    for du in -3i32..=3 {
        for dv in -3i32..=3 {
            if du.abs().max(dv.abs()) == 3 {
                let pos = controller.offset(du, 0, dv).unwrap();
                world.set_block_authored_at(pos, conductor, "ward overload boundary");
                boundary.push(pos);
            }
        }
    }
    world.add_ire(80.0);
    let ire_before = world.ire;
    let current_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let started = world
        .begin_ward_boundary_ritual([45; 16], "overload-worker", controller)
        .unwrap();
    settle_channel(&mut world, started.stable_id);
    assert!(!world.resist_supernatural_pressure_at(controller, "warden", u64::MAX));
    assert!(
        !world
            .workings_state
            .as_ref()
            .unwrap()
            .active
            .contains_key(&started.stable_id)
    );
    let events = world
        .workings_state
        .as_ref()
        .unwrap()
        .history
        .iter()
        .filter(|event| event.id == started.stable_id)
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].outcome, "interrupted");
    assert_eq!(world.ire, ire_before);
    assert!(
        boundary
            .iter()
            .all(|pos| world.get_block_at(*pos) == conductor)
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        current_before
    );
}

#[test]
fn settling_rite_slows_one_process_routes_dross_and_wears_the_real_vessel() {
    let (mut world, controller, first_pos, second_pos, wand) = ritual_world("workings-settling");
    let source_vessel = vessel_stack_at(&world, first_pos);
    charge_fixture_vessel(&mut world, source_vessel.arcane_id, 512);
    let source = controller.offset(0, 2, 0).unwrap();
    world.set_block_at(source, AIR);
    let target = source.offset(2, 0, 0).unwrap();
    world.set_block_at(target, AIR);
    let process = world
        .begin_gleam_working(
            [33; 16],
            "process-worker",
            source,
            wand.arcane_id,
            target,
            4,
            20 * 30,
            false,
        )
        .unwrap();
    settle_channel(&mut world, process.stable_id);
    let due_before = world.workings_state.as_ref().unwrap().active[&process.stable_id].due_tick;
    let process_dross_before = world.workings_state.as_ref().unwrap().active[&process.stable_id]
        .dross_current
        .total();
    assert!(process_dross_before > 0);

    let rite = world
        .begin_settling_rite([34; 16], "settling-worker", controller)
        .unwrap();
    settle_channel(&mut world, rite.stable_id);
    assert!(
        world.workings_state.as_ref().unwrap().active[&process.stable_id].due_tick > due_before
    );
    let dross_vessel_id =
        match world.workings_state.as_ref().unwrap().active[&rite.stable_id].effect {
            crate::workings::WorkingEffect::Settle {
                dross_vessel_id, ..
            } => dross_vessel_id,
            _ => panic!("settling rite reserved the wrong typed effect"),
        };
    assert!(
        [
            vessel_stack_at(&world, first_pos).arcane_id,
            vessel_stack_at(&world, second_pos).arcane_id
        ]
        .contains(&dross_vessel_id)
    );
    let dross_before = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .item_dross_total(dross_vessel_id);
    let wear_before = world
        .implements_state
        .as_ref()
        .unwrap()
        .instance(dross_vessel_id)
        .unwrap()
        .wear;
    let total_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    world.release_working(process.stable_id).unwrap();
    let routed = world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .item_dross_total(dross_vessel_id)
        .saturating_sub(dross_before);
    assert_eq!(routed, process_dross_before - process_dross_before / 4);
    let rite_tx = &world.workings_state.as_ref().unwrap().active[&rite.stable_id];
    assert!(matches!(
        rite_tx.effect,
        crate::workings::WorkingEffect::Settle {
            dross_routed,
            stabilizer_wear,
            ..
        } if dross_routed == routed && stabilizer_wear > 0
    ));
    assert!(
        world
            .implements_state
            .as_ref()
            .unwrap()
            .instance(dross_vessel_id)
            .unwrap()
            .wear
            > wear_before
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        total_before
    );

    let containment = controller.offset(0, 0, -1).unwrap();
    world.set_block_at(containment, AIR);
    let outcomes = world.tick_workings();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].0.stable_id, rite.stable_id);
    assert_eq!(outcomes[0].0.cue, crate::workings::WorkingCueKind::Strain);
}

#[test]
fn crash_after_current_settlement_replays_world_effect_exactly_once_on_restart() {
    let (mut world, dir, source, wand) = persistent_workings_world("workings-crash-replay");
    let fuel = source.offset(2, 0, 0).unwrap();
    let fire_cell = fuel.offset(0, 1, 0).unwrap();
    world.set_block_authored_at(fuel, b(&world.reg, "base:log"), "crash fixture fuel");
    world.set_block_at(fire_cell, AIR);
    save_world(&mut world);
    let total_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let started = world
        .begin_kindle_working(
            [35; 16],
            "crash-worker",
            source,
            wand.arcane_id,
            fuel,
            fire_cell,
            false,
        )
        .unwrap();
    world.fail_chunk_save_for_test(fire_cell.chunk(), true);
    let error = world.complete_working(started.stable_id).unwrap_err();
    assert!(error.contains("injected chunk save failure"));
    let pending = &world.workings_state.as_ref().unwrap().active[&started.stable_id];
    assert_eq!(pending.phase, crate::workings::WorkingPhase::PendingApply);
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&crate::arcane::ArcaneOwner::Working(started.stable_id))
            .is_none(),
        "Current settlement committed before the idempotent world half"
    );
    drop(world);

    let mut reloaded = World::load_or_create(dir, base_reg()).unwrap();
    reloaded.ensure_chunk(fire_cell.chunk());
    assert_eq!(
        reloaded.get_block_at(fire_cell),
        b(&reloaded.reg, "base:fire")
    );
    assert!(
        !reloaded
            .workings_state
            .as_ref()
            .unwrap()
            .active
            .contains_key(&started.stable_id)
    );
    assert_eq!(
        reloaded
            .workings_state
            .as_ref()
            .unwrap()
            .history
            .iter()
            .filter(|event| event.id == started.stable_id)
            .count(),
        1
    );
    assert!(reloaded.complete_working(started.stable_id).is_err());
    assert_eq!(
        reloaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        total_before
    );
}

#[test]
fn dropped_item_nudge_crash_replays_the_host_entity_exactly_once() {
    let (mut world, dir, source, wand) =
        persistent_workings_world("workings-nudge-drop-crash-replay");
    let drop_pos = source
        .entity_center()
        .translated(glam::Vec3::new(2.0, 1.0, 0.0))
        .unwrap()
        .pos;
    let item = it(&world.reg, "base:stone");
    let stable_id = world.spawn_loose_item(crate::entity::ItemEntity::new(
        drop_pos,
        glam::Vec3::ZERO,
        item,
        8,
    ));
    save_world(&mut world);
    let total_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let started = world
        .begin_nudge_working(
            [37; 16],
            "drop-crash-worker",
            source,
            wand.arcane_id,
            stable_id,
            3,
            false,
        )
        .unwrap();
    world.fail_loose_item_save_for_test(true);
    let error = world.complete_working(started.stable_id).unwrap_err();
    assert!(
        error.contains("injected loose-item sidecar save failure"),
        "unexpected completion error: {error}"
    );
    assert_eq!(
        world.workings_state.as_ref().unwrap().active[&started.stable_id].phase,
        crate::workings::WorkingPhase::PendingApply
    );
    let applied_velocity = world
        .loose_items()
        .iter()
        .find(|drop| drop.stable_id == stable_id)
        .unwrap()
        .vel;
    assert!(applied_velocity.x > 0.0);
    drop(world);

    let reloaded = World::load_or_create(dir, base_reg()).unwrap();
    let replayed = reloaded
        .loose_items()
        .iter()
        .find(|drop| drop.stable_id == stable_id)
        .unwrap();
    assert_eq!(replayed.vel, applied_velocity);
    assert!(
        !reloaded
            .workings_state
            .as_ref()
            .unwrap()
            .active
            .contains_key(&started.stable_id)
    );
    let matching = reloaded
        .workings_state
        .as_ref()
        .unwrap()
        .history
        .iter()
        .filter(|event| event.id == started.stable_id)
        .collect::<Vec<_>>();
    assert_eq!(matching.len(), 1);
    assert_eq!(matching[0].outcome, "crash_replay_complete");
    assert_eq!(
        reloaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        total_before
    );
}

#[test]
fn restart_interrupts_unowned_wand_channel_once_and_conserves_its_reservation() {
    let (mut world, dir, source, wand) = persistent_workings_world("workings-restart-interrupt");
    let target = source.offset(2, 0, 0).unwrap();
    world.set_block_at(target, AIR);
    save_world(&mut world);
    let total_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let started = world
        .begin_gleam_working(
            [36; 16],
            "restart-worker",
            source,
            wand.arcane_id,
            target,
            4,
            20 * 30,
            false,
        )
        .unwrap();
    settle_channel(&mut world, started.stable_id);
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&crate::arcane::ArcaneOwner::Working(started.stable_id))
            .is_some()
    );
    drop(world);

    let reloaded = World::load_or_create(dir, base_reg()).unwrap();
    assert!(
        !reloaded
            .workings_state
            .as_ref()
            .unwrap()
            .active
            .contains_key(&started.stable_id)
    );
    let events = reloaded
        .workings_state
        .as_ref()
        .unwrap()
        .history
        .iter()
        .filter(|event| event.id == started.stable_id)
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].outcome, "interrupted");
    assert!(
        reloaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&crate::arcane::ArcaneOwner::Working(started.stable_id))
            .is_none()
    );
    assert_eq!(
        reloaded
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        total_before
    );
}

#[test]
fn hosted_actor_death_or_disconnect_interrupts_every_reservation_exactly_once() {
    let (mut world, source, wand) = workings_world("workings-host-lifecycle");
    let actor = [38; 16];
    let total_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let first = world
        .begin_gleam_working(
            actor,
            "host-lifecycle-worker",
            source,
            wand.arcane_id,
            source,
            3,
            20 * 30,
            false,
        )
        .unwrap();
    settle_channel(&mut world, first.stable_id);

    let results = world.interrupt_actor_workings(actor).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].stable_id, first.stable_id);
    assert_eq!(results[0].cue, crate::workings::WorkingCueKind::Strain);
    assert!(world.interrupt_actor_workings(actor).unwrap().is_empty());
    assert!(world.interrupt_working(first.stable_id).is_err());
    assert_eq!(
        world
            .workings_state
            .as_ref()
            .unwrap()
            .history
            .iter()
            .filter(|event| event.id == first.stable_id)
            .count(),
        1
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        total_before
    );
}

#[test]
fn unloaded_wand_target_interrupts_once_without_rewriting_the_chunk() {
    let (mut world, source, wand) = workings_world("workings-unload-lifecycle");
    let target = source.offset(2, 0, 0).unwrap();
    world.set_block_at(target, AIR);
    let target_before = world.get_block_at(target);
    let total_before = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
    let started = world
        .begin_gleam_working(
            [39; 16],
            "unload-worker",
            source,
            wand.arcane_id,
            target,
            3,
            20 * 30,
            false,
        )
        .unwrap();
    settle_channel(&mut world, started.stable_id);
    world.unload_chunk(target.chunk());

    let first = world.tick_workings();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].0.stable_id, started.stable_id);
    assert_eq!(first[0].0.cue, crate::workings::WorkingCueKind::Strain);
    assert!(world.tick_workings().is_empty());
    world.ensure_chunk(target.chunk());
    assert_eq!(world.get_block_at(target), target_before);
    assert_eq!(
        world
            .workings_state
            .as_ref()
            .unwrap()
            .history
            .iter()
            .filter(|event| event.id == started.stable_id)
            .count(),
        1
    );
    assert_eq!(
        world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
        total_before
    );
}
