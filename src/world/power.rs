//! Power: a boolean with a rate, never a number with a cable.
//!
//! Two site-bound sources (live water, high wind) and one earned one
//! (steam, once the machining age bores its cylinder) drive stations
//! through SHAFTS — real blocks walked by a bounded search, not an
//! energy graph. Wooden millwork is lossy: past WOODEN_RUN blocks a
//! run refuses until a bearing-fitted shaft resets the count.

use super::*;

/// Friction is real: the longest bearing-free stretch of wooden
/// millwork that still turns.
pub const WOODEN_RUN: u32 = 12;

/// Search bound: no legal shaft layout needs more visits than this,
/// and a pathological loop of gears stops wasting time here.
const POWER_VISITS: usize = 192;

/// A cell plus the face-local direction the walk entered it by.
type Step = (BlockPos, crate::planet::Direction6);

/// A turning wheel carries this much momentum: seconds it keeps
/// delivering after its water goes still (flywheels are real, and a
/// race's flow flickers cell to cell as it equalizes).
pub const WHEEL_SPINDOWN_SECS: f32 = 8.0;

impl World {
    /// Steam drives shafts like a wheel does, anywhere coal and
    /// water reach: an engine runs while a boiler beside it, firebox
    /// below, has both fire and water banked.
    pub(super) fn steam_rate_at(&self, pos: BlockPos) -> f32 {
        if self.steam_firebox_at(pos).is_some() {
            STEAM_RATE
        } else {
            0.0
        }
    }

    /// The RUNNING firebox behind an engine block, if any: boiler
    /// horizontally adjacent to the engine, firebox directly below
    /// the boiler, fire and water both banked.
    pub(super) fn steam_firebox_at(&self, pos: BlockPos) -> Option<BlockPos> {
        let boiler = self.reg.block_id("base:boiler")?;
        for direction in [
            crate::planet::Direction6::East,
            crate::planet::Direction6::North,
            crate::planet::Direction6::West,
            crate::planet::Direction6::South,
        ] {
            let boiler_pos = crate::planet::step6(pos, direction)?.pos;
            if self.get_block_at(boiler_pos) != boiler {
                continue;
            }
            let Some(firebox_pos) = boiler_pos.offset(0, -1, 0) else {
                continue;
            };
            if let Some(BlockEntity::Steam(s)) = self.installations.get(&firebox_pos)
                && !s.draft_closed
                && s.fuel > 0.0
                && s.water.water_hu > 0
            {
                return Some(firebox_pos);
            }
        }
        None
    }

    /// Is this water cell going somewhere? Live means the drop rule
    /// could fire here: falling into room below, spilling over an
    /// edge, or pushing a real gradient at a neighbor. Standing
    /// pools turn nothing.
    pub fn is_live_water_at(&self, pos: BlockPos) -> bool {
        let Some(v) = self.reg.water_volume(self.get_block_at(pos)) else {
            return false;
        };
        // Falling: the cell below has room.
        if let Some(below) = pos.offset(0, -1, 0)
            && let Some(nv) = self.flow_potential_at(below)
            && nv < 8
        {
            return true;
        }
        for neighbor in [
            crate::planet::Direction6::East,
            crate::planet::Direction6::North,
            crate::planet::Direction6::West,
            crate::planet::Direction6::South,
        ]
        .into_iter()
        .filter_map(|direction| crate::planet::step6(pos, direction).map(|step| step.pos))
        {
            let Some(nv) = self.flow_potential_at(neighbor) else {
                continue;
            };
            // Over an edge: an empty neighbor with room beneath it.
            if nv == 0
                && let Some(below) = neighbor.offset(0, -1, 0)
                && let Some(bv) = self.flow_potential_at(below)
                && bv < 8
            {
                return true;
            }
            // A real gradient: the creep rule would move water here.
            if v >= nv + 2 {
                return true;
            }
        }
        false
    }

    /// A water wheel turns only on LIVE water in one of the six cells
    /// it hangs into — a weir lip, a spring race, a built channel.
    pub fn wheel_live_at(&self, pos: BlockPos) -> f32 {
        let touch = [
            (0, -1, 0),
            (1, 0, 0),
            (-1, 0, 0),
            (0, 0, 1),
            (0, 0, -1),
            (1, -1, 0),
            (-1, -1, 0),
            (0, -1, 1),
            (0, -1, -1),
        ];
        for at in touch
            .into_iter()
            .filter_map(|(du, dy, dv)| pos.offset(du, dy, dv))
        {
            if self.is_live_water_at(at) {
                return 1.0;
            }
        }
        0.0
    }

    /// Sails want altitude and open sky; the weather sets the rate.
    /// The only machine that fears nothing the kiln fears.
    pub fn sail_live_at(&self, pos: BlockPos) -> f32 {
        if pos.y() < 90
            || pos
                .offset(0, 1, 0)
                .is_none_or(|above| self.light_at_pos(above).1 != 15)
        {
            return 0.0;
        }
        let weather = self.weather_at_surface(pos.surface());
        let speed = weather.wind[0].hypot(weather.wind[1]);
        (0.45
            + speed * 0.75
            + if weather.kind == crate::planet_atlas::LocalWeather::Storm {
                0.25
            } else {
                0.0
            })
        .clamp(0.35, 1.6)
    }

    /// The rate a station's shaft line delivers: walk the millwork
    /// from this block to a live source. Shafts carry straight
    /// through, gears turn corners, bearing-fitted shafts forgive the
    /// wooden friction limit. Returns the strongest source found.
    pub fn power_at_pos(&self, pos: BlockPos) -> f32 {
        let id = |n: &str| self.reg.block_id(n);
        let (Some(shaft), Some(gear)) = (id("base:shaft"), id("base:gear")) else {
            return 0.0;
        };
        let fitted = id("base:fitted_shaft");
        let wheel = [id("base:water_wheel"), id("base:water_wheel_run")];
        let sail = [id("base:windmill_sail"), id("base:windmill_sail_run")];
        let engine = [id("base:steam_engine"), id("base:steam_engine_run")];
        let mut best: f32 = 0.0;
        // State: position, the direction we moved to enter it, and
        // wooden steps since the last bearing.
        let mut queue: VecDeque<(Step, u32)> = VecDeque::new();
        let mut seen: HashSet<Step> = HashSet::new();
        for direction in crate::planet::Direction6::ALL {
            if let Some(step) = crate::planet::step6(pos, direction) {
                queue.push_back(((step.pos, step.direction), 1));
            }
        }
        let mut visits = 0;
        while let Some(((p, entry), steps)) = queue.pop_front() {
            // The run limit counts millwork BETWEEN station and
            // source; the source itself may sit one step past it.
            if visits >= POWER_VISITS || steps > WOODEN_RUN + 1 {
                continue;
            }
            if !seen.insert((p, entry)) {
                continue;
            }
            visits += 1;
            let b = self.get_block_at(p);
            let src = if wheel.contains(&Some(b)) {
                // Live water or banked momentum: the dress tick keeps
                // the installation-owned spin-down clock.
                if self.wheel_live_at(p) > 0.0 || self.installations.work_at(p) > 0.0 {
                    1.0
                } else {
                    0.0
                }
            } else if sail.contains(&Some(b)) {
                self.sail_live_at(p)
            } else if engine.contains(&Some(b)) {
                self.steam_rate_at(p)
            } else {
                0.0
            };
            if src > 0.0 {
                best = best.max(src);
                continue;
            }
            if b == shaft || Some(b) == fitted {
                // Straight through only; a bearing resets the count.
                let steps = if Some(b) == fitted { 0 } else { steps };
                if let Some(next) = crate::planet::step6(p, entry) {
                    queue.push_back(((next.pos, next.direction), steps + 1));
                }
            } else if b == gear {
                for direction in crate::planet::Direction6::ALL {
                    // Never straight back into the face we came from.
                    if direction == entry.opposite() {
                        continue;
                    }
                    if let Some(next) = crate::planet::step6(p, direction) {
                        queue.push_back(((next.pos, next.direction), steps + 1));
                    }
                }
            }
        }
        best
    }

    #[cfg(test)]
    pub fn wheel_live(&self, x: i32, y: i32, z: i32) -> f32 {
        BlockPos::of_world(x, y, z).map_or(0.0, |pos| self.wheel_live_at(pos))
    }

    #[cfg(test)]
    pub fn power_at(&self, x: i32, y: i32, z: i32) -> f32 {
        BlockPos::of_world(x, y, z).map_or(0.0, |pos| self.power_at_pos(pos))
    }
}
