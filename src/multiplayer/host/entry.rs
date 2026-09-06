//! Entry for the authoritative host session.

use super::{BlockPos, Guest, HashSet, HostSession, Refusal, RefusalCode, S2C, Server, roster};

impl HostSession {

    pub(super) fn try_finish_pending_entry(&mut self, server: &mut Server, id: u32) {
        let Some(mut pending) = self.pending_guests.remove(&id) else {
            return;
        };
        if pending
            .required
            .iter()
            .any(|position| !server.world.has_chunk(*position))
        {
            self.pending_guests.insert(id, pending);
            return;
        }
        // Terrain is resident before a stale saved position is repaired, so
        // the rescue scan cannot perform cold generation on the host pump.
        pending.runtime.pos = server.world.free_position_at(pending.runtime.pos);
        let required = crate::world::player_entry_chunks(pending.runtime.pos.surface());
        if required
            .iter()
            .any(|position| !server.world.has_chunk(*position))
        {
            pending.required = required;
            self.pending_guests.insert(id, pending);
            return;
        }

        if let Some(at) = pending.runtime.pos.block() {
            let migrated = server.world.migrate_legacy_player_charms(
                at,
                &mut pending.runtime.inventory,
                &mut pending.runtime.armor,
                &mut pending.runtime.cursor,
                &format!("multiplayer profile {}", pending.runtime.player_id),
            );
            if migrated != 0
                && let Some(profiles) = self.profiles.as_ref()
                && let Err(error) = profiles.save(&pending.runtime, &server.world.reg)
            {
                eprintln!(
                    "implements: migrated {migrated} guest charms but profile save failed: {error}"
                );
            }
        }

        match server.world.resume_pending_inventory_workings(
            pending.runtime.player_id.0,
            &mut pending.runtime.inventory,
        ) {
            Ok(ids) if !ids.is_empty() => {
                let checkpoint = self.profiles.as_ref().map_or_else(
                    || {
                        Err(std::io::Error::other(
                            "profile store is unavailable for pending Fieldmend replay",
                        ))
                    },
                    |profiles| profiles.save(&pending.runtime, &server.world.reg),
                );
                if let Err(error) = checkpoint {
                    eprintln!(
                        "workings: resumed guest Fieldmend remains pending after checkpoint failure: {error}"
                    );
                } else {
                    for working in ids {
                        if let Err(error) = server.world.finish_inventory_working(working) {
                            eprintln!(
                                "workings: guest Fieldmend profile landed but finalization failed: {error}"
                            );
                        }
                    }
                }
            }
            Ok(_) => {}
            Err(error) => {
                eprintln!("workings: pending guest Fieldmend is inconsistent: {error}")
            }
        }

        let reg = server.world.reg.clone();
        let palette: Vec<String> = reg.blocks.iter().map(|block| block.name.clone()).collect();
        let items: Vec<String> = reg.items.iter().map(|item| item.name.clone()).collect();
        let mut roster = Vec::new();
        if let Some(host_name) = &self.host_name {
            roster.push(roster::host_presence(host_name));
        }
        roster.extend(
            self.guests
                .iter()
                .filter(|(_, guest)| guest.entry_ready)
                .map(|(id, guest)| roster::guest_presence(*id, guest)),
        );
        let presence = roster::presence(
            id,
            pending.name.clone(),
            &pending.principal,
            pending.verification_cached,
            pending.public_handle.clone(),
        );
        roster.push(presence);
        let role = self
            .moderation
            .as_ref()
            .map(|store| store.role(&pending.principal))
            .unwrap_or_default();
        self.net.send(
            id,
            &S2C::Welcome {
                seed: server.world.seed,
                mode: server.world.mode.clone(),
                time: server.time_of_day,
                ire: server.world.ire,
                palette,
                items,
                your_id: id,
                your_role: role,
                roster,
                spawn: pending.runtime.pos,
                world_name: self.world_name.clone(),
                player_state: pending.runtime.to_snap(),
            },
        );
        self.net.send(
            id,
            &S2C::EntryManifest {
                spawn: pending.runtime.pos,
                required: required.clone(),
            },
        );
        if let Some(ledger) = &server.world.material_ledger {
            for notice in ledger.retrogen_notices() {
                self.net.send(id, &S2C::Toast(notice));
            }
        }
        let signs: Vec<(BlockPos, [String; 3])> = server
            .world
            .sign_texts()
            .map(|(position, sign)| (position, sign.lines.clone()))
            .collect();
        for (pos, lines) in signs {
            self.net.send(id, &S2C::SignText { pos, lines });
        }
        let runtime = pending.runtime;
        self.guests.insert(
            id,
            Guest {
                player_id: runtime.player_id,
                principals: runtime.principals,
                previous_names: runtime.previous_names,
                first_seen: runtime.first_seen,
                name: pending.name,
                principal: pending.principal,
                verification_cached: pending.verification_cached,
                verified_handle: pending.verified_handle,
                public_handle: pending.public_handle,
                pos: runtime.pos,
                yaw: runtime.yaw,
                container: None,
                mob_cargo: None,
                sleeping: false,
                held: runtime.held,
                style: runtime.style,
                inventory: runtime.inventory,
                armor: runtime.armor,
                health: runtime.health,
                hunger: runtime.hunger,
                nutrition: runtime.nutrition,
                bodily_dross: runtime.bodily_dross,
                spawn: runtime.spawn,
                pitch: runtime.pitch,
                hotbar: runtime.hotbar,
                cursor: runtime.cursor,
                craft_grid: [None; 9],
                has_moved: false,
                sprinting: false,
                action_cooldown: 0.0,
                pending_discovery: None,
                active_working: None,
                since_damage: 100.0,
                regen_timer: 0.0,
                hunger_charm_credit: 0.0,
                starve_timer: 0.0,
                chat_count: 0,
                chat_window: 0.0,
                command_count: 0,
                command_window: 0.0,
                airborne_rise: 0.0,
                sent_chunks: HashSet::new(),
                entry_required: required,
                entry_ready: false,
                view_dist: self.initial_view_dist,
                chunk_requests: 0,
                chunk_window: 0.0,
                edits: 0,
                edit_window: 0.0,
                last_arcane_items: Vec::new(),
                last_implements: Vec::new(),
                last_apparatus: Vec::new(),
                render_from: (runtime.pos.render_pos(), 0.0),
                net_age: 0.0,
                net_interval: 0.05,
            },
        );
    }

    pub(super) fn refuse_server_error(&mut self, id: u32, area: &str, error: &std::io::Error) {
        self.net.send(
            id,
            &S2C::Refused(Refusal::new(
                RefusalCode::Server,
                format!("{area} could not be opened: {error}"),
            )),
        );
        self.net.kick(id);
    }
}
