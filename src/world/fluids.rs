//! Finite-water scheduling, seam waking, and flow simulation.

use super::*;

impl World {
    pub(super) fn schedule_water(&mut self, x: i32, y: i32, z: i32) {
        if self.water_queued.insert((x, y, z)) {
            self.water_queue.push_back((x, y, z));
        }
    }

    pub(super) fn schedule_lava(&mut self, x: i32, y: i32, z: i32) {
        if self.lava_queued.insert((x, y, z)) {
            self.lava_queue.push_back((x, y, z));
        }
    }

    /// Wake both fluids around an edit: each tick skips cells that
    /// aren't its own fluid, and contact reactions need either side
    /// to notice the other.
    pub fn wake_water(&mut self, x: i32, y: i32, z: i32) {
        for (dx, dy, dz) in [
            (0, 0, 0),
            (1, 0, 0),
            (-1, 0, 0),
            (0, 1, 0),
            (0, -1, 0),
            (0, 0, 1),
            (0, 0, -1),
        ] {
            self.schedule_water(x + dx, y + dy, z + dz);
            self.schedule_lava(x + dx, y + dy, z + dz);
        }
    }

    /// Wake water across a fresh chunk's seams: flow deferred at the
    /// edge of the generated world resumes here. Only genuine
    /// differentials queue — a flat ocean seam schedules nothing.
    pub(super) fn wake_seams(&mut self, pos: ChunkPos) {
        let mut wake = Vec::new();
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let np = ChunkPos {
                x: pos.x + dx,
                z: pos.z + dz,
            };
            if !self.chunks.contains_key(&np) {
                continue;
            }
            let n = if dx != 0 { CHUNK_Z } else { CHUNK_X } as i32;
            for i in 0..n {
                // World coords of the facing cells on each side.
                let (ax, az) = if dx != 0 {
                    (
                        pos.x * CHUNK_X as i32 + if dx == 1 { CHUNK_X as i32 - 1 } else { 0 },
                        pos.z * CHUNK_Z as i32 + i,
                    )
                } else {
                    (
                        pos.x * CHUNK_X as i32 + i,
                        pos.z * CHUNK_Z as i32 + if dz == 1 { CHUNK_Z as i32 - 1 } else { 0 },
                    )
                };
                let (bx, bz) = (ax + dx, az + dz);
                for y in 1..CHUNK_Y as i32 {
                    // Only fluid-meets-AIR differentials wake: a hole
                    // beside the sea must flood, but stepped worldgen
                    // water (a terraced river crossing the border)
                    // stays stepped until something actually disturbs
                    // it. Waking water-vs-water here set every steep
                    // river cascading on load — endless sim churn and
                    // remeshes, and the surface looked like broken
                    // glass while it sloshed.
                    let (ba, bb) = (self.get_block(ax, y, az), self.get_block(bx, y, bz));
                    let (a_air, b_air) = (self.reg.is_air(ba), self.reg.is_air(bb));
                    if a_air == b_air {
                        continue;
                    }
                    if let (Some(a), Some(b)) = (
                        self.flow_potential(ax, y, az),
                        self.flow_potential(bx, y, bz),
                    ) && a.abs_diff(b) >= 2
                    {
                        wake.push(if a > b { (ax, y, az) } else { (bx, y, bz) });
                    } else if let (Some(a), Some(b)) = (
                        self.lava_potential(ax, y, az),
                        self.lava_potential(bx, y, bz),
                    ) && a.abs_diff(b) >= 3
                    {
                        wake.push(if a > b { (ax, y, az) } else { (bx, y, bz) });
                    }
                }
            }
        }
        for (x, y, z) in wake {
            self.schedule_water(x, y, z);
            self.schedule_lava(x, y, z);
        }
    }

    /// Fire meets water: the lava cell hardens — obsidian when full,
    /// basalt when partial — and the touching water flashes away (the
    /// one documented exception to water conservation: the steam
    /// left). Both edits wake the neighborhood, so a fluid front
    /// hardens crust cell by cell until the two are separated.
    fn quench(&mut self, lava: (i32, i32, i32), water: (i32, i32, i32)) {
        let lv = self
            .reg
            .lava_volume(self.get_block(lava.0, lava.1, lava.2))
            .unwrap_or(0);
        let hard = if lv >= 8 {
            "base:obsidian"
        } else {
            "base:basalt"
        };
        if let Some(b) = self.reg.block_id(hard) {
            self.set_block(lava.0, lava.1, lava.2, b);
        }
        self.set_block(water.0, water.1, water.2, AIR);
    }

    /// The first watery neighbor of a cell, if any (6-connected).
    fn water_neighbor(&self, x: i32, y: i32, z: i32) -> Option<(i32, i32, i32)> {
        for (dx, dy, dz) in [
            (1, 0, 0),
            (-1, 0, 0),
            (0, 1, 0),
            (0, -1, 0),
            (0, 0, 1),
            (0, 0, -1),
        ] {
            if self.reg.is_water(self.get_block(x + dx, y + dy, z + dz)) {
                return Some((x + dx, y + dy, z + dz));
            }
        }
        None
    }

    /// The first lava neighbor of a cell, if any (6-connected).
    fn lava_neighbor(&self, x: i32, y: i32, z: i32) -> Option<(i32, i32, i32)> {
        for (dx, dy, dz) in [
            (1, 0, 0),
            (-1, 0, 0),
            (0, 1, 0),
            (0, -1, 0),
            (0, 0, 1),
            (0, 0, -1),
        ] {
            if self.reg.is_lava(self.get_block(x + dx, y + dy, z + dz)) {
                return Some((x + dx, y + dy, z + dz));
            }
        }
        None
    }

    /// Volume for flow comparisons: water carries its units, air can
    /// receive (0), anything else opts out of flow entirely.
    pub(super) fn flow_potential(&self, x: i32, y: i32, z: i32) -> Option<u8> {
        let b = self.get_block(x, y, z);
        if self.reg.is_air(b) {
            Some(0)
        } else {
            self.reg.water_volume(b)
        }
    }

    /// Finite water (docs/water-and-ticks-plan.md): each level encodes
    /// volume — level 0 is 8 units, level 7 a 1-unit film. On wake a
    /// cell falls as far as it can, then equalizes toward its lowest
    /// horizontal neighbor with a 2-unit hysteresis so the queue always
    /// quiesces. Volume moves; it is never created or destroyed. Flow
    /// toward ungenerated chunks defers (set_block there silently
    /// drops the write) — `wake_seams` resumes it when the neighbor
    /// generates.
    pub fn tick_water(&mut self, budget: usize) -> bool {
        let mut changed = false;
        for _ in 0..budget {
            let Some(pos) = self.water_queue.pop_front() else {
                break;
            };
            self.water_queued.remove(&pos);
            let (x, y, z) = pos;
            let Some(v) = self.reg.water_volume(self.get_block(x, y, z)) else {
                continue;
            };
            // Fire first: touching lava consumes this cell.
            if let Some(l) = self.lava_neighbor(x, y, z) {
                self.quench(l, (x, y, z));
                changed = true;
                continue;
            }
            // Fall first, greedily (below is always in our own chunk).
            if y > 0
                && let Some(nv) = self.flow_potential(x, y - 1, z)
                && nv < 8
            {
                let t = v.min(8 - nv);
                self.set_block(x, y - 1, z, self.reg.water_for_volume(nv + t));
                self.set_block(x, y, z, self.reg.water_for_volume(v - t));
                changed = true;
                continue;
            }
            // Equalize toward the lowest loaded horizontal neighbor.
            let mut best: Option<(i32, i32, u8)> = None;
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, nz) = (x + dx, z + dz);
                if !self.chunks.contains_key(&ChunkPos::of_world(nx, nz)) {
                    continue; // the world's edge: defer, don't spill
                }
                if let Some(nv) = self.flow_potential(nx, y, nz)
                    && best.is_none_or(|(_, _, b)| nv < b)
                {
                    best = Some((nx, nz, nv));
                }
            }
            if let Some((nx, nz, nv)) = best
                && v >= nv + 2
            {
                let t = ((v - nv) / 2).max(1);
                self.set_block(nx, y, nz, self.reg.water_for_volume(nv + t));
                self.set_block(x, y, z, self.reg.water_for_volume(v - t));
                changed = true;
            }
        }
        changed
    }

    /// Lava potential: air receives, lava carries, all else opts out.
    fn lava_potential(&self, x: i32, y: i32, z: i32) -> Option<u8> {
        let b = self.get_block(x, y, z);
        if self.reg.is_air(b) {
            Some(0)
        } else {
            self.reg.lava_volume(b)
        }
    }

    /// Finite lava: the water rules with a stiffer wrist. Same
    /// conservation law and border deferral, but a 3-unit hysteresis
    /// (lava creeps, water rushes) and the Server ticks it at a
    /// fraction of water's cadence. Contact with water hardens the
    /// lava cell instead of moving it.
    pub fn tick_lava(&mut self, budget: usize) -> bool {
        let mut changed = false;
        for _ in 0..budget {
            let Some(pos) = self.lava_queue.pop_front() else {
                break;
            };
            self.lava_queued.remove(&pos);
            let (x, y, z) = pos;
            let Some(v) = self.reg.lava_volume(self.get_block(x, y, z)) else {
                continue;
            };
            if let Some(w) = self.water_neighbor(x, y, z) {
                self.quench((x, y, z), w);
                changed = true;
                continue;
            }
            if y > 0
                && let Some(nv) = self.lava_potential(x, y - 1, z)
                && nv < 8
            {
                let t = v.min(8 - nv);
                self.set_block(x, y - 1, z, self.reg.lava_for_volume(nv + t));
                self.set_block(x, y, z, self.reg.lava_for_volume(v - t));
                changed = true;
                continue;
            }
            let mut best: Option<(i32, i32, u8)> = None;
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, nz) = (x + dx, z + dz);
                if !self.chunks.contains_key(&ChunkPos::of_world(nx, nz)) {
                    continue;
                }
                if let Some(nv) = self.lava_potential(nx, y, nz)
                    && best.is_none_or(|(_, _, b)| nv < b)
                {
                    best = Some((nx, nz, nv));
                }
            }
            if let Some((nx, nz, nv)) = best
                && v >= nv + 3
            {
                let t = ((v - nv) / 2).max(1);
                self.set_block(nx, y, nz, self.reg.lava_for_volume(nv + t));
                self.set_block(x, y, z, self.reg.lava_for_volume(v - t));
                changed = true;
            }
        }
        changed
    }
}
