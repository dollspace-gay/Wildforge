//! Link furnace, forge, kiln, anvil, and fuel definitions.

use crate::registry::{Registry, SmeltDef, BloomeryDef, WorkedDef, KilnDef};
use crate::registry::schema::{SmeltToml, BloomeryToml, WorkedToml, KilnToml, KilnBaseToml, FuelToml};
use super::lookups::{lookup_item, resolve_ing};

pub(super) fn smelts(reg: &mut Registry, pending_smelts: Vec<(String, SmeltToml)>) {
    for (modid, s) in pending_smelts {
        if let (Some(input), Some(output)) = (
            resolve_ing(reg, &modid, &s.input),
            lookup_item(reg, &modid, &s.output),
        ) {
            let spit = s.spit.as_ref().and_then(|sp| {
                lookup_item(reg, &modid, &sp.item)
                    .map(|it| (it, sp.count.unwrap_or(1).clamp(1, 16)))
            });
            reg.smelts.push(SmeltDef {
                input,
                output,
                time: s.time.unwrap_or(8.0),
                spit,
                loss: s.loss.clone(),
            });
        }
    }
}

pub(super) fn bloomeries(reg: &mut Registry, pending_bloomeries: Vec<(String, BloomeryToml)>) {
    for (modid, b) in pending_bloomeries {
        if let (Some(charge), Some(fuel), Some(bloom)) = (
            lookup_item(reg, &modid, &b.charge),
            lookup_item(reg, &modid, &b.fuel),
            lookup_item(reg, &modid, &b.bloom),
        ) {
            reg.bloomery.push(BloomeryDef {
                charge,
                fuel,
                bloom,
            });
        }
    }
}

pub(super) fn worked(reg: &mut Registry, pending_workeds: Vec<(String, WorkedToml)>) {
    for (modid, w) in pending_workeds {
        if let (Some(input), Some(output)) = (
            lookup_item(reg, &modid, &w.input),
            lookup_item(reg, &modid, &w.output),
        ) {
            reg.worked.push(WorkedDef {
                input,
                output,
                strikes: w.strikes.unwrap_or(3).max(1),
                station: w.station.clone().unwrap_or_else(|| "anvil".into()),
                needs_hammer: w.tool.as_deref().unwrap_or("hammer") == "hammer",
                count: w.count.unwrap_or(1).max(1),
                loss: w.loss.clone(),
            });
        }
    }
}

pub(super) fn kilns(reg: &mut Registry, pending_kilns: Vec<(String, KilnToml)>) {
    for (modid, k) in pending_kilns {
        if let (Some(p), Some(g)) = (
            lookup_item(reg, &modid, &k.powder),
            lookup_item(reg, &modid, &k.glass),
        ) {
            reg.kiln.push(KilnDef {
                powder: p,
                glass: g,
                consumes: k.consumes,
            });
        }
    }
}

pub(super) fn kiln_bases(reg: &mut Registry, pending_kiln_bases: Vec<(String, KilnBaseToml)>) {
    for (modid, k) in pending_kiln_bases {
        if let (Some(sa), Some(fu), Some(cl)) = (
            lookup_item(reg, &modid, &k.sand),
            lookup_item(reg, &modid, &k.fuel),
            lookup_item(reg, &modid, &k.clear),
        ) {
            reg.kiln_base = Some((sa, fu, cl));
        }
    }
}

pub(super) fn fuels(reg: &mut Registry, pending_fuels: Vec<(String, FuelToml)>) {
    for (modid, f) in pending_fuels {
        if let Some(ing) = resolve_ing(reg, &modid, &f.item) {
            reg.fuels.push((ing, f.burn, f.speed.unwrap_or(1.0)));
        }
    }
}
