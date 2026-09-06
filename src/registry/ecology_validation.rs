//! Validate bounded dross-scar and magical ecology content graphs.

use super::{ArcaneDisposition, ArcaneEcologyKind, EcologyRole, MaterialClass, Registry};
use std::collections::BTreeMap;

pub(super) fn validate_dross_scar_graph(reg: &mut Registry) {
    const MAX_DROSS_SCAR_DEFINITIONS: usize = 4_096;
    let mut errors = Vec::new();
    if reg.dross_scars.len() > MAX_DROSS_SCAR_DEFINITIONS {
        errors.push(format!(
            "dross scar registry exceeds its {MAX_DROSS_SCAR_DEFINITIONS}-definition safety bound"
        ));
    }
    for definition in reg.dross_scars.values() {
        let block = reg.block(definition.block);
        let has_scar_observation = block.observation.as_ref().is_some_and(|observation| {
            observation
                .categories
                .iter()
                .any(|category| category == "scar")
                && observation
                    .properties
                    .iter()
                    .any(|property| property == "dross")
        });
        if block.solid
            || block.opaque
            || block.interaction.is_some()
            || block.water_level.is_some()
            || block.hardness.is_none()
            || block.height.is_some_and(|height| height > 0.25)
            || !has_scar_observation
        {
            errors.push(format!(
                "{}: a scar must be removable, nonstructural, non-fluid, inventory-free, at most quarter-height, and visibly categorized as scar/dross",
                definition.content_id
            ));
        }
        let Some((drop, count)) = block.drops else {
            errors.push(format!(
                "{}: a scar lifecycle needs one recoverable contained drop",
                definition.content_id
            ));
            continue;
        };
        let drop = reg.item(drop);
        if count != 1
            || drop.max_stack != 1
            || drop.arcane.as_ref().is_none_or(|arcane| {
                arcane.capacity == 0
                    || !matches!(
                        arcane.on_destroy,
                        ArcaneDisposition::Dross | ArcaneDisposition::Scar
                    )
            })
        {
            errors.push(format!(
                "{}: scar recovery must yield exactly one finite-capacity, non-erasing arcane item",
                definition.content_id
            ));
        }
        if definition.handler == crate::dross::ScarHandler::WaterMarginFilm
            && !definition
                .carriers
                .contains(&crate::dross::DrossCarrier::Water)
        {
            errors.push(format!(
                "{}: a water-margin film must accept waterborne dross",
                definition.content_id
            ));
        }
        if definition.handler == crate::dross::ScarHandler::MineralCrust
            && !definition
                .carriers
                .contains(&crate::dross::DrossCarrier::Soil)
        {
            errors.push(format!(
                "{}: a mineral crust must accept soil/sediment dross",
                definition.content_id
            ));
        }
    }
    for kind in crate::dross::ScarKind::ALL {
        if !reg
            .dross_scars
            .values()
            .any(|definition| definition.provider == "base" && definition.kind == kind)
        {
            errors.push(format!(
                "base content needs a safe fallback dross scar for {kind:?}"
            ));
        }
    }
    reg.arcane_errors.extend(errors);
}

pub(super) fn validate_arcane_ecology_graph(reg: &mut Registry) {
    let mut errors = Vec::new();
    let mut base_roles = BTreeMap::<EcologyRole, usize>::new();
    for block in &reg.blocks {
        let Some(ecology) = &block.arcane_ecology else {
            continue;
        };
        if block.arcane.is_none() {
            errors.push(format!(
                "{}: magical ecology needs an arcane destruction disposition",
                block.name
            ));
        }
        if ecology.charge_capacity > block.arcane.as_ref().map_or(0, |arcane| arcane.capacity) {
            errors.push(format!(
                "{}: ecology capacity exceeds the block's conserved Current capacity",
                block.name
            ));
        }
        if block.name.starts_with("base:") && ecology.kind != ArcaneEcologyKind::FiniteMineral {
            for role in &ecology.roles {
                *base_roles.entry(*role).or_default() += 1;
            }
        }
        if block.harvest.is_some() {
            errors.push(format!(
                "{}: ecology harvest cannot also use the ordinary repeatable block-harvest path",
                block.name
            ));
        }
        if ecology.kind == ArcaneEcologyKind::FiniteMineral
            && (block.material_class != MaterialClass::GeologicallyFinite
                || block.materials.is_empty()
                || !reg
                    .ores
                    .iter()
                    .any(|ore| ore.block == reg.block_by_name[&block.name]))
        {
            errors.push(format!(
                "{}: finite resonant geology needs a finite material identity and deposit rule",
                block.name
            ));
        }
        if ecology.kind == ArcaneEcologyKind::Crystal && (block.drops.is_none() || block.cross) {
            errors.push(format!(
                "{}: a regenerative crystal needs a physical shard drop and cluster block",
                block.name
            ));
        }
    }
    for required in [
        EcologyRole::Gatherer,
        EcologyRole::Reservoir,
        EcologyRole::Conductor,
        EcologyRole::Transformer,
        EcologyRole::Indicator,
        EcologyRole::Stabilizer,
        EcologyRole::Catalyst,
    ] {
        if base_roles.get(&required).copied().unwrap_or(0) < 2 {
            errors.push(format!(
                "base magical ecology needs two reachable renewable {:?} lifecycles",
                required
            ));
        }
    }
    reg.arcane_errors.extend(errors);
}
