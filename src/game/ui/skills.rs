//! Skills layout and UI composition.

use crate::game::Game;
use crate::ui::UiBatch;

impl Game {
    pub(in crate::game) fn draw_skills_screen(&mut self, ui: &mut UiBatch, w: f32, h: f32) {
        ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.6]);
        let title = "SKILL TREE";
        let tw = UiBatch::text_width(3.0, title);
        ui.text_shadow((w - tw) / 2.0, h / 2.0 - 340.0, 3.0, title, [1.0; 4]);
        if !self.skills_enabled() {
            ui.text_shadow(
                w / 2.0 - 260.0,
                h / 2.0 - 40.0,
                1.8,
                "Skills are disabled in this world.",
                [0.8, 0.8, 0.8, 1.0],
            );
        } else {
            let tree = &self.content.reg.skills;
            // Header: level, points, and the XP bar toward the next.
            let header = format!(
                "LEVEL {}   POINTS {}   RESPECS {}",
                self.skills.level, self.skills.points, self.skills.respecs
            );
            ui.text_shadow(
                w / 2.0 - 240.0,
                h / 2.0 - 300.0,
                2.0,
                &header,
                [1.0, 0.9, 0.6, 1.0],
            );
            let needed = tree.xp_for_level(self.skills.level);
            let frac = (self.skills.xp / needed).clamp(0.0, 1.0) as f32;
            let (bx, by, bw) = (w / 2.0 - 220.0, h / 2.0 - 272.0, 440.0);
            ui.rect(bx, by, bw, 6.0, [0.02, 0.02, 0.03, 0.7]);
            ui.rect(bx, by, bw * frac, 6.0, [0.4, 0.75, 0.35, 0.95]);
            let xp_label = format!("{} / {}", self.skills.xp as u64, needed as u64);
            ui.text_shadow(
                bx + bw - UiBatch::text_width(1.3, &xp_label),
                by - 22.0,
                1.3,
                &xp_label,
                [0.8, 0.9, 0.8, 1.0],
            );
            let branches = &tree.branches;
            if !branches.is_empty() {
                let branch_idx = self.ui_state.skills_branch.min(branches.len() - 1);
                for (i, b) in branches.iter().enumerate() {
                    let r = self.skill_branch_tab_rect(i);
                    let active = i == branch_idx;
                    let bg = if active {
                        [0.35, 0.4, 0.5, 0.95]
                    } else {
                        [0.18, 0.2, 0.25, 0.9]
                    };
                    ui.rect(r.0, r.1, r.2, r.3, bg);
                    let lw = UiBatch::text_width(1.6, &b.name);
                    ui.text_shadow(r.0 + (r.2 - lw) / 2.0, r.1 + 8.0, 1.6, &b.name, [1.0; 4]);
                }
                let nodes: Vec<_> = tree
                    .nodes
                    .iter()
                    .filter(|n| n.branch == branches[branch_idx].id)
                    .collect();
                for (i, node) in nodes.iter().enumerate() {
                    let r = self.skill_node_rect(i);
                    let allocated = self.skills.allocated.iter().any(|a| a == &node.id);
                    let unlockable = tree.unlockable(&self.skills, node);
                    let bg = if allocated {
                        [0.25, 0.55, 0.3, 0.95]
                    } else if unlockable {
                        [0.35, 0.35, 0.42, 0.95]
                    } else {
                        [0.12, 0.12, 0.15, 0.95]
                    };
                    ui.rect(r.0, r.1, r.2, r.3, [0.05, 0.05, 0.06, 0.95]);
                    ui.rect(r.0 + 2.0, r.1 + 2.0, r.2 - 4.0, r.3 - 4.0, bg);
                    let lw = UiBatch::text_width(1.5, &node.name);
                    ui.text_shadow(
                        r.0 + (r.2 - lw) / 2.0,
                        r.1 + 10.0,
                        1.5,
                        &node.name,
                        [1.0; 4],
                    );
                    let cost = format!("T{}  {} PT", node.tier, node.cost);
                    let cw = UiBatch::text_width(1.1, &cost);
                    ui.text_shadow(
                        r.0 + (r.2 - cw) / 2.0,
                        r.1 + r.3 - 22.0,
                        1.1,
                        &cost,
                        if allocated {
                            [0.8, 1.0, 0.8, 1.0]
                        } else {
                            [0.8, 0.8, 0.8, 1.0]
                        },
                    );
                    if !node.description.is_empty() {
                        let dw = UiBatch::text_width(1.0, &node.description);
                        ui.text_shadow(
                            r.0 + (r.2 - dw) / 2.0,
                            r.1 + r.3 + 4.0,
                            1.0,
                            &node.description,
                            [0.75, 0.75, 0.75, 1.0],
                        );
                    }
                }
                let respec = self.skill_respec_rect();
                let hover = self.hit(respec);
                let bg = if hover {
                    [0.5, 0.4, 0.3, 0.95]
                } else {
                    [0.3, 0.25, 0.2, 0.95]
                };
                ui.rect(respec.0, respec.1, respec.2, respec.3, bg);
                let lw = UiBatch::text_width(1.5, "RESPEC");
                ui.text_shadow(
                    respec.0 + (respec.2 - lw) / 2.0,
                    respec.1 + 8.0,
                    1.5,
                    "RESPEC",
                    [1.0; 4],
                );
            }
        }
    }
}
