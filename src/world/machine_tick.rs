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
        self.tick_clamps(dt);
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
