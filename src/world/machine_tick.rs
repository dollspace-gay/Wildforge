//! Runtime ticking for bloomeries, clamps, furnaces, and related machines.

use super::*;

impl World {
    /// Advance machines. Returns true if any visible state changed.
    /// Fire every lit bloomery: validate the shell, let the weather
    /// slow or douse an unroofed stack, and cash the batch when done.
    pub(super) fn tick_bloomeries(&mut self, dt: f32) {
        let keys: Vec<(i32, i32, i32)> = self
            .block_entities
            .iter()
            .filter(|(_, e)| matches!(e, BlockEntity::Bloomery(b) if b.lit))
            .map(|(k, _)| *k)
            .collect();
        for pos in keys {
            let Some(BlockEntity::Bloomery(mut b)) = self.block_entities.remove(&pos) else {
                continue;
            };
            let (x, y, z) = pos;
            if self.check_bloomery(x, y, z).is_none() {
                // Breached mid-fire: the heat escapes, the charge survives.
                b.lit = false;
                b.progress = 0.0;
                self.swap_block_keep_entity(x, y, z, "base:bloomery");
                self.block_entities.insert(pos, BlockEntity::Bloomery(b));
                continue;
            }
            // An unroofed stack fights the rain and loses to a storm.
            let unroofed = self.light_at(b.core.0, y + 3, b.core.2).1 == 15;
            let wet = self.weather.precipitating() && self.rains_at(x, z) && unroofed;
            if wet && self.weather == Weather::Storm {
                b.lit = false;
                b.progress = 0.0;
                self.swap_block_keep_entity(x, y, z, "base:bloomery");
                self.block_entities.insert(pos, BlockEntity::Bloomery(b));
                continue;
            }
            b.progress += dt * if wet { 0.5 } else { 1.0 };
            if b.progress >= BLOOMERY_FIRE_SECS {
                // Cash the batch: 2 charge + 2 fuel per bloom, +2 bonus
                // blooms on a full 8+8 firing.
                let chain = self.reg.bloomery.first().cloned();
                if let Some(chain) = chain {
                    let n_charge: u32 = b.charge.iter().flatten().map(|s| s.count).sum();
                    let n_fuel: u32 = b.fuel.iter().flatten().map(|s| s.count).sum();
                    let units = n_charge.min(n_fuel) / 2;
                    let blooms = units + if units >= 4 { 2 } else { 0 };
                    let eat = |slots: &mut [Option<ItemStack>; 4], mut n: u32| {
                        for s in slots.iter_mut() {
                            if n == 0 {
                                break;
                            }
                            if let Some(st) = s {
                                let take = st.count.min(n);
                                n -= take;
                                st.count -= take;
                                if st.count == 0 {
                                    *s = None;
                                }
                            }
                        }
                    };
                    eat(&mut b.charge, units * 2);
                    eat(&mut b.fuel, units * 2);
                    let reg = self.reg.clone();
                    let mut out = ItemStack::new(&reg, chain.bloom, blooms.max(1));
                    out.count = blooms.max(1);
                    // Blooms land in the first empty charge slot.
                    for s in b.charge.iter_mut() {
                        if s.is_none() {
                            *s = Some(out);
                            break;
                        }
                    }
                }
                b.lit = false;
                b.progress = 0.0;
                self.swap_block_keep_entity(x, y, z, "base:bloomery");
            }
            self.block_entities.insert(pos, BlockEntity::Bloomery(b));
        }
    }

    /// Fire every lit forge: chimney and all, so rain never touches
    /// it — the workshop's edge over the open stack. A firing smelts
    /// any furnace recipe in batch at FORGE_ITEMS_PER_FUEL per fuel,
    /// spitting outputs (and cupellation byproducts) at the mouth.
    pub(super) fn tick_forges(&mut self, dt: f32) {
        let keys: Vec<(i32, i32, i32)> = self
            .block_entities
            .iter()
            .filter(|(_, e)| matches!(e, BlockEntity::Forge(f) if f.lit))
            .map(|(k, _)| *k)
            .collect();
        for pos in keys {
            let Some(BlockEntity::Forge(mut f)) = self.block_entities.remove(&pos) else {
                continue;
            };
            let (x, y, z) = pos;
            if self.check_forge(x, y, z).is_none() {
                f.lit = false;
                f.progress = 0.0;
                self.swap_block_keep_entity(x, y, z, "base:forge");
                self.block_entities.insert(pos, BlockEntity::Forge(f));
                continue;
            }
            f.progress += dt;
            if f.progress >= FORGE_FIRE_SECS {
                let reg = self.reg.clone();
                let n_fuel: u32 = f.fuel.iter().flatten().map(|s| s.count).sum();
                let mut budget = n_fuel * FORGE_ITEMS_PER_FUEL;
                let mut burned = 0u32;
                let mut outputs: Vec<ItemStack> = Vec::new();
                for s in f.charge.iter_mut() {
                    let Some(st) = s else { continue };
                    let Some(smelt) = reg
                        .smelts
                        .iter()
                        .find(|sm| sm.input.matches(st.item))
                        .cloned()
                    else {
                        continue; // not smeltable: survives the firing
                    };
                    let n = st.count.min(budget);
                    if n == 0 {
                        continue;
                    }
                    budget -= n;
                    burned += n;
                    st.count -= n;
                    if st.count == 0 {
                        *s = None;
                    }
                    let mut out = ItemStack::new(&reg, smelt.output, 1);
                    out.count = n;
                    outputs.push(out);
                    if let Some((spit, sn)) = smelt.spit {
                        let mut sp = ItemStack::new(&reg, spit, 1);
                        sp.count = sn * n;
                        outputs.push(sp);
                    }
                }
                // Fuel burns only for work done (round up).
                let eat = |slots: &mut [Option<ItemStack>; 4], mut n: u32| {
                    for s in slots.iter_mut() {
                        if n == 0 {
                            break;
                        }
                        if let Some(st) = s {
                            let take = st.count.min(n);
                            n -= take;
                            st.count -= take;
                            if st.count == 0 {
                                *s = None;
                            }
                        }
                    }
                };
                eat(&mut f.fuel, burned.div_ceil(FORGE_ITEMS_PER_FUEL));
                for out in outputs {
                    self.pending_drops.push(((x, y + 1, z), out));
                }
                f.lit = false;
                f.progress = 0.0;
                self.swap_block_keep_entity(x, y, z, "base:forge");
            }
            self.block_entities.insert(pos, BlockEntity::Forge(f));
        }
    }

    /// Food kept in containers ages (economy plan, leg 3): every
    /// PERISH_SWEEP_SECS, each food stack loses that much freshness —
    /// quartered in a cellar (dark and skylight-free, the cool rooms
    /// people actually dig). At zero the stack turns to spoiled mush.
    /// A legacy stack from before freshness (durability 0 on a
    /// perishable) initializes to fresh instead of rotting.
    pub(super) fn tick_perish(&mut self, dt: f32) {
        const PERISH_SWEEP_SECS: f32 = 20.0;
        self.perish_accum += dt;
        if self.perish_accum < PERISH_SWEEP_SECS {
            return;
        }
        self.perish_accum -= PERISH_SWEEP_SECS;
        let reg = self.reg.clone();
        let mush = reg.item_id("base:spoiled_mush");
        let cellar_at: Vec<((i32, i32, i32), bool)> = self
            .block_entities
            .iter()
            .filter(|(_, e)| matches!(e, BlockEntity::Chest(_) | BlockEntity::Offering(_)))
            .map(|(&p, _)| p)
            .map(|p| {
                // Sample above the container: the block itself is
                // opaque and always reads dark.
                let (bl, sky) = self.light_at(p.0, p.1 + 1, p.2);
                (p, sky == 0 && bl <= 3)
            })
            .collect();
        for (pos, cellar) in cellar_at {
            let step = if cellar {
                PERISH_SWEEP_SECS / 4.0
            } else {
                PERISH_SWEEP_SECS
            } as u32;
            let Some(e) = self.block_entities.get_mut(&pos) else {
                continue;
            };
            let slots: &mut [Option<ItemStack>] = match e {
                BlockEntity::Chest(c) => &mut c.slots,
                BlockEntity::Offering(o) => &mut o.slots,
                _ => continue,
            };
            for s in slots.iter_mut() {
                let Some(st) = s else { continue };
                let full = reg.item(st.item).durability;
                if reg.item(st.item).food.is_none() || full == 0 {
                    continue;
                }
                if st.durability == 0 {
                    st.durability = full; // legacy: starts fresh today
                } else if st.durability <= step {
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
    }

    /// Smolder every clamp; venting burns the exposed log away.
    pub(super) fn tick_clamps(&mut self, dt: f32) {
        let keys: Vec<(i32, i32, i32)> = self
            .block_entities
            .iter()
            .filter(|(_, e)| matches!(e, BlockEntity::Clamp(_)))
            .map(|(k, _)| *k)
            .collect();
        let logs_tag = self.reg.tags.get("base:logs").cloned().unwrap_or_default();
        for pos in keys {
            let Some(BlockEntity::Clamp(mut c)) = self.block_entities.remove(&pos) else {
                continue;
            };
            // Logs that stopped being logs (mined) leave the pile.
            c.logs.retain(|&(x, y, z)| {
                let b = self.get_block(x, y, z);
                self.reg
                    .item_id(&self.reg.block(b).name)
                    .is_some_and(|i| logs_tag.contains(&i))
            });
            // A newly exposed log burns to nothing.
            let mut vented: Option<(i32, i32, i32)> = None;
            let mut exposed = 0;
            'scan: for p in &c.logs {
                for d in [
                    (1, 0, 0),
                    (-1, 0, 0),
                    (0, 1, 0),
                    (0, -1, 0),
                    (0, 0, 1),
                    (0, 0, -1),
                ] {
                    let n = (p.0 + d.0, p.1 + d.1, p.2 + d.2);
                    if c.logs.contains(&n) {
                        continue;
                    }
                    if !self.reg.is_solid(self.get_block(n.0, n.1, n.2)) {
                        exposed += 1;
                        if exposed > 1 {
                            vented = Some(*p);
                            break 'scan;
                        }
                    }
                }
            }
            if let Some(p) = vented {
                self.set_block(p.0, p.1, p.2, AIR);
                c.logs.retain(|l| *l != p);
                c.timer -= CLAMP_SECS_PER_LOG;
            }
            if c.logs.is_empty() {
                continue; // the pile is gone; so is the burn
            }
            c.timer -= dt;
            if c.timer <= 0.0 {
                if let Some(cc) = self.reg.block_id("base:charcoal_block") {
                    for p in c.logs.clone() {
                        self.set_block(p.0, p.1, p.2, cc);
                    }
                }
                continue; // done; entity retires
            }
            self.block_entities.insert(pos, BlockEntity::Clamp(c));
        }
    }

    pub fn tick_entities(&mut self, dt: f32) {
        self.tick_bloomeries(dt);
        self.tick_kilns(dt);
        self.tick_forges(dt);
        self.tick_clamps(dt);
        self.tick_perish(dt);
        let reg = self.reg.clone();
        // Byproducts pour out the furnace mouth (cupellation lead);
        // collected here because the entity map is borrowed.
        let mut spat: Vec<((i32, i32, i32), ItemStack)> = Vec::new();
        for (&fpos, e) in self.block_entities.iter_mut() {
            let BlockEntity::Furnace(f) = e else { continue };
            let smelt = f.input.and_then(|s| reg.smelt_for(s.item)).cloned();
            let output_ok = |f: &FurnaceState, out: crate::registry::ItemId| match f.output {
                None => true,
                Some(o) => o.item == out && o.count < reg.item(out).max_stack,
            };
            let can_smelt = smelt.as_ref().is_some_and(|s| output_ok(f, s.output));

            if f.burn_left <= 0.0 && can_smelt {
                // Light more fuel (the forge feeds the wild's ire).
                if let Some(fs) = f.fuel
                    && let Some((burn, speed)) = reg.fuel_value(fs.item)
                {
                    f.burn_left = burn;
                    f.burn_total = burn;
                    f.burn_speed = speed;
                    let left = fs.count - 1;
                    f.fuel = if left > 0 {
                        Some(ItemStack { count: left, ..fs })
                    } else {
                        None
                    };
                    self.ire = (self.ire + 0.1).min(100.0);
                }
            }
            if f.burn_left > 0.0 {
                f.burn_left = (f.burn_left - dt).max(0.0);
                if can_smelt {
                    let s = smelt.as_ref().unwrap();
                    f.progress += dt * f.burn_speed.max(1.0);
                    if f.progress >= s.time {
                        f.progress = 0.0;
                        // Consume one input, emit output.
                        if let Some(inp) = f.input {
                            let left = inp.count - 1;
                            f.input = if left > 0 {
                                Some(ItemStack { count: left, ..inp })
                            } else {
                                None
                            };
                        }
                        f.output = Some(match f.output {
                            Some(o) => ItemStack {
                                count: o.count + 1,
                                ..o
                            },
                            None => ItemStack::new(&reg, s.output, 1),
                        });
                        if let Some((item, count)) = s.spit {
                            spat.push((fpos, ItemStack::new(&reg, item, count)));
                        }
                    }
                } else {
                    f.progress = 0.0;
                }
            } else if f.progress > 0.0 {
                f.progress = (f.progress - dt * 2.0).max(0.0);
            }
        }
        self.pending_drops.extend(spat);
    }

    // ---- block entity persistence (by item name, mod-change safe) ----
}
