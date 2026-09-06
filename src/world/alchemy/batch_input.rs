//! Batch input alchemy transaction coordination.

use super::ensure_apparatus;
use super::near;
use super::result_for;
use crate::alchemy::AlchemyAuditEvent;
use crate::alchemy::AlchemyBatch;
use crate::alchemy::AlchemyCueKind;
use crate::alchemy::AlchemyRequest;
use crate::alchemy::AlchemyResult;
use crate::alchemy::ApparatusKind;
use crate::alchemy::BatchOutcome;
use crate::alchemy::ExactLiquid;
use crate::alchemy::ProcessStep;
use crate::planet::BlockPos;
use crate::planet_atlas::ReservoirMass;
use crate::world::World;

impl World {
    pub(super) fn alchemy_begin(
        &self,
        pos: BlockPos,
        request: &AlchemyRequest,
        preparation_id: String,
        state: &mut crate::alchemy::AlchemyState,
    ) -> Result<AlchemyResult, String> {
        let definition = self
            .reg
            .preparations
            .get(&preparation_id)
            .ok_or("That preparation is not registered in this world.")?
            .clone();
        let required_kind = if definition.steps.first() == Some(&ProcessStep::Grind) {
            ApparatusKind::Mortar
        } else {
            definition.process.apparatus()
        };
        let now = self.alchemy_tick();
        let batch_id = state
            .allocate_batch_id()
            .map_err(|error| error.to_string())?;
        let operation_id = state
            .allocate_operation_id()
            .map_err(|error| error.to_string())?;
        let apparatus = state
            .apparatus
            .get_mut(&pos)
            .ok_or("The apparatus disappeared before the operation.")?;
        if apparatus.kind != required_kind {
            return Err(format!(
                "{} begins at its {:?}, not this {:?}.",
                definition.label, required_kind, apparatus.kind
            ));
        }
        if apparatus.batch.is_some() || !apparatus.residue_materials.is_empty() {
            return Err("Drain and clean this apparatus before beginning another batch.".into());
        }
        if apparatus.integrity_permille < 100 {
            return Err("This apparatus is too damaged to hold a batch safely.".into());
        }
        apparatus.batch = Some(AlchemyBatch {
            id: batch_id,
            preparation_id: definition.id.clone(),
            definition_version: definition.version,
            actor: request.actor,
            actor_label: request.actor_label.clone(),
            installation_id: apparatus.installation_id,
            liquid: ExactLiquid::default(),
            residue_water: ReservoirMass::default(),
            ingredients: Vec::new(),
            charge_input_units: 0,
            current_units: 0,
            dross_units: 0,
            step_index: 0,
            observations: Vec::new(),
            started_tick: now,
            due_tick: now.saturating_add(definition.process_ticks),
            born_tick: now,
            expires_tick: now.saturating_add(definition.shelf_life_ticks),
            outcome: BatchOutcome::Processing,
            revision: 1,
        });
        apparatus.revision = apparatus.revision.saturating_add(1);
        apparatus.last_operator = request.actor;
        let installation_id = apparatus.installation_id;
        state.record(AlchemyAuditEvent {
            operation_id,
            installation_id,
            batch_id,
            actor: request.actor,
            action: "begin".into(),
            preparation_id: definition.id,
            volume_units: 0,
            current_units: 0,
            dross_units: 0,
            tick: now,
            note: "physical batch identity reserved at its first apparatus".into(),
        });
        result_for(
            &self.reg,
            state,
            pos,
            AlchemyCueKind::Bubble,
            "The measured batch begins; its ingredients remain physical.",
            None,
        )
    }

    pub(super) fn alchemy_transfer_mash(
        &self,
        source: BlockPos,
        destination: BlockPos,
        request: &AlchemyRequest,
        state: &mut crate::alchemy::AlchemyState,
    ) -> Result<AlchemyResult, String> {
        if !near(source, destination) {
            return Err(
                "The receiving apparatus must be within the laboratory transfer reach.".into(),
            );
        }
        let destination_kind = self.alchemy_kind_at(destination)?;
        ensure_apparatus(state, destination, destination_kind, request.actor)
            .map_err(|error| error.to_string())?;
        let mut batch = state
            .apparatus
            .get_mut(&source)
            .and_then(|apparatus| apparatus.batch.take())
            .ok_or("There is no mash to transfer.")?;
        let definition = self
            .reg
            .preparations
            .get(&batch.preparation_id)
            .ok_or("The saved preparation definition is unavailable.")?;
        let next_step = definition.steps.get(usize::from(batch.step_index)).copied();
        let required_destination = match next_step {
            Some(ProcessStep::Grind) => ApparatusKind::Mortar,
            Some(ProcessStep::Filter) => ApparatusKind::FilterStand,
            Some(ProcessStep::Distill) => ApparatusKind::Alembic,
            _ => definition.process.apparatus(),
        };
        if destination_kind != required_destination {
            state
                .apparatus
                .get_mut(&source)
                .expect("source existed")
                .batch = Some(batch);
            return Err(format!(
                "{} must continue in its {:?}.",
                definition.label, required_destination
            ));
        }
        if state
            .apparatus
            .get(&destination)
            .is_some_and(|apparatus| apparatus.batch.is_some())
        {
            state
                .apparatus
                .get_mut(&source)
                .expect("source existed")
                .batch = Some(batch);
            return Err("The receiving apparatus already holds a batch.".into());
        }
        let operation_id = state
            .allocate_operation_id()
            .map_err(|error| error.to_string())?;
        let batch_id = batch.id;
        let preparation_id = batch.preparation_id.clone();
        let now = self.alchemy_tick();
        let destination_apparatus = state
            .apparatus
            .get_mut(&destination)
            .expect("destination was ensured");
        batch.installation_id = destination_apparatus.installation_id;
        batch.revision = batch.revision.saturating_add(1);
        destination_apparatus.batch = Some(batch);
        destination_apparatus.revision = destination_apparatus.revision.saturating_add(1);
        destination_apparatus.last_operator = request.actor;
        let destination_installation = destination_apparatus.installation_id;
        let source_installation = {
            let source_apparatus = state.apparatus.get_mut(&source).expect("source existed");
            source_apparatus.revision = source_apparatus.revision.saturating_add(1);
            source_apparatus.last_operator = request.actor;
            source_apparatus.installation_id
        };
        state.record(AlchemyAuditEvent {
            operation_id,
            installation_id: destination_installation,
            batch_id,
            actor: request.actor,
            action: "transfer_mash".into(),
            preparation_id,
            volume_units: 0,
            current_units: 0,
            dross_units: 0,
            tick: now,
            note: format!("mash transferred from installation {source_installation}"),
        });
        result_for(
            &self.reg,
            state,
            destination,
            AlchemyCueKind::Pour,
            "The mash is physically transferred into its process vessel.",
            None,
        )
    }
}
