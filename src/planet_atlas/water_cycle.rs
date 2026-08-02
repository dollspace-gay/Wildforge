//! Exact fixed-point planetary water and salt ownership.
//!
//! Climate decides when and where transfers want to happen. This module owns
//! what they move. One hydro unit (HU) is 1/256 of a full voxel block; one
//! visible fluid level is 32 HU. Salt is integer mass, so concentration is a
//! derived presentation value and no mixing operation averages colours.

use std::collections::BTreeMap;

use super::*;
use crate::chunk::ChunkPos;

pub const HYDRO_UNITS_PER_BLOCK: u64 = 256;
pub const HYDRO_UNITS_PER_VISIBLE_LEVEL: u64 = 32;
pub const SALINITY_SCALE: u64 = 255;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum WaterClass {
    Fresh = 0,
    Brackish = 1,
    Salt = 2,
}

const RESERVOIR_KIND_SHIFT: u32 = 62;
const RESERVOIR_ID_MASK: u64 = (1u64 << RESERVOIR_KIND_SHIFT) - 1;

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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WaterCell {
    pub soil: ReservoirMass,
    pub snow: ReservoirMass,
    pub groundwater: ReservoirMass,
    pub runoff: ReservoirMass,
    pub groundwater_head_milliblocks: i32,
    pub last_recharge_hu: u32,
    pub last_spring_hu: u32,
    pub last_evaporation_hu: u32,
}

impl WaterCell {
    pub fn total(self) -> ReservoirMass {
        [self.soil, self.snow, self.groundwater, self.runoff]
            .into_iter()
            .fold(ReservoirMass::default(), |mut total, mass| {
                total.water_hu = total.water_hu.saturating_add(mass.water_hu);
                total.salt_mass = total.salt_mass.saturating_add(mass.salt_mass);
                total
            })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum AquiferLayer {
    Shallow = 0,
    Perched = 1,
    DeepConfined = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SparseAquiferState {
    pub pos: AtlasPos,
    pub layer: AquiferLayer,
    pub mass: ReservoirMass,
    pub capacity_hu: u64,
    pub head_milliblocks: i32,
    pub permeability: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum SurfaceReservoirKind {
    Dynamic = 0,
    Ocean = 1,
    Lake = 2,
    River = 3,
}

pub const fn surface_reservoir_id(kind: SurfaceReservoirKind, id: u32) -> u64 {
    ((kind as u64) << RESERVOIR_KIND_SHIFT) | id as u64
}

pub const fn surface_reservoir_parts(id: u64) -> (SurfaceReservoirKind, u32) {
    let kind = match id >> RESERVOIR_KIND_SHIFT {
        1 => SurfaceReservoirKind::Ocean,
        2 => SurfaceReservoirKind::Lake,
        3 => SurfaceReservoirKind::River,
        _ => SurfaceReservoirKind::Dynamic,
    };
    (kind, (id & RESERVOIR_ID_MASK) as u32)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SurfaceReservoirState {
    pub id: u64,
    pub name: String,
    /// Coarse, uncommitted water. This is the only surface mass that may be
    /// materialized into a never-before-generated chunk.
    pub coarse: ReservoirMass,
    /// Audit index for mass whose detailed authority is a saved chunk.
    pub committed: ReservoirMass,
    pub initial_total_hu: u64,
    pub level_milliblocks: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpringState {
    pub pos: AtlasPos,
    pub layer: AquiferLayer,
    pub outlet_milliblocks: i32,
    pub last_discharge_hu: u32,
    pub active: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FluxInbox {
    pub pos: AtlasPos,
    pub reservoir: u64,
    pub mass: ReservoirMass,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChunkWaterCommitment {
    pub chunk: ChunkPos,
    pub reservoir: u64,
    pub mass: ReservoirMass,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WaterLedger {
    pub initial_water_hu: u64,
    pub initial_salt_mass: u64,
    pub explicit_water_created_hu: u64,
    pub explicit_water_destroyed_hu: u64,
    pub explicit_salt_created: u64,
    pub explicit_salt_destroyed: u64,
    pub portable: [ReservoirMass; 3],
    pub industrial: ReservoirMass,
    pub precipitated_salt_mass: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WaterCycleState {
    pub completed_surface_hours: u64,
    pub completed_groundwater_days: u64,
    pub cells: AtlasGrid<WaterCell>,
    pub aquifers: Vec<SparseAquiferState>,
    pub reservoirs: Vec<SurfaceReservoirState>,
    pub springs: Vec<SpringState>,
    pub inboxes: Vec<FluxInbox>,
    pub commitments: Vec<ChunkWaterCommitment>,
    pub ledger: WaterLedger,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WaterAudit {
    pub atmosphere: ReservoirMass,
    pub soil: ReservoirMass,
    pub snow: ReservoirMass,
    pub groundwater: ReservoirMass,
    pub runoff: ReservoirMass,
    pub coarse_surface: ReservoirMass,
    pub voxel_surface: ReservoirMass,
    pub pending: ReservoirMass,
    pub portable: ReservoirMass,
    pub industrial: ReservoirMass,
    pub precipitated_salt_mass: u64,
    pub expected_water_hu: i128,
    pub current_water_hu: u64,
    pub expected_salt_mass: i128,
    pub current_salt_mass: u64,
    pub unexplained_water_delta_hu: i128,
    pub unexplained_salt_delta: i128,
}

fn sum_mass(target: &mut ReservoirMass, mass: ReservoirMass) {
    target.water_hu = target
        .water_hu
        .checked_add(mass.water_hu)
        .expect("validated water total fits u64");
    target.salt_mass = target
        .salt_mass
        .checked_add(mass.salt_mass)
        .expect("validated salt total fits u64");
}

impl WaterCycleState {
    pub fn audit(&self, atmosphere: ReservoirMass) -> WaterAudit {
        let mut audit = WaterAudit {
            atmosphere,
            portable: self.ledger.portable.into_iter().fold(
                ReservoirMass::default(),
                |mut total, mass| {
                    sum_mass(&mut total, mass);
                    total
                },
            ),
            industrial: self.ledger.industrial,
            precipitated_salt_mass: self.ledger.precipitated_salt_mass,
            ..WaterAudit::default()
        };
        for cell in self.cells.values() {
            sum_mass(&mut audit.soil, cell.soil);
            sum_mass(&mut audit.snow, cell.snow);
            sum_mass(&mut audit.groundwater, cell.groundwater);
            sum_mass(&mut audit.runoff, cell.runoff);
        }
        for aquifer in &self.aquifers {
            sum_mass(&mut audit.groundwater, aquifer.mass);
        }
        for reservoir in &self.reservoirs {
            sum_mass(&mut audit.coarse_surface, reservoir.coarse);
            sum_mass(&mut audit.voxel_surface, reservoir.committed);
        }
        for inbox in &self.inboxes {
            sum_mass(&mut audit.pending, inbox.mass);
        }
        let masses = [
            audit.atmosphere,
            audit.soil,
            audit.snow,
            audit.groundwater,
            audit.runoff,
            audit.coarse_surface,
            audit.voxel_surface,
            audit.pending,
            audit.portable,
            audit.industrial,
        ];
        audit.current_water_hu = masses
            .iter()
            .fold(0u64, |total, mass| total.saturating_add(mass.water_hu));
        audit.current_salt_mass = masses
            .iter()
            .fold(audit.precipitated_salt_mass, |total, mass| {
                total.saturating_add(mass.salt_mass)
            });
        audit.expected_water_hu = i128::from(self.ledger.initial_water_hu)
            + i128::from(self.ledger.explicit_water_created_hu)
            - i128::from(self.ledger.explicit_water_destroyed_hu);
        audit.expected_salt_mass = i128::from(self.ledger.initial_salt_mass)
            + i128::from(self.ledger.explicit_salt_created)
            - i128::from(self.ledger.explicit_salt_destroyed);
        audit.unexplained_water_delta_hu =
            i128::from(audit.current_water_hu) - audit.expected_water_hu;
        audit.unexplained_salt_delta =
            i128::from(audit.current_salt_mass) - audit.expected_salt_mass;
        audit
    }

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

    pub fn validate(&self, side: u16, atmosphere: ReservoirMass) -> Result<(), AtlasError> {
        if self.cells.side() != side || self.cells.len() != super::atlas_count(side)? {
            return Err(AtlasError::Corrupt(
                "water-cycle cell dimensions do not match atlas".into(),
            ));
        }
        if !self
            .reservoirs
            .windows(2)
            .all(|pair| pair[0].id < pair[1].id)
        {
            return Err(AtlasError::Corrupt(
                "surface reservoir ids are not unique and sorted".into(),
            ));
        }
        if self.aquifers.iter().any(|aquifer| {
            aquifer.pos.u >= side
                || aquifer.pos.v >= side
                || aquifer.mass.water_hu > aquifer.capacity_hu
        }) || self
            .springs
            .iter()
            .any(|spring| spring.pos.u >= side || spring.pos.v >= side)
            || self
                .inboxes
                .iter()
                .any(|inbox| inbox.pos.u >= side || inbox.pos.v >= side)
        {
            return Err(AtlasError::Corrupt(
                "water-cycle sparse record is outside its valid domain".into(),
            ));
        }
        let mut committed = BTreeMap::<u64, ReservoirMass>::new();
        for commitment in &self.commitments {
            let total = committed.entry(commitment.reservoir).or_default();
            total.add_assign(commitment.mass)?;
        }
        for (id, indexed) in committed {
            let Some(reservoir) = self
                .reservoirs
                .binary_search_by_key(&id, |reservoir| reservoir.id)
                .ok()
                .map(|index| &self.reservoirs[index])
            else {
                return Err(AtlasError::Corrupt(format!(
                    "chunk water commitment references missing reservoir {id}"
                )));
            };
            if indexed.water_hu > reservoir.committed.water_hu
                || indexed.salt_mass > reservoir.committed.salt_mass
            {
                return Err(AtlasError::Corrupt(format!(
                    "chunk water commitments exceed reservoir {id} ownership"
                )));
            }
        }
        let audit = self.audit(atmosphere);
        if audit.unexplained_water_delta_hu != 0 || audit.unexplained_salt_delta != 0 {
            return Err(AtlasError::Corrupt(format!(
                "water ledger mismatch: {} HU, {} salt mass",
                audit.unexplained_water_delta_hu, audit.unexplained_salt_delta
            )));
        }
        Ok(())
    }
}

impl PlanetAtlas {
    pub fn water_audit(&self) -> WaterAudit {
        self.water_cycle.audit(ReservoirMass::fresh(
            dynamic_water_total(&self.dynamic) as u64
        ))
    }

    pub fn water_audit_text(&self) -> String {
        use std::fmt::Write as _;

        let audit = self.water_audit();
        let mut out = String::new();
        let _ = writeln!(out, "Wildforge planetary water audit");
        let _ = writeln!(
            out,
            "surface hours: {}",
            self.water_cycle.completed_surface_hours
        );
        let _ = writeln!(
            out,
            "groundwater days: {}",
            self.water_cycle.completed_groundwater_days
        );
        for (name, mass) in [
            ("atmosphere", audit.atmosphere),
            ("soil", audit.soil),
            ("snow/ice reserve", audit.snow),
            ("groundwater", audit.groundwater),
            ("runoff", audit.runoff),
            ("coarse surface", audit.coarse_surface),
            ("voxel surface", audit.voxel_surface),
            ("pending exchanges", audit.pending),
            ("portable", audit.portable),
            ("industrial", audit.industrial),
        ] {
            let _ = writeln!(
                out,
                "{name}: {} HU, {} salt mass",
                mass.water_hu, mass.salt_mass
            );
        }
        let _ = writeln!(out, "precipitated salt: {}", audit.precipitated_salt_mass);
        let _ = writeln!(out, "expected water: {} HU", audit.expected_water_hu);
        let _ = writeln!(out, "current water: {} HU", audit.current_water_hu);
        let _ = writeln!(
            out,
            "unexplained water delta: {} HU",
            audit.unexplained_water_delta_hu
        );
        let _ = writeln!(out, "expected salt: {}", audit.expected_salt_mass);
        let _ = writeln!(out, "current salt: {}", audit.current_salt_mass);
        let _ = writeln!(
            out,
            "unexplained salt delta: {}",
            audit.unexplained_salt_delta
        );
        let _ = writeln!(out, "basins:");
        for reservoir in &self.water_cycle.reservoirs {
            let _ = writeln!(
                out,
                "  {}: level {:.3}, coarse {} HU, committed {} HU, salinity {}",
                reservoir.name,
                f64::from(reservoir.level_milliblocks) / 1000.0,
                reservoir.coarse.water_hu,
                reservoir.committed.water_hu,
                reservoir.coarse.salinity(),
            );
        }
        let mut drawdowns = self
            .water_cycle
            .cells
            .iter()
            .map(|(pos, cell)| {
                let baseline = self.genesis.ground.get(pos).map_or(0, |ground| {
                    (ground.baseline_groundwater_head * 1000.0) as i32
                });
                (
                    baseline.saturating_sub(cell.groundwater_head_milliblocks),
                    pos,
                )
            })
            .collect::<Vec<_>>();
        drawdowns.sort_by(|a, b| b.cmp(a));
        let _ = writeln!(out, "largest aquifer drawdowns (milliblocks):");
        for (drawdown, pos) in drawdowns.into_iter().take(8) {
            let _ = writeln!(out, "  {pos:?}: {drawdown}");
        }
        let mut pending = self.water_cycle.inboxes.clone();
        pending.sort_by_key(|inbox| std::cmp::Reverse(inbox.mass.water_hu));
        let _ = writeln!(out, "largest pending exchanges:");
        for inbox in pending.into_iter().take(8) {
            let _ = writeln!(
                out,
                "  {:?} reservoir {}: {} HU, {} salt",
                inbox.pos, inbox.reservoir, inbox.mass.water_hu, inbox.mass.salt_mass
            );
        }
        out
    }
}

pub(super) fn initial_water_cycle(
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

const WATER_PREFIX_BYTES: usize = 156;
const WATER_CELL_BYTES: usize = 80;

fn put_mass(out: &mut Vec<u8>, mass: ReservoirMass) {
    put_u64(out, mass.water_hu);
    put_u64(out, mass.salt_mass);
}

fn read_mass(reader: &mut ByteReader<'_>) -> Result<ReservoirMass, AtlasError> {
    Ok(ReservoirMass {
        water_hu: reader.u64()?,
        salt_mass: reader.u64()?,
    })
}

fn put_pos(out: &mut Vec<u8>, pos: AtlasPos) {
    put_u8(out, pos.face as u8);
    put_u16(out, pos.u);
    put_u16(out, pos.v);
}

fn read_pos(reader: &mut ByteReader<'_>, side: u16) -> Result<AtlasPos, AtlasError> {
    let face = Face::from_u8(reader.u8()?)
        .ok_or_else(|| AtlasError::Corrupt("water record has an invalid cube face".into()))?;
    AtlasPos::new(face, reader.u16()?, reader.u16()?, side)
}

fn put_layer(out: &mut Vec<u8>, layer: AquiferLayer) {
    put_u8(out, layer as u8);
}

fn read_layer(reader: &mut ByteReader<'_>) -> Result<AquiferLayer, AtlasError> {
    match reader.u8()? {
        0 => Ok(AquiferLayer::Shallow),
        1 => Ok(AquiferLayer::Perched),
        2 => Ok(AquiferLayer::DeepConfined),
        other => Err(AtlasError::Corrupt(format!(
            "unknown aquifer layer {other}"
        ))),
    }
}

pub(super) fn encode_water_cycle(state: &WaterCycleState) -> Result<Vec<u8>, AtlasError> {
    let mut out =
        Vec::with_capacity(WATER_PREFIX_BYTES + state.cells.len().saturating_mul(WATER_CELL_BYTES));
    put_u64(&mut out, state.completed_surface_hours);
    put_u64(&mut out, state.completed_groundwater_days);
    for value in [
        state.ledger.initial_water_hu,
        state.ledger.initial_salt_mass,
        state.ledger.explicit_water_created_hu,
        state.ledger.explicit_water_destroyed_hu,
        state.ledger.explicit_salt_created,
        state.ledger.explicit_salt_destroyed,
    ] {
        put_u64(&mut out, value);
    }
    for portable in state.ledger.portable {
        put_mass(&mut out, portable);
    }
    put_mass(&mut out, state.ledger.industrial);
    put_u64(&mut out, state.ledger.precipitated_salt_mass);
    for count in [
        state.aquifers.len(),
        state.reservoirs.len(),
        state.springs.len(),
        state.inboxes.len(),
        state.commitments.len(),
    ] {
        put_u32(
            &mut out,
            count
                .try_into()
                .map_err(|_| AtlasError::Corrupt("too many water-cycle records".into()))?,
        );
    }
    debug_assert_eq!(out.len(), WATER_PREFIX_BYTES);
    for cell in state.cells.values() {
        put_mass(&mut out, cell.soil);
        put_mass(&mut out, cell.snow);
        put_mass(&mut out, cell.groundwater);
        put_mass(&mut out, cell.runoff);
        put_i32(&mut out, cell.groundwater_head_milliblocks);
        put_u32(&mut out, cell.last_recharge_hu);
        put_u32(&mut out, cell.last_spring_hu);
        put_u32(&mut out, cell.last_evaporation_hu);
    }
    for aquifer in &state.aquifers {
        put_pos(&mut out, aquifer.pos);
        put_layer(&mut out, aquifer.layer);
        put_mass(&mut out, aquifer.mass);
        put_u64(&mut out, aquifer.capacity_hu);
        put_i32(&mut out, aquifer.head_milliblocks);
        put_u16(&mut out, aquifer.permeability);
    }
    for reservoir in &state.reservoirs {
        put_u64(&mut out, reservoir.id);
        let name = reservoir.name.as_bytes();
        put_u16(
            &mut out,
            name.len()
                .try_into()
                .map_err(|_| AtlasError::Corrupt("water reservoir name is too long".into()))?,
        );
        out.extend_from_slice(name);
        put_mass(&mut out, reservoir.coarse);
        put_mass(&mut out, reservoir.committed);
        put_u64(&mut out, reservoir.initial_total_hu);
        put_i32(&mut out, reservoir.level_milliblocks);
    }
    for spring in &state.springs {
        put_pos(&mut out, spring.pos);
        put_layer(&mut out, spring.layer);
        put_i32(&mut out, spring.outlet_milliblocks);
        put_u32(&mut out, spring.last_discharge_hu);
        put_u8(&mut out, u8::from(spring.active));
    }
    for inbox in &state.inboxes {
        put_pos(&mut out, inbox.pos);
        put_u64(&mut out, inbox.reservoir);
        put_mass(&mut out, inbox.mass);
    }
    for commitment in &state.commitments {
        put_u8(&mut out, commitment.chunk.face() as u8);
        put_u16(&mut out, commitment.chunk.u());
        put_u16(&mut out, commitment.chunk.v());
        put_u64(&mut out, commitment.reservoir);
        put_mass(&mut out, commitment.mass);
    }
    Ok(out)
}

pub(super) fn decode_water_cycle(side: u16, payload: &[u8]) -> Result<WaterCycleState, AtlasError> {
    let count = super::atlas_count(side)?;
    let minimum = WATER_PREFIX_BYTES
        .checked_add(count.checked_mul(WATER_CELL_BYTES).ok_or_else(|| {
            AtlasError::Corrupt("water-cycle cell payload length overflow".into())
        })?)
        .ok_or_else(|| AtlasError::Corrupt("water-cycle payload length overflow".into()))?;
    if payload.len() < minimum {
        return Err(AtlasError::Corrupt(
            "water-cycle payload is truncated".into(),
        ));
    }
    let mut reader = ByteReader::new(payload);
    let completed_surface_hours = reader.u64()?;
    let completed_groundwater_days = reader.u64()?;
    let ledger = WaterLedger {
        initial_water_hu: reader.u64()?,
        initial_salt_mass: reader.u64()?,
        explicit_water_created_hu: reader.u64()?,
        explicit_water_destroyed_hu: reader.u64()?,
        explicit_salt_created: reader.u64()?,
        explicit_salt_destroyed: reader.u64()?,
        portable: [
            read_mass(&mut reader)?,
            read_mass(&mut reader)?,
            read_mass(&mut reader)?,
        ],
        industrial: read_mass(&mut reader)?,
        precipitated_salt_mass: reader.u64()?,
    };
    let counts = [
        reader.u32()? as usize,
        reader.u32()? as usize,
        reader.u32()? as usize,
        reader.u32()? as usize,
        reader.u32()? as usize,
    ];
    if counts.iter().any(|value| *value > count.saturating_mul(16)) {
        return Err(AtlasError::Corrupt(
            "water-cycle sparse record count exceeds its bound".into(),
        ));
    }
    let mut cells = Vec::with_capacity(count);
    for _ in 0..count {
        cells.push(WaterCell {
            soil: read_mass(&mut reader)?,
            snow: read_mass(&mut reader)?,
            groundwater: read_mass(&mut reader)?,
            runoff: read_mass(&mut reader)?,
            groundwater_head_milliblocks: reader.i32()?,
            last_recharge_hu: reader.u32()?,
            last_spring_hu: reader.u32()?,
            last_evaporation_hu: reader.u32()?,
        });
    }
    let mut aquifers = Vec::with_capacity(counts[0]);
    for _ in 0..counts[0] {
        aquifers.push(SparseAquiferState {
            pos: read_pos(&mut reader, side)?,
            layer: read_layer(&mut reader)?,
            mass: read_mass(&mut reader)?,
            capacity_hu: reader.u64()?,
            head_milliblocks: reader.i32()?,
            permeability: reader.u16()?,
        });
    }
    let mut reservoirs = Vec::with_capacity(counts[1]);
    for _ in 0..counts[1] {
        let id = reader.u64()?;
        let name_len = usize::from(reader.u16()?);
        let name = std::str::from_utf8(reader.bytes(name_len)?)
            .map_err(|_| AtlasError::Corrupt("water reservoir name is not UTF-8".into()))?
            .to_owned();
        reservoirs.push(SurfaceReservoirState {
            id,
            name,
            coarse: read_mass(&mut reader)?,
            committed: read_mass(&mut reader)?,
            initial_total_hu: reader.u64()?,
            level_milliblocks: reader.i32()?,
        });
    }
    let mut springs = Vec::with_capacity(counts[2]);
    for _ in 0..counts[2] {
        springs.push(SpringState {
            pos: read_pos(&mut reader, side)?,
            layer: read_layer(&mut reader)?,
            outlet_milliblocks: reader.i32()?,
            last_discharge_hu: reader.u32()?,
            active: match reader.u8()? {
                0 => false,
                1 => true,
                _ => {
                    return Err(AtlasError::Corrupt(
                        "spring active flag is not boolean".into(),
                    ));
                }
            },
        });
    }
    let mut inboxes = Vec::with_capacity(counts[3]);
    for _ in 0..counts[3] {
        inboxes.push(FluxInbox {
            pos: read_pos(&mut reader, side)?,
            reservoir: reader.u64()?,
            mass: read_mass(&mut reader)?,
        });
    }
    let mut commitments = Vec::with_capacity(counts[4]);
    for _ in 0..counts[4] {
        let face = Face::from_u8(reader.u8()?)
            .ok_or_else(|| AtlasError::Corrupt("commitment has an invalid cube face".into()))?;
        let chunk = ChunkPos::new(face, reader.u16()?, reader.u16()?)
            .map_err(|error| AtlasError::Corrupt(format!("invalid committed chunk: {error}")))?;
        commitments.push(ChunkWaterCommitment {
            chunk,
            reservoir: reader.u64()?,
            mass: read_mass(&mut reader)?,
        });
    }
    if !reader.is_empty() {
        return Err(AtlasError::Corrupt(
            "water-cycle payload contains trailing bytes".into(),
        ));
    }
    Ok(WaterCycleState {
        completed_surface_hours,
        completed_groundwater_days,
        cells: AtlasGrid::from_values(side, cells)?,
        aquifers,
        reservoirs,
        springs,
        inboxes,
        commitments,
        ledger,
    })
}
