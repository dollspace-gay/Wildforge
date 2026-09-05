use super::{load, ScriptHook};
use std::path::Path;

#[test]
fn base_npc_synthesizes_a_companion_species() {
    let registry = load(Path::new("__no_npc_mods__"));
    assert!(registry.arcane_errors.is_empty());
    let npc_id = registry
        .npc_id("base:elder")
        .expect("base elder npc must load");
    let npc = &registry.npcs[npc_id];
    assert_eq!(npc.name, "base:elder");
    assert_eq!(npc.label, "Elder Rowan");
    assert_eq!(npc.talk_radius, 3.0);
    assert!(npc.dialogue.as_deref() == Some("base:elder"));
    // Companion species exists, is not wildlife, and points back.
    assert!(registry.is_npc_species(npc.species));
    let companion = &registry.animals[npc.species];
    assert_eq!(companion.npc, Some(npc_id));
    assert!(!companion.hostile);
    assert!(companion.biomes.is_empty(), "NPCs never spawn as wildlife");
    assert!(companion.drops.is_empty());
}

#[test]
fn base_dialogue_tree_parses_and_links_choices() {
    let registry = load(Path::new("__no_npc_mods__"));
    let d = registry
        .dialogues
        .iter()
        .find(|d| d.id == "base:elder")
        .expect("base elder dialogue must load");
    assert_eq!(d.root, "welcome");
    let root = d
        .nodes
        .iter()
        .find(|n| n.id == "welcome")
        .expect("root node exists");
    assert!(!root.text.is_empty());
    assert_eq!(root.choices.len(), 2);
    let ores = root
        .choices
        .iter()
        .find(|c| c.next.as_deref() == Some("ores"))
        .expect("ores choice links forward");
    assert_eq!(ores.label, "Ask about the ores");
    let ores_node = d
        .nodes
        .iter()
        .find(|n| n.id == "ores")
        .expect("ores node exists");
    let accept = ores_node
        .choices
        .iter()
        .find(|c| c.callback.is_some())
        .expect("accept choice runs a callback");
    assert_eq!(
        accept.callback.as_ref().unwrap(),
        &ScriptHook {
            mod_id: "base".into(),
            fn_name: "accept_cerium_quest".into(),
        }
    );
}

#[test]
fn base_quest_definitions_resolve_rewards() {
    let registry = load(Path::new("__no_npc_mods__"));
    let quest = registry
        .quests
        .iter()
        .find(|q| q.id == "base:elder_cerium")
        .expect("base elder_cerium quest must load");
    assert_eq!(quest.giver.as_deref(), Some("base:elder"));
    assert_eq!(quest.objectives.len(), 1);
    assert_eq!(quest.objectives[0].key, "cerium_shards");
    assert_eq!(quest.objectives[0].count, 6);
    assert_eq!(quest.rewards.len(), 2);
    let give = quest
        .rewards
        .iter()
        .find(|r| matches!(r, QuestReward::Give(..)))
        .expect("item reward present");
    if let QuestReward::Give(item, count) = give {
        assert_eq!(registry.item(*item).name, "base:amethyst_shard");
        assert_eq!(*count, 2);
    }
    assert!(quest
        .rewards
        .iter()
        .any(|r| matches!(r, QuestReward::SetFlag(flag, v) if flag == "elder_told_tales" && v == "true")));
}
