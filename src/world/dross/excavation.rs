//! Excavation dross transaction coordination.

use crate::arcane::ArcaneOwner;
use crate::arcane::Current;
use crate::inventory::ItemStack;
use crate::planet::BlockPos;
use crate::world::World;

impl World {
    pub(in crate::world) fn excavate_dross_scar(
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
}
