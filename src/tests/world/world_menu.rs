//! World menu scenarios.

use super::*;

#[test]
fn world_listing_only_includes_compatible_planets() {
    // Regression: the title list only read the legacy `seed` file, so
    // world.toml worlds were invisible and their folder names got reused
    // by NEW WORLD — inheriting the old player.toml (inventory carryover).
    let root = tmp_dir("listworlds");
    crate::world::create_world_fixture_atomic(
        &root.join("world1"),
        42,
        "survival",
        4,
        &crate::planet_atlas::CancellationToken::default(),
        |_| {},
    )
    .unwrap();
    std::fs::create_dir_all(root.join("old")).unwrap();
    std::fs::write(root.join("old/seed"), "7").unwrap();
    std::fs::create_dir_all(root.join("junk")).unwrap();
    std::fs::write(root.join("stray.txt"), "x").unwrap();
    let worlds = crate::world::list_worlds(&root);
    assert_eq!(
        worlds,
        vec![("world1".to_string(), 42)],
        "only validated planetary worlds are selectable"
    );
    let inspected = crate::world::inspect_worlds(&root);
    let ready = inspected
        .iter()
        .find(|entry| entry.name == "world1")
        .unwrap();
    assert!(ready.playable);
    assert!(ready.status.contains(&format!(
        "GENERATOR {}",
        crate::world::WORLD_GENERATOR_VERSION
    )));
    assert!(ready.status.contains(&format!(
        "ATLAS {}",
        crate::planet_atlas::ATLAS_FORMAT_VERSION
    )));
    assert!(ready.status.contains("CONTENT"));
    let old = inspected.iter().find(|entry| entry.name == "old").unwrap();
    assert!(!old.playable);
    assert!(old.status.contains("INCOMPATIBLE"));
    assert!(old.status.contains("legacy flat"));
    let junk = inspected.iter().find(|entry| entry.name == "junk").unwrap();
    assert!(!junk.playable);
    assert!(junk.status.contains("INCOMPLETE"));
}

#[test]
fn new_world_name_never_reuses_existing_folder() {
    let root = tmp_dir("nextworld");
    crate::world::write_world_meta(&root.join("world1"), 1, "survival", 0.0).unwrap();
    std::fs::write(root.join("world1/player.toml"), "leftover inventory").unwrap();
    let listed = crate::world::list_worlds(&root);
    assert_eq!(crate::next_world_name(&root, &listed), "world2");
    // Even a folder the listing can't see must not be adopted as "new".
    assert_eq!(crate::next_world_name(&root, &[]), "world2");
    assert_eq!(
        crate::next_world_name(&tmp_dir("nextworld-empty"), &[]),
        "world1"
    );
}

#[test]
fn the_view_distance_slider_stops_where_the_memory_does() {
    use crate::config::{
        CHUNK_RESIDENT_BYTES, Config, MAX_VIEW_DIST, MIN_VIEW_DIST, max_view_dist_for_memory,
    };

    let cap = max_view_dist_for_memory();
    assert!(
        (MIN_VIEW_DIST..=MAX_VIEW_DIST).contains(&cap),
        "the cap stays inside the playable range, got {cap}"
    );

    // Whatever this machine allows, the loaded set at that distance has to be
    // a number of bytes it could plausibly hold. The slider used to offer 64
    // everywhere — over four gigabytes of resident chunks.
    let chunks = (2u64 * cap as u64 + 1).pow(2);
    let bytes = chunks * CHUNK_RESIDENT_BYTES;
    assert!(
        bytes < 64 * 1024 * 1024 * 1024,
        "a {cap}-chunk view wants {} GiB",
        bytes / (1024 * 1024 * 1024)
    );

    // A config file asking for more than the machine can hold is clamped on
    // the way in rather than honoured into an out-of-memory kill.
    let greedy = Config::from_text(&format!("view_dist={MAX_VIEW_DIST}\n"));
    assert!(
        greedy.view_dist <= cap,
        "config asked {} and got {}, past the {cap} cap",
        MAX_VIEW_DIST,
        greedy.view_dist
    );
    // And one below the floor comes up to it.
    let tiny = Config::from_text("view_dist=1\n");
    assert_eq!(tiny.view_dist, MIN_VIEW_DIST);
}
