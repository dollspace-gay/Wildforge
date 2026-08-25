//! In-engine playtest for the belt-quest vertical slice (Milestones B/C).
//!
//! This drives the REAL shipped mods tree through the REAL world sim:
//! the Haven assembly stamps and its people spawn, the quest arc's script
//! callbacks fire, depot deliveries pay appetite, the Brood Gallery opens
//! with its garrison, and the conduit lock judges valve sequences. Every
//! failure here is a content regression a player would hit in-game.

use super::*;
use crate::planet::Face;
use std::path::PathBuf;

fn bq_mods() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("mods")
}

fn bq_reg() -> Arc<Registry> {
    let reg = Arc::new(registry::load(&bq_mods()));
    assert!(
        reg.material_errors.is_empty(),
        "belt_quest loads clean: {:?}",
        reg.material_errors
    );
    reg
}

/// A host-side ScriptHost loaded with belt_quest's main.rhai.
fn bq_scripts() -> crate::script::ScriptHost {
    let mut scripts = crate::script::ScriptHost::new();
    scripts.load_mods(&[("belt_quest".to_string(), bq_mods().join("belt_quest"))]);
    scripts
}

/// Read a belt_quest storage key straight from the script KV (mod namespace).
fn storage_of(scripts: &crate::script::ScriptHost, key: &str) -> Option<String> {
    scripts
        .kv
        .borrow()
        .get("belt_quest")
        .and_then(|m| m.get(key))
        .cloned()
}

/// Dispatch a no-arg event and drain the command queue.
fn fire(scripts: &mut crate::script::ScriptHost, w: &World, event: &str) -> Vec<crate::script::Cmd> {
    if scripts.wants(event) {
        scripts.dispatch(w, event, ());
    }
    scripts.take_cmds()
}

/// Dispatch `on_screen_click` for a named valve/button and drain commands.
fn click(
    scripts: &mut crate::script::ScriptHost,
    w: &World,
    screen: &str,
    action: &str,
) -> Vec<crate::script::Cmd> {
    if scripts.wants("on_screen_click") {
        scripts.dispatch(
            w,
            "on_screen_click",
            (screen.to_string(), action.to_string()),
        );
    }
    scripts.take_cmds()
}

/// Resolve `spawn:npc:` markers through the same seam chunks use.
type Markers = Vec<crate::world::pieces::AssemblyMarker>;
fn resolve_npc_markers(w: &mut World, reg: &Registry, markers: &Markers) {
    for marker in markers {
        if let Some(npc_name) = marker.kind.strip_prefix("spawn:npc:")
            && let Some(ni) = reg.npc_id(npc_name)
            && let Ok(pos) = crate::planet::EntityPos::new(
                marker.at.face(),
                f32::from(marker.at.u()) + 0.5,
                f32::from(marker.at.y()) + 1.05,
                f32::from(marker.at.v()) + 0.5,
            )
        {
            w.spawn_npc_at(ni, pos);
        }
    }
}

#[test]
fn haven_stamps_and_its_people_show_up() {
    let reg = bq_reg();
    let mut w = test_world_with("bq-haven", reg.clone());
    let h = w.surface_height(8, 8);
    // A generous flat stage: the walk may extend several pieces east.
    for x in 16..=48 {
        for z in 16..=40 {
            w.set_block(x, h, z, b(&reg, "base:grass"));
        }
    }
    // Stamp on the Deep: a deterministic void stage with no biome gate or
    // sea-level veto between the test and the content under test. Seeds
    // vary which pool pieces the walk picks; retry until all four voices
    // of Haven are present, then verify the factory stands.
    let asm = reg
        .assemblies
        .iter()
        .find(|a| a.name == "belt_quest:haven")
        .expect("haven assembly resolves")
        .clone();
    let mut npc_names: Vec<String> = Vec::new();
    for seed in 0xBEEFu32..0xBEF6 {
        let anchor = ChunkPos::new(Face::Deep, 16 + (seed % 8) as u16 * 4, 16).unwrap();
        let (markers, count) = w.place_assembly(asm.clone(), anchor, seed);
        assert!(count >= 1, "the entry piece places");
        resolve_npc_markers(&mut w, &reg, &markers);
        npc_names.extend(
            w.npcs()
                .iter()
                .filter_map(|n| reg.npcs.get(n.def).map(|d| d.name.clone())),
        );
        let all = ["Elder Marek", "Tinkerer Sal", "Ava", "Jonas"];
        if all.iter().all(|want| npc_names.iter().any(|n| n.contains(want))) {
            break;
        }
    }
    for want in ["elder_marek", "tinkerer_sal", "merchant_ava", "guard_jonas"] {
        assert!(
            npc_names.iter().any(|n| n.contains(want)),
            "{want} showed up"
        );
    }
    // Factory check: scan every resident Deep chunk for the workshop
    // trio. The workshop piece placed wherever the walk attached it.
    let mut found: Vec<String> = Vec::new();
    'outer: for want in [
        "belt_quest:smelter",
        "belt_quest:assembler",
        "belt_quest:depot",
    ] {
        let want_id = reg.block_id(want).unwrap();
        for (pos, chunk) in w.chunks().iter() {
            if !pos.face().is_deep() {
                continue;
            }
            for i in 0..chunk.raw().len() {
                if chunk.raw()[i] == want_id.0 {
                    found.push(want.to_string());
                    continue 'outer;
                }
            }
        }
    }
    assert_eq!(found.len(), 3, "the workshop trio stands: {found:?}");
}

#[test]
fn quest_arc_callbacks_behave() {
    let w = test_world("bq-arc");
    let mut scripts = bq_scripts();

    // Settling In accepted via Marek's callback (the host pairs the
    // QuestAccept with its own toast).
    let mut accepted = false;
    for cmd in fire(&mut scripts, &w, "accept_settling_in") {
        if let crate::script::Cmd::QuestAccept(id) = cmd {
            assert_eq!(id, "settling_in");
            accepted = true;
        }
    }
    assert!(accepted, "Settling In was accepted");

    // Jonas lends the blade once; storage remembers so he never re-gifts.
    fire(&mut scripts, &w, "greet_first_threat");
    assert_eq!(storage_of(&scripts, "jonas_blade"), Some("given".into()));

    // First Threat: three wolves, three objective units.
    fire(&mut scripts, &w, "accept_first_threat");
    let mut wolf_units = 0;
    for _ in 0..3 {
        if scripts.wants("on_animal_killed") {
            scripts.dispatch(
                &w,
                "on_animal_killed",
                ("base:wolf".to_string(), 0i64, 0i64, 0i64, 0i64),
            );
        }
        for cmd in scripts.take_cmds() {
            let crate::script::Cmd::QuestProgress { quest_id, n, .. } = cmd else {
                continue;
            };
            assert_eq!(quest_id, "first_threat");
            wolf_units += n;
        }
    }
    assert_eq!(wolf_units, 3, "each wolf kill reports one objective unit");

    // Sal's Spark: materials arrive in the queue when the quest is taken.
    let cmds = fire(&mut scripts, &w, "accept_the_spark");
    for c in &cmds {
        if let crate::script::Cmd::Give(name, _) = c {
            let _ = name;
        }
    }
    assert!(
        cmds.iter().any(|c| matches!(c, crate::script::Cmd::Give(name, _) if name == "base:firebrick")),
        "Sal hands over smelter materials"
    );
}

#[test]
fn conduit_lock_judges_valve_sequences() {
    let w = test_world("bq-conduits");
    let mut scripts = bq_scripts();
    const SCREEN: &str = "belt_quest:conduits";

    // Wrong first valve: reset, no progress ever reported.
    for v in ["valve_a", "valve_c"] {
        for cmd in click(&mut scripts, &w, SCREEN, v) {
            let crate::script::Cmd::QuestProgress { quest_id, .. } = cmd else {
                continue;
            };
            panic!("a wrong sequence completed {quest_id}");
        }
    }

    // The authored order (humming, brass, steel, copper) completes the lock.
    let mut solved = false;
    for v in ["valve_d", "valve_a", "valve_c", "valve_b"] {
        for cmd in click(&mut scripts, &w, SCREEN, v) {
            if let crate::script::Cmd::QuestProgress { quest_id, .. } = cmd {
                assert_eq!(quest_id, "mines_conduits");
                solved = true;
            }
        }
    }
    assert!(solved, "the authored valve order completed the lock");
}

#[test]
fn depot_deliveries_pay_haven_appetite() {
    let reg = bq_reg();
    let mut w = test_world_with("bq-depot", reg.clone());
    let h = 80;
    for x in 10..=20 {
        for z in 10..=20 {
            w.set_block(x, h, z, b(&reg, "base:grass"));
        }
    }
    let pos = bp(15, h, 15);
    w.set_block_at(pos, AIR);
    assert!(
        w.place_block_at(pos, b(&reg, "belt_quest:depot")),
        "the depot places"
    );
    // Haven declares bread at rep 2.
    let bread = it(&reg, "base:bread");
    let (wanted, rep_per_unit) = w.depot_need_at(pos, bread).expect("Haven wants bread");
    assert!(wanted > 0);
    assert_eq!(rep_per_unit, 2);
    // Fill the full appetite window by hand.
    let mut remaining = wanted;
    while remaining > 0 {
        let batch = remaining.min(5);
        let stack = ItemStack::new(&reg, bread, batch);
        assert_eq!(w.depot_deposit(pos, &stack), batch, "the depot takes bread");
        remaining -= batch;
    }
    // Staged stock eats the appetite: once Haven's bread window is full,
    // the need disappears until the settlement consumes it.
    assert!(
        w.depot_need_at(pos, bread).is_none(),
        "a full appetite stops wanting bread"
    );
    // A non-need gets refused outright.
    let stone = it(&reg, "base:cobblestone");
    assert!(
        w.depot_need_at(pos, stone).is_none(),
        "no appetite for stone"
    );
}

#[test]
fn brood_gallery_opens_and_the_den_populates() {
    let reg = bq_reg();
    // Procedural variance: the walk may or may not roll the hoard piece.
    // Each attempt uses a FRESH world because entering the same assembly
    // twice legitimately reuses the live run instead of restamping.
    let mut picked: Option<(World, crate::planet::EntityPos)> = None;
    for attempt in 0..8u32 {
        // Vary the world seed: a fresh world otherwise replays the exact
        // same deterministic walk (same seed, same run_seed reset).
        let mut attempt_w =
            test_world_seeded(&format!("bq-gallery-{attempt}"), reg.clone(), 42 + attempt * 977);
        for x in -2..=2 {
            for z in -2..=2 {
                attempt_w.ensure_chunk(tchunk(x, z));
            }
        }
        let player = ep(Vec3::new(8.0, 130.0, 8.0));
        let spawn = attempt_w
            .enter_dungeon(0, player, "belt_quest:brood_gallery")
            .expect("the gallery opens");
        assert_eq!(spawn.block().expect("canonical").face(), Face::Deep);
        let mother_home = attempt_w.nests().any(|(_, i)| {
            reg.nest(i).is_some_and(|n| {
                n.species == reg.animal_id("belt_quest:brood_mother").unwrap()
            })
        });
        if mother_home {
            picked = Some((attempt_w, spawn));
            break;
        }
    }
    let (mut w, spawn) = picked.expect("some run rolled the Brood Mother's den");
    assert_eq!(spawn.block().expect("canonical").face(), Face::Deep);
    let spider = reg.animal_id("belt_quest:cave_spider").unwrap();
    let mother = reg.animal_id("belt_quest:brood_mother").unwrap();
    let depositor = ep(Vec3::new(8.0, 130.0, 8.0));
    let mut rng = 31u32;
    for _ in 0..10 {
        w.tick_nest_spawns(spawn, 0.12, 5.0, &mut rng);
    }
    assert!(
        w.mobs().iter().any(|m| m.species == spider),
        "burrow spiders manifest"
    );
    assert!(
        w.mobs().iter().any(|m| m.species == mother),
        "the Brood Mother holds her den"
    );

    let back = w.exit_dungeon(0, spawn).expect("exit routes home");
    assert_eq!(back, depositor);
}

#[test]
fn custodian_thresholds_unlock_their_paths() {
    let w = test_world("bq-custodian");
    let mut scripts = bq_scripts();

    // Crossing cooperative completes the alliance quest (beacon template).
    scripts.take_cmds();
    // Drive rep up by simulating what wave-two callbacks write.
    {
        let kv = scripts.kv.borrow_mut();
        let mut ns = kv.clone();
        ns.entry("belt_quest".to_string())
            .or_default()
            .insert("custodian_rep".to_string(), "350".to_string());
        drop(kv);
        // Write back through the public surface.
        *scripts.kv.borrow_mut() = ns;
    }
    if scripts.wants("custodian_thresholds") {
        scripts.dispatch(&w, "custodian_thresholds", ());
    }
    let cmds = scripts.take_cmds();
    assert!(
        cmds.iter().any(|c| matches!(
            c,
            crate::script::Cmd::QuestAccept(id) if id == "custodian_alliance"
        )),
        "crossing +300 accepts the alliance quest"
    );

    // Crossing hostile unlocks salvage protocols instead.
    let mut scripts = bq_scripts();
    {
        let mut kv = scripts.kv.borrow_mut();
        kv.entry("belt_quest".to_string())
            .or_default()
            .insert("custodian_rep".to_string(), "-350".to_string());
    }
    if scripts.wants("custodian_thresholds") {
        scripts.dispatch(&w, "custodian_thresholds", ());
    }
    let cmds = scripts.take_cmds();
    assert!(
        cmds.iter().any(|c| matches!(
            c,
            crate::script::Cmd::QuestAccept(id) if id == "custodian_enmity"
        )),
        "crossing -300 accepts the enmity quest"
    );
}
