//! Persistence scenarios.

use super::*;

#[test]
fn alchemy_state_backup_recovery_and_wire_budget_are_bounded() {
    let dir = tmp_dir("alchemy-state-backup");
    let mut state = AlchemyState::load_or_initialize(&dir, 42).unwrap();
    state.save().unwrap();
    let first_next = state.next_operation_id;
    state.allocate_operation_id().unwrap();
    state.save().unwrap();
    std::fs::write(dir.join(crate::alchemy::ALCHEMY_FILE), b"corrupt").unwrap();
    let recovered = AlchemyState::load_or_initialize(&dir, 42).unwrap();
    assert_eq!(recovered.next_operation_id, first_next);
    assert!(recovered.encode().unwrap().len() as u64 <= crate::alchemy::MAX_ALCHEMY_FILE_BYTES);

    let cue = AlchemyCue {
        pos: bp(1, 80, 1),
        installation_id: 1,
        batch_id: 1,
        revision: 1,
        kind: AlchemyCueKind::Pour,
        intensity: 100,
        color: [1, 2, 3],
        message: "bounded".into(),
    };
    let packet = crate::net::S2C::AlchemyEvent(cue);
    let encoded = postcard::to_allocvec(&packet).unwrap();
    assert!(encoded.len() < 1_024);
}

#[test]
fn alchemy_wire_requests_carry_intent_not_client_authored_chemistry() {
    let pos = bp(3, 90, -4);
    let operate = crate::net::C2S::OperateAlchemy {
        pos,
        expected_revision: Some(17),
        action: ApparatusAction::SetHeat {
            temperature_millic: 42_000,
        },
    };
    let bytes = postcard::to_allocvec(&operate).unwrap();
    assert!(bytes.len() < 512);
    let decoded: crate::net::C2S = postcard::from_bytes(&bytes).unwrap();
    match decoded {
        crate::net::C2S::OperateAlchemy {
            pos: decoded_pos,
            expected_revision,
            action: ApparatusAction::SetHeat { temperature_millic },
        } => {
            assert_eq!(decoded_pos, pos);
            assert_eq!(expected_revision, Some(17));
            assert_eq!(temperature_millic, 42_000);
        }
        other => panic!("wrong alchemy intent packet: {other:?}"),
    }

    let apply = crate::net::C2S::UsePreparation {
        slot: 6,
        target: crate::alchemy::AlchemyTarget::Plot(pos),
    };
    let bytes = postcard::to_allocvec(&apply).unwrap();
    assert!(bytes.len() < 256);
    let decoded: crate::net::C2S = postcard::from_bytes(&bytes).unwrap();
    match decoded {
        crate::net::C2S::UsePreparation {
            slot,
            target: crate::alchemy::AlchemyTarget::Plot(decoded_pos),
        } => {
            assert_eq!(slot, 6);
            assert_eq!(decoded_pos, pos);
        }
        other => panic!("wrong preparation intent packet: {other:?}"),
    }
}

#[test]
fn large_alchemy_census_stays_inside_save_wire_and_server_tick_budgets() {
    use std::time::{Duration, Instant};

    use crate::alchemy::{
        AlchemyApparatusState, AlchemyAuditEvent, ApparatusKind, MAX_ALCHEMY_APPARATUS,
        MAX_ALCHEMY_CONTAINERS, MAX_ALCHEMY_FILE_BYTES, MAX_ALCHEMY_HISTORY, MAX_ALCHEMY_STATUSES,
    };
    use crate::planet::Face;
    use crate::workings::WaterCarrier;

    let dir = tmp_dir("alchemy-large-census");
    let mut state = AlchemyState::load_or_initialize(&dir, 42).unwrap();
    let definition = crate::registry::load(std::path::Path::new("__no_alchemy_budget_mods__"))
        .preparations["base:clear_eye_tincture"]
        .clone();

    for index in 0..MAX_ALCHEMY_APPARATUS {
        let pos = crate::planet::BlockPos::new(
            Face::PosZ,
            u16::try_from(index % 512).unwrap(),
            80,
            u16::try_from(index / 512).unwrap(),
        )
        .unwrap();
        let installation_id = u64::try_from(index).unwrap() + 1;
        state.apparatus.insert(
            pos,
            AlchemyApparatusState {
                installation_id,
                kind: ApparatusKind::Mortar,
                pos,
                revision: 1,
                integrity_permille: 1_000,
                cleanliness_permille: 1_000,
                temperature_millic: 20_000,
                agitation: AgitationKind::Still,
                batch: None,
                residue_materials: Default::default(),
                filter_burden: 0,
                filter_medium: None,
                filter_owner_id: 0,
                filter_medium_materials: Default::default(),
                last_operator: [1; 16],
            },
        );
    }
    for index in 0..MAX_ALCHEMY_STATUSES {
        let id = u64::try_from(index).unwrap() + 1;
        let mut actor = [0u8; 16];
        actor[..8].copy_from_slice(&id.to_le_bytes());
        state.statuses.insert(
            actor,
            vec![ActivePreparationStatus {
                status_id: id,
                preparation_id: definition.id.clone(),
                definition_version: definition.version,
                source_batch: id,
                actor,
                dose_volume_units: definition.dose_units,
                active_current: Current::default(),
                dross_current: Current::default(),
                started_tick: 0,
                last_tick: 0,
                due_tick: 10_000,
                recovery_until_tick: 20_000,
                stack_group: definition.stack_group.clone(),
                completed_units: 0,
                refresh_count: 0,
                overdose_until_tick: 0,
            }],
        );
    }
    for index in 0..MAX_ALCHEMY_HISTORY {
        let id = u64::try_from(index).unwrap() + 1;
        state.history.push_back(AlchemyAuditEvent {
            operation_id: id,
            installation_id: id,
            batch_id: id,
            actor: [1; 16],
            action: "budget_sample".into(),
            preparation_id: definition.id.clone(),
            volume_units: definition.dose_units,
            current_units: definition.charge_units,
            dross_units: definition.dross_units,
            tick: id,
            note: "bounded representative audit event".into(),
        });
    }
    state.next_installation_id = u64::try_from(MAX_ALCHEMY_APPARATUS).unwrap() + 1;
    state.next_batch_id = u64::try_from(MAX_ALCHEMY_STATUSES).unwrap() + 1;
    state.next_status_id = u64::try_from(MAX_ALCHEMY_STATUSES).unwrap() + 1;
    state.next_operation_id = u64::try_from(MAX_ALCHEMY_HISTORY).unwrap() + 1;

    let fixed_bytes = state.encode().unwrap().len();
    const REPRESENTATIVE_CONTAINERS: usize = 8_192;
    for index in 0..REPRESENTATIVE_CONTAINERS {
        let id = u64::try_from(index).unwrap() + 1;
        state.containers.insert(
            id,
            PreparationDose {
                container_id: id,
                preparation_id: definition.id.clone(),
                definition_version: definition.version,
                item_name: definition.output_item.clone(),
                liquid: ExactLiquid {
                    carrier: Some(CarrierKind::Alcohol),
                    volume_units: definition.dose_units,
                    water: Default::default(),
                    carrier_state: WaterCarrier {
                        thermal_millic_hu: 20_000 * i64::try_from(definition.dose_units).unwrap(),
                        dross_subunits: 0,
                    },
                    solutes: std::collections::BTreeMap::from([(
                        definition.solvent_item.clone(),
                        definition.dose_units,
                    )]),
                },
                vessel_materials: Default::default(),
                materials: Default::default(),
                current_units: 1,
                dross_units: 0,
                born_tick: 1,
                expires_tick: 100_000,
                last_storage_tick: 1,
                outcome: BatchOutcome::Ready,
                source_installation: 1,
                source_batch: 1,
            },
        );
    }
    let encode_started = Instant::now();
    let encoded = state.encode().unwrap();
    assert!(
        encode_started.elapsed() < Duration::from_secs(5),
        "representative large alchemy census took too long to validate and encode"
    );
    let sampled_container_bytes = encoded.len().saturating_sub(fixed_bytes);
    // Add eight bytes per entry for larger varints and the final map-length
    // prefix. This projects the supported full container ceiling without
    // allocating 65k heap-heavy Rust records in an ordinary test runner.
    let projected_per_container = sampled_container_bytes.div_ceil(REPRESENTATIVE_CONTAINERS) + 8;
    let projected_full_bytes = fixed_bytes
        + projected_per_container
            .checked_mul(MAX_ALCHEMY_CONTAINERS)
            .unwrap();
    assert!(
        u64::try_from(projected_full_bytes).unwrap() <= MAX_ALCHEMY_FILE_BYTES,
        "declared full container census projects to {projected_full_bytes} bytes"
    );

    let mut world = crate::tests::implements::embodied_implements_world("alchemy-budget-ticks");
    world.alchemy_state = Some(state);
    world.set_simulation_clock(1.0);
    let maintenance_started = Instant::now();
    world.tick_alchemy(128).unwrap();
    assert!(
        maintenance_started.elapsed() < Duration::from_secs(1),
        "bounded maintenance scaled with the whole census instead of its visit budget"
    );
    let mut first_actor = [0u8; 16];
    first_actor[..8].copy_from_slice(&1u64.to_le_bytes());
    let status_started = Instant::now();
    world
        .tick_preparation_statuses(first_actor, bp(8, 100, 8), PreparationPhysiology::default())
        .unwrap();
    assert!(
        status_started.elapsed() < Duration::from_secs(1),
        "one actor status tick cloned or scanned the whole alchemy census"
    );
}
