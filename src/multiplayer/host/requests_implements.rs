//! Authenticated implements request adapter.

use super::{
    C2S, EntityPos, HostFx, HostSession, PlayerRuntime, S2C, Server, discovery_reachable,
    operate_guest_working, refresh_held,
};

impl HostSession {
    pub(super) fn request_implements(
        &mut self,
        server: &mut Server,
        id: u32,
        msg: C2S,
        fx: &mut Vec<HostFx>,
        implement_observers: Vec<(u32, EntityPos)>,
    ) {
        let Some(guest) = self.guests.get_mut(&id) else {
            return;
        };
        match msg {
            C2S::OperateBindingFrame {
                pos,
                slot,
                action,
                expected_revision,
            } => {
                if guest.action_cooldown > 0.0
                    || !discovery_reachable(&server.world, guest, pos)
                    || server
                        .world
                        .reg
                        .block(server.world.get_block_at(pos))
                        .interaction
                        .as_deref()
                        != Some("binding_frame")
                {
                    return;
                }
                let index = usize::from(slot);
                let actor = guest.player_id.to_string();
                match server.world.operate_binding_frame(
                    pos,
                    &mut guest.inventory,
                    index,
                    action,
                    expected_revision,
                    &actor,
                ) {
                    Ok(result) => {
                        guest.action_cooldown = 0.25;
                        refresh_held(guest);
                        let event_pos = pos.entity_center();
                        let visual =
                            server
                                .world
                                .block_entity_at(&pos)
                                .and_then(|entity| match entity {
                                    crate::world::BlockEntity::BindingFrame(frame) => frame
                                        .output
                                        .and_then(|stack| server.world.implement_visual(stack)),
                                    _ => None,
                                });
                        for (observer, observer_pos) in &implement_observers {
                            if *observer != id
                                && observer_pos.horizontal_distance_to(event_pos) <= 96.0
                            {
                                self.net.send(
                                    *observer,
                                    &S2C::ImplementActivation {
                                        actor: id,
                                        pos: event_pos,
                                        cue: result.cue,
                                        visual,
                                    },
                                );
                            }
                        }
                        fx.push(HostFx::ImplementActivation {
                            pos: event_pos,
                            cue: result.cue,
                            visual,
                        });
                        self.net.send(id, &S2C::BindingFrameResult { pos, result });
                        self.send_player_state(id);
                    }
                    Err(error) => {
                        let revision = match server.world.block_entity_at(&pos) {
                            Some(crate::world::BlockEntity::BindingFrame(frame)) => frame.revision,
                            _ => 0,
                        };
                        self.net.send(
                            id,
                            &S2C::BindingFrameResult {
                                pos,
                                result: crate::implements::FrameResult {
                                    success: false,
                                    revision,
                                    cue: crate::implements::error_cue(&error),
                                    message: error,
                                    preview: None,
                                    lines: Vec::new(),
                                },
                            },
                        );
                    }
                }
            }
            C2S::OperateWorking {
                working_id,
                held_instance,
                target,
                intent,
            } => {
                if guest.action_cooldown > 0.0
                    && matches!(
                        intent,
                        crate::workings::WorkingIntent::Start
                            | crate::workings::WorkingIntent::StartForced
                    )
                {
                    return;
                }
                let prior = guest.active_working.and_then(|active| {
                    server
                        .world
                        .working_cues()
                        .into_iter()
                        .find(|cue| cue.stable_id == active)
                });
                match operate_guest_working(
                    &mut server.world,
                    guest,
                    &working_id,
                    held_instance,
                    target,
                    intent,
                ) {
                    Ok(mut result) => {
                        if result.phase == Some(crate::workings::WorkingPhase::PendingApply) {
                            let checkpoint = self
                                .profiles
                                .as_ref()
                                .ok_or_else(|| {
                                    "The authoritative profile store is unavailable.".to_string()
                                })
                                .and_then(|profiles| {
                                    profiles
                                        .save(&PlayerRuntime::from_guest(guest), &server.world.reg)
                                        .map_err(|error| error.to_string())
                                });
                            match checkpoint.and_then(|()| {
                                server.world.finish_inventory_working(result.stable_id)
                            }) {
                                Ok(finished) => result = finished,
                                Err(message) => {
                                    guest.active_working = Some(result.stable_id);
                                    self.net.send(
                                        id,
                                        &S2C::WorkingResult(crate::workings::WorkingResult {
                                            success: false,
                                            stable_id: result.stable_id,
                                            phase: Some(
                                                crate::workings::WorkingPhase::PendingApply,
                                            ),
                                            cue: crate::workings::WorkingCueKind::Strain,
                                            warning_band: result.warning_band,
                                            message: format!(
                                                "Fieldmend landed but its profile checkpoint failed: {message}"
                                            ),
                                        }),
                                    );
                                    return;
                                }
                            }
                        }
                        guest.action_cooldown = if matches!(
                            intent,
                            crate::workings::WorkingIntent::Start
                                | crate::workings::WorkingIntent::StartForced
                                | crate::workings::WorkingIntent::Release
                        ) {
                            crate::workings::WAND_RECOVERY_SECONDS
                        } else {
                            0.05
                        };
                        let mut cue = server
                            .world
                            .working_cues()
                            .into_iter()
                            .find(|cue| cue.stable_id == result.stable_id)
                            .or(prior);
                        if let Some(cue) = cue.as_mut()
                            && result.phase.is_none()
                        {
                            cue.kind = result.cue;
                            cue.completion_permille = 1_000;
                        }
                        if let Some(cue) = cue {
                            let event_pos = cue.source.entity_center();
                            for (observer, observer_pos) in &implement_observers {
                                if *observer != id
                                    && observer_pos.horizontal_distance_to(event_pos) <= 96.0
                                {
                                    self.net.send(*observer, &S2C::WorkingEvent(cue.clone()));
                                }
                            }
                            fx.push(HostFx::WorkingEvent(cue));
                        }
                        self.net.send(id, &S2C::WorkingResult(result));
                        self.send_player_state(id);
                    }
                    Err(message) => self.net.send(
                        id,
                        &S2C::WorkingResult(crate::workings::WorkingResult {
                            success: false,
                            stable_id: guest.active_working.unwrap_or_default(),
                            phase: None,
                            cue: crate::workings::WorkingCueKind::Refuse,
                            warning_band: 0,
                            message,
                        }),
                    ),
                }
            }

            _ => {}
        }
    }
}
