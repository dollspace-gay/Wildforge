//! NPC, dialogue, quest, settlement, and feature-gate definitions.

use super::{BlockId, ItemId};

/// A friendly scripted character, authored apart from wildlife (spec 3.1).
/// Carries no hostile/fauna fields; fixed position or patrol waypoints, a
/// talk radius, and a dialogue tree id.
#[derive(Clone, Debug)]
pub struct NpcDef {
    pub name: String, // "base:elder"
    pub label: String,
    /// Dialogue tree id this NPC enters when talked to (3.2).
    pub dialogue: Option<String>,
    /// Companion `AnimalDef` index (renders/persists this NPC as a Mob).
    /// The companion species is synthesized at load from the NPC model/tex.
    pub species: usize,
    /// Talk interaction range in blocks.
    pub talk_radius: f32,
    /// Patrol waypoints as `(du, dy, dv)` offsets from the spawn anchor;
    /// empty = stands fixed. The walk loops.
    pub patrol: Vec<[f32; 3]>,
    /// Seconds paused at each waypoint before setting off again.
    pub pause: f32,
    pub sound_pitch: f32,
}

/// One branching dialogue node (spec 3.2). `condition` gates the node's
/// availability; `choices` lead to further nodes or close the dialogue.
#[derive(Clone, Debug)]
pub struct DialogueDef {
    pub id: String,
    /// Optional npc the tree belongs to (info only; NPC defs reference trees
    /// by id, not the other way).
    pub npc: Option<String>,
    pub root: String,
    pub nodes: Vec<DialogueNode>,
}

/// A named hook into a mod script, evaluated through the existing dispatch
/// machinery (`ScriptHost::dispatch`). `""` mod = any defining mod.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptHook {
    pub mod_id: String,
    pub fn_name: String,
}

impl ScriptHook {
    /// Parse `"mod:fn"` or bare `"fn"` (any mod).
    pub(crate) fn parse(raw: &str) -> ScriptHook {
        match raw.split_once(':') {
            Some((mod_id, fn_name)) if !mod_id.is_empty() && !fn_name.is_empty() => ScriptHook {
                mod_id: mod_id.into(),
                fn_name: fn_name.into(),
            },
            _ => ScriptHook {
                mod_id: String::new(),
                fn_name: raw.into(),
            },
        }
    }
}

#[derive(Clone, Debug)]
pub struct DialogueNode {
    pub id: String,
    pub text: String,
    /// Script hook previously evaluated; `false` hides the node.
    pub condition: Option<ScriptHook>,
    pub choices: Vec<DialogueChoice>,
}

#[derive(Clone, Debug)]
pub struct DialogueChoice {
    pub label: String,
    /// Hook previously evaluated; `false` hides the choice.
    pub condition: Option<ScriptHook>,
    /// Hook run when the choice is selected (rewards, quest flags).
    pub callback: Option<ScriptHook>,
    /// Next node id; `None` closes the dialogue.
    pub next: Option<String>,
}

/// A tracked quest (spec 3.3). Definitions are data; objective/completion
/// state lives in the per-mod KV store (already save- and hot-reload-safe).
#[derive(Clone, Debug)]
pub struct QuestDef {
    pub id: String,
    pub title: String,
    pub description: String,
    /// Giving npc id (optional).
    pub giver: Option<String>,
    /// Quest that must be done first (optional).
    pub prereq: Option<String>,
    pub objectives: Vec<QuestObjective>,
    pub rewards: Vec<QuestReward>,
}

#[derive(Clone, Debug)]
pub struct QuestObjective {
    pub key: String,
    pub description: String,
    pub count: u32,
}

/// One growth tier of a settlement (spec 3.4): the tier number and the
/// reputation value that reveals it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SettlementTier {
    pub tier: u32,
    pub threshold: u32,
}

/// A settlement (spec 3.4): a piece assembly whose tier-tagged pieces are
/// all placed at worldgen, with tier-2+ pieces hidden (non-collidable, not
/// rendered, unbreakable) until the player's numeric reputation crosses the
/// tier's threshold. Reputation lives in the per-player KV under `rep_key`.
#[derive(Clone, Debug)]
pub struct SettlementDef {
    /// Qualified id (`mod:settlement`).
    pub id: String,
    /// Per-player KV key holding the numeric reputation.
    pub rep_key: String,
    /// Growth tiers, ascending, with increasing thresholds.
    pub tiers: Vec<SettlementTier>,
    /// What the settlement wants delivered (capability E13): item and
    /// reputation per unit. A depot bound to this settlement converts
    /// deliveries into reputation through this table.
    pub needs: Vec<SettlementNeed>,
}

/// One deliverable need (capability E13).
#[derive(Clone, Debug, PartialEq)]
pub struct SettlementNeed {
    pub item: ItemId,
    /// Reputation per unit delivered.
    pub rep_per_unit: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuestReward {
    Give(ItemId, u32),
    SetFlag(String, String),
    Reputation(String, u32),
    /// Unlocks the recipe whose output item id is the recipe id (spec 3.5):
    /// writes the `learned:<recipe_id>` KV key truthy at apply time.
    LearnRecipe(String),
}

/// A flag-gated feature (spec 2.5): a sealed block placed by a
/// `feature:<id>` assembly marker that stays locked until the player's KV
/// flag `flag` reads `value`, then breaks normally or is replaced by
/// `unlocked_block`. Definitions are data; the locked state is derived live
/// from the per-player KV each time the block is touched.
#[derive(Clone, Debug)]
pub struct GateDef {
    /// Qualified id referenced by `feature:<id>` markers.
    pub id: String,
    /// The sealed block placed at the marker.
    pub block: BlockId,
    /// Per-player KV key consulted to unlock (e.g. `elder_told_tales`).
    pub flag: String,
    /// KV value that unlocks the gate (default `"true"`).
    pub value: String,
    /// If set, the sealed block is replaced by this once unlocked (e.g. air
    /// to "open" a door). If `None`, the sealed block just becomes breakable.
    pub unlocked_block: Option<BlockId>,
    /// Locked-interaction toast.
    pub message: String,
    /// Whether the sealed block is unbreakable while locked (default true).
    pub unbreakable_when_locked: bool,
}
