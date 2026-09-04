//! Instanced dungeon zone tests (capability E10): the Deep face's void
//! generation, run entry/exit through the slot state machine, party
//! checkpoints, and the empty-timer reset that drops a run's chunks.

use super::*;

use crate::planet::{BlockPos, Face};
use crate::world::dungeon;

const DEN_MOD: &str = r#"id = "denkeep"
world_api = 2
depends = ["base"]
"#;

const DEN_PIECES: &str = r#"
[[assembly]]
id = "denkeep"
biomes = ["plains"]
rarity = 999999
entry = "base:watch_platform"
pools = { path = "base:path" }
max_depth = 2
max_pieces = 4

[assembly.dungeon]
reset = 3.0
"#;

fn den_reg() -> Arc<Registry> {
    let root = tmp_dir("denkeep-mod");
    let dir = root.join("denkeep");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mod.toml"), DEN_MOD).unwrap();
    std::fs::write(dir.join("pieces.toml"), DEN_PIECES).unwrap();
    Arc::new(registry::load(&root))
}

#[test]
fn the_deep_face_is_a_seventh_outside_the_planet() {
    // Stable wire/save ids: existing faces unchanged, Deep appended.
    assert_eq!(Face::from_u8(5), Some(Face::NegZ));
    assert_eq!(Face::from_u8(6), Some(Face::Deep));
    assert_eq!(Face::Deep.name(), "deep");
    assert!(Face::Deep.is_deep());
    assert!(!Face::PosZ.is_deep());
    // The surface roster still holds exactly six; the Deep is not in it.
    assert_eq!(Face::ALL.len(), 6);
    assert!(Face::ALL.iter().all(|f| !f.is_deep()));
    // A Deep chunk key round-trips save/wire like any other.
    let pos = ChunkPos::new(Face::Deep, 17, 33).expect("deep chunk is canonical");
    let bytes = postcard::to_allocvec(&pos).expect("encodes");
    let back: ChunkPos = crate::net::decode(&bytes).expect("decodes");
    assert_eq!(back, pos);
}

#[test]
fn deep_chunks_generate_as_void_and_read_as_grassland() {
    let mut w = test_world("deep-void");
    let pos = ChunkPos::new(Face::Deep, 4, 4).expect("canonical");
    w.ensure_chunk(pos);
    let chunk = w.chunk(pos).expect("deep chunk resident");
    assert!(
        chunk.raw().iter().all(|&b| b == crate::registry::AIR.0),
        "the Deep generates pure void"
    );
    let surface = BlockPos::new(Face::Deep, 64, 40, 64).expect("deep block");
    assert_eq!(
        w.country_biome_at(surface.surface()),
        crate::worldgen::Biome::Plains,
        "dungeons read as generic grassland for habitat checks"
    );
    assert!(
        !w.heart_alive_at_surface(surface.surface()),
        "the wild's heart does not reach below"
    );
}

#[test]
fn entering_stamps_a_run_and_exiting_returns_the_participant() {
    let reg = den_reg();
    let mut w = test_world_with("den-enter", reg.clone());
    let overworld = ep(Vec3::new(8.5, 130.0, 8.5));
    let spawn = w
        .enter_dungeon(0, overworld, "denkeep:denkeep")
        .expect("the dungeon opens");
    // The spawn stands on the Deep inside its slot.
    let block = spawn.block().expect("spawn has a block");
    assert_eq!(block.face(), Face::Deep);
    assert_eq!(w.dungeon_runs.len(), 1);
    let anchor = w.dungeon_runs[0].anchor;
    let chunk = block.chunk();
    assert!(
        chunk.u() >= anchor.u()
            && chunk.u() < anchor.u() + dungeon::SLOT_CHUNKS
            && chunk.v() >= anchor.v()
            && chunk.v() < anchor.v() + dungeon::SLOT_CHUNKS,
        "spawn sits inside the run's slot"
    );
    // Rooms exist: the entry piece's floor landed near the spawn.
    let cob = reg.block_id("base:mossy_cobblestone").unwrap();
    let mut found = false;
    for du in 0..dungeon::SLOT_CHUNKS {
        for dv in 0..dungeon::SLOT_CHUNKS {
            if let Ok(cp) = ChunkPos::new(Face::Deep, anchor.u() + du, anchor.v() + dv)
                && let Some(c) = w.chunk(cp)
                && c.raw().contains(&cob.0)
            {
                found = true;
            }
        }
    }
    assert!(found, "the stamped rooms reached the Deep");
    // Exiting routes back to the participant's own recorded position.
    let back = w.exit_dungeon(0, spawn).expect("exit routes home");
    assert_eq!(back, overworld);
    let _ = reg;
}

#[test]
fn an_empty_run_resets_and_drops_its_chunks() {
    let reg = den_reg();
    let mut w = test_world_with("den-reset", reg.clone());
    let overworld = ep(Vec3::new(8.5, 130.0, 8.5));
    let spawn = w.enter_dungeon(0, overworld, "denkeep:denkeep").unwrap();
    let block = spawn.block().unwrap();
    let chunk = block.chunk();
    assert!(
        w.chunk(ChunkPos::new(Face::Deep, chunk.u(), chunk.v()).unwrap())
            .is_some(),
        "the run's chunks are resident while it lives"
    );
    // Occupied: no reset.
    w.tick_dungeon_runs(10.0, 1);
    assert_eq!(w.dungeon_runs.len(), 1);
    // Empty past the 3s reset: the run dissolves.
    w.exit_dungeon(0, spawn).unwrap();
    w.tick_dungeon_runs(2.0, 0);
    assert_eq!(w.dungeon_runs.len(), 1, "not yet past the reset delay");
    w.tick_dungeon_runs(2.0, 0);
    assert_eq!(w.dungeon_runs.len(), 0, "an emptied run resets");
    assert!(
        w.chunk(ChunkPos::new(Face::Deep, chunk.u(), chunk.v()).unwrap())
            .is_none(),
        "the reset drops the zone's chunks unsaved"
    );
    // Re-entry stamps fresh rooms at the same assembly.
    let again = w.enter_dungeon(0, overworld, "denkeep:denkeep").unwrap();
    assert_eq!(again.block().unwrap().face(), Face::Deep);
    assert_eq!(w.dungeon_runs.len(), 1, "a new run takes a free slot");
}
