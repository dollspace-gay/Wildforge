//! Repair alchemy transaction coordination.

use crate::alchemy::AlchemyAuditEvent;
use crate::alchemy::AlchemyCue;
use crate::alchemy::AlchemyCueKind;
use crate::alchemy::AlchemyRequest;
use crate::alchemy::AlchemyResult;
use crate::alchemy::ApparatusKind;
use crate::planet::BlockPos;
use crate::alchemy::DisposalRoute;
use crate::inventory::Inventory;
use crate::world::World;
use super::produced;
use super::result_for;

impl World {
    pub(super) fn alchemy_repair(
        &mut self,
        pos: BlockPos,
        request: &AlchemyRequest,
        material_slot: usize,
        state: &mut crate::alchemy::AlchemyState,
        inventory: &mut Inventory,
    ) -> Result<AlchemyResult, String> {
        let snapshot = state
            .apparatus
            .get(&pos)
            .cloned()
            .ok_or("The apparatus is not installed.")?;
        if snapshot.integrity_permille >= 1_000 {
            return Err("The apparatus is already structurally sound.".into());
        }
        if snapshot.batch.is_some()
            || !snapshot.residue_materials.is_empty()
            || snapshot.filter_burden != 0
            || snapshot.filter_medium.is_some()
        {
            return Err(
                "Drain and clean the apparatus before repairing its embodied structure.".into(),
            );
        }
        let material = inventory
            .slots
            .get(material_slot)
            .copied()
            .flatten()
            .ok_or("Hold one matching repair material.")?;
        let material_name = self.reg.item(material.item).name.as_str();
        let matching = match snapshot.kind {
            ApparatusKind::Mortar => material_name == "base:cobblestone",
            ApparatusKind::InfusionBasin => {
                matches!(material_name, "base:glass" | "base:bronze_ingot")
            }
            ApparatusKind::Alembic => matches!(
                material_name,
                "base:glass" | "base:copper_ingot" | "base:bronze_ingot"
            ),
            ApparatusKind::FilterStand => {
                matches!(material_name, "base:glass" | "base:filter_cloth")
                    || self
                        .reg
                        .tags
                        .get("base:planks")
                        .is_some_and(|items| items.contains(&material.item))
            }
        };
        if !matching {
            return Err(match snapshot.kind {
                ApparatusKind::Mortar => "Repair this mortar with cobblestone.",
                ApparatusKind::InfusionBasin => "Repair this basin with glass or bronze.",
                ApparatusKind::Alembic => "Repair this alembic with glass, copper, or bronze.",
                ApparatusKind::FilterStand => {
                    "Repair this stand with glass, filter cloth, or matching planks."
                }
            }
            .into());
        }
        let hammer_slot = inventory
            .slots
            .iter()
            .position(|stack| stack.is_some_and(|stack| self.reg.item(stack.item).hammer))
            .ok_or("Structural apparatus repair requires a carried hammer.")?;
        let consumed = inventory
            .take_one_stack(material_slot)
            .ok_or("The matching repair material moved before use.")?;
        let repair_materials = crate::materials::stack_materials(&self.reg, consumed);
        inventory.wear_tool(&self.reg, hammer_slot);

        let operation_id = state
            .allocate_operation_id()
            .map_err(|error| error.to_string())?;
        let now = self.alchemy_tick();
        let apparatus = state
            .apparatus
            .get_mut(&pos)
            .ok_or("The apparatus disappeared during repair.")?;
        let before = apparatus.integrity_permille;
        apparatus.integrity_permille = apparatus.integrity_permille.saturating_add(250).min(1_000);
        apparatus.revision = apparatus.revision.saturating_add(1);
        apparatus.last_operator = request.actor;
        let after = apparatus.integrity_permille;
        state.record(AlchemyAuditEvent {
            operation_id,
            installation_id: snapshot.installation_id,
            batch_id: 0,
            actor: request.actor,
            action: "repair".into(),
            preparation_id: "base:apparatus_repair".into(),
            volume_units: 0,
            current_units: 0,
            dross_units: 0,
            tick: now,
            note: format!("consumed one {material_name}; structural integrity {before}->{after}"),
        });
        let staged_material = self
            .material_ledger
            .as_ref()
            .map(|ledger| ledger.stage_linked_consumption(&repair_materials))
            .transpose()
            .map_err(|error| error.to_string())?
            .flatten();
        if staged_material.is_some() {
            self.commit_alchemy_current_with_material(
                state.clone(),
                operation_id,
                "base:apparatus_repair",
                "matching repair material entered the apparatus in the same linked commit",
                Vec::new(),
                Vec::new(),
                staged_material,
            )?;
        }
        result_for(
            &self.reg,
            state,
            pos,
            AlchemyCueKind::Clean,
            "Matching material and hammer work restore a bounded part of the apparatus; no upgrade multiplies yield.",
            None,
        )
    }

    pub(super) fn alchemy_dismantle(
        &mut self,
        pos: BlockPos,
        _request: &AlchemyRequest,
        _route: DisposalRoute,
        state: &mut crate::alchemy::AlchemyState,
    ) -> Result<AlchemyResult, String> {
        let apparatus = state
            .apparatus
            .get(&pos)
            .ok_or("The apparatus is not installed.")?;
        if apparatus.batch.is_some()
            || state.ordinary_jobs.contains_key(&pos)
            || !apparatus.residue_materials.is_empty()
            || apparatus.filter_burden != 0
            || apparatus.filter_medium.is_some()
        {
            return Err("Drain and clean every batch, ordinary process, residue, and spent filter before dismantling.".into());
        }
        let apparatus = state
            .apparatus
            .remove(&pos)
            .expect("validated apparatus remained installed");
        Ok(AlchemyResult {
            installation_id: apparatus.installation_id,
            batch_id: 0,
            revision: apparatus.revision.saturating_add(1),
            preparation_id: None,
            outcome: None,
            volume_units: 0,
            doses_remaining: 0,
            temperature_millic: apparatus.temperature_millic,
            cleanliness_permille: apparatus.cleanliness_permille,
            next_step: None,
            cue: AlchemyCue {
                pos,
                installation_id: apparatus.installation_id,
                batch_id: 0,
                revision: apparatus.revision.saturating_add(1),
                kind: AlchemyCueKind::Clean,
                intensity: 48,
                color: [170, 170, 160],
                message: "The empty apparatus is released to ordinary block dismantling.".into(),
            },
            produced: None,
        })
    }
}
