//! Linked ecology destruction commits before physical voxel removal.

use crate::arcane::ArcaneLedger;
use crate::arcane_geography::ArcaneGeography;
use crate::planet::BlockPos;
use crate::registry::Registry;
use std::path::Path;

pub(super) fn settle_destruction(
    geography: &mut ArcaneGeography,
    ledger: Option<&mut ArcaneLedger>,
    registry: &Registry,
    save_dir: &Path,
    pos: BlockPos,
) -> Result<bool, String> {
    let Some(site_index) = geography.dynamic.ecology.sites.iter().position(|site| {
        site.block_pos() == Some(pos)
            && site.stage != crate::arcane_ecology::EcologyStage::Harvested
    }) else {
        return Ok(false);
    };
    let atlas_index = geography.dynamic.ecology.sites[site_index]
        .atlas_pos
        .index(geography.manifest.side);
    let old_site = geography.dynamic.ecology.sites[site_index].clone();
    let old_cell = geography.dynamic.cells[atlas_index];
    let old_sequence = geography.dynamic.ecology.event_sequence;
    let changed = crate::arcane_ecology::apply_destructive_loss(geography, registry, pos)?;
    if !changed {
        return Ok(false);
    }
    let operation_id = geography.dynamic.ecology.event_sequence.max(1);
    let (manifest, files) = match geography.linked_dynamic_replacements(save_dir, operation_id) {
        Ok(prepared) => prepared,
        Err(error) => {
            geography.dynamic.ecology.sites[site_index] = old_site;
            geography.dynamic.cells[atlas_index] = old_cell;
            geography.dynamic.ecology.event_sequence = old_sequence;
            return Err(error.to_string());
        }
    };
    let Some(ledger) = ledger else {
        geography.dynamic.ecology.sites[site_index] = old_site;
        geography.dynamic.cells[atlas_index] = old_cell;
        geography.dynamic.ecology.event_sequence = old_sequence;
        return Err("arcane ecology destruction requires the parent ledger".into());
    };
    if let Err(error) =
        ledger.commit_geography_state_linked("ecological biomass or crystal destroyed", files)
    {
        geography.dynamic.ecology.sites[site_index] = old_site;
        geography.dynamic.cells[atlas_index] = old_cell;
        geography.dynamic.ecology.event_sequence = old_sequence;
        return Err(error.to_string());
    }
    geography.accept_linked_manifest(manifest);
    Ok(true)
}
