//! Admission for the authoritative host session.

use super::{C2S, CHUNK_REQUESTS_PER_SECOND, ChunkPos, HostFx, HostSession, MAX_GUEST_VIEW_DIST, S2C, Server, roster};

impl HostSession {

    pub(super) fn on_msg(&mut self, server: &mut Server, id: u32, msg: C2S, fx: &mut Vec<HostFx>) {
        if matches!(&msg, C2S::EntryReady) {
            let accepted = {
                let Some(guest) = self.guests.get_mut(&id) else {
                    return;
                };
                if guest.entry_ready
                    || guest
                        .entry_required
                        .iter()
                        .any(|position| !guest.sent_chunks.contains(position))
                {
                    return;
                }
                guest.entry_ready = true;
                (roster::guest_presence(id, guest), guest.name.clone())
            };
            self.net.send(id, &S2C::EntryAccepted);
            self.broadcast_ready(&S2C::Joined {
                presence: accepted.0,
            });
            fx.push(HostFx::Joined(accepted.1));
            return;
        }
        // An authenticated connection is inert until it has acknowledged
        // every host-declared entry chunk. In particular it cannot move,
        // chat, moderate, request arbitrary terrain, or affect simulation.
        if self.guests.get(&id).is_none_or(|guest| !guest.entry_ready) {
            return;
        }
        if let C2S::Moderate { target, action } = &msg {
            self.on_moderation_request(id, *target, *action);
            return;
        }
        // Both of these are answered before the long-lived guest borrow
        // below, because both have to reach back into the session while
        // holding it would forbid that.
        match &msg {
            C2S::RequestChunk { face, u, v } => {
                let requested = crate::planet::Face::from_u8(*face)
                    .and_then(|face| ChunkPos::new(face, *u, *v).ok());
                let serve = {
                    let Some(g) = self.guests.get_mut(&id) else {
                        return;
                    };
                    if g.chunk_requests >= CHUNK_REQUESTS_PER_SECOND {
                        false
                    } else {
                        g.chunk_requests += 1;
                        // Only ground this guest could be standing near. The
                        // request fills its own holes; it is not a way to
                        // read the map from across the world.
                        let Some(center) = g.pos.chunk() else {
                            return;
                        };
                        requested.is_some_and(|pos| {
                            pos.distance(center) <= f64::from((g.view_dist + 2) * 16)
                        })
                    }
                };
                if serve && let Some(pos) = requested {
                    self.stream_chunk(server, id, pos);
                }
                return;
            }
            C2S::SetViewDistance { chunks } => {
                let granted = (*chunks).clamp(2, MAX_GUEST_VIEW_DIST) as i32;
                let Some(g) = self.guests.get_mut(&id) else {
                    return;
                };
                // Unchanged is free, so a client that resends every frame
                // costs nothing.
                if g.view_dist == granted {
                    return;
                }
                g.view_dist = granted;
                self.net.send(
                    id,
                    &S2C::ViewDistance {
                        chunks: granted as u8,
                    },
                );
                return;
            }
            _ => {}
        }
        let implement_observers = if matches!(
            &msg,
            C2S::OperateBindingFrame { .. }
                | C2S::OperateWorking { .. }
                | C2S::OperateAlchemy { .. }
                | C2S::UsePreparation { .. }
        ) {
            self.guests
                .iter()
                .filter(|(_, guest)| guest.entry_ready)
                .map(|(observer, guest)| (*observer, guest.pos))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let Some(guest) = self.guests.get_mut(&id) else {
            return;
        };
        if !matches!(&msg, C2S::Move { .. }) {
            // The window is SIM time: when the host runs slow, a
            // second stretches, and an honest busy guest (an agent
            // mid-craft) must still fit inside it. Abuse is orders
            // of magnitude past this.
            if guest.command_count >= 160 {
                return;
            }
            guest.command_count += 1;
        }
        match msg {
            C2S::Hello { .. }
            | C2S::Authenticate { .. }
            | C2S::Moderate { .. }
            | C2S::RequestChunk { .. }
            | C2S::SetViewDistance { .. }
            | C2S::EntryReady
            | C2S::Bye => {}
            request @ C2S::Move { .. } => self.request_movement(server, id, request),
            request @ C2S::Break { .. } => self.request_terrain(server, id, request),
            request @ C2S::Scoop { .. } => self.request_terrain(server, id, request),
            request @ C2S::Place { .. } => self.request_terrain(server, id, request),
            request @ C2S::AttackMob { .. } => self.request_animals(server, id, request),
            request @ C2S::FeedMob { .. } => self.request_animals(server, id, request),
            request @ C2S::HackMob { .. } => self.request_animals(server, id, request),
            request @ C2S::LeadMob { .. } => self.request_animals(server, id, request),
            request @ C2S::SaddleMob { .. } => self.request_animals(server, id, request),
            request @ C2S::OpenMobCargo { .. } => self.request_animals(server, id, request),
            request @ C2S::MobCargoClick { .. } => self.request_animals(server, id, request),
            request @ C2S::RideMob { .. } => self.request_animals(server, id, request),
            request @ C2S::StallBuy { .. } => self.request_world_use(server, id, request, fx),
            request @ C2S::SetSign { .. } => self.request_world_use(server, id, request, fx),
            request @ C2S::DepotDeposit { .. } => self.request_world_use(server, id, request, fx),
            request @ C2S::ScreenClick { .. } => self.request_world_use(server, id, request, fx),
            request @ C2S::ToggleSwitch { .. } => self.request_world_use(server, id, request, fx),
            request @ C2S::DungeonUse { .. } => self.request_world_use(server, id, request, fx),
            request @ C2S::BrushBlock { .. } => self.request_field_observation(server, id, request),
            request @ C2S::BeginObserve { .. } => self.request_field_observation(server, id, request),
            request @ C2S::Observe { .. } => self.request_field_observation(server, id, request),
            request @ C2S::ReadKnowledge { .. } => self.request_knowledge(server, id, request),
            request @ C2S::OpenDiscovery { .. } => self.request_knowledge(server, id, request),
            request @ C2S::CopyObservation { .. } => self.request_knowledge(server, id, request),
            request @ C2S::BeginExperiment { .. } => self.request_experiments(server, id, request),
            request @ C2S::SetExperimentItem { .. } => self.request_experiments(server, id, request),
            request @ C2S::RunExperiment { .. } => self.request_experiments(server, id, request),
            request @ C2S::AssembleTuningLens { .. } => self.request_experiments(server, id, request),
            request @ C2S::OperateBindingFrame { .. } => self.request_implements(server, id, request, fx, implement_observers),
            request @ C2S::OperateAlchemy { .. } => self.request_alchemy(server, id, request, fx, implement_observers),
            request @ C2S::UsePreparation { .. } => self.request_alchemy(server, id, request, fx, implement_observers),
            request @ C2S::OperateWorking { .. } => self.request_implements(server, id, request, fx, implement_observers),
            request @ C2S::FireProjectile { .. } => self.request_projectiles(server, id, request),
            request @ C2S::OpenContainer { .. } => self.request_containers(server, id, request),
            request @ C2S::ContainerClick { .. } => self.request_containers(server, id, request),
            request @ C2S::CloseContainer => self.request_containers(server, id, request),
            request @ C2S::LightBloomery { .. } => self.request_containers(server, id, request),
            request @ C2S::LightClamp { .. } => self.request_containers(server, id, request),
            request @ C2S::AnvilPut { .. } => self.request_containers(server, id, request),
            request @ C2S::AnvilStrike { .. } => self.request_containers(server, id, request),
            request @ C2S::AnvilTake { .. } => self.request_containers(server, id, request),
            request @ C2S::SleepRequest => self.request_movement(server, id, request),
            request @ C2S::SleepCancel => self.request_movement(server, id, request),
            request @ C2S::InventoryClick { .. } => self.request_inventory(server, id, request),
            request @ C2S::CraftResult { .. } => self.request_inventory(server, id, request),
            request @ C2S::EatSelected => self.request_inventory(server, id, request),
            request @ C2S::Respawn => self.request_movement(server, id, request),
            request @ C2S::Chat(..) => self.request_chat(server, id, request, fx),
        }
    }
}
