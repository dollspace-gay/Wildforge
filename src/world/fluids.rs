//! Finite-water scheduling, seam waking, and flow simulation.

use super::*;
use crate::planet::{BlockPos, Direction6, step6};

/// What a lava cell keeps back when it pours over an edge, so the flow
/// reads as one connected ribbon instead of a row of islands. Costs
/// nothing in conservation — it moves less, never more.
const LAVA_TRAIL: u8 = 1;

impl World {
    pub(super) fn surface_reservoir_at(&self, pos: BlockPos) -> Option<u64> {
        let atlas = self.planet_atlas.as_ref()?;
        let hydro = atlas.hydrology_sample(pos.surface().center());
        if hydro.ocean_basin_id != 0 {
            Some(crate::planet_atlas::surface_reservoir_id(
                crate::planet_atlas::SurfaceReservoirKind::Ocean,
                u32::from(hydro.ocean_basin_id),
            ))
        } else if hydro.lake_basin_id != 0 {
            Some(crate::planet_atlas::surface_reservoir_id(
                crate::planet_atlas::SurfaceReservoirKind::Lake,
                hydro.lake_basin_id,
            ))
        } else if hydro.river_id != 0 {
            Some(crate::planet_atlas::surface_reservoir_id(
                crate::planet_atlas::SurfaceReservoirKind::River,
                hydro.river_id,
            ))
        } else {
            None
        }
    }

    pub(super) fn register_player_waterwork_at(&mut self, pos: BlockPos) {
        if !crate::planet::neighbors6(pos)
            .any(|neighbor| self.reg.is_water(self.get_block_at(neighbor)))
        {
            return;
        }
        if let Some(weather) = self.weather_state.live_mut() {
            weather.ensure_dynamic_basin(pos.chunk(), i32::from(pos.y()) * 1000);
        }
    }

    /// A new void below the local water table is a real aquifer outlet. One
    /// visible parcel enters immediately; further excavation/pumping draws
    /// further parcels and lowers head through the same audited store. Caves
    /// opened beneath mapped ocean water debit that named ocean instead.
    pub(super) fn seep_into_excavation_at(&mut self, pos: BlockPos) {
        if self.get_block_at(pos) != AIR {
            return;
        }
        let Some(atlas) = &self.planet_atlas else {
            return;
        };
        let atlas_pos = atlas.atlas_pos(pos.surface());
        let index = atlas_pos.index(atlas.side());
        let hydro = atlas.genesis.hydrology.values()[index];
        let ground = atlas.genesis.ground.values()[index];
        let below_ocean = hydro.ocean_basin_id != 0 && i32::from(pos.y()) <= SEA_LEVEL;
        let below_water_table = self.weather_state.live().is_some_and(|weather| {
            weather.water.cells.values()[index].groundwater.water_hu
                >= crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL
                && weather.water.cells.values()[index].groundwater_head_milliblocks
                    > i32::from(pos.y()) * 1000
        });
        if !below_ocean && (!below_water_table || ground.aquifer_permeability < 8_192) {
            return;
        }
        let parcel = if below_ocean {
            let id = crate::planet_atlas::surface_reservoir_id(
                crate::planet_atlas::SurfaceReservoirKind::Ocean,
                u32::from(hydro.ocean_basin_id),
            );
            self.weather_state.live_mut().map_or(
                crate::planet_atlas::ReservoirMass::default(),
                |weather| {
                    weather.materialize_surface_water(
                        id,
                        crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL,
                    )
                },
            )
        } else {
            self.weather_state.live_mut().map_or(
                crate::planet_atlas::ReservoirMass::default(),
                |weather| {
                    weather.pump_groundwater(
                        atlas_pos,
                        crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL,
                    )
                },
            )
        };
        if parcel.water_hu == crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL {
            self.write_water_mass_at(pos, parcel);
        }
    }

    pub fn scoop_water_at(&mut self, pos: BlockPos) -> Option<crate::planet_atlas::WaterClass> {
        let mass = self.water_mass_at(pos)?;
        if mass.water_hu != crate::planet_atlas::HYDRO_UNITS_PER_BLOCK {
            return None;
        }
        let preferred = self.surface_reservoir_at(pos);
        let class = if let Some(weather) = self.weather_state.live_mut() {
            weather.move_detailed_to_portable_from(preferred, mass)?
        } else {
            mass.water_class()
        };
        self.set_block_at(pos, AIR);
        Some(class)
    }

    pub fn place_portable_water_at(
        &mut self,
        pos: BlockPos,
        class: crate::planet_atlas::WaterClass,
    ) -> bool {
        if self.get_block_at(pos) != AIR {
            return false;
        }
        self.player_touched.insert(pos.chunk());
        let mass = if let Some(weather) = self.weather_state.live_mut() {
            let Some(mass) = weather.move_portable_to_detailed(class) else {
                return false;
            };
            mass
        } else {
            crate::planet_atlas::ReservoirMass::with_salinity(
                crate::planet_atlas::HYDRO_UNITS_PER_BLOCK,
                match class {
                    crate::planet_atlas::WaterClass::Fresh => 0,
                    crate::planet_atlas::WaterClass::Brackish => 80,
                    crate::planet_atlas::WaterClass::Salt => 220,
                },
            )
        };
        self.write_water_mass_at(pos, mass);
        if let Some(weather) = self.weather_state.live_mut() {
            let _ = weather.register_dynamic_basin(pos.chunk(), i32::from(pos.y()) * 1000, mass);
        }
        true
    }
    pub(super) fn schedule_water_at(&mut self, pos: BlockPos) {
        if self.water_queued.insert(pos) {
            self.water_queue.push_back(pos);
        }
    }

    pub(super) fn schedule_lava_at(&mut self, pos: BlockPos) {
        if self.lava_queued.insert(pos) {
            self.lava_queue.push_back(pos);
        }
    }

    /// Wake both fluids around an edit: each tick skips cells that
    /// aren't its own fluid, and contact reactions need either side
    /// to notice the other.
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
        self.vaporize_water_at(water);
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

    pub(super) fn write_water_mass_at(
        &mut self,
        pos: BlockPos,
        mass: crate::planet_atlas::ReservoirMass,
    ) {
        debug_assert!(mass.water_hu <= 256);
        debug_assert!(
            mass.water_hu
                .is_multiple_of(crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL)
        );
        let volume = (mass.water_hu / crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL) as u8;
        let block = self.reg.water_for_volume(volume);
        let meta = mass.salinity();
        self.set_block_water_at(
            pos,
            block,
            meta,
            mass.salt_mass.min(u64::from(u16::MAX)) as u16,
        );
    }

    pub(crate) fn move_water_units(&mut self, from: BlockPos, to: BlockPos, units: u8) -> bool {
        let Some(source_before) = self.water_mass_at(from) else {
            return false;
        };
        let destination_before = self.water_mass_at(to).unwrap_or_default();
        let mut source = source_before;
        let parcel =
            source.take(u64::from(units) * crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL);
        let Some(combined) = destination_before.checked_add(parcel) else {
            return false;
        };
        if combined.water_hu > crate::planet_atlas::HYDRO_UNITS_PER_BLOCK {
            return false;
        }
        if self
            .move_tracked_water_carrier(
                from,
                to,
                source_before,
                destination_before,
                parcel.water_hu,
            )
            .is_err()
        {
            return false;
        }
        self.write_water_mass_at(to, combined);
        self.write_water_mass_at(from, source);
        true
    }

    /// Flash a detailed water voxel into the authoritative atmosphere. Salt
    /// remains as an audited precipitate until a halite block implementation
    /// consumes that store.
    pub(super) fn vaporize_water_at(&mut self, pos: BlockPos) {
        let Some(mass) = self.water_mass_at(pos) else {
            return;
        };
        let preferred = self.surface_reservoir_at(pos);
        let transferred = if let (Some(atlas), Some(weather)) =
            (&self.planet_atlas, self.weather_state.live_mut())
        {
            let atlas_pos = atlas.atlas_pos(pos.surface());
            weather.credit_detailed_vapor_from(atlas_pos, preferred, mass)
        } else {
            true
        };
        if transferred {
            self.set_block_at(pos, AIR);
        }
    }

    pub(super) fn evaporate_water_hu_at(&mut self, pos: BlockPos, requested_hu: u64) -> bool {
        let Some(mut source) = self.water_mass_at(pos) else {
            return false;
        };
        let retained_water = if source.salt_mass == 0 {
            0
        } else {
            crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL
        };
        let removable = source.water_hu.saturating_sub(retained_water);
        let vapor = source.take_fresh_water(requested_hu.min(removable));
        if vapor.water_hu == 0 {
            return false;
        }
        let preferred = self.surface_reservoir_at(pos);
        let transferred = if let (Some(atlas), Some(weather)) =
            (&self.planet_atlas, self.weather_state.live_mut())
        {
            weather.credit_detailed_vapor_from(atlas.atlas_pos(pos.surface()), preferred, vapor)
        } else {
            true
        };
        if transferred {
            self.write_water_mass_at(pos, source);
        }
        transferred
    }

    pub(super) fn freeze_water_at(&mut self, pos: BlockPos, ice: BlockId) -> bool {
        let Some(mass) = self.water_mass_at(pos) else {
            return false;
        };
        let retained = mass.salt_mass / 20;
        let rejected = mass.salt_mass.saturating_sub(retained);
        let preferred = self.surface_reservoir_at(pos);
        if rejected != 0
            && let (Some(atlas), Some(weather)) =
                (&self.planet_atlas, self.weather_state.live_mut())
            && !weather.reject_detailed_salt_to_runoff_from(
                atlas.atlas_pos(pos.surface()),
                preferred,
                rejected,
            )
        {
            return false;
        }
        self.set_block_water_at(
            pos,
            ice,
            retained.checked_div(mass.water_hu).unwrap_or(0).min(255) as u8,
            retained.min(u64::from(u16::MAX)) as u16,
        );
        true
    }

    pub(super) fn melt_ice_at(&mut self, pos: BlockPos) -> bool {
        let mass = crate::planet_atlas::ReservoirMass {
            water_hu: crate::planet_atlas::HYDRO_UNITS_PER_BLOCK,
            salt_mass: u64::from(self.get_water_salt_at(pos)),
        };
        self.write_water_mass_at(pos, mass);
        true
    }

    pub(super) fn melt_snow_at(&mut self, pos: BlockPos) -> bool {
        // A visible snow layer already belongs to detailed simulation. Thaw
        // it in place instead of dematerializing it into coarse runoff: the
        // same 32 HU remain visible, auditable, and available to flow.
        self.write_water_mass_at(
            pos,
            crate::planet_atlas::ReservoirMass::fresh(
                crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL,
            ),
        );
        true
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
                self.move_water_units(pos, below, t);
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
                self.move_water_units(pos, neighbor, t);
                changed = true;
                continue;
            }
            if let Some((neighbor, nv)) = best
                && v >= nv + 2
            {
                let t = ((v - nv) / 2).max(1);
                self.move_water_units(pos, neighbor, t);
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
                self.move_water_units(donor_top, receiver, t);
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
    /// enters one shared cascade instead of repeatedly rebuilding overlapping
    /// neighborhoods from scratch.
    fn flush_fluid_relights(&mut self) {
        self.fluid_batch = false;
        let starts = std::mem::take(&mut self.pending_relight);
        self.relight_chunks_and_cascade(starts);
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
