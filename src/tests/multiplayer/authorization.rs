//! Authorization scenarios.

use super::*;

#[test]
fn remote_roles_are_authorized_by_the_host_not_the_client_ui() {
    use crate::identity::Role;
    use crate::net::{C2S, ModerationAction, S2C};

    let reg = base_reg();
    let world = test_world_with("mp-remote-roles", reg);
    let mut sim = crate::server::Server::new(world, 0.3, 5);
    let mut sess = crate::mp::HostSession::start_on("remote-roles".into(), 0).unwrap();
    prepare_test_entry(&mut sess, &sim);
    let addr: std::net::SocketAddr = format!("127.0.0.1:{}", sess.net.port).parse().unwrap();
    let actor_identity =
        crate::identity::LocalIdentity::load_or_create(&tmp_dir("remote-role-actor")).unwrap();
    let target_identity =
        crate::identity::LocalIdentity::load_or_create(&tmp_dir("remote-role-target")).unwrap();
    let mut actor = crate::net::Client::connect(
        addr,
        "Actor".into(),
        sess.content_hash,
        0,
        &actor_identity,
        None,
    )
    .unwrap();
    let mut target = crate::net::Client::connect(
        addr,
        "Target".into(),
        sess.content_hash,
        0,
        &target_identity,
        None,
    )
    .unwrap();

    let mut actor_entry = TestEntry::default();
    let mut target_entry = TestEntry::default();
    for _ in 0..600 {
        sess.pump(&mut sim, None, 0.05);
        let actor_messages = actor.poll();
        let target_messages = target.poll();
        acknowledge_test_entry(&actor, &mut actor_entry, &actor_messages);
        acknowledge_test_entry(&target, &mut target_entry, &target_messages);
        if actor_entry.accepted && target_entry.accepted {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(sess.guests.len(), 2);
    let actor_id = sess
        .guests
        .iter()
        .find_map(|(id, guest)| (guest.name == "ACTOR").then_some(*id))
        .unwrap();
    let target_id = sess
        .guests
        .iter()
        .find_map(|(id, guest)| (guest.name == "TARGET").then_some(*id))
        .unwrap();

    // A forged privileged packet from an ordinary player changes nothing.
    actor.send(&C2S::Moderate {
        target: target_id,
        action: ModerationAction::Kick,
    });
    for _ in 0..60 {
        sess.pump(&mut sim, None, 0.05);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(sess.guests.contains_key(&target_id));
    assert!(actor.poll().iter().any(|message| {
        matches!(message, S2C::Toast(text) if text.contains("does not permit"))
    }));

    // A moderator still cannot grant roles.
    assert!(
        sess.set_guest_role(actor_id, Role::Moderator, "test owner")
            .unwrap()
    );
    actor.send(&C2S::Moderate {
        target: target_id,
        action: ModerationAction::CycleRole,
    });
    for _ in 0..60 {
        sess.pump(&mut sim, None, 0.05);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(sess.guest_role(target_id), Some(Role::Player));

    // An admin request is accepted, persisted, and reflected to the target.
    assert!(
        sess.set_guest_role(actor_id, Role::Admin, "test owner")
            .unwrap()
    );
    actor.send(&C2S::Moderate {
        target: target_id,
        action: ModerationAction::CycleRole,
    });
    for _ in 0..60 {
        sess.pump(&mut sim, None, 0.05);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(sess.guest_role(target_id), Some(Role::Moderator));

    // The same authorized request path applies a durable mute, and the host
    // rejects the target's next chat packet instead of broadcasting it.
    let _ = actor.poll();
    let _ = target.poll();
    actor.send(&C2S::Moderate {
        target: target_id,
        action: ModerationAction::Mute { seconds: 600 },
    });
    for _ in 0..60 {
        sess.pump(&mut sim, None, 0.05);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(actor.poll().iter().any(|message| {
        matches!(message, S2C::Toast(text) if text.contains("muted for 600 seconds"))
    }));
    target.send(&C2S::Chat("this must not be broadcast".into()));
    for _ in 0..60 {
        sess.pump(&mut sim, None, 0.05);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(
        target
            .poll()
            .iter()
            .any(|message| { matches!(message, S2C::Toast(text) if text.contains("muted")) })
    );
    assert!(!actor.poll().iter().any(|message| {
        matches!(message, S2C::Chat { msg, .. } if msg == "this must not be broadcast")
    }));

    actor.send(&C2S::Moderate {
        target: target_id,
        action: ModerationAction::Kick,
    });
    for _ in 0..60 {
        sess.pump(&mut sim, None, 0.05);
        if !sess.guests.contains_key(&target_id) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(!sess.guests.contains_key(&target_id));
}

#[test]
fn atproto_required_refuses_a_local_client_before_admission() {
    let mut host = crate::net::Host::start(
        "required-policy-test".into(),
        0,
        crate::identity::IdentityPolicy::AtprotoRequired,
        crate::identity::AdmissionPolicy::Open,
        0,
    )
    .unwrap();
    let identity =
        crate::identity::LocalIdentity::load_or_create(&tmp_dir("required-local")).unwrap();
    let addr: std::net::SocketAddr = format!("127.0.0.1:{}", host.port).parse().unwrap();
    let error = crate::net::Client::connect(addr, "Fern".into(), 0, 0, &identity, None)
        .err()
        .expect("required policy refuses an unlinked local client");
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    assert!(
        error
            .to_string()
            .contains("requires a linked ATProto account")
    );
    std::thread::sleep(std::time::Duration::from_millis(20));
    assert!(
        host.poll()
            .iter()
            .all(|event| !matches!(event, crate::net::HostEvent::Joined { .. }))
    );
}

#[test]
fn pre_entry_gameplay_is_ignored_until_terrain_is_acknowledged() {
    use crate::net::{C2S, S2C};

    let reg = base_reg();
    let world = test_world_with("mp-auth-order", reg);
    let mut sim = crate::server::Server::new(world, 0.3, 7);
    let mut session = crate::mp::HostSession::start_on("auth-order".into(), 0).unwrap();
    prepare_test_entry(&mut session, &sim);
    let identity =
        crate::identity::LocalIdentity::load_or_create(&tmp_dir("auth-order-client")).unwrap();
    let address: std::net::SocketAddr = format!("127.0.0.1:{}", session.net.port).parse().unwrap();
    let mut client =
        crate::net::Client::connect(address, "Fern".into(), 0, 0, &identity, None).unwrap();
    // Queue gameplay immediately after Authenticate, before the host's game
    // loop has processed the authenticated Join event.
    client.send(&C2S::Chat("too early to overtake welcome".into()));

    let mut order = Vec::new();
    for _ in 0..600 {
        session.pump(&mut sim, None, 0.05);
        for message in client.poll() {
            match message {
                S2C::Welcome { .. } => order.push("welcome"),
                S2C::Chat { .. } => order.push("chat"),
                _ => {}
            }
        }
        if order.contains(&"welcome") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(order, ["welcome"]);
}

#[test]
fn pending_guest_is_inert_until_the_exact_entry_set_is_acknowledged() {
    use crate::net::{C2S, S2C};

    let world = test_world("mp-entry-gate");
    let mut sim = crate::server::Server::new(world, 0.3, 7);
    let mut session = crate::mp::HostSession::start_on("entry-gate".into(), 0).unwrap();
    prepare_test_entry(&mut session, &sim);
    let identity =
        crate::identity::LocalIdentity::load_or_create(&tmp_dir("entry-gate-client")).unwrap();
    let address: std::net::SocketAddr = format!("127.0.0.1:{}", session.net.port).parse().unwrap();
    let mut client =
        crate::net::Client::connect(address, "Fern".into(), 0, 0, &identity, None).unwrap();
    let mut required: Option<std::collections::HashSet<ChunkPos>> = None;
    for _ in 0..1_000 {
        let fx = session.pump(&mut sim, None, 0.05);
        assert!(
            fx.iter()
                .all(|event| !matches!(event, crate::mp::HostFx::Joined(_))),
            "a terrain-decoding connection was announced as joined"
        );
        for message in client.poll() {
            match message {
                S2C::EntryManifest {
                    required: manifest, ..
                } => required = Some(manifest.into_iter().collect()),
                S2C::Chunk { face, u, v, .. } => {
                    if let Some(position) = crate::planet::Face::from_u8(face)
                        .and_then(|face| ChunkPos::new(face, u, v).ok())
                        && let Some(required) = &mut required
                    {
                        required.remove(&position);
                    }
                }
                _ => {}
            }
        }
        if required.as_ref().is_some_and(|set| set.is_empty()) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(required.is_some_and(|set| set.is_empty()));
    let id = *session.guests.keys().next().unwrap();
    let before = (
        session.guests[&id].pos,
        session.guests[&id].health,
        session.guests[&id].hunger,
    );
    assert!(!session.guests[&id].is_active());
    assert!(session.player_ctxs(None).is_empty());

    client.send(&C2S::Move {
        pos: ep(Vec3::new(100.5, 120.0, 100.5)),
        yaw: 1.0,
        hotbar: 0,
        sprint: true,
    });
    client.send(&C2S::Chat("I should not exist yet".into()));
    for _ in 0..40 {
        session.pump(&mut sim, None, 0.25);
        assert!(
            client
                .poll()
                .iter()
                .all(|message| !matches!(message, S2C::Chat { .. }))
        );
    }
    assert_eq!(
        (
            session.guests[&id].pos,
            session.guests[&id].health,
            session.guests[&id].hunger,
        ),
        before,
        "movement and survival are frozen before entry"
    );

    client.send(&C2S::EntryReady);
    let mut accepted = false;
    let mut announced = false;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        let fx = session.pump(&mut sim, None, 0.05);
        announced |= fx
            .iter()
            .any(|event| matches!(event, crate::mp::HostFx::Joined(name) if name == "FERN"));
        accepted |= client
            .poll()
            .iter()
            .any(|message| matches!(message, S2C::EntryAccepted));
        if accepted {
            break;
        }
        // Reliable transport has its own runtime thread. A fixed-count busy
        // loop can consume all 200 pumps before that thread is scheduled under
        // the full serial suite, even though the host accepted correctly.
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(accepted);
    assert!(announced);
    assert!(session.guests[&id].is_active());
    assert_eq!(session.player_ctxs(None).len(), 1);
}
