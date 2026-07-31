//! Finite-water scheduling, seam waking, and flow simulation.

use super::*;
use crate::planet::{BlockPos, Direction6, step6};

/// What a lava cell keeps back when it pours over an edge, so the flow
/// reads as one connected ribbon instead of a row of islands. Costs
/// nothing in conservation — it moves less, never more.
const LAVA_TRAIL: u8 = 1;

impl World {
    pub(super) fn schedule_water_at(&mut self, pos: BlockPos) {
        if self.water_queued.insert(pos) {
            self.water_queue.push_back(pos);
        }
    }

    #[doc(hidden)]
    #[cfg(test)]
    pub(super) fn schedule_water(&mut self, x: i32, y: i32, z: i32) {
        if let Some(pos) = BlockPos::of_world(x, y, z) {
            self.schedule_water_at(pos);
        }
    }

    pub(super) fn schedule_lava_at(&mut self, pos: BlockPos) {
        if self.lava_queued.insert(pos) {
            self.lava_queue.push_back(pos);
        }
    }

    #[doc(hidden)]
    #[cfg(test)]
    pub(super) fn schedule_lava(&mut self, x: i32, y: i32, z: i32) {
        if let Some(pos) = BlockPos::of_world(x, y, z) {
            self.schedule_lava_at(pos);
        }
    }

    /// Wake both fluids around an edit: each tick skips cells that
    /// aren't its own fluid, and contact reactions need either side
    /// to notice the other.
    #[cfg(test)]
    pub fn wake_water(&mut self, x: i32, y: i32, z: i32) {
        if let Some(pos) = BlockPos::of_world(x, y, z) {
            self.wake_water_at(pos);
        }
    }

    pub fn wake_water_at(&mut self, pos: BlockPos) {
        self.schedule_water_at(pos);
        self.schedule_lava_at(pos);
        for neighbor in crate::planet::neighbors6(pos) {
            self.schedule_water_at(neighbor);
            self.schedule_lava_at(neighbor);
        }
    }

    /// Wake water across a fresh chunk's seams: flow deferred at the
    /// edge of the generated world resumes here. Only genuine
    /// differentials queue — a flat ocean seam schedules nothing.
    pub(super) fn wake_seams(&mut self, pos: ChunkPos) {
        let mut wake = Vec::new();
        for direction in [
            Direction6::East,
            Direction6::West,
            Direction6::North,
            Direction6::South,
        ] {
            let np = match direction {
                Direction6::East => pos.offset(1, 0),
                Direction6::West => pos.offset(-1, 0),
                Direction6::North => pos.offset(0, 1),
                Direction6::South => pos.offset(0, -1),
                _ => unreachable!(),
            };
            if !self.chunks.contains_key(&np) {
                continue;
            }
            let n = match direction {
                Direction6::East | Direction6::West => CHUNK_Z,
                _ => CHUNK_X,
            };
            for i in 0..n {
                let (u, v) = match direction {
                    Direction6::East => (
                        pos.u() * CHUNK_X as u16 + CHUNK_X as u16 - 1,
                        pos.v() * CHUNK_Z as u16 + i as u16,
                    ),
                    Direction6::West => (
                        pos.u() * CHUNK_X as u16,
                        pos.v() * CHUNK_Z as u16 + i as u16,
                    ),
                    Direction6::North => (
                        pos.u() * CHUNK_X as u16 + i as u16,
                        pos.v() * CHUNK_Z as u16 + CHUNK_Z as u16 - 1,
                    ),
                    Direction6::South => (
                        pos.u() * CHUNK_X as u16 + i as u16,
                        pos.v() * CHUNK_Z as u16,
                    ),
                    _ => unreachable!(),
                };
                for y in 1..CHUNK_Y as u16 {
                    let a = BlockPos::new(pos.face(), u, y as u8, v).unwrap();
                    let b = step6(a, direction).expect("horizontal planet step").pos;
                    // Only fluid-meets-AIR differentials wake: a hole
                    // beside the sea must flood, but stepped worldgen
                    // water (a terraced river crossing the border)
                    // stays stepped until something actually disturbs
                    // it. Waking water-vs-water here set every steep
                    // river cascading on load — endless sim churn and
                    // remeshes, and the surface looked like broken
                    // glass while it sloshed.
                    let (ba, bb) = (self.get_block_at(a), self.get_block_at(b));
                    let (a_air, b_air) = (self.reg.is_air(ba), self.reg.is_air(bb));
                    if a_air == b_air {
                        continue;
                    }
                    if let (Some(av), Some(bv)) =
                        (self.flow_potential_at(a), self.flow_potential_at(b))
                        && av.abs_diff(bv) >= 2
                    {
                        wake.push(if av > bv { a } else { b });
                    } else if let (Some(av), Some(bv)) =
                        (self.lava_potential_at(a), self.lava_potential_at(b))
                        && av.abs_diff(bv) >= 3
                    {
                        wake.push(if av > bv { a } else { b });
                    }
                }
            }
        }
        for pos in wake {
            self.schedule_water_at(pos);
            self.schedule_lava_at(pos);
        }
    }

    /// One sweep over a chunk returned from disk: wake any fluid saved
    /// mid-flow — hanging over air, or beside same-height air with a
    /// real potential differential. A settled, sealed chunk schedules
    /// nothing; water stranded by older unsealed worldgen (or saved
    /// mid-pour) resumes settling instead of hanging frozen until some
    /// edit happens to touch it. Border pairs are wake_seams' business.
    pub(super) fn wake_stale_fluids(&mut self, pos: ChunkPos) {
        let Some(c) = self.chunks.get(&pos) else {
            return;
        };
        let mut wake = Vec::new();
        for lx in 0..CHUNK_X {
            for lz in 0..CHUNK_Z {
                for y in 1..CHUNK_Y {
                    if !self.reg.is_fluid(c.get(lx, y, lz)) {
                        continue;
                    }
                    let here = BlockPos::new(
                        pos.face(),
                        pos.u() * CHUNK_X as u16 + lx as u16,
                        y as u8,
                        pos.v() * CHUNK_Z as u16 + lz as u16,
                    )
                    .unwrap();
                    if self.reg.is_air(c.get(lx, y - 1, lz)) {
                        wake.push(here);
                        continue;
                    }
                    let class = |cx: usize, cy: usize, cz: usize| -> u8 {
                        if cy + 1 >= CHUNK_Y {
                            return 1;
                        }
                        let a = c.get(cx, cy + 1, cz);
                        if self.reg.is_water(a) {
                            0
                        } else if self.reg.is_air(a) {
                            1
                        } else {
                            2
                        }
                    };
                    for (dx, dz) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                        let (nx, nz) = (lx as i32 + dx, lz as i32 + dz);
                        if !(0..CHUNK_X as i32).contains(&nx) || !(0..CHUNK_Z as i32).contains(&nz)
                        {
                            continue;
                        }
                        // A wet neighbor under a different roof — open
                        // sky here, water or rock there — marks a head
                        // cliff or a pipe mouth: the junctions of a
                        // drain saved mid-pour. Settled sealed worlds
                        // have none. Air neighbors take the potential
                        // check below instead.
                        if !self.reg.is_air(c.get(nx as usize, y, nz as usize)) {
                            if self.reg.is_water(c.get(lx, y, lz))
                                && self.reg.is_water(c.get(nx as usize, y, nz as usize))
                                && class(lx, y, lz) != class(nx as usize, y, nz as usize)
                            {
                                wake.push(here);
                                break;
                            }
                            continue;
                        }
                        let neighbor = BlockPos::new(
                            pos.face(),
                            pos.u() * CHUNK_X as u16 + nx as u16,
                            y as u8,
                            pos.v() * CHUNK_Z as u16 + nz as u16,
                        )
                        .unwrap();
                        if let (Some(a), Some(b)) = (
                            self.flow_potential_at(here),
                            self.flow_potential_at(neighbor),
                        ) && a.abs_diff(b) >= 2
                        {
                            wake.push(here);
                            break;
                        } else if let (Some(a), Some(b)) = (
                            self.lava_potential_at(here),
                            self.lava_potential_at(neighbor),
                        ) && a.abs_diff(b) >= 3
                        {
                            wake.push(here);
                            break;
                        }
                    }
                }
            }
        }
        for pos in wake {
            self.schedule_water_at(pos);
            self.schedule_lava_at(pos);
        }
    }

    /// Fire meets water: the lava cell hardens — obsidian when full,
    /// basalt when partial — and the touching water flashes away (the
    /// one documented exception to water conservation: the steam
    /// left). Both edits wake the neighborhood, so a fluid front
    /// hardens crust cell by cell until the two are separated.
    fn quench(&mut self, lava: BlockPos, water: BlockPos) {
        let lv = self.reg.lava_volume(self.get_block_at(lava)).unwrap_or(0);
        let hard = if lv >= 8 {
            "base:obsidian"
        } else {
            "base:basalt"
        };
        if let Some(b) = self.reg.block_id(hard) {
            self.set_block_at(lava, b);
        }
        self.set_block_at(water, AIR);
    }

    /// The first watery neighbor of a cell, if any (6-connected).
    fn water_neighbor(&self, pos: BlockPos) -> Option<BlockPos> {
        crate::planet::neighbors6(pos)
            .find(|neighbor| self.reg.is_water(self.get_block_at(*neighbor)))
    }

    /// The first lava neighbor of a cell, if any (6-connected).
    fn lava_neighbor(&self, pos: BlockPos) -> Option<BlockPos> {
        crate::planet::neighbors6(pos)
            .find(|neighbor| self.reg.is_lava(self.get_block_at(*neighbor)))
    }

    /// Volume for flow comparisons: water carries its units, air can
    /// receive (0), anything else opts out of flow entirely.
    pub(super) fn flow_potential_at(&self, pos: BlockPos) -> Option<u8> {
        let b = self.get_block_at(pos);
        if self.reg.is_air(b) {
            Some(0)
        } else {
            self.reg.water_volume(b)
        }
    }

    #[doc(hidden)]
    #[cfg(test)]
    pub(super) fn flow_potential(&self, x: i32, y: i32, z: i32) -> Option<u8> {
        BlockPos::of_world(x, y, z).and_then(|pos| self.flow_potential_at(pos))
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
        self.fluid_batch = true;
        let mut changed = false;
        for _ in 0..budget {
            let Some(pos) = self.water_queue.pop_front() else {
                break;
            };
            self.water_queued.remove(&pos);
            let Some(v) = self.reg.water_volume(self.get_block_at(pos)) else {
                continue;
            };
            // Fire first: touching lava consumes this cell.
            if let Some(lava) = self.lava_neighbor(pos) {
                self.quench(lava, pos);
                changed = true;
                continue;
            }
            // Fall first, greedily.
            if let Some(below) = step6(pos, Direction6::Down).map(|step| step.pos)
                && let Some(nv) = self.flow_potential_at(below)
                && nv < 8
            {
                let t = v.min(8 - nv);
                self.set_block_at(below, self.reg.water_for_volume(nv + t));
                self.set_block_at(pos, self.reg.water_for_volume(v - t));
                changed = true;
                continue;
            }
            // Equalize toward the lowest loaded horizontal neighbor.
            // A neighbor over a drop — air beside us with room in the
            // cell below it — drains without hysteresis: the moved
            // water leaves this layer for good (it falls before it
            // could ever slosh back), so even the last unit goes over
            // the edge and a breached pool empties instead of
            // stranding a lip. Only as much as the cell below can
            // swallow moves, keeping the push one-way.
            let mut best: Option<(BlockPos, u8)> = None;
            let mut drop: Option<(BlockPos, u8)> = None;
            for direction in [
                Direction6::East,
                Direction6::North,
                Direction6::West,
                Direction6::South,
            ] {
                let neighbor = step6(pos, direction).expect("horizontal planet step").pos;
                if !self.chunks.contains_key(&neighbor.chunk()) {
                    continue; // the world's edge: defer, don't spill
                }
                let Some(nv) = self.flow_potential_at(neighbor) else {
                    continue;
                };
                if best.is_none_or(|(_, volume)| nv < volume) {
                    best = Some((neighbor, nv));
                }
                if nv == 0
                    && let Some(below) = step6(neighbor, Direction6::Down).map(|step| step.pos)
                    && let Some(bv) = self.flow_potential_at(below)
                    && bv < 8
                    && drop.is_none_or(|(_, room)| 8 - bv > room)
                {
                    drop = Some((neighbor, 8 - bv));
                }
            }
            if let Some((neighbor, room)) = drop {
                let t = v.min(room);
                self.set_block_at(neighbor, self.reg.water_for_volume(t));
                self.set_block_at(pos, self.reg.water_for_volume(v - t));
                changed = true;
                continue;
            }
            if let Some((neighbor, nv)) = best
                && v >= nv + 2
            {
                let t = ((v - nv) / 2).max(1);
                self.set_block_at(neighbor, self.reg.water_for_volume(nv + t));
                self.set_block_at(pos, self.reg.water_for_volume(v - t));
                changed = true;
                continue;
            }
            // Communicating vessels: nothing moved locally, so this
            // cell serves as a junction between the columns it
            // touches. When their surfaces disagree, volume crosses
            // from the tallest column's top to the lowest's — pools
            // connected below the waterline level out even though
            // every layer at the link is full. Each transfer strictly
            // shrinks the head gap, so the queue still quiesces, and
            // a column capped by rock neither rises nor donates from
            // above (the recorded no-pressure limit now covers only
            // fully roofed plumbing).
            // A column receives at its partial top, or in the air
            // above a full one; capped by rock, it can only donate.
            let open = |top: (BlockPos, u8, i64)| -> bool {
                top.1 < 8
                    || step6(top.0, Direction6::Up)
                        .is_some_and(|step| self.get_block_at(step.pos) == AIR)
            };
            let own = self.water_column_top(pos);
            let mut donor = own;
            let mut recv = open(own).then_some(own);
            for direction in [
                Direction6::East,
                Direction6::North,
                Direction6::West,
                Direction6::South,
            ] {
                let neighbor = step6(pos, direction).expect("horizontal planet step").pos;
                if !self.chunks.contains_key(&neighbor.chunk())
                    || !self.reg.is_water(self.get_block_at(neighbor))
                {
                    continue;
                }
                let col = self.water_column_top(neighbor);
                if col.2 > donor.2 {
                    donor = col;
                }
                if open(col) && recv.is_none_or(|receiving| col.2 < receiving.2) {
                    recv = Some(col);
                }
            }
            let (donor_top, donor_volume, donor_head) = donor;
            if let Some((receiver_top, receiver_volume, receiver_head)) = recv
                && donor_head >= receiver_head + 2
            {
                let (receiver, existing) = if receiver_volume < 8 {
                    (receiver_top, receiver_volume)
                } else if let Some(step) = step6(receiver_top, Direction6::Up) {
                    (step.pos, 0)
                } else {
                    continue;
                };
                let t = ((donor_head - receiver_head) / 2)
                    .min(donor_volume as i64)
                    .min(8 - existing as i64) as u8;
                self.set_block_at(receiver, self.reg.water_for_volume(existing + t));
                self.set_block_at(donor_top, self.reg.water_for_volume(donor_volume - t));
                // Keep conducting until the heads meet.
                self.schedule_water_at(pos);
                changed = true;
            }
        }
        self.flush_fluid_relights();
        changed
    }

    /// The open top of a water column: ascend from a wet cell to the
    /// highest connected water above it. Returns (top y, top volume,
    /// head) where head counts total height in volume units — the
    /// quantity pressure equalizes between touching columns.
    fn water_column_top(&self, pos: BlockPos) -> (BlockPos, u8, i64) {
        let mut top = pos;
        let mut tv = self
            .reg
            .water_volume(self.get_block_at(pos))
            .unwrap_or_default();
        while let Some(above) = step6(top, Direction6::Up).map(|step| step.pos)
            && let Some(nv) = self.reg.water_volume(self.get_block_at(above))
        {
            top = above;
            tv = nv;
        }
        (top, tv, i64::from(top.y()) * 8 + i64::from(tv))
    }

    /// End-of-tick light settlement: every chunk a fluid front touched
    /// relights once, cascade included.
    fn flush_fluid_relights(&mut self) {
        self.fluid_batch = false;
        for pos in std::mem::take(&mut self.pending_relight) {
            self.relight_and_cascade(pos);
        }
    }

    /// Lava potential: air receives, lava carries, all else opts out.
    fn lava_potential_at(&self, pos: BlockPos) -> Option<u8> {
        let b = self.get_block_at(pos);
        if self.reg.is_air(b) {
            Some(0)
        } else {
            self.reg.lava_volume(b)
        }
    }

    #[doc(hidden)]
    #[cfg(test)]
    fn lava_potential(&self, x: i32, y: i32, z: i32) -> Option<u8> {
        BlockPos::of_world(x, y, z).and_then(|pos| self.lava_potential_at(pos))
    }

    /// Finite lava: the water rules with a stiffer wrist. Same
    /// conservation law and border deferral, but a 3-unit hysteresis
    /// (lava creeps, water rushes) and the Server ticks it at a
    /// fraction of water's cadence. Contact with water hardens the
    /// lava cell instead of moving it.
    pub fn tick_lava(&mut self, budget: usize) -> bool {
        self.fluid_batch = true;
        let mut changed = false;
        for _ in 0..budget {
            let Some(pos) = self.lava_queue.pop_front() else {
                break;
            };
            self.lava_queued.remove(&pos);
            let Some(v) = self.reg.lava_volume(self.get_block_at(pos)) else {
                continue;
            };
            if let Some(water) = self.water_neighbor(pos) {
                self.quench(pos, water);
                changed = true;
                continue;
            }
            // Lava sets light to what it runs against. Whose fire that
            // is depends on whose ground the lava is standing on: a
            // mountain's own flow across untouched country is the
            // wild's, but lava you led into a forest through a channel
            // you dug is a tool, and the channel marked the ground.
            if self.ignite_around_at(pos) {
                changed = true;
            }
            if let Some(below) = step6(pos, Direction6::Down).map(|step| step.pos)
                && let Some(nv) = self.lava_potential_at(below)
                && nv < 8
            {
                let t = v.min(8 - nv);
                self.set_block_at(below, self.reg.lava_for_volume(nv + t));
                self.set_block_at(pos, self.reg.lava_for_volume(v - t));
                changed = true;
                continue;
            }
            // Same drop rule as water, with one difference that is the
            // whole look of the thing: lava leaves a trail. Water
            // pours its full volume over an edge and the cell it left
            // becomes air, which is right for a rush — but on a slope
            // it marches downhill one step at a time and what you see
            // is a chain of disconnected puddles with rock between
            // them, not a flow. A viscous fluid coats what it runs
            // over, so a lava cell keeps its last unit and the ribbon
            // stays joined from the vent to the front.
            let mut best: Option<(BlockPos, u8)> = None;
            let mut drop: Option<(BlockPos, u8)> = None;
            for direction in [
                Direction6::East,
                Direction6::North,
                Direction6::West,
                Direction6::South,
            ] {
                let neighbor = step6(pos, direction).expect("horizontal planet step").pos;
                if !self.chunks.contains_key(&neighbor.chunk()) {
                    continue;
                }
                let Some(nv) = self.lava_potential_at(neighbor) else {
                    continue;
                };
                if best.is_none_or(|(_, volume)| nv < volume) {
                    best = Some((neighbor, nv));
                }
                if nv == 0
                    && let Some(below) = step6(neighbor, Direction6::Down).map(|step| step.pos)
                    && let Some(bv) = self.lava_potential_at(below)
                    && bv < 8
                    && drop.is_none_or(|(_, room)| 8 - bv > room)
                {
                    drop = Some((neighbor, 8 - bv));
                }
            }
            if let Some((neighbor, room)) = drop {
                let t = v.saturating_sub(LAVA_TRAIL).min(room);
                if t > 0 {
                    self.set_block_at(neighbor, self.reg.lava_for_volume(t));
                    self.set_block_at(pos, self.reg.lava_for_volume(v - t));
                    changed = true;
                    continue;
                }
            }
            if let Some((neighbor, nv)) = best
                && v >= nv + 3
            {
                let t = ((v - nv) / 2).max(1);
                self.set_block_at(neighbor, self.reg.lava_for_volume(nv + t));
                self.set_block_at(pos, self.reg.lava_for_volume(v - t));
                changed = true;
            }
        }
        self.flush_fluid_relights();
        changed
    }
}
