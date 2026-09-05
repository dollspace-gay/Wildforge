//! One container/cursor transaction for local, network, and replica prediction.

use crate::inventory::{ItemStack, click_stack};
use crate::machines::MachineHandler;
use crate::registry::Registry;
use crate::world::{BlockEntity, MachineInstance};

#[derive(Clone, Copy)]
pub(crate) struct Click {
    pub(crate) slot: usize,
    pub(crate) right: bool,
    /// Stable authenticated identity; absent for predictions that cannot edit a stall.
    pub(crate) actor: Option<[u8; 16]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Rejected {
    Missing,
    DepositOnly,
    Unsupported,
    NotOwner,
    Sealed,
    InvalidSlot,
    IneligibleItem,
    EmptyOutput,
    IncompatibleCursor,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Effect {
    /// Adapters keep their existing XP policy; the transaction only reports the transfer.
    pub(crate) took_furnace_output: bool,
}

/// Validate eligibility before committing either participant's state. Network
/// admission/reach/rate checks and replication echoes remain in the adapter.
pub(crate) fn click(
    registry: &Registry,
    entity: &mut BlockEntity,
    cursor: &mut Option<ItemStack>,
    request: Click,
) -> Result<Effect, Rejected> {
    let slot = request.slot;
    match entity {
        BlockEntity::Depot(_) => return Err(Rejected::DepositOnly),
        BlockEntity::Chest(chest) => exchange(
            registry, chest.slots.get_mut(slot).ok_or(Rejected::InvalidSlot)?, cursor, request.right,
        ),
        BlockEntity::Offering(offering) => exchange(
            registry, offering.slots.get_mut(slot).ok_or(Rejected::InvalidSlot)?, cursor, request.right,
        ),
        BlockEntity::Stall(stall) => {
            if request.actor != Some(stall.owner) { return Err(Rejected::NotOwner); }
            let target = match slot {
                0..=5 => &mut stall.goods[slot],
                6 => &mut stall.price,
                7..=12 => &mut stall.till[slot - 7],
                _ => return Err(Rejected::InvalidSlot),
            };
            exchange(registry, target, cursor, request.right);
        }
        BlockEntity::Furnace(furnace) => match slot {
            0 => {
                let previous = furnace.input.map(|stack| stack.item);
                exchange(registry, &mut furnace.input, cursor, request.right);
                if previous != furnace.input.map(|stack| stack.item) { furnace.progress = 0.0; }
            }
            1 => exchange(registry, &mut furnace.fuel, cursor, request.right),
            _ => {
                take_output(registry, &mut furnace.output, cursor)?;
                return Ok(Effect { took_furnace_output: true });
            }
        },
        BlockEntity::Multiblock(machine) => {
            let target = station_slot(registry, machine, slot, *cursor)?;
            exchange(registry, target, cursor, request.right);
        }
        BlockEntity::Clamp(_)
        | BlockEntity::Anvil(_)
        | BlockEntity::Sign(_)
        | BlockEntity::Smoker(_)
        | BlockEntity::Steam(_)
        | BlockEntity::SurveyFolio(_)
        | BlockEntity::DiscoveryApparatus(_)
        | BlockEntity::BindingFrame(_)
        | BlockEntity::ChargeVessel(_)
        | BlockEntity::Switch(_) => return Err(Rejected::Unsupported),
    }
    Ok(Effect::default())
}

fn exchange(
    registry: &Registry,
    target: &mut Option<ItemStack>,
    cursor: &mut Option<ItemStack>,
    right: bool,
) {
    (*target, *cursor) = click_stack(registry, *target, *cursor, right);
}

fn take_output(
    registry: &Registry,
    output: &mut Option<ItemStack>,
    cursor: &mut Option<ItemStack>,
) -> Result<(), Rejected> {
    let produced = output.ok_or(Rejected::EmptyOutput)?;
    let next = match *cursor {
        None => produced,
        Some(held) if held.can_merge(registry, &produced)
            && held.count + produced.count <= registry.item(held.item).max_stack => {
            ItemStack { count: held.count + produced.count, ..held }
        }
        Some(_) => return Err(Rejected::IncompatibleCursor),
    };
    *cursor = Some(next);
    *output = None;
    Ok(())
}

/// Station-specific eligibility is shared with prediction. The returned slot is
/// writable only after the firing, geometry, and item-kind checks have passed.
fn station_slot<'a>(
    registry: &Registry,
    machine: &'a mut MachineInstance,
    slot: usize,
    held: Option<ItemStack>,
) -> Result<&'a mut Option<ItemStack>, Rejected> {
    let handler = machine.kind.handler(registry).ok_or(Rejected::Unsupported)?;
    if !matches!(handler, MachineHandler::Bloomery | MachineHandler::Forge | MachineHandler::Kiln) {
        return Err(Rejected::Unsupported);
    }
    if machine.lit { return Err(Rejected::Sealed); }
    let limit = if handler == MachineHandler::Kiln { 9 } else { 8 };
    if slot >= limit { return Err(Rejected::InvalidSlot); }
    let eligible = match (handler, held) {
        (_, None) => true,
        (MachineHandler::Bloomery, Some(held)) => registry.bloomery.first()
            .is_some_and(|chain| held.item == if slot < 4 { chain.charge } else { chain.fuel }),
        (MachineHandler::Forge, Some(held)) if slot < 4 => {
            registry.smelts.iter().any(|smelt| smelt.input.matches(held.item))
                || registry.forge_salvage.iter().any(|salvage| salvage.input == held.item)
                || crate::materials::is_reclaimable_stock(registry, held.item)
        }
        (MachineHandler::Forge, Some(held)) => registry.fuel_value(held.item).is_some(),
        (MachineHandler::Kiln, Some(held)) => match slot {
            0..=3 => registry.kiln_base.is_some_and(|(sand, _, _)| sand == held.item),
            4 => registry.kiln.iter().any(|recipe| recipe.powder == held.item),
            _ => registry.kiln_base.is_some_and(|(_, fuel, _)| fuel == held.item),
        },
        _ => return Err(Rejected::Unsupported),
    };
    if !eligible { return Err(Rejected::IneligibleItem); }
    match (handler, slot) {
        (_, 0..=3) => Ok(&mut machine.charge[slot]),
        (MachineHandler::Kiln, 4) => Ok(&mut machine.reagent),
        (MachineHandler::Kiln, _) => Ok(&mut machine.fuel[slot - 5]),
        _ => Ok(&mut machine.fuel[slot - 4]),
    }
}
