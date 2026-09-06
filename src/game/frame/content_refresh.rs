//! Content refresh in the graphical frame pipeline.

use crate::atlas;
use crate::game::Game;
use crate::game::content_watch::content_tree_stamp;

impl Game {
    pub(in crate::game) fn refresh_content_and_toasts(&mut self, dt: f32) {
        // The turning of the season repaints the leaves.
        let local_season = self
            .runtime
            .view()
            .season_at_surface(self.player.pos.surface());
        if self.in_world && local_season != self.presentation.atlas_season {
            let mut atlas = atlas::build_atlas(
                &self.content.reg.tex_files,
                &atlas::pack_chain(&self.active_pack_id()),
                &self.content.reg.tex_names,
            );
            atlas::season_tint(&mut atlas.color, atlas.px, local_season);
            self.presentation.atlas_season = local_season;
            self.content.pack_warnings = atlas.warnings;
            self.renderer.set_atlas(
                &atlas.color,
                &atlas.material,
                &atlas.normal,
                atlas.px,
                atlas.interior_base,
                &atlas.layer_params,
            );
        }

        // Hot reload: poll the mods + packs trees once a second.
        self.content.mods_poll += dt;
        if self.content.mods_poll >= 1.0 {
            self.content.mods_poll = 0.0;
            let stamp = content_tree_stamp();
            if stamp != self.content.mods_stamp {
                self.content.mods_stamp = stamp;
                self.reload_mods(false);
            }
        }
        for t in self.presentation.toasts.iter_mut() {
            t.1 -= dt;
        }
        self.presentation.toasts.retain(|t| t.1 > 0.0);
    }
}
