//! Manifestation dross transaction coordination.

use super::ScarManifestCandidate;
use crate::arcane::ArcaneAuthority;
use crate::arcane::ArcaneOwner;
use crate::arcane::ArcaneTransaction;
use crate::world::World;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

impl World {
    pub(super) fn manifest_one_dross_scar(
        &mut self,
        atlas: &crate::planet_atlas::PlanetAtlas,
        hour: u64,
        industrial_regions: &BTreeSet<crate::planet_atlas::AtlasPos>,
        cave_regions: &BTreeSet<crate::planet_atlas::AtlasPos>,
    ) -> Result<Option<u64>, String> {
        if !self
            .arcane_geography
            .as_mut()
            .is_some_and(|geography| geography.dynamic.dross_state.make_room_for_scar())
        {
            return Ok(None);
        }
        let Some(geography) = self.arcane_geography.as_ref() else {
            return Ok(None);
        };
        let mut candidate: Option<ScarManifestCandidate> = None;
        for region in geography.scar_candidates() {
            let center = region.center(atlas.side());
            let Some(surface) = crate::planet::SurfacePos::new(
                center.face,
                center
                    .u
                    .floor()
                    .clamp(0.0, f64::from(crate::planet::FACE_BLOCKS - 1)) as u16,
                center
                    .v
                    .floor()
                    .clamp(0.0, f64::from(crate::planet::FACE_BLOCKS - 1)) as u16,
            )
            .ok() else {
                continue;
            };
            // First manifestation is allowed only in terrain the persisted
            // construction history still considers untouched. Once a site
            // exists, its saved identity may continue to reconcile normally.
            if self
                .player_touched
                .contains(&crate::planet::ChunkPos::from_surface(surface))
            {
                continue;
            }
            let index = region.index(geography.manifest.side);
            let air = geography.dynamic.dross_state.cells[index].airborne_total();
            let water = geography.dynamic.dross_state.cells[index].waterborne_total();
            let soil = geography.dynamic.cells[index].dross_total();
            let (carrier, available) = [
                (crate::dross::DrossCarrier::Air, air),
                (crate::dross::DrossCarrier::Water, water),
                (crate::dross::DrossCarrier::Soil, soil),
            ]
            .into_iter()
            .max_by_key(|(carrier, units)| (*units, *carrier))
            .expect("three environmental dross carriers exist");
            if available == 0 {
                continue;
            }
            let kind = geography.scar_kind_for_region(
                atlas,
                region,
                industrial_regions.contains(&region),
                cave_regions.contains(&region)
                    || matches!(
                        crate::planet_atlas::BedrockFamily::from_id(
                            atlas.genesis.tectonics.values()[index].bedrock_family,
                        ),
                        crate::planet_atlas::BedrockFamily::Limestone
                            | crate::planet_atlas::BedrockFamily::Marble
                    ) && atlas.genesis.tectonics.values()[index].fault_intensity >= 500,
            );
            let band = geography.dynamic.dross_state.cells[index].band;
            let active = geography
                .dynamic
                .dross_state
                .scars
                .values()
                .filter(|site| site.region == region && site.resolved_step.is_none())
                .collect::<Vec<_>>();
            let existing_in_region =
                active
                    .iter()
                    .fold(BTreeMap::<String, usize>::new(), |mut counts, site| {
                        let content_id = if site.content_id.is_empty() {
                            site.kind.block_id().to_string()
                        } else {
                            site.content_id.clone()
                        };
                        *counts.entry(content_id).or_default() += 1;
                        counts
                    });
            let Some(site_slot) = (0..crate::dross::MAX_ACTIVE_SCARS_PER_REGION).find(|slot| {
                active
                    .iter()
                    .all(|site| usize::from(site.site_slot) != *slot)
            }) else {
                continue;
            };
            let raw = u64::from(region.face as u8)
                | (u64::from(region.u) << 8)
                | (u64::from(region.v) << 24)
                | (hour << 40);
            let score = raw.wrapping_mul(0x9e37_79b9_7f4a_7c15).rotate_left(17)
                ^ u64::from(atlas.manifest.seed);
            let Some(definition) =
                self.reg
                    .select_dross_scar(kind, carrier, band, &existing_in_region, score)
            else {
                continue;
            };
            let proposed = ScarManifestCandidate {
                score,
                region,
                index,
                carrier,
                available,
                kind,
                content_id: definition.content_id.clone(),
                site_slot: site_slot as u8,
            };
            if candidate
                .as_ref()
                .is_none_or(|current| score < current.score)
            {
                candidate = Some(proposed);
            }
        }
        let Some(ScarManifestCandidate {
            region,
            index,
            carrier,
            available,
            kind,
            content_id,
            site_slot,
            ..
        }) = candidate
        else {
            return Ok(None);
        };
        let requested = available.div_ceil(4).clamp(1, 256);
        let old_cell = geography.dynamic.cells[index];
        let old_carrier = geography.dynamic.dross_state.cells[index];
        let old_provenance = geography
            .dynamic
            .dross_state
            .provenance
            .get(&region)
            .cloned();
        let old_events = geography.dynamic.dross_state.events.clone();
        let old_event_sequence = geography.dynamic.dross_state.event_sequence;
        let old_catalog = geography.catalog.clone();
        let old_exported = geography.dynamic.ecology.exported;
        let old_external_imported = geography.dynamic.dross_state.external_imported;

        let scar_id = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no Current ledger.")?
            .allocate_scar_id()
            .map_err(|error| error.to_string())?;
        let (current, provenance) = self
            .arcane_geography
            .as_mut()
            .expect("scar geography was checked")
            .export_environmental_dross(region, carrier, requested)?;
        if current.is_empty() {
            return Ok(None);
        }
        let actor_hint = provenance.entries.first().and_then(|entry| entry.actor);
        let installation_hint = provenance
            .entries
            .first()
            .and_then(|entry| entry.installation_id);
        {
            let geography = self
                .arcane_geography
                .as_mut()
                .expect("scar geography was checked");
            geography.dynamic.dross_state.scars.insert(
                scar_id,
                crate::dross::ScarSite {
                    id: scar_id,
                    region,
                    kind,
                    content_id,
                    site_slot,
                    created_step: hour,
                    last_changed_step: hour,
                    breach_count: 0,
                    materialized_at: None,
                    resolved_step: None,
                    actor_hint,
                    installation_hint,
                    provenance,
                },
            );
            geography.dynamic.dross_state.record(
                hour,
                region,
                crate::dross::DrossEventKind::ScarManifested { scar_id, kind },
            );
            geography.mark_scar(atlas, region);
        }
        let rollback = |world: &mut World| {
            if let Some(geography) = world.arcane_geography.as_mut() {
                geography.dynamic.cells[index] = old_cell;
                geography.dynamic.dross_state.cells[index] = old_carrier;
                geography.dynamic.dross_state.scars.remove(&scar_id);
                geography.dynamic.dross_state.events = old_events.clone();
                geography.dynamic.dross_state.event_sequence = old_event_sequence;
                geography.catalog.clone_from(&old_catalog);
                geography.dynamic.ecology.exported = old_exported;
                geography.dynamic.dross_state.external_imported = old_external_imported;
                if let Some(provenance) = &old_provenance {
                    geography
                        .dynamic
                        .dross_state
                        .provenance
                        .insert(region, provenance.clone());
                } else {
                    geography.dynamic.dross_state.provenance.remove(&region);
                }
            }
        };
        let transaction_id = match self
            .arcane_ledger
            .as_mut()
            .expect("scar ledger was checked")
            .system_transaction_id()
        {
            Ok(id) => id,
            Err(error) => {
                rollback(self);
                return Err(error.to_string());
            }
        };
        let (manifest, files) = match self
            .arcane_geography
            .as_ref()
            .expect("scar geography was checked")
            .linked_dross_replacements(&self.save_dir, transaction_id.sequence)
        {
            Ok(prepared) => prepared,
            Err(error) => {
                rollback(self);
                return Err(error.to_string());
            }
        };
        let source = ArcaneOwner::Geography;
        let target = ArcaneOwner::Scar(scar_id);
        let ledger = self
            .arcane_ledger
            .as_mut()
            .expect("scar ledger was checked");
        let transaction = ArcaneTransaction::transfer(
            transaction_id,
            source.clone(),
            ledger.version_of(&source),
            target.clone(),
            ledger.version_of(&target),
            current,
            ArcaneAuthority::System,
            format!("manifest {} at {region:?}", kind.label()),
        );
        if let Err(error) = ledger.commit_linked_files(transaction, files) {
            rollback(self);
            return Err(error.to_string());
        }
        self.arcane_geography
            .as_mut()
            .expect("scar geography survived linked commit")
            .accept_linked_manifest(manifest);
        Ok(Some(scar_id))
    }
}
