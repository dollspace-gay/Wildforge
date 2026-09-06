//! Food storage machine_tick transaction coordination.

use crate::inventory::ItemStack;
use crate::planet::BlockPos;
use crate::world::BlockEntity;
use crate::world::FRESHNESS_PER_SEC;
use crate::world::SMOKE_SECS;
use crate::world::World;

impl World {
    /// Smoke rises: any rack with raw cuts and a live torch directly
    /// beneath cures the whole load together (wild arc, stage 5 —
    /// the woodland answer to salt country).
    pub(in crate::world) fn tick_smokers(&mut self, dt: f32) {
        let reg = self.reg.clone();
        let torch = reg.block_id("base:torch");
        let smoked = reg.item_id("base:smoked_meat");
        let raws = reg.tags.get("base:raw_meats").cloned().unwrap_or_default();
        let keys: Vec<BlockPos> = self
            .installations
            .iter()
            .filter(|(_, e)| matches!(e, BlockEntity::Smoker(_)))
            .map(|(k, _)| *k)
            .collect();
        for pos in keys {
            let lit = pos
                .offset(0, -1, 0)
                .is_some_and(|below| Some(self.get_block_at(below)) == torch);
            let Some(BlockEntity::Smoker(sm)) = self.installations.get_mut(&pos) else {
                continue;
            };
            let curing = sm.meat.iter().flatten().any(|s| raws.contains(&s.item));
            if !lit || !curing {
                sm.progress = 0.0;
                continue;
            }
            sm.progress += dt;
            if sm.progress >= SMOKE_SECS {
                sm.progress = 0.0;
                if let Some(smoked) = smoked {
                    for s in sm.meat.iter_mut() {
                        if let Some(st) = s
                            && raws.contains(&st.item)
                        {
                            *s = Some(ItemStack {
                                item: smoked,
                                count: st.count,
                                durability: reg.item(smoked).durability,
                                arcane_id: st.arcane_id,
                            });
                        }
                    }
                }
            }
        }
    }

    /// Food kept in containers ages (economy plan, leg 3): every
    /// PERISH_SWEEP_SECS, each food stack loses that much freshness —
    /// quartered in a cellar (dark and skylight-free, the cool rooms
    /// people actually dig). At zero the stack turns to spoiled mush.
    /// A legacy stack from before freshness (durability 0 on a
    /// perishable) initializes to fresh instead of rotting.
    pub(in crate::world) fn tick_perish(&mut self, dt: f32) {
        const PERISH_SWEEP_SECS: f32 = 20.0;
        if !self.installations.perish_cycle(dt, PERISH_SWEEP_SECS) {
            return;
        }
        let reg = self.reg.clone();
        let mush = reg.item_id("base:spoiled_mush");
        let mut consumed = Vec::new();
        let alchemy_container_ids = self
            .alchemy_state
            .as_ref()
            .map(|state| {
                state
                    .containers
                    .keys()
                    .copied()
                    .collect::<std::collections::BTreeSet<_>>()
            })
            .unwrap_or_default();
        let mut preparation_assessments = Vec::<(ItemStack, i32, u64)>::new();
        let cellar_at: Vec<(BlockPos, bool)> = self
            .installations
            .iter()
            .filter(|(_, e)| matches!(e, BlockEntity::Chest(_) | BlockEntity::Offering(_)))
            .map(|(&p, _)| p)
            .map(|p| {
                // Sample above the container: the block itself is
                // opaque and always reads dark.
                let (bl, sky) = p
                    .offset(0, 1, 0)
                    .map_or((0, 15), |above| self.light_at_pos(above));
                (p, sky == 0 && bl <= 3)
            })
            .collect();
        for (pos, cellar) in cellar_at {
            let rate = PERISH_SWEEP_SECS * FRESHNESS_PER_SEC;
            let step = if cellar { rate / 4.0 } else { rate } as u32;
            let storage_ticks = ((PERISH_SWEEP_SECS * 20.0) as u64)
                .checked_div(if cellar { 4 } else { 1 })
                .unwrap_or_default();
            let storage_temperature_millic =
                (self.weather_at_surface(pos.surface()).temperature_c * 1_000.0)
                    .round()
                    .clamp(i32::MIN as f32, i32::MAX as f32) as i32;
            let Some(e) = self.installations.get_mut(&pos) else {
                continue;
            };
            let slots: &mut [Option<ItemStack>] = match e {
                BlockEntity::Chest(c) => &mut c.slots,
                BlockEntity::Offering(o) => &mut o.slots,
                _ => continue,
            };
            for s in slots.iter_mut() {
                let Some(st) = s else { continue };
                if st.arcane_id != 0 && alchemy_container_ids.contains(&st.arcane_id) {
                    preparation_assessments.push((*st, storage_temperature_millic, storage_ticks));
                    continue;
                }
                let full = reg.item(st.item).durability;
                if reg.item(st.item).food.is_none() || full == 0 {
                    continue;
                }
                if st.durability == 0 {
                    st.durability = full; // legacy: starts fresh today
                } else if st.durability <= step {
                    consumed.push(*st);
                    *s = mush.map(|m| {
                        let mut sp = ItemStack::new(&reg, m, 1);
                        sp.count = st.count;
                        sp
                    });
                } else {
                    st.durability -= step;
                }
            }
        }
        for (stack, temperature_millic, ordinary_age_ticks) in preparation_assessments {
            if let Err(error) =
                self.age_preparation_storage(stack, temperature_millic, ordinary_age_ticks)
            {
                eprintln!("alchemy: stored preparation aging failed: {error}");
            }
        }

        // Samples mounted in the discovery apparatus are neither inventory
        // nor a cellar. They still live on the same ordinary aging clock; an
        // active Holdfast may only reduce this real decrement. Collect first
        // so the workings ledger can be updated without aliasing block state.
        let mounted: Vec<(BlockPos, u8, ItemStack)> = self
            .installations
            .iter()
            .filter_map(|(&pos, entity)| match entity {
                BlockEntity::DiscoveryApparatus(apparatus) => Some(
                    [apparatus.sample, apparatus.reference]
                        .into_iter()
                        .enumerate()
                        .filter_map(move |(bay, stack)| stack.map(|stack| (pos, bay as u8, stack)))
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
            .flatten()
            .collect();
        let ordinary_step = (PERISH_SWEEP_SECS * FRESHNESS_PER_SEC) as u32;
        for (pos, bay, expected) in mounted {
            let definition = reg.item(expected.item);
            let is_food = definition.food.is_some();
            let is_seed = definition.name.ends_with("_seed");
            if (!is_food && !is_seed) || definition.durability == 0 {
                continue;
            }
            let step = self.holdfast_mounted_age_step(
                pos,
                bay,
                expected,
                ordinary_step,
                PERISH_SWEEP_SECS as u32,
            );
            let Some(BlockEntity::DiscoveryApparatus(apparatus)) = self.installations.get_mut(&pos)
            else {
                continue;
            };
            let slot = match bay {
                0 => &mut apparatus.sample,
                1 => &mut apparatus.reference,
                _ => continue,
            };
            let Some(stack) = slot.as_mut() else {
                continue;
            };
            if *stack != expected {
                continue;
            }
            if stack.durability == 0 {
                stack.durability = definition.durability;
            } else if stack.durability > step {
                stack.durability -= step;
            } else if is_food {
                consumed.push(*stack);
                *slot = mush.map(|item| {
                    let mut spoiled = ItemStack::new(&reg, item, 1);
                    spoiled.count = stack.count;
                    spoiled
                });
            } else {
                stack.durability = 0;
            }
        }
        if let Err(error) = self.record_consumed_stacks(consumed) {
            eprintln!("materials: spoiled container food accounting failed: {error}");
        }
    }
}
