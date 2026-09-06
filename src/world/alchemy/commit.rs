//! Commit alchemy transaction coordination.

use crate::alchemy::ApparatusKind;
use crate::arcane::AccountRead;
use crate::arcane::ArcaneAuthority;
use crate::arcane::ArcaneMove;
use crate::arcane::ArcaneOwner;
use crate::arcane::ArcaneTransaction;
use crate::arcane::Current;
use crate::arcane::LinkedFileReplacement;
use crate::planet::BlockPos;
use crate::world::World;
use std::collections::BTreeMap;

impl World {
    pub(super) fn alchemy_tick(&self) -> u64 {
        (self.calendar_state.clock().max(0.0) * 20.0).round() as u64
    }

    pub(super) fn alchemy_kind_at(&self, pos: BlockPos) -> Result<ApparatusKind, String> {
        match self
            .reg
            .block(self.get_block_at(pos))
            .interaction
            .as_deref()
        {
            Some("alchemy_mortar") => Ok(ApparatusKind::Mortar),
            Some("alchemy_basin") => Ok(ApparatusKind::InfusionBasin),
            Some("alchemy_alembic") => Ok(ApparatusKind::Alembic),
            Some("alchemy_filter") => Ok(ApparatusKind::FilterStand),
            _ => Err("That block is not an alchemy apparatus.".into()),
        }
    }

    pub(super) fn persist_alchemy_state(
        &mut self,
        next_state: crate::alchemy::AlchemyState,
    ) -> Result<(), String> {
        // Ordinary world mutations use the normal autosave/shutdown barrier,
        // just like inventories, chunks, and block entities. Synchronously
        // rewriting the entire sidecar after every stir or inspection made a
        // large settlement freeze on input and still was not atomic with the
        // player's separately saved inventory. Current-moving operations use
        // `commit_alchemy_current*` below and retain their linked durable
        // replacement; this path only installs a validated in-memory result.
        next_state.validate().map_err(|error| error.to_string())?;
        self.alchemy_state = Some(next_state);
        Ok(())
    }

    pub(super) fn commit_alchemy_current(
        &mut self,
        next_state: crate::alchemy::AlchemyState,
        operation_id: u64,
        content_id: &str,
        reason: &str,
        debits: Vec<(ArcaneOwner, Current)>,
        credits: Vec<(ArcaneOwner, Current, Option<String>)>,
    ) -> Result<(), String> {
        self.commit_alchemy_current_with_material(
            next_state,
            operation_id,
            content_id,
            reason,
            debits,
            credits,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn commit_alchemy_current_with_material(
        &mut self,
        next_state: crate::alchemy::AlchemyState,
        operation_id: u64,
        content_id: &str,
        reason: &str,
        debits: Vec<(ArcaneOwner, Current)>,
        credits: Vec<(ArcaneOwner, Current, Option<String>)>,
        staged_material: Option<(crate::materials::MaterialLedger, Vec<u8>)>,
    ) -> Result<(), String> {
        self.commit_alchemy_current_with_material_and_links(
            next_state,
            operation_id,
            content_id,
            reason,
            debits,
            credits,
            staged_material,
            ArcaneAuthority::System,
            Vec::new(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn commit_alchemy_current_with_material_and_links(
        &mut self,
        next_state: crate::alchemy::AlchemyState,
        operation_id: u64,
        content_id: &str,
        reason: &str,
        debits: Vec<(ArcaneOwner, Current)>,
        credits: Vec<(ArcaneOwner, Current, Option<String>)>,
        staged_material: Option<(crate::materials::MaterialLedger, Vec<u8>)>,
        authority: ArcaneAuthority,
        extra_replacements: Vec<LinkedFileReplacement>,
    ) -> Result<(), String> {
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The finite Current ledger is unavailable.")?;
        let mut reads = BTreeMap::<ArcaneOwner, u64>::new();
        for (owner, _) in &debits {
            reads.insert(owner.clone(), ledger.version_of(owner));
        }
        for (owner, _, _) in &credits {
            reads.insert(owner.clone(), ledger.version_of(owner));
        }
        let transaction = ArcaneTransaction {
            id: ledger
                .system_transaction_id()
                .map_err(|error| error.to_string())?,
            reads: reads
                .into_iter()
                .map(|(owner, expected_version)| AccountRead {
                    owner,
                    expected_version,
                })
                .collect(),
            debits: debits
                .into_iter()
                .filter(|(_, current)| !current.is_empty())
                .map(|(owner, current)| ArcaneMove {
                    owner,
                    current,
                    content_id: None,
                })
                .collect(),
            credits: credits
                .into_iter()
                .filter(|(_, current, _)| !current.is_empty())
                .map(|(owner, current, content_id)| ArcaneMove {
                    owner,
                    current,
                    content_id,
                })
                .collect(),
            transforms: Vec::new(),
            authority,
            reason: reason.into(),
            content_id: content_id.into(),
            linked: Vec::new(),
        };
        let replacement = LinkedFileReplacement {
            subsystem: "alchemy".into(),
            operation_id,
            relative_path: crate::alchemy::ALCHEMY_FILE.into(),
            after: Some(next_state.encode().map_err(|error| error.to_string())?),
        };
        let mut replacements = vec![replacement];
        replacements.extend(extra_replacements);
        if let Some((_, bytes)) = &staged_material {
            replacements.push(LinkedFileReplacement {
                subsystem: "materials".into(),
                operation_id,
                relative_path: crate::materials::MaterialLedger::linked_delta_path().into(),
                after: Some(bytes.clone()),
            });
        }
        ledger
            .commit_linked_files(transaction, replacements)
            .map_err(|error| error.to_string())?;
        self.alchemy_state = Some(next_state);
        if let Some((next_material, _)) = staged_material {
            self.material_ledger = Some(next_material);
        }
        Ok(())
    }
}
