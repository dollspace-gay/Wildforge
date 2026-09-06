//! Damage for the authoritative host session.

use super::{
    HostSession, ItemStack, ProfileStore, S2C, Server, refresh_held, server_item_armor_points,
};

impl HostSession {
    /// Apply simulation damage to server-owned survival state. The `Hit`
    /// packet is presentation; the following `PlayerState` is the authority.
    pub fn hurt_guest(
        &mut self,
        server: &mut Server,
        id: u32,
        amount: f32,
        from: crate::planet::EntityPos,
    ) {
        let ready_observers = self
            .guests
            .iter()
            .filter_map(|(observer, guest)| guest.entry_ready.then_some(*observer))
            .collect::<Vec<_>>();
        let Some(guest) = self.guests.get_mut(&id) else {
            return;
        };
        if !guest.entry_ready || guest.health <= 0.0 {
            return;
        }
        let mut armor_points: u32 = guest
            .armor
            .iter()
            .flatten()
            .filter_map(|stack| server_item_armor_points(stack, self.profiles.as_ref()))
            .sum();
        if let Some(mut charm) = guest.armor[4]
            && let Some(pos) = guest.pos.block()
            && server.world.debit_charm_at(
                pos,
                &mut charm,
                "bark",
                "guest bark charm prevented warden damage",
            )
        {
            guest.armor[4] = Some(charm);
            armor_points = armor_points.saturating_add(crate::implements::BARK_CHARM_ARMOR_POINTS);
        }
        // This mirrors local survival: each point blocks four percent, capped.
        let reduced = amount.max(0.0) * (1.0 - armor_points.min(15) as f32 * 0.04);
        if armor_points > 0
            && let Some(registry) = self.profiles.as_ref().map(ProfileStore::registry_hint)
        {
            for armor in &mut guest.armor {
                if let Some(stack) = armor {
                    if registry.item(stack.item).durability == 0 {
                        continue;
                    }
                    stack.durability = stack.durability.saturating_sub(1);
                    if stack.durability == 0 {
                        *armor = registry
                            .item(stack.item)
                            .broken_into
                            .map(|broken| ItemStack {
                                item: broken,
                                count: 1,
                                durability: 0,
                                arcane_id: stack.arcane_id,
                            });
                    }
                }
            }
        }
        guest.health = (guest.health - reduced).max(0.0);
        guest.since_damage = 0.0;
        if guest.health <= 0.0 {
            guest.active_working = None;
            let prior = server
                .world
                .working_cues()
                .into_iter()
                .filter(|cue| {
                    server
                        .world
                        .workings_state
                        .as_ref()
                        .and_then(|state| state.active.get(&cue.stable_id))
                        .is_some_and(|transaction| transaction.actor == guest.player_id.0)
                })
                .map(|cue| (cue.stable_id, cue))
                .collect::<std::collections::HashMap<_, _>>();
            if let Ok(results) = server.world.interrupt_actor_workings(guest.player_id.0) {
                for result in results {
                    if let Some(mut cue) = prior.get(&result.stable_id).cloned() {
                        cue.kind = result.cue;
                        cue.warning_band = result.warning_band;
                        cue.completion_permille = 1_000;
                        for observer in &ready_observers {
                            self.net.send(*observer, &S2C::WorkingEvent(cue.clone()));
                        }
                    }
                }
            }
            if let Some(actor_pos) = guest.pos.block().or_else(|| guest.spawn.block())
                && let Err(error) = server
                    .world
                    .settle_preparations_on_death(guest.player_id.0, actor_pos)
            {
                eprintln!(
                    "alchemy: hosted death settlement for {} failed: {error}",
                    guest.player_id
                );
            }
            let mut lost = guest.inventory.drain();
            lost.extend(guest.armor.iter_mut().filter_map(Option::take));
            lost.extend(guest.cursor.take());
            lost.extend(guest.craft_grid.iter_mut().filter_map(Option::take));
            // Death moves physical stacks into the world's ordinary drop
            // path. Burying them here destroyed the durable reference while
            // leaving a charged implement account behind (and made hosted
            // death behave differently from local death). A windowed host
            // renders these as loose items; a dedicated host routes them
            // through its bounded delivery/banking policy.
            if let Some(pos) = guest.pos.block().or_else(|| guest.spawn.block()) {
                for stack in lost {
                    server.world.push_drop_at(pos, stack);
                }
            } else {
                // A valid player should always have either a present or spawn
                // block. If corrupted coordinates defeat both, settle every
                // charged identity explicitly instead of leaking custody.
                let fallback = crate::planet::BlockPos::new(
                    guest.pos.face(),
                    guest
                        .pos
                        .u()
                        .floor()
                        .clamp(0.0, f32::from(crate::planet::FACE_BLOCKS - 1))
                        as u16,
                    0,
                    guest
                        .pos
                        .v()
                        .floor()
                        .clamp(0.0, f32::from(crate::planet::FACE_BLOCKS - 1))
                        as u16,
                )
                .expect("clamped player surface is a block");
                for stack in lost {
                    if let Some(ledger) = &mut server.world.material_ledger
                        && let Err(error) = ledger.bury_stack(
                            &server.world.reg,
                            fallback,
                            stack,
                            "invalid-position hosted death",
                        )
                    {
                        eprintln!("materials: guest death settlement failed: {error}");
                    }
                    server.world.retire_arcane_stack_at(
                        fallback,
                        stack,
                        "invalid-position hosted death",
                    );
                }
            }
            refresh_held(guest);
        }
        self.net.send(id, &S2C::Hit { dmg: reduced, from });
        self.send_player_state(id);
    }
}
