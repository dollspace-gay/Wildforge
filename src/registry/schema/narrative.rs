//! Raw narrative content schema; no runtime mutation.

use super::{AnimalToml, BoxToml};
use serde::Deserialize;
use std::collections::{HashMap};

#[derive(Deserialize, Default)]
pub(in crate::registry) struct AnimalsFile {
    #[serde(default)]
    pub(in crate::registry) animal: Vec<AnimalToml>,
}

#[derive(Deserialize, Default)]
pub(in crate::registry) struct NpcsFile {
    #[serde(default)]
    pub(in crate::registry) npc: Vec<NpcToml>,
}

#[derive(Deserialize, Default)]
pub(in crate::registry) struct DialogueFile {
    #[serde(default)]
    pub(in crate::registry) dialogue: Vec<DialogueToml>,
}

#[derive(Deserialize, Default)]
pub(in crate::registry) struct QuestsFile {
    #[serde(default)]
    pub(in crate::registry) quest: Vec<QuestToml>,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct NpcToml {
    pub(in crate::registry) id: String,
    #[serde(default)]
    pub(in crate::registry) name: Option<String>,
    pub(in crate::registry) tex: String,
    #[serde(default)]
    pub(in crate::registry) head_tex: Option<String>,
    #[serde(default)]
    pub(in crate::registry) dialogue: Option<String>,
    #[serde(default)]
    pub(in crate::registry) talk_radius: Option<f32>,
    #[serde(default)]
    pub(in crate::registry) patrol: Vec<[f32; 3]>,
    #[serde(default)]
    pub(in crate::registry) pause: Option<f32>,
    #[serde(default)]
    pub(in crate::registry) sound_pitch: Option<f32>,
    #[serde(default)]
    pub(in crate::registry) model: HashMap<String, BoxToml>,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct DialogueToml {
    pub(in crate::registry) id: String,
    #[serde(default)]
    pub(in crate::registry) npc: Option<String>,
    pub(in crate::registry) root: String,
    #[serde(default)]
    pub(in crate::registry) nodes: Vec<DialogueNodeToml>,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct DialogueNodeToml {
    pub(in crate::registry) id: String,
    pub(in crate::registry) text: String,
    #[serde(default)]
    pub(in crate::registry) condition: Option<String>,
    #[serde(default)]
    pub(in crate::registry) choices: Vec<DialogueChoiceToml>,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct DialogueChoiceToml {
    pub(in crate::registry) label: String,
    #[serde(default)]
    pub(in crate::registry) condition: Option<String>,
    #[serde(default)]
    pub(in crate::registry) callback: Option<String>,
    #[serde(default)]
    pub(in crate::registry) next: Option<String>,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct QuestToml {
    pub(in crate::registry) id: String,
    pub(in crate::registry) title: String,
    pub(in crate::registry) description: String,
    #[serde(default)]
    pub(in crate::registry) giver: Option<String>,
    #[serde(default)]
    pub(in crate::registry) prereq: Option<String>,
    #[serde(default)]
    pub(in crate::registry) objectives: Vec<QuestObjectiveToml>,
    #[serde(default)]
    pub(in crate::registry) rewards: Vec<QuestRewardToml>,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct QuestObjectiveToml {
    pub(in crate::registry) key: String,
    pub(in crate::registry) description: String,
    #[serde(default)]
    pub(in crate::registry) count: u32,
}

#[derive(Deserialize, Clone)]
pub(in crate::registry) struct QuestRewardToml {
    #[serde(default)]
    pub(in crate::registry) item: Option<String>,
    #[serde(default)]
    pub(in crate::registry) count: u32,
    #[serde(default)]
    pub(in crate::registry) set_flag: Option<String>,
    #[serde(default)]
    pub(in crate::registry) flag_value: Option<String>,
    #[serde(default)]
    pub(in crate::registry) add_reputation: Option<String>,
    #[serde(default)]
    pub(in crate::registry) rep_amount: u32,
    /// Recipe id whose `learned:<id>` tech key this reward sets truthy (spec
    /// 3.5). Resolved to the qualified output-item id, which is also the
    /// runtime recipe id.
    #[serde(default)]
    pub(in crate::registry) learn_recipe: Option<String>,
}

