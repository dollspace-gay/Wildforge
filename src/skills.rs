//! Data-driven skill tree system (belt-quest capability E5).
//!
//! Mods declare `skills.toml` with branches and nodes (point cost, tier,
//! effects as E4 `StatModifier`s). XP accrues from fixed engine source ids
//! (`mine`, `build`, `craft`, `smelt`, `kill`, `fish`, `harvest`) with
//! per-source diminishing returns; leveling grants skill points. Effects
//! fold into the E4 `StatBlock`. The capability is mode-gated (E1), so
//! Survival/Creative worlds are unchanged without a `skills = true` mode.

use serde::{Deserialize, Serialize};

use crate::stats::{StatBlock, StatModifier};

/// A node of tier N requires this many allocated tier-(N-1) nodes in the
/// same branch before it can be bought (belts phase 5 rule). A branch may
/// override it with `tier_gate`.
pub const DEFAULT_TIER_GATE: u32 = 3;

/// The canonical engine XP sources. Content declares `base`/`decay` for the
/// ones it wants; an undeclared source grants nothing.
pub const XP_SOURCES: [&str; 7] = ["mine", "build", "craft", "smelt", "kill", "fish", "harvest"];

fn qualify(mod_id: &str, id: &str) -> String {
    if id.contains(':') {
        id.to_string()
    } else {
        format!("{mod_id}:{id}")
    }
}

// ---------------- content schema ----------------

#[derive(Deserialize, Clone)]
pub struct RawSkillToml {
    pub schema_version: u32,
    #[serde(default)]
    pub tree: RawTreeToml,
    #[serde(default)]
    pub xp_source: Vec<RawXpSourceToml>,
    #[serde(default)]
    pub branch: Vec<RawBranchToml>,
}

#[derive(Deserialize, Clone)]
pub struct RawTreeToml {
    #[serde(default = "default_max_level")]
    pub max_level: u32,
    #[serde(default = "default_points_per_level")]
    pub points_per_level: u32,
    #[serde(default = "default_xp_base")]
    pub xp_base: f64,
    #[serde(default = "default_xp_exponent")]
    pub xp_exponent: f32,
}

fn default_max_level() -> u32 {
    80
}
fn default_points_per_level() -> u32 {
    1
}
fn default_xp_base() -> f64 {
    100.0
}
fn default_xp_exponent() -> f32 {
    1.5
}

impl Default for RawTreeToml {
    fn default() -> Self {
        RawTreeToml {
            max_level: default_max_level(),
            points_per_level: default_points_per_level(),
            xp_base: default_xp_base(),
            xp_exponent: default_xp_exponent(),
        }
    }
}

#[derive(Deserialize, Clone)]
pub struct RawXpSourceToml {
    pub id: String,
    pub base: f64,
    #[serde(default)]
    pub decay: f64,
}

#[derive(Deserialize, Clone)]
pub struct RawBranchToml {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub tier_gate: Option<u32>,
    #[serde(default)]
    pub node: Vec<RawNodeToml>,
}

#[derive(Deserialize, Clone)]
pub struct RawNodeToml {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub tier: u8,
    #[serde(default)]
    pub cost: Option<u32>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub stats: Vec<StatModifier>,
}

/// Parse one mod's `skills.toml`, qualifying bare ids with the mod id.
/// XP source ids stay engine-global (`mine`, `build`, ...) — they are the
/// closed set of hook points, not content namespaces.
pub fn parse_skills(text: &str, mod_id: &str) -> Result<RawSkillToml, String> {
    let mut raw: RawSkillToml = toml::from_str(text).map_err(|e| format!("skills.toml: {e}"))?;
    if raw.schema_version != 1 {
        return Err("skills.toml: schema_version must be 1".into());
    }
    for branch in &mut raw.branch {
        branch.id = qualify(mod_id, &branch.id);
        if branch.name.is_empty() {
            branch.name = branch.id.clone();
        }
        for node in &mut branch.node {
            node.id = qualify(mod_id, &node.id);
            if node.name.is_empty() {
                node.name = node.id.clone();
            }
        }
    }
    Ok(raw)
}

// ---------------- resolved model ----------------

/// The resolved skill tree for a registry: everything mod content declared,
/// validated and id-qualified. Empty by default (base ships no skills).
#[derive(Clone, Debug)]
pub struct SkillTree {
    pub max_level: u32,
    pub points_per_level: u32,
    pub xp_base: f64,
    pub xp_exponent: f32,
    pub branches: Vec<SkillBranch>,
    pub nodes: Vec<SkillNodeDef>,
    pub xp_sources: std::collections::BTreeMap<String, SkillXpSource>,
}

impl Default for SkillTree {
    fn default() -> Self {
        SkillTree {
            max_level: default_max_level(),
            points_per_level: default_points_per_level(),
            xp_base: default_xp_base(),
            xp_exponent: default_xp_exponent(),
            branches: Vec::new(),
            nodes: Vec::new(),
            xp_sources: std::collections::BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SkillBranch {
    pub id: String,
    pub name: String,
    /// Allocated tier-(N-1) nodes required to buy a tier-N node.
    pub tier_gate: u32,
}

#[derive(Clone, Debug)]
pub struct SkillNodeDef {
    pub id: String,
    pub name: String,
    pub branch: String,
    pub tier: u8,
    pub cost: u32,
    pub description: String,
    pub stats: Vec<StatModifier>,
}

#[derive(Clone, Debug)]
pub struct SkillXpSource {
    pub id: String,
    pub base: f64,
    /// Diminishing returns: the k-th grant of this source is worth
    /// `base / (1 + decay * k)`.
    pub decay: f64,
}

/// Merge every mod's declarations into one tree. Tree-level knobs are
/// last-wins across mods; duplicate node ids and bad tiers are errors.
pub fn resolve(raws: &[RawSkillToml]) -> Result<SkillTree, Vec<String>> {
    let mut tree = SkillTree::default();
    let mut errors = Vec::new();
    for raw in raws {
        tree.max_level = raw.tree.max_level.max(1);
        tree.points_per_level = raw.tree.points_per_level.max(1);
        tree.xp_base = raw.tree.xp_base.max(0.0);
        tree.xp_exponent = raw.tree.xp_exponent.max(0.0);
        for src in &raw.xp_source {
            if tree.xp_sources.contains_key(&src.id) {
                // XP sources are the closed engine hook points, not content
                // namespaces: two mods tuning `kill` is legitimate, and the
                // first declaration in dependency order wins (the same
                // first-wins convention branches and machines use). A hard
                // error here made any two skill mods mutually exclusive —
                // and the resulting default tree silently dropped every
                // branch behind it.
                continue;
            }
            tree.xp_sources.insert(
                src.id.clone(),
                SkillXpSource {
                    id: src.id.clone(),
                    base: src.base.max(0.0),
                    decay: src.decay.max(0.0),
                },
            );
        }
        let mut branch_ids = std::collections::HashSet::new();
        for branch in &raw.branch {
            if !branch_ids.insert(branch.id.clone()) {
                errors.push(format!("skills.toml: duplicate branch {}", branch.id));
            }
            tree.branches.push(SkillBranch {
                id: branch.id.clone(),
                name: branch.name.clone(),
                tier_gate: branch.tier_gate.unwrap_or(DEFAULT_TIER_GATE).max(1),
            });
            for node in &branch.node {
                if tree.nodes.iter().any(|n| n.id == node.id) {
                    errors.push(format!("skills.toml: duplicate node {}", node.id));
                    continue;
                }
                if node.tier == 0 || node.tier > 4 {
                    errors.push(format!("skills.toml: node {} tier must be 1..=4", node.id));
                }
                tree.nodes.push(SkillNodeDef {
                    id: node.id.clone(),
                    name: node.name.clone(),
                    branch: branch.id.clone(),
                    tier: node.tier,
                    cost: node.cost.unwrap_or_else(|| u32::from(node.tier)).max(1),
                    description: node.description.clone(),
                    stats: node.stats.clone(),
                });
            }
        }
    }
    if errors.is_empty() {
        Ok(tree)
    } else {
        Err(errors)
    }
}

impl SkillTree {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty() && self.xp_sources.is_empty()
    }

    pub fn node(&self, id: &str) -> Option<&SkillNodeDef> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn branch(&self, id: &str) -> Option<&SkillBranch> {
        self.branches.iter().find(|b| b.id == id)
    }

    /// XP required to advance from `level` to `level + 1`.
    pub fn xp_for_level(&self, level: u32) -> f64 {
        (self.xp_base * f64::from(level).powf(self.xp_exponent as f64))
            .round()
            .max(1.0)
    }

    /// Allocated nodes in `branch` of exactly `tier` (the tier-gate count).
    pub fn allocated_in_branch_tier(&self, state: &SkillState, branch: &str, tier: u8) -> u32 {
        state
            .allocated
            .iter()
            .filter(|id| {
                self.node(id)
                    .is_some_and(|n| n.branch == branch && n.tier == tier)
            })
            .count() as u32
    }

    /// Whether a node can be bought: unallocated, affordable, and (for
    /// tier > 1) the branch's previous tier is filled up to `tier_gate`.
    pub fn unlockable(&self, state: &SkillState, node: &SkillNodeDef) -> bool {
        if state.allocated.iter().any(|a| a == &node.id) {
            return false;
        }
        if state.points < node.cost {
            return false;
        }
        if node.tier <= 1 {
            return true;
        }
        let gate = self
            .branch(&node.branch)
            .map_or(DEFAULT_TIER_GATE, |b| b.tier_gate);
        self.allocated_in_branch_tier(state, &node.branch, node.tier - 1) >= gate
    }

    /// Spend points on a node. Returns a player-facing reason on failure.
    pub fn allocate(&self, state: &mut SkillState, node_id: &str) -> Result<(), String> {
        let node = self
            .node(node_id)
            .ok_or_else(|| "That skill does not exist.".to_string())?;
        if state.allocated.iter().any(|a| a == node_id) {
            return Err("That skill is already learned.".into());
        }
        if state.points < node.cost {
            return Err("Not enough skill points.".into());
        }
        if !self.unlockable(state, node) {
            return Err("Learn more of this branch's earlier tiers first.".into());
        }
        state.points -= node.cost;
        state.allocated.push(node_id.to_string());
        Ok(())
    }

    /// Refund every allocated node's cost and clear the allocation.
    pub fn respec(&self, state: &mut SkillState) {
        for id in &state.allocated {
            if let Some(node) = self.node(id) {
                state.points += node.cost;
            }
        }
        state.allocated.clear();
        state.respecs += 1;
    }

    /// Apply one XP grant: per-source diminishing returns, then level-ups
    /// with their point payouts. Returns the raw XP gained (0 when the
    /// source is undeclared or the level cap is reached).
    pub fn grant_xp(&self, state: &mut SkillState, source: &str) -> f64 {
        debug_assert!(
            XP_SOURCES.contains(&source),
            "engine XP sources are the closed hook set"
        );
        let Some(src) = self.xp_sources.get(source) else {
            return 0.0;
        };
        if state.level >= self.max_level {
            return 0.0;
        }
        let count = state.source_counts.get(source).copied().unwrap_or(0);
        let gain = src.base / (1.0 + src.decay * f64::from(count));
        state.source_counts.insert(source.to_string(), count + 1);
        state.xp += gain;
        while state.level < self.max_level {
            let needed = self.xp_for_level(state.level);
            if state.xp < needed {
                break;
            }
            state.xp -= needed;
            state.level += 1;
            state.points += self.points_per_level;
        }
        gain
    }

    /// Fold allocated nodes' effects into a stat block.
    pub fn stats_for(&self, state: &SkillState) -> StatBlock {
        let mut block = StatBlock::default();
        for id in &state.allocated {
            if let Some(node) = self.node(id) {
                block.add_all(node.stats.iter().copied());
            }
        }
        block
    }
}

/// One player's progression. Owned by the session and persisted with the
/// player profile.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkillState {
    pub level: u32,
    pub xp: f64,
    pub points: u32,
    #[serde(default)]
    pub allocated: Vec<String>,
    /// Per-source grant counts feeding diminishing returns.
    #[serde(default)]
    pub source_counts: std::collections::BTreeMap<String, u32>,
    #[serde(default)]
    pub respecs: u32,
}

impl Default for SkillState {
    fn default() -> Self {
        SkillState {
            level: 1,
            xp: 0.0,
            points: 0,
            allocated: Vec::new(),
            source_counts: std::collections::BTreeMap::new(),
            respecs: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TREE: &str = r#"
schema_version = 1

[tree]
max_level = 10
points_per_level = 1
xp_base = 100.0
xp_exponent = 1.0

[[xp_source]]
id = "mine"
base = 25
decay = 0.5

[[branch]]
id = "survivalist"
name = "Survivalist"
tier_gate = 3

[[branch.node]]
id = "hardy"
name = "Hardy"
tier = 1
cost = 1
description = "+2 health"
[[branch.node.stats]]
kind = "health"
flat = 2

[[branch.node]]
id = "runner"
name = "Runner"
tier = 1
cost = 1
[[branch.node.stats]]
kind = "move_speed"
mult_permille = 1050

[[branch.node]]
id = "focused"
name = "Focused"
tier = 1
cost = 1
[[branch.node.stats]]
kind = "stamina_regen"
mult_permille = 1100

[[branch.node]]
id = "burly"
name = "Burly"
tier = 2
cost = 2
[[branch.node.stats]]
kind = "carry"
flat = 128
"#;

    fn tree() -> SkillTree {
        let raw = parse_skills(TREE, "gems").expect("parse");
        resolve(std::slice::from_ref(&raw)).expect("resolve")
    }

    #[test]
    fn parses_and_qualifies_nodes_and_sources() {
        let raw = parse_skills(TREE, "gems").expect("parse");
        let tree = resolve(std::slice::from_ref(&raw)).expect("resolve");
        assert_eq!(tree.nodes.len(), 4);
        assert!(tree.node("gems:hardy").is_some());
        assert!(tree.node("hardy").is_none());
        assert_eq!(tree.branch("gems:survivalist").unwrap().tier_gate, 3);
        assert_eq!(tree.xp_sources.get("mine").unwrap().base, 25.0);
    }

    #[test]
    fn unknown_source_and_full_level_grant_nothing() {
        let tree = tree();
        let mut state = SkillState::default();
        assert_eq!(tree.grant_xp(&mut state, "build"), 0.0);
        assert_eq!(state.level, 1);
        state.level = 10;
        assert_eq!(tree.grant_xp(&mut state, "mine"), 0.0);
        assert_eq!(state.level, 10);
    }

    #[test]
    fn xp_diminishes_per_source() {
        let tree = tree();
        let mut state = SkillState::default();
        let first = tree.grant_xp(&mut state, "mine");
        let second = tree.grant_xp(&mut state, "mine");
        let third = tree.grant_xp(&mut state, "mine");
        assert_eq!(first, 25.0);
        assert!(second < first);
        assert!(third < second);
        assert_eq!(state.source_counts.get("mine"), Some(&3));
    }

    #[test]
    fn leveling_pays_points_against_the_curve() {
        let tree = tree();
        let mut state = SkillState::default();
        for _ in 0..16 {
            tree.grant_xp(&mut state, "mine");
        }
        // Decay 0.5 keeps the grind honest; 16 grants cross the 100-XP
        // level-1 line and pay exactly one point per level.
        assert!(state.level >= 2, "level {}", state.level);
        assert_eq!(state.points, state.level - 1);
        assert!(state.xp < tree.xp_for_level(state.level));
    }

    #[test]
    fn level_cap_holds_at_max_level() {
        let tree = tree();
        let mut state = SkillState {
            level: 10,
            ..SkillState::default()
        };
        tree.grant_xp(&mut state, "mine");
        assert_eq!(state.level, 10);
        assert_eq!(state.points, 0);
    }

    #[test]
    fn tier_gate_requires_three_previous_tier_nodes() {
        let tree = tree();
        let mut state = SkillState {
            points: 9,
            ..SkillState::default()
        };
        let burly = tree.node("gems:burly").unwrap();
        tree.allocate(&mut state, "gems:hardy").unwrap();
        tree.allocate(&mut state, "gems:runner").unwrap();
        assert_eq!(state.points, 7);
        assert!(!tree.unlockable(&state, burly));
        assert!(tree.allocate(&mut state, &burly.id).is_err());
        // A third tier-1 node opens the tier-2 gate.
        tree.allocate(&mut state, "gems:focused").unwrap();
        assert_eq!(state.points, 6);
        assert!(tree.unlockable(&state, burly));
        tree.allocate(&mut state, &burly.id).unwrap();
        assert_eq!(state.points, 4);
        assert!(state.allocated.contains(&burly.id));
    }

    #[test]
    fn allocate_spends_points_and_refuses_repeats() {
        let tree = tree();
        let mut state = SkillState {
            points: 2,
            ..SkillState::default()
        };
        tree.allocate(&mut state, "gems:hardy").unwrap();
        assert_eq!(state.points, 1);
        assert!(tree.allocate(&mut state, "gems:hardy").is_err());
        assert!(tree.allocate(&mut state, "gems:nope").is_err());
        assert!(tree.allocate(&mut state, "gems:burly").is_err()); // cost 2 > 1
    }

    #[test]
    fn respec_refunds_every_allocated_cost() {
        let tree = tree();
        let mut state = SkillState {
            points: 2,
            ..SkillState::default()
        };
        tree.allocate(&mut state, "gems:hardy").unwrap();
        tree.allocate(&mut state, "gems:runner").unwrap();
        assert_eq!(state.points, 0);
        tree.respec(&mut state);
        assert_eq!(state.points, 2);
        assert!(state.allocated.is_empty());
        assert_eq!(state.respecs, 1);
    }

    #[test]
    fn allocated_effects_fold_into_a_stat_block() {
        let tree = tree();
        let mut state = SkillState {
            points: 4,
            ..SkillState::default()
        };
        tree.allocate(&mut state, "gems:hardy").unwrap();
        let block = tree.stats_for(&state);
        use crate::stats::StatKind;
        assert_eq!(block.effective(StatKind::Health, 14.0), 16.0);
    }

    #[test]
    fn empty_tree_is_inert() {
        let tree = SkillTree::default();
        assert!(tree.is_empty());
        let mut state = SkillState::default();
        assert_eq!(tree.grant_xp(&mut state, "mine"), 0.0);
        assert!(tree.allocate(&mut state, "any").is_err());
        assert!(
            tree.stats_for(&state)
                .effective(crate::stats::StatKind::Health, 14.0)
                == 14.0
        );
    }

    #[test]
    fn schema_version_and_duplicates_are_rejected() {
        assert!(parse_skills("schema_version = 2\n", "m").is_err());
        let bad = r#"
schema_version = 1
[[branch]]
id = "b"
[[branch.node]]
id = "a"
tier = 1
[[branch.node]]
id = "a"
tier = 1
"#;
        let raw = parse_skills(bad, "m").expect("parse");
        let errors = resolve(std::slice::from_ref(&raw)).expect_err("duplicate");
        assert!(errors.iter().any(|e| e.contains("duplicate node")));
    }

    #[test]
    fn bad_tiers_are_reported() {
        let bad = r#"
schema_version = 1
[[branch]]
id = "b"
[[branch.node]]
id = "deep"
tier = 9
"#;
        let raw = parse_skills(bad, "m").expect("parse");
        let errors = resolve(std::slice::from_ref(&raw)).expect_err("tier");
        assert!(errors.iter().any(|e| e.contains("tier must be 1..=4")));
    }

    #[test]
    fn registry_loads_skills_from_a_mod_dir() {
        let dir = std::env::temp_dir().join(format!("wildforge-skills-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mod_dir = dir.join("progression");
        std::fs::create_dir_all(&mod_dir).unwrap();
        std::fs::write(
            mod_dir.join("mod.toml"),
            "id = \"progression\"\nname = \"Progression\"\nversion = \"1.0.0\"\nworld_api = 2\ndepends = [\"base\"]\n",
        )
        .unwrap();
        std::fs::write(
            mod_dir.join("skills.toml"),
            r#"
schema_version = 1
[tree]
max_level = 50
[[xp_source]]
id = "mine"
base = 5
[[branch]]
id = "starter"
[[branch.node]]
id = "fit"
tier = 1
cost = 1
[[branch.node.stats]]
kind = "health"
flat = 2
"#,
        )
        .unwrap();
        let reg = crate::registry::load(&dir);
        assert!(
            reg.material_errors.is_empty(),
            "skills load errors: {:?}",
            reg.material_errors
        );
        assert_eq!(reg.skills.max_level, 50);
        assert!(reg.skills.node("progression:fit").is_some());
        assert!(reg.skills.xp_sources.contains_key("mine"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
