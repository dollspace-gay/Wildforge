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

    /// Hot reload: rebuild the registry + atlas from disk, remap the live
    /// world and inventories by string id, recompile scripts.
    pub(super) fn reload_mods(&mut self, forced: bool) {
        let old = self.content.reg.clone();
        let new_reg = Arc::new(registry::load(std::path::Path::new("mods")));
        let mut migration_errors = new_reg.material_errors.clone();
        if self.in_world {
            for old_item in &old.items {
                match new_reg.item_id(&old_item.name) {
                    Some(item) if new_reg.item(item).materials != old_item.materials => {
                        migration_errors.push(format!(
                            "{} changes live-stack material identity; a migration is required",
                            old_item.name
                        ));
                    }
                    None if !old_item.materials.is_empty() => migration_errors.push(format!(
                        "{} contains finite material and cannot be removed from a live world",
                        old_item.name
                    )),
                    _ => {}
                }
            }
            for old_block in &old.blocks {
                match new_reg.block_id(&old_block.name) {
                    Some(block) if new_reg.block(block).materials != old_block.materials => {
                        migration_errors.push(format!(
                            "{} changes live-voxel material identity; a migration is required",
                            old_block.name
                        ));
                    }
                    None if !old_block.materials.is_empty() => migration_errors.push(format!(
                        "{} contains finite material and requires a persistent placeholder",
                        old_block.name
                    )),
                    _ => {}
                }
            }
        }
        if !migration_errors.is_empty() {
            for error in migration_errors.iter().take(3) {
                eprintln!("mods: reload refused: {error}");
                self.toast(format!("reload refused: {error}"));
            }
            return;
        }
        // Quest defs (spec 3.3): an accepted quest may never be removed from
        // a live world, or its progress is orphaned. State lives in the KV
        // by quest id; check every `quest_<id>` marker against the new defs.
        if self.in_world {
            let accepted: Vec<String> = self
                .content
                .scripts
                .kv
                .borrow()
                .values()
                .flat_map(|m| m.keys())
                .filter_map(|k| k.strip_prefix("quest_").map(str::to_string))
                .collect();
            for id in accepted {
                if !new_reg.quests.iter().any(|q| q.id == id) {
                    migration_errors.push(format!(
                        "{id} is accepted in this world and its quest def cannot be removed"
                    ));
                }
            }
        }
        if !migration_errors.is_empty() {
            for error in migration_errors.iter().take(3) {
                eprintln!("mods: reload refused: {error}");
                self.toast(format!("reload refused: {error}"));
            }
            return;
        }
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
