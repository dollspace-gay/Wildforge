//! Loadout layout and UI composition.

use crate::game::Game;
use crate::game::widgets;
use crate::inventory::TOTAL_SLOTS;
use crate::ui::UiBatch;

impl Game {
    pub(in crate::game) fn draw_loadout_screen(&mut self, ui: &mut UiBatch, w: f32, h: f32) {
        ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.6]);
        let title = "LOADOUT";
        let tw = UiBatch::text_width(3.0, title);
        ui.text_shadow((w - tw) / 2.0, h / 2.0 - 340.0, 3.0, title, [1.0; 4]);
        if !crate::game::equipment::equipment_enabled(self) {
            ui.text_shadow(
                w / 2.0 - 300.0,
                h / 2.0 - 40.0,
                1.8,
                "Modular equipment is disabled in this world.",
                [0.8, 0.8, 0.8, 1.0],
            );
        } else {
            let reg = &self.content.reg;
            for i in 0..4 {
                let r = self.loadout_frame_rect(i);
                let selected = self.ui_state.loadout_select == i;
                let bg = if selected {
                    [0.3, 0.38, 0.5, 0.95]
                } else {
                    [0.14, 0.16, 0.2, 0.95]
                };
                ui.rect(r.0, r.1, r.2, r.3, bg);
                let (name, status) = match &self.survival.armor[i] {
                    Some(frame) => {
                        let def = reg.item(frame.item);
                        let disabled = crate::game::equipment::equipment_enabled(self)
                            && def.frame.is_some()
                            && frame.durability == 0;
                        let label = if disabled {
                            format!("{} (BROKEN)", def.label)
                        } else {
                            def.label.clone()
                        };
                        (
                            label,
                            format!("durability {} / {}", frame.durability, def.durability),
                        )
                    }
                    None => ("(empty)".into(), String::new()),
                };
                ui.text_shadow(r.0 + 10.0, r.1 + 10.0, 1.4, &name, [1.0; 4]);
                if !status.is_empty() {
                    ui.text_shadow(r.0 + 10.0, r.1 + 40.0, 1.1, &status, [0.8, 0.8, 0.8, 1.0]);
                }
                let slot_name = ["HEAD", "CHEST", "LEGS", "FEET"][i];
                let sw = UiBatch::text_width(1.2, slot_name);
                ui.text_shadow(
                    r.0 + r.2 - sw - 10.0,
                    r.1 + 10.0,
                    1.2,
                    slot_name,
                    [0.6, 0.7, 0.85, 1.0],
                );
                // Component sub-slots drawn beside the frame box.
                let def = self.survival.armor[i].as_ref().map(|f| reg.item(f.item));
                let frame_def = def.and_then(|d| d.frame.as_ref());
                let mut sub = 0usize;
                if let Some(frame_def) = frame_def {
                    for slot in &frame_def.slots {
                        for _ in 0..slot.max {
                            let sr = self.loadout_component_rect(i, sub);
                            let stack = self.survival.loadouts[i]
                                .components
                                .get(sub)
                                .map(|c| c.stack);
                            let label = stack.map(|s| reg.item(s.item).label.clone());
                            widgets::slot(reg, &mut *ui, sr, stack, false, self.hit(sr));
                            if let Some(label) = &label {
                                let lw = UiBatch::text_width(0.8, label);
                                ui.text_shadow(
                                    sr.0 + (sr.2 - lw) / 2.0,
                                    sr.1 + sr.3 + 2.0,
                                    0.8,
                                    label,
                                    [0.85, 0.85, 0.85, 1.0],
                                );
                            }
                            sub += 1;
                        }
                    }
                }
            }
            // Repair + presets.
            let repair = self.loadout_repair_rect();
            let hover = self.hit(repair);
            let bg = if hover {
                [0.4, 0.5, 0.35, 0.95]
            } else {
                [0.25, 0.3, 0.22, 0.95]
            };
            ui.rect(repair.0, repair.1, repair.2, repair.3, bg);
            let lw = UiBatch::text_width(1.4, "REPAIR");
            ui.text_shadow(
                repair.0 + (repair.2 - lw) / 2.0,
                repair.1 + 8.0,
                1.4,
                "REPAIR",
                [1.0; 4],
            );
            for index in 0..4 {
                let r = self.loadout_preset_rect(index);
                let active = self.ui_state.loadout_preset_sel == index;
                let saved = self
                    .survival
                    .loadout_presets
                    .get(index)
                    .is_some_and(|p| p.slots.iter().any(Option::is_some));
                let bg = if active {
                    [0.35, 0.4, 0.5, 0.95]
                } else {
                    [0.18, 0.2, 0.25, 0.9]
                };
                ui.rect(r.0, r.1, r.2, r.3, bg);
                let label = if saved {
                    format!("PRESET {}", index + 1)
                } else {
                    format!("PRESET {} (empty)", index + 1)
                };
                let pw = UiBatch::text_width(1.3, &label);
                ui.text_shadow(r.0 + (r.2 - pw) / 2.0, r.1 + 8.0, 1.3, &label, [1.0; 4]);
                let br = self.loadout_preset_button_rect(index);
                let action = if index == 0 { "SAVE" } else { "APPLY" };
                ui.rect(br.0, br.1, br.2, br.3, [0.28, 0.3, 0.38, 0.95]);
                let bw = UiBatch::text_width(1.2, action);
                ui.text_shadow(br.0 + (br.2 - bw) / 2.0, br.1 + 8.0, 1.2, action, [1.0; 4]);
            }
            // Inventory grid for components.
            for i in 0..TOTAL_SLOTS {
                let r = self.loadout_inv_rect(i);
                widgets::slot(
                    reg,
                    &mut *ui,
                    r,
                    self.inventory.slots[i],
                    false,
                    self.hit(r),
                );
            }
            ui.text_shadow(
                w / 2.0 + 180.0,
                h / 2.0 - 245.0,
                1.3,
                "INVENTORY",
                [0.72, 0.75, 0.78, 1.0],
            );
        }
    }
}
