//! Exact water and salt parcels with conservative transfer arithmetic.

use crate::planet_atlas::AtlasError;
use super::SALINITY_SCALE;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum WaterClass {
    Fresh = 0,
    Brackish = 1,
    Salt = 2,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReservoirMass {
    pub water_hu: u64,
    pub salt_mass: u64,
}

impl ReservoirMass {
    pub fn fresh(water_hu: u64) -> Self {
        Self {
            water_hu,
            salt_mass: 0,
        }
    }

    pub fn with_salinity(water_hu: u64, salinity: u8) -> Self {
        Self {
            water_hu,
            salt_mass: water_hu.saturating_mul(u64::from(salinity)),
        }
    }

    pub fn salinity(self) -> u8 {
        if self.water_hu == 0 {
            return 0;
        }
        (self.salt_mass / self.water_hu).min(SALINITY_SCALE) as u8
    }

    pub fn water_class(self) -> WaterClass {
        match self.salinity() {
            0..=31 => WaterClass::Fresh,
            32..=127 => WaterClass::Brackish,
            _ => WaterClass::Salt,
        }
    }

    pub fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            water_hu: self.water_hu.checked_add(other.water_hu)?,
            salt_mass: self.salt_mass.checked_add(other.salt_mass)?,
        })
    }

    pub fn add_assign(&mut self, other: Self) -> Result<(), AtlasError> {
        *self = self.checked_add(other).ok_or_else(|| {
            AtlasError::Corrupt("water or salt reservoir overflowed its u64 budget".into())
        })?;
        Ok(())
    }

    /// Remove a proportional parcel. Integer division deliberately leaves
    /// the indivisible salt remainder in the source; moving the final HU
    /// carries every remainder, so repeated transfers conserve exactly.
    pub fn take(&mut self, requested_hu: u64) -> Self {
        let water_hu = requested_hu.min(self.water_hu);
        if water_hu == 0 {
            return Self::default();
        }
        let salt_mass = if water_hu == self.water_hu {
            self.salt_mass
        } else {
            ((u128::from(self.salt_mass) * u128::from(water_hu)) / u128::from(self.water_hu)) as u64
        };
        self.water_hu -= water_hu;
        self.salt_mass -= salt_mass;
        Self {
            water_hu,
            salt_mass,
        }
    }

    /// Remove an already-measured parcel without recomputing its
    /// concentration. Chunk generation uses this when the immutable
    /// hydrology layer has supplied exact local salt masses: taking a basin
    /// average here would erase river/lake salinity gradients as soon as the
    /// chunk became authoritative.
    pub fn take_exact(&mut self, requested: Self) -> Option<Self> {
        if self.water_hu < requested.water_hu || self.salt_mass < requested.salt_mass {
            return None;
        }
        self.water_hu -= requested.water_hu;
        self.salt_mass -= requested.salt_mass;
        Some(requested)
    }

    /// Evaporation moves only water. Dissolved salt remains and therefore
    /// becomes more concentrated.
    pub fn take_fresh_water(&mut self, requested_hu: u64) -> Self {
        let water_hu = requested_hu.min(self.water_hu);
        self.water_hu -= water_hu;
        Self::fresh(water_hu)
    }

    /// Freeze water while retaining only five percent of its proportional
    /// salt parcel. Rejected salt remains in the liquid source.
    pub fn freeze(&mut self, requested_hu: u64) -> Self {
        let before_salt = self.salt_mass;
        let mut frozen = self.take(requested_hu);
        let retained = frozen.salt_mass / 20;
        self.salt_mass = self
            .salt_mass
            .saturating_add(frozen.salt_mass.saturating_sub(retained));
        frozen.salt_mass = retained;
        debug_assert_eq!(before_salt, self.salt_mass + frozen.salt_mass);
        frozen
    }
}
