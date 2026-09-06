//! Protocol scenarios.

use super::*;

#[test]
fn net_protocol_round_trips() {
    use crate::net::{C2S, S2C, decode, encode};
    let c2s = [
        C2S::Hello {
            protocol: 2,
            display_name: "DOLL".into(),
            device_public_key: [3; 32],
            client_nonce: [4; 32],
            content_hash: 42,
            style: 0x0102_0304,
        },
        C2S::Authenticate {
            signature: vec![5; 64],
            atproto: None,
        },
        C2S::Move {
            pos: ep(Vec3::new(1.5, 80.0, -3.5)),
            yaw: 1.2,
            hotbar: 2,
            sprint: true,
        },
        C2S::Break { pos: bp(1, 2, 3) },
        C2S::Place { pos: bp(-9, 70, 4) },
        C2S::AttackMob {
            id: 3,
            heavy: false,
        },
        C2S::FeedMob { id: 12 },
        C2S::BrushBlock { pos: bp(4, 30, -2) },
        C2S::BeginObserve {
            target: crate::net::DiscoveryTargetSnap::Block(bp(4, 30, -2)),
        },
        C2S::Observe {
            target: crate::net::DiscoveryTargetSnap::Block(bp(4, 30, -2)),
            ledger_slot: 3,
            calibration_slot: Some(4),
            label: Some("north spring".into()),
        },
        C2S::CopyObservation {
            writing_pos: bp(4, 30, -1),
            source: crate::net::RecordHolderSnap::Inventory { slot: 3 },
            record_id: 17,
            destination: crate::net::RecordHolderSnap::Folio { pos: bp(5, 30, -1) },
            include_location: false,
        },
        C2S::BeginExperiment {
            pos: bp(5, 30, -2),
            kind: crate::discovery::ExperimentKind::Conductivity,
        },
        C2S::SetExperimentItem {
            pos: bp(5, 30, -2),
            slot: 5,
        },
        C2S::RunExperiment {
            pos: bp(5, 30, -2),
            kind: crate::discovery::ExperimentKind::Conductivity,
            ledger_slot: 3,
            calibration_slot: Some(4),
        },
        C2S::OperateWorking {
            working_id: "base:nudge".into(),
            held_instance: 0x1234_5678_9abc_def0,
            target: crate::workings::WorkingTargetIntent::Entity {
                stable_id: (1u64 << 62) + 17,
            },
            intent: crate::workings::WorkingIntent::StartForced,
        },
        C2S::OperateWorking {
            working_id: "base:nudge".into(),
            held_instance: 0x1234_5678_9abc_def0,
            target: crate::workings::WorkingTargetIntent::Entity {
                stable_id: (1u64 << 62) + 17,
            },
            intent: crate::workings::WorkingIntent::Hold,
        },
        C2S::OperateWorking {
            working_id: "base:nudge".into(),
            held_instance: 0x1234_5678_9abc_def0,
            target: crate::workings::WorkingTargetIntent::Entity {
                stable_id: (1u64 << 62) + 17,
            },
            intent: crate::workings::WorkingIntent::Release,
        },
        C2S::OperateWorking {
            working_id: "base:nudge".into(),
            held_instance: 0x1234_5678_9abc_def0,
            target: crate::workings::WorkingTargetIntent::Entity {
                stable_id: (1u64 << 62) + 17,
            },
            intent: crate::workings::WorkingIntent::Cancel,
        },
        C2S::ContainerClick {
            pos: bp(1, 2, 3),
            slot: 4,
            right: true,
        },
        C2S::CloseContainer,
        C2S::Chat("hello wild".into()),
        C2S::Moderate {
            target: 9,
            action: crate::net::ModerationAction::Mute { seconds: 600 },
        },
        C2S::EntryReady,
        C2S::SleepRequest,
    ];
    for m in &c2s {
        let bytes = encode(m);
        assert!(!bytes.is_empty());
        let back: C2S = decode(&bytes).expect("c2s decodes");
        assert_eq!(format!("{m:?}"), format!("{back:?}"));
    }
    let s2c = [
        S2C::Challenge {
            nonce: [7; 32],
            server_fingerprint: [8; 32],
            identity_policy: crate::identity::IdentityPolicy::Local,
            admission_policy: crate::identity::AdmissionPolicy::Open,
        },
        S2C::BlockSet {
            pos: crate::planet::BlockPos::from_centered(crate::planet::Face::PosZ, 1, 2, 3)
                .unwrap(),
            id: 9,
            meta: 173,
            salt_mass: 44_321,
            soil_salinity: 91,
        },
        S2C::TimeIre {
            time: 0.5,
            ire: 33.0,
            day: 7,
        },
        S2C::WeatherCells {
            side: 8,
            cells: vec![(
                crate::planet_atlas::AtlasPos {
                    face: crate::planet::Face::PosZ,
                    u: 7,
                    v: 4,
                },
                crate::planet_atlas::LocalWeatherSample::default(),
            )],
        },
        S2C::ArcaneCue {
            bands: [2, 1],
            dominant: 4,
            ecology: Some(("Rainbells fold shut.".into(), false)),
        },
        S2C::Chat {
            from: "a".into(),
            msg: "b".into(),
        },
        S2C::Sleep {
            sleeping: 1,
            present: 3,
        },
        S2C::Chunk {
            face: crate::planet::Face::PosZ as u8,
            u: 0,
            v: 0,
            rle: vec![1, 2, 3],
        },
        S2C::EntryManifest {
            spawn: ep(Vec3::new(1.5, 80.0, -3.5)),
            required: vec![tchunk(0, 0), tchunk(1, 0)],
        },
        S2C::EntryProgress {
            resident: 4,
            total: 9,
        },
        S2C::EntryAccepted,
        S2C::HeldResult(Some(crate::net::StackSnap {
            item: 2,
            count: 1,
            durability: 40,
            arcane_id: 0,
            current_units: 0,
        })),
        S2C::WorkingResult(crate::workings::WorkingResult {
            success: true,
            stable_id: 91,
            phase: Some(crate::workings::WorkingPhase::Active),
            cue: crate::workings::WorkingCueKind::Active,
            warning_band: 2,
            message: "Nudge settles under visible strain.".into(),
        }),
        S2C::WorkingEvent(crate::workings::WorkingCue {
            stable_id: 91,
            working_id: "base:nudge".into(),
            handler: crate::workings::WorkingHandler::Nudge,
            source: bp(1, 70, 1),
            path: vec![bp(1, 70, 1), bp(3, 70, 1)],
            kind: crate::workings::WorkingCueKind::Active,
            warning_band: 2,
            completion_permille: 350,
        }),
        S2C::Mobs(crate::net::Snapshot::whole(
            1,
            vec![crate::net::MobSnap {
                id: 5,
                species: 1,
                pos: ep(Vec3::new(1.0, 2.0, 3.0)),
                yaw: 0.5,
                growth: 1.0,
                hurt: 0.0,
                health: 6.0,
                fed: true,
            }],
        )),
        S2C::ViewDistance { chunks: 8 },
    ];
    for m in &s2c {
        let back: S2C = decode(&encode(m)).expect("s2c decodes");
        assert_eq!(format!("{m:?}"), format!("{back:?}"));
    }
}

#[test]
fn arcane_interest_updates_stay_within_the_network_budget() {
    use crate::inventory::TOTAL_SLOTS;
    use crate::net::{DATAGRAM_FLOOR, S2C, encode};

    let cue = encode(&S2C::ArcaneCue {
        bands: [4, 4],
        dominant: 6,
        ecology: Some((
            "Lantern reeds bend over the spring margin; their amber light is steady, while the Current beneath them carries a muted tidal cadence. Ashlace farther upslope has caught a trace of dross in its grey-green threads without making it vanish."
                .into(),
            true,
        )),
    });
    assert!(
        cue.len() <= DATAGRAM_FLOOR,
        "the longest normal qualitative ecology cue is {} bytes",
        cue.len()
    );

    // A player can inspect only inventory, armor, and cursor custody. Use a
    // deliberately conservative armor allowance so a future slot expansion
    // fails this budget test before it silently bloats every host update.
    let inspectable_slots = TOTAL_SLOTS + 8 + 1;
    let items = encode(&S2C::ArcaneItems {
        reset: true,
        charges: (1..=inspectable_slots as u64)
            .map(|id| (id, u64::MAX - id))
            .collect(),
        implements: Vec::new(),
        apparatus: Vec::new(),
    });
    assert!(
        items.len() <= DATAGRAM_FLOOR,
        "{inspectable_slots} inspectable charge accounts encode to {} bytes",
        items.len()
    );

    // ArcaneItems uses the reliable channel and the host chunks public
    // implement metadata at sixteen records. Prove the worst declared
    // per-record budget plus a deliberately generous visible-owner and
    // apparatus census remains below the transport's 64 KiB frame ceiling.
    let apparatus = (0..128)
        .map(|index| crate::implements::ApparatusCue {
            pos: bp(index % 32, 100, index / 32),
            charge_band: 3,
            strain_band: 3,
        })
        .collect();
    let fixed = encode(&S2C::ArcaneItems {
        reset: true,
        charges: (1..=128).map(|id| (id, u64::MAX - id)).collect(),
        implements: Vec::new(),
        apparatus,
    })
    .len();
    assert!(
        fixed + 16 * crate::implements::MAX_IMPLEMENT_PUBLIC_BYTES < 64 * 1024,
        "chunked implement snapshot can exceed its reliable frame budget"
    );
}
