//! Link dialogue nodes and quest rewards against the complete content roster.

use super::lookups::lookup_item;
use crate::registry::schema::{DialogueToml, QuestToml, RecipeToml};
use crate::registry::{
    DialogueChoice, DialogueDef, DialogueNode, QuestDef, QuestObjective, QuestReward, Registry,
    ScriptHook, qualify,
};

pub(super) fn dialogues(reg: &mut Registry, pending_dialogues: Vec<(String, DialogueToml)>) {
    // Dialogue and quest definitions resolve by name after every npc/item
    // exists; a bad reference drops the def and reports, never panics.
    for (modid, d) in pending_dialogues {
        let id = qualify(&modid, &d.id);
        if reg.dialogues.iter().any(|x| x.id == id) {
            continue;
        }
        reg.dialogues.push(DialogueDef {
            id,
            npc: d.npc.as_ref().map(|n| qualify(&modid, n)),
            root: d.root.clone(),
            nodes: d
                .nodes
                .iter()
                .map(|node| DialogueNode {
                    id: node.id.clone(),
                    text: node.text.clone(),
                    condition: node.condition.as_deref().map(ScriptHook::parse),
                    choices: node
                        .choices
                        .iter()
                        .map(|choice| DialogueChoice {
                            label: choice.label.clone(),
                            condition: choice.condition.as_deref().map(ScriptHook::parse),
                            callback: choice.callback.as_deref().map(ScriptHook::parse),
                            next: choice.next.clone(),
                        })
                        .collect(),
                })
                .collect(),
        });
    }
}

pub(super) fn quests(
    reg: &mut Registry,
    pending_quests: Vec<(String, QuestToml)>,
    pending_recipes: &[(String, RecipeToml)],
    settlement_errors: &mut Vec<String>,
) {
    for (modid, q) in pending_quests {
        let id = qualify(&modid, &q.id);
        if reg.quests.iter().any(|x| x.id == id) {
            continue;
        }
        let mut quest_errors: Vec<String> = Vec::new();
        let rewards: Vec<QuestReward> = q
            .rewards
            .iter()
            .filter_map(|reward| {
                if let Some(item) = &reward.item {
                    let iid = lookup_item(reg, &modid, item)?;
                    Some(QuestReward::Give(iid, reward.count.max(1)))
                } else if let Some(settlement) = &reward.add_reputation {
                    let settlement_id = qualify(&modid, settlement);
                    if !reg.settlements.iter().any(|s| s.id == settlement_id) {
                        quest_errors.push(format!(
                            "{id}: quest rewards reputation for unknown settlement {settlement_id}"
                        ));
                        return None;
                    }
                    Some(QuestReward::Reputation(
                        settlement_id,
                        reward.rep_amount.max(1),
                    ))
                } else if let Some(recipe_out) = &reward.learn_recipe {
                    let recipe_id = qualify(&modid, recipe_out);
                    // The recipe id is its output item's qualified id; the
                    // recipe must be declared somewhere in this load.
                    let declared = pending_recipes
                        .iter()
                        .any(|(m, r)| qualify(m, &r.output) == recipe_id);
                    if !declared {
                        quest_errors
                            .push(format!("{id}: quest unlocks unknown recipe {recipe_id}"));
                        return None;
                    }
                    Some(QuestReward::LearnRecipe(recipe_id))
                } else {
                    reward.set_flag.as_ref().map(|flag| {
                        QuestReward::SetFlag(
                            flag.clone(),
                            reward.flag_value.clone().unwrap_or_else(|| "1".into()),
                        )
                    })
                }
            })
            .collect();
        settlement_errors.extend(quest_errors);
        reg.quests.push(QuestDef {
            id: id.clone(),
            title: q.title.clone(),
            description: q.description.clone(),
            giver: q.giver.as_ref().map(|g| qualify(&modid, g)),
            prereq: q.prereq.as_ref().map(|p| qualify(&modid, p)),
            objectives: q
                .objectives
                .iter()
                .map(|objective| QuestObjective {
                    key: objective.key.clone(),
                    description: objective.description.clone(),
                    count: objective.count.max(1),
                })
                .collect(),
            rewards,
        });
    }
}
