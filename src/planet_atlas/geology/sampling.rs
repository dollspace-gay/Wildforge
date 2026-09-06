//! Read geological boundaries, host validity, and finite site envelopes.

use super::deposits::host_allows;
use super::geometry::dvec;
use super::plates::classify_pair;
use super::{
    AtlasGeologySample, BedrockFamily, BoundaryEdge, DepositRecord, IntrusionRecord, MineralKind,
    VolcanoRecord,
};
use crate::chunk::{CHUNK_X, CHUNK_Z, ChunkPos, SEA_LEVEL};
use crate::planet::{FACE_BLOCKS, SurfacePoint, SurfacePos, geodesic_distance, local_frame};
use crate::planet_atlas::{AtlasPos, HYDRO_DELTA, HYDRO_FLOODPLAIN, PlanetAtlas};
use glam::{DVec3, Vec2};
use std::cmp::Ordering as CmpOrdering;

impl PlanetAtlas {
    pub fn boundary_between(&self, a: AtlasPos, b: AtlasPos) -> Option<BoundaryEdge> {
        if !a.neighbors4(self.side()).contains(&b) {
            return None;
        }
        let a_cell = *self.genesis.tectonics.get(a)?;
        let b_cell = *self.genesis.tectonics.get(b)?;
        if a_cell.plate_id == b_cell.plate_id {
            return None;
        }
        let a_point = dvec(self.genesis.geometry.get(a)?.unit_direction);
        let b_point = dvec(self.genesis.geometry.get(b)?.unit_direction);
        let point = (a_point + b_point).normalize();
        let (first, second, first_continental, second_continental) =
            if a_cell.plate_id < b_cell.plate_id {
                (
                    a_cell.plate_id,
                    b_cell.plate_id,
                    a_cell.continental_crust >= 32_768,
                    b_cell.continental_crust >= 32_768,
                )
            } else {
                (
                    b_cell.plate_id,
                    a_cell.plate_id,
                    b_cell.continental_crust >= 32_768,
                    a_cell.continental_crust >= 32_768,
                )
            };
        let (detail, strength, _) = classify_pair(
            first,
            second,
            first_continental,
            second_continental,
            point,
            &self.geology.plates,
        );
        Some(BoundaryEdge {
            a,
            b,
            detail,
            strength,
        })
    }

    pub fn deposit_host_is_valid(&self, site: &DepositRecord) -> bool {
        let Some(cell) = self.genesis.tectonics.get(site.pos) else {
            return false;
        };
        let Some(geometry) = self.genesis.geometry.get(site.pos) else {
            return false;
        };
        if site.host != BedrockFamily::from_id(cell.bedrock_family) {
            return false;
        }
        let routed_placer = site.radius_blocks == 72
            && site.depth_min == SEA_LEVEL as u16
            && site.max_blocks_per_chunk == 2
            && site.eligible_chunk_upper_bound == 32
            && matches!(site.mineral, MineralKind::Gold | MineralKind::RareEarth);
        if routed_placer {
            let source_exists = self.geology.deposits.iter().any(|source| {
                source.id == site.source_body_id
                    && source.id != site.id
                    && source.mineral == site.mineral
            });
            let depositional = self
                .genesis
                .hydrology
                .get(site.pos)
                .is_some_and(|hydrology| hydrology.flags & (HYDRO_FLOODPLAIN | HYDRO_DELTA) != 0);
            return source_exists && depositional;
        }
        host_allows(site.mineral, *cell, geometry.latitude_radians)
    }

    pub fn geology_sample(&self, point: SurfacePoint) -> AtlasGeologySample {
        let surface = SurfacePos::new(
            point.face,
            point.u.floor().clamp(0.0, f64::from(FACE_BLOCKS - 1)) as u16,
            point.v.floor().clamp(0.0, f64::from(FACE_BLOCKS - 1)) as u16,
        )
        .expect("clamped geology sample is canonical");
        let pos = self.atlas_pos(surface);
        let cell = *self
            .genesis
            .tectonics
            .get(pos)
            .expect("validated geology query");
        let terrain = *self
            .genesis
            .terrain
            .get(pos)
            .expect("validated geology query");
        let global = DVec3::new(
            f64::from(cell.boundary_strike[0]) / 32_767.0,
            f64::from(cell.boundary_strike[1]) / 32_767.0,
            f64::from(cell.boundary_strike[2]) / 32_767.0,
        )
        .normalize_or_zero();
        let frame = local_frame(point);
        let strike = Vec2::new(
            global.dot(frame.east) as f32,
            global.dot(frame.north) as f32,
        )
        .normalize_or_zero();
        AtlasGeologySample {
            plate_id: cell.plate_id,
            neighbor_plate: cell.neighbor_plate,
            boundary: cell.boundary_detail,
            boundary_distance_blocks: f32::from(cell.boundary_distance)
                * f32::from(self.cell_blocks()),
            boundary_strength: cell.boundary_strength,
            boundary_strike: strike,
            continental_fraction: f32::from(cell.continental_crust) / 65_535.0,
            craton_id: cell.craton_id,
            oceanic_age_myr: cell.oceanic_age,
            bedrock: BedrockFamily::from_id(cell.bedrock_family),
            geological_province: cell.geological_province,
            stratigraphic_stack: cell.stratigraphic_stack,
            metamorphic_grade: cell.metamorphic_grade,
            fault_intensity: cell.fault_intensity,
            volcanic_history: cell.volcanic_history,
            basin: cell.sediment_basin,
            landmass_id: terrain.landmass_id,
        }
    }

    pub fn province_name(&self, id: u16) -> Option<&str> {
        self.geology
            .provinces
            .iter()
            .find(|province| province.id == id)
            .map(|province| province.name.as_str())
    }

    pub fn exact_volcanic_relief(&self, point: SurfacePoint) -> f32 {
        self.geology.volcanoes.iter().fold(0.0, |sum, volcano| {
            let distance = geodesic_distance(point, volcano.pos.center(self.side())) as f32;
            let radius = f32::from(volcano.edifice_radius_blocks);
            if distance >= radius {
                return sum;
            }
            let t = 1.0 - distance / radius;
            let erosion = 1.0 - f32::from(volcano.erosion) / 65_535.0;
            let mut relief = f32::from(volcano.edifice_height_blocks) * t.powf(1.45) * erosion;
            let crater = f32::from(volcano.crater_radius_blocks);
            if distance < crater {
                relief -= f32::from(volcano.crater_depth_blocks) * (1.0 - distance / crater);
            }
            sum + relief
        })
    }

    pub fn sampled_volcanic_relief(&self, point: SurfacePoint) -> f32 {
        self.sample_scalar(point, |pos| {
            self.genesis.terrain.get(pos).unwrap().volcanic_contribution
        })
    }

    pub fn intrusion_margin(&self, point: SurfacePoint, y: f32) -> f32 {
        self.geology
            .intrusions
            .iter()
            .fold(-1.0f32, |best, intrusion| {
                let horizontal = geodesic_distance(point, intrusion.pos.center(self.side())) as f32;
                let depth = (self.terrain_sample(point).eroded_elevation - y).max(0.0);
                if depth < f32::from(intrusion.top_depth_blocks)
                    || depth > f32::from(intrusion.bottom_depth_blocks)
                {
                    return best;
                }
                best.max(1.0 - horizontal / f32::from(intrusion.radius_blocks))
            })
    }

    pub fn magma_at(&self, point: SurfacePoint, y: f32) -> bool {
        self.magma_interval(point)
            .is_some_and(|(minimum, maximum)| y >= minimum && y <= maximum)
    }

    pub fn magma_interval(&self, point: SurfacePoint) -> Option<(f32, f32)> {
        let surface = self.terrain_sample(point).eroded_elevation;
        self.geology
            .volcanoes
            .iter()
            .filter_map(|volcano| {
                let horizontal = geodesic_distance(point, volcano.pos.center(self.side())) as f32;
                let radius = f32::from(volcano.chamber_radius_blocks);
                if horizontal >= radius {
                    return None;
                }
                let center_y = surface - f32::from(volcano.chamber_depth_blocks);
                let vertical_radius =
                    radius * 0.72 * (1.0 - (horizontal / radius).powi(2)).max(0.0).sqrt();
                Some((center_y - vertical_radius, center_y + vertical_radius))
            })
            .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
    }

    pub fn nearest_volcano(&self, point: SurfacePoint, reach: f64) -> Option<&VolcanoRecord> {
        self.geology
            .volcanoes
            .iter()
            .filter_map(|site| {
                let distance = geodesic_distance(point, site.pos.center(self.side()));
                (distance <= reach).then_some((distance, site))
            })
            .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(CmpOrdering::Equal))
            .map(|(_, site)| site)
    }

    pub fn nearest_intrusion(&self, point: SurfacePoint, reach: f64) -> Option<&IntrusionRecord> {
        self.geology
            .intrusions
            .iter()
            .filter_map(|site| {
                let distance = geodesic_distance(point, site.pos.center(self.side()));
                (distance <= reach).then_some((distance, site))
            })
            .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(CmpOrdering::Equal))
            .map(|(_, site)| site)
    }

    pub fn nearest_deposit(
        &self,
        point: SurfacePoint,
        mineral: MineralKind,
        reach: f64,
    ) -> Option<&DepositRecord> {
        self.geology
            .deposits
            .iter()
            .filter(|site| site.mineral == mineral)
            .filter_map(|site| {
                let distance = geodesic_distance(point, site.pos.center(self.side()));
                (distance <= reach).then_some((distance, site))
            })
            .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(CmpOrdering::Equal))
            .map(|(_, site)| site)
    }

    pub fn deposit_allowance(&self, chunk: ChunkPos, mineral: MineralKind) -> u32 {
        let center = SurfacePoint {
            face: chunk.face(),
            u: f64::from(chunk.u()) * CHUNK_X as f64 + CHUNK_X as f64 * 0.5,
            v: f64::from(chunk.v()) * CHUNK_Z as f64 + CHUNK_Z as f64 * 0.5,
        };
        self.geology
            .deposits
            .iter()
            .filter(|site| site.mineral == mineral)
            .filter(|site| {
                geodesic_distance(center, site.pos.center(self.side()))
                    <= f64::from(site.radius_blocks)
            })
            .map(|site| u32::from(site.max_blocks_per_chunk))
            .sum()
    }

    pub fn deposit_center_in_chunk(
        &self,
        chunk: ChunkPos,
        mineral: MineralKind,
    ) -> Option<&DepositRecord> {
        self.geology.deposits.iter().find(|site| {
            site.mineral == mineral
                && ChunkPos::from_surface(
                    SurfacePos::new(
                        site.pos.face,
                        site.pos.u * self.cell_blocks() + self.cell_blocks() / 2,
                        site.pos.v * self.cell_blocks() + self.cell_blocks() / 2,
                    )
                    .expect("atlas deposit center is canonical"),
                ) == chunk
        })
    }
}
