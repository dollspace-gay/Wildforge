//! Stations machine_tick transaction coordination.

use crate::registry::AIR;
use crate::world::BlockEntity;
use crate::planet::BlockPos;
use crate::world::ELEC_RADIUS;
use crate::world::HELVE_STRIKE_SECS;
use crate::inventory::ItemStack;
use crate::world::PUMP_REACH;
use crate::world::PUMP_STROKE_SECS;
use crate::world::STATION_STRIKE_SECS;
use crate::world::World;
use crate::world::station_powered;
use crate::world::worked_table_for;

impl World {
    /// Drive powered stations from their shaft lines: the millstone
    /// grinds unattended, the sawmill rips a whole load, the helve
    /// hammer works the smith's anvil at half his pace and none of
    /// his attention. Sources dress themselves: a wheel on live water
    /// and a sail in wind swap to their _run variants, and back.
    pub(in crate::world) fn tick_stations(&mut self, dt: f32) {
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
                    self.installations.wheel_momentum(pos, wet, dt, crate::world::power::WHEEL_SPINDOWN_SECS)
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
}
