//! Selection in the graphical frame pipeline.

use super::SelectionFrame;
use crate::chunk::CHUNK_X;
use crate::game::Game;
use crate::game::navigation::Screen;
use crate::raycast;
use crate::world::TerrainRead;

impl Game {
    pub(in crate::game) fn prepare_frame_selection(&self, gloom: f32) -> SelectionFrame {
        let playing = self.ui_state.screen == Screen::Playing;
        let outline = if playing && self.config.outline {
            raycast::raycast_at(
                &self.runtime.view(),
                self.player.eye(),
                self.camera.local_forward(),
                self.reach(),
            )
            .map(|h| h.block)
        } else {
            None
        };
        let held_wand = self.inventory.slots[self.input.hotbar_sel].is_some_and(|stack| {
            stack.arcane_id != 0
                && self
                    .content
                    .reg
                    .item(stack.item)
                    .implement
                    .as_ref()
                    .is_some_and(|definition| {
                        definition.kind == crate::implements::ImplementItemKind::Wand
                    })
        });
        let active_warning = if self.multiplayer.remote.is_some() {
            self.presentation
                .working_cues
                .values()
                .map(|(cue, _)| cue.warning_band)
                .max()
                .unwrap_or_default()
        } else {
            self.runtime
                .local()
                .world
                .working_cues()
                .into_iter()
                .map(|cue| cue.warning_band)
                .max()
                .unwrap_or_default()
        };
        let outline_color = if active_warning >= 2 {
            let phase = (self.time_abs * 12.0).sin() * 0.16;
            [0.92, 0.20 + phase.max(0.0), 0.12]
        } else if self.interaction.lens_settle > 0.0 {
            let phase = (self.time_abs * 10.0).sin() * 0.12;
            [0.62 + phase, 0.48 + phase, 0.88]
        } else if held_wand {
            outline.map_or([0.42, 0.34, 0.68], |pos| {
                let block = self.runtime.view().get_block_at(pos);
                let definition = self.content.reg.block(block);
                if self.content.reg.is_water(block) {
                    [0.18, 0.64, 0.92]
                } else if definition.crop_next.is_some() || definition.sapling.is_some() {
                    [0.26, 0.78, 0.38]
                } else if definition.burns != 0 {
                    [0.94, 0.46, 0.14]
                } else {
                    [0.42, 0.34, 0.68]
                }
            })
        } else {
            [0.05, 0.05, 0.05]
        };
        let underwater = self.player.head_underwater(&self.runtime.view());
        let fog = (self.config.view_dist as f32 - 0.5) * CHUNK_X as f32 * (1.0 - 0.35 * gloom);

        SelectionFrame {
            playing,
            outline,
            outline_color,
            underwater,
            fog,
        }
    }
}
