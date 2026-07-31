//! Runtime ticking for bloomeries, clamps, furnaces, and related machines.

use super::*;

impl World {
    /// Advance machines. Returns true if any visible state changed.
    /// Fire every lit bloomery: validate the shell, let the weather
    /// slow or douse an unroofed stack, and cash the batch when done.
    pub(super) fn tick_bloomeries(&mut self, dt: f32) {
        let keys: Vec<BlockPos> = self
            .block_entities
            .iter()
            .filter(|(_, e)| matches!(e, BlockEntity::Bloomery(b) if b.lit))
            .map(|(k, _)| *k)
            .collect();
        for pos in keys {
            let Some(BlockEntity::Bloomery(mut b)) = self.block_entities.remove(&pos) else {
                continue;
            };
            if self.check_bloomery_at(pos).is_none() {
                // Breached mid-fire: the heat escapes, the charge survives.
                b.lit = false;
                b.progress = 0.0;
                self.swap_block_keep_entity_at(pos, "base:bloomery");
                self.block_entities.insert(pos, BlockEntity::Bloomery(b));
                continue;
            }
            // An unroofed stack fights the rain and loses to a storm.
            let unroofed = b
                .core
                .and_then(|core| core.offset(0, 3, 0))
                .is_some_and(|above| self.light_at_pos(above).1 == 15);
            let wet =
                self.weather.precipitating() && self.rains_at_surface(pos.surface()) && unroofed;
            if wet && self.weather == Weather::Storm {
                b.lit = false;
                b.progress = 0.0;
                self.swap_block_keep_entity_at(pos, "base:bloomery");
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
                self.swap_block_keep_entity_at(pos, "base:bloomery");
            }
            self.block_entities.insert(pos, BlockEntity::Bloomery(b));
        }
    }

    /// Fire every lit forge: chimney and all, so rain never touches
    /// it — the workshop's edge over the open stack. A firing smelts
    /// any furnace recipe in batch at FORGE_ITEMS_PER_FUEL per fuel,
    /// spitting outputs (and cupellation byproducts) at the mouth.
    pub(super) fn tick_forges(&mut self, dt: f32) {
        let keys: Vec<BlockPos> = self
            .block_entities
            .iter()
            .filter(|(_, e)| matches!(e, BlockEntity::Forge(f) if f.lit))
            .map(|(k, _)| *k)
            .collect();
        for pos in keys {
            let Some(BlockEntity::Forge(mut f)) = self.block_entities.remove(&pos) else {
                continue;
            };
            if self.check_forge_at(pos).is_none() {
                f.lit = false;
                f.progress = 0.0;
                self.swap_block_keep_entity_at(pos, "base:forge");
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
                    if let Some(above) = pos.offset(0, 1, 0) {
                        self.push_drop_at(above, out);
                    }
                }
                f.lit = false;
                f.progress = 0.0;
                self.swap_block_keep_entity_at(pos, "base:forge");
            }
            self.block_entities.insert(pos, BlockEntity::Forge(f));
        }
    }

    /// Smoke rises: any rack with raw cuts and a live torch directly
    /// beneath cures the whole load together (wild arc, stage 5 —
    /// the woodland answer to salt country).
    pub(super) fn tick_smokers(&mut self, dt: f32) {
        let reg = self.reg.clone();
        let torch = reg.block_id("base:torch");
        let smoked = reg.item_id("base:smoked_meat");
        let raws = reg.tags.get("base:raw_meats").cloned().unwrap_or_default();
        let keys: Vec<BlockPos> = self
            .block_entities
            .iter()
            .filter(|(_, e)| matches!(e, BlockEntity::Smoker(_)))
            .map(|(k, _)| *k)
            .collect();
        for pos in keys {
            let lit = pos
                .offset(0, -1, 0)
                .is_some_and(|below| Some(self.get_block_at(below)) == torch);
            let Some(BlockEntity::Smoker(sm)) = self.block_entities.get_mut(&pos) else {
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
    pub(super) fn tick_perish(&mut self, dt: f32) {
        const PERISH_SWEEP_SECS: f32 = 20.0;
        self.perish_accum += dt;
        if self.perish_accum < PERISH_SWEEP_SECS {
            return;
        }
        self.perish_accum -= PERISH_SWEEP_SECS;
        let reg = self.reg.clone();
        let mush = reg.item_id("base:spoiled_mush");
        let cellar_at: Vec<(BlockPos, bool)> = self
            .block_entities
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
        let keys: Vec<BlockPos> = self
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
            c.logs.retain(|at| {
                let b = self.get_block_at(*at);
                self.reg
                    .item_id(&self.reg.block(b).name)
                    .is_some_and(|i| logs_tag.contains(&i))
            });
            // A newly exposed log burns to nothing.
            let mut vented: Option<BlockPos> = None;
            let mut exposed = 0;
            'scan: for p in &c.logs {
                for n in crate::planet::neighbors6(*p) {
                    if c.logs.contains(&n) {
                        continue;
                    }
                    if !self.reg.is_solid(self.get_block_at(n)) {
                        exposed += 1;
                        if exposed > 1 {
                            vented = Some(*p);
                            break 'scan;
                        }
                    }
                }
            }
            if let Some(p) = vented {
                self.set_block_at(p, AIR);
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
                        self.set_block_at(p, cc);
                    }
                }
                continue; // done; entity retires
            }
            self.block_entities.insert(pos, BlockEntity::Clamp(c));
        }
    }

    /// Drive powered stations from their shaft lines: the millstone
    /// grinds unattended, the sawmill rips a whole load, the helve
    /// hammer works the smith's anvil at half his pace and none of
    /// his attention. Sources dress themselves: a wheel on live water
    /// and a sail in wind swap to their _run variants, and back.
    pub(super) fn tick_stations(&mut self, dt: f32) {
        let reg = self.reg.clone();
        let keys: Vec<BlockPos> = self
            .block_entities
            .iter()
            .filter(|(_, e)| matches!(e, BlockEntity::Anvil(_)))
            .map(|(k, _)| *k)
            .collect();
        for pos in keys {
            let Some(st) = self.station_at(pos) else {
                continue;
            };
            // Sources carry a marker entity so this sweep can find
            // them without scanning the world; they hold no items.
            // The generator: shaft in, field out. It dresses to its
            // running form and sweeps lamps in reach each second.
            if st == "generator" {
                let running = self.power_at_pos(pos) > 0.0;
                let want = if running {
                    "base:generator_run"
                } else {
                    "base:generator"
                };
                if Some(self.get_block_at(pos)) != reg.block_id(want) {
                    self.swap_block_keep_entity_at(pos, want);
                }
                let w = self.station_work.entry(pos).or_insert(0.0);
                *w += dt;
                if *w < 1.0 {
                    continue;
                }
                *w = 0.0;
                let pairs = [
                    ("base:arc_lamp", "base:arc_lamp_lit"),
                    ("base:blue_arc_lamp", "base:blue_arc_lamp_lit"),
                    ("base:red_arc_lamp", "base:red_arc_lamp_lit"),
                ];
                for dx in -ELEC_RADIUS..=ELEC_RADIUS {
                    for dy in -ELEC_RADIUS..=ELEC_RADIUS {
                        for dz in -ELEC_RADIUS..=ELEC_RADIUS {
                            let Some(lamp_pos) = pos.offset(dx, dy, dz) else {
                                continue;
                            };
                            let b = self.get_block_at(lamp_pos);
                            for (off, on) in pairs {
                                let (off_id, on_id) = (reg.block_id(off), reg.block_id(on));
                                if running && Some(b) == off_id {
                                    if let Some(on) = on_id {
                                        self.set_block_at(lamp_pos, on);
                                    }
                                } else if !running
                                    && Some(b) == on_id
                                    && let Some(off) = off_id
                                {
                                    self.set_block_at(lamp_pos, off);
                                }
                            }
                        }
                    }
                }
                continue;
            }
            if st == "wheel" || st == "sail" {
                let live = if st == "wheel" {
                    // Momentum: a wheel spins down over seconds, not
                    // the instant one cell of its race goes still.
                    let wet = self.wheel_live_at(pos) > 0.0;
                    let bank = self.station_work.entry(pos).or_insert(0.0);
                    if wet {
                        *bank = super::power::WHEEL_SPINDOWN_SECS;
                    } else {
                        *bank = (*bank - dt).max(0.0);
                    }
                    if *bank > 0.0 { 1.0 } else { 0.0 }
                } else {
                    self.sail_live_at(pos)
                };
                let base = format!(
                    "base:{}",
                    if st == "wheel" {
                        "water_wheel"
                    } else {
                        "windmill_sail"
                    }
                );
                let want = if live > 0.0 {
                    format!("{base}_run")
                } else {
                    base
                };
                if Some(self.get_block_at(pos)) != reg.block_id(&want) {
                    self.swap_block_keep_entity_at(pos, &want);
                }
                continue;
            }
            // The pump: the cylinder's first customer. Each stroke
            // lifts the highest water cell in the column below to an
            // open cell beside the pump — finite water, conserved,
            // and a flooded shaft empties one honest stroke at a
            // time (mechanization stage 4: mine drainage).
            if st == "pump" {
                let rate = self.power_at_pos(pos);
                if rate <= 0.0 {
                    self.station_work.remove(&pos);
                    continue;
                }
                let w = self.station_work.entry(pos).or_insert(0.0);
                *w += dt * rate;
                if *w < PUMP_STROKE_SECS {
                    continue;
                }
                *w -= PUMP_STROKE_SECS;
                let lift = (1..=PUMP_REACH)
                    .filter_map(|d| pos.offset(0, -d, 0))
                    .find(|&at| self.reg.water_volume(self.get_block_at(at)).is_some());
                let Some(cell) = lift else { continue };
                let v = self.reg.water_volume(self.get_block_at(cell)).unwrap_or(0);
                let out = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .into_iter()
                    .filter_map(|(dx, dz)| pos.offset(dx, 0, dz))
                    .find(|&at| self.get_block_at(at) == AIR);
                let Some(out) = out else { continue };
                self.set_block_at(cell, AIR);
                let wet = self.reg.water_for_volume(v);
                self.set_block_at(out, wet);
                continue;
            }
            // The helve hammer: a powered arm over the smith's anvil.
            if st == "anvil" {
                let helve = [
                    reg.block_id("base:helve_hammer"),
                    reg.block_id("base:helve_hammer_run"),
                ];
                let arm = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .into_iter()
                    .filter_map(|(dx, dz)| pos.offset(dx, 0, dz))
                    .find(|&at| helve.contains(&Some(self.get_block_at(at))));
                let Some(hp) = arm else { continue };
                let rate = self.power_at_pos(hp);
                let has_work = matches!(
                    self.block_entities.get(&pos),
                    Some(BlockEntity::Anvil(a)) if a.bloom.is_some()
                );
                let want = if rate > 0.0 && has_work {
                    "base:helve_hammer_run"
                } else {
                    "base:helve_hammer"
                };
                if Some(self.get_block_at(hp)) != reg.block_id(want) {
                    self.swap_block_keep_entity_at(hp, want);
                }
                if rate <= 0.0 || !has_work {
                    self.station_work.remove(&pos);
                    continue;
                }
                let w = self.station_work.entry(pos).or_insert(0.0);
                *w += dt * rate;
                if *w >= HELVE_STRIKE_SECS {
                    *w = 0.0;
                    if let Some(out) = self.anvil_strike_at(pos) {
                        if let Some(above) = pos.offset(0, 1, 0) {
                            self.push_drop_at(above, out);
                        }
                    }
                }
                continue;
            }
            if !station_powered(&st) {
                continue;
            }
            let mut rate = self.power_at_pos(pos);
            // The electric quern: a millstone in a generator's field
            // grinds where geography and coal both said no.
            if rate <= 0.0 && st == "millstone" && self.generator_near_at(pos, ELEC_RADIUS) {
                rate = 1.0;
            }
            if rate <= 0.0 {
                self.station_work.remove(&pos);
                continue;
            }
            // Precision machines want workholding: an iron lathe or
            // boring mill with no vice in reach only spins.
            if matches!(st.as_str(), "iron_lathe" | "boring") && !self.vice_near_at(pos) {
                continue;
            }
            let Some(BlockEntity::Anvil(a)) = self.block_entities.get(&pos) else {
                continue;
            };
            let Some(pile) = a.bloom else { continue };
            let table = worked_table_for(&st);
            let Some(def) = reg
                .worked
                .iter()
                .find(|w| w.input == pile.item && w.station == table)
                .cloned()
            else {
                continue;
            };
            let w = self.station_work.entry(pos).or_insert(0.0);
            *w += dt * rate;
            if *w < STATION_STRIKE_SECS {
                continue;
            }
            *w -= STATION_STRIKE_SECS;
            let Some(BlockEntity::Anvil(a)) = self.block_entities.get_mut(&pos) else {
                continue;
            };
            a.strikes += 1;
            if a.strikes < def.strikes {
                continue;
            }
            // The whole load converts in one firing and spits at the
            // mouth (the forge precedent): sixteen ground for the
            // attention of loading once.
            a.bloom = None;
            a.strikes = 0;
            let total = def.count * pile.count;
            let max = reg.item(def.output).max_stack.max(1);
            let mut left = total;
            while left > 0 {
                let n = left.min(max);
                left -= n;
                let mut out = ItemStack::new(&reg, def.output, 1);
                out.count = n;
                if let Some(above) = pos.offset(0, 1, 0) {
                    self.push_drop_at(above, out);
                }
            }
        }
    }

    /// Burn every steaming firebox: fire and water spend together,
    /// the boiler drinks adjacent cells when its bank runs low, and
    /// the firebox and engine dress to their running forms. Power
    /// leaves the river (mechanization stage 5).
    pub(super) fn tick_steam(&mut self, dt: f32) {
        let reg = self.reg.clone();
        let keys: Vec<BlockPos> = self
            .block_entities
            .iter()
            .filter(|(_, e)| matches!(e, BlockEntity::Steam(_)))
            .map(|(k, _)| *k)
            .collect();
        for pos in keys {
            // The boiler sits on the firebox; engines hang off it.
            let boiler_pos = pos.offset(0, 1, 0);
            let boiler_here = boiler_pos
                .is_some_and(|at| reg.block_id("base:boiler") == Some(self.get_block_at(at)));
            // Drink: a low water bank swallows one adjacent cell.
            let mut drink: Option<(BlockPos, u8)> = None;
            if boiler_here
                && let Some(BlockEntity::Steam(s)) = self.block_entities.get(&pos)
                && s.water < STEAM_SECS_PER_WATER
            {
                'search: for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    for dy in [1, 0] {
                        let Some(c) = pos.offset(dx, dy, dz) else {
                            continue;
                        };
                        if let Some(v) = reg.water_volume(self.get_block_at(c)) {
                            drink = Some((c, v));
                            break 'search;
                        }
                    }
                }
            }
            if let Some((c, v)) = drink {
                self.set_block_at(c, AIR);
                if let Some(BlockEntity::Steam(s)) = self.block_entities.get_mut(&pos) {
                    s.water += v as f32 * STEAM_SECS_PER_WATER / 8.0;
                }
            }
            let Some(BlockEntity::Steam(s)) = self.block_entities.get_mut(&pos) else {
                continue;
            };
            let running = boiler_here && s.fuel > 0.0 && s.water > 0.0;
            if running {
                s.fuel = (s.fuel - dt).max(0.0);
                s.water = (s.water - dt).max(0.0);
            }
            let want = if running {
                "base:firebox_lit"
            } else {
                "base:firebox"
            };
            if Some(self.get_block_at(pos)) != reg.block_id(want)
                && reg.block(self.get_block_at(pos)).interaction.as_deref() == Some("firebox")
            {
                self.swap_block_keep_entity_at(pos, want);
            }
            // Dress the engine beside the boiler to match.
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let Some(e) = pos.offset(dx, 1, dz) else {
                    continue;
                };
                let b = self.get_block_at(e);
                let is_engine = [
                    reg.block_id("base:steam_engine"),
                    reg.block_id("base:steam_engine_run"),
                ]
                .contains(&Some(b));
                if !is_engine {
                    continue;
                }
                let want = if running {
                    "base:steam_engine_run"
                } else {
                    "base:steam_engine"
                };
                if Some(b) != reg.block_id(want) {
                    self.swap_block_keep_entity_at(e, want);
                }
            }
        }
    }

    /// Fire every charged separator on a valid firebrick stack: one
    /// powder and one fuel a batch, neodymium and cerium out — the
    /// rare-earth thread, finally honest (mechanization stage 6).
    pub(super) fn tick_separators(&mut self, dt: f32) {
        let reg = self.reg.clone();
        let keys: Vec<BlockPos> = self
            .block_entities
            .iter()
            .filter(|(_, e)| matches!(e, BlockEntity::Separator(_)))
            .map(|(k, _)| *k)
            .collect();
        for pos in keys {
            let valid = self.check_separator_at(pos).is_some();
            let Some(BlockEntity::Separator(sp)) = self.block_entities.get_mut(&pos) else {
                continue;
            };
            let working = valid && sp.powder >= 1 && sp.fuel >= 1;
            if !working {
                sp.progress = 0.0;
            } else {
                sp.progress += dt;
                if sp.progress >= SEPARATE_SECS {
                    sp.progress = 0.0;
                    sp.powder -= 1;
                    sp.fuel -= 1;
                    sp.nd += 1;
                    sp.ce += 2;
                }
            }
            let want = if working {
                "base:separator_lit"
            } else {
                "base:separator"
            };
            if Some(self.get_block_at(pos)) != reg.block_id(want) {
                self.swap_block_keep_entity_at(pos, want);
            }
        }
    }

    /// A running generator within reach: the field that lights lamps
    /// and turns the electric quern. Generators are shaft-driven
    /// machines; their markers make them findable.
    pub fn generator_near_at(&self, pos: BlockPos, r: i32) -> bool {
        let gens = [
            self.reg.block_id("base:generator"),
            self.reg.block_id("base:generator_run"),
        ];
        self.block_entities.iter().any(|(gpos, e)| {
            matches!(e, BlockEntity::Anvil(_))
                && gpos.entity_center().distance_to(pos.entity_center()) <= r as f32
                && gens.contains(&Some(self.get_block_at(*gpos)))
                && self.power_at_pos(*gpos) > 0.0
        })
    }

    #[cfg(test)]
    pub(super) fn generator_near(&self, pos: (i32, i32, i32), r: i32) -> bool {
        BlockPos::of_world(pos.0, pos.1, pos.2).is_some_and(|at| self.generator_near_at(at, r))
    }

    pub fn tick_entities(&mut self, dt: f32) {
        self.tick_steam(dt);
        self.tick_separators(dt);
        self.tick_bloomeries(dt);
        self.tick_kilns(dt);
        self.tick_forges(dt);
        self.tick_clamps(dt);
        self.tick_smokers(dt);
        self.tick_stations(dt);
        self.tick_perish(dt);
        let reg = self.reg.clone();
        // Byproducts pour out the furnace mouth (cupellation lead);
        // collected here because the entity map is borrowed.
        let mut spat: Vec<(BlockPos, ItemStack)> = Vec::new();
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
        for (pos, stack) in spat {
            self.push_drop_at(pos, stack);
        }
    }

    // ---- block entity persistence (by item name, mod-change safe) ----
}
