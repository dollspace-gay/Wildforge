//! Authenticated alchemy request adapter.

use super::{C2S, EntityPos, HostFx, HostSession, S2C, Server, discovery_reachable, refresh_held};

impl HostSession {
    pub(super) fn request_alchemy(&mut self, server: &mut Server, id: u32, msg: C2S, fx: &mut Vec<HostFx>, implement_observers: Vec<(u32, EntityPos)>) {
        let Some(guest) = self.guests.get_mut(&id) else { return; };
        match msg {
            C2S::OperateAlchemy {
                pos,
                expected_revision,
                action,
            } => {
                if guest.action_cooldown > 0.0
                    || !discovery_reachable(&server.world, guest, pos)
                    || !matches!(
                        server
                            .world
                            .reg
                            .block(server.world.get_block_at(pos))
                            .interaction
                            .as_deref(),
                        Some(
                            "alchemy_mortar"
                                | "alchemy_basin"
                                | "alchemy_alembic"
                                | "alchemy_filter"
                        )
                    )
                {
                    return;
                }
                let request = crate::alchemy::AlchemyRequest {
                    actor: guest.player_id.0,
                    actor_label: guest.name.clone(),
                    expected_revision,
                    action,
                };
                match server
                    .world
                    .operate_alchemy(pos, &mut guest.inventory, request)
                {
                    Ok(result) => {
                        guest.action_cooldown = 0.15;
                        refresh_held(guest);
                        let cue = result.cue.clone();
                        for (observer, observer_pos) in &implement_observers {
                            if *observer != id
                                && observer_pos.horizontal_distance_to(cue.pos.entity_center())
                                    <= 96.0
                            {
                                self.net.send(*observer, &S2C::AlchemyEvent(cue.clone()));
                            }
                        }
                        fx.push(HostFx::AlchemyEvent(cue));
                        self.net.send(id, &S2C::AlchemyResult { pos, result });
                        self.send_player_state(id);
                    }
                    Err(error) => self.net.send(id, &S2C::Toast(error)),
                }
            }
            C2S::UsePreparation { slot, target } => {
                if guest.action_cooldown > 0.0 {
                    return;
                }
                let Some(actor_pos) = guest.pos.block() else {
                    return;
                };
                let target_is_reachable = match target {
                    crate::alchemy::AlchemyTarget::SelfActor => true,
                    crate::alchemy::AlchemyTarget::Plot(pos)
                    | crate::alchemy::AlchemyTarget::Surface(pos) => {
                        discovery_reachable(&server.world, guest, pos)
                    }
                    crate::alchemy::AlchemyTarget::Item(item_id) => item_id != 0,
                };
                if !target_is_reachable {
                    return;
                }
                match server.world.use_preparation(
                    guest.player_id.0,
                    &guest.name,
                    actor_pos,
                    &mut guest.inventory,
                    usize::from(slot),
                    target,
                ) {
                    Ok(result) => {
                        guest.action_cooldown = 0.3;
                        refresh_held(guest);
                        let cue = result.cue.clone();
                        for (observer, observer_pos) in &implement_observers {
                            if *observer != id
                                && observer_pos.horizontal_distance_to(cue.pos.entity_center())
                                    <= 96.0
                            {
                                self.net.send(*observer, &S2C::AlchemyEvent(cue.clone()));
                            }
                        }
                        fx.push(HostFx::AlchemyEvent(cue));
                        self.net.send(id, &S2C::PreparationResult(result));
                        self.send_player_state(id);
                    }
                    Err(error) => self.net.send(id, &S2C::Toast(error)),
                }
            }

            _ => {}
        }
    }
}
