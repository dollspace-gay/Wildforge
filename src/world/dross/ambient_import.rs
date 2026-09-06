//! Ambient import dross transaction coordination.

use crate::arcane::AccountRead;
use crate::arcane::ArcaneAuthority;
use crate::arcane::ArcaneMove;
use crate::arcane::ArcaneOwner;
use crate::arcane::ArcaneTransaction;
use std::collections::BTreeMap;
use crate::arcane::Current;
use crate::world::World;
use super::DenseImportBefore;
use super::MAX_DROSS_IMPORT_ACCOUNTS;

impl World {
    /// Adopt bounded sparse environmental owners into the compact Geography
    /// account. The geography bytes and parent-ledger custody move under one
    /// recovery coordinator, so a crash cannot copy or lose Current.
    pub(super) fn import_pending_environmental_dross(&mut self) -> Result<u64, String> {
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
}
