//! Headless dedicated-server runtime.

use super::*;

/// Headless dedicated host: same binary, no window. `--server <world>`.
pub(super) fn run_headless_server(world_name: &str) {
    let reg = Arc::new(registry::load(std::path::Path::new("mods")));
    let world = World::load_or_create(PathBuf::from("saves").join(world_name), reg.clone());
    let mut sim = server::Server::new(world, 0.3, 0xd5ed);
    sim.world.set_edit_logging(true);
    let mut sess = match mp::HostSession::start(world_name.to_string()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("server: could not bind: {e}");
            std::process::exit(1);
        }
    };
    eprintln!(
        "wildforge --server \"{world_name}\": listening on port {} (LAN beacon on)",
        sess.net.port
    );
    let mut last = Instant::now();
    let mut save_timer = 0.0f32;
    loop {
        let now = Instant::now();
        let dt = (now - last).as_secs_f32().min(0.25);
        last = now;
        for f in sess.pump(&mut sim, None, dt) {
            match f {
                mp::HostFx::Joined(n) => eprintln!("server: {n} joined"),
                mp::HostFx::Left(n) => eprintln!("server: {n} left"),
                mp::HostFx::Chat { from, msg } => eprintln!("<{from}> {msg}"),
                mp::HostFx::AllSlept => eprintln!("server: the camp sleeps to dawn"),
            }
        }
        let players = sess.player_ctxs(None);
        let mut evs = Vec::new();
        sim.advance(dt, &players, &mut evs);
        for ev in evs {
            if let server::SimEvent::PlayerHit { who, dmg, from } = ev {
                let ids: Vec<u32> = sess.guests.keys().copied().collect();
                if let Some(gid) = ids.get(who) {
                    sess.net.send(*gid, &net::S2C::Hit { dmg, from });
                }
            }
        }
        save_timer += dt;
        if save_timer >= 300.0 {
            save_timer = 0.0;
            sim.world.settle_falling();
            sim.world.save_modified();
            eprintln!("server: world saved");
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
}
