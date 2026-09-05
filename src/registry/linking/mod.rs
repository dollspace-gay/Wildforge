//! Pure interpretation before registry publication.

mod material;
pub(super) use material::{inferred_material_class, salvage_def};
mod observation;
pub(super) use observation::{observation_def, discovery_item_def, discovery_fixture_def};
mod magic;
pub(super) use magic::{arcane_def, arcane_ecology_def};
mod pending;
mod register;
mod bootstrap;
mod shells;
mod lookups;
mod resources;
mod structures;
mod settlements;
mod stations;
mod fauna;
mod npcs;
mod narrative;
mod crafting;
mod features;
mod modes;
mod extensions;

use crate::registry::{Registry, ModInfo};
use crate::registry::schema::RawMod;
use crate::registry::material_graph::reconcile_material_definitions;

/// Build privately. Ordering is part of the content-ID and error-report contract.
pub(super) fn build(raws: Vec<RawMod>, mut failed: Vec<ModInfo>) -> Registry {
    let mut reg = bootstrap::empty();
    shells::resonances(&mut reg, &raws);
    shells::workings(&mut reg, &raws);
    shells::sites(&mut reg, &raws);
    bootstrap::air(&mut reg);
    let pending = register::register(&mut reg, &raws);
    bootstrap::unknown(&mut reg);

    resources::tags(&mut reg, pending.tags);
    resources::bonus(&mut reg, pending.bonus);
    resources::brush(&mut reg, pending.brush);
    structures::loot(&mut reg, pending.loots);
    structures::templates(&mut reg, pending.structs);
    structures::pieces(&mut reg, pending.pieces);
    structures::pools(&mut reg, pending.pools);
    structures::assemblies(&mut reg, pending.assemblies);
    let mut settlement_errors = settlements::resolve(&mut reg, pending.settlements);
    resources::drops(&mut reg, pending.drops);
    stations::smelts(&mut reg, pending.smelts);
    stations::bloomeries(&mut reg, pending.bloomeries);
    stations::worked(&mut reg, pending.workeds);
    stations::kilns(&mut reg, pending.kilns);
    stations::kiln_bases(&mut reg, pending.kiln_bases);
    stations::fuels(&mut reg, pending.fuels);
    let pending_prey = fauna::resolve(&mut reg, pending.animals);
    npcs::resolve(&mut reg, pending.npcs);
    narrative::dialogues(&mut reg, pending.dialogues);
    narrative::quests(&mut reg, pending.quests, &pending.recipes, &mut settlement_errors);
    fauna::prey(&mut reg, pending_prey);
    resources::harvests(&mut reg, pending.harvests);
    resources::aliases(&mut reg, pending.aliases);
    resources::places(&mut reg, pending.places);
    let recipe_errors = crafting::resolve(&mut reg, pending.recipes);
    resources::crop_drops(&mut reg);
    let gate_errors = features::resolve(&mut reg, pending.features);
    resources::creative_items(&mut reg);
    shells::preparations(&mut reg, &raws);
    let mode_errors = modes::resolve(&mut reg, &raws);
    extensions::skills(&mut reg, &raws);
    let machine_errors = extensions::machines(&mut reg, &raws);
    let nest_errors = extensions::nests(&mut reg, &raws);

    // Material reconciliation rebuilds its errors. Keep other stage diagnostics
    // separate until it finishes, in the existing reporting order.
    reconcile_material_definitions(&mut reg);
    reg.material_errors.extend(gate_errors);
    reg.material_errors.extend(settlement_errors);
    reg.material_errors.extend(recipe_errors);
    reg.material_errors.extend(mode_errors);
    reg.material_errors.extend(machine_errors);
    reg.material_errors.extend(nest_errors);
    extensions::screens(&mut reg, &raws);
    reg.mods.append(&mut failed);
    reg
}
