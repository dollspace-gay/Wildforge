//! Host admission follows explicit terrain and presentation milestones.

use std::collections::HashSet;
use std::time::Instant;

use crate::chunk::ChunkPos;
use crate::planet::EntityPos;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PresentationRequirement {
    TerrainOnly,
    FirstFrame,
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub(crate) enum AdmissionError {
    #[error("entry manifest arrived outside world preparation")]
    UnexpectedManifest,
    #[error("host entry manifest did not match Welcome spawn")]
    SpawnMismatch,
    #[error("host entry manifest did not include the spawn chunk")]
    MissingSpawnChunk,
    #[error("host changed the entry manifest during preparation")]
    ManifestChanged,
    #[error("host accepted entry before terrain and presentation were ready")]
    UnexpectedAcceptance,
    #[error("initial frame became ready outside completed terrain preparation")]
    UnexpectedFrame,
}

#[derive(Debug)]
struct Entry {
    world_name: String,
    spawn: EntityPos,
}

#[derive(Debug)]
struct Terrain {
    entry: Entry,
    declared: HashSet<ChunkPos>,
    pending: HashSet<ChunkPos>,
    presentation: Presentation,
}

#[derive(Debug)]
enum Presentation {
    Pending,
    Ready,
}

#[derive(Debug)]
enum Phase {
    AwaitingWelcome,
    AwaitingManifest(Entry),
    Receiving(Terrain),
    ReadySent(Entry),
    Active,
    Closed,
}

/// Shared protocol state; rendering supplies its own first-frame milestone.
#[derive(Debug)]
pub(crate) struct Admission {
    requirement: PresentationRequirement,
    phase: Phase,
    last_activity: Instant,
}

impl Admission {
    pub(crate) fn new(requirement: PresentationRequirement, now: Instant) -> Self {
        Self {
            requirement,
            phase: Phase::AwaitingWelcome,
            last_activity: now,
        }
    }

    pub(crate) fn begin(&mut self, world_name: String, spawn: EntityPos, now: Instant) {
        self.phase = Phase::AwaitingManifest(Entry { world_name, spawn });
        self.last_activity = now;
    }

    /// Already-resident chunks support transport batches that precede a manifest.
    pub(crate) fn manifest(
        &mut self,
        spawn: EntityPos,
        required: Vec<ChunkPos>,
        is_resident: impl Fn(ChunkPos) -> bool,
    ) -> Result<(), AdmissionError> {
        let phase = std::mem::replace(&mut self.phase, Phase::Closed);
        let (entry, previous) = match phase {
            Phase::AwaitingManifest(entry) => (entry, None),
            Phase::Receiving(terrain) => (terrain.entry, Some(terrain.declared)),
            _ => return Err(AdmissionError::UnexpectedManifest),
        };
        if spawn != entry.spawn {
            return Err(AdmissionError::SpawnMismatch);
        }
        let declared: HashSet<_> = required.into_iter().collect();
        if spawn
            .chunk()
            .is_none_or(|center| !declared.contains(&center))
        {
            return Err(AdmissionError::MissingSpawnChunk);
        }
        if previous.is_some_and(|previous| previous != declared) {
            return Err(AdmissionError::ManifestChanged);
        }
        let pending = declared
            .iter()
            .copied()
            .filter(|pos| !is_resident(*pos))
            .collect();
        let presentation = match self.requirement {
            PresentationRequirement::TerrainOnly => Presentation::Ready,
            PresentationRequirement::FirstFrame => Presentation::Pending,
        };
        self.phase = Phase::Receiving(Terrain {
            entry,
            declared,
            pending,
            presentation,
        });
        Ok(())
    }

    /// Call only after the replica confirms that the decoded chunk is resident.
    pub(crate) fn resident(&mut self, position: ChunkPos) {
        if let Phase::Receiving(terrain) = &mut self.phase {
            terrain.pending.remove(&position);
        }
    }

    pub(crate) fn frame_needed(&self) -> Option<ChunkPos> {
        match &self.phase {
            Phase::Receiving(terrain)
                if terrain.pending.is_empty()
                    && matches!(terrain.presentation, Presentation::Pending) =>
            {
                terrain.entry.spawn.chunk()
            }
            _ => None,
        }
    }

    pub(crate) fn frame_ready(&mut self) -> Result<(), AdmissionError> {
        if let Phase::Receiving(terrain) = &mut self.phase
            && terrain.pending.is_empty()
            && matches!(terrain.presentation, Presentation::Pending)
        {
            terrain.presentation = Presentation::Ready;
            return Ok(());
        }
        self.phase = Phase::Closed;
        Err(AdmissionError::UnexpectedFrame)
    }

    /// Claim the outgoing acknowledgement once, after every required milestone.
    pub(crate) fn take_ready(&mut self) -> bool {
        let phase = std::mem::replace(&mut self.phase, Phase::Closed);
        match phase {
            Phase::Receiving(terrain)
                if terrain.pending.is_empty()
                    && matches!(terrain.presentation, Presentation::Ready) =>
            {
                self.phase = Phase::ReadySent(terrain.entry);
                true
            }
            other => {
                self.phase = other;
                false
            }
        }
    }

    pub(crate) fn accepted(&mut self) -> Result<String, AdmissionError> {
        match std::mem::replace(&mut self.phase, Phase::Closed) {
            Phase::ReadySent(entry) => {
                self.phase = Phase::Active;
                Ok(entry.world_name)
            }
            _ => Err(AdmissionError::UnexpectedAcceptance),
        }
    }

    pub(crate) fn note_activity(&mut self, now: Instant) {
        self.last_activity = now;
    }

    pub(crate) fn timed_out(&self, now: Instant) -> bool {
        !matches!(self.phase, Phase::Active | Phase::Closed)
            && now.saturating_duration_since(self.last_activity).as_secs() > 15
    }

    pub(crate) fn close(&mut self) {
        self.phase = Phase::Closed;
    }

    pub(crate) fn is_closed(&self) -> bool {
        matches!(self.phase, Phase::Closed)
    }

    pub(super) fn receives_world(&self) -> bool {
        matches!(
            self.phase,
            Phase::AwaitingManifest(_) | Phase::Receiving(_) | Phase::ReadySent(_) | Phase::Active
        )
    }
}

#[cfg(test)]
#[path = "admission_tests.rs"]
mod tests;
