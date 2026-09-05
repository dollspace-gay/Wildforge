//! Resolve friendly NPCs and their non-wildlife companion species.

use std::collections::HashMap;
use crate::registry::{Registry, AnimalDef, NpcDef, ModelBox, BehaviorArchetype, ArchetypeParams, qualify};
use super::pending::PendingNpc;

pub(super) fn resolve(reg: &mut Registry, pending_npcs: Vec<PendingNpc>) {
    // Friendly NPCs (spec 3.1): each synthesizes a companion AnimalDef so
    // the whole mob pipeline (render, persist, network, raycast) treats it
    // as an ordinary species. The companion is non-hostile, never flees,
    // never tames, has no drops/belly/prey, and is never wildlife-spawned
    // (empty biomes). `AnimalDef.npc` points back to the NpcDef.
    for PendingNpc { modid, definition: n, tile, head, box_tiles } in pending_npcs {
        let full = qualify(&modid, &n.id);
        if reg.npcs.iter().any(|x| x.name == full) {
            continue; // duplicate id — first wins, like blocks/items
        }
        let species = reg.animals.len();
        let model: Vec<ModelBox> = n
            .model
            .iter()
            .map(|(name, b)| ModelBox {
                name: name.clone(),
                size: b.size,
                at: b.at,
                tile: box_tiles.get(name).copied(),
            })
            .collect();
        let (model, half_w, height) = if model.is_empty() {
            // Default humanoid silhouette: a head, torso, and legs.
            let m = vec![
                ModelBox {
                    name: "head".into(),
                    size: [6.0, 6.0, 6.0],
                    at: [0.0, 22.0, 0.0],
                    tile: None,
                },
                ModelBox {
                    name: "body".into(),
                    size: [8.0, 10.0, 4.0],
                    at: [0.0, 12.0, 0.0],
                    tile: None,
                },
                ModelBox {
                    name: "leg".into(),
                    size: [3.0, 10.0, 3.0],
                    at: [1.5, 2.0, 0.0],
                    tile: None,
                },
            ];
            let mut half_w = 0.2f32;
            let mut height = 0.4f32;
            for b in &m {
                half_w = half_w
                    .max((b.at[0].abs() + b.size[0] / 2.0) / 16.0)
                    .max((b.at[2].abs() + b.size[2] / 2.0) / 16.0);
                height = height.max((b.at[1] + b.size[1]) / 16.0);
            }
            (m, half_w.min(0.45), height)
        } else {
            let mut half_w = 0.2f32;
            let mut height = 0.4f32;
            for b in &model {
                half_w = half_w
                    .max((b.at[0].abs() + b.size[0] / 2.0) / 16.0)
                    .max((b.at[2].abs() + b.size[2] / 2.0) / 16.0);
                height = height.max((b.at[1] + b.size[1]) / 16.0);
            }
            (model, half_w.min(0.45), height)
        };
        reg.animals.push(AnimalDef {
            name: format!("{full}#npc"),
            label: n.name.clone().unwrap_or_else(|| n.id.clone()),
            biomes: Vec::new(),
            habitats: Vec::new(),
            temperature_c: None,
            vegetation: None,
            elevation: None,
            health: 1000.0, // effectively unkillable this phase
            speed: 1.6,
            flee_range: 0.0,
            group: [1, 1],
            rarity: 1_000_000,
            tile,
            head_tile: head,
            sound_pitch: n.sound_pitch.unwrap_or(1.0),
            drops: Vec::new(),
            model,
            half_w,
            height,
            hostile: false,
            attack: 0.0,
            resistances: HashMap::new(),
            attacks: Vec::new(),
            behavior: BehaviorArchetype::Standard,
            builder: None,
            hack: None,
            archetype: ArchetypeParams::default(),
            aggro_range: 0.0,
            ire_min: 0.0,
            movement_float: false,
            movement_swim: false,
            aquatic: None,
            winged: false,
            emissive: false,
            glow: None,
            spawn_light_max: 0,
            breed_food: None,
            carrier: false,
            vehicle: false,
            belly_secs: 0.0,
            grazes: false,
            prey: Vec::new(),
            fierce: false,
            guards: false,
            arcane: None,
            projectile: None,
            npc: Some(reg.npcs.len()),
        });
        reg.npcs.push(NpcDef {
            name: full.clone(),
            label: n.name.clone().unwrap_or_else(|| n.id.clone()),
            dialogue: n
                .dialogue
                .as_ref()
                .map(|d| qualify(&modid, d))
                .or_else(|| n.dialogue.as_ref().cloned()),
            species,
            talk_radius: n.talk_radius.unwrap_or(3.0),
            patrol: n.patrol.clone(),
            pause: n.pause.unwrap_or(2.0),
            sound_pitch: n.sound_pitch.unwrap_or(1.0),
        });
    }
}
