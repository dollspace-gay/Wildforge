//! Dispatch alchemy transaction coordination.

use crate::alchemy::AlchemyCueKind;
use crate::alchemy::AlchemyRequest;
use crate::alchemy::AlchemyResult;
use crate::alchemy::ApparatusAction;
use crate::planet::BlockPos;
use crate::inventory::Inventory;
use crate::world::World;
use super::ensure_apparatus;
use super::result_for;

impl World {
    /// The one native dispatch used by local play, hosted guests, automation,
    /// and agents. Clients supply intent, a revision precondition, and
    /// inventory slot choices; the host reconstructs every item, volume,
    /// process, time, and Current consequence.
    pub fn operate_alchemy(
        &mut self,
        pos: BlockPos,
        inventory: &mut Inventory,
        request: AlchemyRequest,
    ) -> Result<AlchemyResult, String> {
        if request.actor == [0; 16]
            || request.actor_label.trim().is_empty()
            || request.actor_label.len() > crate::alchemy::MAX_PREPARATION_ID_BYTES
        {
            return Err("Alchemy needs a stable, bounded operator identity.".into());
        }
        let kind = self.alchemy_kind_at(pos)?;
        // Inspection is the hot UI polling path. Once an apparatus has a
        // stable installation record it is read-only, so answer directly from
        // authoritative state instead of cloning a settlement-wide census.
        if matches!(&request.action, ApparatusAction::Inspect) {
            let state = self
                .alchemy_state
                .as_ref()
                .ok_or("The authoritative alchemy state is unavailable.")?;
            if let Some(apparatus) = state.apparatus.get(&pos) {
                if apparatus.kind != kind {
                    return Err("Saved apparatus kind disagrees with the world block.".into());
                }
                if request
                    .expected_revision
                    .is_some_and(|revision| revision != apparatus.revision)
                {
                    return Err(format!(
                        "The apparatus changed (expected revision {}, found {}); inspect it before retrying.",
                        request.expected_revision.unwrap_or_default(),
                        apparatus.revision
                    ));
                }
                return result_for(
                    &self.reg,
                    state,
                    pos,
                    AlchemyCueKind::Bubble,
                    "The apparatus is inspected.",
                    None,
                );
            }
        }
        let mut next_state = self
            .alchemy_state
            .clone()
            .ok_or("The authoritative alchemy state is unavailable.")?;
        let mut next_inventory = inventory.clone();
        ensure_apparatus(&mut next_state, pos, kind, request.actor)
            .map_err(|error| error.to_string())?;
        let actual_revision = next_state
            .apparatus
            .get(&pos)
            .map(|apparatus| apparatus.revision)
            .unwrap_or_default();
        if request
            .expected_revision
            .is_some_and(|revision| revision != actual_revision)
        {
            return Err(format!(
                "The apparatus changed (expected revision {}, found {actual_revision}); inspect it before retrying.",
                request.expected_revision.unwrap_or_default()
            ));
        }

        let result = match request.action.clone() {
            ApparatusAction::Inspect => result_for(
                &self.reg,
                &next_state,
                pos,
                AlchemyCueKind::Bubble,
                "The apparatus is inspected.",
                None,
            ),
            ApparatusAction::Begin { preparation_id } => {
                self.alchemy_begin(pos, &request, preparation_id, &mut next_state)
            }
            ApparatusAction::Grind { inventory_slot } => self.alchemy_grind(
                pos,
                &request,
                usize::from(inventory_slot),
                &mut next_state,
                &mut next_inventory,
            ),
            ApparatusAction::TransferMash { destination } => {
                self.alchemy_transfer_mash(pos, destination, &request, &mut next_state)
            }
            ApparatusAction::LoadCarrier { inventory_slot } => self.alchemy_load_carrier(
                pos,
                &request,
                usize::from(inventory_slot),
                &mut next_state,
                &mut next_inventory,
            ),
            ApparatusAction::LoadFilter { inventory_slot } => self.alchemy_load_filter(
                pos,
                &request,
                usize::from(inventory_slot),
                &mut next_state,
                &mut next_inventory,
            ),
            ApparatusAction::SetHeat { temperature_millic } => {
                self.alchemy_set_heat(pos, &request, temperature_millic, &mut next_state)
            }
            ApparatusAction::SetAgitation { agitation } => {
                self.alchemy_set_agitation(pos, &request, agitation, &mut next_state)
            }
            ApparatusAction::Advance { step } => {
                self.alchemy_advance(pos, &request, step, &mut next_state)
            }
            ApparatusAction::Charge {
                inventory_slot,
                units,
            } => self.alchemy_charge(
                pos,
                &request,
                inventory_slot.map(usize::from),
                units,
                &mut next_state,
                &next_inventory,
            ),
            ApparatusAction::Sample => self.alchemy_sample(pos, &request, &mut next_state),
            ApparatusAction::Decant { vessel_slot } => self.alchemy_decant(
                pos,
                &request,
                usize::from(vessel_slot),
                &mut next_state,
                &mut next_inventory,
            ),
            ApparatusAction::Clean {
                water_slot,
                filter_slot,
            } => self.alchemy_clean(
                pos,
                &request,
                usize::from(water_slot),
                filter_slot.map(usize::from),
                &mut next_state,
                &mut next_inventory,
            ),
            ApparatusAction::Repair { material_slot } => self.alchemy_repair(
                pos,
                &request,
                usize::from(material_slot),
                &mut next_state,
                &mut next_inventory,
            ),
            ApparatusAction::Drain { route } => {
                self.alchemy_drain(pos, &request, route, &mut next_state)
            }
            ApparatusAction::Dismantle { route } => {
                self.alchemy_dismantle(pos, &request, route, &mut next_state)
            }
            ApparatusAction::FermentAlcohol {
                water_slot,
                wheat_slot,
                berry_slot,
            } => self.alchemy_ferment(
                pos,
                &request,
                [
                    usize::from(water_slot),
                    usize::from(wheat_slot),
                    usize::from(berry_slot),
                ],
                &mut next_state,
                &mut next_inventory,
            ),
            ApparatusAction::PressOil { seed_slot } => self.alchemy_press_oil(
                pos,
                &request,
                usize::from(seed_slot),
                &mut next_state,
                &mut next_inventory,
            ),
        }?;
        self.persist_alchemy_state(next_state)?;
        *inventory = next_inventory;
        Ok(result)
    }
}
