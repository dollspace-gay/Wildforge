//! Cold load/generation execution; no world mutation, GPU, or wire encoding.

use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use super::queue::WorkQueue;
use super::result::{Completion, GenerationId, TerrainFailure};
use super::{ChunkOrigin, PreparedChunk, TerrainContext, WorkerPolicy};
use crate::world::ChunkRead;
use crate::worldgen::Generator;

pub(super) fn start(
    context: &TerrainContext,
    policy: WorkerPolicy,
    queue: &Arc<(Mutex<WorkQueue>, Condvar)>,
    ready: &Sender<Completion>,
    generation: &GenerationId,
    handles: &mut Vec<JoinHandle<()>>,
) {
    let parallelism = std::thread::available_parallelism().ok().map(usize::from);
    for _ in 0..policy.count(parallelism) {
        let context = context.clone();
        let queue = Arc::clone(queue);
        let ready = ready.clone();
        let generation = generation.clone();
        handles.push(std::thread::spawn(move || {
            let generator = context.atlas.map_or_else(
                || Generator::new(context.seed, &context.registry),
                |atlas| Generator::with_atlas(context.seed, &context.registry, atlas),
            );
            loop {
                let position = {
                    let (lock, wake) = &*queue;
                    let Ok(mut queue) = lock.lock() else {
                        return;
                    };
                    loop {
                        if let Some(position) = queue.take() {
                            break position;
                        }
                        if queue.is_stopped() {
                            return;
                        }
                        let Ok(next) = wake.wait(queue) else {
                            return;
                        };
                        queue = next;
                    }
                };
                let prepared = context
                    .loader
                    .load(position)
                    .map(|source| {
                        let (chunk, origin) = match source {
                            ChunkRead::Present(chunk) => (chunk, ChunkOrigin::Saved),
                            ChunkRead::Missing => (
                                generator.generate(position, &context.registry),
                                ChunkOrigin::Generated,
                            ),
                            ChunkRead::LegacyPlaceholder => {
                                eprintln!(
                                    "terrain: repairing legacy all-placeholder chunk {position:?}"
                                );
                                (
                                    generator.generate(position, &context.registry),
                                    ChunkOrigin::RepairedPlaceholder,
                                )
                            }
                        };
                        PreparedChunk {
                            position,
                            chunk,
                            origin,
                            generation: generation.clone(),
                        }
                    })
                    .map_err(|source| TerrainFailure {
                        position,
                        source: Arc::new(source),
                        generation: generation.clone(),
                    });
                // Publish under the queue lock: shutdown either cancels this
                // result or drains it after joining; it cannot arrive later.
                let Ok(queue) = queue.0.lock() else {
                    return;
                };
                if queue.is_stopped() {
                    return;
                }
                if ready.send(prepared).is_err() {
                    return;
                }
            }
        }));
    }
}
