//! Read-only surface water and hydrological name queries.

use super::{HYDRO_FLOODPLAIN, HYDRO_RIVER, WaterBodyKind};
use crate::chunk::SEA_LEVEL;
use crate::planet::{FACE_BLOCKS, PLANET_RADIUS, SurfacePoint, SurfacePos, surface_to_unit};
use crate::planet_atlas::{AtlasPos, PlanetAtlas, mix64};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtlasHydrologySample {
    pub water_body: WaterBodyKind,
    pub near_channel: bool,
    pub channel_distance_blocks: f32,
    pub channel_width_blocks: f32,
    pub channel_depth_blocks: f32,
    pub channel_bed_elevation: f32,
    pub water_surface_elevation: Option<f32>,
    pub discharge: f32,
    pub sediment_energy: f32,
    pub salinity: u8,
    pub flags: u16,
    pub river_id: u32,
    pub lake_basin_id: u32,
    pub ocean_basin_id: u16,
}

impl Default for AtlasHydrologySample {
    fn default() -> Self {
        Self {
            water_body: WaterBodyKind::Land,
            near_channel: false,
            channel_distance_blocks: f32::INFINITY,
            channel_width_blocks: 0.0,
            channel_depth_blocks: 0.0,
            channel_bed_elevation: SEA_LEVEL as f32,
            water_surface_elevation: None,
            discharge: 0.0,
            sediment_energy: 0.0,
            salinity: 0,
            flags: 0,
            river_id: 0,
            lake_basin_id: 0,
            ocean_basin_id: 0,
        }
    }
}

fn segment_distance(
    point: SurfacePoint,
    a: SurfacePoint,
    b: SurfacePoint,
    lateral_offset_blocks: f32,
) -> (f32, f32) {
    let q = surface_to_unit(point);
    let a = surface_to_unit(a);
    let b = surface_to_unit(b);
    let chord = b - a;
    let length_squared = chord.length_squared();
    let t = if length_squared <= f64::EPSILON {
        0.0
    } else {
        ((q - a).dot(chord) / length_squared).clamp(0.0, 1.0)
    };
    let nearest = (a + chord * t).normalize_or_zero();
    // Floodplain reaches bow away from the atlas chord but return exactly to
    // both declared endpoints. This gives sub-atlas meanders without moving a
    // confluence, mouth, lake outlet, chunk seam, or cube-face crossing.
    let tangent = (chord - nearest * chord.dot(nearest)).normalize_or_zero();
    let lateral = nearest.cross(tangent).normalize_or_zero();
    let bend = f64::from(lateral_offset_blocks) * (std::f64::consts::PI * t).sin() / PLANET_RADIUS;
    let curved = (nearest * bend.cos() + lateral * bend.sin()).normalize_or_zero();
    let angle = q.dot(curved).clamp(-1.0, 1.0).acos();
    ((angle * PLANET_RADIUS) as f32, t as f32)
}

impl PlanetAtlas {
    /// Exact generator-facing surface-water constraint.  The query considers
    /// nearby atlas edges in planet space, so one river centerline is shared
    /// by both chunks and both cube-face charts at a seam.
    pub fn hydrology_sample(&self, point: SurfacePoint) -> AtlasHydrologySample {
        let home = AtlasPos::from_surface(
            SurfacePos::new(
                point.face,
                point.u.floor().clamp(0.0, f64::from(FACE_BLOCKS - 1)) as u16,
                point.v.floor().clamp(0.0, f64::from(FACE_BLOCKS - 1)) as u16,
            )
            .expect("clamped hydrology query"),
            self.side(),
        );
        let home_cell = self
            .genesis
            .hydrology
            .get(home)
            .expect("validated hydrology query");
        if matches!(
            home_cell.water_body,
            WaterBodyKind::Ocean | WaterBodyKind::Lake | WaterBodyKind::Playa
        ) {
            let depth = f32::from(home_cell.channel_depth_centiblocks) / 100.0;
            return AtlasHydrologySample {
                water_body: home_cell.water_body,
                near_channel: true,
                channel_distance_blocks: 0.0,
                channel_width_blocks: f32::from(self.cell_blocks()),
                channel_depth_blocks: depth,
                channel_bed_elevation: home_cell.channel_bed_elevation,
                water_surface_elevation: (home_cell.baseline_water_units > 0)
                    .then_some(home_cell.water_surface_elevation),
                discharge: home_cell.mean_discharge,
                sediment_energy: f32::from(home_cell.sediment_energy) / 65_535.0,
                salinity: home_cell.salinity,
                flags: home_cell.flags,
                river_id: home_cell.river_id,
                lake_basin_id: home_cell.lake_basin_id,
                ocean_basin_id: home_cell.ocean_basin_id,
            };
        }
        let mut best = AtlasHydrologySample {
            flags: home_cell.flags,
            water_body: home_cell.water_body,
            ..AtlasHydrologySample::default()
        };
        let mut best_score = f32::INFINITY;
        for candidate in self.bounded_stencil(home, 2, 32) {
            let cell = self
                .genesis
                .hydrology
                .get(candidate)
                .expect("hydrology stencil");
            if cell.flags & HYDRO_RIVER == 0 || cell.drainage_receiver == u32::MAX {
                continue;
            }
            let Some(receiver) = AtlasPos::from_index(cell.drainage_receiver as usize, self.side())
            else {
                continue;
            };
            let width = f32::from(cell.channel_width_centiblocks) / 100.0;
            let segment_key = (candidate.index(self.side()) as u64).rotate_left(23)
                ^ receiver.index(self.side()) as u64;
            let direction = if mix64(segment_key) & 1 == 0 {
                -1.0
            } else {
                1.0
            };
            let meander = if cell.flags & HYDRO_FLOODPLAIN != 0 {
                direction * (width * 1.4).clamp(1.5, 24.0)
            } else {
                0.0
            };
            let (distance, _) = segment_distance(
                point,
                candidate.center(self.side()),
                receiver.center(self.side()),
                meander,
            );
            let score = distance / width.max(1.0);
            if score > 2.2
                || (score > best_score)
                || (score == best_score && cell.mean_discharge <= best.discharge)
            {
                continue;
            }
            best_score = score;
            let inside = distance <= width * 0.5;
            best = AtlasHydrologySample {
                water_body: if inside {
                    cell.water_body
                } else {
                    WaterBodyKind::Land
                },
                near_channel: true,
                channel_distance_blocks: distance,
                channel_width_blocks: width,
                channel_depth_blocks: f32::from(cell.channel_depth_centiblocks) / 100.0,
                channel_bed_elevation: cell.channel_bed_elevation,
                water_surface_elevation: (inside && cell.baseline_water_units > 0)
                    .then_some(cell.water_surface_elevation),
                discharge: cell.mean_discharge,
                sediment_energy: f32::from(cell.sediment_energy) / 65_535.0,
                salinity: cell.salinity,
                flags: cell.flags,
                river_id: cell.river_id,
                lake_basin_id: cell.lake_basin_id,
                ocean_basin_id: cell.ocean_basin_id,
            };
        }
        best
    }

    pub fn hydrological_name_at(&self, surface: SurfacePos) -> Option<&str> {
        let cell = self.genesis.hydrology.get(self.atlas_pos(surface))?;
        if cell.river_id != 0 {
            return self
                .hydrology
                .rivers
                .iter()
                .find(|river| river.id == cell.river_id)
                .map(|river| river.name.as_str());
        }
        if cell.lake_basin_id != 0 {
            return self
                .hydrology
                .lakes
                .iter()
                .find(|lake| lake.id == cell.lake_basin_id)
                .map(|lake| lake.name.as_str());
        }
        if cell.ocean_basin_id != 0 {
            return self
                .hydrology
                .oceans
                .iter()
                .find(|ocean| ocean.id == cell.ocean_basin_id)
                .map(|ocean| ocean.name.as_str());
        }
        self.hydrology
            .watersheds
            .iter()
            .find(|watershed| watershed.id == cell.watershed_id)
            .map(|watershed| watershed.name.as_str())
    }
}
