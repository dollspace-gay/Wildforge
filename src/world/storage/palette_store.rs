//! Session-owned palette publication and immutable worker mappings.

use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::palette::{Mapping, Palette};
use crate::registry::Registry;

#[derive(Clone, Debug)]
pub(in crate::world) struct PaletteSnapshot(Result<Mapping, Arc<io::Error>>);

impl PaletteSnapshot {
    pub(super) fn mapping(&self) -> io::Result<&Mapping> {
        self.0
            .as_ref()
            .map_err(|error| io::Error::new(error.kind(), Arc::clone(error)))
    }

    pub(in crate::world) fn validate(&self) -> io::Result<()> {
        self.mapping().map(|_| ())
    }

    #[cfg(test)]
    pub(super) fn for_decode(decode: Vec<crate::registry::BlockId>) -> Arc<Self> {
        Arc::new(Self(Ok(Mapping {
            decode,
            encode: Vec::new(),
        })))
    }
}

struct State {
    palette: Option<Palette>,
    registry: Arc<Registry>,
    snapshot: Arc<PaletteSnapshot>,
    dirty: bool,
}

impl State {
    fn new(path: &std::path::Path, registry: &Arc<Registry>) -> Self {
        let loaded = Palette::read(path);
        let dirty = matches!(&loaded, Ok(None));
        let (palette, result) = match loaded {
            Ok(palette) => {
                let mut palette = palette.unwrap_or_default();
                let result = palette.bind(registry);
                (Some(palette), result)
            }
            Err(error) => (None, Err(error)),
        };
        let changed = result.as_ref().is_ok_and(|(_, changed)| *changed);
        Self {
            palette,
            registry: Arc::clone(registry),
            snapshot: Arc::new(PaletteSnapshot(
                result.map(|(mapping, _)| mapping).map_err(Arc::new),
            )),
            dirty: dirty || changed,
        }
    }

    fn bind(&mut self, registry: &Arc<Registry>) {
        if Arc::ptr_eq(&self.registry, registry) {
            return;
        }
        self.registry = Arc::clone(registry);
        if let Some(palette) = &mut self.palette {
            let result = palette.bind(registry);
            self.dirty |= result.as_ref().is_ok_and(|(_, changed)| *changed);
            self.snapshot = Arc::new(PaletteSnapshot(
                result.map(|(mapping, _)| mapping).map_err(Arc::new),
            ));
        }
    }
}

pub(in crate::world) struct PaletteStore {
    path: PathBuf,
    state: Mutex<State>,
    poisoned: Arc<PaletteSnapshot>,
}

impl PaletteStore {
    /// Refuse damaged input before world entry can create or update sidecars.
    pub(in crate::world) fn validate_saved(directory: &std::path::Path) -> io::Result<()> {
        Palette::read(&directory.join("palette")).map(|_| ())
    }

    pub(in crate::world) fn new(directory: &std::path::Path, registry: &Arc<Registry>) -> Self {
        let path = directory.join("palette");
        Self {
            state: Mutex::new(State::new(&path, registry)),
            path,
            poisoned: Arc::new(PaletteSnapshot(Err(Arc::new(io::Error::other(
                "block palette owner poisoned",
            ))))),
        }
    }

    /// No filesystem access on the streaming pump. Failed initialization is
    /// retained as an immutable failure until the World session is reopened.
    pub(in crate::world) fn snapshot(&self, registry: &Arc<Registry>) -> Arc<PaletteSnapshot> {
        let Ok(mut state) = self.state.lock() else {
            return Arc::clone(&self.poisoned);
        };
        state.bind(registry);
        Arc::clone(&state.snapshot)
    }

    /// Publish new bindings before any chunk can use them. Failed publication
    /// retains the dirty table for retry and never changes the reader snapshot.
    pub(in crate::world) fn publish(
        &self,
        registry: &Arc<Registry>,
    ) -> io::Result<Arc<PaletteSnapshot>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("block palette owner poisoned"))?;
        state.bind(registry);
        state.snapshot.validate()?;
        if state.dirty {
            let palette = state
                .palette
                .as_ref()
                .ok_or_else(|| io::Error::other("block palette is unavailable"))?;
            crate::world::persistence::atomic_replace(&self.path, palette.text().as_bytes())?;
            state.dirty = false;
            // Retire any result prepared before the naming table was durable.
            state.snapshot = Arc::new((*state.snapshot).clone());
        }
        Ok(Arc::clone(&state.snapshot))
    }
}
