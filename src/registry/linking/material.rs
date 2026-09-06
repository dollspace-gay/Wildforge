//! Material class inference and salvage declaration validation.

use crate::registry::schema::SalvageToml;
use crate::registry::{MaterialClass, SalvageDef};

pub(in crate::registry) fn inferred_material_class(name: &str) -> MaterialClass {
    let local = name.rsplit(':').next().unwrap_or(name);
    if local.contains("heart")
        || local.contains("charm")
        || local.contains("ember")
        || local.contains("frost")
        || local.contains("living_")
    {
        MaterialClass::Exceptional
    } else if local.contains("coal")
        || local.contains("charcoal")
        || local.contains("fuel")
        || local.contains("food")
        || local.contains("bread")
    {
        MaterialClass::Consumptive
    } else if local.contains("ore")
        || local.starts_with("raw_")
        || local.contains("ingot")
        || local.contains("metal")
        || local.contains("diamond")
        || local.contains("monazite")
        || local.contains("bastnasite")
    {
        MaterialClass::GeologicallyFinite
    } else if local.contains("stone")
        || local.contains("sand")
        || local.contains("clay")
        || local.contains("glass")
        || local.contains("brick")
        || local.contains("ceramic")
        || local.contains("gravel")
        || local.contains("dirt")
    {
        MaterialClass::TransformativeFinite
    } else {
        MaterialClass::Renewable
    }
}

pub(in crate::registry) fn salvage_def(
    raw: &Option<SalvageToml>,
    errs: &mut Vec<String>,
    name: &str,
) -> Option<SalvageDef> {
    raw.as_ref().map(|salvage| {
        if !(0.0..=1.0).contains(&salvage.recovery) {
            errs.push(format!("{name}: salvage recovery must be between 0 and 1"));
        }
        SalvageDef {
            station: salvage.station.clone(),
            recovery_permille: (salvage.recovery.clamp(0.0, 1.0) * 1000.0).round() as u16,
        }
    })
}
