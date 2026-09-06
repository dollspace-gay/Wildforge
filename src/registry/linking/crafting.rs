//! Resolve shaped recipes after quest unlocks and aliases are available.

use super::lookups::lookup_item;
use crate::registry::schema::RecipeToml;
use crate::registry::{Ingredient, RecipeDef, Registry, qualify};

pub(super) fn resolve(
    reg: &mut Registry,
    pending_recipes: Vec<(String, RecipeToml)>,
) -> Vec<String> {
    let mut recipe_errors = Vec::new();
    // Recipes unlocked by a `learn_recipe` reward default their tech key to
    // `learned:<recipe_id>` (spec 3.5) when they don't declare an explicit
    // `tech`. The quests are parsed above, so their rewards are visible here.
    let learned_recipe_ids: std::collections::HashSet<&str> = reg
        .quests
        .iter()
        .flat_map(|q| &q.rewards)
        .filter_map(|reward| match reward {
            crate::registry::QuestReward::LearnRecipe(id) => Some(id.as_str()),
            _ => None,
        })
        .collect();
    for (modid, r) in pending_recipes {
        let h = r.pattern.len();
        let w = r
            .pattern
            .iter()
            .map(|s| s.chars().count())
            .max()
            .unwrap_or(0);
        if h == 0 || w == 0 || h > 3 || w > 3 {
            continue;
        }
        let mut pattern = vec![None; w * h];
        let mut ok = true;
        for (y, row) in r.pattern.iter().enumerate() {
            for (x, ch) in row.chars().enumerate() {
                if ch == '.' || ch == ' ' {
                    continue;
                }
                let key = ch.to_string();
                let Some(name) = r.keys.get(&key) else {
                    ok = false;
                    continue;
                };
                if let Some(tag) = name.strip_prefix('#') {
                    let tag_name = qualify(&modid, tag);
                    match reg.tags.get(&tag_name) {
                        Some(list) if !list.is_empty() => {
                            pattern[y * w + x] = Some(Ingredient::Any(list.clone()))
                        }
                        _ => ok = false,
                    }
                } else {
                    match lookup_item(reg, &modid, name) {
                        Some(i) => pattern[y * w + x] = Some(Ingredient::One(i)),
                        None => ok = false,
                    }
                }
            }
        }
        let Some(out) = lookup_item(reg, &modid, &r.output) else {
            continue;
        };
        let blueprint = match r.blueprint.as_deref() {
            Some(name) => match lookup_item(reg, &modid, name) {
                Some(item) => Some(item),
                None => {
                    recipe_errors.push(format!("{}: recipe blueprint {name} is unknown", r.output));
                    continue;
                }
            },
            None => None,
        };
        if ok {
            let tech = r.tech.clone().or_else(|| {
                let recipe_id = reg.item(out).name.clone();
                learned_recipe_ids
                    .contains(recipe_id.as_str())
                    .then(|| format!("learned:{recipe_id}"))
            });
            reg.recipes.push(RecipeDef {
                w,
                h,
                pattern,
                output: out,
                count: r.count.unwrap_or(1),
                station: r.station.clone(),
                loss: r.loss.clone(),
                byproducts: r
                    .byproducts
                    .iter()
                    .filter_map(|byproduct| {
                        lookup_item(reg, &modid, &byproduct.item)
                            .map(|item| (item, byproduct.count))
                    })
                    .collect(),
                tech,
                blueprint,
            });
        }
    }
    recipe_errors
}
