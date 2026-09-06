//! Gameplay hud layout and UI composition.

use crate::world::TerrainRead;
use crate::game::widgets;
use crate::ui::UiBatch;
use glam::Vec3;
use crate::game::Game;
use crate::game::MAX_AIR;
use crate::game::navigation::Screen;

impl Game {
    pub(in crate::game) fn draw_gameplay_hud(&mut self, ui: &mut UiBatch, w: f32, h: f32) {
        // Keep gameplay instrumentation in gameplay. Inventory and container
        // screens already present those objects directly; repeating the
        // hotbar, vitals, chat, and nameplates behind them creates two
        // competing visual hierarchies.
        if self.ui_state.screen == Screen::Playing {
            // Hotbar.
            for i in 0..HOTBAR_SLOTS {
                let mut r = self.inventory_layout().hotbar_rect(i);
                // Selection bounce: 1.0 -> 1.12 -> 1.0 over ~120ms.
                if i == self.input.hotbar_sel && self.presentation.sel_bounce < 1.0 {
                    let t = self.presentation.sel_bounce;
                    let sc = 1.0 + 0.12 * (t * std::f32::consts::PI).sin();
                    let (cx, cy) = (r.0 + r.2 / 2.0, r.1 + r.3 / 2.0);
                    r = (cx - r.2 / 2.0 * sc, cy - r.3 / 2.0 * sc, r.2 * sc, r.3 * sc);
                }
                widgets::slot(
                    &self.content.reg,
                    &mut *ui,
                    r,
                    self.inventory.slots[i],
                    i == self.input.hotbar_sel,
                    false,
                );
                // Pickup pulse: one bright cycle over the receiving slot.
                let p = self.presentation.slot_pulse[i];
                if p > 0.0 {
                    let a = (p / 0.18) * 0.35;
                    ui.rect(
                        r.0 + 1.0,
                        r.1 + 1.0,
                        r.2 - 2.0,
                        r.3 - 2.0,
                        [1.0, 1.0, 0.9, a],
                    );
                }
            }
            // Ghost icons fly from the pickup point to their slot.
            for &(icon, (fx, fy), slot, age) in &self.presentation.ui_flies {
                let t = (age / 0.22).min(1.0);
                let t = t * t; // ease-in quad
                let r = self.inventory_layout().hotbar_rect(slot);
                let (tx, ty) = (r.0 + r.2 / 2.0, r.1 + r.3 / 2.0);
                let x = fx + (tx - fx) * t;
                let y = fy + (ty - fy) * t;
                let sz = 28.0 * (1.0 - 0.4 * t);
                ui.tile(
                    x - sz / 2.0,
                    y - sz / 2.0,
                    sz,
                    sz,
                    icon,
                    [1.0, 1.0, 1.0, 0.9 * (1.0 - t * 0.5)],
                );
            }
            // Selected item name above the hotbar.
            if let Some(s) = self.inventory.slots[self.input.hotbar_sel] {
                let name = &self.content.reg.item(s.item).label.to_uppercase();
                let tw = UiBatch::text_width(2.0, name);
                let (hx0, hy0) = self.inventory_layout().hotbar_origin();
                ui.text_shadow(
                    hx0 + (9.0 * Self::SLOT - tw) / 2.0,
                    hy0 - 56.0,
                    2.0,
                    name,
                    [1.0; 4],
                );
            }

            // Hearts above the hotbar (count follows max health).
            let (hx, hy) = self.inventory_layout().hotbar_origin();
            let hs = 2.6;
            let hearts = if self.creative {
                0
            } else {
                (self.max_health() / 2.0).ceil() as i32
            };
            let clock = self.total_frames as f32 / 60.0;
            for i in 0..hearts {
                let kind = if self.survival.health >= (i * 2 + 2) as f32 {
                    2
                } else if self.survival.health >= (i * 2 + 1) as f32 {
                    1
                } else {
                    0
                };
                let wobble = if self.presentation.juice && self.survival.health <= 6.0 && kind > 0 {
                    (clock * 9.0 + i as f32 * 1.7).sin() * 2.0
                } else {
                    0.0
                };
                ui.heart(hx + i as f32 * 8.0 * hs, hy - 24.0 + wobble, hs, kind);
            }
            // Stamina bar under the hearts; hidden in creative (never
            // exhausts). Amber when low, since combat costs live here.
            if !self.creative {
                let frac = (self.combat.stamina / self.stamina_max()).clamp(0.0, 1.0);
                let sw = 9.0 * Self::SLOT * 0.55;
                let sy = hy - 10.0;
                ui.rect(hx, sy, sw, 3.0, [0.02, 0.02, 0.03, 0.7]);
                let col = if frac > 0.35 {
                    [0.5, 0.85, 0.4, 0.95]
                } else {
                    [0.9, 0.5, 0.3, 0.95]
                };
                ui.rect(hx, sy, sw * frac.max(0.04), 3.0, col);
                // Carry burden meter under the stamina bar.
                let capacity = self.carry_capacity();
                if capacity > 0.0 {
                    let burden = (self.carried_weight() / capacity).clamp(0.0, 1.0);
                    let by = sy + 5.0;
                    ui.rect(hx, by, sw, 2.0, [0.02, 0.02, 0.03, 0.7]);
                    let col = if burden < 0.7 {
                        [0.65, 0.6, 0.5, 0.9]
                    } else if burden < 0.9 {
                        [0.9, 0.7, 0.3, 0.95]
                    } else {
                        [0.95, 0.4, 0.3, 0.95]
                    };
                    ui.rect(hx, by, sw * burden.max(0.05), 2.0, col);
                }
                // Skill XP bar (capability E5) under the burden meter.
                if self.skills_enabled() {
                    let needed = self.skills_xp_to_next();
                    if needed > 0.0 {
                        let frac = (self.skills.xp / needed).clamp(0.0, 1.0) as f32;
                        let xy = sy + 10.0;
                        ui.rect(hx, xy, sw, 2.0, [0.02, 0.02, 0.03, 0.7]);
                        ui.rect(hx, xy, sw * frac.max(0.05), 2.0, [0.35, 0.7, 0.3, 0.95]);
                    }
                }
            }
            // Armor pips above the hearts, only while wearing any.
            let ap = if self.creative {
                0
            } else {
                self.armor_points()
            };
            for i in 0..ap.min(15) {
                let x = hx + i as f32 * 6.0 * hs * 0.8;
                ui.rect(x, hy - 48.0, 4.0 * hs, 4.0 * hs, [0.75, 0.72, 0.6, 0.95]);
            }
            // Hunger pips, right-aligned above the hotbar.
            let pips = (self.survival.hunger / 2.0).ceil() as i32;
            for i in 0..if self.creative { 0 } else { 10 } {
                let x = hx + 9.0 * Self::SLOT - (i + 1) as f32 * 8.0 * hs;
                let a = if i < pips { 1.0 } else { 0.25 };
                ui.rect(
                    x,
                    hy - 24.0 + 4.0,
                    6.0 * hs * 0.7,
                    5.0 * hs * 0.7,
                    [0.85, 0.55, 0.2, a],
                );
            }
            // Bow draw near the crosshair (red until min draw, then filling).
            if self.interaction.bow_draw > 0.0 {
                let t = ((self.interaction.bow_draw - 0.25) / 0.75).clamp(0.0, 1.0);
                ui.rect(
                    w / 2.0 - 30.0,
                    h / 2.0 + 24.0,
                    60.0,
                    6.0,
                    [0.1, 0.1, 0.1, 0.8],
                );
                let col = if self.interaction.bow_draw < 0.25 {
                    [0.7, 0.3, 0.2, 0.95]
                } else {
                    [0.75, 0.9, 0.5, 0.95]
                };
                ui.rect(w / 2.0 - 30.0, h / 2.0 + 24.0, 60.0 * t.max(0.06), 6.0, col);
            }
            // Chat entry line.
            if self.multiplayer.chat_open {
                ui.rect(12.0, h - 46.0, w * 0.5, 30.0, [0.0, 0.0, 0.0, 0.7]);
                let line = format!("SAY: {}_", self.multiplayer.chat_text.to_uppercase());
                ui.text_shadow(18.0, h - 40.0, 2.0, &line, [1.0; 4]);
            }
            // Other players: world-space identity labels with distance fading,
            // screen clipping, and terrain occlusion.
            if let Some(r) = &self.multiplayer.remote {
                for (id, (name, _, _)) in &r.players {
                    if let Some(&pos) = r.player_positions.get(id) {
                        self.draw_world_nameplate(&mut *ui, name, pos, w, h);
                    }
                }
            }
            if let Some(hst) = &self.multiplayer.host {
                for g in hst.guests.values() {
                    if !g.is_active() {
                        continue;
                    }
                    self.draw_world_nameplate(
                        &mut *ui,
                        &g.public_label(),
                        g.render_entity_pos(),
                        w,
                        h,
                    );
                }
            }
            if std::env::var("WILDFORGE_DEMO_PLAYER").is_ok() && self.in_world {
                for (i, name) in ["ROWAN", "MICA", "SOL"].iter().enumerate() {
                    let translated = self
                        .player
                        .pos
                        .translated(Vec3::new([-1.2, 0.0, 1.2][i], 0.0, 3.5))
                        .expect("demo player offset stays on the planet")
                        .pos;
                    let surface = translated
                        .block()
                        .expect("demo player remains inside the voxel shell")
                        .surface();
                    let at = crate::planet::EntityPos::new(
                        surface.face(),
                        translated.u(),
                        self.runtime.view().surface_height_at(surface) as f32 + 1.0,
                        translated.v(),
                    )
                    .expect("demo player position is canonical");
                    self.draw_world_nameplate(&mut *ui, name, at, w, h);
                }
            }
            // Signs and waystones wear their words in the world,
            // nameplate-style (occluded, distance-gated).
            let sign_texts: Vec<(crate::planet::EntityPos, [String; 3])> = self.runtime.view().sign_texts()
                .map(|(pos, st)| (pos.entity_center(), st.lines.clone()))
                .collect();
            for (at, lines) in sign_texts {
                if at.distance_to(self.player.pos) > 24.0 {
                    continue;
                }
                for (i, l) in lines.iter().enumerate() {
                    if l.is_empty() {
                        continue;
                    }
                    // Every line's head stays above the post itself,
                    // or the sign block occludes its own lower lines.
                    if let Ok(feet) = at.translated(Vec3::new(0.0, -(0.32 + i as f32 * 0.3), 0.0)) {
                        self.draw_world_nameplate(&mut *ui, &l.to_uppercase(), feet.pos, w, h);
                    }
                }
            }
            self.draw_combat_overlays(&mut *ui, w, h);
            // Brushing progress near the crosshair.
            if self.interaction.anvil_work > 0.0 {
                let t = (self.interaction.anvil_work / 2.0).min(1.0);
                ui.rect(
                    w / 2.0 - 30.0,
                    h / 2.0 + 24.0,
                    60.0,
                    6.0,
                    [0.1, 0.1, 0.1, 0.8],
                );
                ui.rect(
                    w / 2.0 - 30.0,
                    h / 2.0 + 24.0,
                    60.0 * t,
                    6.0,
                    [0.85, 0.85, 0.9, 0.95],
                );
            }
            if self.interaction.brushing > 0.0 {
                let t = (self.interaction.brushing / 1.5).min(1.0);
                ui.rect(
                    w / 2.0 - 30.0,
                    h / 2.0 + 24.0,
                    60.0,
                    6.0,
                    [0.1, 0.1, 0.1, 0.8],
                );
                ui.rect(
                    w / 2.0 - 30.0,
                    h / 2.0 + 24.0,
                    60.0 * t,
                    6.0,
                    [0.75, 0.7, 0.5, 0.95],
                );
            }
            if self.interaction.lens_settle > 0.0 {
                let t = (self.interaction.lens_settle / 1.25).min(1.0);
                let label = if t < 0.34 {
                    "LENS: FINDING REFERENCE"
                } else if t < 0.75 {
                    "LENS: NEEDLE SETTLING"
                } else {
                    "LENS: READING STABLE"
                };
                let text_width = UiBatch::text_width(1.25, label);
                ui.text_shadow(
                    w / 2.0 - text_width / 2.0,
                    h / 2.0 + 34.0,
                    1.25,
                    label,
                    [0.9, 0.88, 1.0, 0.98],
                );
                ui.rect(
                    w / 2.0 - 38.0,
                    h / 2.0 + 24.0,
                    76.0,
                    6.0,
                    [0.1, 0.1, 0.12, 0.85],
                );
                ui.rect(
                    w / 2.0 - 38.0,
                    h / 2.0 + 24.0,
                    76.0 * t,
                    6.0,
                    [0.65, 0.52, 0.9, 0.98],
                );
                // A moving white needle and explicit text carry the same
                // state as color, including for color-vision deficiencies.
                ui.rect(
                    w / 2.0 - 38.0 + 76.0 * t,
                    h / 2.0 + 21.0,
                    2.0,
                    12.0,
                    [1.0, 1.0, 1.0, 1.0],
                );
            }
            // Eat progress near the crosshair.
            if self.survival.eating > 0.0
                && let Some(f) = self.inventory.slots[self.input.hotbar_sel]
                    .and_then(|s| self.content.reg.item(s.item).food.clone())
            {
                let t = (self.survival.eating / f.eat_time).min(1.0);
                ui.rect(
                    w / 2.0 - 30.0,
                    h / 2.0 + 24.0,
                    60.0,
                    6.0,
                    [0.1, 0.1, 0.1, 0.8],
                );
                ui.rect(
                    w / 2.0 - 30.0,
                    h / 2.0 + 24.0,
                    60.0 * t,
                    6.0,
                    [0.9, 0.8, 0.3, 0.95],
                );
            }

            // Air bubbles (right-aligned above hotbar) when submerged.
            if self.survival.air < MAX_AIR && !self.creative {
                let n = (self.survival.air / MAX_AIR * 10.0).ceil() as usize;
                for i in 0..n {
                    let x = hx + 9.0 * Self::SLOT - (i + 1) as f32 * 8.0 * hs;
                    ui.bubble(x, hy - 16.0 * hs - 8.0, hs);
                }
            }
            self.draw_roster_overlay(&mut *ui, w);
        }

    }
}
