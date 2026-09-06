//! Liquid materials shared alchemy rules.

use crate::alchemy::AlchemyBatch;
use crate::alchemy::CarrierKind;
use crate::alchemy::ExactLiquid;
use crate::registry::MaterialVector;

pub(super) fn separate_brine_distillate(batch: &mut AlchemyBatch) -> Result<u64, String> {
    if batch.liquid.carrier != Some(CarrierKind::Brine) {
        return Ok(0);
    }
    let salt_mass = batch.liquid.water.salt_mass;
    batch.residue_water.salt_mass = batch
        .residue_water
        .salt_mass
        .checked_add(salt_mass)
        .ok_or("Distillation salt residue overflowed its exact custody.")?;
    batch.liquid.water.salt_mass = 0;
    batch.liquid.carrier = Some(CarrierKind::FreshWater);
    batch.liquid.validate().map_err(|error| error.to_string())?;
    Ok(salt_mass)
}

pub(super) fn proportional_units(
    total: u64,
    requested_volume: u64,
    before_volume: u64,
) -> Result<u64, String> {
    if requested_volume > before_volume || before_volume == 0 {
        return Err("Invalid exact-volume proportion.".into());
    }
    if requested_volume == before_volume {
        return Ok(total);
    }
    u64::try_from(u128::from(total) * u128::from(requested_volume) / u128::from(before_volume))
        .map_err(|_| "Exact-volume proportion overflowed.".into())
}

pub(super) fn add_dissolved_displacement(
    liquid: &mut ExactLiquid,
    definition: &crate::alchemy::PreparationDef,
    temperature_millic: i32,
) -> Result<(), String> {
    let displaced = definition.dissolved_units;
    if displaced == 0 {
        return Ok(());
    }
    let before = liquid.volume_units;
    if before != definition.solvent_units || liquid.carrier != Some(definition.carrier) {
        return Err("Dissolved displacement needs the complete declared carrier first.".into());
    }
    liquid.volume_units = before
        .checked_add(displaced)
        .filter(|volume| *volume <= crate::alchemy::MAX_BATCH_VOLUME_UNITS)
        .ok_or("Dissolved displacement overflowed the batch vessel.")?;
    liquid.carrier_state.thermal_millic_hu = liquid
        .carrier_state
        .thermal_millic_hu
        .checked_add(
            i64::from(temperature_millic)
                .checked_mul(
                    i64::try_from(displaced)
                        .map_err(|_| "Dissolved displacement heat overflowed.")?,
                )
                .ok_or("Dissolved displacement heat overflowed.")?,
        )
        .ok_or("Dissolved displacement heat overflowed.")?;
    let total_parts = definition
        .ingredients
        .iter()
        .map(|ingredient| u64::from(ingredient.count))
        .sum::<u64>();
    if total_parts == 0 {
        return Err("Dissolved displacement has no physical ingredient source.".into());
    }
    let mut assigned = 0u64;
    for (index, ingredient) in definition.ingredients.iter().enumerate() {
        let units = if index + 1 == definition.ingredients.len() {
            displaced.saturating_sub(assigned)
        } else {
            u64::try_from(
                u128::from(displaced) * u128::from(ingredient.count) / u128::from(total_parts),
            )
            .map_err(|_| "Dissolved displacement split overflowed.")?
        };
        assigned = assigned
            .checked_add(units)
            .ok_or("Dissolved displacement split overflowed.")?;
        if units != 0 {
            let entry = liquid.solutes.entry(ingredient.item.clone()).or_default();
            *entry = entry
                .checked_add(units)
                .ok_or("Dissolved solute custody overflowed.")?;
        }
    }
    liquid.validate().map_err(|error| error.to_string())
}

pub(super) fn take_material_fraction(
    source: &mut MaterialVector,
    requested_volume: u64,
    before_volume: u64,
) -> Result<MaterialVector, String> {
    let mut parcel = MaterialVector::new();
    for (name, remaining) in source.iter_mut() {
        let moved = proportional_units(*remaining, requested_volume, before_volume)?;
        *remaining -= moved;
        if moved != 0 {
            parcel.insert(name.clone(), moved);
        }
    }
    source.retain(|_, units| *units != 0);
    Ok(parcel)
}

pub(super) fn split_materials(
    materials: &MaterialVector,
    retention_permille: u16,
) -> (MaterialVector, MaterialVector) {
    let mut retained = MaterialVector::new();
    let mut residue = MaterialVector::new();
    for (name, units) in materials {
        let kept = u64::try_from(u128::from(*units) * u128::from(retention_permille) / 1_000)
            .unwrap_or(*units);
        if kept != 0 {
            retained.insert(name.clone(), kept);
        }
        if *units != kept {
            residue.insert(name.clone(), *units - kept);
        }
    }
    (retained, residue)
}

pub(super) fn add_materials(
    into: &mut MaterialVector,
    from: &MaterialVector,
) -> Result<(), String> {
    for (name, units) in from {
        let value = into
            .get(name)
            .copied()
            .unwrap_or_default()
            .checked_add(*units)
            .ok_or("Alchemy material custody overflowed.")?;
        into.insert(name.clone(), value);
    }
    Ok(())
}
