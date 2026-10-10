//! Exact custody outcomes independent of reservoir traversal implementation.

use crate::planet::{ChunkPos, Face};
use crate::planet_atlas::{AtlasGrid, ChunkWaterCommitment, WaterCell, WaterLedger};

use super::{ReservoirMass, SurfaceReservoirState, WaterCycleState};

fn mass(water_hu: u64, salt_mass: u64) -> ReservoirMass {
    ReservoirMass {
        water_hu,
        salt_mass,
    }
}

fn commitment(index: u16, reservoir: u64, mass: ReservoirMass) -> ChunkWaterCommitment {
    ChunkWaterCommitment {
        chunk: ChunkPos::new(Face::PosZ, index, 0).unwrap(),
        reservoir,
        mass,
    }
}

fn state(masses: &[ReservoirMass]) -> WaterCycleState {
    let reservoirs: Vec<_> = masses
        .iter()
        .enumerate()
        .map(|(index, &committed)| SurfaceReservoirState {
            id: (index as u64 + 1) * 10,
            name: format!("reservoir {index}"),
            coarse: mass(100 + index as u64, 200 + index as u64),
            committed,
            initial_total_hu: 1000 + index as u64,
            level_milliblocks: 23,
        })
        .collect();
    let commitments = reservoirs
        .iter()
        .enumerate()
        .map(|(index, reservoir)| {
            commitment(index.try_into().unwrap(), reservoir.id, reservoir.committed)
        })
        .collect();
    WaterCycleState {
        completed_surface_hours: 5,
        completed_groundwater_days: 7,
        cells: AtlasGrid::filled(1, WaterCell::default()).unwrap(),
        aquifers: Vec::new(),
        reservoirs,
        springs: Vec::new(),
        inboxes: Vec::new(),
        commitments,
        ledger: WaterLedger::default(),
    }
}

#[test]
fn detailed_debits_use_preferred_then_ascending_reservoirs() {
    let initial = state(&[mass(4, 4), mass(10, 10), mass(6, 6)]);
    for (preferred, remaining) in [
        (Some(20), [mass(1, 1), mass(0, 0), mass(6, 6)]),
        (Some(30), [mass(0, 0), mass(7, 7), mass(0, 0)]),
        (Some(10), [mass(0, 0), mass(1, 1), mass(6, 6)]),
        (Some(404), [mass(0, 0), mass(1, 1), mass(6, 6)]),
        (None, [mass(0, 0), mass(1, 1), mass(6, 6)]),
    ] {
        let mut actual = initial.clone();
        assert!(actual.debit_detailed_exact_from(preferred, mass(13, 13)));
        let mut expected = initial.clone();
        for (index, remaining) in remaining.into_iter().enumerate() {
            expected.reservoirs[index].committed = remaining;
            expected.commitments[index].mass = remaining;
        }
        expected
            .commitments
            .retain(|record| record.mass != ReservoirMass::default());
        assert_eq!(actual, expected, "preferred reservoir: {preferred:?}");
    }
}

#[test]
fn detailed_water_and_salt_can_debit_different_reservoirs() {
    let mut actual = state(&[mass(0, 0), mass(8, 0), mass(0, 9), mass(6, 4)]);
    actual.commitments = vec![
        commitment(0, 40, mass(2, 1)),
        commitment(1, 20, mass(8, 0)),
        commitment(2, 40, mass(4, 3)),
        commitment(3, 30, mass(0, 9)),
        commitment(4, 10, mass(0, 0)),
    ];
    let mut expected = actual.clone();
    expected.reservoirs[1].committed = mass(4, 0);
    expected.reservoirs[2].committed = mass(0, 2);
    expected.reservoirs[3].committed = mass(0, 0);
    expected.commitments = vec![commitment(1, 20, mass(4, 0)), commitment(3, 30, mass(0, 2))];

    assert!(actual.debit_detailed_exact_from(Some(40), mass(10, 11)));
    assert_eq!(actual, expected);
}

#[test]
fn detailed_debits_preserve_commitment_order_and_independent_parcels() {
    let mut actual = state(&[mass(12, 12)]);
    actual.commitments = vec![
        commitment(0, 10, mass(5, 1)),
        commitment(1, 10, mass(2, 7)),
        commitment(2, 10, mass(5, 4)),
    ];
    let mut expected = actual.clone();
    expected.reservoirs[0].committed = mass(6, 6);
    expected.commitments = vec![commitment(1, 10, mass(1, 2)), commitment(2, 10, mass(5, 4))];

    assert!(actual.debit_detailed_exact_from(Some(10), mass(6, 6)));
    assert_eq!(actual, expected);
}

#[test]
fn zero_detailed_debits_keep_mass_and_remove_empty_commitments() {
    let initial = state(&[mass(0, 0), mass(9, 8), mass(0, 0)]);
    let mut expected = initial.clone();
    expected.commitments = vec![initial.commitments[1]];
    for preferred in [None, Some(10), Some(20), Some(404)] {
        let mut actual = initial.clone();
        assert!(actual.debit_detailed_exact_from(preferred, mass(0, 0)));
        assert_eq!(actual, expected);
    }
}

#[test]
fn insufficient_detailed_water_or_salt_preserves_complete_state() {
    let initial = state(&[mass(2, 5), mass(0, 0), mass(3, 2)]);
    for preferred in [None, Some(10), Some(30), Some(404)] {
        for requested in [mass(6, 1), mass(4, 8), mass(6, 8)] {
            let mut actual = initial.clone();
            assert!(!actual.debit_detailed_exact_from(preferred, requested));
            // Rejection must also keep existing zero commitments, coarse
            // water, reservoir metadata, calendars, cells, and the ledger.
            assert_eq!(actual, initial);
        }
    }
}

#[test]
fn empty_detailed_custody_accepts_zero_and_refuses_nonzero_parcels() {
    let initial = state(&[]);
    let mut actual = initial.clone();
    assert!(actual.debit_detailed_exact(mass(0, 0)));
    assert_eq!(actual, initial);
    for requested in [mass(1, 0), mass(0, 1), mass(1, 1)] {
        assert!(!actual.debit_detailed_exact(requested));
        assert_eq!(actual, initial);
    }
}
