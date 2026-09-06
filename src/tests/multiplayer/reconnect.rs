//! Reconnect scenarios.

use super::*;

#[test]
fn loopback_reconnect_reopens_the_same_server_profile() {
    use crate::net::S2C;

    let reg = base_reg();
    let world = test_world_with("mp-reconnect", reg.clone());
    let mut sim = crate::server::Server::new(world, 0.3, 7);
    let mut session = crate::mp::HostSession::start_on("reconnect".into(), 0).unwrap();
    prepare_test_entry(&mut session, &sim);
    let identity =
        crate::identity::LocalIdentity::load_or_create(&tmp_dir("reconnect-client")).unwrap();
    let address: std::net::SocketAddr = format!("127.0.0.1:{}", session.net.port).parse().unwrap();
    let mut first =
        crate::net::Client::connect(address, "Fern".into(), 0, 0, &identity, None).unwrap();
    for _ in 0..600 {
        session.pump(&mut sim, None, 0.05);
        if first
            .poll()
            .iter()
            .any(|message| matches!(message, S2C::Welcome { .. }))
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let first_connection = *session.guests.keys().next().unwrap();
    let player_id = session.guests[&first_connection].player_id;
    let torch = reg.item_id("base:torch").unwrap();
    session
        .guests
        .get_mut(&first_connection)
        .unwrap()
        .inventory
        .slots[0] = Some(ItemStack::new(&reg, torch, 6));
    // The owned client shutdown drains Bye before its runtime stops. The
    // ordinary drop path must persist the same profile as explicit departure.
    drop(first);
    for _ in 0..1_000 {
        session.pump(&mut sim, None, 0.05);
        if session.guests.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(session.guests.is_empty());
    let mut second =
        crate::net::Client::connect(address, "New Name".into(), 0, 0, &identity, None).unwrap();
    for _ in 0..600 {
        session.pump(&mut sim, None, 0.05);
        if second
            .poll()
            .iter()
            .any(|message| matches!(message, S2C::Welcome { .. }))
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let guest = session.guests.values().next().unwrap();
    assert_eq!(guest.player_id, player_id);
    assert_eq!(guest.inventory.slots[0].unwrap().count, 6);
    assert_eq!(guest.name, "NEW NAME");
}
