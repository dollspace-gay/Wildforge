//! Worker-side preparation; no window, renderer, or live session is borrowed.

use std::io;
use std::path::Path;

use super::{CreationRequest, EntryRequest, LoadingProgress, PreparedWorld, WorkOutput};
use crate::background::Progress;
use crate::planet::{EntityPos, SurfacePos};
use crate::planet_atlas::CancellationToken;
use crate::world::{World, WorldCreationProgress};

pub(super) fn create(
    request: CreationRequest,
    progress: Progress<LoadingProgress>,
    cancel: CancellationToken,
) -> io::Result<WorkOutput> {
    let mut progress_error = None;
    let result = crate::world::create_world_atomic(
        &request.destination,
        request.seed,
        &request.mode,
        request.content_hash,
        request.reg,
        &cancel,
        |value| {
            let value = match value {
                WorldCreationProgress::Atlas(p) => LoadingProgress {
                    stage: p.stage.label().into(),
                    completed: p.completed_stages,
                    total: p.total_stages,
                },
                WorldCreationProgress::Arcane(p) => LoadingProgress {
                    stage: p.stage.label().into(),
                    completed: p.completed_stages,
                    total: p.total_stages,
                },
                WorldCreationProgress::Homeland {
                    stage,
                    completed,
                    total,
                } => LoadingProgress {
                    stage: stage.to_uppercase(),
                    completed,
                    total,
                },
            };
            if let Err(error) = progress.publish(value) {
                progress_error = Some(error);
                cancel.cancel();
            }
        },
    );
    if let Some(error) = progress_error {
        return Err(error);
    }
    result.map(|()| WorkOutput::Created)
}

pub(super) fn enter(
    request: EntryRequest,
    progress: Progress<LoadingProgress>,
    cancel: CancellationToken,
) -> io::Result<WorkOutput> {
    check_cancelled(&cancel)?;
    let mut world =
        World::load_or_create_cancellable(request.save_dir.clone(), request.reg, &cancel)?;
    check_cancelled(&cancel)?;
    // Creating the profile parent before atomic world creation would make
    // a fresh world look like an already existing destination.
    let profile_path =
        crate::identity::local_profile_path(&request.save_dir, request.device_id).ok();
    let notices = world
        .material_ledger
        .as_ref()
        .map_or_else(Vec::new, |ledger| ledger.retrogen_notices());
    let spawn = if let Some(wanted) = request.override_wanted {
        prepare_doorstep(
            &mut world,
            wanted,
            "LOADING DEVELOPMENT ENTRY",
            &progress,
            &cancel,
        )?;
        world.safe_spawn_at(wanted)
    } else {
        let mut progress_error = None;
        let result = world.prepare_common_spawn_cancellable(&cancel, |stage, completed, total| {
            if let Err(error) = progress.publish(LoadingProgress {
                stage: stage.to_uppercase(),
                completed,
                total,
            }) {
                progress_error = Some(error);
            }
        });
        if let Some(error) = progress_error {
            return Err(error);
        }
        result?
    };
    check_cancelled(&cancel)?;
    if let Some(saved) = profile_path.as_deref().and_then(saved_profile_position) {
        prepare_doorstep(
            &mut world,
            saved.surface(),
            "LOADING SAVED DOORSTEP",
            &progress,
            &cancel,
        )?;
    }
    check_cancelled(&cancel)?;
    Ok(WorkOutput::Ready(Box::new(PreparedWorld {
        world,
        spawn,
        notices,
    })))
}

fn prepare_doorstep(
    world: &mut World,
    surface: SurfacePos,
    stage: &str,
    progress: &Progress<LoadingProgress>,
    cancel: &CancellationToken,
) -> io::Result<()> {
    let chunks = crate::world::player_entry_chunks(surface);
    for (index, position) in chunks.iter().copied().enumerate() {
        check_cancelled(cancel)?;
        world.try_ensure_chunk(position)?;
        if !world.has_chunk(position) {
            return Err(io::Error::other(format!(
                "could not prepare entry chunk {position:?}"
            )));
        }
        progress.publish(LoadingProgress {
            stage: stage.into(),
            completed: index + 1,
            total: chunks.len(),
        })?;
    }
    Ok(())
}

fn check_cancelled(cancel: &CancellationToken) -> io::Result<()> {
    if cancel.is_cancelled() {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "world entry cancelled",
        ))
    } else {
        Ok(())
    }
}

fn saved_profile_position(path: &Path) -> Option<EntityPos> {
    #[derive(serde::Deserialize)]
    struct Position {
        version: u32,
        face: u8,
        u: f32,
        y: f32,
        v: f32,
    }
    let text = std::fs::read_to_string(path).ok()?;
    let saved: Position = toml::from_str(&text).ok()?;
    if saved.version != 2 {
        return None;
    }
    let face = crate::planet::Face::from_u8(saved.face)?;
    EntityPos::new(face, saved.u, saved.y, saved.v).ok()
}
