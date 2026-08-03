//! Headless dedicated-server runtime.

use super::*;

/// Headless dedicated host: same binary, no window. `--server <world>`.
pub(super) fn run_headless_server(world_name: &str) {
    let reg = Arc::new(registry::load(std::path::Path::new("mods")));
    let mut world =
        match World::load_or_create(PathBuf::from("saves").join(world_name), reg.clone()) {
            Ok(world) => world,
            Err(error) => {
                eprintln!("server: could not open world \"{world_name}\": {error}");
                std::process::exit(1);
            }
        };
    let prepared_spawn = match world.prepare_common_spawn(|stage, completed, total| {
        if completed == 0 || completed == total || completed.is_multiple_of(5) {
            eprintln!("server: {stage} {completed}/{total}");
        }
    }) {
        Ok(spawn) => spawn,
        Err(error) => {
            eprintln!("server: could not prepare a qualified homeland: {error}");
            std::process::exit(1);
        }
    };
    if let Some(ledger) = &world.material_ledger {
        for notice in ledger.retrogen_notices() {
            eprintln!("server: {notice}");
        }
    }
    let mut sim = server::Server::new(world, 0.3, 0xd5ed);
    sim.world.set_edit_logging(true);
    let mut sess = match mp::HostSession::start(world_name.to_string()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("server: could not bind: {e}");
            std::process::exit(1);
        }
    };
    sess.fresh_spawn = Some(prepared_spawn);
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
    let mut residency_timer = 0.0f32;
    let mut last_datagram_report = 0u64;
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
                mp::HostFx::ImplementActivation { .. } | mp::HostFx::WorkingEvent(_) => {}
            }
        }
        let players = sess.authoritative_player_ctxs(&sim.world, None);
        let mut evs = Vec::new();
        sim.advance(dt, &players, &mut evs);
        for ev in evs {
            match ev {
                server::SimEvent::PlayerHit { who, dmg, from } => {
                    // `who` is the guest's own net id; no positional lookup.
                    sess.hurt_guest(&mut sim, who, dmg, from);
                }
                server::SimEvent::Working(_, cue) => sess.broadcast_working_cue(cue),
                _ => {}
            }
        }
        // Chunk residency. Nothing here ever released a chunk before: the
        // eviction rule lived in the client's streaming path, so the windowed
        // host got it for free (it is also a player) and the dedicated server
        // — the deployment that actually needs it — grew by 448 KB for every
        // chunk any guest walked through and never gave one back.
        //
        // Wall-clock, not calendar-scaled: this is a memory policy, not
        // something that happens in the world.
        residency_timer += dt;
        if residency_timer >= 5.0 {
            residency_timer = 0.0;
            let (centers, radius) = sess.residency();
            let residency = sim.world.retain_chunks(&centers, radius + 2);
            if residency.released > 0 {
                eprintln!(
                    "server: released {} chunks ({} resident)",
                    residency.released,
                    sim.world.chunk_count()
                );
            }
            if !residency.is_ok() {
                eprintln!("server: chunk eviction incomplete: {}", residency.summary());
            }
            // A state datagram the path refused is a guest quietly missing
            // wildlife. Zero is the only healthy number here.
            let dropped_datagrams = sess.net.datagram_failures();
            if dropped_datagrams > last_datagram_report {
                last_datagram_report = dropped_datagrams;
                eprintln!("server: {dropped_datagrams} state datagrams dropped");
            }
        }
        save_timer += dt;
        if save_timer >= 300.0 {
            save_timer = 0.0;
            sim.world.settle_falling();
            let report = sim.world.save_modified();
            eprintln!("{}", save_log_line(&report));
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
}

fn save_log_line(report: &world::SaveReport) -> String {
    if report.is_ok() {
        format!("server: world saved ({})", report.summary())
    } else {
        format!("server: world save incomplete: {}", report.summary())
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
                .iter()
                .filter(|(_, guest)| guest.is_active())
                .filter_map(|(id, _)| sess.guest_identity_summary(*id).map(|summary| format!("{id}: {summary}")))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedicated_save_log_never_calls_a_partial_save_success() {
        let success = world::SaveReport {
            chunks_saved: 2,
            failures: Vec::new(),
        };
        assert_eq!(
            save_log_line(&success),
            "server: world saved (2 dirty chunks written)"
        );

        let partial = world::SaveReport {
            chunks_saved: 1,
            failures: vec![world::SaveFailure {
                component: "animals".into(),
                path: PathBuf::from("saves/world1/animals.toml"),
                error: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "read-only"),
            }],
        };
        let line = save_log_line(&partial);
        assert!(line.starts_with("server: world save incomplete:"));
        assert!(line.contains("animals"));
        assert!(!line.starts_with("server: world saved"));
    }
}
