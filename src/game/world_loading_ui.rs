//! Presentation and capture policy for the owned local-world loading operation.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::world_loading::{CreationRequest, EntryRequest, LoadingEvent, LoadingKind};
use super::{Game, Screen};

impl Game {
    pub(super) fn start_world(&mut self, name: &str) {
        self.start_world_with_origin(name, false);
    }

    fn start_world_with_origin(&mut self, name: &str, created_here: bool) {
        if self.loading.is_active() {
            return;
        }
        if let Err(error) = self.content.validate() {
            eprintln!("world: entry refused: {error}");
            self.toast(format!("Could not enter world: {error}"));
            return;
        }
        // Capture fixtures may pin an exact atlas site instead of the common homeland.
        let override_wanted = std::env::var("WILDFORGE_SPAWN").ok().and_then(|s| {
            let mut fields = s.split(',').map(str::trim);
            let face = crate::planet::Face::from_name(fields.next()?)?;
            let u = fields.next()?.parse().ok()?;
            let v = fields.next()?.parse().ok()?;
            fields.next().is_none().then_some(())?;
            crate::planet::SurfacePos::new(face, u, v).ok()
        });
        let result = self.loading.enter(EntryRequest {
            name: name.into(),
            save_dir: PathBuf::from("saves").join(name),
            reg: Arc::clone(&self.content.reg),
            device_id: self.identity.device_id(),
            override_wanted,
            created_here,
        });
        if let Err(error) = result {
            self.loading_failure(name, LoadingKind::Entry { created_here }, error);
            return;
        }
        self.ui_state.creation_status = "OPENING PLANET".into();
        self.ui_state.creation_progress = (0, 1);
        self.set_screen(Screen::CreatingWorld);
    }

    pub(super) fn create_new_world(&mut self) {
        if self.loading.is_active() {
            return;
        }
        if let Err(error) = self.content.validate() {
            eprintln!("world: creation refused: {error}");
            self.ui_state.new_world_status = error.to_string().to_uppercase();
            return;
        }
        let Ok(seed) = self.ui_state.new_world_seed.parse::<u32>() else {
            self.ui_state.new_world_status = "SEED MUST BE AN INTEGER FROM 0 TO 4294967295".into();
            return;
        };
        self.ui_state.new_world_status.clear();
        let name = next_world_name(Path::new("saves"), &self.worlds);
        let result = self.loading.create(CreationRequest {
            name: name.clone(),
            destination: PathBuf::from("saves").join(&name),
            seed,
            mode: self.ui_state.new_world_mode.clone(),
            content_hash: crate::planet_atlas::genesis_content_hash(Path::new("mods")),
            reg: Arc::clone(&self.content.reg),
        });
        if let Err(error) = result {
            self.loading_failure(&name, LoadingKind::Creation, error);
            return;
        }
        self.ui_state.creation_status = "SHAPING PLANET".into();
        self.ui_state.creation_progress = (0, crate::planet_atlas::AtlasStage::ALL.len());
        self.set_screen(Screen::CreatingWorld);
    }

    pub(super) fn cancel_world_creation(&mut self) {
        self.ui_state.creation_status = match self.loading.cancel() {
            Some(LoadingKind::Creation) => "CANCELLING PLANET CREATION".into(),
            Some(LoadingKind::Entry { .. }) => "CANCELLING WORLD ENTRY".into(),
            None => return,
        };
    }

    pub(super) fn poll_world_creation(&mut self) {
        let Some(event) = self.loading.poll(&self.content.reg) else {
            return;
        };
        match event {
            LoadingEvent::Progress(progress) => {
                self.ui_state.creation_status = progress.stage;
                self.ui_state.creation_progress = (progress.completed, progress.total);
            }
            LoadingEvent::Created { name, enter } => {
                self.refresh_worlds();
                if enter {
                    self.start_world_with_origin(&name, true);
                } else {
                    self.set_screen(Screen::Title);
                    self.toast("Planet created; world entry cancelled".into());
                }
            }
            LoadingEvent::Ready { name, prepared } => {
                let super::world_loading::PreparedWorld {
                    world,
                    spawn,
                    notices,
                } = *prepared;
                self.finish_world_entry(&name, world, spawn, notices);
            }
            LoadingEvent::Reenter { name, created_here } => {
                self.start_world_with_origin(&name, created_here)
            }
            LoadingEvent::Cancelled(kind) => {
                self.refresh_worlds();
                self.set_screen(match kind {
                    LoadingKind::Creation | LoadingKind::Entry { created_here: true } => {
                        Screen::NewWorld
                    }
                    LoadingKind::Entry {
                        created_here: false,
                    } => Screen::Title,
                });
                self.toast(match kind {
                    LoadingKind::Creation => "Planet creation cancelled".into(),
                    LoadingKind::Entry { .. } => "World entry cancelled".into(),
                });
            }
            LoadingEvent::Failed { name, kind, error } => self.loading_failure(&name, kind, error),
        }
    }

    fn loading_failure(&mut self, name: &str, kind: LoadingKind, error: std::io::Error) {
        eprintln!("world: {kind:?} of {name} failed: {error}");
        match kind {
            LoadingKind::Creation => {
                self.ui_state.new_world_status = format!("CREATION FAILED: {error}");
                self.set_screen(Screen::NewWorld);
                self.toast(format!("Could not create world: {error}"));
            }
            LoadingKind::Entry { created_here } => {
                if created_here {
                    self.ui_state.new_world_status =
                        format!("WORLD WAS CREATED, BUT ENTRY FAILED: {error}");
                    self.set_screen(Screen::NewWorld);
                } else {
                    self.set_screen(Screen::Title);
                }
                self.toast(format!("Could not enter world: {error}"));
            }
        }
    }
}

/// First free "worldN" name. A name is taken if it's in the world list OR
/// its folder exists on disk at all — a new world must never adopt an
/// existing folder's chunks/player.toml, even one the listing can't parse.
pub(crate) fn next_world_name(saves: &std::path::Path, worlds: &[(String, u32)]) -> String {
    let mut n = 1;
    loop {
        let name = format!("world{n}");
        if !worlds.iter().any(|(w, _)| w == &name) && !saves.join(&name).exists() {
            return name;
        }
        n += 1;
    }
}

