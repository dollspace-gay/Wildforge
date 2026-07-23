//! Finite-water scheduling, seam waking, and flow simulation.

use super::*;

impl World {
    pub(super) fn schedule_water(&mut self, x: i32, y: i32, z: i32) {
        if self.water_queued.insert((x, y, z)) {
            self.water_queue.push_back((x, y, z));
        }
    }

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
                    if let (Some(a), Some(b)) = (
                        self.flow_potential(ax, y, az),
                        self.flow_potential(bx, y, bz),
                    ) && a.abs_diff(b) >= 2
                    {
                        wake.push(if a > b { (ax, y, az) } else { (bx, y, bz) });
                    }
                }
            }
        }
        for (x, y, z) in wake {
            self.schedule_water(x, y, z);
        }
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
}
