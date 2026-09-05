//! Runtime ticking for bloomeries, clamps, furnaces, and related machines.

use super::multiblock::BlockStore;
use super::*;
use crate::machines::MachineHandler;

impl World {
    /// Advance machines. Returns true if any visible state changed.
    /// Fire every lit bloomery: the weather can slow or douse an
    /// unroofed stack, and the batch is cashed when the fire's done.
    /// The shell itself is revalidated by the edit hook, not here.
    pub(super) fn tick_bloomeries(&mut self, dt: f32) {
        tick_bloomery_machines(self, dt);
    }

    /// Fire every lit forge: chimney and all, so rain never touches
    /// it — the workshop's edge over the open stack. A firing smelts
    /// any furnace recipe in batch at FORGE_ITEMS_PER_FUEL per fuel,
    /// spitting outputs (and cupellation byproducts) at the mouth.
    /// The shell is revalidated by the edit hook, not here.
    pub(super) fn tick_forges(&mut self, dt: f32) {
        tick_forge_machines(self, dt);
    }

    /// Smoke rises: any rack with raw cuts and a live torch directly
    /// beneath cures the whole load together (wild arc, stage 5 —
    /// the woodland answer to salt country).
    pub(super) fn tick_smokers(&mut self, dt: f32) {
        let reg = self.reg.clone();
        let torch = reg.block_id("base:torch");
        let smoked = reg.item_id("base:smoked_meat");
        let raws = reg.tags.get("base:raw_meats").cloned().unwrap_or_default();
        let keys: Vec<BlockPos> = self.installations
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
    pub(super) fn tick_perish(&mut self, dt: f32) {
        const PERISH_SWEEP_SECS: f32 = 20.0;
        if !self.installations.perish_cycle(dt, PERISH_SWEEP_SECS) { return; }
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
        let cellar_at: Vec<(BlockPos, bool)> = self.installations
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
        let mounted: Vec<(BlockPos, u8, ItemStack)> = self.installations
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
            let Some(BlockEntity::DiscoveryApparatus(apparatus)) =
                self.installations.get_mut(&pos)
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

    /// Smolder every clamp; venting burns the exposed log away.
    pub(super) fn tick_clamps(&mut self, dt: f32) {
        let keys: Vec<BlockPos> = self.installations
            .iter()
            .filter(|(_, e)| matches!(e, BlockEntity::Clamp(_)))
            .map(|(k, _)| *k)
            .collect();
        let logs_tag = self.reg.tags.get("base:logs").cloned().unwrap_or_default();
        for pos in keys {
            let Some(BlockEntity::Clamp(mut c)) = self.installations.remove(&pos) else {
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
            self.installations.insert(pos, BlockEntity::Clamp(c));
        }
    }

    /// Drive powered stations from their shaft lines: the millstone
    /// grinds unattended, the sawmill rips a whole load, the helve
    /// hammer works the smith's anvil at half his pace and none of
    /// his attention. Sources dress themselves: a wheel on live water
    /// and a sail in wind swap to their _run variants, and back.
    pub(super) fn tick_stations(&mut self, dt: f32) {
        let reg = self.reg.clone();
        let keys: Vec<BlockPos> = self.installations
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
                let work = self.installations.accumulate_work(pos, dt);
                if work < 1.0 {
                    continue;
                }
                self.installations.reset_work(pos);
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
                    self.installations.wheel_momentum(pos, wet, dt, super::power::WHEEL_SPINDOWN_SECS)
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
                    self.installations.forget_work(pos);
                    continue;
                }
                let work = self.installations.accumulate_work(pos, dt * rate);
                if work < PUMP_STROKE_SECS {
                    continue;
                }
                self.installations.consume_work(pos, PUMP_STROKE_SECS);
                let lift = (1..=PUMP_REACH)
                    .filter_map(|d| pos.offset(0, -d, 0))
                    .find(|&at| self.reg.water_volume(self.get_block_at(at)).is_some());
                let out = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .into_iter()
                    .filter_map(|(dx, dz)| pos.offset(dx, 0, dz))
                    .find(|&at| self.get_block_at(at) == AIR);
                let Some(out) = out else { continue };
                if let Some(cell) = lift {
                    let v = self.reg.water_volume(self.get_block_at(cell)).unwrap_or(0);
                    self.move_water_units(cell, out, v);
                } else if let (Some(atlas), Some(weather)) =
                    (&self.planet_atlas, self.weather_state.live_mut())
                {
                    let atlas_pos = atlas.atlas_pos(pos.surface());
                    let parcel = weather
                        .pump_groundwater(atlas_pos, crate::planet_atlas::HYDRO_UNITS_PER_BLOCK);
                    if parcel.water_hu != 0 {
                        self.write_water_mass_at(out, parcel);
                    }
                }
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
                    self.installations.get(&pos),
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
                    self.installations.forget_work(pos);
                    continue;
                }
                let work = self.installations.accumulate_work(pos, dt * rate);
                if work >= HELVE_STRIKE_SECS {
                    self.installations.reset_work(pos);
                    if let Some(out) = self.anvil_strike_at(pos)
                        && let Some(above) = pos.offset(0, 1, 0)
                    {
                        self.push_drop_at(above, out);
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
                self.installations.forget_work(pos);
                continue;
            }
            // Precision machines want workholding: an iron lathe or
            // boring mill with no vice in reach only spins.
            if matches!(st.as_str(), "iron_lathe" | "boring") && !self.vice_near_at(pos) {
                continue;
            }
            let Some(BlockEntity::Anvil(a)) = self.installations.get(&pos) else {
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
            let work = self.installations.accumulate_work(pos, dt * rate);
            if work < STATION_STRIKE_SECS {
                continue;
            }
            self.installations.consume_work(pos, STATION_STRIKE_SECS);
            let Some(BlockEntity::Anvil(a)) = self.installations.get_mut(&pos) else {
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
            if let Some(ledger) = &mut self.material_ledger
                && let Err(error) = ledger.record_recipe_loss_scaled(&def.loss, pile.count)
            {
                eprintln!("materials: powered station accounting failed: {error}");
            }
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
        let keys: Vec<BlockPos> = self.installations
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
                && let Some(BlockEntity::Steam(s)) = self.installations.get(&pos)
                && s.water.water_hu < crate::planet_atlas::HYDRO_UNITS_PER_BLOCK
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
            if let Some((c, _v)) = drink {
                let mass = self.water_mass_at(c).unwrap_or_default();
                let preferred = self.surface_reservoir_at(c);
                let accepted = if let Some(weather) = self.weather_state.live_mut() {
                    weather.move_detailed_to_industrial_from(preferred, mass)
                } else {
                    true
                };
                if accepted {
                    self.set_block_at(c, AIR);
                    if let Some(BlockEntity::Steam(s)) = self.installations.get_mut(&pos) {
                        s.water
                            .add_assign(mass)
                            .expect("boiler water reservoir fits");
                    }
                }
            }
            let Some(BlockEntity::Steam(s)) = self.installations.get(&pos) else {
                continue;
            };
            let running = !s.draft_closed && boiler_here && s.fuel > 0.0 && s.water.water_hu > 0;
            if running {
                let micros = (f64::from(dt) * 1_000_000.0).round().max(0.0) as u64;
                let numerator = self.installations
                    .get(&pos)
                    .and_then(|entity| match entity {
                        BlockEntity::Steam(state) => Some(state.steam_numerator_remainder),
                        _ => None,
                    })
                    .unwrap_or(0)
                    .saturating_add(
                        micros.saturating_mul(crate::planet_atlas::HYDRO_UNITS_PER_BLOCK),
                    );
                let requested = numerator
                    .saturating_div((STEAM_SECS_PER_WATER * 1_000_000.0) as u64)
                    .min(s.water.water_hu);
                let exhausted = if let (Some(atlas), Some(weather)) =
                    (&self.planet_atlas, self.weather_state.live_mut())
                {
                    let alchemy_reserved = self
                        .alchemy_state
                        .as_ref()
                        .map_or(0, |state| state.total_water_custody().water_hu);
                    weather.exhaust_industrial_vapor_excluding(
                        atlas.atlas_pos(pos.surface()),
                        requested,
                        alchemy_reserved,
                    )
                } else {
                    requested
                };
                let Some(BlockEntity::Steam(s)) = self.installations.get_mut(&pos) else {
                    continue;
                };
                s.fuel = (s.fuel - dt).max(0.0);
                let _ = s.water.take_fresh_water(exhausted);
                s.steam_numerator_remainder = numerator.saturating_sub(
                    exhausted.saturating_mul((STEAM_SECS_PER_WATER * 1_000_000.0) as u64),
                );
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
    /// rare-earth thread, finally honest (mechanization stage 6). The
    /// shell is revalidated by the edit hook, not here.
    pub(super) fn tick_separators(&mut self, dt: f32) {
        tick_separator_machines(self, dt);
    }

    /// A running generator within reach: the field that lights lamps
    /// and turns the electric quern. Generators are shaft-driven
    /// machines; their markers make them findable.
    pub fn generator_near_at(&self, pos: BlockPos, r: i32) -> bool {
        let gens = [
            self.reg.block_id("base:generator"),
            self.reg.block_id("base:generator_run"),
        ];
        self.installations.iter().any(|(gpos, e)| {
            matches!(e, BlockEntity::Anvil(_))
                && gpos.entity_center().distance_to(pos.entity_center()) <= r as f32
                && gens.contains(&Some(self.get_block_at(*gpos)))
                && self.power_at_pos(*gpos) > 0.0
        })
    }

    pub fn tick_entities(&mut self, dt: f32) {
        // Capability E12: running machines feed regional ire.
        self.tick_industrial_ire(dt);
        self.tick_loose_items(dt);
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
        let mut material_fuels = Vec::<(BlockPos, ItemStack)>::new();
        let mut arcane_inputs = Vec::<(BlockPos, ItemStack)>::new();
        let mut material_losses = Vec::<crate::registry::MaterialVector>::new();
        let mut secondary_recoveries = Vec::<crate::registry::MaterialVector>::new();
        for (&fpos, e) in self.installations.iter_mut() {
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
                    material_fuels.push((fpos, ItemStack { count: 1, ..fs }));
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
                            if inp.arcane_id != 0 {
                                arcane_inputs.push((fpos, ItemStack { count: 1, ..inp }));
                            }
                            if crate::materials::is_secondary_item(&reg, inp.item) {
                                secondary_recoveries.push(crate::materials::stack_materials(
                                    &reg,
                                    ItemStack::new(&reg, inp.item, 1),
                                ));
                            }
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
                        material_losses.push(s.loss.clone());
                    }
                } else {
                    f.progress = 0.0;
                }
            } else if f.progress > 0.0 {
                f.progress = (f.progress - dt * 2.0).max(0.0);
            }
        }
        if let Some(ledger) = &mut self.material_ledger {
            for (_, stack) in &material_fuels {
                let materials = crate::materials::stack_materials(&reg, *stack);
                if let Err(error) = ledger.record_consumption(&materials) {
                    eprintln!("materials: furnace fuel accounting failed: {error}");
                }
            }
            for loss in material_losses {
                if let Err(error) = ledger.record_recipe_loss(&loss) {
                    eprintln!("materials: furnace process accounting failed: {error}");
                }
            }
            for materials in secondary_recoveries {
                if let Err(error) = ledger.record_secondary_recovery(&materials) {
                    eprintln!("materials: furnace secondary recovery failed: {error}");
                }
            }
        }
        for (pos, stack) in material_fuels {
            self.retire_arcane_stack_at(pos, stack, "furnace fuel consumed");
        }
        for (pos, stack) in arcane_inputs {
            self.retire_arcane_stack_at(pos, stack, "furnace input transformed");
        }
        for (pos, stack) in spat {
            self.push_drop_at(pos, stack);
        }
        self.tick_rail_motion(dt);
        self.tick_belts(dt);
        self.tick_local_structure_machines(dt);
    }

    // ---- block entity persistence (by item name, mod-change safe) ----
}

impl World {
    /// Fire every lit kiln: shared shell/weather rules, glass out. The
    /// shell is revalidated by the edit hook, not here; a chimneyed
    /// kiln reads its glassworks bonus from the folded stats.
    pub(super) fn tick_kilns(&mut self, dt: f32) {
        tick_kiln_machines(self, dt);
    }
}

impl World {
    /// Tick machines hosted inside spawned structures, with the same
    /// per-kind logic and progress as world-hosted machines, but
    /// structure-local state only. Structure machines are exempt from
    /// the main world's ire and material ledger by design; their
    /// outputs collect in `LocalStructure.outbox`.
    pub(super) fn tick_local_structure_machines(&mut self, dt: f32) {
        for structure in self.local_structures.iter_mut() {
            tick_bloomery_machines(structure, dt);
            tick_kiln_machines(structure, dt);
            tick_forge_machines(structure, dt);
            tick_separator_machines(structure, dt);
        }
    }
}

pub(super) fn tick_bloomery_machines<B: BlockStore>(store: &mut B, dt: f32) {
    let keys: Vec<B::Pos> = store
        .block_entities()
        .iter()
        .filter(|(_, e)| {
            matches!(e, BlockEntity::Multiblock(m)
                    if m.kind.handler(store.reg()) == Some(MachineHandler::Bloomery) && m.lit)
        })
        .map(|(k, _)| *k)
        .collect();
    for pos in keys {
        let Some(BlockEntity::Multiblock(mut b)) = store.block_entities_mut().remove(&pos) else {
            continue;
        };
        // An unroofed stack fights the rain and loses to a storm.
        let unroofed = b.core.is_some_and(|core| store.open_sky_above(core));
        let local_weather = store
            .to_world(pos)
            .map(|w| store.weather_at(w))
            .unwrap_or_default();
        let wet =
            local_weather.precipitation == crate::planet_atlas::PrecipitationForm::Rain && unroofed;
        if wet && local_weather.kind == crate::planet_atlas::LocalWeather::Storm {
            b.lit = false;
            b.progress = 0.0;
            store.swap_block_keep_entity(pos, "base:bloomery");
            store
                .block_entities_mut()
                .insert(pos, BlockEntity::Multiblock(b));
            continue;
        }
        let heat = b.stats.heat_multiplier();
        b.progress += dt * if wet { 0.5 } else { 1.0 } * heat;
        let fire_secs = store
            .reg()
            .machine(b.kind)
            .map(|def| def.fire_secs)
            .unwrap_or(crate::world::BLOOMERY_FIRE_SECS);
        if b.progress >= fire_secs {
            // Cash the batch: 2 charge + 2 fuel per bloom, +2 bonus
            // blooms on a full 8+8 firing.
            let chain = store.reg().bloomery.first().cloned();
            if let Some(chain) = chain {
                let n_charge: u32 = b.charge.iter().flatten().map(|s| s.count).sum();
                let n_fuel: u32 = b.fuel.iter().flatten().map(|s| s.count).sum();
                let units = n_charge.min(n_fuel) / 2;
                let blooms = units + if units >= 4 { 2 } else { 0 };
                let charge_used = units * 2;
                let fuel_used = units * 2;
                let input_materials = crate::materials::stack_materials(
                    store.reg(),
                    ItemStack::new(store.reg(), chain.charge, charge_used),
                );
                let output_materials = crate::materials::stack_materials(
                    store.reg(),
                    ItemStack::new(store.reg(), chain.bloom, blooms),
                );
                let mut process_loss = crate::registry::MaterialVector::new();
                for (material, input) in &input_materials {
                    let output = output_materials.get(material).copied().unwrap_or_default();
                    if *input > output {
                        process_loss.insert(material.clone(), input - output);
                    }
                }
                let mut fuel_materials = crate::registry::MaterialVector::new();
                let mut remaining = fuel_used;
                for stack in b.fuel.iter().flatten() {
                    let take = stack.count.min(remaining);
                    remaining -= take;
                    let materials = crate::materials::stack_materials(
                        store.reg(),
                        ItemStack {
                            count: take,
                            ..*stack
                        },
                    );
                    for (material, amount) in materials {
                        *fuel_materials.entry(material).or_default() += amount;
                    }
                    if remaining == 0 {
                        break;
                    }
                }
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
                eat(&mut b.charge, charge_used);
                eat(&mut b.fuel, fuel_used);
                if let Some(ledger) = store.material_ledger() {
                    if let Err(error) = ledger.record_consumption(&fuel_materials) {
                        eprintln!("materials: bloomery fuel accounting failed: {error}");
                    }
                    if let Err(error) = ledger.record_secondary_output(&process_loss) {
                        eprintln!("materials: bloomery slag accounting failed: {error}");
                    }
                }
                let reg = store.reg().clone();
                if blooms != 0 {
                    let out = ItemStack::new(&reg, chain.bloom, blooms);
                    // Blooms land in the first empty charge slot.
                    for s in b.charge.iter_mut() {
                        if s.is_none() {
                            *s = Some(out);
                            break;
                        }
                    }
                }
                if let Some(slag) = reg.item_id("base:iron_slag") {
                    let per_slag = reg.item(slag).materials.get("iron").copied().unwrap_or(1);
                    let slag_count =
                        process_loss.get("iron").copied().unwrap_or_default() / per_slag;
                    if slag_count != 0
                        && let Some(above) = store.to_world(pos).and_then(|w| w.offset(0, 1, 0))
                    {
                        store.push_drop_at(above, ItemStack::new(&reg, slag, slag_count as u32));
                    }
                }
            }
            b.lit = false;
            b.progress = 0.0;
            store.swap_block_keep_entity(pos, "base:bloomery");
        }
        store
            .block_entities_mut()
            .insert(pos, BlockEntity::Multiblock(b));
    }
}

pub(super) fn tick_forge_machines<B: BlockStore>(store: &mut B, dt: f32) {
    let keys: Vec<B::Pos> = store
        .block_entities()
        .iter()
        .filter(|(_, e)| {
            matches!(e, BlockEntity::Multiblock(m)
                    if m.kind.handler(store.reg()) == Some(MachineHandler::Forge) && m.lit)
        })
        .map(|(k, _)| *k)
        .collect();
    for pos in keys {
        let Some(BlockEntity::Multiblock(mut f)) = store.block_entities_mut().remove(&pos) else {
            continue;
        };
        f.progress += dt * f.stats.heat_multiplier();
        let fire_secs = store
            .reg()
            .machine(f.kind)
            .map(|def| def.fire_secs)
            .unwrap_or(crate::world::FORGE_FIRE_SECS);
        if f.progress >= fire_secs {
            let reg = store.reg().clone();
            let items_per_fuel = reg
                .machine(f.kind)
                .map(|def| def.items_per_fuel)
                .unwrap_or(crate::world::FORGE_ITEMS_PER_FUEL);
            let n_fuel: u32 = f.fuel.iter().flatten().map(|s| s.count).sum();
            let mut budget = n_fuel * items_per_fuel;
            let mut burned = 0u32;
            let mut outputs: Vec<ItemStack> = Vec::new();
            for s in f.charge.iter_mut() {
                let Some(st) = s else { continue };
                let input_item = st.item;
                let salvage = reg
                    .forge_salvage
                    .iter()
                    .find(|salvage| salvage.input == st.item)
                    .cloned();
                let smelt = reg
                    .smelts
                    .iter()
                    .find(|sm| sm.input.matches(st.item))
                    .cloned();
                let reclaim = crate::materials::is_reclaimable_stock(&reg, st.item);
                if salvage.is_none() && smelt.is_none() && !reclaim {
                    continue; // not smeltable/salvageable: survives
                }
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
                if reclaim {
                    let materials = crate::materials::stack_materials(
                        &reg,
                        ItemStack::new(&reg, input_item, n),
                    );
                    for (material, units) in materials {
                        *f.reclaim.entry(material).or_default() += units;
                    }
                    continue;
                }
                let output = salvage
                    .as_ref()
                    .map_or_else(|| smelt.as_ref().unwrap().output, |recipe| recipe.output);
                let mut out = ItemStack::new(&reg, output, 1);
                out.count = n;
                outputs.push(out);
                if let Some(recipe) = salvage {
                    debug_assert!(matches!(recipe.recovery_permille, 900 | 950));
                    let mut scale = ItemStack::new(&reg, recipe.byproduct, 1);
                    scale.count = n;
                    outputs.push(scale);
                    if let Some(ledger) = store.material_ledger() {
                        let materials = crate::materials::stack_materials(
                            &reg,
                            ItemStack::new(&reg, recipe.byproduct, n),
                        );
                        if let Err(error) = ledger.record_secondary_output(&materials) {
                            eprintln!("materials: forge scale accounting failed: {error}");
                        }
                    }
                } else if let Some((spit, sn)) = smelt.as_ref().and_then(|smelt| smelt.spit) {
                    let mut sp = ItemStack::new(&reg, spit, 1);
                    sp.count = sn * n;
                    outputs.push(sp);
                }
                if let Some(smelt) = smelt
                    && let Some(ledger) = store.material_ledger()
                    && let Err(error) = ledger.record_recipe_loss_scaled(&smelt.loss, n)
                {
                    eprintln!("materials: forge loss accounting failed: {error}");
                }
                if crate::materials::is_secondary_item(&reg, input_item)
                    && let Some(ledger) = store.material_ledger()
                {
                    let materials = crate::materials::stack_materials(
                        &reg,
                        ItemStack::new(&reg, input_item, n),
                    );
                    if let Err(error) = ledger.record_secondary_recovery(&materials) {
                        eprintln!("materials: forge slag recovery accounting failed: {error}");
                    }
                }
            }
            outputs.extend(crate::materials::consolidate_reclaimed_stock(
                &reg,
                &mut f.reclaim,
            ));
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
            let fuel_used = burned.div_ceil(FORGE_ITEMS_PER_FUEL);
            let mut fuel_materials = crate::registry::MaterialVector::new();
            let mut remaining_fuel = fuel_used;
            for stack in f.fuel.iter().flatten() {
                let take = stack.count.min(remaining_fuel);
                remaining_fuel -= take;
                let used = crate::materials::stack_materials(
                    &reg,
                    ItemStack {
                        count: take,
                        ..*stack
                    },
                );
                for (material, units) in used {
                    *fuel_materials.entry(material).or_default() += units;
                }
                if remaining_fuel == 0 {
                    break;
                }
            }
            eat(&mut f.fuel, fuel_used);
            if let Some(ledger) = store.material_ledger()
                && let Err(error) = ledger.record_consumption(&fuel_materials)
            {
                eprintln!("materials: forge fuel accounting failed: {error}");
            }
            let mouth = store.to_world(pos).and_then(|w| w.offset(0, 1, 0));
            for out in outputs {
                if let Some(above) = mouth {
                    store.push_drop_at(above, out);
                }
            }
            f.lit = false;
            f.progress = 0.0;
            store.swap_block_keep_entity(pos, "base:forge");
        }
        store
            .block_entities_mut()
            .insert(pos, BlockEntity::Multiblock(f));
    }
}

pub(super) fn tick_separator_machines<B: BlockStore>(store: &mut B, dt: f32) {
    let keys: Vec<B::Pos> = store
        .block_entities()
        .iter()
        .filter(|(_, e)| {
            matches!(e, BlockEntity::Multiblock(m)
                if m.kind.handler(store.reg()) == Some(MachineHandler::Separator))
        })
        .map(|(k, _)| *k)
        .collect();
    for pos in keys {
        let Some(BlockEntity::Multiblock(mut sp)) = store.block_entities_mut().remove(&pos) else {
            continue;
        };
        let def = store.reg().machine(sp.kind).cloned();
        let working = sp.powder >= 1 && sp.separator_fuel >= 1;
        if !working {
            sp.progress = 0.0;
        } else {
            sp.progress += dt;
            if sp.progress >= SEPARATE_SECS {
                sp.progress = 0.0;
                sp.powder -= 1;
                sp.separator_fuel -= 1;
                sp.neodymium += 1;
                sp.cerium += 2;
            }
        }
        let want = if working {
            def.as_ref()
                .and_then(|def| def.mouth_lit.clone())
                .unwrap_or_else(|| "base:separator_lit".to_string())
        } else {
            def.as_ref()
                .map(|def| def.mouth.clone())
                .unwrap_or_else(|| "base:separator".to_string())
        };
        if Some(store.get_block(pos)) != store.reg().block_id(&want) {
            store.swap_block_keep_entity(pos, &want);
        }
        store
            .block_entities_mut()
            .insert(pos, BlockEntity::Multiblock(sp));
    }
}

pub(super) fn tick_kiln_machines<B: BlockStore>(store: &mut B, dt: f32) {
    let keys: Vec<B::Pos> = store
        .block_entities()
        .iter()
        .filter(|(_, e)| {
            matches!(e, BlockEntity::Multiblock(m)
                    if m.kind.handler(store.reg()) == Some(MachineHandler::Kiln) && m.lit)
        })
        .map(|(k, _)| *k)
        .collect();
    for pos in keys {
        let Some(BlockEntity::Multiblock(mut k)) = store.block_entities_mut().remove(&pos) else {
            continue;
        };
        // A chimneyed kiln is a glassworks: rain can't reach the
        // fire, and the draft doubles what each fuel fires.
        let glassworks = k.stats.chimney;
        let unroofed = k.core.is_some_and(|core| store.open_sky_above(core));
        let local_weather = store
            .to_world(pos)
            .map(|w| store.weather_at(w))
            .unwrap_or_default();
        let wet = !glassworks
            && local_weather.precipitation == crate::planet_atlas::PrecipitationForm::Rain
            && unroofed;
        if wet && local_weather.kind == crate::planet_atlas::LocalWeather::Storm {
            k.lit = false;
            k.progress = 0.0;
            store.swap_block_keep_entity(pos, "base:kiln");
            store
                .block_entities_mut()
                .insert(pos, BlockEntity::Multiblock(k));
            continue;
        }
        let heat = k.stats.heat_multiplier();
        k.progress += dt * if wet { 0.5 } else { 1.0 } * heat;
        let fire_secs = store
            .reg()
            .machine(k.kind)
            .map(|def| def.fire_secs)
            .unwrap_or(crate::world::KILN_FIRE_SECS);
        if k.progress >= fire_secs {
            if let Some((_, fuel_item, clear)) = store.reg().kiln_base {
                let n_sand: u32 = k.charge.iter().flatten().map(|s| s.count).sum();
                let n_fuel: u32 = k.fuel.iter().flatten().map(|s| s.count).sum();
                let fuel_reach = if glassworks { n_fuel * 2 } else { n_fuel };
                let pairs = n_sand.min(fuel_reach) / 2;
                let out_n = pairs * 2;
                // One powder colors the whole batch.
                let colored = k.reagent.as_ref().and_then(|p| {
                    store
                        .reg()
                        .kiln
                        .iter()
                        .find(|recipe| recipe.powder == p.item)
                        .map(|recipe| recipe.glass)
                });
                let out_item = colored.unwrap_or(clear);
                let powder_materials = colored
                    .and(k.reagent)
                    .map(|stack| {
                        crate::materials::stack_materials(
                            store.reg(),
                            ItemStack { count: 1, ..stack },
                        )
                    })
                    .unwrap_or_default();
                if colored.is_some()
                    && let Some(p) = &mut k.reagent
                {
                    p.count -= 1;
                    if p.count == 0 {
                        k.reagent = None;
                    }
                }
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
                eat(&mut k.charge, pairs * 2);
                let fuel_used = if glassworks {
                    (pairs * 2).div_ceil(2)
                } else {
                    pairs * 2
                };
                let mut fuel_materials = crate::registry::MaterialVector::new();
                let mut remaining = fuel_used;
                for stack in k.fuel.iter().flatten() {
                    let take = stack.count.min(remaining);
                    remaining -= take;
                    let materials = crate::materials::stack_materials(
                        store.reg(),
                        ItemStack {
                            count: take,
                            ..*stack
                        },
                    );
                    for (material, amount) in materials {
                        *fuel_materials.entry(material).or_default() += amount;
                    }
                    if remaining == 0 {
                        break;
                    }
                }
                eat(&mut k.fuel, fuel_used);
                if let Some(ledger) = store.material_ledger() {
                    if let Err(error) = ledger.record_consumption(&powder_materials) {
                        eprintln!("materials: kiln pigment accounting failed: {error}");
                    }
                    if let Err(error) = ledger.record_consumption(&fuel_materials) {
                        eprintln!("materials: kiln fuel accounting failed: {error}");
                    }
                }
                let _ = fuel_item;
                if out_n > 0 {
                    let reg = store.reg().clone();
                    let mut out = ItemStack::new(&reg, out_item, 1);
                    out.count = out_n;
                    for s in k.charge.iter_mut() {
                        if s.is_none() {
                            *s = Some(out);
                            break;
                        }
                    }
                }
            }
            k.lit = false;
            k.progress = 0.0;
            store.swap_block_keep_entity(pos, "base:kiln");
        }
        store
            .block_entities_mut()
            .insert(pos, BlockEntity::Multiblock(k));
    }
}
