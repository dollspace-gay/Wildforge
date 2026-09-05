//! Resolve optional skills, machines, nests, and screen capabilities.

use crate::registry::{Registry, NestDef, qualify};
use crate::registry::schema::RawMod;

pub(super) fn skills(reg: &mut Registry, raws: &[RawMod]) {
    // Capability E5: merge every mod's skill tree into the registry.
    // Failures surface as pack errors on the mods screen.
    let raw_skills: Vec<crate::skills::RawSkillToml> =
        raws.iter().filter_map(|raw| raw.skills.clone()).collect();
    match crate::skills::resolve(&raw_skills) {
        Ok(tree) => reg.skills = tree,
        Err(errors) => reg.material_errors.extend(errors),
    }
}

pub(super) fn machines(reg: &mut Registry, raws: &[RawMod]) -> Vec<String> {
    // Capability E7: merge every mod's machine kinds into the registry, in
    // declaration order (base first, so kind 0 is a base machine).
    // Failures surface as pack errors on the mods screen.
    let raw_machines: Vec<(String, crate::machines::RawMachineToml)> = raws
        .iter()
        .filter_map(|raw| {
            raw.machines
                .clone()
                .map(|machines| (raw.info.id.clone(), machines))
        })
        .collect();
    let mut machine_errors = Vec::new();
    match crate::machines::resolve(&raw_machines) {
        Ok(machines) => reg.machines = machines,
        Err(errors) => machine_errors.extend(errors),
    }

    machine_errors
}

pub(super) fn nests(reg: &mut Registry, raws: &[RawMod]) -> Vec<String> {
    // Capability E9: resolve mods' nest spawn-gates after the block and
    // species rosters exist. Each `[[nest]]` names a block (its marker) and
    // a species; both must resolve or the nest is dropped with an error.
    let mut nest_errors = Vec::new();
    for (modid, nests) in raws
        .iter()
        .filter_map(|raw| raw.nests.clone().map(|nests| (raw.info.id.clone(), nests)))
    {
        for nest in &nests.nest {
            let full = qualify(&modid, &nest.id);
            let block = qualify(&modid, &nest.block);
            let species = qualify(&modid, &nest.species);
            let Some(block) = reg.block_id(&block).or_else(|| reg.block_id(&nest.block)) else {
                nest_errors.push(format!("nest {full}: unknown block {}", nest.block));
                continue;
            };
            let Some(species) = reg
                .animal_id(&species)
                .or_else(|| reg.animal_id(&nest.species))
            else {
                nest_errors.push(format!("nest {full}: unknown species {}", nest.species));
                continue;
            };
            reg.nests.push(NestDef {
                id: full,
                block,
                species,
                radius: nest.radius.unwrap_or(24.0),
                interval: nest.interval.unwrap_or(8.0),
                cap: nest.cap.unwrap_or(4),
            });
        }
    }

    nest_errors
}

pub(super) fn screens(reg: &mut Registry, raws: &[RawMod]) {
    // Capability E11: merge every mod's screens into the registry, in
    // declaration order. Failures surface as pack errors on the mods
    // screen, collected locally because `validate_material_graph` rebuilds
    // `material_errors` from scratch.
    let raw_screens: Vec<(String, crate::screens::RawScreensToml)> = raws
        .iter()
        .filter_map(|raw| {
            raw.screens
                .clone()
                .map(|screens| (raw.info.id.clone(), screens))
        })
        .collect();
    match crate::screens::resolve(&raw_screens) {
        Ok(screens) => reg.screens = screens,
        Err(errors) => reg.material_errors.extend(errors),
    }
}
