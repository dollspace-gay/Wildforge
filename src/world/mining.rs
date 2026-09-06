//! Mining coordinator for the authoritative world.

use super::{
    AIR, BlockBreak, BlockPos, ItemId, ItemStack, World, cancel_unapplied_material_operation,
};

impl World {
    pub fn break_block_at(
        &mut self,
        pos: BlockPos,
        tool: Option<ItemId>,
        award_drop: bool,
        affect_ire: bool,
    ) -> Option<BlockBreak> {
        // Spec 2.5: a sealed gate is unbreakable while its position is gated
        // and the gate def requires it. This is the world-level backstop —
        // scripts, commands, and remote hosts cannot bypass the seal even if
        // the UI layer is bypassed. (The interact path opens gates.)
        if let Some(gate) = self.gated.get(&pos).copied()
            && self
                .reg
                .gates
                .get(gate)
                .is_some_and(|g| g.unbreakable_when_locked)
        {
            return None;
        }
        // Spec 3.4: a hidden settlement growth cell is unbreakable — it does
        // not exist to the player until its tier is revealed.
        if self.is_hidden(pos) {
            return None;
        }
        let block = self.get_block_at(pos);
        if block == AIR || self.reg.block(block).hardness.is_none() {
            return None;
        }
        let tool_tier = tool
            .and_then(|item| self.reg.item(item).tool.map(|(_, _, tier)| tier))
            .unwrap_or(0);
        let block_definition = self.reg.block(block);
        let held_tool_kind = tool.and_then(|item| self.reg.item(item).tool.map(|tool| tool.0));
        let alchemy_apparatus = matches!(
            block_definition.interaction.as_deref(),
            Some("alchemy_mortar" | "alchemy_basin" | "alchemy_alembic" | "alchemy_filter")
        );
        let release_alchemy_installation = if alchemy_apparatus {
            let installed = self
                .alchemy_state
                .as_ref()
                .and_then(|state| state.apparatus.get(&pos));
            if installed.is_some_and(|apparatus| {
                apparatus.batch.is_some()
                    || !apparatus.residue_materials.is_empty()
                    || apparatus.filter_burden != 0
                    || apparatus.filter_medium.is_some()
            }) || self
                .alchemy_state
                .as_ref()
                .is_some_and(|state| state.ordinary_jobs.contains_key(&pos))
            {
                // A pick swing cannot orphan conserved liquid, residue,
                // filter, or a timed carrier job. The player must drain and
                // clean it first.
                return None;
            }
            installed.is_some()
        } else {
            false
        };
        let breaking_binding_frame =
            block_definition.interaction.as_deref() == Some("binding_frame");
        let controlled_frame_break = award_drop
            && held_tool_kind == block_definition.tool
            && (!block_definition.requires_tool || tool_tier >= block_definition.min_tier);
        if block_definition.interaction.as_deref() == Some("charge_vessel")
            && (held_tool_kind != block_definition.tool
                || block_definition.requires_tool && tool_tier < block_definition.min_tier)
        {
            // A placed vessel is an embodied container, not a free inventory
            // pickup. The correct dismantling tool returns its exact physical
            // item/identity through the block-entity spill path; bare hands or
            // an undersized tool leave it in place.
            return None;
        }
        let ecology_plan = self.arcane_geography.as_ref().and_then(|geography| {
            crate::arcane_ecology::plan_harvest(geography, &self.reg, pos, tool_tier)
        });
        let ecology_site_present = self.arcane_geography.as_ref().is_some_and(|geography| {
            crate::arcane_ecology::owns_materialized_block(geography, pos)
        });
        let dross_scar_present = self.owns_dross_scar_block(pos);
        // A recovering plant or crystal is real persistent state, not an
        // ordinary loot block. In particular, a second host command must not
        // bypass its harvest cooldown merely because no new plan is ready.
        if award_drop && ecology_site_present && ecology_plan.is_none() {
            return None;
        }
        if ecology_plan.as_ref().is_some_and(|plan| {
            plan.water_hu != 0
                && self.weather_state.live().is_some_and(|weather| {
                    let Some(atlas) = self.planet_atlas.as_ref() else {
                        return true;
                    };
                    weather.ecology_soil_water_hu(atlas.atlas_pos(pos.surface())) < plan.water_hu
                })
        }) {
            return None;
        }
        let is_finite_resonant_mineral =
            self.reg
                .block(block)
                .arcane_ecology
                .as_ref()
                .is_some_and(|definition| {
                    definition.kind == crate::registry::ArcaneEcologyKind::FiniteMineral
                });
        let mineral_plan = self
            .reg
            .block(block)
            .arcane_ecology
            .as_ref()
            .filter(|_| is_finite_resonant_mineral)
            .and_then(|definition| {
                self.arcane_geography.as_ref().and_then(|geography| {
                    self.planet_atlas.as_ref().and_then(|atlas| {
                        crate::arcane_ecology::plan_finite_mineral_harvest(
                            geography, atlas, pos, definition,
                        )
                    })
                })
            });
        let mut drop = award_drop
            .then(|| self.reg.drops_for(block, tool))
            .flatten()
            .map(|(item, count)| {
                let item = if tool.is_some() {
                    self.reg.block(block).dismantles_to.unwrap_or(item)
                } else {
                    item
                };
                ItemStack::new(&self.reg, item, count)
            });
        let material_operation = {
            let definition = self.reg.block(block);
            if let Some(ledger) = self.material_ledger.as_mut() {
                match ledger.begin_break(pos, &definition.name, &definition.materials) {
                    Ok(operation) => operation,
                    Err(error) => {
                        eprintln!("materials: block break cancelled at {pos:?}: {error}");
                        return None;
                    }
                }
            } else {
                None
            }
        };
        let mut dross_scar_handled = false;
        if dross_scar_present {
            let settled = if award_drop {
                if let Some(stack) = drop.as_mut() {
                    self.excavate_dross_scar(pos, stack)
                } else {
                    self.release_dross_scar(pos)
                }
            } else {
                self.release_dross_scar(pos)
            };
            if let Err(error) = settled {
                eprintln!("dross: scar break cancelled at {pos:?}: {error}");
                cancel_unapplied_material_operation(
                    self.material_ledger.as_ref(),
                    &material_operation,
                );
                return None;
            }
            dross_scar_handled = true;
        }
        if breaking_binding_frame
            && let Err(error) = self.settle_binding_frame_break_at(pos, controlled_frame_break)
        {
            eprintln!("implements: binding-frame break cancelled at {pos:?}: {error}");
            cancel_unapplied_material_operation(self.material_ledger.as_ref(), &material_operation);
            return None;
        }
        let mut arcane_harvest_handled =
            ecology_site_present || is_finite_resonant_mineral || dross_scar_handled;
        let mut leaves_bud = false;
        if award_drop {
            if let (Some(plan), Some(stack)) = (ecology_plan.as_ref(), drop.as_mut()) {
                let Some(geography) = self.arcane_geography.as_mut() else {
                    cancel_unapplied_material_operation(
                        self.material_ledger.as_ref(),
                        &material_operation,
                    );
                    return None;
                };
                let Some(site_index) = geography
                    .dynamic
                    .ecology
                    .sites
                    .iter()
                    .position(|site| site.id == plan.site_id)
                else {
                    cancel_unapplied_material_operation(
                        self.material_ledger.as_ref(),
                        &material_operation,
                    );
                    return None;
                };
                let old_site = geography.dynamic.ecology.sites[site_index].clone();
                let old_sequence = geography.dynamic.ecology.event_sequence;
                let old_exported = geography.dynamic.ecology.exported;
                let old_external_imported = geography.dynamic.dross_state.external_imported;
                if let Err(error) = crate::arcane_ecology::apply_harvest(
                    geography,
                    &self.reg,
                    plan,
                    u64::from(self.calendar_state.day()),
                ) {
                    eprintln!("arcane ecology: site changed during harvest at {pos:?}: {error}");
                    cancel_unapplied_material_operation(
                        self.material_ledger.as_ref(),
                        &material_operation,
                    );
                    return None;
                }
                let operation_id = geography.dynamic.ecology.event_sequence.max(1);
                let (manifest, files) = match geography
                    .linked_dynamic_replacements(&self.save_dir, operation_id)
                {
                    Ok(prepared) => prepared,
                    Err(error) => {
                        geography.dynamic.ecology.sites[site_index] = old_site;
                        geography.dynamic.ecology.event_sequence = old_sequence;
                        geography.dynamic.ecology.exported = old_exported;
                        geography.dynamic.dross_state.external_imported = old_external_imported;
                        eprintln!("arcane ecology: could not stage harvest at {pos:?}: {error}");
                        cancel_unapplied_material_operation(
                            self.material_ledger.as_ref(),
                            &material_operation,
                        );
                        return None;
                    }
                };
                let Some(ledger) = self.arcane_ledger.as_mut() else {
                    geography.dynamic.ecology.sites[site_index] = old_site;
                    geography.dynamic.ecology.event_sequence = old_sequence;
                    geography.dynamic.ecology.exported = old_exported;
                    geography.dynamic.dross_state.external_imported = old_external_imported;
                    eprintln!("arcane ecology: harvest cancelled without a ledger");
                    cancel_unapplied_material_operation(
                        self.material_ledger.as_ref(),
                        &material_operation,
                    );
                    return None;
                };
                let charged = !plan.current.is_empty() || !plan.dross_current.is_empty();
                let committed = if charged {
                    ledger
                        .bind_new_item_exact_linked(
                            crate::arcane::ArcaneOwner::Geography,
                            plan.current.clone(),
                            plan.dross_current.clone(),
                            &plan.item_content,
                            "ecological harvest",
                            files,
                        )
                        .map(Some)
                } else {
                    ledger
                        .commit_geography_state_linked("uncharged ecological harvest", files)
                        .map(|()| None)
                };
                match committed {
                    Ok(item_id) => {
                        if let Some(item_id) = item_id {
                            stack.arcane_id = item_id;
                        }
                        geography.accept_linked_manifest(manifest);
                    }
                    Err(error) => {
                        geography.dynamic.ecology.sites[site_index] = old_site;
                        geography.dynamic.ecology.event_sequence = old_sequence;
                        geography.dynamic.ecology.exported = old_exported;
                        geography.dynamic.dross_state.external_imported = old_external_imported;
                        eprintln!("arcane ecology: harvest cancelled at {pos:?}: {error}");
                        cancel_unapplied_material_operation(
                            self.material_ledger.as_ref(),
                            &material_operation,
                        );
                        return None;
                    }
                }
                leaves_bud = plan.leaves_bud;
                if plan.water_hu != 0
                    && let (Some(weather), Some(atlas)) =
                        (self.weather_state.live_mut(), self.planet_atlas.as_ref())
                {
                    let moved = weather
                        .harvest_ecology_water(atlas.atlas_pos(pos.surface()), plan.water_hu);
                    debug_assert_eq!(moved, plan.water_hu, "prechecked dew water changed");
                }
            } else if let (Some(current), Some(stack)) = (mineral_plan.as_ref(), drop.as_mut()) {
                let content_id = self.reg.item(stack.item).name.clone();
                let (Some(geography), Some(atlas)) =
                    (self.arcane_geography.as_mut(), self.planet_atlas.as_ref())
                else {
                    cancel_unapplied_material_operation(
                        self.material_ledger.as_ref(),
                        &material_operation,
                    );
                    return None;
                };
                let atlas_index = atlas.atlas_pos(pos.surface()).index(atlas.side());
                let old_cell = geography.dynamic.cells[atlas_index];
                let old_sequence = geography.dynamic.ecology.event_sequence;
                let old_exported = geography.dynamic.ecology.exported;
                let old_external_imported = geography.dynamic.dross_state.external_imported;
                if let Err(error) = crate::arcane_ecology::apply_finite_mineral_harvest(
                    geography, atlas, pos, current,
                ) {
                    eprintln!("arcane ecology: mineral charge changed at {pos:?}: {error}");
                    cancel_unapplied_material_operation(
                        self.material_ledger.as_ref(),
                        &material_operation,
                    );
                    return None;
                }
                geography.dynamic.ecology.event_sequence = old_sequence.saturating_add(1);
                let operation_id = geography.dynamic.ecology.event_sequence.max(1);
                let (manifest, files) =
                    match geography.linked_dynamic_replacements(&self.save_dir, operation_id) {
                        Ok(prepared) => prepared,
                        Err(error) => {
                            geography.dynamic.cells[atlas_index] = old_cell;
                            geography.dynamic.ecology.event_sequence = old_sequence;
                            geography.dynamic.ecology.exported = old_exported;
                            geography.dynamic.dross_state.external_imported = old_external_imported;
                            eprintln!("arcane ecology: could not stage mineral harvest: {error}");
                            cancel_unapplied_material_operation(
                                self.material_ledger.as_ref(),
                                &material_operation,
                            );
                            return None;
                        }
                    };
                let Some(ledger) = self.arcane_ledger.as_mut() else {
                    geography.dynamic.cells[atlas_index] = old_cell;
                    geography.dynamic.ecology.event_sequence = old_sequence;
                    geography.dynamic.ecology.exported = old_exported;
                    geography.dynamic.dross_state.external_imported = old_external_imported;
                    cancel_unapplied_material_operation(
                        self.material_ledger.as_ref(),
                        &material_operation,
                    );
                    return None;
                };
                match ledger.bind_new_item_exact_linked(
                    crate::arcane::ArcaneOwner::Geography,
                    current.clone(),
                    crate::arcane::Current::default(),
                    &content_id,
                    "finite resonant mineral harvest",
                    files,
                ) {
                    Ok(id) => {
                        stack.arcane_id = id;
                        geography.accept_linked_manifest(manifest);
                    }
                    Err(error) => {
                        geography.dynamic.cells[atlas_index] = old_cell;
                        geography.dynamic.ecology.event_sequence = old_sequence;
                        geography.dynamic.ecology.exported = old_exported;
                        geography.dynamic.dross_state.external_imported = old_external_imported;
                        eprintln!("arcane ecology: mineral harvest cancelled at {pos:?}: {error}");
                        cancel_unapplied_material_operation(
                            self.material_ledger.as_ref(),
                            &material_operation,
                        );
                        return None;
                    }
                }
            }
        }
        if ecology_site_present && (!award_drop || drop.is_none()) {
            if let Err(error) = self.settle_arcane_ecology_destruction(pos) {
                eprintln!("arcane ecology: destructive loss cancelled at {pos:?}: {error}");
                cancel_unapplied_material_operation(
                    self.material_ledger.as_ref(),
                    &material_operation,
                );
                return None;
            }
            arcane_harvest_handled = true;
        }
        self.player_touched.insert(pos.chunk());
        if affect_ire && self.ruleset().ire {
            let mut cost = self.ire_for_block(block);
            if let Some(plan) = &ecology_plan {
                cost += if plan.destructive { 2.0 } else { 0.35 };
                if plan.protected {
                    cost *= 0.5;
                }
            }
            self.add_ire_at_surface(pos.surface(), cost);
        }
        let was_heart = self.reg.block(block).name.starts_with("base:heart_");
        if release_alchemy_installation && let Some(state) = &mut self.alchemy_state {
            // A clean empty installation has no conserved contents. Its
            // sidecar identity is released with the ordinary block edit.
            state.apparatus.remove(&pos);
        }
        self.set_block_at(pos, if leaves_bud { block } else { AIR });
        if let Some(stack) = &mut drop
            && self.reg.item(stack.item).arcane.is_some()
            && !arcane_harvest_handled
            && let Err(error) = self.bind_arcane_stack_at(pos, stack, "magical harvest")
        {
            eprintln!("arcane: magical harvest drop cancelled: {error}");
            drop = None;
        }
        if let Some(operation) = material_operation {
            self.complete_material_operation(&operation);
            if !award_drop
                && let Some(ledger) = &mut self.material_ledger
                && let Err(error) = ledger.record_admin_deletion(&operation.materials)
            {
                eprintln!("materials: could not record creative/admin deletion: {error}");
            }
            if award_drop
                && drop.is_none()
                && let Some(ledger) = &mut self.material_ledger
                && let Err(error) =
                    ledger.bury_materials(pos, &operation.materials, "destructive block breaking")
            {
                eprintln!("materials: could not move destructive breakage to salvage: {error}");
            }
        }
        self.register_player_waterwork_at(pos);
        self.seep_into_excavation_at(pos);
        if was_heart {
            self.heart_struck_at(pos);
        }
        Some(BlockBreak { block, drop })
    }
}
