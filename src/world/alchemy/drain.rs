//! Drain alchemy transaction coordination.

use super::add_materials;
use super::result_for;
use crate::alchemy::AlchemyAuditEvent;
use crate::alchemy::AlchemyCueKind;
use crate::alchemy::AlchemyRequest;
use crate::alchemy::AlchemyResult;
use crate::alchemy::DisposalRoute;
use crate::arcane::ArcaneOwner;
use crate::arcane::Current;
use crate::planet::BlockPos;
use crate::registry::MaterialVector;
use crate::world::World;

impl World {
    pub(super) fn alchemy_drain(
        &mut self,
        pos: BlockPos,
        request: &AlchemyRequest,
        route: DisposalRoute,
        state: &mut crate::alchemy::AlchemyState,
    ) -> Result<AlchemyResult, String> {
        if route == DisposalRoute::SealedWaste {
            return Err("Sealed disposal needs a physical filter and cleaning action; it is not a delete button.".into());
        }
        let apparatus = state
            .apparatus
            .get(&pos)
            .ok_or("The apparatus is not installed.")?;
        let batch = apparatus
            .batch
            .as_ref()
            .ok_or("There is no batch to drain.")?
            .clone();
        let installation_id = apparatus.installation_id;
        let clean_owner = ArcaneOwner::Alchemy(crate::alchemy::batch_owner_id(batch.id));
        let dross_owner = ArcaneOwner::AlchemyDross(crate::alchemy::batch_owner_id(batch.id));
        let clean = self
            .arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.account(&clean_owner))
            .map_or_else(Current::default, |account| account.current.clone());
        let dross = self
            .arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.account(&dross_owner))
            .map_or_else(Current::default, |account| account.current.clone());
        let mut environmental = clean.clone();
        environmental
            .checked_add(&dross)
            .map_err(|error| error.to_string())?;
        let operation_id = state
            .allocate_operation_id()
            .map_err(|error| error.to_string())?;
        let now = self.alchemy_tick();
        let wastewater = batch
            .liquid
            .water
            .checked_add(batch.residue_water)
            .ok_or("Batch wastewater custody overflowed.")?;
        let atlas_pos = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()));
        self.preflight_industrial_water_return(
            pos,
            wastewater,
            !matches!(route, DisposalRoute::Soil),
        )?;
        let mut materials = MaterialVector::new();
        for ingredient in &batch.ingredients {
            add_materials(&mut materials, &ingredient.retained_materials)?;
            add_materials(&mut materials, &ingredient.residue_materials)?;
        }
        let pollution = state.pollution.entry(pos).or_default();
        pollution.water = pollution
            .water
            .checked_add(wastewater)
            .ok_or("Local wastewater custody overflowed.")?;
        if let Some(carrier) = batch.liquid.carrier {
            let stored = pollution.carrier_units.entry(carrier).or_default();
            *stored = stored
                .checked_add(batch.liquid.volume_units)
                .ok_or("Local carrier pollution overflowed.")?;
        }
        add_materials(&mut pollution.materials, &materials)?;
        add_materials(&mut pollution.solutes, &batch.liquid.solutes)?;
        pollution
            .dross
            .checked_add(&environmental)
            .map_err(|error| error.to_string())?;
        pollution.last_actor = request.actor;
        pollution.last_operation_id = operation_id;
        pollution.updated_tick = now;
        let apparatus = state
            .apparatus
            .get_mut(&pos)
            .ok_or("The apparatus disappeared.")?;
        apparatus.batch = None;
        // Installed/spent filter media stays in the stand. Dumping the liquid
        // is not a way to delete its hazardous burden or physical material.
        apparatus.cleanliness_permille = apparatus.cleanliness_permille.saturating_sub(250);
        apparatus.revision = apparatus.revision.saturating_add(1);
        apparatus.last_operator = request.actor;
        state.record(AlchemyAuditEvent {
            operation_id,
            installation_id,
            batch_id: batch.id,
            actor: request.actor,
            action: format!("drain_{route:?}").to_lowercase(),
            preparation_id: batch.preparation_id.clone(),
            volume_units: batch.liquid.volume_units,
            current_units: clean.total(),
            dross_units: dross.total(),
            tick: now,
            note: "harmful dumping retained exact local custody and attribution".into(),
        });
        if !environmental.is_empty() {
            let region =
                atlas_pos.ok_or("Environmental dross settlement needs the planet atlas.")?;
            let medium = match route {
                DisposalRoute::Soil => crate::arcane::DrossMedium::Soil,
                DisposalRoute::Runoff => crate::arcane::DrossMedium::Water,
                DisposalRoute::Air => crate::arcane::DrossMedium::Air,
                DisposalRoute::SealedWaste => unreachable!(),
            };
            let mut debits = Vec::new();
            if !clean.is_empty() {
                debits.push((clean_owner, clean));
            }
            if !dross.is_empty() {
                debits.push((dross_owner, dross));
            }
            self.commit_alchemy_current(
                state.clone(),
                operation_id,
                &batch.preparation_id,
                "dumped alchemy became attributed local dross instead of disappearing",
                debits,
                vec![(ArcaneOwner::Dross { region, medium }, environmental, None)],
            )?;
        } else {
            self.persist_alchemy_state(state.clone())?;
        }
        self.apply_industrial_water_return(pos, wastewater, !matches!(route, DisposalRoute::Soil))?;
        if let Some(ledger) = &mut self.material_ledger {
            ledger
                .bury_materials(pos, &materials, "alchemy batch dumped into local pollution")
                .map_err(|error| error.to_string())?;
        }
        result_for(
            &self.reg,
            state,
            pos,
            AlchemyCueKind::Leak,
            "The batch is gone from the vessel because its water, chemicals, material, and dross now pollute the selected local medium.",
            None,
        )
    }
}
