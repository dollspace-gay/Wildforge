//! Region format and failure-preservation scenarios.

use super::*;
use crate::planet::Face;

fn tchunk(u: i32, v: i32) -> ChunkPos {
    ChunkPos::from_centered(Face::PosZ, u, v).unwrap()
}

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("wildforge-region-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn chunks_round_trip_through_one_region_file() {
    let dir = tmp("round-trip");
    let a = tchunk(3, 9);
    let b = tchunk(31, 0);
    write_chunk(&dir, a, b"first chunk").unwrap();
    write_chunk(&dir, b, b"second chunk").unwrap();
    assert_eq!(read_chunk(&dir, a).unwrap().unwrap(), b"first chunk");
    assert_eq!(read_chunk(&dir, b).unwrap().unwrap(), b"second chunk");
    // Both landed in the same file: that is the whole point.
    assert_eq!(region_path(&dir, a), region_path(&dir, b));
    let files: Vec<_> = fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).collect();
    assert_eq!(files.len(), 1, "one file, not one per chunk");
}

#[test]
fn negative_coordinates_land_in_their_own_region() {
    let dir = tmp("negatives");
    // -1 belongs to region -1, not region 0: a floor shift, not a
    // truncating divide, or the whole western half of a world collides
    // with the eastern half.
    let west = tchunk(-1, -1);
    let east = tchunk(0, 0);
    assert_ne!(region_path(&dir, west), region_path(&dir, east));
    write_chunk(&dir, west, b"west").unwrap();
    write_chunk(&dir, east, b"east").unwrap();
    assert_eq!(read_chunk(&dir, west).unwrap().unwrap(), b"west");
    assert_eq!(read_chunk(&dir, east).unwrap().unwrap(), b"east");
    assert_ne!(slot_of(west), slot_of(east));
}

#[test]
fn rewriting_a_chunk_replaces_it() {
    let dir = tmp("rewrite");
    let pos = tchunk(5, 5);
    for n in 0..12 {
        write_chunk(&dir, pos, format!("version {n}").as_bytes()).unwrap();
    }
    assert_eq!(read_chunk(&dir, pos).unwrap().unwrap(), b"version 11");
    // Neighbours in the same region are undisturbed by the churn.
    let other = tchunk(6, 5);
    write_chunk(&dir, other, b"neighbour").unwrap();
    write_chunk(&dir, pos, b"final").unwrap();
    assert_eq!(read_chunk(&dir, other).unwrap().unwrap(), b"neighbour");
    assert_eq!(read_chunk(&dir, pos).unwrap().unwrap(), b"final");
}

#[test]
fn a_missing_chunk_reads_as_absent() {
    let dir = tmp("absent");
    assert!(read_chunk(&dir, tchunk(0, 0)).unwrap().is_none());
    write_chunk(&dir, tchunk(0, 0), b"here").unwrap();
    assert!(read_chunk(&dir, tchunk(1, 0)).unwrap().is_none());
}

#[test]
fn a_flat_pre_region_file_is_not_visible_to_planetary_storage() {
    let dir = tmp("legacy");
    let pos = tchunk(2, -7);
    let legacy = dir.join("c.2.-7.wfc");
    fs::write(&legacy, b"old flat file").unwrap();
    assert!(read_chunk(&dir, pos).unwrap().is_none());
    write_chunk(&dir, pos, b"new").unwrap();
    assert!(legacy.exists(), "refusal does not modify old flat data");
    assert_eq!(read_chunk(&dir, pos).unwrap().unwrap(), b"new");
}

#[test]
fn compaction_keeps_every_live_chunk() {
    let dir = tmp("compact");
    let a = tchunk(1, 1);
    let b = tchunk(2, 2);
    write_chunk(&dir, b, b"b stays").unwrap();
    // Enough churn to trip the compaction thresholds.
    let big = vec![7u8; 512 * 1024];
    for _ in 0..20 {
        write_chunk(&dir, a, &big).unwrap();
    }
    assert_eq!(read_chunk(&dir, b).unwrap().unwrap(), b"b stays");
    assert_eq!(read_chunk(&dir, a).unwrap().unwrap().len(), big.len());
    let size = fs::metadata(region_path(&dir, a)).unwrap().len();
    assert!(
        size < big.len() as u64 * 4,
        "twenty rewrites of a 512 KB chunk left {size} bytes behind"
    );
}

#[test]
fn invalid_headers_are_errors_and_writes_leave_them_untouched() {
    let dir = tmp("invalid-headers");
    let position = tchunk(0, 0);
    let path = region_path(&dir, position);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    for bytes in [b"torn header".to_vec(), vec![0; HEADER_BYTES as usize]] {
        fs::write(&path, &bytes).unwrap();
        assert_eq!(
            read_chunk(&dir, position).unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
        assert_eq!(
            write_chunk(&dir, position, b"replacement")
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::InvalidData
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn invalid_offsets_and_lengths_do_not_allocate_or_destroy_other_payloads() {
    let dir = tmp("invalid-slots");
    let position = tchunk(0, 0);
    write_chunk(&dir, position, b"original").unwrap();
    let path = region_path(&dir, position);
    let original = fs::read(&path).unwrap();
    let at = 4 + slot_of(position) * 8;
    for (offset, len) in [(0, 1), (HEADER_BYTES as u32, u32::MAX), (u32::MAX, 4)] {
        let mut corrupted = original.clone();
        corrupted[at..at + 4].copy_from_slice(&offset.to_le_bytes());
        corrupted[at + 4..at + 8].copy_from_slice(&len.to_le_bytes());
        fs::write(&path, &corrupted).unwrap();
        assert_eq!(
            read_chunk(&dir, position).unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
        assert!(compact(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), corrupted);
    }
}

#[test]
fn inaccessible_region_parent_is_not_reported_as_missing() {
    let dir = tmp("unreadable-parent");
    let position = tchunk(0, 0);
    fs::write(dir.join(position.face().name()), b"not a directory").unwrap();
    let error = read_chunk(&dir, position).unwrap_err();
    assert_ne!(error.kind(), std::io::ErrorKind::NotFound);
    assert_ne!(error.kind(), std::io::ErrorKind::InvalidData);
}
