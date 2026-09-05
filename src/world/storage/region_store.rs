//! Session-owned region coordination and in-flight saved-terrain revisions.

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};

use crate::chunk::ChunkPos;
use crate::world::region;

#[derive(Default)]
struct RegionState {
    watched: HashMap<ChunkPos, Weak<AtomicBool>>,
}

struct StoreState {
    directory: PathBuf,
    regions: Mutex<HashMap<PathBuf, Weak<Mutex<RegionState>>>>,
}

/// One world's persistence owner. Clones share locks, never global I/O state.
#[derive(Clone)]
pub(in crate::world) struct RegionStore(Arc<StoreState>);

/// Opaque proof that a read still describes this world's persisted chunk.
///
/// Tokens retain their region lock so a later write can invalidate them even
/// after the read finishes. Weak watches disappear once no prepared result
/// needs them; this is not a growing journal of every chunk ever saved.
pub(crate) struct ChunkRevision {
    store: Arc<StoreState>,
    position: ChunkPos,
    _region: Arc<Mutex<RegionState>>,
    changed: Arc<AtomicBool>,
}

impl RegionStore {
    pub(in crate::world) fn new(directory: PathBuf) -> Self {
        Self(Arc::new(StoreState {
            directory,
            regions: Mutex::new(HashMap::new()),
        }))
    }

    fn region(&self, position: ChunkPos) -> io::Result<Arc<Mutex<RegionState>>> {
        let mut regions = self
            .0
            .regions
            .lock()
            .map_err(|_| io::Error::other("region coordination index poisoned"))?;
        regions.retain(|_, region| region.strong_count() != 0);
        let path = region::region_path(&self.0.directory, position);
        if let Some(region) = regions.get(&path).and_then(Weak::upgrade) {
            return Ok(region);
        }
        let region = Arc::new(Mutex::new(RegionState::default()));
        regions.insert(path, Arc::downgrade(&region));
        Ok(region)
    }

    /// Hold the region lock through raw I/O; decoding happens after release.
    pub(in crate::world) fn read(
        &self,
        position: ChunkPos,
    ) -> io::Result<(Option<Vec<u8>>, ChunkRevision)> {
        let region = self.region(position)?;
        let mut state = region
            .lock()
            .map_err(|_| io::Error::other("saved region poisoned during read"))?;
        let bytes = region::read_chunk(&self.0.directory, position)?;
        state.watched.retain(|_, watch| watch.strong_count() != 0);
        let changed = state
            .watched
            .get(&position)
            .and_then(Weak::upgrade)
            .unwrap_or_else(|| {
                let changed = Arc::new(AtomicBool::new(false));
                state.watched.insert(position, Arc::downgrade(&changed));
                changed
            });
        drop(state);
        Ok((
            bytes,
            ChunkRevision {
                store: Arc::clone(&self.0),
                position,
                _region: region,
                changed,
            },
        ))
    }

    /// Invalidate before attempting a write: even a failed write may have
    /// published bytes. Compaction stays within the same region lock.
    pub(in crate::world) fn write(&self, position: ChunkPos, bytes: &[u8]) -> io::Result<()> {
        let region = self.region(position)?;
        let mut state = region
            .lock()
            .map_err(|_| io::Error::other("saved region poisoned during write"))?;
        if let Some(changed) = state
            .watched
            .remove(&position)
            .and_then(|weak| weak.upgrade())
        {
            changed.store(true, Ordering::Release);
        }
        region::write_chunk(&self.0.directory, position, bytes)
    }

    /// Adoption runs on the only authoritative writer's thread. This check
    /// needs no file I/O or contended region lock on the live game/host pump.
    pub(in crate::world) fn is_current(
        &self,
        position: ChunkPos,
        revision: &ChunkRevision,
    ) -> bool {
        position == revision.position
            && Arc::ptr_eq(&self.0, &revision.store)
            && !revision.changed.load(Ordering::Acquire)
    }
}

#[cfg(test)]
#[path = "region_store_tests.rs"]
mod tests;
