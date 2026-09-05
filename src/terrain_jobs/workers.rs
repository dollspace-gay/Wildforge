//! Cold load/generation execution; no world mutation, GPU, or wire encoding.

use std::io;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use super::queue::WorkQueue;
use super::result::{Completion, GenerationId, TerrainFailure};
use super::{ChunkOrigin, PreparedChunk, TerrainContext, WorkerPolicy};
use crate::world::ChunkRead;
use crate::worldgen::Generator;

pub(super) use crate::background::{Task, spawn};

pub(super) fn start(
    context: &TerrainContext,
    policy: WorkerPolicy,
    queue: &Arc<(Mutex<WorkQueue>, Condvar)>,
    ready: &Sender<Completion>,
    generation: &GenerationId,
    handles: &mut Vec<JoinHandle<()>>,
    spawn: &mut impl FnMut(String, Task) -> io::Result<JoinHandle<()>>,
) -> io::Result<()> {
    let parallelism = std::thread::available_parallelism().ok().map(usize::from);
    for index in 0..policy.count(parallelism) {
        let context = context.clone();
        let queue = Arc::clone(queue);
        let ready = ready.clone();
        let generation = generation.clone();
        handles.push(spawn(
            format!("terrain-{index}"),
            Box::new(move || {
                supervise(&queue, || run(context, &queue, ready, generation));
            }),
        )?);
    }
    Ok(())
}

/// A panic cannot be resumed safely, but these workers own only immutable
/// context and pure preparation. Stop the whole queue after unwinding; recover
/// poisoned state solely to cancel work and wake all waiters.
pub(super) fn supervise(
    queue: &Arc<(Mutex<WorkQueue>, Condvar)>,
    task: impl FnOnce() -> io::Result<()>,
) {
    let Err(error) = crate::background::run_guarded("terrain", task) else {
        return;
    };
    let (lock, wake) = &**queue;
    let mut state = lock
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    state.fail(error);
    wake.notify_all();
}

fn run(
    context: TerrainContext,
    queue: &Arc<(Mutex<WorkQueue>, Condvar)>,
    ready: Sender<Completion>,
    generation: GenerationId,
) -> io::Result<()> {
    let generator = context.atlas.map_or_else(
        || Generator::new(context.seed, &context.registry),
        |atlas| Generator::with_atlas(context.seed, &context.registry, atlas),
    );
    loop {
        let position = {
            let (lock, wake) = &**queue;
            let mut queue = lock
                .lock()
                .map_err(|_| io::Error::other("terrain queue poisoned during request"))?;
            loop {
                if let Some(position) = queue.take() {
                    break position;
                }
                if queue.is_stopped() {
                    return Ok(());
                }
                let next = wake
                    .wait(queue)
                    .map_err(|_| io::Error::other("terrain queue poisoned while waiting"))?;
                queue = next;
            }
        };
        let prepared = context
            .loader
            .load_versioned(position)
            .map(|source| {
                let (chunk, origin) = match source.content {
                    ChunkRead::Present(chunk) => (chunk, ChunkOrigin::Saved),
                    ChunkRead::Missing => (
                        generator.generate(position, &context.registry),
                        ChunkOrigin::Generated,
                    ),
                    ChunkRead::LegacyPlaceholder => {
                        eprintln!("terrain: repairing legacy all-placeholder chunk {position:?}");
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
                    revision: source.revision,
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
        let queue = queue
            .0
            .lock()
            .map_err(|_| io::Error::other("terrain queue poisoned during completion"))?;
        if queue.is_stopped() {
            return Ok(());
        }
        if ready.send(prepared).is_err() {
            return Ok(());
        }
    }
}
