//! Transformation graphs cannot mint unbacked Current owners.

use super::{Ingredient, ItemId, Registry};

/// Transformation stations currently conserve or destroy an input item's
/// Current, but they do not have authority to mint a newly charged owner.
/// Reject content graphs that would therefore produce an unbacked magical
/// item. Natural discoveries and creature drops are bound at their world
/// source instead and are intentionally outside this graph.
pub(super) fn validate_arcane_graph(reg: &mut Registry) {
    let mut errors = Vec::new();
    {
        let mut charged_output = |kind: &str, index: usize, item: ItemId| {
            if reg.item(item).arcane.is_some() && reg.item(item).implement.is_none() {
                errors.push(format!(
                    "{kind} {index} has charged output {}; transformations cannot create Current",
                    reg.item(item).name
                ));
            }
        };
        for (index, recipe) in reg.recipes.iter().enumerate() {
            let output = reg.item(recipe.output);
            let identity_preserving_lens_assembly = recipe.station.as_deref()
                == Some("lens_assembly_bench")
                && output
                    .discovery
                    .as_ref()
                    .is_some_and(|definition| definition.kind == "tuning_lens")
                && output.arcane.as_ref().is_some_and(|output_arcane| {
                    recipe.pattern.iter().flatten().any(|ingredient| {
                        let items: &[ItemId] = match ingredient {
                            Ingredient::One(item) => std::slice::from_ref(item),
                            Ingredient::Any(items) => items,
                        };
                        items
                            .iter()
                            .any(|item| reg.item(*item).arcane.as_ref() == Some(output_arcane))
                    })
                });
            if !identity_preserving_lens_assembly {
                charged_output("recipe", index, recipe.output);
            }
            for (item, _) in &recipe.byproducts {
                charged_output("recipe byproduct", index, *item);
            }
        }
        for (index, smelt) in reg.smelts.iter().enumerate() {
            charged_output("smelt", index, smelt.output);
            if let Some((item, _)) = smelt.spit {
                charged_output("smelt byproduct", index, item);
            }
        }
        for (index, worked) in reg.worked.iter().enumerate() {
            charged_output("worked recipe", index, worked.output);
        }
        for (index, kiln) in reg.kiln.iter().enumerate() {
            charged_output("kiln recipe", index, kiln.glass);
        }
        if let Some((_, _, output)) = reg.kiln_base {
            charged_output("kiln base", 0, output);
        }
        for (index, bloomery) in reg.bloomery.iter().enumerate() {
            charged_output("bloomery recipe", index, bloomery.bloom);
        }
        for (index, salvage) in reg.forge_salvage.iter().enumerate() {
            charged_output("forge salvage", index, salvage.output);
            charged_output("forge salvage byproduct", index, salvage.byproduct);
        }
    }
    let mut unsupported_input = |kind: &str, index: usize, item: ItemId| {
        if reg.item(item).arcane.is_some() {
            errors.push(format!(
                "{kind} {index} consumes charged input {}; that station has no Current transaction",
                reg.item(item).name
            ));
        }
    };
    for (index, worked) in reg.worked.iter().enumerate() {
        unsupported_input("worked recipe", index, worked.input);
    }
    for (index, kiln) in reg.kiln.iter().enumerate() {
        unsupported_input("kiln recipe", index, kiln.powder);
    }
    if let Some((sand, fuel, _)) = reg.kiln_base {
        unsupported_input("kiln base sand", 0, sand);
        unsupported_input("kiln base fuel", 0, fuel);
    }
    for (index, bloomery) in reg.bloomery.iter().enumerate() {
        unsupported_input("bloomery charge", index, bloomery.charge);
        unsupported_input("bloomery fuel", index, bloomery.fuel);
    }
    for (index, salvage) in reg.forge_salvage.iter().enumerate() {
        unsupported_input("forge salvage", index, salvage.input);
    }
    reg.arcane_errors.extend(errors);
}
