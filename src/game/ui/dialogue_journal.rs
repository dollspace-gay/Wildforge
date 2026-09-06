//! Dialogue journal layout and UI composition.

use crate::game::Game;
use crate::ui::UiBatch;

impl Game {
    pub(in crate::game) fn draw_dialog_screen(
        &mut self,
        ui: &mut UiBatch,
        w: f32,
        h: f32,
        npc: u32,
        node_id: String,
    ) {
        ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.55]);
        let mob_id = npc;
        let name = self
            .runtime
            .view()
            .npc_by_mob(mob_id)
            .and_then(|n| self.content.reg.npcs.get(n.def))
            .map(|d| d.label.clone())
            .unwrap_or_else(|| "…".to_string());
        let title = name.to_uppercase();
        let tw = UiBatch::text_width(3.0, &title);
        ui.text_shadow((w - tw) / 2.0, h / 2.0 - 300.0, 3.0, &title, [1.0; 4]);
        let text = self.node_text(mob_id, &node_id);
        let body = UiBatch::text_width(1.6, &text);
        ui.text_shadow(
            w / 2.0 - body / 2.0,
            h / 2.0 - 220.0,
            1.6,
            &text,
            [0.95, 0.95, 0.95, 1.0],
        );
        let choices = self.visible_choices(mob_id, &node_id);
        if choices.is_empty() {
            ui.text_shadow(
                w / 2.0 - 150.0,
                h / 2.0 + 40.0,
                1.5,
                "—",
                [0.8, 0.8, 0.8, 1.0],
            );
        }
        for (i, c) in choices.iter().enumerate() {
            let r = (
                w / 2.0 - 300.0,
                h / 2.0 + 60.0 + i as f32 * 40.0,
                600.0,
                32.0,
            );
            let hover = self.hit(r);
            let bg = if hover {
                [0.45, 0.45, 0.45, 0.95]
            } else {
                [0.18, 0.18, 0.18, 0.95]
            };
            ui.rect(r.0, r.1, r.2, r.3, [0.08, 0.08, 0.08, 0.95]);
            ui.rect(r.0 + 2.0, r.1 + 2.0, r.2 - 4.0, r.3 - 4.0, bg);
            let lw = UiBatch::text_width(1.6, &c.label);
            ui.text_shadow(
                r.0 + (r.2 - lw) / 2.0,
                r.1 + (r.3 - 12.0) / 2.0,
                1.6,
                &c.label,
                [1.0; 4],
            );
        }
    }
    pub(in crate::game) fn draw_journal_screen(&mut self, ui: &mut UiBatch, w: f32, h: f32) {
        ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.55]);
        let title = "QUEST JOURNAL";
        let tw = UiBatch::text_width(3.0, title);
        ui.text_shadow((w - tw) / 2.0, h / 2.0 - 300.0, 3.0, title, [1.0; 4]);
        // Per-objective x / count from the mod KV; state read-only.
        let mut row = 0usize;
        for quest in self.content.reg.quests.iter() {
            let accepted = self
                .quest_state(&quest.id)
                .map(|s| s == "accepted" || s == "done")
                .unwrap_or(false);
            if !accepted {
                continue;
            }
            let (tx, ty) = (w / 2.0 - 380.0, h / 2.0 - 240.0 + row as f32 * 96.0);
            ui.text_shadow(tx, ty, 2.0, &quest.title, [1.0; 4]);
            for (j, obj) in quest.objectives.iter().enumerate() {
                let got = self.quest_progress(&quest.id, &obj.key).min(obj.count);
                let line = format!("   {}: {} / {}", obj.description, got, obj.count);
                ui.text_shadow(
                    tx + 12.0,
                    ty + 30.0 + j as f32 * 22.0,
                    1.3,
                    &line,
                    [0.9, 0.9, 0.9, 1.0],
                );
            }
            row += 1;
        }
        if row == 0 {
            ui.text_shadow(
                w / 2.0 - 220.0,
                h / 2.0 - 40.0,
                1.8,
                "No quests accepted yet.",
                [0.8, 0.8, 0.8, 1.0],
            );
        }
    }
}
