//! Link ore generation and flag-gated block features.

use crate::registry::{Registry, OreFeature, VeinShape, RetrogenPolicy, GateDef, AIR, qualify};
use crate::registry::schema::FeatureToml;

pub(super) fn resolve(reg: &mut Registry, pending_features: Vec<(String, FeatureToml)>) -> Vec<String> {
    // Gate errors are collected locally: `validate_material_graph` rebuilds
    // `material_errors` from scratch at the end of build, so pushing straight
    // to it here would be wiped.
    let mut gate_errors = Vec::new();
    for (modid, f) in pending_features {
        if f.r#type == "ore" {
            let lookup_block = |name: &str| {
                reg.block_id(&qualify(&modid, name))
                    .or_else(|| reg.block_id(name))
            };
            let (Some(block), Some(replaces)) = (
                lookup_block(&f.block),
                lookup_block(f.replaces.as_deref().unwrap_or("base:stone")),
            ) else {
                continue;
            };
            let [y0, y1] = f.y_range.unwrap_or([4, 60]);
            reg.ores.push(OreFeature {
                block,
                replaces,
                vein_size: f.vein_size.unwrap_or(5).clamp(1, 32),
                per_chunk: f.per_chunk.unwrap_or(6).clamp(0, 64),
                y_min: y0,
                y_max: y1,
                shape: match f.shape.as_deref() {
                    Some("seam") => VeinShape::Seam,
                    Some("streak") => VeinShape::Streak,
                    _ => VeinShape::Walk,
                },
                chance: f.chance.unwrap_or(1.0).clamp(0.0, 1.0),
                resource_key: reg.block(block).name.clone(),
                mod_id: modid.clone(),
                retrogen: reg
                    .mods
                    .iter()
                    .find(|info| info.id == modid)
                    .and_then(|info| info.retrogen)
                    .unwrap_or(RetrogenPolicy::NoRetrogen),
            });
        } else if f.r#type == "gate" {
            // Spec 2.5: a sealed block placed by `feature:<id>` markers,
            // locked until the player's KV flag reads `value`. An unknown
            // block fails the pack load (a sealed wall you can never open is
            // a silent softlock, unlike an unknown ore that just never grows).
            let Some(gate_id) = f.id.as_deref() else {
                gate_errors.push(format!("{modid}: gate feature missing `id`"));
                continue;
            };
            let id = qualify(&modid, gate_id);
            if reg.gates.iter().any(|g| g.id == id) {
                gate_errors.push(format!("{id}: duplicate gate feature id"));
                continue;
            }
            let Some(flag) = f.flag.clone() else {
                gate_errors.push(format!(
                    "{id}: gate feature missing `flag` (the KV key it unlocks on)"
                ));
                continue;
            };
            let lookup_block = |name: &str| {
                reg.block_id(&qualify(&modid, name))
                    .or_else(|| reg.block_id(name))
            };
            let Some(block) = lookup_block(&f.block) else {
                gate_errors.push(format!(
                    "{id}: gate feature references unknown block {:?}",
                    f.block
                ));
                continue;
            };
            let unlocked_block = f
                .unlocked_block
                .as_deref()
                .and_then(lookup_block)
                .or_else(|| {
                    if f.unlocked_block.as_deref() == Some("base:air") {
                        Some(AIR)
                    } else {
                        None
                    }
                });
            if f.unlocked_block.is_some() && unlocked_block.is_none() {
                gate_errors.push(format!(
                    "{id}: gate feature references unknown unlocked_block {:?}",
                    f.unlocked_block.as_deref().unwrap_or_default()
                ));
                continue;
            }
            reg.gates.push(GateDef {
                id,
                block,
                flag,
                value: f.value.clone().unwrap_or_else(|| "true".into()),
                unlocked_block,
                message: f
                    .message
                    .clone()
                    .unwrap_or_else(|| "It's locked tight.".into()),
                unbreakable_when_locked: f.unbreakable_when_locked.unwrap_or(true),
            });
        }
    }
    // Reverse block -> gate map, built after every gate resolves so a shared
    // sealed block can back multiple gates (last wins; authors should use one
    // block per gate unless they deliberately share).
    for (index, gate) in reg.gates.iter().enumerate() {
        reg.gate_for_block.insert(gate.block, index);
    }

    gate_errors
}
