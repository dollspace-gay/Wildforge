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

use rhai::{AST, Dynamic, Engine, FuncArgs, Scope};

use crate::world::World;

/// Deferred world mutations queued by scripts during an event, applied by the
/// game loop afterwards (scripts never hold `&mut World`).
pub enum Cmd {
    SetBlock(i32, i32, i32, String),
    Give(String, u32),
    Hud(String),
    Sound(String),
    SpawnAnimal(String, f32, f32, f32),
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
        engine.register_fn("set_block", move |x: i64, y: i64, z: i64, block: &str| {
            q.borrow_mut()
                .push(Cmd::SetBlock(x as i32, y as i32, z as i32, block.into()));
        });
        engine.register_fn("get_block", |x: i64, y: i64, z: i64| -> String {
            with_world(
                |w| {
                    w.reg
                        .block(w.get_block(x as i32, y as i32, z as i32))
                        .name
                        .clone()
                },
                String::new(),
            )
        });
        engine.register_fn("surface_height", |x: i64, z: i64| -> i64 {
            with_world(|w| w.surface_height(x as i32, z as i32) as i64, 0)
        });
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
        let q = queue.clone();
        engine.register_fn(
            "spawn_animal",
            move |species: &str, x: i64, y: i64, z: i64| {
                q.borrow_mut().push(Cmd::SpawnAnimal(
                    species.into(),
                    x as f32 + 0.5,
                    y as f32,
                    z as f32 + 0.5,
                ));
            },
        );
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

    pub fn save_kv(&self, world_dir: &Path) {
        let kv = self.kv.borrow();
        if kv.is_empty() {
            return;
        }
        if let Ok(text) = toml::to_string(&*kv) {
            let _ = std::fs::write(world_dir.join("modstore.toml"), text);
        }
    }
}
