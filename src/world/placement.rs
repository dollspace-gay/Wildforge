//! Placement coordinator for the authoritative world.

use super::{
    AIR, BlockEntity, BlockId, BlockPos, ChargeVesselState, DepotState, ItemStack, MachineInstance,
    SurveyFolioState, World, soil,
};

impl World {
    #[cfg(test)]
    pub fn place_block(&mut self, pos: (i32, i32, i32), block: BlockId) -> bool {
        let Some(block_pos) = BlockPos::of_world(pos.0, pos.1, pos.2) else {
            return false;
        };
        self.place_block_at(block_pos, block)
    }

    pub fn place_block_at(&mut self, pos: BlockPos, block: BlockId) -> bool {
        if self.reg.blocks.get(block.0 as usize).is_none()
            || !self.reg.is_replaceable(self.get_block_at(pos))
        {
            return false;
        }
        let material_operation = {
            let before = self.reg.block(self.get_block_at(pos)).name.clone();
            let definition = self.reg.block(block);
            if let Some(ledger) = self.material_ledger.as_mut() {
                match ledger.begin_place(pos, &before, &definition.name, &definition.materials) {
                    Ok(operation) => operation,
                    Err(error) => {
                        eprintln!("materials: block placement cancelled at {pos:?}: {error}");
                        return false;
                    }
                }
            } else {
                None
            }
        };
        self.player_touched.insert(pos.chunk());
        // Soil arrives prepared. A block that carries fertility placed
        // at zero is dead ground that LOOKS tilled — it grows nothing
        // and it counts for nothing, which is a trap in either mode and
        // was the reason a hand-laid field around a dead heart did
        // absolutely nothing. Placed farmland is freshly-turned soil.
        if self.reg.block(block).fert_tiles.is_some() {
            let meta = soil::soil_meta(soil::FERT_TILL_GRASS, 0);
            self.set_block_meta_at(pos, block, meta);
            self.initialize_tilled_soil_at(pos);
        } else {
            self.set_block_at(pos, block);
        }
        if let Some(operation) = material_operation {
            self.complete_material_operation(&operation);
        }
        if self.reg.is_solid(block) {
            self.register_player_waterwork_at(pos);
        }
        // Industrial response gradient (capability E12): raising an
        // industrial building — any machine mouth — costs the valley a
        // little ire, once, alongside the machine's running feed.
        if self.ruleset().ire
            && self.ruleset().industrial_ire
            && self
                .reg
                .block(block)
                .interaction
                .as_deref()
                .is_some_and(|i| self.reg.machine_by_interaction(i).is_some())
        {
            self.add_ire_at_surface(pos.surface(), Self::INDUSTRIAL_BUILDING_IRE);
        }
        // Power sources carry a marker entity from birth so the
        // station sweep finds them without scanning the world.
        match self.reg.block(block).interaction.as_deref() {
            Some("wheel" | "sail" | "pump" | "generator") => {
                self.installations
                    .entry(pos)
                    .or_insert_with(|| BlockEntity::Anvil(Default::default()));
            }
            Some("firebox") => {
                self.installations
                    .entry(pos)
                    .or_insert_with(|| BlockEntity::Steam(Default::default()));
            }
            // Settlement depots (capability E13): `depot:<settlement id>`
            // binds the depot to that settlement's delivery needs.
            Some(interaction) if interaction.starts_with("depot:") => {
                let settlement = interaction.trim_start_matches("depot:").to_string();
                self.installations.entry(pos).or_insert_with(|| {
                    BlockEntity::Depot(DepotState {
                        settlement,
                        storage: Default::default(),
                    })
                });
            }
            Some(interaction)
                if self
                    .reg
                    .machine_by_interaction(interaction)
                    .is_some_and(|kind| {
                        self.reg
                            .machine(kind)
                            .is_some_and(|def| def.handler.hand_fed())
                    }) =>
            {
                let kind = self
                    .reg
                    .machine_by_interaction(interaction)
                    .expect("resolved above");
                self.installations.entry(pos).or_insert_with(|| {
                    BlockEntity::Multiblock(MachineInstance {
                        kind,
                        ..Default::default()
                    })
                });
            }
            Some("discovery_lab") => {
                self.installations
                    .entry(pos)
                    .or_insert_with(|| BlockEntity::DiscoveryApparatus(Default::default()));
            }
            Some("binding_frame") => {
                self.installations
                    .entry(pos)
                    .or_insert_with(|| BlockEntity::BindingFrame(Default::default()));
            }
            _ => {}
        }
        true
    }

    /// Place the block carried by one inventory instance. Charged placeables
    /// discharge into their declared local environmental reservoir when the
    /// physical item becomes a block; no item owner is left behind for a
    /// later save-recovery pass to clean up.
    pub fn place_item_block_at(&mut self, pos: BlockPos, stack: ItemStack) -> bool {
        let returns_ecology_water = self.reg.item(stack.item).name == "base:rainbell_dew";
        let Some(block) = self.reg.item(stack.item).places else {
            return false;
        };
        if !self.place_block_at(pos, block) {
            return false;
        }
        if self
            .reg
            .item(stack.item)
            .implement
            .as_ref()
            .is_some_and(|definition| {
                definition.kind == crate::implements::ImplementItemKind::ChargeVessel
            })
        {
            if stack.count != 1 {
                self.set_block_at(pos, AIR);
                return false;
            }
            self.installations.insert(
                pos,
                BlockEntity::ChargeVessel(ChargeVesselState {
                    vessel: Some(ItemStack { count: 1, ..stack }),
                    damage: 0,
                    revision: 0,
                }),
            );
            return true;
        }
        if self
            .reg
            .item(stack.item)
            .discovery
            .as_ref()
            .is_some_and(|definition| definition.kind == "survey_folio")
        {
            let mut physical = ItemStack { count: 1, ..stack };
            if let Err(error) = self.bind_discovery_stack_at(pos, &mut physical) {
                eprintln!("discovery: survey folio placement cancelled: {error}");
                self.set_block_at(pos, AIR);
                return false;
            }
            self.installations.insert(
                pos,
                BlockEntity::SurveyFolio(SurveyFolioState {
                    object_id: physical.arcane_id,
                }),
            );
            return true;
        }
        if let Some(definition) = self.reg.block(block).arcane_ecology.clone()
            && definition.kind != crate::registry::ArcaneEcologyKind::FiniteMineral
            && let (Some(atlas), Some(geography)) =
                (self.planet_atlas.as_ref(), self.arcane_geography.as_mut())
        {
            let content_id = &self.reg.block(block).name;
            let restored =
                crate::arcane_ecology::restore_with_seed(geography, &self.reg, content_id, pos);
            if !restored
                && let Err(error) = crate::arcane_ecology::register_cultivated(
                    geography,
                    atlas,
                    content_id,
                    pos,
                    &definition,
                )
            {
                eprintln!("arcane ecology: cultivation cancelled at {pos:?}: {error}");
                self.set_block_at(pos, AIR);
                return false;
            }
            if restored {
                self.plant_ire_at_surface(pos.surface(), 0.35);
            }
        }
        self.refresh_loaded_arcane_ecology();
        if returns_ecology_water
            && let (Some(atlas), Some(weather)) =
                (self.planet_atlas.as_ref(), self.weather_state.live_mut())
        {
            weather.return_ecology_water_to_soil(
                atlas.atlas_pos(pos.surface()),
                crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL,
            );
        }
        self.retire_arcane_stack_at(
            pos,
            ItemStack { count: 1, ..stack },
            "charged block placed into the environment",
        );
        true
    }
}
