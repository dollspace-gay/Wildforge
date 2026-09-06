//! Streaming scenarios.

use super::*;

#[test]
fn host_and_guest_cross_a_planet_seam_smoothly_with_both_faces_streamed() {
    use crate::net::{C2S, S2C};
    use crate::planet::{EntityPos, FACE_BLOCKS, Face};

    let (mut sess, mut sim, mut client, id) = loopback_pair("mp-planet-seam");
    let start = EntityPos::new(Face::PosZ, f32::from(FACE_BLOCKS) - 0.2, 120.0, 4096.5).unwrap();
    let end = start.translated(Vec3::X * 0.7).unwrap().pos;
    assert_ne!(start.face(), end.face(), "fixture crosses a cube face");
    sess.guests.get_mut(&id).unwrap().prime_move_for_test(start);
    let mut streamed_faces = std::collections::HashSet::new();

    client.send(&C2S::Move {
        pos: end,
        yaw: 0.4,
        hotbar: 0,
        sprint: false,
    });
    for _ in 0..200 {
        sess.pump(&mut sim, Some((end, 0.4, false, u16::MAX, 0)), 0.0);
        for message in client.poll() {
            if let S2C::Chunk { face, .. } = message
                && let Some(face) = Face::from_u8(face)
            {
                streamed_faces.insert(face);
            }
        }
        if sess.guests[&id].pos == end {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(sess.guests[&id].pos, end, "the host accepts the seam step");

    let from_render = start.render_pos();
    let to_render = end.render_pos();
    let just_accepted = sess.guests[&id].render_pos().0;
    assert!(
        just_accepted.distance(from_render) < 1.0e-4,
        "the render span starts at the pre-seam position"
    );
    sess.pump(&mut sim, Some((end, 0.4, false, u16::MAX, 0)), 0.15);
    let midway = sess.guests[&id].render_pos().0;
    assert!(
        midway.distance(from_render) > 0.05 && midway.distance(to_render) > 0.05,
        "embedded interpolation must not snap to either face endpoint"
    );

    let mut saw_host_across = false;
    let streaming_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < streaming_deadline {
        sess.pump(&mut sim, Some((end, 0.4, false, u16::MAX, 0)), 0.06);
        for message in client.poll() {
            match message {
                S2C::Chunk { face, .. } => {
                    if let Some(face) = Face::from_u8(face) {
                        streamed_faces.insert(face);
                    }
                }
                S2C::Players(part) => {
                    saw_host_across |= part
                        .items
                        .iter()
                        .any(|(player, pos, ..)| *player == 0 && pos.face() == end.face());
                }
                _ => {}
            }
        }
        let guest = &sess.guests[&id];
        if guest.holds_chunk_at(start.chunk().unwrap())
            && guest.holds_chunk_at(end.chunk().unwrap())
            && saw_host_across
            && streamed_faces.contains(&start.face())
            && streamed_faces.contains(&end.face())
        {
            break;
        }
        // Chunk load/generation and RLE encoding are intentionally off the
        // host pump, and recording a reliable send is not the same as the
        // client having polled it. Yield until both sides of that contract
        // are observed instead of racing the transport after send-side state
        // happens to become complete.
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let guest = &sess.guests[&id];
    assert!(
        guest.holds_chunk_at(start.chunk().unwrap()),
        "the source-face seam chunk remains resident"
    );
    assert!(
        guest.holds_chunk_at(end.chunk().unwrap()),
        "the destination-face seam chunk streams"
    );
    assert!(
        streamed_faces.contains(&start.face()) && streamed_faces.contains(&end.face()),
        "the client receives terrain from both sides of the seam"
    );
    assert!(
        saw_host_across,
        "the guest receives the host's canonical destination-face snapshot"
    );
}

#[test]
fn a_crowded_world_still_reaches_the_guest() {
    use crate::mobs::Mob;

    let (mut sess, mut sim, mut client, id) = loopback_pair("mp-crowded");
    let gpos = Vec3::new(8.5, sim.world.surface_height(8, 8) as f32 + 1.0, 8.5);
    sess.guests.get_mut(&id).unwrap().pos = ep(gpos);

    // Two hundred mobs inside the guest's reach: far past what ever fit in
    // one datagram, which is exactly the case that used to go silent.
    for i in 0..200 {
        let angle = i as f32 * 0.31;
        let pos = gpos + Vec3::new(angle.sin() * 30.0, 0.0, angle.cos() * 30.0);
        let mut mob = Mob::new(0, pos, 0.0);
        mob.id = i as u32 + 1;
        sim.world.spawn_mob(mob);
    }
    assert!(sim.world.mob_count() >= 200);

    let mut seen: std::collections::HashSet<u32> = Default::default();
    for _ in 0..400 {
        sess.pump(&mut sim, Some((ep(gpos), 0.0, false, u16::MAX, 0)), 0.06);
        for msg in client.poll() {
            if let S2C::Mobs(part) = msg {
                seen.extend(part.items.iter().map(|m| m.id));
            }
        }
        if seen.len() >= 200 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(
        sess.net.datagram_failures(),
        0,
        "the host must not be dropping state datagrams on the floor"
    );
    assert!(
        seen.len() >= 200,
        "a guest standing among 200 mobs saw only {} of them",
        seen.len()
    );
}

#[test]
fn a_guest_that_dropped_a_chunk_can_ask_for_it_again() {
    let (mut sess, mut sim, mut client, id, drained) = loopback_pair_drained("mp-rechunk");
    let gpos = Vec3::new(8.5, sim.world.surface_height(8, 8) as f32 + 1.0, 8.5);
    sess.guests.get_mut(&id).unwrap().pos = ep(gpos);

    // The host starts the ordinary ring as soon as admission completes, so
    // the first chunk is allowed to share the poll that carried
    // EntryAccepted. Ignoring the handshake drain made this test depend on
    // thread scheduling even though the host had correctly recorded and sent
    // the ground.
    let mut first = drained.into_iter().find_map(|message| {
        let S2C::Chunk { face, u, v, .. } = message else {
            return None;
        };
        crate::planet::Face::from_u8(face).and_then(|face| ChunkPos::new(face, u, v).ok())
    });
    for _ in 0..400 {
        sess.pump(&mut sim, Some((ep(gpos), 0.0, false, u16::MAX, 0)), 0.06);
        for msg in client.poll() {
            if let S2C::Chunk { face, u, v, .. } = msg
                && let Some(face) = crate::planet::Face::from_u8(face)
                && let Ok(pos) = ChunkPos::new(face, u, v)
            {
                first.get_or_insert(pos);
            }
        }
        if first.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let pos = first.expect("the host streams a ring unprompted");
    assert!(
        sess.guests[&id].holds_chunk(pos.centered_u(), pos.centered_v()),
        "the host records what it sent"
    );

    // The guest evicts it (walking away and back does this for real), then
    // asks. Before RequestChunk existed this was a permanent hole.
    client.send(&crate::net::C2S::RequestChunk {
        face: pos.face() as u8,
        u: pos.u(),
        v: pos.v(),
    });
    let mut resent = false;
    for _ in 0..400 {
        sess.pump(&mut sim, Some((ep(gpos), 0.0, false, u16::MAX, 0)), 0.06);
        for msg in client.poll() {
            resent |= matches!(
                msg,
                S2C::Chunk { face, u, v, .. }
                    if (face, u, v) == (pos.face() as u8, pos.u(), pos.v())
            );
        }
        if resent {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(resent, "the host re-serves ground the guest asked for");
}

#[test]
fn the_host_serves_the_view_distance_a_guest_asks_for() {
    use crate::net::MAX_GUEST_VIEW_DIST;

    let (mut sess, mut sim, mut client, id) = loopback_pair("mp-viewdist");
    let gpos = Vec3::new(8.5, sim.world.surface_height(8, 8) as f32 + 1.0, 8.5);
    sess.guests.get_mut(&id).unwrap().pos = ep(gpos);
    // The old host served a hardcoded ring of five however far the guest
    // could actually see.
    assert_eq!(sess.guests[&id].granted_view_dist(), 5);

    client.send(&crate::net::C2S::SetViewDistance { chunks: 9 });
    let mut granted = None;
    for _ in 0..300 {
        sess.pump(&mut sim, Some((ep(gpos), 0.0, false, u16::MAX, 0)), 0.06);
        for msg in client.poll() {
            if let S2C::ViewDistance { chunks } = msg {
                granted = Some(chunks);
            }
        }
        if granted.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(granted, Some(9), "the host honours a reasonable request");
    assert_eq!(sess.guests[&id].granted_view_dist(), 9);

    // And refuses to page in the world on one client's say-so.
    client.send(&crate::net::C2S::SetViewDistance { chunks: 200 });
    let mut capped = None;
    for _ in 0..300 {
        sess.pump(&mut sim, Some((ep(gpos), 0.0, false, u16::MAX, 0)), 0.06);
        for msg in client.poll() {
            if let S2C::ViewDistance { chunks } = msg {
                capped = Some(chunks);
            }
        }
        if capped.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(capped, Some(MAX_GUEST_VIEW_DIST), "a greedy ask is clamped");
}

#[test]
fn a_world_releases_chunks_no_player_is_near() {
    let mut w = test_world("mp-residency");
    for x in -6..=6 {
        for z in -6..=6 {
            w.ensure_chunk(tchunk(x, z));
        }
    }
    let loaded = w.chunk_count();
    assert!(loaded >= 169);

    // One player near the origin: distant ground goes.
    let report = w.retain_chunks(&[tchunk(0, 0)], 2);
    assert!(
        report.released > 0,
        "chunks far from every player must be released"
    );
    assert!(report.is_ok(), "clean eviction: {}", report.summary());
    assert_eq!(
        w.chunk_count(),
        13,
        "a geodesic radius keeps a circular neighborhood"
    );
    assert!(w.has_chunk(tchunk(2, 0)));
    assert!(!w.has_chunk(tchunk(2, 2)));
    assert!(!w.has_chunk(tchunk(5, 5)));

    // Two players far apart each keep their own ground — the dedicated
    // server's case, where there is no local player at all.
    let mut w = test_world("mp-residency-two");
    for x in -6..=6 {
        for z in -6..=6 {
            w.ensure_chunk(tchunk(x, z));
        }
    }
    let report = w.retain_chunks(&[tchunk(-5, -5), tchunk(5, 5)], 1);
    assert!(report.is_ok(), "two-center eviction: {}", report.summary());
    assert!(w.has_chunk(tchunk(-5, -5)), "first player's ground");
    assert!(w.has_chunk(tchunk(5, 5)), "second player's ground");
    assert!(!w.has_chunk(tchunk(0, 0)), "the empty middle goes");

    // Nobody home: an idle server holds no world.
    let mut w = test_world("mp-residency-empty");
    for x in -3..=3 {
        for z in -3..=3 {
            w.ensure_chunk(tchunk(x, z));
        }
    }
    assert!(w.chunk_count() > 0);
    let report = w.retain_chunks(&[], 12);
    assert!(
        report.is_ok(),
        "empty-server eviction: {}",
        report.summary()
    );
    assert_eq!(w.chunk_count(), 0, "no players means no resident chunks");
}

#[test]
fn failed_dirty_chunk_eviction_keeps_only_the_unsaved_ground() {
    let mut w = test_world("mp-residency-save-failure");
    let failed = tchunk(-2, 0);
    let saved = tchunk(2, 0);
    for pos in [failed, saved] {
        w.ensure_chunk(pos);
        let x = pos.centered_u() * crate::chunk::CHUNK_X as i32;
        let y = w.surface_height(x, 0) + 1;
        let stone = w.reg.block_id("base:stone").unwrap();
        w.set_block(x, y, 0, stone);
    }
    w.fail_chunk_save_for_test(failed, true);

    let first = w.retain_chunks(&[], 0);
    assert_eq!(
        first.released, 24,
        "every healthy fixture chunk still leaves"
    );
    assert_eq!(first.retained_dirty, 1);
    assert_eq!(first.failures.len(), 1);
    assert_eq!(w.chunk_count(), 1, "only the failed chunk remains");
    assert!(w.has_chunk(failed), "the newest copy stays in memory");
    assert!(!w.has_chunk(saved), "successful ground was released");

    w.fail_chunk_save_for_test(failed, false);
    let retry = w.retain_chunks(&[], 0);
    assert_eq!(retry.released, 1);
    assert_eq!(retry.retained_dirty, 0);
    assert!(retry.is_ok(), "retry lands: {}", retry.summary());
    assert!(!w.has_chunk(failed));
}

#[test]
fn a_guest_receives_the_ring_it_was_granted() {
    // Asking for a view distance has to actually put that much ground on the
    // wire. The host used to serve a hardcoded ring of five however far the
    // guest could see; this pins the ring to the grant.
    let (mut sess, mut sim, mut client, id, drained) = loopback_pair_drained("mp-ring");
    let gpos = Vec3::new(8.5, sim.world.surface_height(8, 8) as f32 + 1.0, 8.5);
    sess.guests.get_mut(&id).unwrap().pos = ep(gpos);
    let center = ChunkPos::of_world(8, 8);

    client.send(&crate::net::C2S::SetViewDistance { chunks: 6 });
    let mut got: std::collections::HashSet<ChunkPos> = drained
        .iter()
        .filter_map(|m| match m {
            S2C::Chunk { face, u, v, .. } => crate::planet::Face::from_u8(*face)
                .and_then(|face| ChunkPos::new(face, *u, *v).ok()),
            _ => None,
        })
        .collect();
    let want = (-6..=6)
        .flat_map(|du| (-6..=6).map(move |dv| center.offset(du, dv)))
        .filter(|pos| pos.distance(center) <= 6.0 * 16.0 + 1.0)
        .collect::<std::collections::HashSet<_>>()
        .len();
    for _ in 0..4000 {
        sess.pump(&mut sim, Some((ep(gpos), 0.0, false, u16::MAX, 0)), 0.06);
        for msg in client.poll() {
            if let S2C::Chunk { face, u, v, .. } = msg
                && let Some(face) = crate::planet::Face::from_u8(face)
                && let Ok(pos) = ChunkPos::new(face, u, v)
            {
                got.insert(pos);
            }
        }
        if got.len() >= want {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(
        got.len(),
        want,
        "a guest granted 6 chunks should receive a 13x13 ring, got {}",
        got.len()
    );
    // ...and all of it around the guest, not somewhere else.
    for pos in &got {
        assert!(
            pos.distance(center) <= 6.0 * 16.0 + 1.0,
            "chunk {pos:?} is outside the granted ring around {center:?}"
        );
    }
}
