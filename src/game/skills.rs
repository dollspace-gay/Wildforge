//! Game-side skill-tree glue (belt-quest capability E5): gating, XP
//! granting, allocation, respec, and the effects fold into the E4 stat
//! surface. The data model lives in `crate::skills`; this file wires it to
//! the live world (mode-gated via E1) and the player session.

use super::*;
use crate::stats::StatBlock;

impl Game {
    /// Whether the skill tree is live: the world's mode opts in (E1
    /// `skills = true`), the pack ships content, and we are not creative.
    pub(super) fn skills_enabled(&self) -> bool {
        if self.creative {
            return false;
        }
        let tree = &self.content.reg.skills;
        if tree.is_empty() {
            return false;
        }
        self.server.world.ruleset().skills
    }

    /// Grant XP from one canonical source. No-op when skills are disabled
    /// or the source is undeclared; the tree applies diminishing returns
    /// and pays level-up points.
    pub(super) fn grant_xp(&mut self, source: &str) {
        if !self.skills_enabled() {
            return;
        }
        self.content.reg.skills.grant_xp(&mut self.skills, source);
    }

    /// Aggregated stat modifiers from allocated skill nodes, merged into
    /// the E4 `StatBlock` (empty when skills are disabled).
    pub(super) fn skills_stats(&self) -> StatBlock {
        if !self.skills_enabled() {
            return StatBlock::default();
        }
        self.content.reg.skills.stats_for(&self.skills)
    }

    /// Try to learn a node; player-facing error on failure.
    pub(super) fn allocate_skill(&mut self, node_id: &str) -> Result<(), String> {
        if !self.skills_enabled() {
            return Err("Skills are disabled in this world.".into());
        }
        let tree = &self.content.reg.skills;
        let name = tree.node(node_id).map(|n| n.name.clone());
        tree.allocate(&mut self.skills, node_id)?;
        self.sfx(Sfx::Click);
        if let Some(name) = name {
            self.toast(format!("Skill learned: {name}"));
        }
        Ok(())
    }

    /// Refund every allocated point (a temporary free respec until a
    /// content-gated respec item exists).
    pub(super) fn respec_skills(&mut self) {
        if !self.skills_enabled() {
            return;
        }
        let tree = &self.content.reg.skills;
        let before = self.skills.allocated.len();
        tree.respec(&mut self.skills);
        if self.skills.allocated.len() < before {
            self.sfx(Sfx::Click);
            self.toast("Skills reset; points refunded.".to_string());
        }
    }

    /// The XP required to reach the next level, for the HUD bar.
    pub(super) fn skills_xp_to_next(&self) -> f64 {
        if !self.skills_enabled() {
            return 0.0;
        }
        self.content.reg.skills.xp_for_level(self.skills.level)
    }

    // ------- UI geometry shared by the skill screen and HUD -------

    /// A skill node's box on the skill screen, by index within the current
    /// branch. Rows are tiers; columns spread nodes left to right.
    pub(super) fn skill_node_rect(&self, index: usize) -> (f32, f32, f32, f32) {
        let (w, h) = (self.window.inner_size().width as f32, self.window.inner_size().height as f32);
        let cols = 4usize;
        let (col, tier) = (index % cols, index / cols);
        let bw = 170.0;
        let bh = 84.0;
        let gap = 24.0;
        let total_w = cols as f32 * bw + (cols - 1) as f32 * gap;
        let x = w / 2.0 - total_w / 2.0 + col as f32 * (bw + gap);
        let y = h / 2.0 - 120.0 + tier as f32 * (bh + gap);
        (x, y, bw, bh)
    }

    /// Branch tab rect on the skill screen.
    pub(super) fn skill_branch_tab_rect(&self, index: usize) -> (f32, f32, f32, f32) {
        let (w, _h) = (self.window.inner_size().width as f32, self.window.inner_size().height as f32);
        let tw = 180.0;
        let x = w / 2.0 - 320.0 + index as f32 * 200.0;
        (x, 160.0, tw, 34.0)
    }

    /// The respec button rect on the skill screen.
    pub(super) fn skill_respec_rect(&self) -> (f32, f32, f32, f32) {
        let (w, h) = (self.window.inner_size().width as f32, self.window.inner_size().height as f32);
        (w / 2.0 - 75.0, h / 2.0 + 200.0, 150.0, 34.0)
    }
}