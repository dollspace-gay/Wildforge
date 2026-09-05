//! Cancellable immutable homeland trials, before authoritative adoption.

use std::io;
use std::sync::Arc;
use std::time::Duration;

use crate::background::SnapshotJobs;
use crate::chunk::{Chunk, ChunkPos};
use crate::planet_atlas::{CancellationToken, PlanetAtlas};
use crate::registry::Registry;
use crate::worldgen::Generator;

pub(super) fn check_cancelled(cancel: &CancellationToken) -> io::Result<()> {
    if cancel.is_cancelled() {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "world preparation cancelled",
        ))
    } else {
        Ok(())
    }
}

pub(super) fn generate_trial_region(
    seed: u32,
    reg: Arc<Registry>,
    atlas: Arc<PlanetAtlas>,
    positions: &[ChunkPos],
    cancel: &CancellationToken,
    mut progress: impl FnMut(usize, usize),
) -> io::Result<Vec<(ChunkPos, Chunk)>> {
    check_cancelled(cancel)?;
    if positions.is_empty() {
        return Ok(Vec::new());
    }
    let worker_count = std::thread::available_parallelism()
        .map(|count| count.get().saturating_sub(2).clamp(2, 4))
        .unwrap_or(2)
        .min(positions.len());
    let mut jobs =
        SnapshotJobs::with_initializer("homeland", worker_count, positions.len(), move || {
            let reg = Arc::clone(&reg);
            let generator = Generator::with_atlas(seed, &reg, Arc::clone(&atlas));
            move |position| (position, generator.generate(position, &reg))
        })?;
    for position in positions {
        if !jobs.request(*position) {
            return Err(worker_error(&jobs));
        }
    }
    let mut generated = Vec::with_capacity(positions.len());
    while generated.len() < positions.len() {
        if jobs.failure().is_some() {
            return Err(worker_error(&jobs));
        }
        check_cancelled(cancel)?;
        if let Some(result) = jobs.try_ready() {
            generated.push(result);
            progress(generated.len(), positions.len());
        } else {
            // This orchestration itself runs off the UI thread. Bound the
            // cancellation observation interval without spinning during work.
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    jobs.shutdown()?;
    check_cancelled(cancel)?;
    generated.sort_by_key(|(position, _)| *position);
    Ok(generated)
}

fn worker_error(jobs: &SnapshotJobs<ChunkPos, (ChunkPos, Chunk)>) -> io::Error {
    jobs.failure().map_or_else(
        || io::Error::other("homeland generation workers stopped"),
        |error| {
            io::Error::new(
                error.kind(),
                format!("homeland preparation failed: {error}"),
            )
        },
    )
}

#[cfg(test)]
#[path = "preparation_tests.rs"]
mod tests;
