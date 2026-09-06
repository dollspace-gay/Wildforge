//! Protocol codecs and loopback host/guest behavior.

use super::*;
use crate::world::{ReplicaWorld, ReplicationTarget, TerrainRead};

fn prepare_test_entry(session: &mut crate::mp::HostSession, sim: &crate::server::Server) {
    let y = sim.world.surface_height(8, 8) as f32 + 1.0;
    session.fresh_spawn = Some(ep(Vec3::new(8.5, y, 8.5)));
}

#[derive(Default)]
struct TestEntry {
    required: Option<std::collections::HashSet<ChunkPos>>,
    ready_sent: bool,
    accepted: bool,
}

fn acknowledge_test_entry(
    client: &crate::net::Client,
    entry: &mut TestEntry,
    messages: &[crate::net::S2C],
) {
    for message in messages {
        match message {
            crate::net::S2C::EntryManifest { required, .. } => {
                entry.required = Some(required.iter().copied().collect());
            }
            crate::net::S2C::Chunk { face, u, v, .. } => {
                if let Some(position) = crate::planet::Face::from_u8(*face)
                    .and_then(|face| ChunkPos::new(face, *u, *v).ok())
                    && let Some(required) = &mut entry.required
                {
                    required.remove(&position);
                }
            }
            crate::net::S2C::EntryAccepted => entry.accepted = true,
            _ => {}
        }
    }
    if entry
        .required
        .as_ref()
        .is_some_and(|required| required.is_empty())
        && !entry.ready_sent
    {
        client.send(&crate::net::C2S::EntryReady);
        entry.ready_sent = true;
    }
}

// ---------------- scaling: the guest is a first-class citizen ----------------

use crate::net::S2C;

/// Build `n` mob snapshots spread over a wide area.
fn mob_snaps(n: usize) -> Vec<crate::net::MobSnap> {
    (0..n)
        .map(|i| crate::net::MobSnap {
            id: i as u32 + 1,
            species: (i % 7) as u16,
            pos: ep(Vec3::new(i as f32 * 3.5, 64.0, i as f32 * -2.5)),
            yaw: 0.7,
            growth: 1.0,
            hurt: 0.0,
            health: 8.0,
            fed: i % 2 == 0,
        })
        .collect()
}

/// Stand up a host with one connected guest. Returns the session, the sim,
/// the client, and the guest's id.
fn loopback_pair(
    name: &str,
) -> (
    crate::mp::HostSession,
    crate::server::Server,
    crate::net::Client,
    u32,
) {
    let (sess, sim, client, id, _) = loopback_pair_drained(name);
    (sess, sim, client, id)
}

/// As `loopback_pair`, but also hands back everything that arrived while the
/// handshake was settling — the host starts streaming its ring immediately,
/// so a test that counts chunks has to count those too.
fn loopback_pair_drained(
    name: &str,
) -> (
    crate::mp::HostSession,
    crate::server::Server,
    crate::net::Client,
    u32,
    Vec<S2C>,
) {
    loopback_pair_drained_with_reg(name, base_reg())
}

fn loopback_pair_drained_with_reg(
    name: &str,
    reg: Arc<Registry>,
) -> (
    crate::mp::HostSession,
    crate::server::Server,
    crate::net::Client,
    u32,
    Vec<S2C>,
) {
    let world = test_world_with(name, reg);
    let mut sim = crate::server::Server::new(world, 0.3, 5);
    sim.world.set_edit_logging(true);
    let mut sess = crate::mp::HostSession::start_on(name.into(), 0).expect("host binds");
    prepare_test_entry(&mut sess, &sim);
    let addr: std::net::SocketAddr = format!("127.0.0.1:{}", sess.net.port).parse().unwrap();
    let identity =
        crate::identity::LocalIdentity::load_or_create(&tmp_dir(&format!("{name}-id"))).unwrap();
    let mut client =
        crate::net::Client::connect(addr, "tester".into(), sess.content_hash, 0, &identity, None)
            .expect("connect");
    let mut drained = Vec::new();
    let mut required: Option<std::collections::HashSet<ChunkPos>> = None;
    let mut entry_ready_sent = false;
    for _ in 0..600 {
        sess.pump(&mut sim, None, 0.05);
        let batch = client.poll();
        for message in &batch {
            match message {
                S2C::EntryManifest {
                    required: manifest, ..
                } => required = Some(manifest.iter().copied().collect()),
                S2C::Chunk { face, u, v, .. } => {
                    if let Some(position) = crate::planet::Face::from_u8(*face)
                        .and_then(|face| ChunkPos::new(face, *u, *v).ok())
                        && let Some(required) = &mut required
                    {
                        required.remove(&position);
                    }
                }
                _ => {}
            }
        }
        if required
            .as_ref()
            .is_some_and(|required| required.is_empty())
            && !entry_ready_sent
        {
            client.send(&crate::net::C2S::EntryReady);
            entry_ready_sent = true;
        }
        let done = batch
            .iter()
            .any(|message| matches!(message, S2C::EntryAccepted));
        drained.extend(batch);
        if done {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let id = *sess.guests.keys().next().expect("guest admitted");
    (sess, sim, client, id, drained)
}

mod authorization;
mod entry;
mod gameplay;
mod loopback_join_stream_and_edit;
mod protocol;
mod reconnect;
mod snapshots;
mod streaming;
