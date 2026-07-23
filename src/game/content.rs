//! Texture-pack application and hot-reload orchestration.

use super::*;

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
            pack_source_of(&self.active_pack_id()),
            &self.content.reg.tex_names,
        );
        let season = if self.in_world {
            self.server.world.season()
        } else {
            1
        };
        atlas::season_tint(&mut atlas.color, atlas.px, season);
        self.presentation.atlas_season = season;
        self.renderer
            .set_atlas(&atlas.color, &atlas.material, &atlas.normal, atlas.px);
        self.content.pack_warnings = atlas.warnings;
        self.config.save();
    }

    /// Hot reload: rebuild the registry + atlas from disk, remap the live
    /// world and inventories by string id, recompile scripts.
    pub(super) fn reload_mods(&mut self, forced: bool) {
        let old = self.content.reg.clone();
        let new_reg = Arc::new(registry::load(std::path::Path::new("mods")));
        let mut atlas = atlas::build_atlas(
            &new_reg.tex_files,
            pack_source_of(&self.active_pack_id()),
            &new_reg.tex_names,
        );
        let season = if self.in_world {
            self.server.world.season()
        } else {
            1
        };
        atlas::season_tint(&mut atlas.color, atlas.px, season);
        self.presentation.atlas_season = season;
        self.content.pack_warnings = atlas.warnings;
        self.renderer
            .set_atlas(&atlas.color, &atlas.material, &atlas.normal, atlas.px);

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
        self.interaction
            .items
            .retain_mut(|e| match remap_item(&new_reg, e.item) {
                Some(item) => {
                    e.item = item;
                    true
                }
                None => false,
            });
        self.interaction.breaking = None;

        self.content.reg = new_reg.clone();
        self.server.world.reg = new_reg.clone();
        self.server.world.remap_from(&old);
        self.server.world.generator = worldgen::Generator::new(self.server.world.seed, &new_reg);
        self.content.scripts.load_mods(&script_mod_dirs(&new_reg));

        let errors: Vec<String> = new_reg
            .mods
            .iter()
            .filter_map(|m| m.error.clone())
            .chain(
                self.content
                    .scripts
                    .mods
                    .iter()
                    .filter_map(|m| m.error.clone()),
            )
            .collect();
        if errors.is_empty() {
            eprintln!(
                "mods: reloaded ({} blocks, {} items, {} recipes)",
                new_reg.blocks.len(),
                new_reg.items.len(),
                new_reg.recipes.len()
            );
            self.toast(format!(
                "mods reloaded ({} blocks, {} items, {} recipes)",
                new_reg.blocks.len(),
                new_reg.items.len(),
                new_reg.recipes.len()
            ));
        } else {
            for e in errors.iter().take(3) {
                self.toast(format!("mod error: {e}"));
            }
        }
        if forced {
            self.sfx(Sfx::Click);
        }
    }
}
