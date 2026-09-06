//! Pump delivery for the authoritative host session.

use super::{HostSession, ItemStack, S2C, Server, Vec3};

impl HostSession {
    pub(super) fn pump_delivery(&mut self, server: &mut Server) {
        // Authoritative block edits out.
        if !server.world.edits().is_empty() {
            for (pos, b, meta, salt_mass, soil_salinity) in server.world.take_edits() {
                if let Some(jobs) = self.chunk_jobs.as_mut() {
                    jobs.invalidate_encoded(pos.chunk());
                }
                self.broadcast_ready(&S2C::BlockSet {
                    pos,
                    id: b.0,
                    meta,
                    salt_mass,
                    soil_salinity,
                });
            }
        }
        // Items owed to guests (arrow recovery, mining, mob drops,
        // brush finds) — full stacks so durability crosses the wire.
        for (owner, s) in server.world.take_pending_gives() {
            let mut delivered = 0;
            let mut overflow_at = None;
            if let Some(guest) = self.guests.get_mut(&owner) {
                let left = guest.inventory.add_stack(&server.world.reg, s);
                delivered = s.count - left;
                overflow_at = guest.pos.block().map(|pos| (pos, left));
            }
            if delivered != 0 {
                self.net.send(
                    owner,
                    &S2C::Give {
                        item: s.item.0,
                        count: delivered,
                        durability: s.durability,
                        arcane_id: s.arcane_id,
                        current_units: server
                            .world
                            .inspectable_item_current(s.arcane_id)
                            .unwrap_or(0),
                    },
                );
            }
            if let Some((pos, left)) = overflow_at.filter(|(_, left)| *left != 0)
                && let Some(ledger) = &mut server.world.material_ledger
                && let Err(error) = ledger.bury_stack(
                    &server.world.reg,
                    pos,
                    ItemStack { count: left, ..s },
                    "full guest inventory",
                )
            {
                eprintln!("materials: guest delivery overflow accounting failed: {error}");
            }
            if let Some((pos, left)) = overflow_at.filter(|(_, left)| *left != 0) {
                server.world.retire_arcane_stack_at(
                    pos,
                    ItemStack { count: left, ..s },
                    "full guest inventory",
                );
            }
            self.send_player_state(owner);
        }
        // Dropped items are the same host-owned physical entities on a
        // windowed or dedicated host. Nearby guests pick them up through the
        // authoritative inventory; full packs leave the remainder in-world.
        let mut loose = server.world.take_loose_items();
        let mut changed = std::collections::BTreeSet::new();
        let mut index = 0;
        while index < loose.len() {
            let item = &loose[index];
            let nearest = (item.age > crate::entity::PICKUP_DELAY)
                .then(|| {
                    self.guests
                        .iter()
                        .filter(|(_, guest)| guest.entry_ready && guest.health > 0.0)
                        .filter_map(|(id, guest)| {
                            let target = guest.pos.translated(Vec3::new(0.0, 0.9, 0.0)).ok()?.pos;
                            let distance = item.pos.distance_to(target);
                            (distance < 1.4).then_some((*id, distance))
                        })
                        .min_by(|(left_id, left), (right_id, right)| {
                            left.total_cmp(right).then_with(|| left_id.cmp(right_id))
                        })
                        .map(|(id, _)| id)
                })
                .flatten();
            let Some(id) = nearest else {
                index += 1;
                continue;
            };
            let mut stack = ItemStack::new(&server.world.reg, item.item, item.count);
            stack.durability = item.durability;
            stack.arcane_id = item.arcane_id;
            let guest = self.guests.get_mut(&id).expect("selected guest exists");
            let left = guest.inventory.add_stack(&server.world.reg, stack);
            let delivered = stack.count.saturating_sub(left);
            if delivered != 0 {
                self.net.send(
                    id,
                    &S2C::Give {
                        item: stack.item.0,
                        count: delivered,
                        durability: stack.durability,
                        arcane_id: stack.arcane_id,
                        current_units: server
                            .world
                            .inspectable_item_current(stack.arcane_id)
                            .unwrap_or(0),
                    },
                );
                changed.insert(id);
            }
            if left == 0 {
                loose.swap_remove(index);
            } else {
                loose[index].count = left;
                index += 1;
            }
        }
        server.world.replace_loose_items(loose);
        for id in changed {
            self.send_player_state(id);
        }
    }
}
