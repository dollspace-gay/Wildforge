//! Water/salt accounting, ownership consistency, and read-only audit reports.

use std::collections::BTreeMap;
use crate::planet_atlas::{AtlasError, PlanetAtlas, dynamic_water_total};
use crate::planet_atlas::grid::atlas_count;
use super::{ReservoirMass, WaterCycleState};

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
}

impl WaterCycleState {
    pub fn validate(&self, side: u16, atmosphere: ReservoirMass) -> Result<(), AtlasError> {
        if self.cells.side() != side || self.cells.len() != atlas_count(side)? {
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
