//! Adaptation in the graphical frame pipeline.

use crate::world::TerrainRead;
use crate::atlas;
use glam::Vec3;
use crate::game::Game;

impl Game {
    pub(in crate::game) fn prepare_frame_adaptation(&self, daylight: f32, mut amb_col: Vec3) -> (f32, Vec3) {
        // The flat fill under everything. It and the room tint are answering
        // the same question — what lights a surface no lamp reaches — and at
        // 0.12 the flat one is several times the honest one, so the room's own
        // colour cannot be seen past it. WILDFORGE_AMBIENT_FLOOR to explore.
        let mut ambient_floor = std::env::var("WILDFORGE_AMBIENT_FLOOR")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| (0.0..=1.0).contains(v))
            .unwrap_or(if self.config.stark { 0.04 } else { 0.12 });
        // Clear-eye is adaptation, not x-ray vision: it lifts only the final
        // low-light floor and gives already-authorized local Current signs a
        // very faint resonance tint. Guests use the same coarse strength and
        // dominant-category packet they receive without the preparation; no
        // ore, entity, inventory, exact mixture, or server-hidden state enters
        // the frame.
        let trace_strength = f32::from(self.survival.preparation_modifiers.trace_sight).min(250.0)
            / 250.0
            * f32::from(self.survival.preparation_modifiers.perception_permille)
            / 1_000.0;
        let darkness = (1.0 - daylight).clamp(0.0, 1.0);
        let trace_adaptation = trace_strength * 0.055 * darkness;
        ambient_floor = (ambient_floor + trace_adaptation).min(0.18);
        if trace_strength > 0.0 {
            let (bands, dominant) = if self.multiplayer.remote.is_some() {
                (
                    self.runtime.view().remote_arcane_cue(),
                    self.runtime.view().remote_arcane_dominant(),
                )
            } else if let Some(atlas) = self.runtime.view().planet_atlas() {
                self.runtime.local().world.arcane_sensory_cue_at(atlas.atlas_pos(self.player.pos.surface()))
            } else {
                ([0; 2], 0)
            };
            let local_sign = f32::from(bands[0].max(bands[1]).min(4)) / 4.0;
            let resonance = match dominant {
                1 => Vec3::new(0.55, 0.92, 0.52), // root
                2 => Vec3::new(0.36, 0.76, 0.94), // tide
                3 => Vec3::new(1.00, 0.56, 0.30), // ember
                4 => Vec3::new(0.78, 0.78, 0.72), // stone
                5 => Vec3::new(0.62, 0.80, 1.00), // gale
                6 => Vec3::new(0.48, 0.72, 1.00), // echo
                _ => Vec3::splat(0.72),
            };
            amb_col += resonance * (0.025 * trace_strength * local_sign * darkness);
        }

        (ambient_floor, amb_col)
    }
}
