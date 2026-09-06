//! Watcher scenarios.

use super::*;

#[test]
fn the_watcher_warns_stands_down_or_graduates() {
    let reg = base_reg();
    let mut w = test_world("watcher");
    // Aggrieved country far from spawn protections.
    for _ in 0..12 {
        w.add_ire_at(500, 500, 1.0);
    }
    w.ire = 30.0;
    let thorn = reg
        .animals
        .iter()
        .position(|a| a.hostile)
        .expect("a warden species");
    let mut m = crate::mobs::Mob::new(thorn, Vec3::new(500.5, 220.0, 500.5), 0.0);
    m.health = 10.0;
    m.watcher = true;
    m.watch_baseline = w.regional_ire_at(500, 500);
    w.spawn_mob(m);
    // A watcher does not hunt, whatever the provocation.
    let def = &reg.animals[thorn];
    let players = [crate::server::PlayerCtx {
        id: 0,
        pos: ep(Vec3::new(504.5, 220.0, 500.5)),
        spawn: ep(Vec3::ZERO),
        attackable: true,
        aggro_mod: 0.0,
        quiet_charm: None,
    }];
    let mut rng = 7u32;
    let mut events = Vec::new();
    let stub = test_world("watcher-stub");
    {
        let mob = w.mob_mut(w.mob_count() - 1).unwrap();
        for _ in 0..40 {
            mob.tick(&stub, def, &players, 0.1, &mut rng, &mut events);
        }
    }
    // (ticked against a stub world only for physics; the state gate
    // is what we assert)
    let mob = w.mob(w.mob_count() - 1).unwrap();
    assert_ne!(
        mob.state,
        crate::mobs::MobState::Hunt,
        "watchers never hunt"
    );
    assert!(mob.watch_timer > 3.5, "the vigil is timed");
    // Mend the ground: the watcher melts away without a corpse.
    for _ in 0..8 {
        w.plant_ire_at(500, 500, 1.0);
    }
    let before = w.mob_count();
    w.grade_watchers();
    assert_eq!(w.mob_count(), before - 1, "answered, it leaves");
    assert!(
        w.whispers.iter().any(|l| l.contains("melts")),
        "and says so"
    );
}
