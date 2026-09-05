//! Host-owned terrain preparation, wire encoding, revision tracking, and cache.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::chunk::{Chunk, ChunkPos};
use crate::registry::Registry;
use crate::server::Server;
use crate::terrain_jobs::{Priority, TerrainContext, TerrainJobs, WorkerPolicy};

const MAX_GENERATION_IN_FLIGHT: usize = 32;
const ADOPT_PER_PUMP: usize = 2;
const ADOPT_BUDGET: Duration = Duration::from_millis(4);

/// Renderer-independent cold terrain workers for a host. Generation is pure;
/// only the simulation thread adopts results and commits world side effects.
pub(super) struct HostChunkJobs {
    terrain: TerrainJobs,
    encode_request: Sender<(ChunkPos, u64, Chunk)>,
    encoded: Receiver<(ChunkPos, u64, Vec<u8>)>,
    encoding: HashSet<(ChunkPos, u64)>,
    encoded_cache: HashMap<ChunkPos, (u64, Arc<Vec<u8>>)>,
    revisions: HashMap<ChunkPos, u64>,
}

impl HostChunkJobs {
    pub(super) fn failures(&self) -> impl Iterator<Item = &crate::terrain_jobs::TerrainFailure> {
        self.terrain.failures()
    }

    pub(super) fn new(
        seed: u32,
        reg: Arc<Registry>,
        atlas: Option<Arc<crate::planet_atlas::PlanetAtlas>>,
        loader: crate::world::ChunkLoader,
    ) -> Self {
        let (encode_request, encode_rx) = channel::<(ChunkPos, u64, Chunk)>();
        let (encoded_tx, encoded) = channel();
        let terrain = TerrainJobs::new(
            TerrainContext::new(seed, reg, atlas, loader),
            WorkerPolicy::Dedicated,
        );
        let encode_rx = Arc::new(Mutex::new(encode_rx));
        for _ in 0..2 {
            let encode_rx = Arc::clone(&encode_rx);
            let encoded_tx = encoded_tx.clone();
            std::thread::spawn(move || {
                loop {
                    let request = {
                        let Ok(receiver) = encode_rx.lock() else {
                            return;
                        };
                        let Ok(request) = receiver.recv() else {
                            return;
                        };
                        request
                    };
                    let (pos, revision, chunk) = request;
                    let payload = crate::world::encode_stream_chunk(&chunk);
                    if encoded_tx.send((pos, revision, payload)).is_err() {
                        return;
                    }
                }
            });
        }
        Self {
            terrain,
            encode_request,
            encoded,
            encoding: HashSet::new(),
            encoded_cache: HashMap::new(),
            revisions: HashMap::new(),
        }
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
                server
                    .world
                    .adopt_prepared(prepared.position, prepared.chunk, fresh);
            }
            if started.elapsed() >= ADOPT_BUDGET {
                break;
            }
        }
        while let Ok((pos, revision, payload)) = self.encoded.try_recv() {
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
            && self
                .encode_request
                .send((pos, revision, chunk.clone()))
                .is_err()
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
