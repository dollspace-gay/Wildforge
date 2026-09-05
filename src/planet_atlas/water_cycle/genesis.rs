//! Initial finite water reservoirs derived from immutable atlas layers.

use std::collections::BTreeMap;
use crate::chunk::SEA_LEVEL;
use crate::planet_atlas::{AtlasError, AtlasGrid, AtlasPos, BasinKind, GenesisLayers, HydrologyModel, HABITAT_SPRING};
use super::{AquiferLayer, ReservoirMass, SparseAquiferState, SpringState, SurfaceReservoirKind, SurfaceReservoirState, WaterCell, WaterCycleState, WaterLedger, HYDRO_UNITS_PER_VISIBLE_LEVEL, surface_reservoir_id, surface_reservoir_parts};

pub(in crate::planet_atlas) fn initial_water_cycle(
    side: u16,
    genesis: &GenesisLayers,
    hydrology: &HydrologyModel,
    atmosphere: ReservoirMass,
) -> Result<WaterCycleState, AtlasError> {
    let mut water_cells = Vec::with_capacity(genesis.geometry.len());
    for index in 0..genesis.geometry.len() {
        let climate = genesis.climate.values()[index];
        let ground = genesis.ground.values()[index];
        let hydro = genesis.hydrology.values()[index];
        let soil_capacity_hu = u64::from(
            ground
                .aquifer_capacity
                .saturating_div(8)
                .max((climate.mean_precipitation * 3.0) as u32)
                .max(256),
        ) * HYDRO_UNITS_PER_VISIBLE_LEVEL;
        let soil_hu = ((climate.mean_precipitation * 4.0).max(0.0) as u64)
            .saturating_mul(HYDRO_UNITS_PER_VISIBLE_LEVEL)
            .min(soil_capacity_hu);
        let snow_hu = ((climate.snow_persistence * 1000.0).max(0.0) as u64)
            .saturating_mul(HYDRO_UNITS_PER_VISIBLE_LEVEL);
        let groundwater_hu =
            u64::from(ground.aquifer_capacity).saturating_mul(HYDRO_UNITS_PER_VISIBLE_LEVEL);
        let groundwater_salinity = if hydro.ocean_basin_id != 0 {
            hydro.salinity.saturating_sub(24)
        } else {
            hydro.salinity.min(63)
        };
        water_cells.push(WaterCell {
            soil: ReservoirMass::fresh(soil_hu),
            snow: ReservoirMass::fresh(snow_hu),
            groundwater: ReservoirMass::with_salinity(groundwater_hu, groundwater_salinity),
            runoff: ReservoirMass::default(),
            groundwater_head_milliblocks: (ground.baseline_groundwater_head * 1000.0)
                .round()
                .clamp(i32::MIN as f32, i32::MAX as f32)
                as i32,
            ..WaterCell::default()
        });
    }

    let mut surface = BTreeMap::<u64, ReservoirMass>::new();
    for cell in genesis.hydrology.values() {
        let id = if cell.ocean_basin_id != 0 {
            Some(surface_reservoir_id(
                SurfaceReservoirKind::Ocean,
                u32::from(cell.ocean_basin_id),
            ))
        } else if cell.lake_basin_id != 0 {
            Some(surface_reservoir_id(
                SurfaceReservoirKind::Lake,
                cell.lake_basin_id,
            ))
        } else if cell.river_id != 0 {
            Some(surface_reservoir_id(
                SurfaceReservoirKind::River,
                cell.river_id,
            ))
        } else {
            None
        };
        let Some(id) = id else { continue };
        let water_hu = cell
            .baseline_water_units
            .saturating_mul(HYDRO_UNITS_PER_VISIBLE_LEVEL);
        let mass = surface.entry(id).or_default();
        mass.water_hu = mass.water_hu.saturating_add(water_hu);
        mass.salt_mass = mass
            .salt_mass
            .saturating_add(water_hu.saturating_mul(u64::from(cell.salinity)));
    }
    let ocean_names: BTreeMap<_, _> = hydrology
        .oceans
        .iter()
        .map(|ocean| (u32::from(ocean.id), ocean.name.clone()))
        .collect();
    let lake_names: BTreeMap<_, _> = hydrology
        .lakes
        .iter()
        .map(|lake| (lake.id, lake.name.clone()))
        .collect();
    let river_names: BTreeMap<_, _> = hydrology
        .rivers
        .iter()
        .map(|river| (river.id, river.name.clone()))
        .collect();
    let mut reservoirs = Vec::with_capacity(surface.len());
    for (id, coarse) in surface {
        let (kind, local_id) = surface_reservoir_parts(id);
        let name = match kind {
            SurfaceReservoirKind::Ocean => ocean_names.get(&local_id),
            SurfaceReservoirKind::Lake => lake_names.get(&local_id),
            SurfaceReservoirKind::River => river_names.get(&local_id),
            SurfaceReservoirKind::Dynamic => None,
        }
        .cloned()
        .unwrap_or_else(|| format!("dynamic reservoir {local_id}"));
        let level = match kind {
            SurfaceReservoirKind::Ocean => SEA_LEVEL * 1000,
            SurfaceReservoirKind::Lake => hydrology
                .lakes
                .iter()
                .find(|lake| lake.id == local_id)
                .map_or(SEA_LEVEL * 1000, |lake| {
                    (lake.surface_elevation * 1000.0).round() as i32
                }),
            _ => 0,
        };
        reservoirs.push(SurfaceReservoirState {
            id,
            name,
            initial_total_hu: coarse.water_hu,
            coarse,
            committed: ReservoirMass::default(),
            level_milliblocks: level,
        });
    }

    let mut aquifers = Vec::new();
    let mut springs = Vec::new();
    for index in 0..genesis.geometry.len() {
        let pos = AtlasPos::from_index(index, side).expect("water-cycle atlas index");
        let ground = genesis.ground.values()[index];
        let tectonic = genesis.tectonics.values()[index];
        let terrain = genesis.terrain.values()[index];
        if ground.aquifer_permeability > 43_000 && index.is_multiple_of(97) {
            let capacity_hu = u64::from(ground.aquifer_capacity)
                .saturating_mul(HYDRO_UNITS_PER_VISIBLE_LEVEL / 2);
            aquifers.push(SparseAquiferState {
                pos,
                layer: AquiferLayer::Perched,
                mass: ReservoirMass::fresh(capacity_hu * 3 / 5),
                capacity_hu,
                head_milliblocks: ((terrain.eroded_elevation - 2.0) * 1000.0) as i32,
                permeability: ground.aquifer_permeability,
            });
        }
        if (tectonic.sediment_basin != BasinKind::None || tectonic.fault_intensity > 36_000)
            && index.is_multiple_of(131)
        {
            let capacity_hu =
                u64::from(ground.aquifer_capacity).saturating_mul(HYDRO_UNITS_PER_VISIBLE_LEVEL);
            aquifers.push(SparseAquiferState {
                pos,
                layer: AquiferLayer::DeepConfined,
                mass: ReservoirMass::fresh(capacity_hu * 4 / 5),
                capacity_hu,
                head_milliblocks: ((terrain.eroded_elevation + 1.0) * 1000.0) as i32,
                permeability: ground.aquifer_permeability / 2,
            });
        }
        if genesis.biomes.values()[index].habitat_flags & HABITAT_SPRING != 0 {
            springs.push(SpringState {
                pos,
                layer: AquiferLayer::Shallow,
                outlet_milliblocks: (terrain.eroded_elevation * 1000.0) as i32,
                last_discharge_hu: 0,
                active: false,
            });
        }
    }
    aquifers.sort_by_key(|aquifer| (aquifer.pos, aquifer.layer as u8));
    springs.sort_by_key(|spring| spring.pos);

    let mut state = WaterCycleState {
        completed_surface_hours: 0,
        completed_groundwater_days: 0,
        cells: AtlasGrid::from_values(side, water_cells)?,
        aquifers,
        reservoirs,
        springs,
        inboxes: Vec::new(),
        commitments: Vec::new(),
        ledger: WaterLedger::default(),
    };
    let audit = state.audit(atmosphere);
    state.ledger.initial_water_hu = audit.current_water_hu;
    state.ledger.initial_salt_mass = audit.current_salt_mass;
    Ok(state)
}
