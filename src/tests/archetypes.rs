//! Enemy archetypes (spec 3.6): resistances, the attack wheel, charge,
//! brute enrage, construct hacking, and builder stamps.
//!
//! Hostile mobs dissolve in `World::tick_mobs`' daylight retirement pass
//! (sky light is unreliable in test worlds), so like the rest of the mob
//! suite these drive `Mob::tick` directly and apply `MobEvent`s themselves.

use super::*;

fn pad(w: &mut World, reg: &Registry, x0: i32, x1: i32, z0: i32, z1: i32, h: i32) {
    let grass = b(reg, "base:grass");
    for x in x0..=x1 {
        for z in z0..=z1 {
            w.set_block(x, h, z, grass);
            for dy in 1..5 {
                if w.get_block(x, h + dy, z) != AIR {
                    w.set_block(x, h + dy, z, AIR);
                }
            }
        }
    }
}

fn ctx(pos: glam::Vec3) -> crate::server::PlayerCtx {
    crate::server::PlayerCtx {
        id: 0,
        pos: ep(pos),
        spawn: ep(glam::Vec3::ZERO),
        attackable: true,
        aggro_mod: 0.0,
        quiet_charm: None,
    }
}

fn mob(reg: &Registry, name: &str, at: glam::Vec3) -> crate::mobs::Mob {
    let si = reg.animal_id(name).expect("species exists");
    let mut m = crate::mobs::Mob::new(si, at, 0.0);
    m.health = reg.animals[si].health;
    m
}

#[test]
fn resistances_scale_warden_hurt_by_damage_class() {
    let reg = base_reg();
    let si = reg.animal_id("base:stonebrute").unwrap();
    let def = reg.animals[si].clone();
    assert_eq!(def.behavior, crate::registry::BehaviorArchetype::Brute);
    let at = ep(glam::Vec3::new(8.5, 181.0, 8.5));
    let from = ep(glam::Vec3::new(9.5, 181.0, 8.5));
    let mut m = crate::mobs::Mob::new_at(si, at, 0.0);
    m.health = def.health;
    // Fire is the brute's vulnerability (x1.5) and enrages it.
    m.hurt(&def, 4.0, Some("fire"), from);
    assert!((def.health - m.health - 6.0).abs() < 0.001, "fire dealt 6");
    assert!(m.rage > 0.0, "a fire hit enrages the brute");
    // Blunt is armoured (x0.6); untyped and unknown classes are full.
    m.hurt(&def, 4.0, Some("blunt"), from);
    assert!((m.health - (def.health - 6.0 - 2.4)).abs() < 0.001, "blunt dealt 2.4");
    m.hurt(&def, 4.0, None, from);
    assert!((m.health - (def.health - 6.0 - 2.4 - 4.0)).abs() < 0.001, "untyped dealt 4");
    m.hurt(&def, 4.0, Some("pierce"), from);
    assert!((m.health - (def.health - 6.0 - 2.4 - 8.0)).abs() < 0.001, "unknown dealt 4");
    // A non-vulnerable hit does not re-enrage after the rage window ends.
    m.rage = 0.0;
    m.hurt(&def, 1.0, Some("blunt"), from);
    assert_eq!(m.rage, 0.0, "armoured classes do not enrage");
}

#[test]
fn attack_wheel_swings_melee_in_reach_then_waits_for_its_cooldown() {
    let reg = base_reg();
    let mut w = test_world("archetype-wheel");
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 20, 0, 20, h);
    let si = reg.animal_id("base:stonebrute").unwrap();
    let def = reg.animals[si].clone();
    let mut m = mob(&reg, "base:stonebrute", glam::Vec3::new(8.5, h as f32 + 1.0, 8.5));
    let player = glam::Vec3::new(9.5, h as f32 + 1.0, 8.5);
    let mut rng = 11u32;
    // The wheel picks the melee "slam" once the player is in reach.
    let mut events = Vec::new();
    for _ in 0..600 {
        m.tick(&w, &def, &[ctx(player)], 1.0 / 60.0, &mut rng, &mut events);
        if events
            .iter()
            .any(|e| matches!(e, crate::mobs::MobEvent::HitPlayer { who: 0, .. }))
        {
            break;
        }
    }
    let hit = events
        .iter()
        .find_map(|e| match e {
            crate::mobs::MobEvent::HitPlayer {
                who: 0,
                dmg,
                dmg_type,
                attack,
                ..
            } => Some((*dmg, dmg_type.clone(), attack.clone())),
            _ => None,
        })
        .expect("the brute slammed from melee reach");
    assert_eq!(hit.2, "slam");
    assert_eq!(hit.0, 6.0);
    assert_eq!(hit.1.as_deref(), Some("blunt"));
    assert!(m.attack_cds[0] > 0.0, "the slam is on its 1.4s cooldown");
    // Nothing else in the wheel is ready in range, and the slam itself is
    // cooling down: a third of a second later there is no second swing.
    let slams = events.len();
    for _ in 0..20 {
        m.tick(&w, &def, &[ctx(player)], 1.0 / 60.0, &mut rng, &mut events);
    }
    let slams_after = events
        .iter()
        .filter(|e| matches!(e, crate::mobs::MobEvent::HitPlayer { attack, .. } if attack == "slam"))
        .count();
    assert_eq!(
        slams_after, slams,
        "the slam did not swing again mid-cooldown"
    );
}

#[test]
fn charge_winds_up_dashes_straight_and_lands_on_the_target() {
    let reg = base_reg();
    let mut w = test_world("archetype-charge");
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 30, 0, 30, h);
    let si = reg.animal_id("base:stonebrute").unwrap();
    let def = reg.animals[si].clone();
    let mut m = mob(&reg, "base:stonebrute", glam::Vec3::new(8.5, h as f32 + 1.0, 8.5));
    let player = glam::Vec3::new(8.5, h as f32 + 1.0, 12.5);
    let mut rng = 13u32;
    let mut events = Vec::new();
    let mut wound_up = false;
    let mut dashed = false;
    for _ in 0..600 {
        m.tick(&w, &def, &[ctx(player)], 1.0 / 60.0, &mut rng, &mut events);
        if m.charge_wind_up > 0.0 {
            wound_up = true;
        }
        if m.dash.is_some() {
            dashed = true;
        }
        if events
            .iter()
            .any(|e| matches!(e, crate::mobs::MobEvent::HitPlayer { who: 0, attack, .. } if attack == "ram"))
        {
            break;
        }
    }
    assert!(wound_up, "the charge wound up before moving");
    assert!(dashed, "the charge became a straight dash");
    let hit = events
        .iter()
        .find_map(|e| match e {
            crate::mobs::MobEvent::HitPlayer {
                who: 0,
                dmg,
                attack,
                ..
            } if attack == "ram" => Some(*dmg),
            _ => None,
        })
        .expect("the charge landed its hit on the player");
    assert_eq!(hit, 9.0);
}

#[test]
fn construct_hack_freezes_it_and_drops_the_core_instantly() {
    let reg = base_reg();
    let mut w = test_world("archetype-construct");
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 20, 0, 20, h);
    let si = reg.animal_id("base:cogmaw").unwrap();
    let def = reg.animals[si].clone();
    assert_eq!(def.behavior, crate::registry::BehaviorArchetype::Construct);
    assert!(def.hack.is_some(), "a construct carries a hack table");
    w.spawn_mob(mob(&reg, "base:cogmaw", glam::Vec3::new(8.5, h as f32 + 1.0, 8.5)));
    let idx = w.mobs().iter().position(|m| m.species == si).unwrap();
    let mut rng = 5u32;
    assert!(w.hack_mob(idx, &mut rng) > 0, "the core drops immediately");
    let core = w.pending_drops().iter().any(|(_, stack)| {
        let n = reg.item(stack.item).name.clone();
        n == "base:iron_gear" || n == "base:plate"
    });
    assert!(core, "the core drops land beside the construct");
    // A hacked construct never fights: it sits inert even at the player's side.
    let mut m = w.mobs()[idx].clone();
    let player = glam::Vec3::new(9.5, h as f32 + 1.0, 8.5);
    let mut events = Vec::new();
    for _ in 0..120 {
        m.tick(&w, &def, &[ctx(player)], 1.0 / 60.0, &mut rng, &mut events);
    }
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, crate::mobs::MobEvent::HitPlayer { .. })),
        "a hacked construct is inert"
    );
    assert_eq!(m.state, crate::mobs::MobState::Idle);
    // Wildlife has no hack table: hacking a deer is a no-op with no drops.
    w.spawn_mob(mob(&reg, "base:deer", glam::Vec3::new(3.5, h as f32 + 1.0, 3.5)));
    let didx = w.mobs().iter().position(|m| m.species == reg.animal_id("base:deer").unwrap()).unwrap();
    let before = w.pending_drops().len();
    assert_eq!(w.hack_mob(didx, &mut rng), 0);
    assert_eq!(w.pending_drops().len(), before, "no drops for a non-construct");
}

#[test]
fn builder_stamps_its_template_to_the_cap_and_cells_survive_reload() {
    let reg = base_reg();
    let dir = tmp_dir("archetype-builder").join("world");
    let mut w = World::load_or_create(dir.clone(), reg.clone()).unwrap();
    for x in -3..=3 {
        for z in -3..=3 {
            w.ensure_chunk(tchunk(x, z));
        }
    }
    let h = w.surface_height(8, 8);
    pad(&mut w, &reg, 0, 20, 0, 20, h);
    let si = reg.animal_id("base:tumulus").unwrap();
    let def = reg.animals[si].clone();
    assert_eq!(def.behavior, crate::registry::BehaviorArchetype::Builder);
    let builder = def.builder.as_ref().expect("tumulus carries a builder def");
    // Capture a small template under the authored name.
    let stone = reg.block_id("base:stone").unwrap();
    w.set_block(5, h, 5, stone);
    w.set_block(6, h, 5, stone);
    w.set_block(5, h + 1, 5, stone);
    let name = builder.template.as_deref().unwrap();
    assert_eq!(w.capture_and_save(bp(5, h, 5), bp(6, h + 1, 5), name).unwrap(), 3);
    // A far-off builder idles and stamps its template on the interval.
    let mut m = mob(&reg, "base:tumulus", glam::Vec3::new(12.5, h as f32 + 1.0, 12.5));
    let mut rng = 9u32;
    let mut events = Vec::new();
    for _ in 0..200 {
        m.tick(&w, &def, &[], 1.0, &mut rng, &mut events);
        for e in &events {
            // The game loop applies Build events through the block path.
            if let crate::mobs::MobEvent::Build { template, anchor, rot } = e {
                let t = w.template(template).unwrap().clone();
                w.stamp_mob(&t, *anchor, *rot);
            }
        }
        events.clear();
    }
    assert_eq!(
        m.built_count,
        builder.cap,
        "the builder stamped up to its cap"
    );
    let count = |world: &World| {
        let mut n = 0;
        for x in 5..=19 {
            for z in 5..=19 {
                for y in h + 1..=h + 2 {
                    if world.get_block(x, y, z) != AIR {
                        n += 1;
                    }
                }
            }
        }
        n
    };
    assert!(count(&w) >= 3, "stamped cells landed through the block path");
    save_world(&mut w);
    let mut w2 = World::load_or_create(dir, reg.clone()).unwrap();
    for x in -3..=3 {
        for z in -3..=3 {
            w2.ensure_chunk(tchunk(x, z));
        }
    }
    assert_eq!(
        count(&w2),
        count(&w),
        "the stamped cells survived save and reload"
    );
}