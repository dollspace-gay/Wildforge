//! Daily shallow and sparse-aquifer exchange in deterministic edge order.

use super::PlanetaryWeather;
use crate::planet_atlas::{
    AtlasError, AtlasPos, HYDRO_UNITS_PER_VISIBLE_LEVEL, PlanetAtlas, ReservoirMass,
};

impl PlanetaryWeather {
    pub(super) fn advance_groundwater_day(
        &mut self,
        atlas: &PlanetAtlas,
    ) -> Result<(), AtlasError> {
        let side = atlas.side();
        let count = self.water.cells.len();
        let mut available = self
            .water
            .cells
            .values()
            .iter()
            .map(|cell| cell.groundwater)
            .collect::<Vec<_>>();
        let mut inbound = vec![ReservoirMass::default(); count];
        let mut edges = std::collections::BTreeSet::new();
        for index in 0..count {
            let pos = AtlasPos::from_index(index, side).expect("groundwater index");
            for neighbor in pos.neighbors4(side) {
                let neighbor = neighbor.index(side);
                if neighbor != index {
                    edges.insert((index.min(neighbor), index.max(neighbor)));
                }
            }
        }
        for (index, neighbor) in edges {
            let a_head = self.water.cells.values()[index].groundwater_head_milliblocks;
            let b_head = self.water.cells.values()[neighbor].groundwater_head_milliblocks;
            let (source, destination, gradient) = if a_head > b_head {
                (index, neighbor, u64::from((a_head - b_head) as u32))
            } else {
                (neighbor, index, u64::from((b_head - a_head) as u32))
            };
            if gradient < 2 {
                continue;
            }
            let permeability = u64::from(
                atlas.genesis.ground.values()[source]
                    .aquifer_permeability
                    .min(atlas.genesis.ground.values()[destination].aquifer_permeability),
            );
            let capacity = u64::from(atlas.genesis.ground.values()[destination].aquifer_capacity)
                .saturating_mul(HYDRO_UNITS_PER_VISIBLE_LEVEL);
            let room = capacity.saturating_sub(
                available[destination]
                    .water_hu
                    .saturating_add(inbound[destination].water_hu),
            );
            let requested = gradient
                .saturating_mul(permeability)
                .saturating_div(65_535 * 512)
                .max(1)
                .min(room)
                .min(available[source].water_hu / 128 + 1);
            let parcel = available[source].take(requested);
            inbound[destination].add_assign(parcel)?;
        }
        for index in 0..count {
            available[index].add_assign(inbound[index])?;
            let cell = &mut self.water.cells.values_mut()[index];
            cell.groundwater = available[index];
            let ground = atlas.genesis.ground.values()[index];
            let capacity = u64::from(ground.aquifer_capacity)
                .saturating_mul(HYDRO_UNITS_PER_VISIBLE_LEVEL)
                .max(1);
            let saturation_milli = cell
                .groundwater
                .water_hu
                .saturating_mul(4_000)
                .saturating_div(capacity)
                .min(4_000) as i32;
            cell.groundwater_head_milliblocks =
                (ground.baseline_groundwater_head * 1000.0).round() as i32 - 4_000
                    + saturation_milli;
        }
        // Perched and confined stores exchange slowly with the shallow cell.
        // This keeps their pressure response visible without voxelizing pores.
        for aquifer in &mut self.water.aquifers {
            let index = aquifer.pos.index(side);
            let shallow = &mut self.water.cells.values_mut()[index];
            if aquifer.head_milliblocks > shallow.groundwater_head_milliblocks {
                let parcel = aquifer.mass.take((aquifer.mass.water_hu / 2048).max(1));
                shallow.groundwater.add_assign(parcel)?;
            } else {
                let room = aquifer.capacity_hu.saturating_sub(aquifer.mass.water_hu);
                let parcel = shallow
                    .groundwater
                    .take((shallow.groundwater.water_hu / 4096).min(room));
                aquifer.mass.add_assign(parcel)?;
            }
            let fullness = aquifer
                .mass
                .water_hu
                .saturating_mul(4_000)
                .saturating_div(aquifer.capacity_hu.max(1)) as i32;
            aquifer.head_milliblocks = aquifer.head_milliblocks.saturating_sub(2_000) + fullness;
        }
        self.water.completed_groundwater_days =
            self.water.completed_groundwater_days.saturating_add(1);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn advance_groundwater_day_for_test(
        &mut self,
        atlas: &PlanetAtlas,
    ) -> Result<(), AtlasError> {
        self.advance_groundwater_day(atlas)
    }
}
