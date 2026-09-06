//! Snapshots scenarios.

use super::*;

#[test]
fn a_full_world_of_mobs_is_batched_under_the_datagram_budget() {
    use crate::net::{DATAGRAM_FLOOR, S2C, batch_snapshot};

    // The old code put every mob in the world into one datagram. QUIC will
    // not fragment a datagram, so past roughly forty mobs the whole snapshot
    // was refused and guests silently stopped seeing wildlife entirely.
    let whole = crate::net::encode(&S2C::Mobs(crate::net::Snapshot::whole(
        1,
        mob_snaps(crate::world::MOB_CAP),
    )));
    assert!(
        whole.len() > DATAGRAM_FLOOR * 4,
        "a full world of mobs is far past one datagram ({} bytes) — \
         if this ever stops being true the batching below is untested",
        whole.len()
    );

    for count in [0, 1, 2, 39, 40, 41, 120, crate::world::MOB_CAP] {
        let parts = batch_snapshot(7, mob_snaps(count), DATAGRAM_FLOOR, S2C::Mobs);
        assert!(!parts.is_empty(), "{count} mobs must still send something");
        for (i, bytes) in parts.iter().enumerate() {
            assert!(
                bytes.len() <= DATAGRAM_FLOOR,
                "{count} mobs: part {i} is {} bytes, over the {DATAGRAM_FLOOR} budget",
                bytes.len()
            );
        }
    }
}

#[test]
fn a_split_snapshot_is_applied_only_once_it_is_whole() {
    use crate::client_session::SnapshotAssembler;
    use crate::net::{DATAGRAM_FLOOR, S2C, batch_snapshot, decode};

    let sent = mob_snaps(200);
    let parts = batch_snapshot(9, sent.clone(), DATAGRAM_FLOOR, S2C::Mobs);
    assert!(parts.len() > 1, "200 mobs must actually split");

    let mut rx: SnapshotAssembler<crate::net::MobSnap> = Default::default();
    let mut delivered = None;
    for (i, bytes) in parts.iter().enumerate() {
        let Some(S2C::Mobs(part)) = decode::<S2C>(bytes) else {
            panic!("part {i} decodes")
        };
        let last = i + 1 == parts.len();
        match rx.accept(part) {
            Some(whole) => {
                assert!(last, "a generation must not apply before its last part");
                delivered = Some(whole);
            }
            None => assert!(!last, "the last part completes the generation"),
        }
    }
    let got = delivered.expect("the whole snapshot arrives");
    assert_eq!(got.len(), sent.len(), "every mob survives the split");
    let ids: Vec<u32> = got.iter().map(|m| m.id).collect();
    let want: Vec<u32> = sent.iter().map(|m| m.id).collect();
    assert_eq!(ids, want, "order and identity survive reassembly");
}

#[test]
fn host_owned_loose_item_ids_survive_batched_guest_and_agent_snapshots() {
    use crate::client_session::SnapshotAssembler;
    use crate::net::{DATAGRAM_FLOOR, S2C, batch_snapshot, decode};

    let sent = (0..200)
        .map(|index| crate::net::LooseItemSnap {
            id: (1u64 << 62) + index,
            pos: ep(Vec3::new(index as f32 * 0.25, 80.0, 0.5)),
            vel: Vec3::new(0.1, 0.0, -0.1),
            item: (index % 16) as u16,
            count: (index % 64 + 1) as u32,
            age: index as f32 * 0.1,
            durability: index as u32,
            arcane_id: 0,
        })
        .collect::<Vec<_>>();
    let parts = batch_snapshot(11, sent.clone(), DATAGRAM_FLOOR, S2C::LooseItems);
    assert!(parts.len() > 1);
    assert!(parts.iter().all(|part| part.len() <= DATAGRAM_FLOOR));

    let mut receiver: SnapshotAssembler<crate::net::LooseItemSnap> = Default::default();
    let mut delivered = None;
    for bytes in parts {
        let Some(S2C::LooseItems(part)) = decode::<S2C>(&bytes) else {
            panic!("loose-item snapshot did not decode")
        };
        if let Some(items) = receiver.accept(part) {
            delivered = Some(items);
        }
    }
    let delivered = delivered.expect("the whole loose-item generation arrives");
    assert_eq!(delivered.len(), sent.len());
    assert_eq!(
        delivered.iter().map(|item| item.id).collect::<Vec<_>>(),
        sent.iter().map(|item| item.id).collect::<Vec<_>>()
    );
    assert!(delivered.iter().all(|item| item.id < (1u64 << 63)));
}

#[test]
fn a_lost_part_costs_its_generation_and_nothing_after_it() {
    use crate::client_session::SnapshotAssembler;
    use crate::net::{DATAGRAM_FLOOR, S2C, batch_snapshot, decode};

    let mut rx: SnapshotAssembler<crate::net::MobSnap> = Default::default();
    let dropped = batch_snapshot(1, mob_snaps(200), DATAGRAM_FLOOR, S2C::Mobs);
    assert!(dropped.len() > 1);
    // Everything but the last part of generation 1 lands.
    for bytes in &dropped[..dropped.len() - 1] {
        let Some(S2C::Mobs(part)) = decode::<S2C>(bytes) else {
            panic!()
        };
        assert!(rx.accept(part).is_none());
    }
    // Generation 2 arrives whole and is applied regardless.
    let good = batch_snapshot(2, mob_snaps(200), DATAGRAM_FLOOR, S2C::Mobs);
    let mut applied = None;
    for bytes in &good {
        let Some(S2C::Mobs(part)) = decode::<S2C>(bytes) else {
            panic!()
        };
        if let Some(whole) = rx.accept(part) {
            applied = Some(whole);
        }
    }
    assert_eq!(
        applied.map(|m| m.len()),
        Some(200),
        "a torn generation must not stall the stream behind it"
    );
}
