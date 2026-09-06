//! World labels layout and UI composition.

use crate::world::TerrainRead;
use crate::identity;
use crate::raycast;
use crate::ui::UiBatch;
use glam::Vec3;
use crate::game::Game;
use super::{project_world_label};

impl Game {

    pub(in crate::game) fn draw_world_nameplate(
        &self,
        ui: &mut UiBatch,
        name: &str,
        feet: crate::planet::EntityPos,
        width: f32,
        height: f32,
    ) {
        let Ok(head) = feet.translated(Vec3::new(0.0, 2.12, 0.0)) else {
            return;
        };
        let head = head.pos;
        let local_sight = self.player.eye().local_delta_to(head);
        let distance = local_sight.length();
        if !(1.0..=64.0).contains(&distance)
            || raycast::raycast_at(
                &self.runtime.view(),
                self.player.eye(),
                local_sight,
                (distance - 0.3).max(0.0),
            )
            .is_some()
        {
            return;
        }
        let relative = head.render_pos() - self.camera.pos;
        let Some((sx, sy)) = project_world_label(self.camera.view_proj(), relative, width, height)
        else {
            return;
        };
        // Keep world labels compact even when the roster includes an opted-in
        // handle. The roster is the detail surface; the world needs a readable
        // display name and a small verification signal.
        let label = if let Some((identity, verification)) = name.split_once(" [") {
            let display_name = identity
                .split_once(" @")
                .map_or(identity, |(display_name, _)| display_name);
            let badge = if verification.starts_with("VERIFIED/CACHED") {
                "V*"
            } else {
                "V"
            };
            format!("{display_name} [{badge}]")
        } else {
            name.to_owned()
        }
        .to_uppercase();
        let scale = if distance < 24.0 { 1.5 } else { 1.25 };
        let alpha = ((64.0 - distance) / 16.0).clamp(0.35, 1.0);
        let text_width = UiBatch::text_width(scale, &label);
        let text_y = sy - 9.0 * scale;
        ui.rect(
            sx - text_width * 0.5 - 4.0,
            text_y - 3.0,
            text_width + 8.0,
            7.0 * scale + 6.0,
            [0.01, 0.01, 0.015, 0.52 * alpha],
        );
        ui.text_shadow(
            sx - text_width * 0.5,
            text_y,
            scale,
            &label,
            [1.0, 1.0, 1.0, alpha],
        );
    }

    /// World-space combat feedback: floating damage numbers and health bars
    /// over damaged or hostile mobs.
    pub(in crate::game) fn draw_combat_overlays(&self, ui: &mut UiBatch, w: f32, h: f32) {
        if !self.in_world {
            return;
        }
        for n in &self.combat.damage_numbers {
            let t = (n.age / n.lifetime).clamp(0.0, 1.0);
            let relative = n.pos - self.camera.pos;
            let Some((sx, sy)) = project_world_label(self.camera.view_proj(), relative, w, h)
            else {
                continue;
            };
            let scale = if n.critical { 2.0 } else { 1.4 };
            let alpha = (1.0 - t).clamp(0.0, 1.0);
            let text = format!("{:.1}", n.value);
            let tw = UiBatch::text_width(scale, &text);
            let color = if n.critical {
                [1.0, 0.55, 0.25, alpha]
            } else {
                [0.95, 0.95, 0.95, alpha]
            };
            ui.text_shadow(sx - tw * 0.5, sy - t * 14.0, scale, &text, color);
        }
        let reg = &self.content.reg;
        for m in self.runtime.view().mobs() {
            let Some(def) = reg.animals.get(m.species) else {
                continue;
            };
            // Fresh friendly critters keep their bars hidden; a damaged
            // mob or anything hostile earns one.
            if !def.hostile && m.health >= def.health {
                continue;
            }
            let Ok(head) = m.pos.translated(Vec3::new(0.0, def.height + 0.4, 0.0)) else {
                continue;
            };
            let head = head.pos;
            let local_sight = self.player.eye().local_delta_to(head);
            let distance = local_sight.length();
            if !(1.0..=24.0).contains(&distance)
                || raycast::raycast_at(
                    &self.runtime.view(),
                    self.player.eye(),
                    local_sight,
                    (distance - 0.3).max(0.0),
                )
                .is_some()
            {
                continue;
            }
            let relative = head.render_pos() - self.camera.pos;
            let Some((sx, sy)) = project_world_label(self.camera.view_proj(), relative, w, h)
            else {
                continue;
            };
            let bw = (12.0 * 24.0 / distance).clamp(6.0, 18.0);
            let frac = (m.health / def.health).clamp(0.0, 1.0);
            let fg = if frac > 0.55 {
                [0.55, 0.85, 0.4, 0.95]
            } else if frac > 0.25 {
                [0.9, 0.8, 0.3, 0.95]
            } else {
                [0.9, 0.3, 0.25, 0.95]
            };
            ui.rect(sx - bw * 0.5, sy, bw, 2.0, [0.0, 0.0, 0.0, 0.55]);
            ui.rect(sx - bw * 0.5, sy, bw * frac.max(0.02), 2.0, fg);
        }
    }
}
