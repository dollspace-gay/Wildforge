//! Host-authoritative embodied preparation operations.
use crate::alchemy::AgitationKind;
use crate::alchemy::AlchemyApparatusState;
use crate::alchemy::AlchemyBatch;
use crate::alchemy::AlchemyCue;
use crate::alchemy::AlchemyCueKind;
use crate::alchemy::AlchemyError;
use crate::alchemy::AlchemyResult;
use crate::alchemy::ApparatusKind;
use crate::arcane::ArcaneOwner;
use std::collections::BTreeMap;
use crate::alchemy::BatchFailure;
use crate::alchemy::BatchIngredientState;
use crate::alchemy::BatchOutcome;
use crate::world::BlockPos;
use crate::alchemy::CarrierKind;
use crate::arcane::Current;
use crate::alchemy::ExactLiquid;
use crate::inventory::Inventory;
use crate::inventory::ItemStack;
use crate::registry::MaterialVector;
use crate::alchemy::PreparationHandler;
use crate::alchemy::ProcessStep;
use crate::alchemy::ProducedStack;
use crate::planet_atlas::ReservoirMass;
use crate::workings::WaterCarrier;

const CARRIER_ITEM_UNITS: u64 = 64;
const APPARATUS_REACH: i32 = 4;

/// A successful still run moves the exact salt mass out of brine without
/// creating or deleting a single hydro unit. The salt remains physically in
/// the apparatus residue until cleaning or disposal returns it through the
/// ordinary water/material paths; the bottled condensate is fresh water.

/// Dissolved physical ingredients may displace a declared amount of liquid
/// volume without creating water or carrier matter. Keep that displacement
/// explicit in the solution's volume, heat, and named-solute state so a mod
/// recipe admitted by validation can actually yield every declared dose.

mod liquid_materials;
use liquid_materials::{separate_brine_distillate, proportional_units, add_dissolved_displacement, take_material_fraction, split_materials, add_materials};
mod failure_outcomes;
use failure_outcomes::{process_failure};
mod inventory_helpers;
use inventory_helpers::{take_count, produced, take_exact_slot};
mod status_rules;
use status_rules::{incompatible_status_groups, settle_immediate_current, preparation_color, debit_nutrition};
mod apparatus_helpers;
use apparatus_helpers::{add_current_map, next_block_key, next_u64_key, ensure_apparatus, near, result_for};

mod observation;
mod container_custody;
mod dispatch;
mod commit;
mod carrier_load;
mod filter_load;
mod processing;
mod charge;
mod decant;
mod repair;
mod clean;
mod drain;
mod ordinary_carriers;
mod preparation_use;
mod water_return;
mod status_tick;
mod apparatus_tick;
mod batch_input;
mod grind;

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_batch(definition: &crate::alchemy::PreparationDef) -> AlchemyBatch {
        AlchemyBatch {
            id: 1,
            preparation_id: definition.id.clone(),
            definition_version: definition.version,
            actor: [7; 16],
            actor_label: "fixture".into(),
            installation_id: 1,
            liquid: ExactLiquid::default(),
            residue_water: ReservoirMass::default(),
            ingredients: vec![BatchIngredientState {
                item: definition.ingredients[0].item.clone(),
                count: definition.ingredients[0].count,
                condition_permille: 1_000,
                retained_materials: MaterialVector::new(),
                residue_materials: MaterialVector::new(),
                source_arcane_ids: Vec::new(),
            }],
            charge_input_units: definition.charge_units,
            current_units: definition
                .charge_units
                .saturating_sub(definition.dross_units),
            dross_units: definition.dross_units,
            step_index: 0,
            observations: Vec::new(),
            started_tick: 10,
            due_tick: 20,
            born_tick: 10,
            expires_tick: 1_000,
            outcome: BatchOutcome::Processing,
            revision: 1,
        }
    }

    #[test]
    fn every_named_failure_is_deterministic_and_has_a_physical_output() {
        let registry = crate::registry::load(std::path::Path::new("/nonexistent-alchemy-mods"));
        let definition = registry.preparations.get("base:hearth_tonic").unwrap();
        let batch = fixture_batch(definition);
        let midpoint = definition.temperature_millic[0]
            + (definition.temperature_millic[1] - definition.temperature_millic[0]) / 2;

        assert_eq!(
            process_failure(
                definition,
                midpoint,
                definition.agitation,
                definition.cleanliness_min.saturating_sub(1),
                &batch,
                ProcessStep::Settle,
                batch.due_tick,
            ),
            Some(BatchFailure::FouledBatch)
        );
        let mut overcharged = batch.clone();
        overcharged.charge_input_units += 1;
        assert_eq!(
            process_failure(
                definition,
                midpoint,
                definition.agitation,
                1_000,
                &overcharged,
                ProcessStep::Settle,
                batch.due_tick,
            ),
            Some(BatchFailure::OverchargedBatch)
        );
        let mut spent = batch.clone();
        spent.charge_input_units -= 1;
        assert_eq!(
            process_failure(
                definition,
                midpoint,
                definition.agitation,
                1_000,
                &spent,
                ProcessStep::Charge,
                batch.due_tick,
            ),
            Some(BatchFailure::SpentLiquor)
        );
        assert_eq!(
            process_failure(
                definition,
                definition.temperature_millic[1] + 1,
                definition.agitation,
                1_000,
                &batch,
                ProcessStep::Heat,
                batch.due_tick,
            ),
            Some(BatchFailure::ScorchedMash)
        );
        assert_eq!(
            process_failure(
                definition,
                definition.temperature_millic[0] - 1,
                definition.agitation,
                1_000,
                &batch,
                ProcessStep::Heat,
                batch.due_tick,
            ),
            Some(BatchFailure::WeakExtraction)
        );
        let wrong_agitation = match definition.agitation {
            AgitationKind::Still => AgitationKind::Shaken,
            AgitationKind::Stirred | AgitationKind::Shaken => AgitationKind::Still,
        };
        assert_eq!(
            process_failure(
                definition,
                midpoint,
                wrong_agitation,
                1_000,
                &batch,
                ProcessStep::Agitate,
                batch.due_tick,
            ),
            Some(BatchFailure::BrokenEmulsion)
        );
        let mut stale = batch.clone();
        stale.ingredients[0].condition_permille = 249;
        assert_eq!(
            process_failure(
                definition,
                midpoint,
                definition.agitation,
                1_000,
                &stale,
                ProcessStep::Settle,
                batch.due_tick,
            ),
            Some(BatchFailure::WeakExtraction)
        );

        for failure in BatchFailure::ALL {
            let item = registry.item_id(failure.item_id()).unwrap();
            assert_eq!(registry.item(item).max_stack, 1);
            assert!(registry.item(item).arcane.is_some());
        }
    }

    #[test]
    fn valid_declared_steps_have_no_hidden_failure_roll() {
        let registry = crate::registry::load(std::path::Path::new("/nonexistent-alchemy-mods"));
        for definition in registry.preparations.values() {
            let batch = fixture_batch(definition);
            let midpoint = definition.temperature_millic[0]
                + (definition.temperature_millic[1] - definition.temperature_millic[0]) / 2;
            for step in &definition.steps {
                assert_eq!(
                    process_failure(
                        definition,
                        if *step == ProcessStep::Cool {
                            definition.storage_temperature_millic[1]
                        } else {
                            midpoint
                        },
                        definition.agitation,
                        1_000,
                        &batch,
                        *step,
                        batch.due_tick,
                    ),
                    None,
                    "{} failed a valid {:?} control",
                    definition.id,
                    step
                );
            }
        }
    }

    #[test]
    fn brine_distillation_keeps_exact_water_and_leaves_exact_salt_residue() {
        let registry = crate::registry::load(std::path::Path::new(
            "/nonexistent-alchemy-brine-distillation-mods",
        ));
        let definition = registry.preparations.get("base:storm_cordial").unwrap();
        let mut batch = fixture_batch(definition);
        batch.liquid = ExactLiquid {
            carrier: Some(CarrierKind::Brine),
            volume_units: 257,
            water: ReservoirMass {
                water_hu: 256,
                salt_mass: 65_537,
            },
            carrier_state: WaterCarrier {
                thermal_millic_hu: 18_000 * 257,
                dross_subunits: 13,
            },
            solutes: BTreeMap::from([("base:bucket_salt".into(), 257)]),
        };
        batch.residue_water = ReservoirMass {
            water_hu: 7,
            salt_mass: 11,
        };
        let water_before = batch.liquid.water.water_hu + batch.residue_water.water_hu;
        let salt_before = batch.liquid.water.salt_mass + batch.residue_water.salt_mass;

        assert_eq!(separate_brine_distillate(&mut batch).unwrap(), 65_537);
        assert_eq!(batch.liquid.carrier, Some(CarrierKind::FreshWater));
        assert_eq!(batch.liquid.water.salt_mass, 0);
        assert_eq!(batch.residue_water.salt_mass, salt_before);
        assert_eq!(
            batch.liquid.water.water_hu + batch.residue_water.water_hu,
            water_before
        );
        assert_eq!(
            batch.liquid.water.salt_mass + batch.residue_water.salt_mass,
            salt_before
        );
        batch.liquid.validate().unwrap();
    }

    #[test]
    fn declared_dissolved_matter_displaces_volume_without_conjuring_water() {
        let registry = crate::registry::load(std::path::Path::new(
            "/nonexistent-alchemy-displacement-mods",
        ));
        let mut definition = registry.preparations["base:hearth_tonic"].clone();
        definition.dissolved_units = 3;
        let mut liquid = ExactLiquid {
            carrier: Some(CarrierKind::FreshWater),
            volume_units: definition.solvent_units,
            water: ReservoirMass::fresh(definition.solvent_units),
            carrier_state: WaterCarrier {
                thermal_millic_hu: i64::try_from(definition.solvent_units).unwrap() * 20_000,
                dross_subunits: 0,
            },
            solutes: BTreeMap::from([(definition.solvent_item.clone(), definition.solvent_units)]),
        };
        let water_before = liquid.water;
        add_dissolved_displacement(&mut liquid, &definition, 20_000).unwrap();
        assert_eq!(
            liquid.volume_units,
            definition.solvent_units + definition.dissolved_units
        );
        assert_eq!(liquid.water, water_before);
        assert_eq!(
            definition
                .ingredients
                .iter()
                .map(|ingredient| liquid.solutes.get(&ingredient.item).copied().unwrap_or(0))
                .sum::<u64>(),
            definition.dissolved_units
        );
        liquid.validate().unwrap();
    }
}
