//! Authoritative bridge between sparse dross transaction owners and the
//! dense, weather-coupled planetary subledger.

use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::arcane::{
    AccountRead, ArcaneAuthority, ArcaneMove, ArcaneOwner, ArcaneTransaction, Current,
};
use crate::dross::{DrossAdvance, DrossConditions};

const MAX_DROSS_IMPORT_ACCOUNTS: usize = 12;

#[derive(Clone)]
struct DenseImportBefore {
    cell: crate::arcane_geography::ArcaneDynamicCell,
    carrier: crate::dross::DrossCellState,
    provenance: Option<crate::dross::DrossProvenance>,
}

struct ScarManifestCandidate {
    score: u64,
    region: crate::planet_atlas::AtlasPos,
    index: usize,
    carrier: crate::dross::DrossCarrier,
    available: u64,
    kind: crate::dross::ScarKind,
    content_id: String,
    site_slot: u8,
}

impl World {
    pub(crate) fn environmental_dross_band_at(&self, pos: BlockPos) -> crate::dross::DrossBand {
        let Some(atlas) = self.planet_atlas.as_ref() else {
            return crate::dross::DrossBand::Clear;
        };
        let region = atlas.atlas_pos(pos.surface());
        self.arcane_geography
            .as_ref()
            .map_or(crate::dross::DrossBand::Clear, |geography| {
                geography.dross_band_at(region)
            })
    }

    pub(crate) fn apply_environmental_dross_exposure(
        &mut self,
        actor: [u8; 16],
        pos: BlockPos,
        now: u64,
        band: crate::dross::DrossBand,
        physiology: &mut crate::alchemy::PreparationPhysiology,
        modifiers: &mut crate::alchemy::PreparationModifiers,
    ) {
        let previous = self.dross_exposure_tick.entry(actor).or_insert(now);
        let elapsed = now.saturating_sub(*previous) / 20;
        if elapsed != 0 {
            *previous = previous.saturating_add(elapsed.saturating_mul(20));
            let rate = match band {
                crate::dross::DrossBand::Seep => 1,
                crate::dross::DrossBand::Scar => 2,
                crate::dross::DrossBand::BreachRisk => 4,
                crate::dross::DrossBand::Clear
                | crate::dross::DrossBand::Trace
                | crate::dross::DrossBand::Strained => 0,
            };
            if rate == 0 {
                physiology.bodily_dross = physiology.bodily_dross.saturating_sub(elapsed);
            } else {
                physiology.bodily_dross = physiology
                    .bodily_dross
                    .saturating_add(elapsed.saturating_mul(rate))
                    .min(4_096);
            }
        }
        modifiers.dross_band = band.ordinal();
        modifiers.dross_pattern = band.ordinal();
        let (recovery, perception, stamina) = match band {
            crate::dross::DrossBand::Clear
            | crate::dross::DrossBand::Trace
            | crate::dross::DrossBand::Strained => (1_000, 1_000, 1_000),
            crate::dross::DrossBand::Seep => (850, 950, 950),
            crate::dross::DrossBand::Scar => (650, 850, 800),
            crate::dross::DrossBand::BreachRisk => (450, 700, 650),
        };
        modifiers.recovery_permille = recovery;
        modifiers.perception_permille = perception;
        modifiers.stamina_permille = stamina;
        if band >= crate::dross::DrossBand::Seep {
            for status in self.dross_scar_statuses_at(pos) {
                match status {
                    crate::dross::ScarStatusHandler::RecoveryDrag => {
                        modifiers.recovery_permille =
                            modifiers.recovery_permille.saturating_sub(100);
                    }
                    crate::dross::ScarStatusHandler::PerceptionWarp => {
                        modifiers.perception_permille =
                            modifiers.perception_permille.saturating_sub(100);
                    }
                    crate::dross::ScarStatusHandler::StaminaDrag => {
                        modifiers.stamina_permille = modifiers.stamina_permille.saturating_sub(100);
                    }
                    crate::dross::ScarStatusHandler::WorkingInstability => {
                        modifiers.strain_permille =
                            modifiers.strain_permille.saturating_add(100).min(1_500);
                    }
                }
            }
        }
    }

    fn dross_scar_statuses_at(&self, pos: BlockPos) -> BTreeSet<crate::dross::ScarStatusHandler> {
        let (Some(atlas), Some(geography)) = (&self.planet_atlas, &self.arcane_geography) else {
            return BTreeSet::new();
        };
        let region = atlas.atlas_pos(pos.surface());
        geography
            .dynamic
            .dross_state
            .scars
            .values()
            .filter(|site| site.region == region && site.resolved_step.is_none())
            .filter_map(|site| {
                self.reg
                    .resolve_dross_scar(&site.content_id, site.kind)
                    .and_then(|definition| definition.status)
            })
            .collect()
    }

    /// Adopt bounded sparse environmental owners into the compact Geography
    /// account. The geography bytes and parent-ledger custody move under one
    /// recovery coordinator, so a crash cannot copy or lose Current.
    fn import_pending_environmental_dross(&mut self) -> Result<u64, String> {
        let candidates = self
            .arcane_ledger
            .as_ref()
            .map(|ledger| {
                ledger
                    .accounts
                    .iter()
                    .filter_map(|(owner, account)| match owner {
                        ArcaneOwner::Dross { region, medium } if !account.current.is_empty() => {
                            Some((owner.clone(), *region, *medium, account.current.clone()))
                        }
                        _ => None,
                    })
                    .take(MAX_DROSS_IMPORT_ACCOUNTS)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if candidates.is_empty() {
            return Ok(0);
        }
        let Some(geography) = self.arcane_geography.as_ref() else {
            return Ok(0);
        };
        let side = geography.manifest.side;
        let mut before = BTreeMap::new();
        for (_, region, _, _) in &candidates {
            let index = region.index(side);
            before.entry(*region).or_insert_with(|| DenseImportBefore {
                cell: geography.dynamic.cells[index],
                carrier: geography.dynamic.dross_state.cells[index],
                provenance: geography
                    .dynamic
                    .dross_state
                    .provenance
                    .get(region)
                    .cloned(),
            });
        }
        let (old_imported, old_generated) = (
            geography.dynamic.dross_state.imported_units,
            geography.dynamic.dross_state.generated_units,
        );
        let old_generated_by_process = geography.dynamic.dross_state.generated_by_process.clone();
        let old_exported = geography.dynamic.ecology.exported;
        let old_external_imported = geography.dynamic.dross_state.external_imported;

        let rollback = |world: &mut World| {
            if let Some(geography) = world.arcane_geography.as_mut() {
                geography.dynamic.dross_state.imported_units = old_imported;
                geography.dynamic.dross_state.generated_units = old_generated;
                geography
                    .dynamic
                    .dross_state
                    .generated_by_process
                    .clone_from(&old_generated_by_process);
                geography.dynamic.ecology.exported = old_exported;
                geography.dynamic.dross_state.external_imported = old_external_imported;
                for (region, previous) in &before {
                    let index = region.index(geography.manifest.side);
                    geography.dynamic.cells[index] = previous.cell;
                    geography.dynamic.dross_state.cells[index] = previous.carrier;
                    if let Some(provenance) = &previous.provenance {
                        geography
                            .dynamic
                            .dross_state
                            .provenance
                            .insert(*region, provenance.clone());
                    } else {
                        geography.dynamic.dross_state.provenance.remove(region);
                    }
                }
            }
        };

        let mut moved = Vec::new();
        let mut total = Current::default();
        for (owner, region, medium, offered) in candidates {
            let attribution = self
                .arcane_ledger
                .as_ref()
                .expect("dross candidates came from the ledger")
                .environmental_dross_attribution(&owner, offered.total());
            let adopted = match self
                .arcane_geography
                .as_mut()
                .expect("dross geography was checked")
                .import_environmental_dross(region, medium, &offered, &attribution)
            {
                Ok(adopted) => adopted,
                Err(error) => {
                    rollback(self);
                    return Err(error);
                }
            };
            if !adopted.is_empty() {
                total
                    .checked_add(&adopted)
                    .map_err(|error| error.to_string())?;
                moved.push((owner, adopted));
            }
        }
        if moved.is_empty() {
            return Ok(0);
        }

        let transaction_id = match self
            .arcane_ledger
            .as_mut()
            .expect("dross ledger was checked")
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
            .expect("dross geography was checked")
            .linked_dross_replacements(&self.save_dir, transaction_id.sequence)
        {
            Ok(prepared) => prepared,
            Err(error) => {
                rollback(self);
                return Err(error.to_string());
            }
        };
        let geography_owner = ArcaneOwner::Geography;
        let ledger = self
            .arcane_ledger
            .as_mut()
            .expect("dross ledger was checked");
        let mut reads = moved
            .iter()
            .map(|(owner, _)| AccountRead {
                owner: owner.clone(),
                expected_version: ledger.version_of(owner),
            })
            .collect::<Vec<_>>();
        reads.push(AccountRead {
            owner: geography_owner.clone(),
            expected_version: ledger.version_of(&geography_owner),
        });
        reads.sort_by(|left, right| left.owner.cmp(&right.owner));
        let transaction = ArcaneTransaction {
            id: transaction_id,
            reads,
            debits: moved
                .into_iter()
                .map(|(owner, current)| ArcaneMove {
                    owner,
                    current,
                    content_id: None,
                })
                .collect(),
            credits: vec![ArcaneMove {
                owner: geography_owner,
                current: total.clone(),
                content_id: Some("base:planetary_current".into()),
            }],
            transforms: Vec::new(),
            authority: ArcaneAuthority::System,
            reason: "adopt released dross into weather-coupled planetary custody".into(),
            content_id: "base:environmental_dross_import".into(),
            linked: Vec::new(),
        };
        if let Err(error) = ledger.commit_linked_files(transaction, files) {
            rollback(self);
            return Err(error.to_string());
        }
        self.arcane_geography
            .as_mut()
            .expect("dross geography survived linked commit")
            .accept_linked_manifest(manifest);
        Ok(total.total())
    }

    /// Slice one planetary dross hour. Weather routing, heart mortality, and
    /// ecology are sampled from host-owned state; chunk residency never
    /// determines transport or manifestations.
    pub fn tick_dross(&mut self, budget: usize) -> std::io::Result<DrossAdvance> {
        self.import_pending_environmental_dross()
            .map_err(std::io::Error::other)?;
        let Some(atlas) = self.planet_atlas.as_ref().cloned() else {
            return Ok(DrossAdvance::default());
        };
        let target_hour = self
            .planetary_weather
            .as_ref()
            .map_or(0, |weather| weather.completed_hours);
        let runoff_routes = self
            .planetary_weather
            .as_ref()
            .map_or_else(Vec::new, |weather| weather.last_runoff_routes().to_vec());
        let mut living_hearts = atlas
            .biomes
            .countries
            .iter()
            .map(|country| country.id)
            .collect::<BTreeSet<_>>();
        for heart in self.hearts.values().filter(|heart| heart.stage == 0) {
            if let Some(country) = atlas.country_at(heart.pos.surface()) {
                living_hearts.remove(&country.id);
            }
        }
        let industrial_regions = self
            .alchemy_state
            .as_ref()
            .into_iter()
            .flat_map(|state| state.apparatus.keys())
            .map(|pos| atlas.atlas_pos(pos.surface()))
            .collect::<BTreeSet<_>>();
        let cave_regions = BTreeSet::new();
        let conditions = DrossConditions {
            runoff_routes: &runoff_routes,
            living_hearts: &living_hearts,
            long_winter: self.calendar_state.long_winter(),
        };
        let report =
            self.arcane_geography
                .as_mut()
                .map_or(Ok(DrossAdvance::default()), |geography| {
                    geography
                        .advance_dross_toward(&atlas, &self.reg, target_hour, budget, conditions)
                        .map_err(std::io::Error::other)
                })?;
        if let Some(hour) = report.completed_hour {
            self.manifest_one_dross_scar(&atlas, hour, &industrial_regions, &cave_regions)
                .map_err(std::io::Error::other)?;
            self.refresh_loaded_dross_scars();
        }
        Ok(report)
    }

    fn manifest_one_dross_scar(
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

    pub(super) fn owns_dross_scar_block(&self, pos: BlockPos) -> bool {
        self.arcane_geography.as_ref().is_some_and(|geography| {
            geography
                .dynamic
                .dross_state
                .materialized
                .contains_key(&pos)
        })
    }

    fn stage_resolved_scar(
        &mut self,
        pos: BlockPos,
    ) -> Result<
        (
            u64,
            crate::dross::ScarSite,
            std::collections::VecDeque<crate::dross::DrossEvent>,
            u64,
            crate::arcane_geography::ArcaneSiteCatalog,
        ),
        String,
    > {
        let geography = self
            .arcane_geography
            .as_mut()
            .ok_or("The finite magical geography is unavailable.")?;
        let scar_id = geography
            .dynamic
            .dross_state
            .materialized
            .get(&pos)
            .copied()
            .ok_or("That block is not a live dross scar.")?;
        let old_site = geography
            .dynamic
            .dross_state
            .scars
            .get(&scar_id)
            .cloned()
            .ok_or("The scar's persistent site is missing.")?;
        let old_events = geography.dynamic.dross_state.events.clone();
        let old_sequence = geography.dynamic.dross_state.event_sequence;
        let old_catalog = geography.catalog.clone();
        let step = geography.dynamic.dross_state.completed_steps;
        let site = geography
            .dynamic
            .dross_state
            .scars
            .get_mut(&scar_id)
            .expect("scar site was checked");
        site.materialized_at = None;
        site.resolved_step = Some(step);
        site.last_changed_step = step;
        geography.dynamic.dross_state.materialized.remove(&pos);
        geography.dynamic.dross_state.record(
            step,
            old_site.region,
            crate::dross::DrossEventKind::ScarExcavated { scar_id },
        );
        let region_still_scarred = geography
            .dynamic
            .dross_state
            .scars
            .values()
            .any(|site| site.region == old_site.region && site.resolved_step.is_none());
        if !region_still_scarred
            && let Some(place) = geography.catalog.sites.iter_mut().find(|place| {
                place.kind == crate::arcane_geography::ArcanePlaceType::Scar
                    && place.center == old_site.region
            })
        {
            place.active = false;
        }
        Ok((scar_id, old_site, old_events, old_sequence, old_catalog))
    }

    fn rollback_resolved_scar(
        &mut self,
        pos: BlockPos,
        scar_id: u64,
        old_site: crate::dross::ScarSite,
        old_events: std::collections::VecDeque<crate::dross::DrossEvent>,
        old_sequence: u64,
        old_catalog: crate::arcane_geography::ArcaneSiteCatalog,
    ) {
        if let Some(geography) = self.arcane_geography.as_mut() {
            geography
                .dynamic
                .dross_state
                .scars
                .insert(scar_id, old_site);
            geography
                .dynamic
                .dross_state
                .materialized
                .insert(pos, scar_id);
            geography.dynamic.dross_state.events = old_events;
            geography.dynamic.dross_state.event_sequence = old_sequence;
            geography.catalog = old_catalog;
        }
    }

    pub(super) fn excavate_dross_scar(
        &mut self,
        pos: BlockPos,
        stack: &mut ItemStack,
    ) -> Result<(), String> {
        if self.reg.item(stack.item).name != "base:scar_fragment" || stack.count != 1 {
            return Err("A scar must excavate into one stable fragment item.".into());
        }
        let (scar_id, old_site, old_events, old_sequence, old_catalog) =
            self.stage_resolved_scar(pos)?;
        let source = ArcaneOwner::Scar(scar_id);
        let Some(current) = self
            .arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.account(&source))
            .map(|account| account.current.clone())
        else {
            self.rollback_resolved_scar(
                pos,
                scar_id,
                old_site,
                old_events,
                old_sequence,
                old_catalog,
            );
            return Err("The scar's exact Current account is missing.".into());
        };
        let operation_id = self
            .arcane_geography
            .as_ref()
            .expect("scar geography was staged")
            .dynamic
            .dross_state
            .event_sequence;
        let item_id = match self
            .arcane_ledger
            .as_mut()
            .expect("scar ledger was checked")
            .allocate_item_id()
        {
            Ok(item_id) => item_id,
            Err(error) => {
                self.rollback_resolved_scar(
                    pos,
                    scar_id,
                    old_site,
                    old_events,
                    old_sequence,
                    old_catalog,
                );
                return Err(error.to_string());
            }
        };
        if let Some(geography) = self.arcane_geography.as_mut() {
            if let Err(error) =
                geography.record_contained_dross_provenance(item_id, old_site.provenance.clone())
            {
                self.rollback_resolved_scar(
                    pos,
                    scar_id,
                    old_site,
                    old_events,
                    old_sequence,
                    old_catalog,
                );
                return Err(error);
            }
            if let Some(site) = geography.dynamic.dross_state.scars.get_mut(&scar_id) {
                site.provenance = crate::dross::DrossProvenance::default();
            }
        }
        let (manifest, files) = match self
            .arcane_geography
            .as_ref()
            .expect("scar geography was staged")
            .linked_dross_replacements(&self.save_dir, operation_id)
        {
            Ok(prepared) => prepared,
            Err(error) => {
                if let Some(geography) = self.arcane_geography.as_mut() {
                    geography
                        .dynamic
                        .dross_state
                        .contained_provenance
                        .remove(&item_id);
                }
                self.rollback_resolved_scar(
                    pos,
                    scar_id,
                    old_site,
                    old_events,
                    old_sequence,
                    old_catalog,
                );
                return Err(error.to_string());
            }
        };
        let Some(ledger) = self.arcane_ledger.as_mut() else {
            if let Some(geography) = self.arcane_geography.as_mut() {
                geography
                    .dynamic
                    .dross_state
                    .contained_provenance
                    .remove(&item_id);
            }
            self.rollback_resolved_scar(
                pos,
                scar_id,
                old_site,
                old_events,
                old_sequence,
                old_catalog,
            );
            return Err("The world has no Current ledger.".into());
        };
        let result = ledger.bind_preallocated_item_exact_linked(
            item_id,
            source,
            Current::default(),
            current,
            "base:scar_fragment",
            "excavate manifested dross scar",
            files,
        );
        match result {
            Ok(item_id) => {
                stack.arcane_id = item_id;
                self.arcane_geography
                    .as_mut()
                    .expect("scar geography survived excavation")
                    .accept_linked_manifest(manifest);
                Ok(())
            }
            Err(error) => {
                if let Some(geography) = self.arcane_geography.as_mut() {
                    geography
                        .dynamic
                        .dross_state
                        .contained_provenance
                        .remove(&item_id);
                }
                self.rollback_resolved_scar(
                    pos,
                    scar_id,
                    old_site,
                    old_events,
                    old_sequence,
                    old_catalog,
                );
                Err(error.to_string())
            }
        }
    }

    pub(super) fn release_dross_scar(&mut self, pos: BlockPos) -> Result<(), String> {
        let (scar_id, old_site, old_events, old_sequence, old_catalog) =
            self.stage_resolved_scar(pos)?;
        let source = ArcaneOwner::Scar(scar_id);
        let target = ArcaneOwner::Dross {
            region: old_site.region,
            medium: crate::arcane::DrossMedium::Air,
        };
        let Some(ledger) = self.arcane_ledger.as_mut() else {
            self.rollback_resolved_scar(
                pos,
                scar_id,
                old_site,
                old_events,
                old_sequence,
                old_catalog,
            );
            return Err("The world has no Current ledger.".into());
        };
        let Some(current) = ledger
            .account(&source)
            .map(|account| account.current.clone())
        else {
            self.rollback_resolved_scar(
                pos,
                scar_id,
                old_site,
                old_events,
                old_sequence,
                old_catalog,
            );
            return Err("The scar's exact Current account is missing.".into());
        };
        let transaction_id = match ledger.system_transaction_id() {
            Ok(id) => id,
            Err(error) => {
                self.rollback_resolved_scar(
                    pos,
                    scar_id,
                    old_site,
                    old_events,
                    old_sequence,
                    old_catalog,
                );
                return Err(error.to_string());
            }
        };
        let (manifest, files) = match self
            .arcane_geography
            .as_ref()
            .expect("scar geography was staged")
            .linked_dross_replacements(&self.save_dir, transaction_id.sequence)
        {
            Ok(prepared) => prepared,
            Err(error) => {
                self.rollback_resolved_scar(
                    pos,
                    scar_id,
                    old_site,
                    old_events,
                    old_sequence,
                    old_catalog,
                );
                return Err(error.to_string());
            }
        };
        let ledger = self
            .arcane_ledger
            .as_mut()
            .expect("scar ledger was checked");
        let mut transaction = ArcaneTransaction::transfer(
            transaction_id,
            source.clone(),
            ledger.version_of(&source),
            target.clone(),
            ledger.version_of(&target),
            current,
            ArcaneAuthority::System,
            "destroyed scar releases its charge into air",
        );
        transaction.content_id = "base:scar_release".into();
        if let Err(error) = ledger.commit_linked_files(transaction, files) {
            self.rollback_resolved_scar(
                pos,
                scar_id,
                old_site,
                old_events,
                old_sequence,
                old_catalog,
            );
            return Err(error.to_string());
        }
        self.arcane_geography
            .as_mut()
            .expect("scar geography survived release")
            .accept_linked_manifest(manifest);
        Ok(())
    }
}
