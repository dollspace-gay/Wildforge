//! Cold load/generation execution; no world mutation, GPU, or wire encoding.

use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use super::queue::WorkQueue;
use super::result::GenerationId;
use super::{ChunkOrigin, PreparedChunk, TerrainContext, WorkerPolicy};
use crate::worldgen::Generator;

pub(super) fn start(
    context: &TerrainContext,
    policy: WorkerPolicy,
    queue: &Arc<(Mutex<WorkQueue>, Condvar)>,
    ready: &Sender<PreparedChunk>,
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
                let (chunk, origin) = match context.loader.load(position) {
                    Some(chunk) => (chunk, ChunkOrigin::Saved),
                    None => (
                        generator.generate(position, &context.registry),
                        ChunkOrigin::Generated,
                    ),
                };
                // Publish under the queue lock: shutdown either cancels this
                // result or drains it after joining; it cannot arrive later.
                let Ok(queue) = queue.0.lock() else {
                    return;
                };
                if queue.is_stopped() {
                    return;
                }
                if ready
                    .send(PreparedChunk {
                        position,
                        chunk,
                        origin,
                        generation: generation.clone(),
                    })
                    .is_err()
                {
                    return;
                }
            }
        }));
    }
}
