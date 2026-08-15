//! Rhai script runtime for mods: event dispatch, sandboxed host API,
//! and per-mod persistent key/value storage.
//!
//! Scripts are stateless between events by design — durable state belongs in
//! the KV store (`storage_set`/`storage_get`), which is owned by the engine,
//! survives hot reloads, and is saved with the world.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use rhai::{AST, Dynamic, Engine, FuncArgs, Map, Scope};

use crate::planet::{
    BlockPos, Direction6, EntityPos, Face, SurfacePos, geodesic_distance, great_circle_bearing,
    step6,
};
use crate::world::World;

/// Deferred world mutations queued by scripts during an event, applied by the
/// game loop afterwards (scripts never hold `&mut World`).
pub enum Cmd {
    SetBlock(BlockPos, String),
    Give(String, u32),
    Hud(String),
    Sound(String),
    SpawnAnimal(String, EntityPos),
    SpawnNpc(String, EntityPos),
    QuestProgress {
        quest_id: String,
        objective: String,
        n: u32,
    },
    QuestAccept(String),
    ArcaneMoveWorking {
        mod_id: String,
        from: u64,
        to: u64,
        resonance: String,
        units: u64,
        reason: String,
    },
}

pub struct ScriptMod {
    pub id: String,
    pub ast: Option<AST>,
    pub error: Option<String>,
}

pub struct ScriptHost {
    engine: Engine,
    pub mods: Vec<ScriptMod>,
    pub queue: Rc<RefCell<Vec<Cmd>>>,
    /// mod id -> key -> value; persisted per world.
    pub kv: Rc<RefCell<HashMap<String, HashMap<String, String>>>>,
    current: Rc<RefCell<String>>,
}

thread_local! {
    static WORLD: Cell<*const World> = const { Cell::new(std::ptr::null()) };
}

/// Scoped access to the world for read-only host functions during dispatch.
struct WorldGuard;
impl WorldGuard {
    fn new(world: &World) -> WorldGuard {
        WORLD.with(|w| w.set(world as *const World));
        WorldGuard
    }
}
impl Drop for WorldGuard {
    fn drop(&mut self) {
        WORLD.with(|w| w.set(std::ptr::null()));
    }
}

fn with_world<R>(f: impl FnOnce(&World) -> R, default: R) -> R {
    WORLD.with(|w| {
        let p = w.get();
        if p.is_null() {
            default
        } else {
            // SAFETY: the pointer is set only for the duration of a dispatch
            // call that holds `&World`, on this thread, and cleared on drop.
            f(unsafe { &*p })
        }
    })
}

fn block_pos(face: &str, u: i64, y: i64, v: i64) -> Option<BlockPos> {
    BlockPos::new(
        Face::from_name(face)?,
        u16::try_from(u).ok()?,
        u8::try_from(y).ok()?,
        u16::try_from(v).ok()?,
    )
    .ok()
}

fn surface_pos(face: &str, u: i64, v: i64) -> Option<SurfacePos> {
    SurfacePos::new(
        Face::from_name(face)?,
        u16::try_from(u).ok()?,
        u16::try_from(v).ok()?,
    )
    .ok()
}

fn direction(value: &str) -> Option<Direction6> {
    match value {
        "east" => Some(Direction6::East),
        "north" => Some(Direction6::North),
        "west" => Some(Direction6::West),
        "south" => Some(Direction6::South),
        "up" => Some(Direction6::Up),
        "down" => Some(Direction6::Down),
        _ => None,
    }
}

fn pos_map(pos: BlockPos, direction: Direction6) -> Map {
    let mut map = Map::new();
    map.insert("face".into(), pos.face().name().into());
    map.insert("u".into(), i64::from(pos.u()).into());
    map.insert("y".into(), i64::from(pos.y()).into());
    map.insert("v".into(), i64::from(pos.v()).into());
    map.insert(
        "direction".into(),
        match direction {
            Direction6::East => "east",
            Direction6::North => "north",
            Direction6::West => "west",
            Direction6::South => "south",
            Direction6::Up => "up",
            Direction6::Down => "down",
        }
        .into(),
    );
    map
}

impl ScriptHost {
    pub fn new() -> ScriptHost {
        let queue: Rc<RefCell<Vec<Cmd>>> = Rc::new(RefCell::new(Vec::new()));
        let kv: Rc<RefCell<HashMap<String, HashMap<String, String>>>> =
            Rc::new(RefCell::new(HashMap::new()));
        let current: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));

        let mut engine = Engine::new();
        // Sandbox: cap runaway scripts; rhai has no fs/net access by default.
        engine.set_max_operations(200_000);
        engine.set_max_call_levels(32);
        engine.set_max_expr_depths(64, 64);

        let q = queue.clone();
        engine.register_fn(
            "set_block",
            move |face: &str, u: i64, y: i64, v: i64, block: &str| {
                if let Some(pos) = block_pos(face, u, y, v) {
                    q.borrow_mut().push(Cmd::SetBlock(pos, block.into()));
                }
            },
        );
        engine.register_fn(
            "get_block",
            |face: &str, u: i64, y: i64, v: i64| -> String {
                let Some(pos) = block_pos(face, u, y, v) else {
                    return String::new();
                };
                with_world(
                    |w| w.reg.block(w.get_block_at(pos)).name.clone(),
                    String::new(),
                )
            },
        );
        engine.register_fn("surface_height", |face: &str, u: i64, v: i64| -> i64 {
            let Some(surface) = surface_pos(face, u, v) else {
                return -1;
            };
            with_world(|w| i64::from(w.surface_height_at(surface)), -1)
        });
        engine.register_fn("arcane_estimate", |face: &str, u: i64, v: i64| -> Map {
            let Some(surface) = surface_pos(face, u, v) else {
                return Map::new();
            };
            with_world(
                |world| {
                    let mut map = Map::new();
                    let bands = world
                        .planet_atlas()
                        .map(|atlas| world.arcane_cue_at(atlas.atlas_pos(surface)))
                        .unwrap_or([0; 2]);
                    map.insert("current_band".into(), i64::from(bands[0]).into());
                    map.insert("dross_band".into(), i64::from(bands[1]).into());
                    map
                },
                Map::new(),
            )
        });
        engine.register_fn(
            "neighbor",
            |face: &str, u: i64, y: i64, v: i64, heading: &str| -> Map {
                let Some(pos) = block_pos(face, u, y, v) else {
                    return Map::new();
                };
                let Some(heading) = direction(heading) else {
                    return Map::new();
                };
                step6(pos, heading)
                    .map(|step| pos_map(step.pos, step.direction))
                    .unwrap_or_default()
            },
        );
        engine.register_fn(
            "surface_distance",
            |face_a: &str, u_a: i64, v_a: i64, face_b: &str, u_b: i64, v_b: i64| -> f64 {
                let Some(a) = surface_pos(face_a, u_a, v_a) else {
                    return -1.0;
                };
                let Some(b) = surface_pos(face_b, u_b, v_b) else {
                    return -1.0;
                };
                geodesic_distance(a.center(), b.center())
            },
        );
        engine.register_fn(
            "surface_bearing",
            |face_a: &str, u_a: i64, v_a: i64, face_b: &str, u_b: i64, v_b: i64| -> f64 {
                let Some(a) = surface_pos(face_a, u_a, v_a) else {
                    return f64::NAN;
                };
                let Some(b) = surface_pos(face_b, u_b, v_b) else {
                    return f64::NAN;
                };
                great_circle_bearing(a.center(), b.center()).unwrap_or(f64::NAN)
            },
        );
        let q = queue.clone();
        engine.register_fn("give", move |item: &str, count: i64| {
            q.borrow_mut()
                .push(Cmd::Give(item.into(), count.max(0) as u32));
        });
        let q = queue.clone();
        engine.register_fn("hud_message", move |msg: &str| {
            q.borrow_mut().push(Cmd::Hud(msg.into()));
        });
        let q = queue.clone();
        engine.register_fn("play_sound", move |name: &str| {
            q.borrow_mut().push(Cmd::Sound(name.into()));
        });
        let (q, cur) = (queue.clone(), current.clone());
        engine.register_fn(
            "arcane_move_working",
            move |from: i64, to: i64, resonance: &str, units: i64, reason: &str| {
                if from <= 0
                    || to <= 0
                    || from == to
                    || units <= 0
                    || units > i64::from(u32::MAX)
                    || resonance.len() > 96
                    || reason.is_empty()
                    || reason.len() > 128
                    || q.borrow().len() >= 256
                {
                    return;
                }
                q.borrow_mut().push(Cmd::ArcaneMoveWorking {
                    mod_id: cur.borrow().clone(),
                    from: from as u64,
                    to: to as u64,
                    resonance: resonance.into(),
                    units: units as u64,
                    reason: reason.into(),
                });
            },
        );
        let q = queue.clone();
        engine.register_fn(
            "spawn_animal",
            move |species: &str, face: &str, u: i64, y: i64, v: i64| {
                let Some(block) = block_pos(face, u, y, v) else {
                    return;
                };
                let Ok(pos) = EntityPos::new(
                    block.face(),
                    f32::from(block.u()) + 0.5,
                    f32::from(block.y()),
                    f32::from(block.v()) + 0.5,
                ) else {
                    return;
                };
                q.borrow_mut().push(Cmd::SpawnAnimal(species.into(), pos));
            },
        );
        let q = queue.clone();
        engine.register_fn(
            "spawn_npc",
            move |npc: &str, face: &str, u: i64, y: i64, v: i64| {
                let Some(block) = block_pos(face, u, y, v) else {
                    return;
                };
                let Ok(pos) = EntityPos::new(
                    block.face(),
                    f32::from(block.u()) + 0.5,
                    f32::from(block.y()),
                    f32::from(block.v()) + 0.5,
                ) else {
                    return;
                };
                q.borrow_mut().push(Cmd::SpawnNpc(npc.into(), pos));
            },
        );
        // Quest progression (spec 3.3): queue an increment; the game loop
        // applies it (never write KV from inside the script).
        let q = queue.clone();
        engine.register_fn("quest_progress", move |quest_id: &str, objective: &str, n: i64| {
            q.borrow_mut().push(Cmd::QuestProgress {
                quest_id: quest_id.into(),
                objective: objective.into(),
                n: n.max(1) as u32,
            });
        });
        // Accept a quest by id (gated on prereq by the apply side).
        let q = queue.clone();
        engine.register_fn("quest_accept", move |quest_id: &str| {
            q.borrow_mut().push(Cmd::QuestAccept(quest_id.into()));
        });
        let cur = current.clone();
        engine.register_fn("log", move |msg: &str| {
            eprintln!("[mod:{}] {msg}", cur.borrow());
        });
        let (k, cur) = (kv.clone(), current.clone());
        engine.register_fn("storage_get", move |key: &str| -> String {
            k.borrow()
                .get(&*cur.borrow())
                .and_then(|m| m.get(key))
                .cloned()
                .unwrap_or_default()
        });
        let (k, cur) = (kv.clone(), current.clone());
        engine.register_fn("storage_set", move |key: &str, value: &str| {
            k.borrow_mut()
                .entry(cur.borrow().clone())
                .or_default()
                .insert(key.into(), value.into());
        });

        ScriptHost {
            engine,
            mods: Vec::new(),
            queue,
            kv,
            current,
        }
    }

    /// Compile `main.rhai` for each mod dir. On error, keeps the previous
    /// AST for that mod (if any) so a typo doesn't kill a session.
    pub fn load_mods(&mut self, mods: &[(String, std::path::PathBuf)]) {
        let mut next: Vec<ScriptMod> = Vec::new();
        for (id, dir) in mods {
            let path = dir.join("main.rhai");
            if !path.exists() {
                continue;
            }
            let old = self.mods.iter_mut().find(|m| &m.id == id);
            match std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|src| self.engine.compile(&src).map_err(|e| e.to_string()))
            {
                Ok(ast) => next.push(ScriptMod {
                    id: id.clone(),
                    ast: Some(ast),
                    error: None,
                }),
                Err(e) => {
                    let kept = old.and_then(|m| m.ast.take());
                    next.push(ScriptMod {
                        id: id.clone(),
                        ast: kept,
                        error: Some(format!("{id}/main.rhai: {e}")),
                    });
                }
            }
        }
        self.mods = next;
    }

    /// Dispatch an event to every mod that defines it. Returns false if any
    /// handler explicitly returned `false` (cancels cancellable events).
    pub fn dispatch(&mut self, world: &World, event: &str, args: impl FuncArgs + Clone) -> bool {
        let _guard = WorldGuard::new(world);
        let mut allow = true;
        for m in &self.mods {
            let Some(ast) = &m.ast else { continue };
            if !ast.iter_functions().any(|f| f.name == event) {
                continue;
            }
            *self.current.borrow_mut() = m.id.clone();
            let mut scope = Scope::new();
            match self
                .engine
                .call_fn::<Dynamic>(&mut scope, ast, event, args.clone())
            {
                Ok(ret) => {
                    if ret.as_bool() == Ok(false) {
                        allow = false;
                    }
                }
                Err(e) => eprintln!("[mod:{}] {event}: {e}", m.id),
            }
        }
        allow
    }

    /// Does any loaded mod define this event? (Skip arg building otherwise.)
    pub fn wants(&self, event: &str) -> bool {
        self.mods.iter().any(|m| {
            m.ast
                .as_ref()
                .is_some_and(|a| a.iter_functions().any(|f| f.name == event))
        })
    }

    /// Run one named hook function (`"mod:fn"` or bare `"fn"` = any mod that
    /// defines it) over string args, returning its raw return value. Used by
    /// dialogue `condition`/`callback` and node-text hooks so scripts can
    /// gate node availability, mutate flags, and return interpolated text.
    pub fn run_fn(
        &mut self,
        world: &World,
        hook: &crate::registry::ScriptHook,
        args: Vec<String>,
    ) -> Dynamic {
        let _guard = WorldGuard::new(world);
        let hook_mod = &hook.mod_id;
        for m in &self.mods {
            let Some(ast) = &m.ast else { continue };
            if !hook_mod.is_empty() && &m.id != hook_mod {
                continue;
            }
            if !ast.iter_functions().any(|f| f.name == hook.fn_name) {
                continue;
            }
            *self.current.borrow_mut() = m.id.clone();
            let mut scope = Scope::new();
            let dyn_args: Vec<Dynamic> = args
                .iter()
                .map(|s| Dynamic::from(s.clone()))
                .collect();
            match self
                .engine
                .call_fn::<Dynamic>(&mut scope, ast, &hook.fn_name, dyn_args)
            {
                Ok(ret) => return ret,
                Err(e) => eprintln!("[mod:{}] {}: {e}", m.id, hook.fn_name),
            }
        }
        Dynamic::UNIT
    }

    pub fn take_cmds(&self) -> Vec<Cmd> {
        std::mem::take(&mut self.queue.borrow_mut())
    }

    // ---- KV persistence (saved with the world) ----

    pub fn load_kv(&self, world_dir: &Path) {
        let mut kv = self.kv.borrow_mut();
        kv.clear();
        if let Ok(text) = std::fs::read_to_string(world_dir.join("modstore.toml"))
            && let Ok(parsed) = toml::from_str::<HashMap<String, HashMap<String, String>>>(&text)
        {
            *kv = parsed;
        }
    }

    pub fn save_kv(&self, world_dir: &Path) -> std::io::Result<()> {
        let kv = self.kv.borrow();
        let path = world_dir.join("modstore.toml");
        if kv.is_empty() {
            return crate::persist::remove_if_exists(&path);
        }
        let text = toml::to_string(&*kv).map_err(std::io::Error::other)?;
        crate::identity::atomic_write(&path, text.as_bytes(), false)
    }
}
