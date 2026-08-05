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
        .and_then(|name| {
            crate::planet::Face::from_name(name).or(match name {
                // Compatibility for the names the first MCP schema advertised.
                "PosX" => Some(crate::planet::Face::PosX),
                "NegX" => Some(crate::planet::Face::NegX),
                "PosY" => Some(crate::planet::Face::PosY),
                "NegY" => Some(crate::planet::Face::NegY),
                "PosZ" => Some(crate::planet::Face::PosZ),
                "NegZ" => Some(crate::planet::Face::NegZ),
                _ => None,
            })
        })
        .ok_or(
            "face must be pos_x, neg_x, pos_y, neg_y, pos_z, or neg_z \
             (legacy PosX, NegX, PosY, NegY, PosZ, and NegZ are also accepted)",
        )?;
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
            "face": {
                "type": "string",
                "enum": ["pos_x", "neg_x", "pos_y", "neg_y", "pos_z", "neg_z"],
                "description": "canonical planet face"
            },
            "u": {
                "type": "integer",
                "minimum": 0,
                "maximum": 8191,
                "description": "bounded east coordinate"
            },
            "y": {
                "type": "integer",
                "minimum": 0,
                "maximum": 255,
                "description": "radial block height"
            },
            "v": {
                "type": "integer",
                "minimum": 0,
                "maximum": 8191,
                "description": "bounded north coordinate"
            }
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
        tool(
            "observe_magic",
            "Settle a held tuning lens on a reachable block and write the host-authored qualitative reading to a physical ledger. Omit coordinates to survey the local region.",
            {
                let mut fields = pos().as_object().cloned().unwrap_or_default();
                fields.insert(
                    "ledger".into(),
                    s("record-holder item, usually base:field_ledger"),
                );
                fields.insert("calibration".into(), s("optional calibration plate item"));
                fields.insert("label".into(), s("optional short public label"));
                Value::Object(fields)
            },
            &["ledger"],
        ),
        tool(
            "read_knowledge",
            "Read a physical artifact, field ledger, or carried survey folio. This reveals only records present in that item.",
            json!({"item": s("item name in the pack")}),
            &["item"],
        ),
        tool(
            "read_folio",
            "Read a reachable placed settlement survey folio.",
            pos(),
            &["face", "u", "y", "v"],
        ),
        tool(
            "copy_observation",
            "At a reachable writing surface, copy one signed observation between carried holders or an adjacent placed folio. Use the literal placed_folio plus the matching nested position. Location may be omitted.",
            {
                let mut fields = pos().as_object().cloned().unwrap_or_default();
                fields.insert("source".into(), s("source item name or placed_folio"));
                fields.insert(
                    "source_folio".into(),
                    json!({"type": "object", "properties": pos()}),
                );
                fields.insert("record_id".into(), json!({"type": "integer", "minimum": 1}));
                fields.insert(
                    "destination".into(),
                    s("destination item name or placed_folio"),
                );
                fields.insert(
                    "destination_folio".into(),
                    json!({"type": "object", "properties": pos()}),
                );
                fields.insert(
                    "include_location".into(),
                    json!({"type": "boolean", "description": "copy site coordinates too (default false)"}),
                );
                Value::Object(fields)
            },
            &["face", "u", "y", "v", "source", "record_id", "destination"],
        ),
        tool(
            "run_magic_experiment",
            "Load one physical sample and the experiment's calibrated reference into a reachable apparatus, run a repeatable trial, and write its qualitative result to a ledger. The sample and reference remain installed and are not consumed.",
            {
                let mut fields = pos().as_object().cloned().unwrap_or_default();
                fields.insert(
                    "kind".into(),
                    s("capacity | conductivity | stability | biological_response | dross_response"),
                );
                fields.insert("sample".into(), s("sample item in the pack"));
                fields.insert("ledger".into(), s("record-holder item"));
                fields.insert("calibration".into(), s("optional calibration plate item"));
                Value::Object(fields)
            },
            &["face", "u", "y", "v", "kind", "sample", "ledger"],
        ),
        tool(
            "assemble_tuning_lens",
            "Use a reachable lens assembly bench to fit an initial frame and Echo Slate plate, or reuse a surviving fitted mount, with a replaceable Wellglass element from the pack.",
            pos(),
            &["face", "u", "y", "v"],
        ),
        tool(
            "binding_frame",
            "Use the same physical binding-frame actions as a player. Contextual mounts/retrieves/assembles from the current authoritative state; explicit actions inspect, calibrate, bind charms, transfer, discharge, disassemble, swap a focus, or repair.",
            {
                let mut fields = pos().as_object().cloned().unwrap_or_default();
                fields.insert("action".into(), s("contextual | exchange | assemble | bind_charm | transfer | discharge | disassemble | swap_focus | repair | calibrate | inspect"));
                fields.insert(
                    "item".into(),
                    s("optional item to hold/select before acting"),
                );
                Value::Object(fields)
            },
            &["face", "u", "y", "v", "action"],
        ),
        tool(
            "alchemy",
            "Use the ordinary host-authoritative laboratory actions: inspect/begin/grind/transfer/load/heat/agitate/advance/charge/sample/decant/clean/repair/drain/dismantle/ferment/press, or drink/apply one stable preparation.",
            {
                let mut fields = pos().as_object().cloned().unwrap_or_default();
                fields.insert("action".into(), s("inspect | begin | grind | transfer | load | media | heat | agitate | advance | charge | sample | decant | clean | repair | drain | dismantle | ferment | press | use"));
                fields.insert(
                    "preparation".into(),
                    s("qualified recipe id for begin, or preparation item for use"),
                );
                fields.insert(
                    "item".into(),
                    s("held ingredient/carrier/vessel/water/seed/charge vessel"),
                );
                fields.insert("filter".into(), s("optional filter item for clean"));
                fields.insert("wheat".into(), s("wheat item for ferment"));
                fields.insert("berry".into(), s("berry item for ferment"));
                fields.insert(
                    "destination".into(),
                    json!({"type": "object", "properties": pos()}),
                );
                fields.insert(
                    "target_pos".into(),
                    json!({"type": "object", "properties": pos()}),
                );
                fields.insert("target".into(), s("self | plot | surface | item for use"));
                fields.insert("target_item".into(), s("stable carried item to wash/coat"));
                fields.insert(
                    "step".into(),
                    s("heat | agitate | settle | distill | filter | cool"),
                );
                fields.insert("agitation".into(), s("still | stirred | shaken"));
                fields.insert("route".into(), s("soil | runoff | air | sealed_waste"));
                fields.insert(
                    "temperature_millic".into(),
                    json!({"type": "integer", "minimum": -50000, "maximum": 250000}),
                );
                fields.insert(
                    "units".into(),
                    json!({"type": "integer", "minimum": 0, "maximum": 65536}),
                );
                Value::Object(fields)
            },
            &["face", "u", "y", "v", "action"],
        ),
        tool(
            "working",
            "Aim, start, hold, release, or cancel the same host-authoritative wand workings and constructed rituals available to players. Start needs a target kind and any relevant physical target; later intents continue the active request.",
            json!({
                "working": s("qualified id, e.g. base:trace or base:ward_boundary"),
                "intent": s("aim | start | force | hold | release | cancel"),
                "target": s("none | block | water | entity | inventory | ritual"),
                "target_pos": {"type": "object", "properties": pos()},
                "secondary_pos": {"type": "object", "properties": pos()},
                "entity_id": {"type": "integer", "minimum": 1},
                "target_slot": {"type": "integer", "minimum": 0, "maximum": 45},
                "material_slot": {"type": "integer", "minimum": 0, "maximum": 45},
                "magnitude": {"type": "integer", "minimum": 1, "maximum": 64},
                "water_hu": {"type": "integer", "enum": [32, 64]},
                "item": s("wand item to select before start (not used by rituals)")
            }),
            &["working", "intent"],
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
        "observe_magic" => {
            let target = if args.get("face").is_some() {
                match planetary_pos(args) {
                    Ok(pos) => Some(pos),
                    Err(error) => return error,
                }
            } else {
                None
            };
            match gs("ledger") {
                Some(ledger) => agent
                    .observe_discovery(target, &ledger, gs("calibration").as_deref(), gs("label"))
                    .unwrap_or_else(|error| error),
                None => need.into(),
            }
        }
        "read_knowledge" => match gs("item") {
            Some(item) => agent.read_knowledge(&item).unwrap_or_else(|error| error),
            None => need.into(),
        },
        "read_folio" => match planetary_pos(args) {
            Ok(pos) => agent.read_folio(pos).unwrap_or_else(|error| error),
            Err(error) => error,
        },
        "copy_observation" => {
            let record_id = args.get("record_id").and_then(Value::as_u64);
            let holder = |name: Option<String>, position: &str| match name.as_deref() {
                Some("placed_folio") => args
                    .get(position)
                    .ok_or_else(|| format!("missing {position}"))
                    .and_then(planetary_pos)
                    .map(|pos| crate::net::RecordHolderSnap::Folio { pos }),
                Some(name) => agent.discovery_holder_for_item(name),
                None => Err(need.into()),
            };
            match (
                planetary_pos(args),
                holder(gs("source"), "source_folio"),
                record_id,
                holder(gs("destination"), "destination_folio"),
            ) {
                (Ok(writing_pos), Ok(source), Some(record_id), Ok(destination)) => agent
                    .copy_observation(
                        writing_pos,
                        source,
                        record_id,
                        destination,
                        args.get("include_location")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    )
                    .unwrap_or_else(|error| error),
                (Err(error), _, _, _) | (_, Err(error), _, _) | (_, _, _, Err(error)) => error,
                _ => need.into(),
            }
        }
        "run_magic_experiment" => match (
            planetary_pos(args),
            gs("kind").and_then(|kind| crate::discovery::ExperimentKind::parse(&kind)),
            gs("sample"),
            gs("ledger"),
        ) {
            (Ok(pos), Some(kind), Some(sample), Some(ledger)) => agent
                .run_discovery_experiment(pos, kind, &sample, &ledger, gs("calibration").as_deref())
                .unwrap_or_else(|error| error),
            (Err(error), _, _, _) => error,
            _ => "missing or unknown experiment argument".into(),
        },
        "assemble_tuning_lens" => match planetary_pos(args) {
            Ok(pos) => agent
                .assemble_discovery_lens(pos)
                .unwrap_or_else(|error| error),
            Err(error) => error,
        },
        "binding_frame" => match (planetary_pos(args), gs("action")) {
            (Ok(pos), Some(action)) => {
                let action = match action.as_str() {
                    "contextual" => Some(crate::implements::FrameAction::Contextual),
                    "exchange" => Some(crate::implements::FrameAction::ExchangeSelected),
                    "assemble" => Some(crate::implements::FrameAction::Assemble),
                    "bind_charm" => Some(crate::implements::FrameAction::BindCharm),
                    "transfer" => Some(crate::implements::FrameAction::Transfer),
                    "discharge" => Some(crate::implements::FrameAction::SafeDischarge),
                    "disassemble" => Some(crate::implements::FrameAction::Disassemble),
                    "swap_focus" => Some(crate::implements::FrameAction::SwapFocus),
                    "repair" => Some(crate::implements::FrameAction::Repair),
                    "calibrate" => Some(crate::implements::FrameAction::Calibrate),
                    "inspect" => Some(crate::implements::FrameAction::Inspect),
                    _ => None,
                };
                match action {
                    Some(action) => agent
                        .operate_binding_frame(pos, action, gs("item").as_deref())
                        .unwrap_or_else(|error| error),
                    None => "unknown binding-frame action".into(),
                }
            }
            (Err(error), _) => error,
            _ => need.into(),
        },
        "alchemy" => {
            use crate::alchemy::{
                AgitationKind, AlchemyTarget, ApparatusAction, DisposalRoute, ProcessStep,
            };
            let pos = match planetary_pos(args) {
                Ok(pos) => pos,
                Err(error) => return error,
            };
            let Some(action_name) = gs("action") else {
                return "missing alchemy action".into();
            };
            let route = || match gs("route").as_deref() {
                Some("soil") => Ok(DisposalRoute::Soil),
                Some("runoff") => Ok(DisposalRoute::Runoff),
                Some("air") => Ok(DisposalRoute::Air),
                Some("sealed_waste") => Ok(DisposalRoute::SealedWaste),
                _ => Err("missing or unknown disposal route".to_string()),
            };
            let step = || match gs("step").as_deref() {
                Some("heat") => Ok(ProcessStep::Heat),
                Some("agitate") => Ok(ProcessStep::Agitate),
                Some("settle") => Ok(ProcessStep::Settle),
                Some("distill") => Ok(ProcessStep::Distill),
                Some("filter") => Ok(ProcessStep::Filter),
                Some("cool") => Ok(ProcessStep::Cool),
                _ => Err("missing or unknown process step".to_string()),
            };
            if action_name == "use" {
                let Some(preparation) = gs("preparation") else {
                    return "use needs a preparation item".into();
                };
                let target = match gs("target").as_deref().unwrap_or("self") {
                    "self" => Ok(AlchemyTarget::SelfActor),
                    "plot" => args
                        .get("target_pos")
                        .ok_or_else(|| "plot use needs target_pos".to_string())
                        .and_then(planetary_pos)
                        .map(AlchemyTarget::Plot),
                    "surface" => args
                        .get("target_pos")
                        .ok_or_else(|| "surface use needs target_pos".to_string())
                        .and_then(planetary_pos)
                        .map(AlchemyTarget::Surface),
                    "item" => gs("target_item")
                        .ok_or_else(|| "item use needs target_item".to_string())
                        .and_then(|name| agent.named_slot(&name))
                        .and_then(|slot| {
                            agent.inventory.slots[slot]
                                .filter(|stack| stack.count == 1 && stack.arcane_id != 0)
                                .map(|stack| AlchemyTarget::Item(stack.arcane_id))
                                .ok_or_else(|| "target item needs one stable identity".to_string())
                        }),
                    _ => Err("unknown preparation target".to_string()),
                };
                return target
                    .and_then(|target| agent.apply_preparation(&preparation, target))
                    .unwrap_or_else(|error| error);
            }
            let held = gs("item");
            let action = match action_name.as_str() {
                "inspect" => Ok(ApparatusAction::Inspect),
                "begin" => gs("preparation")
                    .ok_or_else(|| "begin needs preparation".to_string())
                    .map(|preparation_id| ApparatusAction::Begin { preparation_id }),
                "grind" => Ok(ApparatusAction::Grind { inventory_slot: 0 }),
                "transfer" => args
                    .get("destination")
                    .ok_or_else(|| "transfer needs destination".to_string())
                    .and_then(planetary_pos)
                    .map(|destination| ApparatusAction::TransferMash { destination }),
                "load" => Ok(ApparatusAction::LoadCarrier { inventory_slot: 0 }),
                "media" => Ok(ApparatusAction::LoadFilter { inventory_slot: 0 }),
                "heat" => args
                    .get("temperature_millic")
                    .and_then(Value::as_i64)
                    .and_then(|value| i32::try_from(value).ok())
                    .ok_or_else(|| "heat needs temperature_millic".to_string())
                    .map(|temperature_millic| ApparatusAction::SetHeat { temperature_millic }),
                "agitate" => match gs("agitation").as_deref() {
                    Some("still") => Ok(AgitationKind::Still),
                    Some("stirred") => Ok(AgitationKind::Stirred),
                    Some("shaken") => Ok(AgitationKind::Shaken),
                    _ => Err("agitate needs still, stirred, or shaken".to_string()),
                }
                .map(|agitation| ApparatusAction::SetAgitation { agitation }),
                "advance" => step().map(|step| ApparatusAction::Advance { step }),
                "charge" => args
                    .get("units")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| "charge needs units".to_string())
                    .map(|units| ApparatusAction::Charge {
                        inventory_slot: held.as_ref().map(|_| 0),
                        units,
                    }),
                "sample" => Ok(ApparatusAction::Sample),
                "decant" => Ok(ApparatusAction::Decant { vessel_slot: 0 }),
                "clean" => gs("filter")
                    .map(|name| agent.named_slot(&name).map(|slot| slot as u8))
                    .transpose()
                    .map(|filter_slot| ApparatusAction::Clean {
                        water_slot: 0,
                        filter_slot,
                    }),
                "repair" => Ok(ApparatusAction::Repair { material_slot: 0 }),
                "drain" => route().map(|route| ApparatusAction::Drain { route }),
                "dismantle" => route().map(|route| ApparatusAction::Dismantle { route }),
                "ferment" => match (held.as_deref(), gs("wheat"), gs("berry")) {
                    (Some(water), Some(wheat), Some(berry)) => agent
                        .named_slot(water)
                        .and_then(|water_slot| {
                            Ok((
                                water_slot,
                                agent.named_slot(&wheat)?,
                                agent.named_slot(&berry)?,
                            ))
                        })
                        .map(|(water_slot, wheat_slot, berry_slot)| {
                            ApparatusAction::FermentAlcohol {
                                water_slot: water_slot as u8,
                                wheat_slot: wheat_slot as u8,
                                berry_slot: berry_slot as u8,
                            }
                        }),
                    _ => Err("ferment needs item (water), wheat, and berry".to_string()),
                },
                "press" => Ok(ApparatusAction::PressOil { seed_slot: 0 }),
                _ => Err("unknown alchemy action".to_string()),
            };
            match action {
                Ok(action) => agent
                    .operate_alchemy(pos, action, held.as_deref())
                    .unwrap_or_else(|error| error),
                Err(error) => error,
            }
        }
        "working" => {
            let Some(working) = gs("working") else {
                return "missing working id".into();
            };
            let Some(intent) = gs("intent") else {
                return "missing working intent".into();
            };
            if intent == "aim" {
                return args
                    .get("target_pos")
                    .ok_or_else(|| "aim needs target_pos".to_string())
                    .and_then(planetary_pos)
                    .and_then(|pos| agent.aim_working_at(pos))
                    .unwrap_or_else(|error| error);
            }
            if intent != "start" && intent != "force" && intent != "start_forced" {
                let intent = match intent.as_str() {
                    "hold" => Some(crate::workings::WorkingIntent::Hold),
                    "release" => Some(crate::workings::WorkingIntent::Release),
                    "cancel" => Some(crate::workings::WorkingIntent::Cancel),
                    _ => None,
                };
                return intent
                    .ok_or_else(|| "unknown working intent".to_string())
                    .and_then(|intent| agent.continue_working(intent))
                    .unwrap_or_else(|error| error);
            }
            let target_kind = gs("target").unwrap_or_else(|| "none".into());
            let target_pos = || {
                args.get("target_pos")
                    .ok_or_else(|| "that target needs target_pos".to_string())
                    .and_then(planetary_pos)
            };
            let secondary_pos = || {
                args.get("secondary_pos")
                    .ok_or_else(|| "that target needs secondary_pos".to_string())
                    .and_then(planetary_pos)
            };
            let target = match target_kind.as_str() {
                "none" => Ok(crate::workings::WorkingTargetIntent::None),
                "block" => target_pos().map(|pos| crate::workings::WorkingTargetIntent::Block {
                    pos,
                    adjacent: args
                        .get("secondary_pos")
                        .and_then(|value| planetary_pos(value).ok()),
                }),
                "water" => target_pos().and_then(|from| {
                    secondary_pos().map(|to| crate::workings::WorkingTargetIntent::Water {
                        from,
                        to,
                        water_hu: args.get("water_hu").and_then(Value::as_u64).unwrap_or(32),
                    })
                }),
                "entity" => args
                    .get("entity_id")
                    .and_then(Value::as_u64)
                    .filter(|id| *id != 0)
                    .map(|stable_id| crate::workings::WorkingTargetIntent::Entity { stable_id })
                    .ok_or_else(|| "entity target needs entity_id".to_string()),
                "inventory" => args
                    .get("target_slot")
                    .and_then(Value::as_u64)
                    .and_then(|slot| u8::try_from(slot).ok())
                    .map(
                        |target_slot| crate::workings::WorkingTargetIntent::Inventory {
                            target_slot,
                            material_slot: args
                                .get("material_slot")
                                .and_then(Value::as_u64)
                                .and_then(|slot| u8::try_from(slot).ok()),
                            magnitude: args.get("magnitude").and_then(Value::as_u64).unwrap_or(1)
                                as u32,
                        },
                    )
                    .ok_or_else(|| "inventory target needs target_slot".to_string()),
                "ritual" => target_pos()
                    .map(|controller| crate::workings::WorkingTargetIntent::Ritual { controller }),
                _ => Err("unknown working target kind".into()),
            };
            target
                .and_then(|target| {
                    agent.start_working(
                        &working,
                        target,
                        gs("item").as_deref(),
                        intent == "force" || intent == "start_forced",
                    )
                })
                .unwrap_or_else(|error| error)
        }
        "respawn" => {
            agent.send(&crate::net::C2S::Respawn);
            agent.pump_for(0.3);
            "back on my feet".into()
        }
        other => format!("unknown tool {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planetary_coordinates_use_canonical_faces_and_accept_legacy_aliases() {
        let canonical = planetary_pos(&json!({"face": "pos_z", "u": 12, "y": 34, "v": 56}))
            .expect("canonical MCP coordinate");
        let legacy = planetary_pos(&json!({"face": "PosZ", "u": 12, "y": 34, "v": 56}))
            .expect("legacy advertised MCP coordinate");
        assert_eq!(canonical, legacy);
        assert_eq!(canonical.face(), crate::planet::Face::PosZ);
        assert!(planetary_pos(&json!({"face": "POS_Z", "u": 12, "y": 34, "v": 56})).is_err());
        assert!(planetary_pos(&json!({"face": "pos_z", "u": 12.5, "y": 34, "v": 56})).is_err());
    }

    #[test]
    fn coordinate_tool_schemas_publish_bounded_integer_coordinates() {
        let tools = tool_schemas();
        let at = tools
            .iter()
            .find(|tool| tool["name"] == "at")
            .expect("at schema");
        let properties = &at["inputSchema"]["properties"];
        assert_eq!(properties["face"]["enum"][4], "pos_z");
        assert_eq!(properties["u"]["type"], "integer");
        assert_eq!(properties["u"]["maximum"], 8191);
        assert_eq!(properties["y"]["maximum"], 255);
        assert_eq!(properties["v"]["minimum"], 0);
    }
}
