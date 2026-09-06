//! Alchemy context interaction adapter.

use crate::audio::Sfx;
use crate::game::Game;
use crate::world::TerrainRead;

impl Game {
    /// Contextual laboratory controls keep every operation explicit while
    /// avoiding a shapeless crafting screen. Empty-hand mortar use selects a
    /// visible recipe; held ingredients/carriers/vessels perform their one
    /// physical action; empty hand controls transfer, heat, agitation,
    /// measured charge, and timed advancement.
    pub(in crate::game) fn operate_alchemy_contextual(&mut self, pos: crate::planet::BlockPos) {
        use crate::alchemy::{
            AgitationKind, ApparatusAction, ApparatusKind, BatchOutcome, ProcessStep,
        };

        let held_slot = self.input.hotbar_sel;
        let held = self.inventory.slots[held_slot];
        let apparatus = self.runtime.view().alchemy_apparatus_at(pos).cloned();
        let interaction = self
            .content
            .reg
            .block(self.runtime.view().get_block_at(pos))
            .interaction
            .as_deref();
        let kind = match interaction {
            Some("alchemy_mortar") => ApparatusKind::Mortar,
            Some("alchemy_basin") => ApparatusKind::InfusionBasin,
            Some("alchemy_alembic") => ApparatusKind::Alembic,
            Some("alchemy_filter") => ApparatusKind::FilterStand,
            _ => return,
        };

        let definitions = self.content.reg.preparations.values().collect::<Vec<_>>();
        if definitions.is_empty() {
            self.toast("This world has no registered preparation recipes.".into());
            return;
        }
        self.interaction.alchemy_recipe %= definitions.len();

        let Some(batch) = apparatus
            .as_ref()
            .and_then(|apparatus| apparatus.batch.as_ref())
        else {
            if let Some(job) = self.runtime.view().ordinary_alchemy_job_at(pos) {
                let action = match job.kind {
                    crate::alchemy::OrdinaryProcessKind::FermentAlcohol => {
                        ApparatusAction::FermentAlcohol {
                            water_slot: 0,
                            wheat_slot: 0,
                            berry_slot: 0,
                        }
                    }
                    crate::alchemy::OrdinaryProcessKind::PressOil => {
                        ApparatusAction::PressOil { seed_slot: 0 }
                    }
                };
                self.perform_alchemy_action(pos, action);
                return;
            }
            if let (Some(apparatus), Some(stack)) = (apparatus.as_ref(), held)
                && apparatus.integrity_permille < 1_000
            {
                let item_name = self.content.reg.item(stack.item).name.as_str();
                let matching = match kind {
                    ApparatusKind::Mortar => item_name == "base:cobblestone",
                    ApparatusKind::InfusionBasin => {
                        matches!(item_name, "base:glass" | "base:bronze_ingot")
                    }
                    ApparatusKind::Alembic => matches!(
                        item_name,
                        "base:glass" | "base:copper_ingot" | "base:bronze_ingot"
                    ),
                    ApparatusKind::FilterStand => {
                        matches!(item_name, "base:glass" | "base:filter_cloth")
                            || self
                                .content
                                .reg
                                .tags
                                .get("base:planks")
                                .is_some_and(|items| items.contains(&stack.item))
                    }
                };
                if matching {
                    self.perform_alchemy_action(
                        pos,
                        ApparatusAction::Repair {
                            material_slot: held_slot as u8,
                        },
                    );
                    return;
                }
            }
            if apparatus.as_ref().is_some_and(|apparatus| {
                !apparatus.residue_materials.is_empty()
                    || apparatus.filter_medium.is_some()
                    || apparatus.filter_burden != 0
            }) {
                if let Some(stack) = held
                    && self.content.reg.item(stack.item).name == "base:bucket_water"
                {
                    let filter_slot = self.inventory.slots.iter().position(|stack| {
                        stack.is_some_and(|stack| {
                            self.content.reg.item(stack.item).name == "base:filter_cloth"
                        })
                    });
                    self.perform_alchemy_action(
                        pos,
                        ApparatusAction::Clean {
                            water_slot: held_slot as u8,
                            filter_slot: filter_slot.map(|slot| slot as u8),
                        },
                    );
                } else {
                    self.toast(
                        "The apparatus holds physical residue. Hold a water bucket to clean it; a carried filter cloth captures the burden."
                            .into(),
                    );
                }
                return;
            }
            if kind == ApparatusKind::InfusionBasin
                && held.is_some_and(|stack| {
                    self.content.reg.item(stack.item).name == "base:bucket_water"
                })
            {
                let wheat_slot = self.inventory.slots.iter().position(|stack| {
                    stack.is_some_and(|stack| {
                        self.content.reg.item(stack.item).name == "base:wheat"
                            && stack.count >= 2
                            && stack.arcane_id == 0
                    })
                });
                let berry_slot = self.inventory.slots.iter().position(|stack| {
                    stack.is_some_and(|stack| {
                        self.content.reg.item(stack.item).name == "base:berry"
                            && stack.count >= 2
                            && stack.arcane_id == 0
                    })
                });
                if let (Some(wheat_slot), Some(berry_slot)) = (wheat_slot, berry_slot) {
                    self.perform_alchemy_action(
                        pos,
                        ApparatusAction::FermentAlcohol {
                            water_slot: held_slot as u8,
                            wheat_slot: wheat_slot as u8,
                            berry_slot: berry_slot as u8,
                        },
                    );
                } else {
                    self.toast(
                        "Fermentation needs this fresh-water bucket plus 2 wheat and 2 berries in your pack."
                            .into(),
                    );
                }
                return;
            }
            if kind == ApparatusKind::Mortar
                && held.is_some_and(|stack| {
                    self.content.reg.item(stack.item).name == "base:wheat_seeds"
                        && stack.count >= 4
                        && stack.arcane_id == 0
                })
            {
                self.perform_alchemy_action(
                    pos,
                    ApparatusAction::PressOil {
                        seed_slot: held_slot as u8,
                    },
                );
                return;
            }
            if kind != ApparatusKind::Mortar {
                self.perform_alchemy_action(pos, ApparatusAction::Inspect);
                return;
            }
            if held.is_none() {
                if self.interaction.alchemy_recipe_initialized {
                    self.interaction.alchemy_recipe =
                        (self.interaction.alchemy_recipe + 1) % definitions.len();
                } else {
                    self.interaction.alchemy_recipe_initialized = true;
                }
                let selected = definitions[self.interaction.alchemy_recipe];
                let ingredients = selected
                    .ingredients
                    .iter()
                    .map(|ingredient| format!("{}x {}", ingredient.count, ingredient.item))
                    .collect::<Vec<_>>()
                    .join(", ");
                self.toast(format!(
                    "Selected {} — ingredients: {ingredients}. Hold an ingredient and use the mortar to reserve this measured batch.",
                    selected.label
                ));
                self.sfx(Sfx::Click);
                return;
            }
            let selected = definitions[self.interaction.alchemy_recipe];
            self.perform_alchemy_action(
                pos,
                ApparatusAction::Begin {
                    preparation_id: selected.id.clone(),
                },
            );
            return;
        };

        let Some(definition) = self.content.reg.preparations.get(&batch.preparation_id) else {
            self.toast("This batch's preparation definition is no longer available.".into());
            return;
        };
        if held.is_some_and(|stack| self.content.reg.item(stack.item).name == "base:tuning_lens") {
            self.perform_alchemy_action(pos, ApparatusAction::Sample);
            return;
        }
        if !matches!(batch.outcome, BatchOutcome::Processing) {
            let empty_batch = batch.liquid.volume_units == 0
                && batch.current_units == 0
                && batch.dross_units == 0;
            if empty_batch
                && held.is_some_and(|stack| {
                    self.content.reg.item(stack.item).name == "base:bucket_water"
                })
            {
                let filter_slot = self.inventory.slots.iter().position(|stack| {
                    stack.is_some_and(|stack| {
                        self.content.reg.item(stack.item).name == "base:filter_cloth"
                    })
                });
                self.perform_alchemy_action(
                    pos,
                    ApparatusAction::Clean {
                        water_slot: held_slot as u8,
                        filter_slot: filter_slot.map(|slot| slot as u8),
                    },
                );
            } else if empty_batch {
                self.toast(
                    "The batch is empty but its residue remains. Hold a fresh-water bucket to clean and recover the physical waste."
                        .into(),
                );
            } else if let Some(stack) = held
                && self.content.reg.item(stack.item).name == definition.empty_vessel
            {
                self.perform_alchemy_action(
                    pos,
                    ApparatusAction::Decant {
                        vessel_slot: held_slot as u8,
                    },
                );
            } else if batch.liquid.volume_units < definition.dose_units && self.input.keys.sprint {
                self.perform_alchemy_action(
                    pos,
                    ApparatusAction::Drain {
                        route: crate::alchemy::DisposalRoute::Runoff,
                    },
                );
            } else if batch.liquid.volume_units < definition.dose_units {
                self.toast(
                    "Less than one dose remains. Hold Ctrl while using the apparatus to drain it into local runoff, with pollution attributed."
                        .into(),
                );
            } else {
                self.toast(format!(
                    "{} is ready. Hold a {} to decant one exact dose.",
                    definition.label, definition.empty_vessel
                ));
            }
            return;
        }

        let Some(step) = definition.steps.get(usize::from(batch.step_index)).copied() else {
            self.perform_alchemy_action(pos, ApparatusAction::Inspect);
            return;
        };
        let required_kind = match step {
            ProcessStep::Grind => ApparatusKind::Mortar,
            ProcessStep::Filter => ApparatusKind::FilterStand,
            ProcessStep::Distill => ApparatusKind::Alembic,
            _ => definition.process.apparatus(),
        };
        if kind != required_kind {
            let destination = self.runtime.view().idle_apparatus_near(pos, required_kind);
            if let Some(destination) = destination {
                self.perform_alchemy_action(pos, ApparatusAction::TransferMash { destination });
            } else {
                self.toast(format!(
                    "The visible {step:?} step needs a nearby {required_kind:?}. Use that apparatus once to register it, then transfer here."
                ));
            }
            return;
        }
        if step == ProcessStep::Filter
            && apparatus
                .as_ref()
                .is_some_and(|apparatus| apparatus.filter_medium.is_none())
        {
            if let Some(stack) = held
                && matches!(
                    self.content.reg.item(stack.item).name.as_str(),
                    "base:ashlace_tissue"
                        | "base:filter_cloth"
                        | "base:charcoal"
                        | "base:still_salt"
                )
            {
                self.perform_alchemy_action(
                    pos,
                    ApparatusAction::LoadFilter {
                        inventory_slot: held_slot as u8,
                    },
                );
            } else {
                self.toast(
                    "Mount Ashlace, filter cloth, charcoal, or still salt before passing the batch through this stand."
                        .into(),
                );
            }
            return;
        }

        if matches!(
            step,
            ProcessStep::Agitate | ProcessStep::Settle | ProcessStep::Distill | ProcessStep::Filter
        ) && apparatus.as_ref().is_some_and(|apparatus| {
            !(definition.temperature_millic[0]..=definition.temperature_millic[1])
                .contains(&apparatus.temperature_millic)
        }) {
            self.perform_alchemy_action(
                pos,
                ApparatusAction::SetHeat {
                    temperature_millic: definition.temperature_millic[0]
                        + (definition.temperature_millic[1] - definition.temperature_millic[0]) / 2,
                },
            );
            return;
        }

        let action = match step {
            ProcessStep::Grind => held.map(|_| ApparatusAction::Grind {
                inventory_slot: held_slot as u8,
            }),
            ProcessStep::Load => held.map(|_| ApparatusAction::LoadCarrier {
                inventory_slot: held_slot as u8,
            }),
            ProcessStep::Heat => {
                let apparatus = apparatus.as_ref().expect("batch came from apparatus");
                if (definition.temperature_millic[0]..=definition.temperature_millic[1])
                    .contains(&apparatus.temperature_millic)
                {
                    Some(ApparatusAction::Advance { step })
                } else {
                    Some(ApparatusAction::SetHeat {
                        temperature_millic: definition.temperature_millic[0]
                            + (definition.temperature_millic[1] - definition.temperature_millic[0])
                                / 2,
                    })
                }
            }
            ProcessStep::Agitate => {
                let apparatus = apparatus.as_ref().expect("batch came from apparatus");
                if apparatus.agitation == definition.agitation {
                    Some(ApparatusAction::Advance { step })
                } else {
                    Some(ApparatusAction::SetAgitation {
                        agitation: definition.agitation,
                    })
                }
            }
            ProcessStep::Charge => {
                let remaining = definition
                    .charge_units
                    .saturating_sub(batch.charge_input_units);
                let units = remaining.min(u64::from(definition.charge_rate[1]));
                Some(ApparatusAction::Charge {
                    inventory_slot: held.map(|_| held_slot as u8),
                    units,
                })
            }
            ProcessStep::Cool => {
                let apparatus = apparatus.as_ref().expect("batch came from apparatus");
                if apparatus.temperature_millic <= definition.storage_temperature_millic[1] {
                    Some(ApparatusAction::Advance { step })
                } else {
                    Some(ApparatusAction::SetHeat {
                        temperature_millic: definition.storage_temperature_millic[1],
                    })
                }
            }
            ProcessStep::Settle | ProcessStep::Distill | ProcessStep::Filter => {
                let now = (self.runtime.view().clock().max(0.0) * 20.0).round() as u64;
                if now < batch.due_tick {
                    let seconds = (batch.due_tick - now).div_ceil(20);
                    self.toast(format!(
                        "{} still needs about {seconds} seconds in its controlled interval.",
                        definition.label
                    ));
                    None
                } else if step == ProcessStep::Settle
                    && apparatus
                        .as_ref()
                        .is_some_and(|apparatus| apparatus.agitation != AgitationKind::Still)
                {
                    Some(ApparatusAction::SetAgitation {
                        agitation: AgitationKind::Still,
                    })
                } else {
                    Some(ApparatusAction::Advance { step })
                }
            }
        };
        if let Some(action) = action {
            self.perform_alchemy_action(pos, action);
        } else if held.is_none() && matches!(step, ProcessStep::Grind | ProcessStep::Load) {
            self.toast(format!(
                "The visible next step is {step:?}; hold its declared input."
            ));
        }
    }
}
