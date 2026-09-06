//! Pump spoilage for the authoritative host session.

use super::{HostSession, ItemStack, Server};

impl HostSession {
    pub(super) fn pump_spoilage(&mut self, server: &mut Server, dt: f32) {
        // Guest inventory mirrors age like everyone else's pack, so a
        // relog can't refresh yesterday's venison.
        self.perish_timer += dt;
        if self.perish_timer >= 20.0 {
            self.perish_timer -= 20.0;
            let reg = server.world.reg.clone();
            let mush = reg.item_id("base:spoiled_mush");
            let mut consumed = Vec::new();
            let step = (20.0 * crate::world::FRESHNESS_PER_SEC) as u32;
            let sweep_ticks = 20u64 * 20;
            let mut aged_guests = Vec::new();
            for (guest_id, g) in self
                .guests
                .iter_mut()
                .filter(|(_, guest)| guest.entry_ready)
            {
                let mut changed = false;
                let pack_temperature_millic = (server
                    .world
                    .weather_at_surface(g.pos.surface())
                    .temperature_c
                    * 1_000.0)
                    .round()
                    .clamp(i32::MIN as f32, i32::MAX as f32)
                    as i32;
                for (slot, s) in g.inventory.slots.iter_mut().enumerate() {
                    let Some(st) = s else { continue };
                    if st.arcane_id != 0 {
                        let holdfast_step =
                            server
                                .world
                                .holdfast_age_step(g.player_id.0, slot, *st, step, 20);
                        let ordinary_age_ticks = sweep_ticks
                            .saturating_mul(u64::from(holdfast_step))
                            .div_ceil(u64::from(step.max(1)));
                        match server.world.age_preparation_storage(
                            *st,
                            pack_temperature_millic,
                            ordinary_age_ticks,
                        ) {
                            Ok(Some(newly_spoiled)) => {
                                changed |= newly_spoiled;
                                continue;
                            }
                            Ok(None) => {}
                            Err(error) => {
                                eprintln!("alchemy: guest storage aging failed: {error}");
                                continue;
                            }
                        }
                    }
                    let full = reg.item(st.item).durability;
                    let food = reg.item(st.item).food.is_some();
                    let viable_seed = reg.item(st.item).name.ends_with("_seed");
                    if (!food && !viable_seed) || full == 0 {
                        continue;
                    }
                    if st.durability == 0 {
                        st.durability = full;
                        changed = true;
                    } else {
                        let holdfast_step =
                            server
                                .world
                                .holdfast_age_step(g.player_id.0, slot, *st, step, 20);
                        let actual_step = if st.arcane_id == 0 {
                            holdfast_step
                        } else {
                            server.world.coated_specimen_age_advance(
                                st.arcane_id,
                                u64::from(holdfast_step),
                                pack_temperature_millic,
                            ) as u32
                        };
                        if st.durability > actual_step {
                            st.durability -= actual_step;
                            changed = true;
                            continue;
                        }
                        if food {
                            consumed.push(*st);
                            *s = mush.map(|m| {
                                let mut sp = ItemStack::new(&reg, m, 1);
                                sp.count = st.count;
                                sp
                            });
                        } else {
                            st.durability = 0;
                        }
                        changed = true;
                    }
                }
                if let Some(at) = g.pos.block() {
                    for slot in 0..g.inventory.slots.len() {
                        if let Some(stack) = g.inventory.slots[slot]
                            && let Err(error) = server.world.leak_fragile_item_charge(
                                g.player_id.0,
                                slot,
                                stack,
                                at,
                                20,
                            )
                        {
                            eprintln!("arcane specimen leakage failed: {error}");
                        }
                    }
                }
                if changed {
                    aged_guests.push(*guest_id);
                }
            }
            if let Err(error) = server.world.record_consumed_stacks(consumed) {
                eprintln!("materials: spoiled guest food accounting failed: {error}");
            }
            for guest_id in aged_guests {
                self.send_player_state(guest_id);
            }
        }
    }
}
