//! Validate geological manifests against the chosen immutable cell layers.

use crate::planet_atlas::{TectonicCell, TerrainCell, ResourceCell, AtlasError, GEOLOGY_SCHEMA_VERSION, atlas_count};
use crate::chunk::SEA_LEVEL;
use super::{GeologyModel, VolcanoSource, DetailedBoundary};
use super::volcanism::chamber_volume;

impl GeologyModel {
    pub(in crate::planet_atlas) fn validate(
        &self,
        side: u16,
        tectonics: &[TectonicCell],
        terrain: &[TerrainCell],
        resources: &[ResourceCell],
    ) -> Result<(), AtlasError> {
        let expected = atlas_count(side)?;
        if self.schema_version != GEOLOGY_SCHEMA_VERSION {
            return Err(AtlasError::UnsupportedVersion(format!(
                "geology schema {} (supported {})",
                self.schema_version, GEOLOGY_SCHEMA_VERSION
            )));
        }
        if tectonics.len() != expected || terrain.len() != expected || resources.len() != expected {
            return Err(AtlasError::Corrupt(
                "geology cell layers have inconsistent lengths".into(),
            ));
        }
        if !(16..=22).contains(&self.plates.len()) || !(6..=9).contains(&self.cratons.len()) {
            return Err(AtlasError::Corrupt(
                "plate or craton count is outside its fixed band".into(),
            ));
        }
        if side >= 32 {
            if !(0.62..=0.70).contains(&self.achieved_ocean_fraction)
                || self.largest_ocean_share < 0.90
            {
                return Err(AtlasError::Corrupt(
                    "geological land/ocean constraints are not satisfied".into(),
                ));
            }
            let major = self.continents.iter().filter(|record| record.major).count();
            if !(4..=7).contains(&major)
                || self
                    .continents
                    .iter()
                    .any(|record| record.share_of_land > 0.65)
            {
                return Err(AtlasError::Corrupt(
                    "continent constraints are not satisfied".into(),
                ));
            }
            for source in [VolcanoSource::ContinentalArc, VolcanoSource::IslandArc] {
                let sites: Vec<_> = self
                    .volcanoes
                    .iter()
                    .filter(|volcano| volcano.source == source)
                    .collect();
                if side >= 128
                    && !sites.is_empty()
                    && !sites.iter().any(|volcano| {
                        terrain[volcano.pos.index(side)].eroded_elevation > SEA_LEVEL as f32 + 2.0
                    })
                {
                    return Err(AtlasError::Corrupt(format!(
                        "{source:?} has no emergent volcanic edifice"
                    )));
                }
            }
        }
        if tectonics
            .iter()
            .any(|cell| usize::from(cell.plate_id) >= self.plates.len())
        {
            return Err(AtlasError::Corrupt(
                "cell references an unknown tectonic plate".into(),
            ));
        }
        let expected_source = |detail, cell: &TectonicCell| match detail {
            DetailedBoundary::OceanContinentSubduction => cell.continental_crust >= 32_768,
            DetailedBoundary::OceanOceanSubduction => {
                cell.continental_crust < 32_768 && cell.plate_id < cell.neighbor_plate
            }
            DetailedBoundary::ContinentalRift => cell.continental_crust >= 32_768,
            DetailedBoundary::OceanRidge => cell.continental_crust < 32_768,
            _ => false,
        };
        for (detail, source) in [
            (
                DetailedBoundary::OceanContinentSubduction,
                VolcanoSource::ContinentalArc,
            ),
            (
                DetailedBoundary::OceanOceanSubduction,
                VolcanoSource::IslandArc,
            ),
            (DetailedBoundary::ContinentalRift, VolcanoSource::Rift),
            (DetailedBoundary::OceanRidge, VolcanoSource::OceanRidge),
        ] {
            if tectonics.iter().any(|cell| {
                cell.boundary_distance == 0
                    && cell.boundary_detail == detail
                    && expected_source(detail, cell)
            }) && !self
                .volcanoes
                .iter()
                .any(|volcano| volcano.source == source)
            {
                return Err(AtlasError::Corrupt(format!(
                    "geology has a {detail:?} run but no {source:?} volcanic site"
                )));
            }
        }
        for (index, volcano) in self.volcanoes.iter().enumerate() {
            if volcano.id != index as u32 + 1
                || volcano.pos.u >= side
                || volcano.pos.v >= side
                || usize::from(volcano.plate_id) >= self.plates.len()
                || tectonics[volcano.pos.index(side)].plate_id != volcano.plate_id
                || volcano.chamber_volume_blocks < chamber_volume(volcano.chamber_radius_blocks)
            {
                return Err(AtlasError::Corrupt(
                    "volcano manifest contains an invalid site or finite chamber budget".into(),
                ));
            }
            let site = tectonics[volcano.pos.index(side)];
            let required_continental = match volcano.source {
                VolcanoSource::ContinentalArc | VolcanoSource::Rift => Some(true),
                VolcanoSource::IslandArc | VolcanoSource::OceanRidge => Some(false),
                VolcanoSource::Hotspot => None,
            };
            if required_continental
                .is_some_and(|required| (site.continental_crust >= 32_768) != required)
            {
                return Err(AtlasError::Corrupt(format!(
                    "{:?} volcano {} at {:?} is on the wrong crustal side ({})",
                    volcano.source, volcano.id, volcano.pos, site.continental_crust
                )));
            }
        }
        for (index, site) in self.deposits.iter().enumerate() {
            if site.id != index as u32 + 1
                || site.pos.u >= side
                || site.pos.v >= side
                || site.tonnage_blocks
                    < u64::from(site.max_blocks_per_chunk)
                        * u64::from(site.eligible_chunk_upper_bound)
            {
                return Err(AtlasError::Corrupt(
                    "deposit manifest contains an invalid budget or address".into(),
                ));
            }
        }
        for cell in resources {
            if cell.deposit_site_ref != 0 && cell.deposit_site_ref as usize > self.deposits.len() {
                return Err(AtlasError::Corrupt(
                    "resource cell references an unknown deposit".into(),
                ));
            }
        }
        Ok(())
    }
}
