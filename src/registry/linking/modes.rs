//! Resolve named survival modes and diagnose inheritance cycles.

use crate::registry::schema::RawMod;
use crate::registry::{ModeDef, Registry, qualify};

pub(super) fn resolve(reg: &mut Registry, raws: &[RawMod]) -> Vec<String> {
    // Named rulesets (capability E1): resolve `[[mode]]` base chains so a
    // world's `mode` string maps to a Ruleset via `ruleset_for`. A mode
    // whose base is undeclared or cyclic is recorded as a load error and
    // falls back to survival semantics.
    let mut mode_errors = Vec::new();
    {
        let mut pending: Vec<ModeDef> = Vec::new();
        for raw in raws {
            for m in &raw.modes {
                let id = qualify(&raw.info.id, &m.id);
                if m.id == "survival" || m.id == "creative" {
                    mode_errors.push(format!("mode {id}: built-in mode id is reserved"));
                    continue;
                }
                if pending.iter().any(|p| p.id == id) {
                    mode_errors.push(format!("mode {id}: duplicate mode id"));
                    continue;
                }
                let base = m.base.as_deref().map(|b| {
                    if b == "survival" || b == "creative" {
                        b.to_string()
                    } else {
                        qualify(&raw.info.id, b)
                    }
                });
                pending.push(ModeDef {
                    id,
                    base,
                    creative: m.creative,
                    hunger: m.hunger,
                    fall_damage: m.fall_damage,
                    drowning: m.drowning,
                    lava_burn: m.lava_burn,
                    hostile_spawns: m.hostile_spawns,
                    ire: m.ire,
                    hearts: m.hearts,
                    weather_extremes: m.weather_extremes,
                    pvp: m.pvp,
                    skills: m.skills,
                    equipment: m.equipment,
                    industrial_ire: m.industrial_ire,
                    nest_spawns: m.nest_spawns,
                });
            }
        }
        for mode in &pending {
            let mut base = mode.base.clone().unwrap_or_else(|| "survival".into());
            let mut chain = vec![mode.id.clone()];
            // Chase the base chain to its root, cycle-guarded.
            while base != "survival" && base != "creative" {
                let Some(next) = pending.iter().find(|p| p.id == base) else {
                    mode_errors.push(format!(
                        "mode {}: base {base} is not a declared mode",
                        mode.id
                    ));
                    break;
                };
                if chain.contains(&next.id) {
                    mode_errors.push(format!(
                        "mode {}: cyclic base chain through {}",
                        mode.id, next.id
                    ));
                    break;
                }
                chain.push(next.id.clone());
                base = next.base.clone().unwrap_or_else(|| "survival".into());
            }
        }
        let modes = pending;
        reg.modes = modes;
    }

    mode_errors
}
