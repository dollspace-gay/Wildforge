//! Detailed water debit/credit coordinated with reservoir commitments.

use crate::planet_atlas::AtlasError;
use super::{ReservoirMass, SurfaceReservoirKind, SurfaceReservoirState, WaterCycleState, surface_reservoir_id};

impl WaterCycleState {
    pub fn reservoir_mut(&mut self, id: u64) -> Option<&mut SurfaceReservoirState> {
        self.reservoirs
            .binary_search_by_key(&id, |reservoir| reservoir.id)
            .ok()
            .map(|index| &mut self.reservoirs[index])
    }

    /// Remove an exact detailed parcel from chunk-owned accounting. Water
    /// and salt are debited independently because evaporation can remove
    /// fresh water while leaving all dissolved salt behind. A preferred
    /// reservoir keeps local basin changes local; the remaining stores are a
    /// deterministic fallback for mixed/player-moved water whose original
    /// basin is no longer knowable from the voxel alone.
    pub fn debit_detailed_exact_from(
        &mut self,
        preferred: Option<u64>,
        mass: ReservoirMass,
    ) -> bool {
        let available =
            self.reservoirs
                .iter()
                .fold(ReservoirMass::default(), |mut total, reservoir| {
                    total.water_hu = total.water_hu.saturating_add(reservoir.committed.water_hu);
                    total.salt_mass = total
                        .salt_mass
                        .saturating_add(reservoir.committed.salt_mass);
                    total
                });
        if available.water_hu < mass.water_hu || available.salt_mass < mass.salt_mass {
            return false;
        }
        let mut order = Vec::with_capacity(self.reservoirs.len());
        if let Some(id) = preferred
            && let Ok(index) = self
                .reservoirs
                .binary_search_by_key(&id, |reservoir| reservoir.id)
        {
            order.push(index);
        }
        for index in 0..self.reservoirs.len() {
            if !order.contains(&index) {
                order.push(index);
            }
        }
        let mut water_left = mass.water_hu;
        let mut salt_left = mass.salt_mass;
        for index in order {
            let reservoir = &mut self.reservoirs[index];
            let water = water_left.min(reservoir.committed.water_hu);
            reservoir.committed.water_hu -= water;
            water_left -= water;
            let salt = salt_left.min(reservoir.committed.salt_mass);
            reservoir.committed.salt_mass -= salt;
            salt_left -= salt;

            let mut commitment_water = water;
            let mut commitment_salt = salt;
            for commitment in self
                .commitments
                .iter_mut()
                .filter(|commitment| commitment.reservoir == reservoir.id)
            {
                let taken_water = commitment_water.min(commitment.mass.water_hu);
                commitment.mass.water_hu -= taken_water;
                commitment_water -= taken_water;
                let taken_salt = commitment_salt.min(commitment.mass.salt_mass);
                commitment.mass.salt_mass -= taken_salt;
                commitment_salt -= taken_salt;
                if commitment_water == 0 && commitment_salt == 0 {
                    break;
                }
            }
            if water_left == 0 && salt_left == 0 {
                break;
            }
        }
        self.commitments
            .retain(|commitment| commitment.mass.water_hu != 0 || commitment.mass.salt_mass != 0);
        debug_assert_eq!((water_left, salt_left), (0, 0));
        true
    }

    pub fn debit_detailed_exact(&mut self, mass: ReservoirMass) -> bool {
        self.debit_detailed_exact_from(None, mass)
    }

    pub fn credit_detailed_to(
        &mut self,
        preferred: Option<u64>,
        mass: ReservoirMass,
    ) -> Result<(), AtlasError> {
        let id =
            preferred.unwrap_or_else(|| surface_reservoir_id(SurfaceReservoirKind::Dynamic, 0));
        if self.reservoir_mut(id).is_none() {
            if preferred.is_some() {
                return Err(AtlasError::Corrupt(format!(
                    "detailed water references missing reservoir {id}"
                )));
            }
            let insertion = self
                .reservoirs
                .binary_search_by_key(&id, |reservoir| reservoir.id)
                .unwrap_err();
            self.reservoirs.insert(
                insertion,
                SurfaceReservoirState {
                    id,
                    name: "materialized voxel water".into(),
                    coarse: ReservoirMass::default(),
                    committed: ReservoirMass::default(),
                    initial_total_hu: 0,
                    level_milliblocks: 0,
                },
            );
        }
        self.reservoir_mut(id)
            .expect("detailed reservoir exists")
            .committed
            .add_assign(mass)
    }

    pub fn credit_detailed(&mut self, mass: ReservoirMass) -> Result<(), AtlasError> {
        self.credit_detailed_to(None, mass)
    }
}
