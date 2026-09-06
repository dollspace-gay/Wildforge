//! Working context for authenticated host operations.

use super::{Guest, World, discovery_reachable};

pub(super) fn operate_guest_working(
    world: &mut World,
    guest: &mut Guest,
    working_id: &str,
    held_instance: u64,
    target: crate::workings::WorkingTargetIntent,
    intent: crate::workings::WorkingIntent,
) -> Result<crate::workings::WorkingResult, String> {
    use crate::workings::{WorkingHandler, WorkingIntent, WorkingTargetIntent};

    let source = guest
        .pos
        .block()
        .ok_or("The player is outside a valid working cell.")?;
    let actor = guest.player_id.0;
    match intent {
        WorkingIntent::Start | WorkingIntent::StartForced => {
            let forced = intent == WorkingIntent::StartForced;
            if guest.active_working.is_some() {
                return Err("Finish or cancel the working already in hand.".into());
            }
            if working_id == "base:auto_ritual" {
                if forced {
                    return Err("A physical ritual has no forced wand draw mode.".into());
                }
                let WorkingTargetIntent::Ritual { controller } = target else {
                    return Err("A contextual ritual requires its physical controller.".into());
                };
                if held_instance != 0 || !discovery_reachable(world, guest, controller) {
                    return Err("The ritual controller is out of sight or reach.".into());
                }
                return world.begin_contextual_ritual(actor, &guest.name, controller);
            }
            let definition = world
                .reg
                .workings
                .get(working_id)
                .cloned()
                .ok_or("That working is not registered on this host.")?;
            if definition.mode == crate::workings::DeliveryMode::Ritual {
                if forced {
                    return Err("A physical ritual has no forced wand draw mode.".into());
                }
                let WorkingTargetIntent::Ritual { controller } = target else {
                    return Err("A constructed ritual requires its physical controller.".into());
                };
                if held_instance != 0 {
                    return Err(
                        "Ritual authority belongs to its apparatus, not a held item.".into(),
                    );
                }
                if !discovery_reachable(world, guest, controller) {
                    return Err("The ritual controller is out of sight or reach.".into());
                }
                let result = world.begin_ritual(actor, &guest.name, working_id, controller)?;
                guest.active_working = Some(result.stable_id);
                return Ok(result);
            }
            let held = guest.inventory.slots[guest.hotbar]
                .ok_or("A physical wand must be held to begin a working.")?;
            if held.arcane_id == 0
                || held.arcane_id != held_instance
                || world
                    .reg
                    .item(held.item)
                    .implement
                    .as_ref()
                    .is_none_or(|definition| {
                        definition.kind != crate::implements::ImplementItemKind::Wand
                    })
            {
                return Err(
                    "The requested held instance is not the host-authoritative wand.".into(),
                );
            }
            if definition.mode != crate::workings::DeliveryMode::Wand {
                return Err(
                    "A constructed ritual cannot be requested as a held wand working.".into(),
                );
            }
            let result = world.begin_wand_working(
                actor,
                &guest.name,
                source,
                held_instance,
                working_id,
                target,
                Some(&guest.inventory),
                forced,
            )?;
            guest.active_working = Some(result.stable_id);
            Ok(result)
        }
        WorkingIntent::Hold | WorkingIntent::Release | WorkingIntent::Cancel => {
            let active = guest
                .active_working
                .ok_or("There is no active working to hold, release, or cancel.")?;
            let transaction = world
                .workings_state
                .as_ref()
                .and_then(|state| state.active.get(&active))
                .ok_or("The host no longer has that active working.")?;
            let apparatus_matches = match transaction.apparatus {
                crate::workings::WorkingApparatus::Wand { instance_id, .. } => {
                    instance_id == held_instance
                }
                crate::workings::WorkingApparatus::Ritual { controller, .. } => {
                    held_instance == 0
                        && matches!(target, WorkingTargetIntent::Ritual { controller: at } if at == controller)
                }
            };
            if transaction.actor != actor
                || transaction.definition.id != working_id
                || !apparatus_matches
            {
                return Err("Working identity, actor, or held apparatus no longer matches.".into());
            }
            let handler = transaction.definition.handler;
            if intent != WorkingIntent::Cancel && !world.wand_working_reachable_from(active, source)
            {
                guest.active_working = None;
                let mut result = world.interrupt_working(active)?;
                result.message =
                    "The wand path leaves its bounded reach and breaks cleanly.".into();
                return Ok(result);
            }
            let result = match intent {
                WorkingIntent::Hold => world.activate_working(active),
                WorkingIntent::Release if handler == WorkingHandler::Fieldmend => {
                    world.complete_inventory_working(active, &mut guest.inventory)
                }
                WorkingIntent::Release => world.release_working(active),
                WorkingIntent::Cancel => world.cancel_working(active),
                WorkingIntent::Start | WorkingIntent::StartForced => unreachable!(),
            }?;
            if !matches!(intent, WorkingIntent::Hold) {
                guest.active_working = None;
            }
            Ok(result)
        }
    }
}
