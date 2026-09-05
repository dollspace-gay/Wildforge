//! Surface basin custody, storage levels, terminal salts, and river baseflow.

use crate::chunk::ChunkPos;
use crate::planet_atlas::{PlanetAtlas, ReservoirMass, SurfaceReservoirKind, SurfaceReservoirState, ChunkWaterCommitment, HydrologyCell, WaterCell, StoragePoint, LakeClass, HYDRO_UNITS_PER_VISIBLE_LEVEL, surface_reservoir_id, surface_reservoir_parts};
use super::PlanetaryWeather;

impl PlanetaryWeather {
    pub fn materialize_surface_water(
        &mut self,
        reservoir_id: u64,
        requested_hu: u64,
    ) -> ReservoirMass {
        let Some(reservoir) = self.water.reservoir_mut(reservoir_id) else {
            return ReservoirMass::default();
        };
        let parcel = reservoir.coarse.take(requested_hu);
        let reservoir = self
            .water
            .reservoir_mut(reservoir_id)
            .expect("source surface reservoir still exists");
        if reservoir.committed.add_assign(parcel).is_err() {
            reservoir
                .coarse
                .add_assign(parcel)
                .expect("surface rollback fits");
            return ReservoirMass::default();
        }
        parcel
    }

    pub fn dematerialize_surface_water(&mut self, reservoir_id: u64, mass: ReservoirMass) -> bool {
        if !self
            .water
            .debit_detailed_exact_from(Some(reservoir_id), mass)
        {
            return false;
        }
        let Some(reservoir) = self.water.reservoir_mut(reservoir_id) else {
            self.water
                .credit_detailed(mass)
                .expect("dematerialization rollback fits");
            return false;
        };
        reservoir.coarse.add_assign(mass).is_ok()
    }

    pub fn register_dynamic_basin(
        &mut self,
        chunk: ChunkPos,
        level_milliblocks: i32,
        mass: ReservoirMass,
    ) -> Option<u64> {
        let id = self.ensure_dynamic_basin(chunk, level_milliblocks);
        self.water.reservoir_mut(id)?.committed.checked_add(mass)?;
        if self
            .water
            .commitments
            .iter()
            .find(|commitment| commitment.chunk == chunk && commitment.reservoir == id)
            .is_some_and(|commitment| commitment.mass.checked_add(mass).is_none())
        {
            return None;
        }
        if !self.water.debit_detailed_exact(mass) {
            return None;
        }
        self.water
            .reservoir_mut(id)
            .expect("dynamic basin exists")
            .committed
            .add_assign(mass)
            .expect("dynamic basin addition was preflighted");
        if let Some(commitment) = self
            .water
            .commitments
            .iter_mut()
            .find(|commitment| commitment.chunk == chunk && commitment.reservoir == id)
        {
            commitment
                .mass
                .add_assign(mass)
                .expect("dynamic commitment addition was preflighted");
        } else if mass.water_hu != 0 || mass.salt_mass != 0 {
            self.water.commitments.push(ChunkWaterCommitment {
                chunk,
                reservoir: id,
                mass,
            });
            self.water
                .commitments
                .sort_by_key(|commitment| (commitment.chunk, commitment.reservoir));
        }
        Some(id)
    }

    /// Register the topological fact that player terrain can hold a local
    /// basin even before it contains a visible HU. Waterfront masonry and
    /// excavations call this; later bucket/flux transfers attach exact mass
    /// to the same stable chunk-derived id.
    pub fn ensure_dynamic_basin(&mut self, chunk: ChunkPos, level_milliblocks: i32) -> u64 {
        let local_id = (chunk.face() as u32)
            .saturating_mul(u32::from(crate::planet::FACE_CHUNKS).pow(2))
            .saturating_add(u32::from(chunk.v()) * u32::from(crate::planet::FACE_CHUNKS))
            .saturating_add(u32::from(chunk.u()))
            .saturating_add(1);
        let id = surface_reservoir_id(SurfaceReservoirKind::Dynamic, local_id);
        if self.water.reservoir_mut(id).is_none() {
            let insertion = self
                .water
                .reservoirs
                .binary_search_by_key(&id, |reservoir| reservoir.id)
                .unwrap_err();
            self.water.reservoirs.insert(
                insertion,
                SurfaceReservoirState {
                    id,
                    name: format!(
                        "player basin {}:{},{}",
                        chunk.face().name(),
                        chunk.u(),
                        chunk.v()
                    ),
                    coarse: ReservoirMass::default(),
                    committed: ReservoirMass::default(),
                    initial_total_hu: 0,
                    level_milliblocks,
                },
            );
        }
        id
    }

    pub fn breach_surface_reservoir(
        &mut self,
        source: u64,
        destination: u64,
        requested_hu: u64,
    ) -> ReservoirMass {
        let Some(source_index) = self
            .water
            .reservoirs
            .binary_search_by_key(&source, |reservoir| reservoir.id)
            .ok()
        else {
            return ReservoirMass::default();
        };
        let parcel = self.water.reservoirs[source_index]
            .coarse
            .take(requested_hu);
        let Some(destination) = self.water.reservoir_mut(destination) else {
            self.water.reservoirs[source_index]
                .coarse
                .add_assign(parcel)
                .expect("breach rollback fits");
            return ReservoirMass::default();
        };
        if destination.coarse.add_assign(parcel).is_err() {
            self.water.reservoirs[source_index]
                .coarse
                .add_assign(parcel)
                .expect("breach rollback fits");
            return ReservoirMass::default();
        }
        parcel
    }

    pub(super) fn reconcile_basin_levels(&mut self, atlas: &PlanetAtlas) {
        for reservoir in &mut self.water.reservoirs {
            let (kind, id) = surface_reservoir_parts(reservoir.id);
            let total_hu = reservoir
                .coarse
                .water_hu
                .saturating_add(reservoir.committed.water_hu);
            let volume_units = total_hu / HYDRO_UNITS_PER_VISIBLE_LEVEL;
            let curve = match kind {
                SurfaceReservoirKind::Ocean => atlas
                    .hydrology
                    .oceans
                    .iter()
                    .find(|record| u32::from(record.id) == id)
                    .map(|record| record.volume_elevation_curve.as_slice()),
                SurfaceReservoirKind::Lake => atlas
                    .hydrology
                    .lakes
                    .iter()
                    .find(|record| record.id == id)
                    .map(|record| record.volume_elevation_curve.as_slice()),
                _ => None,
            };
            if let Some(curve) = curve {
                reservoir.level_milliblocks = storage_level_milliblocks(curve, volume_units);
            }
        }
    }

    pub(super) fn precipitate_terminal_lake_salt(&mut self, atlas: &PlanetAtlas) {
        let mut total_precipitated = 0u64;
        for lake in &atlas.hydrology.lakes {
            if !matches!(
                lake.class,
                LakeClass::TerminalFresh | LakeClass::SalineTerminal | LakeClass::SeasonalPlaya
            ) {
                continue;
            }
            let id = surface_reservoir_id(SurfaceReservoirKind::Lake, lake.id);
            let Some(reservoir) = self.water.reservoir_mut(id) else {
                continue;
            };
            let saturation = reservoir.coarse.water_hu.saturating_mul(240);
            let excess = reservoir.coarse.salt_mass.saturating_sub(saturation);
            if excess == 0 {
                continue;
            }
            let precipitated = (excess / 64).max(1);
            reservoir.coarse.salt_mass -= precipitated;
            total_precipitated = total_precipitated.saturating_add(precipitated);
        }
        self.water.ledger.precipitated_salt_mass = self
            .water
            .ledger
            .precipitated_salt_mass
            .saturating_add(total_precipitated);
    }
}

pub(super) fn hydrology_surface_id(cell: HydrologyCell) -> Option<u64> {
    if cell.ocean_basin_id != 0 {
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
    }
}

/// Pressure-independent minimum river support from the shallow aquifer.
/// Stream order is the immutable hydrology distinction between perennial
/// channels and intermittent drainage lines; the transfer itself is bounded
/// by available groundwater and therefore stops under sustained drought.
pub(crate) fn take_river_baseflow(
    hydro: HydrologyCell,
    water: &mut WaterCell,
) -> Option<(u32, ReservoirMass)> {
    if hydro.river_id == 0 || hydro.stream_order < 3 {
        return None;
    }
    let parcel = water
        .groundwater
        .take((water.groundwater.water_hu / 4096).min(64));
    (parcel.water_hu != 0).then_some((hydro.river_id, parcel))
}

fn storage_level_milliblocks(curve: &[StoragePoint], volume_units: u64) -> i32 {
    let Some(first) = curve.first() else { return 0 };
    if volume_units <= first.volume_units {
        return (first.elevation * 1000.0).round() as i32;
    }
    for pair in curve.windows(2) {
        let [lower, upper] = pair else { unreachable!() };
        if volume_units <= upper.volume_units {
            let span = upper.volume_units.saturating_sub(lower.volume_units).max(1);
            let offset = volume_units.saturating_sub(lower.volume_units);
            let fraction = offset as f64 / span as f64;
            return ((f64::from(lower.elevation)
                + f64::from(upper.elevation - lower.elevation) * fraction)
                * 1000.0)
                .round() as i32;
        }
    }
    (curve.last().expect("curve is nonempty").elevation * 1000.0).round() as i32
}
