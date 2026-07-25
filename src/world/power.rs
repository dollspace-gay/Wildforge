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

/// A cell plus the direction the walk entered it by.
type Step = ((i32, i32, i32), (i32, i32, i32));

/// A turning wheel carries this much momentum: seconds it keeps
/// delivering after its water goes still (flywheels are real, and a
/// race's flow flickers cell to cell as it equalizes).
pub const WHEEL_SPINDOWN_SECS: f32 = 8.0;

/// Wind strength by weather: the only machine that loves a storm.
pub fn wind_rate(w: Weather) -> f32 {
    match w {
        Weather::Clear => 0.6,
        Weather::Overcast => 0.8,
        Weather::Precip => 1.0,
        Weather::Storm => 1.4,
    }
}

impl World {
    /// Steam drives shafts like a wheel does, anywhere coal and
    /// water reach: an engine runs while a boiler beside it, firebox
    /// below, has both fire and water banked.
    pub(super) fn steam_rate(&self, x: i32, y: i32, z: i32) -> f32 {
        if self.steam_firebox(x, y, z).is_some() {
            STEAM_RATE
        } else {
            0.0
        }
    }

    /// The RUNNING firebox behind an engine block, if any: boiler
    /// horizontally adjacent to the engine, firebox directly below
    /// the boiler, fire and water both banked.
    pub(super) fn steam_firebox(&self, x: i32, y: i32, z: i32) -> Option<(i32, i32, i32)> {
        let boiler = self.reg.block_id("base:boiler")?;
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (bx, bz) = (x + dx, z + dz);
            if self.get_block(bx, y, bz) != boiler {
                continue;
            }
            let fpos = (bx, y - 1, bz);
            if let Some(BlockEntity::Steam(s)) = self.block_entities.get(&fpos)
                && s.fuel > 0.0
                && s.water > 0.0
            {
                return Some(fpos);
            }
        }
        None
    }

    /// Is this water cell going somewhere? Live means the drop rule
    /// could fire here: falling into room below, spilling over an
    /// edge, or pushing a real gradient at a neighbor. Standing
    /// pools turn nothing.
    pub fn is_live_water(&self, x: i32, y: i32, z: i32) -> bool {
        let Some(v) = self.reg.water_volume(self.get_block(x, y, z)) else {
            return false;
        };
        // Falling: the cell below has room.
        if y > 0
            && let Some(nv) = self.flow_potential(x, y - 1, z)
            && nv < 8
        {
            return true;
        }
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, nz) = (x + dx, z + dz);
            let Some(nv) = self.flow_potential(nx, y, nz) else {
                continue;
            };
            // Over an edge: an empty neighbor with room beneath it.
            if nv == 0
                && y > 0
                && let Some(bv) = self.flow_potential(nx, y - 1, nz)
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
    pub fn wheel_live(&self, x: i32, y: i32, z: i32) -> f32 {
        let touch = [
            (x, y - 1, z),
            (x + 1, y, z),
            (x - 1, y, z),
            (x, y, z + 1),
            (x, y, z - 1),
            (x + 1, y - 1, z),
            (x - 1, y - 1, z),
            (x, y - 1, z + 1),
            (x, y - 1, z - 1),
        ];
        for (tx, ty, tz) in touch {
            if self.is_live_water(tx, ty, tz) {
                return 1.0;
            }
        }
        0.0
    }

    /// Sails want altitude and open sky; the weather sets the rate.
    /// The only machine that fears nothing the kiln fears.
    pub fn sail_live(&self, x: i32, y: i32, z: i32) -> f32 {
        if y < 90 || self.light_at(x, y + 1, z).1 != 15 {
            return 0.0;
        }
        wind_rate(self.weather)
    }

    /// The rate a station's shaft line delivers: walk the millwork
    /// from this block to a live source. Shafts carry straight
    /// through, gears turn corners, bearing-fitted shafts forgive the
    /// wooden friction limit. Returns the strongest source found.
    pub fn power_at(&self, x: i32, y: i32, z: i32) -> f32 {
        let id = |n: &str| self.reg.block_id(n);
        let (Some(shaft), Some(gear)) = (id("base:shaft"), id("base:gear")) else {
            return 0.0;
        };
        let fitted = id("base:fitted_shaft");
        let wheel = [id("base:water_wheel"), id("base:water_wheel_run")];
        let sail = [id("base:windmill_sail"), id("base:windmill_sail_run")];
        let engine = [id("base:steam_engine"), id("base:steam_engine_run")];
        const DIRS: [(i32, i32, i32); 6] = [
            (1, 0, 0),
            (-1, 0, 0),
            (0, 1, 0),
            (0, -1, 0),
            (0, 0, 1),
            (0, 0, -1),
        ];
        let mut best: f32 = 0.0;
        // State: position, the direction we moved to enter it, and
        // wooden steps since the last bearing.
        let mut queue: VecDeque<(Step, u32)> = VecDeque::new();
        let mut seen: HashSet<Step> = HashSet::new();
        for d in DIRS {
            queue.push_back((((x + d.0, y + d.1, z + d.2), d), 1));
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
            let b = self.get_block(p.0, p.1, p.2);
            let src = if wheel.contains(&Some(b)) {
                // Live water or banked momentum: the dress tick keeps
                // the spin-down clock in station_work.
                if self.wheel_live(p.0, p.1, p.2) > 0.0
                    || self.station_work.get(&p).copied().unwrap_or(0.0) > 0.0
                {
                    1.0
                } else {
                    0.0
                }
            } else if sail.contains(&Some(b)) {
                self.sail_live(p.0, p.1, p.2)
            } else if engine.contains(&Some(b)) {
                self.steam_rate(p.0, p.1, p.2)
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
                let n = (p.0 + entry.0, p.1 + entry.1, p.2 + entry.2);
                queue.push_back(((n, entry), steps + 1));
            } else if b == gear {
                for d in DIRS {
                    // Never straight back into the face we came from.
                    if d == (-entry.0, -entry.1, -entry.2) {
                        continue;
                    }
                    queue.push_back((((p.0 + d.0, p.1 + d.1, p.2 + d.2), d), steps + 1));
                }
            }
        }
        best
    }
}
