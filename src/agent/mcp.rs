//! The MCP skin: a newline-delimited JSON-RPC (stdio) server whose
//! tools drive one agent guest. One MCP session, one connection, one
//! identity — a party of agents is a party of processes.

use super::*;
use serde_json::{Value, json};

/// `wildforge --agent <addr> [--name NAME]`: connect and serve MCP
/// on stdio until the pipe closes or the host drops us.
pub fn run_agent(addr: &str, name: &str) {
    let addr: std::net::SocketAddr = if addr.contains(':') {
        addr.parse()
    } else {
        format!("{addr}:27431").parse()
    }
    .unwrap_or_else(|e| {
        eprintln!("agent: bad address: {e}");
        std::process::exit(2);
    });
    eprintln!("agent {name}: connecting to {addr}...");
    let mut agent = match Agent::connect(addr, name) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("agent: {e}");
            std::process::exit(1);
        }
    };
    eprintln!(
        "agent {name}: in world at {:.0} {:.0} {:.0}; serving MCP on stdio",
        agent.player.pos.x, agent.player.pos.y, agent.player.pos.z
    );
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        use std::io::BufRead;
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    loop {
        agent.pump(0.02);
        while let Ok(line) = rx.try_recv() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(reply) = handle(&mut agent, line) {
                use std::io::Write;
                let mut out = std::io::stdout().lock();
                let _ = writeln!(out, "{reply}");
                let _ = out.flush();
            }
        }
        if !agent.in_world {
            eprintln!("agent: connection closed");
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

fn handle(agent: &mut Agent, line: &str) -> Option<String> {
    let req: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            return Some(
                json!({"jsonrpc":"2.0","id":null,
                       "error":{"code":-32700,"message":format!("parse: {e}")}})
                .to_string(),
            );
        }
    };
    let id = req.get("id").cloned();
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");
    match method {
        "initialize" => Some(rpc_ok(
            id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "wildforge-agent", "version": env!("CARGO_PKG_VERSION")}
            }),
        )),
        "notifications/initialized" | "notifications/cancelled" => None,
        "ping" => Some(rpc_ok(id, json!({}))),
        "tools/list" => Some(rpc_ok(id, json!({"tools": tool_schemas()}))),
        "tools/call" => {
            let name = req
                .pointer("/params/name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let args = req
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or(json!({}));
            let text = call_tool(agent, name, &args);
            Some(rpc_ok(
                id,
                json!({"content": [{"type": "text", "text": text}]}),
            ))
        }
        _ => Some(
            json!({"jsonrpc":"2.0","id":id,
                   "error":{"code":-32601,"message":format!("unknown method {method}")}})
            .to_string(),
        ),
    }
}

fn rpc_ok(id: Option<Value>, result: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
}

fn planetary_pos(args: &Value) -> Result<crate::planet::BlockPos, String> {
    let face = args
        .get("face")
        .and_then(Value::as_str)
        .and_then(crate::planet::Face::from_name)
        .ok_or("face must be PosX, NegX, PosY, NegY, PosZ, or NegZ")?;
    let coord = |name: &str| {
        args.get(name)
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("{name} must be a nonnegative integer"))
    };
    let u = u16::try_from(coord("u")?).map_err(|_| "u is outside 0..8191")?;
    let y = u8::try_from(coord("y")?).map_err(|_| "y is outside 0..255")?;
    let v = u16::try_from(coord("v")?).map_err(|_| "v is outside 0..8191")?;
    crate::planet::BlockPos::new(face, u, y, v).map_err(|error| error.to_string())
}

fn tool(name: &str, desc: &str, props: Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "description": desc,
        "inputSchema": {"type": "object", "properties": props, "required": required}
    })
}

fn tool_schemas() -> Vec<Value> {
    let num = |d: &str| json!({"type": "number", "description": d});
    let s = |d: &str| json!({"type": "string", "description": d});
    let pos = || {
        json!({
            "face": s("planet face: PosX, NegX, PosY, NegY, PosZ, or NegZ"),
            "u": num("bounded east coordinate, 0..8191"),
            "y": num("radial block height, 0..255"),
            "v": num("bounded north coordinate, 0..8191")
        })
    };
    vec![
        tool(
            "look_around",
            "Where you are, a minimap, company, and conditions.",
            json!({}),
            &[],
        ),
        tool(
            "nearest",
            "Nearest matches: a block name (base:log), a tag (#base:logs), 'tree', 'water', or 'player <name>'.",
            json!({"kind": s("what to look for"), "radius": num("search radius, up to 48")}),
            &["kind"],
        ),
        tool(
            "at",
            "Inspect one block position.",
            pos(),
            &["face", "u", "y", "v"],
        ),
        tool("inventory", "Your pack, slot by slot.", json!({}), &[]),
        tool(
            "status",
            "Position, health, hunger, time, current behavior.",
            json!({}),
            &[],
        ),
        tool(
            "events",
            "Drain everything that happened since last asked (chat, drops, damage, arrivals).",
            json!({}),
            &[],
        ),
        tool(
            "chat",
            "Say something to everyone.",
            json!({"message": s("what to say")}),
            &["message"],
        ),
        tool(
            "go_to",
            "Walk to a position (pathfinds; blocks until arrival, failure, or timeout).",
            pos(),
            &["face", "u", "y", "v"],
        ),
        tool(
            "follow",
            "Follow a player's path at heel until told to stop. Returns immediately.",
            json!({"player": s("player name"), "distance": num("blocks to hang back (default 3)")}),
            &["player"],
        ),
        tool("stop", "Stop walking or following.", json!({}), &[]),
        tool(
            "chop_nearest_tree",
            "Find the nearest tree, walk to it, fell the trunk, collect the drops.",
            json!({}),
            &[],
        ),
        tool(
            "break_block",
            "Break one block in reach.",
            pos(),
            &["face", "u", "y", "v"],
        ),
        tool(
            "place",
            "Place an item from the pack as a block.",
            {
                let mut fields = pos().as_object().cloned().unwrap_or_default();
                fields.insert("item".into(), s("item name, e.g. base:chest"));
                Value::Object(fields)
            },
            &["face", "u", "y", "v", "item"],
        ),
        tool(
            "craft",
            "Craft what the hands know: planks, stick, crafting_table, chest.",
            json!({"what": s("planks | stick | crafting_table | chest"), "times": num("how many times (default 1)")}),
            &["what"],
        ),
        tool(
            "deposit",
            "Stow pack contents into a chest in reach.",
            {
                let mut fields = pos().as_object().cloned().unwrap_or_default();
                fields.insert("item".into(), s("only this item (optional)"));
                Value::Object(fields)
            },
            &["face", "u", "y", "v"],
        ),
        tool(
            "station_put",
            "Rest an item on a station (anvil, quern, millstone...).",
            {
                let mut fields = pos().as_object().cloned().unwrap_or_default();
                fields.insert("item".into(), s("item to rest"));
                Value::Object(fields)
            },
            &["face", "u", "y", "v", "item"],
        ),
        tool(
            "station_take",
            "Take resting work back off a station.",
            pos(),
            &["face", "u", "y", "v"],
        ),
        tool(
            "eat",
            "Eat a food item from the pack.",
            json!({"item": s("food item name")}),
            &["item"],
        ),
        tool("respawn", "Respawn after death.", json!({}), &[]),
    ]
}

fn call_tool(agent: &mut Agent, name: &str, args: &Value) -> String {
    let gi = |k: &str| args.get(k).and_then(Value::as_f64).map(|v| v as i32);
    let gs = |k: &str| args.get(k).and_then(Value::as_str).map(str::to_string);
    let need = "missing argument";
    match name {
        "look_around" => agent.look_around(),
        "nearest" => match gs("kind") {
            Some(kind) => agent.nearest(&kind, gi("radius").unwrap_or(32)),
            None => need.into(),
        },
        "at" => planetary_pos(args)
            .map(|pos| agent.at(pos))
            .unwrap_or_else(|error| error),
        "inventory" => agent.inventory_text(),
        "status" => agent.status(),
        "events" => {
            let drained: Vec<String> = agent.events.drain(..).collect();
            if drained.is_empty() {
                "nothing new".into()
            } else {
                drained.join("\n")
            }
        }
        "chat" => match gs("message") {
            Some(m) => {
                agent.send(&crate::net::C2S::Chat(m));
                "said".into()
            }
            None => need.into(),
        },
        "go_to" => match planetary_pos(args) {
            Ok(pos) => match agent.go_to(pos) {
                Ok(full) => {
                    let note = if full { "" } else { " (partial route)" };
                    let outcome = agent.wait_idle(90.0);
                    format!("{outcome}{note}")
                }
                Err(e) => e,
            },
            Err(error) => error,
        },
        "follow" => match gs("player") {
            Some(p) => {
                let want = p.to_lowercase();
                let hit = agent
                    .players
                    .iter()
                    .find(|(_, (n, _, _))| n.to_lowercase().contains(&want))
                    .map(|(id, (n, _, _))| (*id, n.clone()));
                match hit {
                    Some((id, n)) => {
                        let d = args.get("distance").and_then(Value::as_f64).unwrap_or(3.0);
                        agent.behavior = Behavior::Follow {
                            id,
                            distance: d as f32,
                        };
                        format!("at your heel, {n}")
                    }
                    None => format!("can't see a player called {p}"),
                }
            }
            None => need.into(),
        },
        "stop" => {
            agent.behavior = Behavior::Idle;
            "standing down".into()
        }
        "chop_nearest_tree" => {
            let logs = agent.reg.tags.get("base:logs").cloned().unwrap_or_default();
            let reg = agent.reg.clone();
            let p = agent.player.pos;
            let Some(origin) = motion::cell_of(p) else {
                return "outside the voxel shell".into();
            };
            let py = i32::from(origin.y());
            let mut best: Option<(crate::planet::BlockPos, f32)> = None;
            for du in -48..=48 {
                for dv in -48..=48 {
                    for y in (py - 24).max(1)..=(py + 24).min(255) {
                        let Some(at) = origin.offset(du, y - py, dv) else {
                            continue;
                        };
                        let b = agent.world.get_block_at(at);
                        if reg
                            .item_id(&reg.block(b).name)
                            .is_some_and(|i| logs.contains(&i))
                        {
                            let d = at.entity_center().distance_to(p);
                            if best.is_none_or(|(_, bd)| d < bd) {
                                best = Some((at, d));
                            }
                        }
                    }
                }
            }
            match best {
                Some((pos, _)) => agent.chop_at(pos).unwrap_or_else(|e| e),
                None => "no trees on any streamed ground near you".into(),
            }
        }
        "break_block" => match planetary_pos(args) {
            Ok(pos) => match agent.break_block_at(pos) {
                Ok(()) => "broken".into(),
                Err(e) => e,
            },
            Err(error) => error,
        },
        "place" => match (planetary_pos(args), gs("item")) {
            (Ok(pos), Some(item)) => match agent.place_at(pos, &item) {
                Ok(()) => "placed".into(),
                Err(e) => e,
            },
            (Err(error), _) => error,
            _ => need.into(),
        },
        "craft" => match gs("what") {
            Some(what) => {
                let times = args.get("times").and_then(Value::as_f64).unwrap_or(1.0);
                agent.craft(&what, times as u32).unwrap_or_else(|e| e)
            }
            None => need.into(),
        },
        "deposit" => match planetary_pos(args) {
            Ok(pos) => {
                let only = gs("item");
                agent.deposit(pos, only.as_deref()).unwrap_or_else(|e| e)
            }
            Err(error) => error,
        },
        "station_put" => match (planetary_pos(args), gs("item")) {
            (Ok(pos), Some(item)) => match agent.station_put(pos, &item) {
                Ok(()) => "rested".into(),
                Err(e) => e,
            },
            (Err(error), _) => error,
            _ => need.into(),
        },
        "station_take" => match planetary_pos(args) {
            Ok(pos) => match agent.station_take(pos) {
                Ok(()) => "taken (check events for what arrived)".into(),
                Err(e) => e,
            },
            Err(error) => error,
        },
        "eat" => match gs("item") {
            Some(item) => match agent.eat(&item) {
                Ok(()) => "ate".into(),
                Err(e) => e,
            },
            None => need.into(),
        },
        "respawn" => {
            agent.send(&crate::net::C2S::Respawn);
            agent.pump_for(0.3);
            "back on my feet".into()
        }
        other => format!("unknown tool {other}"),
    }
}
