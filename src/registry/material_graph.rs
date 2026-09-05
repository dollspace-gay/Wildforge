//! Material fixed-point inference and exact transformation balance validation.

use super::{BlockId, Ingredient, ItemId, MaterialClass, MaterialVector, Registry, SalvageDef};
use super::salvage::register_salvage_content;
use super::arcane_validation::validate_arcane_graph;
use super::ecology_validation::{validate_arcane_ecology_graph, validate_dross_scar_graph};

fn add_materials(into: &mut MaterialVector, from: &MaterialVector, multiplier: u64) {
    for (material, units) in from {
        *into.entry(material.clone()).or_default() = into
            .get(material)
            .copied()
            .unwrap_or_default()
            .saturating_add(units.saturating_mul(multiplier));
    }
    into.retain(|_, units| *units != 0);
}

fn ingredient_materials(reg: &Registry, ingredient: &Ingredient) -> Option<MaterialVector> {
    match ingredient {
        Ingredient::One(item) => Some(reg.item(*item).materials.clone()),
        Ingredient::Any(items) => {
            let first = items
                .first()
                .map(|item| reg.item(*item).materials.clone())?;
            items
                .iter()
                .all(|item| reg.item(*item).materials == first)
                .then_some(first)
        }
    }
}

fn subtract_materials(total: &MaterialVector, sinks: &MaterialVector) -> Option<MaterialVector> {
    let mut left = total.clone();
    for (material, units) in sinks {
        let value = left.get_mut(material)?;
        *value = value.checked_sub(*units)?;
    }
    left.retain(|_, units| *units != 0);
    Some(left)
}

fn per_item_materials(total: &MaterialVector, count: u32) -> Option<MaterialVector> {
    let divisor = u64::from(count.max(1));
    total
        .iter()
        .map(|(material, units)| {
            units
                .is_multiple_of(divisor)
                .then(|| (material.clone(), units / divisor))
        })
        .collect()
}

fn set_materials_if_missing(reg: &mut Registry, item: ItemId, materials: MaterialVector) -> bool {
    if reg.item(item).materials_declared || reg.item(item).materials == materials {
        return false;
    }
    let definition = &mut reg.items[item.0 as usize];
    definition.materials = materials;
    if !matches!(
        definition.material_class,
        MaterialClass::Consumptive | MaterialClass::Exceptional
    ) {
        definition.material_class = MaterialClass::GeologicallyFinite;
    }
    true
}

/// Close material identity over all transformation graphs, then validate the
/// fixed point. Content authors annotate geological sources and intentional
/// sinks; ordinary components inherit exact constituents from their recipes.
pub(super) fn reconcile_material_definitions(reg: &mut Registry) {
    for _ in 0..reg.items.len().min(64) {
        let mut changed = false;
        for recipe in reg.recipes.clone() {
            let mut input = MaterialVector::new();
            let mut known = true;
            for ingredient in recipe.pattern.iter().flatten() {
                if let Some(vector) = ingredient_materials(reg, ingredient) {
                    add_materials(&mut input, &vector, 1);
                } else {
                    known = false;
                }
            }
            // A blueprint item is extra input consumed on craft (spec 3.5).
            if let Some(blueprint) = recipe.blueprint {
                add_materials(&mut input, &reg.item(blueprint).materials, 1);
            }
            if !known {
                continue;
            }
            let mut sinks = recipe.loss.clone();
            for (item, count) in &recipe.byproducts {
                add_materials(&mut sinks, &reg.item(*item).materials, u64::from(*count));
            }
            if let Some(remaining) = subtract_materials(&input, &sinks)
                && let Some(per_item) = per_item_materials(&remaining, recipe.count)
            {
                changed |= set_materials_if_missing(reg, recipe.output, per_item);
            }
        }
        for smelt in reg.smelts.clone() {
            let Some(input) = ingredient_materials(reg, &smelt.input) else {
                continue;
            };
            let mut sinks = smelt.loss.clone();
            if let Some((item, count)) = smelt.spit {
                add_materials(&mut sinks, &reg.item(item).materials, u64::from(count));
            }
            if let Some(output) = subtract_materials(&input, &sinks) {
                changed |= set_materials_if_missing(reg, smelt.output, output);
            }
        }
        for worked in reg.worked.clone() {
            let input = reg.item(worked.input).materials.clone();
            if let Some(remaining) = subtract_materials(&input, &worked.loss)
                && let Some(output) = per_item_materials(&remaining, worked.count)
            {
                changed |= set_materials_if_missing(reg, worked.output, output);
            }
        }
        for bloomery in reg.bloomery.clone() {
            let input = reg.item(bloomery.charge).materials.clone();
            changed |= set_materials_if_missing(reg, bloomery.bloom, input);
        }
        if !changed {
            break;
        }
    }

    // A block's held form and placed form are one material object. Ore blocks
    // without a held form inherit their exact drop vector.
    for block_index in 0..reg.blocks.len() {
        let block_id = BlockId(block_index as u16);
        let direct = reg
            .item_id(&reg.blocks[block_index].name)
            .map(|item| reg.item(item).materials.clone())
            .filter(|materials| !materials.is_empty());
        let dropped = reg.blocks[block_index]
            .drops
            .map(|(item, count)| {
                let mut materials = MaterialVector::new();
                add_materials(&mut materials, &reg.item(item).materials, u64::from(count));
                materials
            })
            .filter(|materials| !materials.is_empty());
        if let Some(materials) = direct.or(dropped) {
            reg.blocks[block_index].materials = materials;
            reg.blocks[block_index].material_class = MaterialClass::GeologicallyFinite;
        }
        // Creative-only state items are still classified and carry identity
        // if an operator places one into a survival world.
        for item in &mut reg.items {
            if item.places == Some(block_id) && item.materials.is_empty() {
                item.materials = reg.blocks[block_index].materials.clone();
                item.material_class = reg.blocks[block_index].material_class;
            }
        }
    }

    // The assembly bench changes three physical components into one composite
    // without using the ordinary crafting graph (the Wellglass owner must
    // survive). Give the completed lens exactly the frame + Echo Slate
    // constituents so the finite-material audit sees a relabel, not a sink.
    if let (Some(frame), Some(slate), Some(mount), Some(lens)) = (
        reg.item_id("base:tuning_lens_frame"),
        reg.item_id("base:echo_slate"),
        reg.item_id("base:tuning_lens_mount"),
        reg.item_id("base:tuning_lens"),
    ) {
        let mut materials = reg.item(frame).materials.clone();
        add_materials(&mut materials, &reg.item(slate).materials, 1);
        reg.items[mount.0 as usize].materials = materials.clone();
        reg.items[mount.0 as usize].materials_declared = true;
        reg.items[lens.0 as usize].materials = materials;
        reg.items[lens.0 as usize].materials_declared = true;
    }

    for item in &mut reg.items {
        if !item.materials.is_empty() && item.salvage.is_none() {
            // Food reuses the durability field as a freshness clock. It is a
            // consumable, not a metal object that belongs in a forge.
            if (item.durability > 0 && item.food.is_none()) || item.armor.is_some() {
                item.salvage = Some(SalvageDef {
                    station: "forge".into(),
                    recovery_permille: 900,
                });
            } else if item.places.is_some() {
                item.salvage = Some(SalvageDef {
                    station: "dismantling".into(),
                    recovery_permille: 950,
                });
            }
        }
    }

    register_salvage_content(reg);
    validate_material_graph(reg);
    validate_arcane_graph(reg);
    validate_arcane_ecology_graph(reg);
    validate_dross_scar_graph(reg);
}

fn validate_material_graph(reg: &mut Registry) {
    let mut errors = Vec::new();
    for (index, recipe) in reg.recipes.iter().enumerate() {
        let mut input = MaterialVector::new();
        let mut valid_tags = true;
        for ingredient in recipe.pattern.iter().flatten() {
            if let Some(vector) = ingredient_materials(reg, ingredient) {
                add_materials(&mut input, &vector, 1);
            } else {
                valid_tags = false;
            }
        }
        // A blueprint item is extra input consumed on craft (spec 3.5).
        if let Some(blueprint) = recipe.blueprint {
            add_materials(&mut input, &reg.item(blueprint).materials, 1);
        }
        if !valid_tags {
            errors.push(format!(
                "recipe {index}: tag members have unequal material mass"
            ));
            continue;
        }
        let mut accounted = recipe.loss.clone();
        add_materials(
            &mut accounted,
            &reg.item(recipe.output).materials,
            u64::from(recipe.count),
        );
        for (item, count) in &recipe.byproducts {
            add_materials(
                &mut accounted,
                &reg.item(*item).materials,
                u64::from(*count),
            );
        }
        if input != accounted {
            errors.push(format!(
                "recipe {index} -> {} is not material-balanced: input {input:?}, accounted {accounted:?}",
                reg.item(recipe.output).name
            ));
        }
    }
    for (index, smelt) in reg.smelts.iter().enumerate() {
        let Some(input) = ingredient_materials(reg, &smelt.input) else {
            errors.push(format!(
                "smelt {index}: tag members have unequal material mass"
            ));
            continue;
        };
        let mut accounted = smelt.loss.clone();
        add_materials(&mut accounted, &reg.item(smelt.output).materials, 1);
        if let Some((item, count)) = smelt.spit {
            add_materials(&mut accounted, &reg.item(item).materials, u64::from(count));
        }
        if input != accounted {
            errors.push(format!(
                "smelt {index} -> {} is not material-balanced: input {input:?}, accounted {accounted:?}",
                reg.item(smelt.output).name
            ));
        }
    }
    for (index, worked) in reg.worked.iter().enumerate() {
        let input = reg.item(worked.input).materials.clone();
        let mut accounted = worked.loss.clone();
        add_materials(
            &mut accounted,
            &reg.item(worked.output).materials,
            u64::from(worked.count),
        );
        if input != accounted {
            errors.push(format!(
                "worked {index} -> {} is not material-balanced: input {input:?}, accounted {accounted:?}",
                reg.item(worked.output).name
            ));
        }
    }
    for (index, kiln) in reg.kiln.iter().enumerate() {
        if !reg.item(kiln.powder).materials.is_empty() && !kiln.consumes {
            errors.push(format!(
                "kiln {index} consumes finite {} without consumes = true",
                reg.item(kiln.powder).name
            ));
        }
    }
    for (index, bloomery) in reg.bloomery.iter().enumerate() {
        let input = &reg.item(bloomery.charge).materials;
        let output = &reg.item(bloomery.bloom).materials;
        if input != output {
            errors.push(format!(
                "bloomery {index} changes charge identity: {input:?} -> {output:?}"
            ));
        }
    }
    reg.material_errors = errors;
}

