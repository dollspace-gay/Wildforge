//! Concurrency and revision scenarios for the session persistence owner.

use std::sync::{Arc, Barrier};

use super::RegionStore;
use crate::chunk::ChunkPos;
use crate::planet::Face;

fn position(u: u16) -> ChunkPos {
    ChunkPos::new(Face::PosZ, u, 0).unwrap()
}

fn store(label: &str) -> RegionStore {
    let directory = std::env::temp_dir().join(format!("wildforge-{label}-{}", std::process::id()));
    if directory.exists() {
        std::fs::remove_dir_all(&directory).unwrap();
    }
    RegionStore::new(directory)
}

#[test]
fn readers_share_the_writers_region_lock_but_other_regions_can_progress() {
    let store = store("region-coordination");
    let clone = store.clone();
    let first = store.region(position(0)).unwrap();
    let same = clone.region(position(1)).unwrap();
    let other = clone.region(position(32)).unwrap();
    assert!(Arc::ptr_eq(&first, &same));
    assert!(!Arc::ptr_eq(&first, &other));
    let guard = first.lock().unwrap();
    assert!(same.try_lock().is_err());
    // An actual independent write/read finishes while the first lock is held.
    std::thread::scope(|scope| {
        scope.spawn(|| {
            clone.write(position(32), b"other region").unwrap();
            assert_eq!(
                clone.read(position(32)).unwrap().0.unwrap(),
                b"other region"
            );
        });
    });
    drop(guard);
    assert!(same.try_lock().is_ok());
}

#[test]
fn concurrent_reads_observe_whole_payloads_across_new_headers_and_slot_updates() {
    let store = store("region-whole-reads");
    let gate = Barrier::new(3);
    let small = vec![0x35; 17];
    let large = vec![0xba; 24 * 1024];
    std::thread::scope(|scope| {
        scope.spawn(|| {
            gate.wait();
            for index in 0..150 {
                let payload = if index % 2 == 0 { &small } else { &large };
                store.write(position(0), payload).unwrap();
            }
        });
        for _ in 0..2 {
            scope.spawn(|| {
                gate.wait();
                for _ in 0..200 {
                    if let Some(bytes) = store.read(position(0)).unwrap().0 {
                        assert!(bytes == small || bytes == large);
                    }
                }
            });
        }
    });
}

#[test]
fn revisions_survive_idle_io_and_are_scoped_to_the_chunk_and_store() {
    let store = store("region-revisions");
    let clone = store.clone();
    let (missing, original) = store.read(position(0)).unwrap();
    assert!(missing.is_none());
    assert!(clone.is_current(position(0), &original));
    assert!(!clone.is_current(position(1), &original));
    let independent = RegionStore::new(store.0.directory.clone());
    assert!(!independent.is_current(position(0), &original));
    clone.write(position(1), b"neighbor").unwrap();
    assert!(store.is_current(position(0), &original));
    // No I/O guard remains alive, only the prepared result's revision.
    clone.write(position(0), b"edited").unwrap();
    assert!(!store.is_current(position(0), &original));
    let (bytes, current) = store.read(position(0)).unwrap();
    assert_eq!(bytes.unwrap(), b"edited");
    assert!(store.is_current(position(0), &current));
    let (_, second_reader) = clone.read(position(0)).unwrap();
    clone.write(position(0), b"another edit").unwrap();
    assert!(!store.is_current(position(0), &current));
    assert!(!store.is_current(position(0), &second_reader));
}

#[test]
fn failed_write_invalidates_prepared_content_without_replacing_bad_bytes() {
    let store = store("region-failed-revision");
    let (_, old) = store.read(position(0)).unwrap();
    let path = crate::world::region::region_path(&store.0.directory, position(0));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"damaged").unwrap();
    assert!(store.write(position(0), b"new content").is_err());
    assert!(!store.is_current(position(0), &old));
    assert_eq!(std::fs::read(path).unwrap(), b"damaged");
}

#[test]
fn finished_work_does_not_retain_a_history_of_region_locks_or_revisions() {
    let store = store("region-watch-lifetime");
    let (_, retained) = store.read(position(0)).unwrap();
    for index in 1..32 {
        let _ = store.read(position(index)).unwrap();
    }
    // A new read prunes expired watches in this still-active region.
    let (_, current) = store.read(position(0)).unwrap();
    assert_eq!(retained._region.lock().unwrap().watched.len(), 1);
    drop(current);
    drop(retained);
    let _ = store.read(position(32)).unwrap();
    // Acquisition pruned the previous region; the map retains only one weak slot.
    assert_eq!(store.0.regions.lock().unwrap().len(), 1);
    assert!(
        store
            .0
            .regions
            .lock()
            .unwrap()
            .values()
            .all(|weak| weak.strong_count() == 0)
    );
}
