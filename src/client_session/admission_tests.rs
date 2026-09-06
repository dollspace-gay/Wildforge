//! The same host sequence gates headless and graphical entry independently.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{Admission, AdmissionError, PresentationRequirement};
use crate::chunk::ChunkPos;
use crate::planet::{EntityPos, Face};
use crate::registry;
use crate::world::{ReplicaWorld, ReplicationTarget, TerrainRead};

fn spawn() -> EntityPos {
    EntityPos::new(Face::PosZ, 100.0, 80.0, 100.0).unwrap()
}

fn start(requirement: PresentationRequirement) -> Admission {
    let now = Instant::now();
    let mut admission = Admission::new(requirement, now);
    admission.begin("entry-fixture".into(), spawn(), now);
    admission
}

#[test]
fn both_consumers_require_terrain_and_graphics_also_requires_its_first_frame() {
    let center = spawn().chunk().unwrap();
    let neighbor = center.offset(1, 0);
    for requirement in [
        PresentationRequirement::TerrainOnly,
        PresentationRequirement::FirstFrame,
    ] {
        let mut admission = start(requirement);
        assert!(!admission.take_ready());
        admission
            .manifest(spawn(), vec![center, neighbor], |_| false)
            .unwrap();
        admission.resident(neighbor);
        assert!(!admission.take_ready());
        assert_eq!(admission.frame_needed(), None);
        admission.resident(center);
        if requirement == PresentationRequirement::FirstFrame {
            assert!(!admission.take_ready());
            assert_eq!(admission.frame_needed(), Some(center));
            admission.frame_ready().unwrap();
        }
        assert!(admission.take_ready());
        assert!(!admission.take_ready());
        assert_eq!(admission.accepted().unwrap(), "entry-fixture");
        assert!(!admission.timed_out(Instant::now() + Duration::from_secs(100)));
        assert_eq!(
            admission.accepted(),
            Err(AdmissionError::UnexpectedAcceptance)
        );
        assert!(admission.is_closed());
    }
}

#[test]
fn early_acceptance_and_presentation_fail_closed() {
    let center = spawn().chunk().unwrap();
    for requirement in [
        PresentationRequirement::TerrainOnly,
        PresentationRequirement::FirstFrame,
    ] {
        let mut admission = start(requirement);
        assert_eq!(
            admission.accepted(),
            Err(AdmissionError::UnexpectedAcceptance)
        );
        assert!(admission.is_closed());
        assert!(!admission.take_ready());
    }
    let mut admission = start(PresentationRequirement::FirstFrame);
    admission
        .manifest(spawn(), vec![center], |_| false)
        .unwrap();
    assert_eq!(
        admission.frame_ready(),
        Err(AdmissionError::UnexpectedFrame)
    );
    assert!(admission.is_closed());
}

#[test]
fn a_manifest_can_adopt_already_resident_chunks_without_acknowledging_an_unseen_manifest() {
    let center = spawn().chunk().unwrap();
    let mut admission = start(PresentationRequirement::TerrainOnly);
    admission.resident(center);
    assert!(!admission.take_ready());
    admission
        .manifest(spawn(), vec![center], |pos| pos == center)
        .unwrap();
    assert!(admission.take_ready());
}

#[test]
fn missing_spawn_or_conflicting_manifests_are_rejected() {
    let center = spawn().chunk().unwrap();
    for required in [vec![], vec![center.offset(1, 0)]] {
        let mut admission = start(PresentationRequirement::TerrainOnly);
        assert_eq!(
            admission.manifest(spawn(), required, |_| true),
            Err(AdmissionError::MissingSpawnChunk)
        );
        assert!(admission.is_closed());
    }
    let mut admission = start(PresentationRequirement::TerrainOnly);
    let other_spawn = EntityPos::new(Face::PosZ, 101.0, 80.0, 100.0).unwrap();
    assert_eq!(
        admission.manifest(other_spawn, vec![center], |_| true),
        Err(AdmissionError::SpawnMismatch)
    );
    assert!(admission.is_closed());
    let mut admission = start(PresentationRequirement::TerrainOnly);
    admission
        .manifest(spawn(), vec![center, center.offset(1, 0)], |_| false)
        .unwrap();
    assert_eq!(
        admission.manifest(spawn(), vec![center], |_| true),
        Err(AdmissionError::ManifestChanged)
    );
    assert!(admission.is_closed());
}

#[test]
fn duplicate_manifests_preserve_the_declared_set_and_recheck_residency() {
    let center = spawn().chunk().unwrap();
    let neighbor = center.offset(1, 0);
    let mut admission = start(PresentationRequirement::TerrainOnly);
    admission
        .manifest(spawn(), vec![center, neighbor], |_| false)
        .unwrap();
    admission
        .manifest(spawn(), vec![neighbor, center, center], |pos| pos == center)
        .unwrap();
    assert!(!admission.take_ready());
    admission.resident(neighbor);
    assert!(admission.take_ready());
}

#[test]
fn new_welcome_and_disconnect_discard_readiness_from_the_previous_world() {
    let center = spawn().chunk().unwrap();
    let mut admission = start(PresentationRequirement::TerrainOnly);
    admission.manifest(spawn(), vec![center], |_| true).unwrap();
    assert!(admission.take_ready());
    admission.begin("replacement".into(), spawn(), Instant::now());
    assert!(!admission.take_ready());
    assert_eq!(
        admission.accepted(),
        Err(AdmissionError::UnexpectedAcceptance)
    );
    admission.begin("replacement".into(), spawn(), Instant::now());
    admission.manifest(spawn(), vec![center], |_| true).unwrap();
    assert!(admission.take_ready());
    admission.close();
    assert_eq!(
        admission.accepted(),
        Err(AdmissionError::UnexpectedAcceptance)
    );
    assert!(admission.is_closed());
}

#[test]
fn activity_retains_the_existing_idle_timeout_without_timing_out_active_sessions() {
    let now = Instant::now();
    let mut admission = Admission::new(PresentationRequirement::TerrainOnly, now);
    assert!(!admission.timed_out(now + Duration::from_secs(15)));
    assert!(admission.timed_out(now + Duration::from_secs(16)));
    admission.note_activity(now + Duration::from_secs(10));
    assert!(!admission.timed_out(now + Duration::from_secs(25)));
    assert!(admission.timed_out(now + Duration::from_secs(26)));
    admission.close();
    assert!(!admission.timed_out(now + Duration::from_secs(100)));
    admission.begin("next".into(), spawn(), now + Duration::from_secs(100));
    assert!(!admission.timed_out(now + Duration::from_secs(110)));
}

#[test]
fn rejected_chunk_payloads_cannot_count_as_decoded_entry_terrain() {
    let reg = Arc::new(registry::load(Path::new("/nonexistent-mods-dir")));
    let center = spawn().chunk().unwrap();
    let mut world = ReplicaWorld::new(1, reg, 0.0);
    for requirement in [
        PresentationRequirement::TerrainOnly,
        PresentationRequirement::FirstFrame,
    ] {
        let mut admission = start(requirement);
        admission
            .manifest(spawn(), vec![center], |pos| world.has_chunk(pos))
            .unwrap();
        for bytes in [b"invalid".as_slice(), b"WFC9".as_slice()] {
            world.insert_remote_chunks([(center, bytes)], &[]);
            // Both adapters acknowledge residency only after the decoder ran.
            if world.has_chunk(center) {
                admission.resident(center);
            }
            assert!(!world.has_chunk(center));
            assert!(!admission.take_ready());
            assert_eq!(admission.frame_needed(), None);
        }
        assert_eq!(
            admission.accepted(),
            Err(AdmissionError::UnexpectedAcceptance)
        );
    }
}

#[test]
fn a_manifest_before_welcome_cannot_install_entry_state() {
    let mut admission = Admission::new(PresentationRequirement::TerrainOnly, Instant::now());
    let center: ChunkPos = spawn().chunk().unwrap();
    assert_eq!(
        admission.manifest(spawn(), vec![center], |_| true),
        Err(AdmissionError::UnexpectedManifest)
    );
    assert!(admission.is_closed());
}
