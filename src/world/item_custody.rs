//! Item custody coordinator for the authoritative world.

use super::{BlockId, ItemStack, World};

impl World {
    /// Every destructive item path converges here. Unknown/removed charged
    /// content defaults to regional Dross, retaining both quantity and its
    /// saved content identity instead of deleting an uninspectable account.
    pub fn retire_arcane_stack_at(
        &mut self,
        at: crate::planet::BlockPos,
        stack: ItemStack,
        reason: &str,
    ) -> bool {
        if stack.arcane_id == 0 {
            return false;
        }
        if self
            .alchemy_state
            .as_ref()
            .is_some_and(|state| state.containers.contains_key(&stack.arcane_id))
        {
            if let Err(error) = self.destroy_preparation_container_at(at, stack, reason) {
                eprintln!("alchemy: destructive container settlement failed: {error}");
            }
            // The exact dose sidecar owns both its ingredient and vessel
            // vectors, even if settlement reported a recoverable error. Do
            // not let the generic item-material path double-count either.
            return true;
        }
        if self
            .implements_state
            .as_ref()
            .is_some_and(|state| state.instance(stack.arcane_id).is_some())
        {
            if let Err(error) = self.retire_implement_at(at, stack, reason) {
                eprintln!("implements: destructive item settlement failed: {error}");
            }
            return true;
        }
        let Some(atlas) = &self.planet_atlas else {
            return false;
        };
        let region = atlas.atlas_pos(at.surface());
        let heat_dispersal = reason.contains("lava") || reason.contains("burned in fire");
        let disposition = self
            .reg
            .items
            .get(stack.item.0 as usize)
            .and_then(|definition| definition.arcane.as_ref())
            .map_or(crate::registry::ArcaneDisposition::Dross, |arcane| {
                arcane.on_destroy
            });
        let Some(ledger) = &mut self.arcane_ledger else {
            return false;
        };
        if ledger.item_current_total(stack.arcane_id).is_none() {
            eprintln!(
                "arcane: discarded item {} names missing Current account {}",
                self.reg.item(stack.item).name,
                stack.arcane_id
            );
            return false;
        }
        let destination = match disposition {
            crate::registry::ArcaneDisposition::Ambient => {
                crate::arcane::ArcaneOwner::Ambient(region)
            }
            // A destructive "scar" disposition means severe environmental
            // dross, not permission to mint a bare Scar owner. Only the dross
            // manifestation coordinator may create a Scar(id), after the
            // persisted warning ladder and canonical-site checks.
            crate::registry::ArcaneDisposition::Dross
            | crate::registry::ArcaneDisposition::Scar => crate::arcane::ArcaneOwner::Dross {
                region,
                medium: if heat_dispersal {
                    crate::arcane::DrossMedium::Air
                } else {
                    crate::arcane::DrossMedium::Soil
                },
            },
        };
        if let Err(error) = ledger.move_all_item(stack.arcane_id, destination, reason) {
            eprintln!("arcane: destructive item transfer failed: {error}");
        }
        false
    }

    /// Charge newly discovered/generated content from the finite reserve of
    /// its country (or Deep where no country owns the site).
    pub fn bind_arcane_stack_at(
        &mut self,
        at: crate::planet::BlockPos,
        stack: &mut ItemStack,
        reason: &str,
    ) -> std::io::Result<()> {
        if stack.arcane_id != 0 {
            return Ok(());
        }
        let definition = self
            .reg
            .items
            .get(stack.item.0 as usize)
            .and_then(|item| item.arcane.clone());
        let sealed_dross = self
            .reg
            .item(stack.item)
            .discovery
            .as_ref()
            .and_then(|definition| definition.evidence_class.as_deref())
            == Some("sealed_dross_ampoule");
        let Some(definition) = definition else {
            return Ok(());
        };
        if stack.count != 1 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "charged content must be instantiated as single-item stacks",
            ));
        }
        let source = self
            .planet_atlas
            .as_ref()
            .and_then(|atlas| atlas.country_at(at.surface()).map(|country| country.id))
            .map(crate::arcane::ArcaneOwner::Heart)
            .unwrap_or(crate::arcane::ArcaneOwner::Deep);
        let content_id = self.reg.item(stack.item).name.clone();
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or_else(|| std::io::Error::other("charged content requires an arcane ledger"))?;
        stack.arcane_id = ledger
            .bind_new_item(source, &definition, &content_id, reason)
            .map_err(std::io::Error::other)?;
        if sealed_dross {
            ledger
                .move_all(
                    crate::arcane::ArcaneOwner::Item(stack.arcane_id),
                    crate::arcane::ArcaneOwner::ItemDross(stack.arcane_id),
                    "sealed archaeological dross containment",
                )
                .map_err(std::io::Error::other)?;
        }
        if self.reg.item(stack.item).charm_def.is_some() {
            self.ensure_charm_instance_at(at, stack, reason)
                .map_err(std::io::Error::other)?;
        }
        Ok(())
    }

    /// Roll a block's chance drop on the authoritative simulation stream and
    /// bind magical results before any local entity or network delivery can
    /// observe them.
    pub fn roll_bonus_drop_at(
        &mut self,
        at: crate::planet::BlockPos,
        block: BlockId,
        rng: &mut u32,
    ) -> Option<ItemStack> {
        let (item, chance) = self.reg.block(block).bonus_drop?;
        *rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let roll = (*rng >> 8) as f32 / (1 << 24) as f32;
        if roll >= chance {
            return None;
        }
        let mut stack = ItemStack::new(&self.reg, item, 1);
        if let Err(error) = self.bind_arcane_stack_at(at, &mut stack, "magical bonus harvest") {
            eprintln!("arcane: magical bonus drop cancelled: {error}");
            return None;
        }
        Some(stack)
    }
}
