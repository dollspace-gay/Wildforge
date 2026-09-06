//! Steam machine_tick transaction coordination.

use crate::registry::AIR;
use crate::world::BlockEntity;
use crate::planet::BlockPos;
use crate::world::STEAM_SECS_PER_WATER;
use crate::world::World;

impl World {
    /// Burn every steaming firebox: fire and water spend together,
    /// the boiler drinks adjacent cells when its bank runs low, and
    /// the firebox and engine dress to their running forms. Power
    /// leaves the river (mechanization stage 5).
    pub(in crate::world) fn tick_steam(&mut self, dt: f32) {
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
}
