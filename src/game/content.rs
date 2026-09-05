//! Texture-pack application and hot-reload orchestration.

use std::sync::Arc;
use super::{ContentRuntime, Game, script_mod_dirs};
use crate::{atlas, registry, visual_capture, worldgen};
use crate::audio::Sfx;
use crate::inventory::ItemStack;
use crate::registry::{Registry, ItemId};

#[derive(Debug, thiserror::Error)]
pub(super) enum RuntimeContentError {
    #[error(transparent)]
    Registry(#[from] registry::ContentErrors),
    #[error(transparent)]
    Scripts(#[from] crate::script::ScriptErrors),
}

impl ContentRuntime {
    /// The menu may inspect a rejected pack; world entry may not activate it.
    pub(super) fn validate(&self) -> Result<(), RuntimeContentError> {
        self.reg.validate()?;
        self.scripts.validate_loaded()?;
        Ok(())
    }
}

impl Game {
    /// The pack id in effect: the dev env override, else the config choice.
    pub(super) fn active_pack_id(&self) -> String {
        self.content
            .pack_override
            .clone()
            .unwrap_or_else(|| self.config.pack.clone())
    }

    /// Rebuild + swap the atlas for the currently selected texture pack and
    /// persist the choice. Registry/scripts are untouched — packs are art only.
    pub(super) fn apply_pack(&mut self) {
        let mut atlas = atlas::build_atlas(
            &self.content.reg.tex_files,
            &atlas::pack_chain(&self.active_pack_id()),
            &self.content.reg.tex_names,
        );
        let season = if self.in_world {
            self.server
                .world
                .season_at_surface(self.player.pos.surface())
        } else {
            1
        };
        atlas::season_tint(&mut atlas.color, atlas.px, season);
        self.presentation.atlas_season = season;
        self.renderer.set_atlas(
            &atlas.color,
            &atlas.material,
            &atlas.normal,
            atlas.px,
            atlas.interior_base,
            &atlas.layer_params,
        );
        self.content.pack_warnings = atlas.warnings;
        // Variant choice is baked into chunk uvs, so a pack whose alternates
        // differ (or vanish) leaves every mesh pointing at a slot the new pack
        // never filled. Compare before storing, then remesh the world.
        let changed = self.content.tile_variants.signature() != atlas.variants.signature();
        self.content.tile_variants = atlas.variants;
        if self.content.diagnostic_families.is_some() {
            self.content.diagnostic_families = Some(visual_capture::diagnostic_families(
                &self.content.reg,
                &self.content.tile_variants,
            ));
        }
        if changed && self.in_world {
            self.server.world.mark_all_chunks_dirty();
        }
        self.config.save();
    }

    fn report_reload_errors(&mut self, errors: &[String]) {
        for error in errors.iter().take(3) {
            eprintln!("mods: reload refused: {error}");
            self.toast(format!("reload refused: {error}"));
        }
    }

    /// Hot reload: rebuild the registry + atlas from disk, remap the live
    /// world and inventories by string id, recompile scripts.
    pub(super) fn reload_mods(&mut self, forced: bool) {
        let old = self.content.reg.clone();
        let new_reg = match registry::load_validated(std::path::Path::new("mods")) {
            Ok(reg) => Arc::new(reg),
            Err(errors) => {
                self.report_reload_errors(errors.diagnostics());
                return;
            }
        };
        if self.in_world {
            let accepted: Vec<String> = self.content.scripts.kv.borrow().values()
                .flat_map(|values| values.keys())
                .filter_map(|key| key.strip_prefix("quest_").map(str::to_string))
                .collect();
            if let Err(errors) = new_reg.validate_reload_from(&old, &accepted) {
                self.report_reload_errors(errors.diagnostics());
                return;
            }
        }
        let prepared_scripts = match self.content.scripts.prepare_mods(&script_mod_dirs(&new_reg)) {
            Ok(scripts) => scripts,
            Err(errors) => {
                self.report_reload_errors(errors.diagnostics());
                return;
            }
        };
        let mut atlas = atlas::build_atlas(
            &new_reg.tex_files,
            &atlas::pack_chain(&self.active_pack_id()),
            &new_reg.tex_names,
        );
        let season = if self.in_world {
            self.server
                .world
                .season_at_surface(self.player.pos.surface())
        } else {
            1
        };
        atlas::season_tint(&mut atlas.color, atlas.px, season);
        self.presentation.atlas_season = season;
        self.content.pack_warnings = atlas.warnings;
        // A reload can add or drop tiles, which reshuffles variant slots. No
        // explicit remesh needed: `remap_from` below dirties every chunk.
        self.content.tile_variants = atlas.variants;
        self.renderer.set_atlas(
            &atlas.color,
            &atlas.material,
            &atlas.normal,
            atlas.px,
            atlas.interior_base,
            &atlas.layer_params,
        );

        // Remap items by name (old registry -> new); unknown items vanish.
        let remap_item =
            |reg: &Registry, it: ItemId| -> Option<ItemId> { reg.item_id(&old.item(it).name) };
        let fix_stack = |reg: &Registry, s: Option<ItemStack>| -> Option<ItemStack> {
            s.and_then(|s| remap_item(reg, s.item).map(|item| ItemStack { item, ..s }))
        };
        for slot in self.inventory.slots.iter_mut() {
            *slot = fix_stack(&new_reg, *slot);
        }
        for slot in self.interaction.craft_grid.iter_mut() {
            *slot = fix_stack(&new_reg, *slot);
        }
        self.ui_state.held_stack = fix_stack(&new_reg, self.ui_state.held_stack);
        self.server
            .world
            .loose_items_mut()
            .retain_mut(|e| match remap_item(&new_reg, e.item) {
                Some(item) => {
                    e.item = item;
                    true
                }
                None => false,
            });
        self.interaction.breaking = None;

        self.content.reg = new_reg.clone();
        if let Some(remote) = self.multiplayer.remote.as_mut() {
            remote.session.rebind_content(Arc::clone(&new_reg));
        }
        if self.content.diagnostic_families.is_some() {
            self.content.diagnostic_families = Some(visual_capture::diagnostic_families(
                &new_reg,
                &self.content.tile_variants,
            ));
        }
        self.server.world.reg = new_reg.clone();
        self.server.world.remap_from(&old);
        self.server.world.generator = self.server.world.planet_atlas().map_or_else(
            || worldgen::Generator::new(self.server.world.seed, &new_reg),
            |atlas| worldgen::Generator::with_atlas(self.server.world.seed, &new_reg, atlas),
        );
        if let (Some(atlas), Some(ledger)) = (
            self.server.world.planet_atlas(),
            &mut self.server.world.material_ledger,
        ) && (ledger.reconcile_mod_manifests(&atlas, &new_reg)
            | ledger.reconcile_saved_definitions(&new_reg))
            && let Err(error) = ledger.save()
        {
            eprintln!("materials: could not persist hot-reload manifest: {error}");
        }
        self.content.scripts.install_prepared(prepared_scripts);

        eprintln!(
            "mods: reloaded ({} blocks, {} items, {} recipes)",
            new_reg.blocks.len(), new_reg.items.len(), new_reg.recipes.len()
        );
        self.toast(format!(
            "mods reloaded ({} blocks, {} items, {} recipes)",
            new_reg.blocks.len(), new_reg.items.len(), new_reg.recipes.len()
        ));
        if forced {
            self.sfx(Sfx::Click);
        }
    }
}
