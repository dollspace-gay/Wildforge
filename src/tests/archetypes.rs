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
    assert!(
        (m.health - (def.health - 6.0 - 2.4)).abs() < 0.001,
        "blunt dealt 2.4"
    );
    m.hurt(&def, 4.0, None, from);
    assert!(
        (m.health - (def.health - 6.0 - 2.4 - 4.0)).abs() < 0.001,
        "untyped dealt 4"
    );
    m.hurt(&def, 4.0, Some("pierce"), from);
    assert!(
        (m.health - (def.health - 6.0 - 2.4 - 8.0)).abs() < 0.001,
        "unknown dealt 4"
    );
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
    let mut m = mob(
        &reg,
        "base:stonebrute",
        glam::Vec3::new(8.5, h as f32 + 1.0, 8.5),
    );
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
        .filter(
            |e| matches!(e, crate::mobs::MobEvent::HitPlayer { attack, .. } if attack == "slam"),
        )
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
    let mut m = mob(
        &reg,
        "base:stonebrute",
        glam::Vec3::new(8.5, h as f32 + 1.0, 8.5),
    );
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
    w.spawn_mob(mob(
        &reg,
        "base:cogmaw",
        glam::Vec3::new(8.5, h as f32 + 1.0, 8.5),
    ));
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
    w.spawn_mob(mob(
        &reg,
        "base:deer",
        glam::Vec3::new(3.5, h as f32 + 1.0, 3.5),
    ));
    let didx = w
        .mobs()
        .iter()
        .position(|m| m.species == reg.animal_id("base:deer").unwrap())
        .unwrap();
    let before = w.pending_drops().len();
    assert_eq!(w.hack_mob(didx, &mut rng), 0);
    assert_eq!(
        w.pending_drops().len(),
        before,
        "no drops for a non-construct"
    );
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
    assert_eq!(
        w.capture_and_save(bp(5, h, 5), bp(6, h + 1, 5), name)
            .unwrap(),
        3
    );
    // A far-off builder idles and stamps its template on the interval.
    let mut m = mob(
        &reg,
        "base:tumulus",
        glam::Vec3::new(12.5, h as f32 + 1.0, 12.5),
    );
    let mut rng = 9u32;
    let mut events = Vec::new();
    for _ in 0..200 {
        m.tick(&w, &def, &[], 1.0, &mut rng, &mut events);
        for e in &events {
            // The game loop applies Build events through the block path.
            if let crate::mobs::MobEvent::Build {
                template,
                anchor,
                rot,
            } = e
            {
                let t = w.template(template).unwrap().clone();
                w.stamp_mob(&t, *anchor, *rot);
            }
        }
        events.clear();
    }
    assert_eq!(
        m.built_count, builder.cap,
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
    assert!(
        count(&w) >= 3,
        "stamped cells landed through the block path"
    );
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
// ---------------- capability E9: data-driven behavior archetypes ----------------
//
// These species live in a temp mod so the engine's general-purpose
// archetypes are exercised without coupling base content to them.

const E9_ANIMALS: &str = r#"
[[animal]]
id = "redprowler"
name = "Red Prowler"
hostile = true
biomes = ["plains"]
health = 8
speed = 3.0
attack = 4
tex = "@deer"
aggro_range = 24
behavior = "rusher"
rusher = { rush_mult = 2.4 }

[[animal]]
id = "laggard"
name = "Laggard"
hostile = true
biomes = ["plains"]
health = 8
speed = 3.0
attack = 4
tex = "@deer"
aggro_range = 24

[[animal]]
id = "boulderback"
name = "Boulderback"
hostile = true
biomes = ["plains"]
health = 40
speed = 1.2
attack = 5
tex = "@deer"
aggro_range = 24
behavior = "tank"
tank = { knockback_mult = 0.25 }

[[animal]]
id = "spinecaster"
name = "Spinecaster"
hostile = true
biomes = ["plains"]
health = 10
speed = 2.0
attack = 2
tex = "@deer"
aggro_range = 40
behavior = "sniper"
sniper = { keep_min = 9, keep_max = 16 }
attacks = [
  { name = "spit", kind = "projectile", damage = 3, cooldown = 2.0, range = 18, damage_type = "pierce",
    projectile = { tex = "@ember_bolt", damage = 3, speed = 13, cooldown = 2.0 } },
]

[[animal]]
id = "willowkeeper"
name = "Willowkeeper"
hostile = true
biomes = ["plains"]
health = 15
speed = 1.8
attack = 2
tex = "@deer"
aggro_range = 20
behavior = "support"
support = { radius = 10, interval = 6, heal = 2 }

[[animal]]
id = "broodmother"
name = "Broodmother"
hostile = true
biomes = ["plains"]
health = 20
speed = 1.5
attack = 4
tex = "@deer"
aggro_range = 20
behavior = "swarm"
swarm = { spawn = "e9fauna:broodling", count = 3 }

[[animal]]
id = "broodling"
name = "Broodling"
hostile = true
biomes = ["plains"]
health = 4
speed = 3.0
attack = 2
tex = "@deer"
aggro_range = 16

[[animal]]
id = "cinderlord"
name = "Cinderlord"
hostile = true
biomes = ["plains"]
health = 25
speed = 1.6
attack = 4
tex = "@deer"
aggro_range = 20
behavior = "controller"
controller = { spawn = "e9fauna:cinder", count = 2, interval = 4, max = 6 }

[[animal]]
id = "cinder"
name = "Cinder"
hostile = true
biomes = ["plains"]
health = 3
speed = 2.5
attack = 1
tex = "@deer"
aggro_range = 16

[[animal]]
id = "wispphantom"
name = "Wisp Phantom"
hostile = true
biomes = ["plains"]
health = 6
speed = 2.0
attack = 3
tex = "@deer"
aggro_range = 30
behavior = "phaser"
phaser = { blink_range = 7, blink_cd = 1 }

[[animal]]
id = "huskwarden"
name = "Huskwarden"
hostile = true
biomes = ["plains"]
health = 30
speed = 1.5
attack = 4
tex = "@deer"
aggro_range = 20
behavior = "shield_bearer"
shield = { front_mult = 0.35, front_deg = 90 }
"#;

fn e9_reg(tag: &str) -> Arc<Registry> {
    let root = tmp_dir(&format!("e9-fauna-{tag}"));
    let dir = root.join("e9fauna");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("mod.toml"),
        "id = \"e9fauna\"\nworld_api = 2\ndepends = [\"base\"]\n",
    )
    .unwrap();
    std::fs::write(dir.join("animals.toml"), E9_ANIMALS).unwrap();
    Arc::new(registry::load(&root))
}

#[test]
fn e9_behavior_strings_parse_to_archetypes() {
    use crate::registry::BehaviorArchetype;
    let reg = e9_reg("parse");
    let cases = [
        ("e9fauna:redprowler", BehaviorArchetype::Rusher),
        ("e9fauna:boulderback", BehaviorArchetype::Tank),
        ("e9fauna:spinecaster", BehaviorArchetype::Sniper),
        ("e9fauna:willowkeeper", BehaviorArchetype::Support),
        ("e9fauna:broodmother", BehaviorArchetype::Swarm),
        ("e9fauna:cinderlord", BehaviorArchetype::Controller),
        ("e9fauna:wispphantom", BehaviorArchetype::Phaser),
        ("e9fauna:huskwarden", BehaviorArchetype::ShieldBearer),
    ];
    for (name, behavior) in cases {
        let si = reg.animal_id(name).expect("species exists");
        assert_eq!(reg.animals[si].behavior, behavior, "{name}");
    }
    // Params land with engine defaults applied.
    let sniper = reg.animals[reg.animal_id("e9fauna:spinecaster").unwrap()]
        .archetype
        .sniper
        .as_ref()
        .unwrap();
    assert_eq!(sniper.keep_max, 16.0);
    let swarm = reg.animals[reg.animal_id("e9fauna:broodmother").unwrap()]
        .archetype
        .swarm
        .as_ref()
        .unwrap();
    assert_eq!(swarm.count, 3);
    let controller = reg.animals[reg.animal_id("e9fauna:cinderlord").unwrap()]
        .archetype
        .controller
        .as_ref()
        .unwrap();
    assert_eq!(controller.max, 6);
}

/// Run `species` hunting a player 10 blocks north for `frames` frames and
/// return the remaining distance (a sprint closes more ground).
fn hunt_closing_distance(reg: &Arc<Registry>, species: &str, h: i32, frames: usize) -> f32 {
    let mut w = test_world_with("e9-hunt", Arc::clone(reg));
    pad(&mut w, reg, 0, 40, 0, 40, h);
    let si = reg.animal_id(species).unwrap();
    let def = reg.animals[si].clone();
    let mut m = crate::mobs::Mob::new(si, glam::Vec3::new(8.5, h as f32 + 1.0, 8.5), 0.0);
    m.health = def.health;
    let player = glam::Vec3::new(8.5, h as f32 + 1.0, 18.5);
    let mut rng = 7u32;
    let mut events = Vec::new();
    for _ in 0..frames {
        m.tick(&w, &def, &[ctx(player)], 1.0 / 60.0, &mut rng, &mut events);
    }
    m.pos.local_delta_to(ep(player)).length()
}

#[test]
fn rusher_sprints_while_hunting() {
    let reg = e9_reg("rusher");
    let h = 80;
    let rusher = hunt_closing_distance(&reg, "e9fauna:redprowler", h, 40);
    let laggard = hunt_closing_distance(&reg, "e9fauna:laggard", h, 40);
    assert!(
        rusher < laggard,
        "the rusher closed more ground: {rusher} vs {laggard}"
    );
}

#[test]
fn tank_holds_its_ground() {
    let reg = e9_reg("tank");
    let tank_si = reg.animal_id("e9fauna:boulderback").unwrap();
    let lag_si = reg.animal_id("e9fauna:laggard").unwrap();
    let tank_def = reg.animals[tank_si].clone();
    let lag_def = reg.animals[lag_si].clone();
    let mut tank = crate::mobs::Mob::new(tank_si, glam::Vec3::new(0.0, 80.0, 0.0), 0.0);
    tank.health = 100.0;
    let mut lag = crate::mobs::Mob::new(lag_si, glam::Vec3::new(0.0, 80.0, 0.0), 0.0);
    lag.health = 100.0;
    let from = ep(glam::Vec3::new(3.0, 80.0, 0.0));
    tank.hurt(&tank_def, 5.0, None, from);
    lag.hurt(&lag_def, 5.0, None, from);
    let tank_kb = glam::Vec2::new(tank.vel.x, tank.vel.z).length();
    let lag_kb = glam::Vec2::new(lag.vel.x, lag.vel.z).length();
    assert!(
        tank_kb < lag_kb * 0.5,
        "the tank held its ground: {tank_kb} vs {lag_kb}"
    );
}

/// Run the sniper toward `player` for 60 frames and return the final
/// distance to the player.
fn sniper_band(reg: &Arc<Registry>, start: glam::Vec3, player: glam::Vec3) -> f32 {
    let mut w = test_world_with("e9-sniper", Arc::clone(reg));
    let h = 80;
    pad(&mut w, reg, 0, 60, 0, 60, h);
    let si = reg.animal_id("e9fauna:spinecaster").unwrap();
    let def = reg.animals[si].clone();
    let mut m = crate::mobs::Mob::new(si, start, 0.0);
    m.health = def.health;
    let mut rng = 3u32;
    let mut events = Vec::new();
    for _ in 0..60 {
        m.tick(&w, &def, &[ctx(player)], 1.0 / 60.0, &mut rng, &mut events);
    }
    m.pos.local_delta_to(ep(player)).length()
}

#[test]
fn sniper_keeps_its_band() {
    let reg = e9_reg("sniper");
    let h = 80.0;
    // Too close (4 blocks < keep_min 9): retreats.
    let close = sniper_band(
        &reg,
        glam::Vec3::new(8.5, h + 1.0, 12.5),
        glam::Vec3::new(8.5, h + 1.0, 8.5),
    );
    assert!(close > 4.6, "the sniper retreated, got {close}");
    // Too far (20 blocks > keep_max 16): closes in.
    let far = sniper_band(
        &reg,
        glam::Vec3::new(8.5, h + 1.0, 12.5),
        glam::Vec3::new(8.5, h + 1.0, 32.5),
    );
    assert!(far < 19.0, "the sniper closed in, got {far}");
}

#[test]
fn support_heals_allies() {
    let reg = e9_reg("support");
    let mut w = test_world_with("e9-support", reg.clone());
    let h = 80;
    pad(&mut w, &reg, 0, 40, 0, 40, h);
    let support_si = reg.animal_id("e9fauna:willowkeeper").unwrap();
    let ally_si = reg.animal_id("e9fauna:cinder").unwrap();
    let support_def = reg.animals[support_si].clone();
    let ally_def = reg.animals[ally_si].clone();
    let mut support =
        crate::mobs::Mob::new(support_si, glam::Vec3::new(8.5, h as f32 + 1.0, 8.5), 0.0);
    support.health = support_def.health;
    let mut ally = crate::mobs::Mob::new(ally_si, glam::Vec3::new(10.5, h as f32 + 1.0, 9.5), 0.0);
    ally.health = 1.0; // wounded
    let mut rng = 3u32;
    let mut events = Vec::new();
    // ~7s crosses the 6s interval.
    for _ in 0..420 {
        support.tick(
            &w,
            &support_def,
            &[ctx(glam::Vec3::new(20.5, h as f32 + 1.0, 20.5))],
            1.0 / 60.0,
            &mut rng,
            &mut events,
        );
    }
    let pulse = events.iter().find_map(|e| match e {
        crate::mobs::MobEvent::HealPulse {
            origin,
            radius,
            heal,
        } => Some((*origin, *radius, *heal)),
        _ => None,
    });
    let Some((origin, radius, heal)) = pulse else {
        panic!("no heal pulse fired");
    };
    assert_eq!(heal, 2.0);
    assert!(
        ally.pos.local_delta_to(origin).length() <= radius,
        "the ally sat inside the pulse radius"
    );
    ally.health = (ally.health + heal).min(ally_def.health);
    assert_eq!(ally.health, ally_def.health, "the ally recovered to full");
}

#[test]
fn swarm_releases_brood_on_death() {
    let reg = e9_reg("swarm");
    let mut w = test_world_with("e9-swarm", reg.clone());
    let h = 80;
    pad(&mut w, &reg, 0, 40, 0, 40, h);
    let mother_si = reg.animal_id("e9fauna:broodmother").unwrap();
    let brood_si = reg.animal_id("e9fauna:broodling").unwrap();
    let mother_def = reg.animals[mother_si].clone();
    let before = w.mobs().iter().filter(|m| m.species == brood_si).count();
    let mother = crate::mobs::Mob::new(mother_si, glam::Vec3::new(8.5, h as f32 + 1.0, 8.5), 0.0);
    w.spawn_mob(mother);
    if let Some(m) = w.mob_by_id_mut(w.mobs().last().unwrap().id) {
        m.health = 0.0;
    }
    let mut rng = 5u32;
    w.settle_dead_mobs(&mut rng);
    let after = w.mobs().iter().filter(|m| m.species == brood_si).count();
    assert_eq!(
        after - before,
        3,
        "the brood released where the mother fell"
    );
    let _ = mother_def;
}

#[test]
fn controller_summons_minions_up_to_its_cap() {
    let reg = e9_reg("controller");
    let mut w = test_world_with("e9-controller", reg.clone());
    let h = 80;
    pad(&mut w, &reg, 0, 40, 0, 40, h);
    let lord_si = reg.animal_id("e9fauna:cinderlord").unwrap();
    let cinder_si = reg.animal_id("e9fauna:cinder").unwrap();
    let lord_def = reg.animals[lord_si].clone();
    let mut lord = crate::mobs::Mob::new(lord_si, glam::Vec3::new(8.5, h as f32 + 1.0, 8.5), 0.0);
    lord.health = lord_def.health;
    let mut rng = 9u32;
    let mut events = Vec::new();
    for _ in 0..360 {
        lord.tick(
            &w,
            &lord_def,
            &[ctx(glam::Vec3::new(40.0, h as f32 + 1.0, 40.0))],
            1.0 / 60.0,
            &mut rng,
            &mut events,
        );
        let summons: Vec<(crate::planet::EntityPos, usize, u32)> = events
            .drain(..)
            .filter_map(|e| match e {
                crate::mobs::MobEvent::SpawnMinions {
                    pos,
                    species,
                    count,
                } => Some((pos, species, count)),
                _ => None,
            })
            .collect();
        for (pos, species, count) in summons {
            for _ in 0..count.min(4) {
                let mut c = crate::mobs::Mob::new_at(species, pos, 0.0);
                c.health = 3.0;
                w.spawn_mob(c);
            }
        }
    }
    let cinders = w.mobs().iter().filter(|m| m.species == cinder_si).count();
    assert!(
        cinders >= 2,
        "the controller summoned its minions, got {cinders}"
    );
}

#[test]
fn phaser_blinks_toward_its_target() {
    let reg = e9_reg("phaser");
    let mut w = test_world_with("e9-phaser", reg.clone());
    let h = 80;
    pad(&mut w, &reg, 0, 40, 0, 40, h);
    let si = reg.animal_id("e9fauna:wispphantom").unwrap();
    let def = reg.animals[si].clone();
    let mut m = crate::mobs::Mob::new(si, glam::Vec3::new(8.5, h as f32 + 1.0, 8.5), 0.0);
    m.health = def.health;
    let player = glam::Vec3::new(8.5, h as f32 + 1.0, 14.5);
    let mut rng = 11u32;
    let mut events = Vec::new();
    let mut blinked = false;
    for _ in 0..180 {
        let before = m.pos;
        m.tick(&w, &def, &[ctx(player)], 1.0 / 60.0, &mut rng, &mut events);
        let d = before.local_delta_to(m.pos).length();
        if d > 0.5 {
            blinked = true;
            break;
        }
    }
    assert!(blinked, "the phaser blinked to reposition");
    let dist = m.pos.local_delta_to(ep(player)).length();
    assert!(dist < 3.0, "the phaser blinked into reach: {dist}");
}

#[test]
fn shield_bearer_blocks_front_and_is_vulnerable_from_behind() {
    let reg = e9_reg("shield");
    let si = reg.animal_id("e9fauna:huskwarden").unwrap();
    let def = reg.animals[si].clone();
    let mut front = crate::mobs::Mob::new(si, glam::Vec3::new(0.0, 80.0, 0.0), 0.0); // yaw 0 faces +v (north)
    front.health = 100.0;
    let mut back = crate::mobs::Mob::new(si, glam::Vec3::new(0.0, 80.0, 0.0), 0.0);
    back.health = 100.0;
    let from_front = ep(glam::Vec3::new(0.0, 80.0, 4.0));
    let from_back = ep(glam::Vec3::new(0.0, 80.0, -4.0));
    front.hurt(&def, 10.0, None, from_front);
    back.hurt(&def, 10.0, None, from_back);
    assert!(
        (front.health - (100.0 - 3.5)).abs() < 0.01,
        "a front hit was blocked: {}",
        front.health
    );
    assert!(
        (back.health - 90.0).abs() < 0.01,
        "a rear hit landed full: {}",
        back.health
    );
}
