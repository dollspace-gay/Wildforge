//! Ambient custody implements transaction coordination.

use crate::arcane::AccountRead;
use crate::arcane::ArcaneAuthority;
use crate::arcane::ArcaneMove;
use crate::arcane::ArcaneOwner;
use crate::arcane::ArcaneTransaction;
use crate::arcane::Current;
use crate::world::World;

impl World {
    /// Make a bounded amount of this cell's dense planetary Ambient Current
    /// available to sparse apparatus transactions. This is a custody handoff,
    /// not recharge: Geography loses exactly what the regional Ambient owner
    /// gains, with the geography files committed through the same journal.
    pub(in crate::world) fn ensure_regional_ambient_units(
        &mut self,
        region: crate::planet_atlas::AtlasPos,
        requested: u64,
        reason: &str,
    ) -> Result<(), String> {
        let owner = ArcaneOwner::Ambient(region);
        let present = self
            .arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.account(&owner))
            .map_or(0, |account| account.current.total());
        if present >= requested {
            return Ok(());
        }
        let missing = requested.saturating_sub(present);
        let (index, old_cell, old_sequence, old_exported, old_external_imported) = {
            let geography = self
                .arcane_geography
                .as_ref()
                .ok_or("The finite magical geography is unavailable.")?;
            let index = region.index(geography.manifest.side);
            (
                index,
                geography.dynamic.cells[index],
                geography.dynamic.ecology.event_sequence,
                geography.dynamic.ecology.exported,
                geography.dynamic.dross_state.external_imported,
            )
        };
        let moved = self
            .arcane_geography
            .as_mut()
            .ok_or("The finite magical geography is unavailable.")?
            .export_ambient_for_apparatus(region, missing)
            .map_err(|error| error.to_string())?;
        let operation_id = self
            .arcane_geography
            .as_ref()
            .expect("geography was checked")
            .dynamic
            .ecology
            .event_sequence
            .max(1);
        let rollback = |world: &mut World| {
            if let Some(geography) = world.arcane_geography.as_mut() {
                geography.dynamic.cells[index] = old_cell;
                geography.dynamic.ecology.event_sequence = old_sequence;
                geography.dynamic.ecology.exported = old_exported;
                geography.dynamic.dross_state.external_imported = old_external_imported;
            }
        };
        let (manifest, files) = match self
            .arcane_geography
            .as_ref()
            .expect("geography was checked")
            .linked_dynamic_replacements(&self.save_dir, operation_id)
        {
            Ok(prepared) => prepared,
            Err(error) => {
                rollback(self);
                return Err(error.to_string());
            }
        };
        if self.arcane_ledger.is_none() {
            rollback(self);
            return Err("The world has no Current ledger.".into());
        }
        let geography_owner = ArcaneOwner::Geography;
        let transaction_id = match self
            .arcane_ledger
            .as_mut()
            .expect("ledger was checked")
            .system_transaction_id()
        {
            Ok(id) => id,
            Err(error) => {
                rollback(self);
                return Err(error.to_string());
            }
        };
        let ledger = self.arcane_ledger.as_mut().expect("ledger was checked");
        let mut reads = vec![
            AccountRead {
                owner: geography_owner.clone(),
                expected_version: ledger.version_of(&geography_owner),
            },
            AccountRead {
                owner: owner.clone(),
                expected_version: ledger.version_of(&owner),
            },
        ];
        reads.sort_by(|a, b| a.owner.cmp(&b.owner));
        let transaction = ArcaneTransaction {
            id: transaction_id,
            reads,
            debits: vec![ArcaneMove {
                owner: geography_owner,
                current: moved.clone(),
                content_id: None,
            }],
            credits: vec![ArcaneMove {
                owner: owner.clone(),
                current: moved,
                content_id: Some("base:regional_ambient".into()),
            }],
            transforms: Vec::new(),
            authority: ArcaneAuthority::System,
            reason: reason.into(),
            content_id: "base:regional_ambient".into(),
            linked: Vec::new(),
        };
        if let Err(error) = ledger.commit_linked_files(transaction, files) {
            rollback(self);
            return Err(error.to_string());
        }
        self.arcane_geography
            .as_mut()
            .expect("geography survived linked commit")
            .accept_linked_manifest(manifest);
        let now = self
            .arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.account(&owner))
            .map_or(0, |account| account.current.total());
        if now < requested {
            return Err(format!(
                "The local Ambient Current supplied {now} of {requested} required units."
            ));
        }
        Ok(())
    }
}
