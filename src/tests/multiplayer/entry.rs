//! Entry scenarios.

use super::*;

#[test]
fn host_residency_keeps_the_prepared_doorstep_without_guests() {
    let mut session = crate::mp::HostSession::start_on("spawn-residency".into(), 0).unwrap();
    let spawn = ep(Vec3::new(17.5, 80.2, -9.5));
    let chunk = spawn.chunk().unwrap();
    session.fresh_spawn = Some(spawn);
    let (centers, radius) = session.residency();
    assert_eq!(centers, vec![chunk]);
    assert_eq!(radius, 1);
}

#[test]
fn late_join_gets_complete_roster_and_duplicate_name_is_refused() {
    use crate::net::{RefusalCode, S2C};
    let reg = base_reg();
    let world = test_world_with("mp-roster", reg);
    let mut sim = crate::server::Server::new(world, 0.3, 7);
    let mut sess = crate::mp::HostSession::start_on_with_policy(
        "roster".into(),
        0,
        Some(crate::identity::DisplayName::parse("Host").unwrap()),
        crate::identity::IdentityPolicy::Local,
        crate::identity::AdmissionPolicy::Open,
    )
    .unwrap();
    prepare_test_entry(&mut sess, &sim);
    let addr: std::net::SocketAddr = format!("127.0.0.1:{}", sess.net.port).parse().unwrap();
    let first_identity =
        crate::identity::LocalIdentity::load_or_create(&tmp_dir("roster-one")).unwrap();
    let mut first = crate::net::Client::connect(
        addr,
        "Fern".into(),
        sess.content_hash,
        0,
        &first_identity,
        None,
    )
    .unwrap();
    let mut first_entry = TestEntry::default();
    for _ in 0..300 {
        sess.pump(&mut sim, None, 0.05);
        let messages = first.poll();
        acknowledge_test_entry(&first, &mut first_entry, &messages);
        if first_entry.accepted {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(sess.guests.len(), 1);

    let second_identity =
        crate::identity::LocalIdentity::load_or_create(&tmp_dir("roster-two")).unwrap();
    let mut second = crate::net::Client::connect(
        addr,
        "Moss".into(),
        sess.content_hash,
        0,
        &second_identity,
        None,
    )
    .unwrap();
    let mut names = Vec::new();
    let mut second_entry = TestEntry::default();
    for _ in 0..300 {
        sess.pump(&mut sim, None, 0.05);
        let messages = second.poll();
        acknowledge_test_entry(&second, &mut second_entry, &messages);
        for message in messages {
            if let S2C::Welcome { roster, .. } = message {
                names = roster
                    .into_iter()
                    .map(|presence| presence.display_name)
                    .collect();
            }
        }
        if !names.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    names.sort();
    assert_eq!(names, ["FERN", "HOST", "MOSS"]);

    let third_identity =
        crate::identity::LocalIdentity::load_or_create(&tmp_dir("roster-three")).unwrap();
    let mut duplicate = crate::net::Client::connect(
        addr,
        "mOsS".into(),
        sess.content_hash,
        0,
        &third_identity,
        None,
    )
    .unwrap();
    let mut refused = false;
    for _ in 0..300 {
        sess.pump(&mut sim, None, 0.05);
        refused |= duplicate.poll().iter().any(|message| {
            matches!(
                message,
                S2C::Refused(refusal) if refusal.code == RefusalCode::NameInUse
            )
        });
        if refused {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(refused);
    assert_eq!(sess.guests.len(), 2);
}
