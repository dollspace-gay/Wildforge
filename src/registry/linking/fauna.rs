//! Resolve wildlife definitions and deferred prey links.

use super::arcane_def;
use super::lookups::lookup_item;
use super::pending::PendingAnimal;
use crate::registry::schema::ResistTomlList;
use crate::registry::{
    AnimalDef, AquaticHabitatDef, ArchetypeParams, AttackDef, AttackKind, BehaviorArchetype,
    BuilderDef, ControllerDef, HackDef, ModelBox, PhaserDef, ProjectileDef, Registry, RusherDef,
    ShieldDef, SniperDef, SupportDef, SwarmDef, TankDef, qualify,
};

pub(super) struct PendingPrey {
    hunter: usize,
    modid: String,
    names: Vec<String>,
}

pub(super) fn resolve(reg: &mut Registry, pending_animals: Vec<PendingAnimal>) -> Vec<PendingPrey> {
    let mut pending_prey: Vec<PendingPrey> = Vec::new();
    for PendingAnimal {
        modid,
        definition: a,
        tile,
        head_tile,
        box_tiles,
        proj_tile,
        attack_proj_tiles,
    } in pending_animals
    {
        let full = qualify(&modid, &a.id);
        if reg.animals.iter().any(|x| x.name == full) {
            continue; // duplicate id — first wins, like blocks/items
        }
        if !a.prey.is_empty() {
            pending_prey.push(PendingPrey {
                hunter: reg.animals.len(),
                modid: modid.clone(),
                names: a.prey.clone(),
            });
        }
        let drops = a
            .drops
            .iter()
            .filter_map(|d| {
                lookup_item(reg, &modid, &d.item)
                    .map(|i| (i, d.min.unwrap_or(1), d.max.unwrap_or(1)))
            })
            .collect();
        let mut model: Vec<ModelBox> = a
            .model
            .iter()
            .map(|(name, b)| ModelBox {
                name: name.clone(),
                size: b.size,
                at: b.at,
                tile: box_tiles.get(name).copied(),
            })
            .collect();
        if model.is_empty() {
            model = vec![
                ModelBox {
                    name: "body".into(),
                    size: [6.0, 6.0, 10.0],
                    at: [0.0, 7.0, 0.0],
                    tile: None,
                },
                ModelBox {
                    name: "head".into(),
                    size: [4.0, 4.0, 4.0],
                    at: [0.0, 11.0, -6.0],
                    tile: None,
                },
                ModelBox {
                    name: "leg".into(),
                    size: [2.0, 7.0, 2.0],
                    at: [2.0, 0.0, 3.0],
                    tile: None,
                },
            ];
        }
        model.sort_by(|a, b| a.name.cmp(&b.name));
        let mut half_w = 0.2f32;
        let mut height = 0.4f32;
        for b in &model {
            half_w = half_w
                .max((b.at[0].abs() + b.size[0] / 2.0) / 16.0)
                .max((b.at[2].abs() + b.size[2] / 2.0) / 16.0);
            height = height.max((b.at[1] + b.size[1]) / 16.0);
        }
        let winged = model.iter().any(|b| b.name.starts_with("wing"));
        let movement_swim = a.movement.as_deref() == Some("swim");
        let aquatic = movement_swim.then(|| {
            let mut habitat = AquaticHabitatDef::default();
            if let Some(configured) = &a.aquatic {
                habitat.temperature_c = configured.temperature_c.unwrap_or(habitat.temperature_c);
                habitat.depth_blocks = configured.depth_blocks.unwrap_or(habitat.depth_blocks);
                habitat.discharge = configured.discharge.unwrap_or(habitat.discharge);
                habitat.salinity = configured.salinity.unwrap_or(habitat.salinity);
            }
            habitat
        });
        let full = qualify(&modid, &a.id);
        let arcane = match arcane_def(a.arcane.as_ref(), &modid, &full, &reg.arcane_registry) {
            Ok(definition) => definition,
            Err(error) => {
                reg.arcane_errors.push(error);
                None
            }
        };
        reg.animals.push(AnimalDef {
            name: full,
            label: a.name.clone().unwrap_or_else(|| a.id.clone()),
            biomes: a.biomes.iter().map(|b| b.to_lowercase()).collect(),
            habitats: a.habitats.iter().map(|tag| tag.to_lowercase()).collect(),
            temperature_c: a.temperature_c,
            vegetation: a.vegetation,
            elevation: a.elevation,
            health: a.health.unwrap_or(8.0),
            speed: a.speed.unwrap_or(2.0),
            flee_range: a.flee_range.unwrap_or(6.0),
            group: a.group.unwrap_or([1, 2]),
            rarity: a.rarity.unwrap_or(6).max(1),
            tile,
            head_tile,
            sound_pitch: a.sound_pitch.unwrap_or(1.0),
            drops,
            model,
            half_w: half_w.min(0.45),
            height,
            hostile: a.hostile,
            attack: a.attack.unwrap_or(3.0),
            resistances: a
                .resist
                .as_ref()
                .map(ResistTomlList::resolved)
                .unwrap_or_default(),
            attacks: {
                let reach = half_w.min(0.45) + 0.9;
                let attack = a.attack.unwrap_or(3.0);
                let mut list: Vec<AttackDef> = Vec::with_capacity(a.attacks.len());
                for (atk, ptile) in a.attacks.iter().zip(&attack_proj_tiles) {
                    let kind = match atk.kind.as_str() {
                        "charge" => AttackKind::Charge,
                        "projectile" => AttackKind::Projectile,
                        _ => AttackKind::Melee,
                    };
                    list.push(AttackDef {
                        name: atk.name.clone().unwrap_or_else(|| atk.kind.clone()),
                        kind,
                        damage: atk.damage.unwrap_or(attack),
                        cooldown: atk.cooldown.unwrap_or(1.0),
                        range: atk.range.unwrap_or(match kind {
                            AttackKind::Projectile => 14.0,
                            _ => reach,
                        }),
                        damage_type: atk.damage_type.clone(),
                        projectile: atk.projectile.as_ref().map(|pr| ProjectileDef {
                            tile: ptile.unwrap_or(crate::atlas::UNKNOWN_SLOT),
                            damage: pr.damage,
                            damage_type: pr.damage_type.clone(),
                            speed: pr.speed.unwrap_or(14.0),
                            cooldown: pr.cooldown.unwrap_or(2.0),
                        }),
                    });
                }
                if list.is_empty() {
                    // Back-compat synthesis for the pre-spec 3.6 scalar
                    // fields. A legacy `projectile` becomes a ranged "cast"
                    // attack riding the projectile's own cooldown; every
                    // warden keeps the implicit melee `attack` scalar, so a
                    // caster still swings when the player closes in.
                    if let Some(pr) = a.projectile.as_ref() {
                        list.push(AttackDef {
                            name: "cast".into(),
                            kind: AttackKind::Projectile,
                            damage: pr.damage,
                            cooldown: pr.cooldown.unwrap_or(2.0),
                            range: 14.0,
                            damage_type: pr.damage_type.clone(),
                            projectile: Some(ProjectileDef {
                                tile: proj_tile.unwrap_or(crate::atlas::UNKNOWN_SLOT),
                                damage: pr.damage,
                                damage_type: pr.damage_type.clone(),
                                speed: pr.speed.unwrap_or(14.0),
                                cooldown: pr.cooldown.unwrap_or(2.0),
                            }),
                        });
                    }
                    list.push(AttackDef {
                        name: "melee".into(),
                        kind: AttackKind::Melee,
                        damage: attack,
                        cooldown: 1.0,
                        range: reach,
                        damage_type: None,
                        projectile: None,
                    });
                }
                list
            },
            behavior: match a.behavior.as_deref() {
                Some("brute") => BehaviorArchetype::Brute,
                Some("construct") => BehaviorArchetype::Construct,
                Some("builder") => BehaviorArchetype::Builder,
                Some("rusher") => BehaviorArchetype::Rusher,
                Some("tank") => BehaviorArchetype::Tank,
                Some("sniper") => BehaviorArchetype::Sniper,
                Some("support") => BehaviorArchetype::Support,
                Some("swarm") => BehaviorArchetype::Swarm,
                Some("controller") => BehaviorArchetype::Controller,
                Some("phaser") => BehaviorArchetype::Phaser,
                Some("shield_bearer") => BehaviorArchetype::ShieldBearer,
                _ => BehaviorArchetype::Standard,
            },
            archetype: ArchetypeParams {
                rusher: a.rusher.as_ref().map(|r| RusherDef {
                    rush_mult: r.rush_mult.unwrap_or(2.4),
                }),
                tank: a.tank.as_ref().map(|t| TankDef {
                    knockback_mult: t.knockback_mult.unwrap_or(0.25),
                }),
                sniper: a.sniper.as_ref().map(|s| SniperDef {
                    keep_min: s.keep_min.unwrap_or(9.0),
                    keep_max: s.keep_max.unwrap_or(16.0),
                }),
                support: a.support.as_ref().map(|s| SupportDef {
                    radius: s.radius.unwrap_or(10.0),
                    interval: s.interval.unwrap_or(6.0),
                    heal: s.heal.unwrap_or(2.0),
                }),
                swarm: a.swarm.as_ref().map(|s| SwarmDef {
                    spawn: qualify(&modid, s.spawn.as_deref().unwrap_or("")),
                    count: s.count.unwrap_or(3),
                }),
                controller: a.controller.as_ref().map(|c| ControllerDef {
                    spawn: qualify(&modid, c.spawn.as_deref().unwrap_or("")),
                    count: c.count.unwrap_or(2),
                    interval: c.interval.unwrap_or(12.0),
                    max: c.max.unwrap_or(6),
                }),
                phaser: a.phaser.as_ref().map(|p| PhaserDef {
                    blink_range: p.blink_range.unwrap_or(7.0),
                    blink_cd: p.blink_cd.unwrap_or(5.0),
                }),
                shield: a.shield.as_ref().map(|sh| ShieldDef {
                    front_mult: sh.front_mult.unwrap_or(0.35),
                    front_deg: sh.front_deg.unwrap_or(90.0),
                }),
            },
            builder: a.builder.as_ref().map(|b| BuilderDef {
                template: b.template.clone(),
                cap: b.cap.unwrap_or(8),
                interval: b.interval.unwrap_or(30.0),
            }),
            hack: a.hack.as_ref().map(|h| HackDef {
                tool: h.tool.clone(),
                drops: h
                    .drops
                    .iter()
                    .filter_map(|d| {
                        lookup_item(reg, &modid, &d.item)
                            .map(|i| (i, d.min.unwrap_or(1), d.max.unwrap_or(1)))
                    })
                    .collect(),
            }),
            aggro_range: a.aggro_range.unwrap_or(12.0),
            ire_min: a.ire_min.unwrap_or(0.0),
            movement_float: a.movement.as_deref() == Some("float"),
            movement_swim,
            aquatic,
            winged,
            emissive: a.emissive,
            glow: a.glow,
            spawn_light_max: a.spawn_light_max.unwrap_or(3),
            breed_food: a
                .breed_food
                .as_ref()
                .and_then(|f| lookup_item(reg, &modid, f)),
            carrier: a.carrier,
            vehicle: a.vehicle,
            belly_secs: a.belly.unwrap_or(0.0).max(0.0),
            grazes: a.grazes,
            prey: Vec::new(), // resolved after every species exists
            fierce: a.fierce,
            guards: a.guards,
            arcane,
            projectile: a.projectile.as_ref().map(|pr| ProjectileDef {
                tile: proj_tile.unwrap_or(crate::atlas::UNKNOWN_SLOT),
                damage: pr.damage,
                damage_type: pr.damage_type.clone(),
                speed: pr.speed.unwrap_or(14.0),
                cooldown: pr.cooldown.unwrap_or(2.0),
            }),
            npc: None,
        });
    }
    pending_prey
}

pub(super) fn prey(reg: &mut Registry, pending_prey: Vec<PendingPrey>) {
    // Prey lists resolve after the whole roster exists (a fox may be
    // declared before the rabbit it hunts).
    for PendingPrey {
        hunter,
        modid,
        names,
    } in pending_prey
    {
        let ids: Vec<usize> = names
            .iter()
            .filter_map(|n| {
                let q = qualify(&modid, n);
                reg.animal_id(&q).or_else(|| reg.animal_id(n))
            })
            .collect();
        reg.animals[hunter].prey = ids;
    }
}
