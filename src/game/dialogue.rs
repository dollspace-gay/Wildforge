//! Dialogue screen logic (spec 3.2 runtime).

use crate::identity;
use super::Game;
use super::navigation::Screen;
use crate::registry::{DialogueChoice, ScriptHook};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

impl Game {
    /// Look up the dialogue def a mob-id NPC opens.
    fn dialogue_for(&self, mob_id: u32) -> Option<crate::registry::DialogueDef> {
        let reg = &self.content.reg;
        let def = self.runtime.view().npc_by_mob(mob_id)?;
        let npc = reg.npcs.get(def.def)?;
        let id = npc.dialogue.as_ref()?;
        reg.dialogues.iter().find(|d| &d.id == id).cloned()
    }

    /// A choice's hook is "visible" when its condition script returns true
    /// (or it has none). Conditions receive the mob id and node id as args.
    fn hook_visible(&mut self, hook: &Option<ScriptHook>, mob_id: u32, node_id: &str) -> bool {
        let Some(hook) = hook else {
            return true;
        };
        let ret = self.content.scripts.run_fn_view(
            &self.runtime.view(),
            hook,
            vec![mob_id.to_string(), node_id.to_string()],
        );
        ret.as_bool().unwrap_or(true)
    }

    /// A node's visible choices. Re-evaluated each frame so a callback that
    /// flips a flag prunes or reveals following choices immediately.
    pub(super) fn visible_choices(&mut self, mob_id: u32, node_id: &str) -> Vec<DialogueChoice> {
        let Some(dd) = self.dialogue_for(mob_id) else {
            return Vec::new();
        };
        let Some(node) = dd.nodes.iter().find(|n| n.id == *node_id) else {
            return Vec::new();
        };
        node.choices
            .iter()
            .filter(|c| self.hook_visible(&c.condition, mob_id, node_id))
            .cloned()
            .collect()
    }

    /// The on-screen text of a node: a `node_text` hook may return a
    /// replacement string; otherwise the authored text is used verbatim.
    pub(super) fn node_text(&mut self, mob_id: u32, node_id: &str) -> String {
        let Some(dd) = self.dialogue_for(mob_id) else {
            return String::new();
        };
        let Some(node) = dd.nodes.iter().find(|n| n.id == *node_id) else {
            return String::new();
        };
        let text = node.text.clone();
        let hook = ScriptHook::parse("node_text");
        let ret = self.content.scripts.run_fn_view(
            &self.runtime.view(),
            &hook,
            vec![mob_id.to_string(), node_id.to_string()],
        );
        let custom = ret.try_cast::<String>().filter(|s| !s.is_empty());
        custom.unwrap_or(text)
    }

    /// Select choice `sel` (0-based over the *visible* list): run its
    /// callback, then advance to `next` or close the dialogue.
    pub(super) fn dialog_select(&mut self, mob_id: u32, node_id: &str, sel: usize) {
        let choices = self.visible_choices(mob_id, node_id);
        let Some(choice) = choices.get(sel) else {
            return;
        };
        if let Some(hook) = &choice.callback {
            let _ = self.content.scripts.run_fn_view(
                &self.runtime.view(),
                hook,
                vec![mob_id.to_string(), node_id.to_string()],
            );
        }
        match &choice.next {
            Some(next) => {
                self.ui_state.screen = Screen::Dialog {
                    npc: mob_id,
                    node_id: next.clone(),
                    choice_sel: 0,
                };
            }
            None => self.set_screen(Screen::Playing),
        }
    }

    /// Clicked choice row index while the dialogue is open, if any.
    pub(super) fn dialog_choice_at_click(&mut self) -> Option<usize> {
        let Screen::Dialog { npc, node_id, .. } = &self.ui_state.screen else {
            return None;
        };
        let mob_id = *npc;
        let node_id = node_id.clone();
        let choices = self.visible_choices(mob_id, &node_id);
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        for (i, _) in choices.iter().enumerate() {
            let r = (
                w / 2.0 - 300.0,
                h / 2.0 + 60.0 + i as f32 * 40.0,
                600.0,
                32.0,
            );
            if self.hit(r) {
                return Some(i);
            }
        }
        None
    }

    // ---- Quest state (spec 3.3) ----
    //
    // Quest state lives in the existing per-mod KV, namespaced per player:
    //   "player_<uid>" -> { "quest_<id>" = "accepted"|"done",
    //                       "progress_<id>/<obj>" = "n" }
    // The player namespace rides the written-with-the-world KV, so it
    // survives saves, reloads, and (once the 11c rule lands) hot reloads.

    pub(super) fn player_namespace(&self) -> String {
        let mut id = [0u8; 16];
        let world_dir = self.runtime.player_sidecar_dir();
        if let Ok(p) = identity::local_player_id(&world_dir, self.identity.device_id()) {
            id = p.0;
        }
        format!("player_{}", hex(id))
    }

    pub(super) fn write_player_kv(&self, key: &str, value: String) {
        let ns = self.player_namespace();
        self.content
            .scripts
            .kv
            .borrow_mut()
            .entry(ns)
            .or_default()
            .insert(key.to_string(), value);
    }

    pub(super) fn read_player_kv(&self, key: &str) -> Option<String> {
        let ns = self.player_namespace();
        self.content
            .scripts
            .kv
            .borrow()
            .get(&ns)
            .and_then(|m| m.get(key))
            .cloned()
    }

    /// "accepted" | "done" | None
    pub(super) fn quest_state(&self, quest_id: &str) -> Option<String> {
        self.read_player_kv(&format!("quest_{quest_id}"))
    }

    /// Current completed count for one objective.
    pub(super) fn quest_progress(&self, quest_id: &str, key: &str) -> u32 {
        self.read_player_kv(&format!("progress_{quest_id}/{key}"))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }

    /// Accept a quest if its prereq is done. Returns Ok/Err for toast UX.
    pub(super) fn quest_accept(&self, quest_id: &str) -> Result<(), String> {
        let reg = &self.content.reg;
        let Some(q) = reg.quests.iter().find(|q| q.id == *quest_id) else {
            return Err(format!("no quest named {quest_id}"));
        };
        if let Some(prereq) = &q.prereq {
            let pr = self.quest_state(prereq).unwrap_or_default();
            if pr != "done" {
                return Err(format!("Complete \"{}\" first.", prereq));
            }
        }
        match self.quest_state(quest_id).unwrap_or_default().as_str() {
            "done" => Err(format!("\"{}\" is already complete.", q.title)),
            "accepted" => Err(format!("\"{}\" is already accepted.", q.title)),
            _ => {
                self.write_player_kv(&format!("quest_{quest_id}"), "accepted".into());
                Ok(())
            }
        }
    }

    /// Apply a queued progress increment: bump the objective, detect
    /// completion, set done, and pay rewards.
    pub(super) fn quest_progress_apply(&mut self, quest_id: &str, key: &str, n: u32) {
        let Some(q) = self
            .content
            .reg
            .quests
            .iter()
            .find(|q| q.id == *quest_id)
            .cloned()
        else {
            return;
        };
        if self.quest_state(quest_id).unwrap_or_default() != "accepted" {
            return;
        }
        let Some(objective) = q.objectives.iter().find(|o| o.key == *key) else {
            return;
        };
        let at = self
            .quest_progress(quest_id, key)
            .saturating_add(n)
            .min(objective.count);
        self.write_player_kv(&format!("progress_{quest_id}/{key}"), at.to_string());
        let complete = q
            .objectives
            .iter()
            .all(|o| self.quest_progress(quest_id, &o.key) >= o.count);
        if complete {
            self.write_player_kv(&format!("quest_{quest_id}"), "done".into());
            for reward in &q.rewards {
                match reward {
                    crate::registry::QuestReward::Give(item, count) => {
                        let reg = &self.content.reg;
                        let left = self.inventory.add(reg, *item, *count);
                        if left > 0 {
                            self.drop_stack(crate::inventory::ItemStack::new(reg, *item, left));
                        }
                    }
                    crate::registry::QuestReward::SetFlag(flag, value) => {
                        self.write_player_kv(flag, value.clone());
                    }
                    crate::registry::QuestReward::Reputation(settlement, amount) => {
                        // Spec 3.4: reputation is a numeric per-player KV under
                        // the settlement's `rep_key`. Crossing a tier threshold
                        // reveals that tier's world cells one-way.
                        apply_reputation_reward(
                            &self.content.scripts.kv,
                            &self.player_namespace(),
                            &self.content.reg,
                            &mut self.runtime.local_mut().world,
                            settlement,
                            *amount,
                        );
                    }
                    crate::registry::QuestReward::LearnRecipe(recipe_id) => {
                        apply_recipe_unlock_reward(
                            &self.content.scripts.kv,
                            &self.player_namespace(),
                            recipe_id,
                        );
                    }
                }
            }
        }
    }
}

fn hex(bytes: [u8; 16]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Apply an `add_reputation` quest reward (spec 3.4): increment the per-player
/// `rep_key` KV and reveal any settlement tiers crossed. Standalone so the
/// KV-write + reveal linkage is testable without a live `Game`.
pub(crate) fn apply_reputation_reward(
    kv: &Rc<RefCell<HashMap<String, HashMap<String, String>>>>,
    namespace: &str,
    reg: &crate::registry::Registry,
    world: &mut crate::world::World,
    settlement: &str,
    amount: u32,
) {
    let rep_key = reg
        .settlements
        .iter()
        .find(|s| s.id == settlement)
        .map(|s| s.rep_key.clone())
        .unwrap_or_else(|| format!("rep_{settlement}"));
    let at = kv
        .borrow()
        .get(namespace)
        .and_then(|m| m.get(&rep_key))
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0)
        .saturating_add(amount);
    kv.borrow_mut()
        .entry(namespace.to_string())
        .or_default()
        .insert(rep_key, at.to_string());
    if let Some(idx) = reg.settlement_id(settlement) {
        world.reveal_settlement(idx, at);
    }
}

/// Apply a `learn_recipe` quest reward (spec 3.5): write the recipe's runtime
/// tech key `learned:<recipe_id>` truthy in the player's KV namespace.
/// Standalone so the KV write is testable without a live `Game`.
pub(crate) fn apply_recipe_unlock_reward(
    kv: &Rc<RefCell<HashMap<String, HashMap<String, String>>>>,
    namespace: &str,
    recipe_id: &str,
) {
    kv.borrow_mut()
        .entry(namespace.to_string())
        .or_default()
        .insert(format!("learned:{recipe_id}"), "1".to_string());
}
