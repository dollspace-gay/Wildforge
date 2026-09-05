//! Loss, duplication, malformed fragments, and wrapping sequence numbers.

use super::SnapshotAssembler;
use crate::net::Snapshot;

fn part(seq: u32, part: u8, parts: u8, item: u32) -> Snapshot<u32> {
    Snapshot {
        seq,
        part,
        parts,
        items: vec![item],
    }
}

#[test]
fn stale_or_duplicate_single_packets_cannot_reapply_an_older_world() {
    let mut receiver = SnapshotAssembler::default();
    assert_eq!(receiver.accept(part(10, 0, 1, 10)), Some(vec![10]));
    assert_eq!(receiver.accept(part(9, 0, 1, 9)), None);
    assert_eq!(receiver.accept(part(10, 0, 1, 100)), None);
    assert_eq!(receiver.accept(part(11, 0, 1, 11)), Some(vec![11]));
}

#[test]
fn a_new_single_packet_retires_partial_older_generations() {
    let mut receiver = SnapshotAssembler::default();
    assert_eq!(receiver.accept(part(10, 0, 2, 10)), None);
    assert_eq!(receiver.accept(part(9, 0, 1, 9)), None);
    assert_eq!(receiver.accept(part(11, 0, 1, 11)), Some(vec![11]));
    assert_eq!(receiver.accept(part(10, 1, 2, 12)), None);
}

#[test]
fn reordered_and_duplicate_parts_apply_once_in_declared_order() {
    let mut receiver = SnapshotAssembler::default();
    assert_eq!(receiver.accept(part(10, 1, 3, 2)), None);
    assert_eq!(receiver.accept(part(10, 1, 3, 200)), None);
    assert_eq!(receiver.accept(part(10, 2, 3, 3)), None);
    assert_eq!(receiver.accept(part(10, 0, 3, 1)), Some(vec![1, 2, 3]));
    assert_eq!(receiver.accept(part(10, 0, 1, 100)), None);
    for index in 0..3 {
        assert_eq!(receiver.accept(part(10, index, 3, u32::from(index))), None);
    }
}

#[test]
fn invalid_fragment_bounds_cannot_replace_a_valid_pending_generation() {
    let mut receiver = SnapshotAssembler::default();
    assert_eq!(receiver.accept(part(10, 0, 2, 1)), None);
    assert_eq!(receiver.accept(part(11, 0, 0, 100)), None);
    assert_eq!(receiver.accept(part(11, 1, 1, 100)), None);
    assert_eq!(receiver.accept(part(11, 2, 2, 100)), None);
    assert_eq!(receiver.accept(part(10, 1, 2, 2)), Some(vec![1, 2]));
}

#[test]
fn conflicting_part_counts_do_not_mix_two_different_snapshot_layouts() {
    let mut receiver = SnapshotAssembler::default();
    assert_eq!(receiver.accept(part(10, 0, 2, 1)), None);
    assert_eq!(receiver.accept(part(10, 0, 1, 100)), None);
    assert_eq!(receiver.accept(part(10, 1, 3, 200)), None);
    assert_eq!(receiver.accept(part(10, 1, 2, 2)), Some(vec![1, 2]));
}

#[test]
fn sequence_wrap_accepts_forward_progress_and_rejects_old_or_ambiguous_packets() {
    let mut receiver = SnapshotAssembler::default();
    assert_eq!(receiver.accept(part(u32::MAX, 0, 1, 1)), Some(vec![1]));
    assert_eq!(receiver.accept(part(0, 1, 2, 3)), None);
    assert_eq!(receiver.accept(part(0, 0, 2, 2)), Some(vec![2, 3]));
    assert_eq!(receiver.accept(part(u32::MAX, 0, 1, 100)), None);
    assert_eq!(receiver.accept(part(1 << 31, 0, 1, 100)), None);
    assert_eq!(receiver.accept(part(1, 0, 1, 4)), Some(vec![4]));
}
