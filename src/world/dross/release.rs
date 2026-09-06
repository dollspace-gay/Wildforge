//! Release dross transaction coordination.

use crate::arcane::ArcaneAuthority;
use crate::arcane::ArcaneOwner;
use crate::arcane::ArcaneTransaction;
use crate::planet::BlockPos;
use crate::arcane::Current;
use crate::world::World;

impl World {
    pub(in crate::world) fn release_dross_scar(&mut self, pos: BlockPos) -> Result<(), String> {
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
