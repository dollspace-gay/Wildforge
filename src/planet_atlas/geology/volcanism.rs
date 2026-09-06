//! Place boundary volcanoes, hotspot chains, and intrusive bodies before voxelization.

use super::geometry::{dvec, unit_from_hash};
use super::{
    CratonRecord, DetailedBoundary, IntrusionKind, IntrusionRecord, MagmaChemistry, VolcanoRecord,
    VolcanoSource,
};
use crate::planet::{FACE_BLOCKS, PLANET_RADIUS, geodesic_distance};
use crate::planet_atlas::{AtlasGrid, AtlasPos, GeometryCell, TectonicCell, cell_hash, mix64};
use std::cmp::Ordering as CmpOrdering;

pub(super) fn moved_cell(
    start: AtlasPos,
    side: u16,
    tectonics: &[TectonicCell],
    steps: usize,
    required_continental: Option<bool>,
) -> AtlasPos {
    let own = tectonics[start.index(side)].plate_id;
    let mut at = start;
    for _ in 0..steps {
        let current_distance = tectonics[at.index(side)].boundary_distance;
        let next = at
            .neighbors4(side)
            .into_iter()
            .filter(|candidate| {
                let cell = tectonics[candidate.index(side)];
                cell.plate_id == own
                    && required_continental
                        .is_none_or(|continental| (cell.continental_crust >= 32_768) == continental)
            })
            .max_by_key(|candidate| tectonics[candidate.index(side)].boundary_distance);
        if let Some(next) = next
            && tectonics[next.index(side)].boundary_distance >= current_distance
        {
            at = next;
        }
    }
    at
}

pub(super) fn volcano_record(
    seed: u32,
    id: u32,
    site: AtlasPos,
    source: VolcanoSource,
    plate_id: u16,
) -> VolcanoRecord {
    let hash = cell_hash(seed, site, 0x0065_6469_6669_6365);
    let chemistry = match source {
        VolcanoSource::ContinentalArc => MagmaChemistry::Andesitic,
        VolcanoSource::IslandArc
        | VolcanoSource::Rift
        | VolcanoSource::OceanRidge
        | VolcanoSource::Hotspot => MagmaChemistry::Basaltic,
    };
    let (radius, height) = match source {
        VolcanoSource::ContinentalArc => (90 + (hash % 56) as u16, 38 + ((hash >> 8) % 31) as u16),
        VolcanoSource::IslandArc => (82 + (hash % 54) as u16, 72 + ((hash >> 8) % 48) as u16),
        VolcanoSource::Rift => (62 + (hash % 45) as u16, 20 + ((hash >> 8) % 25) as u16),
        VolcanoSource::OceanRidge => (72 + (hash % 54) as u16, 12 + ((hash >> 8) % 18) as u16),
        VolcanoSource::Hotspot => unreachable!("hotspot chains have age-specific records"),
    };
    let chamber_radius = 14 + ((hash >> 24) % 22) as u16;
    VolcanoRecord {
        id,
        pos: site,
        source,
        plate_id,
        chain_id: 0,
        age_myr: (hash % 12) as u16,
        edifice_radius_blocks: radius,
        edifice_height_blocks: height,
        crater_radius_blocks: (radius / 7).max(6),
        crater_depth_blocks: (height / 4).max(4),
        chamber_depth_blocks: 18 + ((hash >> 16) % 30) as u16,
        chamber_radius_blocks: chamber_radius,
        chamber_volume_blocks: chamber_volume(chamber_radius),
        chemistry,
        hydrothermal_radius_blocks: radius.saturating_mul(2),
        erosion: (hash % 12_000) as u16,
    }
}

pub(super) fn chamber_volume(radius: u16) -> u32 {
    // The chamber materializer uses a vertically compressed sphere with a
    // 0.72 height ratio.  Persist the corresponding finite voxel envelope,
    // rather than deriving magma volume from the unrelated surface edifice.
    ((4.0 / 3.0) * std::f64::consts::PI * f64::from(radius).powi(3) * 0.72).ceil() as u32
}

pub(super) fn geology_sites(
    seed: u32,
    side: u16,
    geometry: &AtlasGrid<GeometryCell>,
    tectonics: &[TectonicCell],
    cratons: &[CratonRecord],
) -> (Vec<VolcanoRecord>, Vec<IntrusionRecord>) {
    let mut volcanoes = Vec::new();
    let mut accepted = Vec::<AtlasPos>::new();
    for index in 0..geometry.len() {
        let cell = tectonics[index];
        let pos = AtlasPos::from_index(index, side).expect("geology index");
        if cell.boundary_distance != 0 || !cell.boundary_detail.is_volcanic() {
            continue;
        }
        let eligible = match cell.boundary_detail {
            DetailedBoundary::OceanContinentSubduction => cell.continental_crust >= 32_768,
            DetailedBoundary::OceanOceanSubduction => {
                cell.continental_crust < 32_768 && cell.plate_id < cell.neighbor_plate
            }
            DetailedBoundary::ContinentalRift => cell.continental_crust >= 32_768,
            DetailedBoundary::OceanRidge => cell.continental_crust < 32_768,
            _ => false,
        };
        if !eligible {
            continue;
        }
        let stride = match cell.boundary_detail {
            DetailedBoundary::OceanContinentSubduction | DetailedBoundary::OceanOceanSubduction => {
                23
            }
            DetailedBoundary::ContinentalRift => 41,
            _ => 67,
        };
        if !cell_hash(seed, pos, 0x0076_6f6c_6361_6e6f).is_multiple_of(stride) {
            continue;
        }
        let source = match cell.boundary_detail {
            DetailedBoundary::OceanContinentSubduction => VolcanoSource::ContinentalArc,
            DetailedBoundary::OceanOceanSubduction => VolcanoSource::IslandArc,
            DetailedBoundary::ContinentalRift => VolcanoSource::Rift,
            DetailedBoundary::OceanRidge => VolcanoSource::OceanRidge,
            _ => unreachable!("only volcanic boundary classes pass eligibility"),
        };
        let required_continental = match source {
            VolcanoSource::ContinentalArc | VolcanoSource::Rift => Some(true),
            VolcanoSource::IslandArc | VolcanoSource::OceanRidge => Some(false),
            VolcanoSource::Hotspot => None,
        };
        let site = moved_cell(pos, side, tectonics, 2, required_continental);
        if accepted
            .iter()
            .any(|other| geodesic_distance(site.center(side), other.center(side)) < 190.0)
        {
            continue;
        }
        accepted.push(site);
        volcanoes.push(volcano_record(
            seed,
            volcanoes.len() as u32 + 1,
            site,
            source,
            cell.plate_id,
        ));
    }

    for (class, source) in [
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
        if volcanoes.iter().any(|volcano| volcano.source == source) {
            continue;
        }
        let candidate = tectonics
            .iter()
            .enumerate()
            .filter(|(_, cell)| {
                cell.boundary_distance == 0
                    && cell.boundary_detail == class
                    && (class != DetailedBoundary::OceanContinentSubduction
                        || cell.continental_crust >= 32_768)
                    && (class != DetailedBoundary::OceanOceanSubduction
                        || (cell.continental_crust < 32_768 && cell.plate_id < cell.neighbor_plate))
                    && (class != DetailedBoundary::ContinentalRift
                        || cell.continental_crust >= 32_768)
                    && (class != DetailedBoundary::OceanRidge || cell.continental_crust < 32_768)
            })
            .min_by_key(|(index, _)| {
                let pos = AtlasPos::from_index(*index, side).unwrap();
                cell_hash(seed, pos, 0x6172_635f_6775_6172)
            });
        if let Some((index, cell)) = candidate {
            let boundary = AtlasPos::from_index(index, side).unwrap();
            let required_continental = match source {
                VolcanoSource::ContinentalArc | VolcanoSource::Rift => Some(true),
                VolcanoSource::IslandArc | VolcanoSource::OceanRidge => Some(false),
                VolcanoSource::Hotspot => None,
            };
            let site = moved_cell(boundary, side, tectonics, 2, required_continental);
            volcanoes.push(volcano_record(
                seed,
                volcanoes.len() as u32 + 1,
                site,
                source,
                cell.plate_id,
            ));
        }
    }

    // Hotspots are independent of boundaries.  A present-day source and four
    // progressively older edifices are advected along the carrying plate.
    let hotspot_count = 4 + (mix64(u64::from(seed) ^ 0x0068_6f74_7370_6f74) % 3) as usize;
    let interior_cells = (320 / (FACE_BLOCKS / side)).max(2);
    let mut candidates: Vec<_> = (0..geometry.len())
        .filter(|index| tectonics[*index].boundary_distance > interior_cells)
        .map(|index| {
            let pos = AtlasPos::from_index(index, side).unwrap();
            (cell_hash(seed, pos, 0x0068_6f74_7370_6f74), pos)
        })
        .collect();
    candidates.sort_unstable_by_key(|candidate| candidate.0);
    let mut hotspot_roots = Vec::new();
    for (_, root) in candidates {
        if hotspot_roots.iter().all(|other| {
            geodesic_distance(root.center(side), AtlasPos::center(*other, side)) > 1_300.0
        }) {
            hotspot_roots.push(root);
            if hotspot_roots.len() == hotspot_count {
                break;
            }
        }
    }
    for (chain, root) in hotspot_roots.into_iter().enumerate() {
        let plate = tectonics[root.index(side)].plate_id;
        let strike = {
            let point = dvec(geometry.get(root).unwrap().unit_direction);
            let pole = unit_from_hash(seed, 0x6873_706f_6c65 ^ u64::from(plate));
            pole.cross(point).normalize_or_zero()
        };
        let root_unit = dvec(geometry.get(root).unwrap().unit_direction);
        for age_step in 0..5 {
            let angle = age_step as f64 * 170.0 / PLANET_RADIUS;
            let target_unit = (root_unit * angle.cos() + strike * angle.sin()).normalize();
            let site = geometry
                .iter()
                .filter(|(pos, _)| tectonics[pos.index(side)].plate_id == plate)
                .max_by(|(_, a), (_, b)| {
                    dvec(a.unit_direction)
                        .dot(target_unit)
                        .partial_cmp(&dvec(b.unit_direction).dot(target_unit))
                        .unwrap_or(CmpOrdering::Equal)
                })
                .map(|(pos, _)| pos)
                .expect("atlas is nonempty");
            let hash = cell_hash(seed, site, 0x686f_7463_6861_696e ^ chain as u64);
            let erosion = age_step as u16 * 11_000;
            volcanoes.push(VolcanoRecord {
                id: volcanoes.len() as u32 + 1,
                pos: site,
                source: VolcanoSource::Hotspot,
                plate_id: plate,
                chain_id: chain as u16 + 1,
                age_myr: age_step as u16 * 9,
                edifice_radius_blocks: 72 + (hash % 38) as u16,
                edifice_height_blocks: 36u16.saturating_sub(age_step as u16 * 5),
                crater_radius_blocks: 8,
                crater_depth_blocks: 5,
                chamber_depth_blocks: 28,
                chamber_radius_blocks: 22,
                chamber_volume_blocks: 80_000,
                chemistry: MagmaChemistry::Basaltic,
                hydrothermal_radius_blocks: 150,
                erosion,
            });
        }
    }

    let mut intrusions = Vec::new();
    for volcano in volcanoes.iter().step_by(3) {
        let hash = cell_hash(seed, volcano.pos, 0x696e_7472_7573_696f);
        let kind = if hash.is_multiple_of(31) {
            IntrusionKind::Carbonatite
        } else if matches!(
            volcano.source,
            VolcanoSource::Rift | VolcanoSource::OceanRidge
        ) {
            IntrusionKind::DikeSwarm
        } else {
            IntrusionKind::Pluton
        };
        intrusions.push(IntrusionRecord {
            id: intrusions.len() as u32 + 1,
            pos: volcano.pos,
            kind,
            plate_id: volcano.plate_id,
            radius_blocks: 70 + (hash % 90) as u16,
            top_depth_blocks: 8 + ((hash >> 9) % 28) as u16,
            bottom_depth_blocks: 110 + ((hash >> 17) % 80) as u16,
            volume_blocks: 180_000 + hash % 900_000,
            contact_radius_blocks: 180 + ((hash >> 24) % 120) as u16,
            chemistry: if kind == IntrusionKind::Carbonatite {
                MagmaChemistry::Carbonatitic
            } else {
                volcano.chemistry
            },
            age_myr: volcano.age_myr.saturating_add(4),
        });
    }
    for craton in cratons {
        let center = dvec(craton.center_unit);
        let pos = geometry
            .iter()
            .max_by(|(_, a), (_, b)| {
                dvec(a.unit_direction)
                    .dot(center)
                    .partial_cmp(&dvec(b.unit_direction).dot(center))
                    .unwrap_or(CmpOrdering::Equal)
            })
            .map(|(pos, _)| pos)
            .unwrap();
        let hash = cell_hash(seed, pos, 0x6261_7468_6f6c_6974);
        intrusions.push(IntrusionRecord {
            id: intrusions.len() as u32 + 1,
            pos,
            kind: IntrusionKind::Batholith,
            plate_id: tectonics[pos.index(side)].plate_id,
            radius_blocks: 180 + (hash % 150) as u16,
            top_depth_blocks: 18,
            bottom_depth_blocks: 190,
            volume_blocks: 2_000_000 + hash % 4_000_000,
            contact_radius_blocks: 360,
            chemistry: MagmaChemistry::Rhyolitic,
            age_myr: craton.interior_age_myr.saturating_sub(600),
        });
    }
    (volcanoes, intrusions)
}
