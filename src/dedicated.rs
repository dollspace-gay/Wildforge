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
    eprintln!("server: type 'help' for moderation commands");
    let (command_tx, command_rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        use std::io::BufRead;
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if command_tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut last = Instant::now();
    let mut save_timer = 0.0f32;
    loop {
        while let Ok(command) = command_rx.try_recv() {
            run_console_command(&mut sess, &command);
        }
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
                    sess.hurt_guest(*gid, dmg, from);
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

fn run_console_command(sess: &mut mp::HostSession, line: &str) {
    let mut words = line.split_whitespace();
    let command = words.next().unwrap_or("").to_ascii_lowercase();
    let id = || {
        words
            .clone()
            .next()
            .and_then(|value| value.parse::<u32>().ok())
    };
    let result: std::io::Result<Option<String>> = match command.as_str() {
        "" => Ok(None),
        "help" => Ok(Some(
            "commands: players, identity <id>, kick <id>, mute <id> [seconds], ban <id> [seconds|perm], allow <id>, role <id> <player|moderator|admin>, unban <player-uuid>"
                .into(),
        )),
        "players" => {
            let mut rows: Vec<String> = sess
                .guests
                .keys()
                .filter_map(|id| sess.guest_identity_summary(*id).map(|summary| format!("{id}: {summary}")))
                .collect();
            rows.sort();
            Ok(Some(if rows.is_empty() { "no players connected".into() } else { rows.join("\n") }))
        }
        "identity" => Ok(id().and_then(|id| sess.guest_identity_summary(id))),
        "kick" => Ok(id().and_then(|id| sess.kick_guest(id)).map(|name| format!("kicked {name}"))),
        "mute" => {
            let Some(id) = words.next().and_then(|value| value.parse::<u32>().ok()) else {
                eprintln!("server: usage: mute <id> [seconds]");
                return;
            };
            let seconds = words.next().and_then(|value| value.parse::<u64>().ok()).unwrap_or(600);
            sess.mute_guest(id, "dedicated console mute", Some(seconds), "console")
                .map(|changed| changed.then_some(format!("muted {id} for {seconds} seconds")))
        }
        "ban" => {
            let Some(id) = words.next().and_then(|value| value.parse::<u32>().ok()) else {
                eprintln!("server: usage: ban <id> [seconds|perm]");
                return;
            };
            let duration = match words.next() {
                None | Some("perm") | Some("permanent") => None,
                Some(value) => match value.parse::<u64>() {
                    Ok(seconds) => Some(seconds),
                    Err(_) => {
                        eprintln!("server: ban duration must be seconds or 'perm'");
                        return;
                    }
                },
            };
            sess.ban_guest(id, "dedicated console ban", duration, "console")
                .map(|name| name.map(|name| format!("banned {name}")))
        }
        "allow" => {
            let Some(id) = id() else {
                eprintln!("server: usage: allow <id>");
                return;
            };
            sess.allow_guest(id, "console")
                .map(|changed| changed.then_some(format!("allowlisted {id}")))
        }
        "role" => {
            let Some(id) = words.next().and_then(|value| value.parse::<u32>().ok()) else {
                eprintln!("server: usage: role <id> <player|moderator|admin>");
                return;
            };
            let role = match words.next().unwrap_or("") {
                "player" => mp::Role::Player,
                "moderator" | "mod" => mp::Role::Moderator,
                "admin" => mp::Role::Admin,
                _ => {
                    eprintln!("server: role must be player, moderator, or admin");
                    return;
                }
            };
            sess.set_guest_role(id, role, "console")
                .map(|changed| changed.then_some(format!("set {id} role to {role:?}")))
        }
        "unban" => {
            let Some(player_id) = words.next().and_then(identity::PlayerId::parse) else {
                eprintln!("server: usage: unban <player-uuid>");
                return;
            };
            sess.unban_player(player_id, "console")
                .map(|changed| changed.then_some(format!("unbanned {player_id}")))
        }
        _ => Ok(Some("unknown command; type 'help'".into())),
    };
    match result {
        Ok(Some(message)) => eprintln!("server: {message}"),
        Ok(None) => eprintln!("server: no matching connected player or record"),
        Err(error) => eprintln!("server: command failed: {error}"),
    }
}
