//! Hunt cancellation scenarios.

use super::*;

#[test]
fn a_wolf_ends_its_player_chase_when_animal_prey_appears() {
    let (mut world, player) = wolf_hunt_fixture("wolf-new-prey", -999.0);
    world.tick_mobs(&[player], 0.1, 0.05, &mut 17);
    assert_eq!(world.mobs()[0].state, crate::mobs::MobState::Hunt);
    world.spawn_mob(beast(&world.reg, "base:deer", Vec3::new(12.5, 101.0, 8.5)));
    let events = world.tick_mobs(&[player], 0.1, 0.05, &mut 17);
    assert_eq!(world.mobs()[0].state, crate::mobs::MobState::Stalk);
    assert!(!world.mobs()[0].bold);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, crate::mobs::MobEvent::HitPlayer { .. }))
    );
}

#[test]
fn a_wolf_ends_its_player_chase_when_no_longer_starving() {
    let (mut world, player) = wolf_hunt_fixture("wolf-fed-again", -999.0);
    world.tick_mobs(&[player], 0.1, 0.05, &mut 17);
    assert_eq!(world.mobs()[0].state, crate::mobs::MobState::Hunt);
    world.mobs_mut()[0].belly = 480.0;
    let events = world.tick_mobs(&[player], 0.1, 0.05, &mut 17);
    assert_ne!(world.mobs()[0].state, crate::mobs::MobState::Hunt);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, crate::mobs::MobEvent::HitPlayer { .. }))
    );
}

#[test]
fn a_hungry_but_not_starving_wolf_does_not_hunt_players() {
    let (mut world, player) = wolf_hunt_fixture("wolf-not-desperate", -1.0);
    let events = world.tick_mobs(&[player], 0.1, 0.05, &mut 17);
    assert!(!world.mobs()[0].bold);
    assert_ne!(world.mobs()[0].state, crate::mobs::MobState::Hunt);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, crate::mobs::MobEvent::HitPlayer { .. }))
    );
}

#[test]
fn a_wounded_starving_wolf_keeps_fleeing() {
    let (mut world, player) = wolf_hunt_fixture("wolf-wounded-flees", -999.0);
    world.tick_mobs(&[player], 0.1, 0.05, &mut 17);
    let definition = world.reg.animals[world.mobs()[0].species].clone();
    world.mobs_mut()[0].hurt(&definition, 4.0, None, player.pos);
    let events = world.tick_mobs(&[player], 0.1, 0.05, &mut 17);
    assert_eq!(world.mobs()[0].state, crate::mobs::MobState::Flee);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, crate::mobs::MobEvent::HitPlayer { .. }))
    );
}

#[test]
fn a_frog_flees_players_even_when_hungry_at_night() {
    let (mut world, player) = wolf_hunt_fixture("frog-flees-player", -999.0);
    let mut frog = beast(&world.reg, "base:frog", Vec3::new(6.5, 101.0, 8.5));
    frog.belly = -999.0;
    world.replace_mobs(vec![frog]);
    let events = world.tick_mobs(&[player], 0.1, 0.05, &mut 17);
    assert_eq!(world.mobs()[0].state, crate::mobs::MobState::Flee);
    assert!(!world.mobs()[0].bold);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, crate::mobs::MobEvent::HitPlayer { .. }))
    );
}
