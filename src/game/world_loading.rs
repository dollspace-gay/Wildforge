//! Exclusive ownership of local world creation and entry until session adoption.

use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use crate::background::{Operation, OperationUpdate, Progress};
use crate::identity::DeviceKeyId;
use crate::planet::{EntityPos, SurfacePos};
use crate::planet_atlas::CancellationToken;
use crate::registry::Registry;
use crate::world::World;

#[path = "world_loading_work.rs"]
mod work;

pub(super) struct CreationRequest {
    pub name: String,
    pub destination: PathBuf,
    pub seed: u32,
    pub mode: String,
    pub content_hash: u64,
    pub reg: Arc<Registry>,
}

pub(super) struct EntryRequest {
    pub name: String,
    pub save_dir: PathBuf,
    pub reg: Arc<Registry>,
    pub device_id: DeviceKeyId,
    pub override_wanted: Option<SurfacePos>,
    pub created_here: bool,
}

pub(super) struct LoadingProgress {
    pub stage: String,
    pub completed: usize,
    pub total: usize,
}

pub(super) struct PreparedWorld {
    pub world: World,
    pub spawn: EntityPos,
    pub notices: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LoadingKind {
    Creation,
    Entry { created_here: bool },
}

pub(super) enum LoadingEvent {
    Progress(LoadingProgress),
    Created {
        name: String,
        enter: bool,
    },
    Ready {
        name: String,
        prepared: Box<PreparedWorld>,
    },
    Reenter {
        name: String,
        created_here: bool,
    },
    Cancelled(LoadingKind),
    Failed {
        name: String,
        kind: LoadingKind,
        error: io::Error,
    },
}

enum WorkOutput {
    Created,
    Ready(Box<PreparedWorld>),
}

struct ActiveOperation {
    name: String,
    kind: LoadingKind,
    // Retain the requested registry independently of World::reg: restoring
    // saved placeholders can legitimately make a private registry copy.
    requested_reg: Arc<Registry>,
    operation: Operation<LoadingProgress, WorkOutput>,
}

#[derive(Default)]
pub(super) struct WorldLoading {
    active: Option<ActiveOperation>,
}

impl WorldLoading {
    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }

    pub fn create(&mut self, request: CreationRequest) -> io::Result<()> {
        self.begin(
            request.name.clone(),
            LoadingKind::Creation,
            Arc::clone(&request.reg),
            move |progress, cancel| work::create(request, progress, cancel),
        )
    }

    pub fn enter(&mut self, request: EntryRequest) -> io::Result<()> {
        self.begin(
            request.name.clone(),
            LoadingKind::Entry {
                created_here: request.created_here,
            },
            Arc::clone(&request.reg),
            move |progress, cancel| work::enter(request, progress, cancel),
        )
    }

    fn begin(
        &mut self,
        name: String,
        kind: LoadingKind,
        requested_reg: Arc<Registry>,
        work: impl FnOnce(Progress<LoadingProgress>, CancellationToken) -> io::Result<WorkOutput>
        + Send
        + 'static,
    ) -> io::Result<()> {
        if self.is_active() {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "a world operation is already active",
            ));
        }
        let cancel = CancellationToken::default();
        let worker_cancel = cancel.clone();
        let operation = Operation::spawn(
            "world-loading",
            move || cancel.cancel(),
            move |progress| work(progress, worker_cancel),
        )?;
        self.active = Some(ActiveOperation {
            name,
            kind,
            requested_reg,
            operation,
        });
        Ok(())
    }

    pub fn cancel(&mut self) -> Option<LoadingKind> {
        let active = self.active.as_mut()?;
        active.operation.cancel();
        Some(active.kind)
    }

    /// A terminal event is produced only after joining. Cancellation and
    /// registry replacement take precedence over adopting a prepared world.
    pub fn poll(&mut self, current_reg: &Arc<Registry>) -> Option<LoadingEvent> {
        let active = self.active.as_mut()?;
        let result = match active.operation.poll()? {
            OperationUpdate::Progress(progress) => {
                return (!active.operation.cancellation_requested())
                    .then_some(LoadingEvent::Progress(progress));
            }
            OperationUpdate::Finished(result) => result,
        };
        let active = self.active.take()?;
        let cancelled = active.operation.cancellation_requested();
        Some(match result {
            // Publication is durable even if cancellation wins the UI race.
            Ok(WorkOutput::Created) => LoadingEvent::Created {
                name: active.name,
                enter: !cancelled,
            },
            Ok(WorkOutput::Ready(_)) if cancelled => LoadingEvent::Cancelled(active.kind),
            Ok(WorkOutput::Ready(_)) if !Arc::ptr_eq(&active.requested_reg, current_reg) => {
                let created_here = matches!(active.kind, LoadingKind::Entry { created_here: true });
                LoadingEvent::Reenter {
                    name: active.name,
                    created_here,
                }
            }
            Ok(WorkOutput::Ready(prepared)) => LoadingEvent::Ready {
                name: active.name,
                prepared,
            },
            Err(error) if cancelled && is_cancellation(&error) => {
                LoadingEvent::Cancelled(active.kind)
            }
            Err(error) => LoadingEvent::Failed {
                name: active.name,
                kind: active.kind,
                error,
            },
        })
    }
}

fn is_cancellation(error: &io::Error) -> bool {
    if error.kind() == io::ErrorKind::Interrupted {
        return true;
    }
    let Some(source) = error.get_ref() else {
        return false;
    };
    matches!(
        source.downcast_ref::<crate::planet_atlas::AtlasError>(),
        Some(crate::planet_atlas::AtlasError::Cancelled)
    ) || matches!(
        source.downcast_ref::<crate::arcane_geography::ArcaneGeographyError>(),
        Some(crate::arcane_geography::ArcaneGeographyError::Cancelled)
    )
}

#[cfg(test)]
#[path = "world_loading_tests.rs"]
mod tests;
