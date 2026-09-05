//! Host-owned terrain preparation, wire encoding, revision tracking, and cache.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::background::SnapshotJobs;
use std::time::{Duration, Instant};

use crate::chunk::{Chunk, ChunkPos};
use crate::registry::Registry;
use crate::server::Server;
use crate::terrain_jobs::{Priority, TerrainContext, TerrainJobs, WorkerPolicy};

const MAX_GENERATION_IN_FLIGHT: usize = 32;
const MAX_ENCODING_IN_FLIGHT: usize = 32;

type EncodeRequest = (ChunkPos, u64, Chunk);
type EncodedChunk = (ChunkPos, u64, Vec<u8>);
const ADOPT_PER_PUMP: usize = 2;
const ADOPT_BUDGET: Duration = Duration::from_millis(4);

/// Renderer-independent cold terrain workers for a host. Generation is pure;
/// only the simulation thread adopts results and commits world side effects.
pub(super) struct HostChunkJobs {
    terrain: TerrainJobs,
    encoder: SnapshotJobs<EncodeRequest, EncodedChunk>,
    encoding: HashSet<(ChunkPos, u64)>,
    encoded_cache: HashMap<ChunkPos, (u64, Arc<Vec<u8>>)>,
    revisions: HashMap<ChunkPos, u64>,
}

impl HostChunkJobs {
    #[cfg(test)]
    pub(super) fn panic_worker_for_test(&mut self) {
        self.terrain.panic_worker_for_test();
    }

    pub(super) fn fatal_failure(&self) -> Option<Arc<std::io::Error>> {
        self.terrain
            .fatal_failure()
            .or_else(|| self.encoder.failure())
    }

    pub(super) fn failures(&self) -> impl Iterator<Item = &crate::terrain_jobs::TerrainFailure> {
        self.terrain.failures()
    }

    pub(super) fn new(
        seed: u32,
        reg: Arc<Registry>,
        atlas: Option<Arc<crate::planet_atlas::PlanetAtlas>>,
        loader: crate::world::ChunkLoader,
    ) -> std::io::Result<Self> {
        let terrain = TerrainJobs::new(
            TerrainContext::new(seed, reg, atlas, loader),
            WorkerPolicy::Dedicated,
        )?;
        let encoder = SnapshotJobs::new(
            "chunk-encode",
            2,
            MAX_ENCODING_IN_FLIGHT,
            |(pos, revision, chunk): EncodeRequest| {
                (pos, revision, crate::world::encode_stream_chunk(&chunk))
            },
        )?;
        Ok(Self {
            terrain,
            encoder,
            encoding: HashSet::new(),
            encoded_cache: HashMap::new(),
            revisions: HashMap::new(),
        })
    }

    pub(super) fn enqueue(&mut self, pos: ChunkPos, entry_priority: bool) {
        let priority = if entry_priority {
            Priority::Entry
        } else {
            Priority::Ordinary
        };
        self.terrain
            .request(pos, priority, MAX_GENERATION_IN_FLIGHT);
    }

    pub(super) fn cancel_queued(&mut self, wanted: &impl Fn(ChunkPos) -> bool) {
        self.terrain.cancel_queued(wanted);
        for (pos, revision, _) in self.encoder.cancel_queued(|(pos, _, _)| wanted(*pos)) {
            self.encoding.remove(&(pos, revision));
        }
    }

    pub(super) fn drain_into(&mut self, server: &mut Server, wanted: &impl Fn(ChunkPos) -> bool) {
        let started = Instant::now();
        for _ in 0..ADOPT_PER_PUMP {
            let Some(prepared) = self.terrain.try_ready() else {
                break;
            };
            if let Ok(prepared) = prepared
                && wanted(prepared.position)
            {
                let fresh = prepared.is_fresh();
                server.world.adopt_prepared_at_revision(
                    prepared.position,
                    prepared.chunk,
                    fresh,
                    &prepared.revision,
                );
            }
            if started.elapsed() >= ADOPT_BUDGET {
                break;
            }
        }
        while let Some((pos, revision, payload)) = self.encoder.try_ready() {
            self.encoding.remove(&(pos, revision));
            if wanted(pos) && self.revisions.get(&pos).copied().unwrap_or(0) == revision {
                self.encoded_cache
                    .insert(pos, (revision, Arc::new(payload)));
            }
        }
        self.encoded_cache.retain(|position, _| wanted(*position));
    }

    pub(super) fn encoded_or_enqueue(
        &mut self,
        pos: ChunkPos,
        chunk: &Chunk,
    ) -> Option<Arc<Vec<u8>>> {
        let revision = self.revisions.get(&pos).copied().unwrap_or(0);
        if let Some((cached_revision, payload)) = self.encoded_cache.get(&pos)
            && *cached_revision == revision
        {
            return Some(Arc::clone(payload));
        }
        if self.encoding.insert((pos, revision))
            && !self.encoder.request((pos, revision, chunk.clone()))
        {
            self.encoding.remove(&(pos, revision));
        }
        None
    }

    pub(super) fn invalidate_encoded(&mut self, pos: ChunkPos) {
        let revision = self.revisions.entry(pos).or_default();
        *revision = revision.wrapping_add(1);
        self.encoded_cache.remove(&pos);
    }
}

impl Drop for HostChunkJobs {
    fn drop(&mut self) {
        // Stop both queues before waiting for either kind of active work.
        self.terrain.stop();
        self.encoder.stop();
        let terrain = self.terrain.shutdown();
        let encoding = self.encoder.shutdown();
        for result in [terrain, encoding] {
            if let Err(error) = result {
                eprintln!("host chunk jobs: {error}");
            }
        }
    }
}
