//! Read-only atlas interpolation, local fields, and bounded graph queries.

use crate::chunk::{SEA_LEVEL};
use crate::planet::{Direction4, FACE_BLOCKS, QuarterTurn, SurfacePoint, SurfacePos, canonicalize_surface_point, geodesic_distance};
use crate::planet_atlas::{AtlasCell, AtlasError, AtlasGrid, AtlasPos, BoundaryClass, PlanetAtlas};
use glam::{Vec2, Vec3};
use std::collections::{BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtlasTerrainSample {
    pub base_elevation: f32,
    pub eroded_elevation: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtlasClimateSample {
    pub mean_temperature: f32,
    pub mean_precipitation: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtlasTectonicSample {
    pub boundary: BoundaryClass,
    pub oceanic: bool,
    pub east_neighbor_oceanic: bool,
}

impl PlanetAtlas {
    #[inline]
    pub const fn side(&self) -> u16 {
        self.manifest.atlas_face_side
    }

    #[inline]
    pub const fn cell_blocks(&self) -> u16 {
        self.manifest.atlas_cell_blocks
    }

    pub fn cell(&self, pos: AtlasPos) -> Option<AtlasCell> {
        let geometry = self.genesis.geometry.get(pos)?;
        Some(AtlasCell {
            pos,
            unit_direction: geometry.unit_direction,
            latitude_radians: geometry.latitude_radians,
            physical_area: geometry.physical_area,
        })
    }

    pub fn atlas_pos(&self, surface: SurfacePos) -> AtlasPos {
        AtlasPos::from_surface(surface, self.side())
    }

    /// Generator-facing terrain query. Chunk code depends on this typed
    /// interface rather than atlas storage layout or layer vectors.
    pub fn terrain_sample(&self, point: SurfacePoint) -> AtlasTerrainSample {
        AtlasTerrainSample {
            base_elevation: self.sample_scalar(point, |pos| {
                self.genesis
                    .terrain
                    .get(pos)
                    .expect("validated atlas query is in range")
                    .base_elevation
            }),
            eroded_elevation: self.sample_scalar(point, |pos| {
                self.genesis
                    .terrain
                    .get(pos)
                    .expect("validated atlas query is in range")
                    .eroded_elevation
            }),
        }
    }

    pub fn climate_sample(&self, point: SurfacePoint) -> AtlasClimateSample {
        AtlasClimateSample {
            mean_temperature: self.sample_scalar(point, |pos| {
                self.genesis
                    .climate
                    .get(pos)
                    .expect("validated atlas query is in range")
                    .mean_temperature
            }),
            mean_precipitation: self.sample_scalar(point, |pos| {
                self.genesis
                    .climate
                    .get(pos)
                    .expect("validated atlas query is in range")
                    .mean_precipitation
            }),
        }
    }

    pub fn tectonic_sample(&self, surface: SurfacePos) -> AtlasTectonicSample {
        let pos = self.atlas_pos(surface);
        let east = pos.step(Direction4::East, self.side()).pos;
        AtlasTectonicSample {
            boundary: self
                .genesis
                .tectonics
                .get(pos)
                .expect("validated atlas query is in range")
                .boundary,
            oceanic: self
                .genesis
                .terrain
                .get(pos)
                .is_some_and(|cell| cell.eroded_elevation <= SEA_LEVEL as f32),
            east_neighbor_oceanic: self
                .genesis
                .terrain
                .get(east)
                .is_some_and(|cell| cell.eroded_elevation <= SEA_LEVEL as f32),
        }
    }
}

impl PlanetAtlas {
    pub fn sample_scalar(&self, point: SurfacePoint, value: impl Fn(AtlasPos) -> f32) -> f32 {
        let side = self.side();
        let cell = f64::from(self.cell_blocks());
        let x = point.u / cell - 0.5;
        let z = point.v / cell - 0.5;
        let base_u = x.floor() as i32;
        let base_v = z.floor() as i32;
        let tx = (x - f64::from(base_u)) as f32;
        let tz = (z - f64::from(base_v)) as f32;
        let sample = |du: i32, dv: i32| {
            let center = AtlasPos {
                face: point.face,
                u: base_u.clamp(0, i32::from(side) - 1) as u16,
                v: base_v.clamp(0, i32::from(side) - 1) as u16,
            };
            let source_u = (f64::from(base_u + du) + 0.5) * cell;
            let source_v = (f64::from(base_v + dv) + 0.5) * cell;
            let canonical = canonicalize_surface_point(point.face, source_u, source_v)
                .expect("bilinear stencil crosses at most one face edge");
            let pos = AtlasPos {
                face: canonical.point.face,
                u: (canonical.point.u / cell)
                    .floor()
                    .clamp(0.0, f64::from(side - 1)) as u16,
                v: (canonical.point.v / cell)
                    .floor()
                    .clamp(0.0, f64::from(side - 1)) as u16,
            };
            let _ = center;
            value(pos)
        };
        let a = sample(0, 0);
        let b = sample(1, 0);
        let c = sample(0, 1);
        let d = sample(1, 1);
        let ab = a + (b - a) * tx;
        let cd = c + (d - c) * tx;
        ab + (cd - ab) * tz
    }

    pub fn sample_tangent_vector(
        &self,
        point: SurfacePoint,
        value: impl Fn(AtlasPos) -> [f32; 2],
    ) -> Vec2 {
        let side = self.side();
        let cell = f64::from(self.cell_blocks());
        let x = point.u / cell - 0.5;
        let z = point.v / cell - 0.5;
        let base_u = x.floor() as i32;
        let base_v = z.floor() as i32;
        let tx = (x - f64::from(base_u)) as f32;
        let tz = (z - f64::from(base_v)) as f32;
        let sample = |du: i32, dv: i32| {
            let source_u = (f64::from(base_u + du) + 0.5) * cell;
            let source_v = (f64::from(base_v + dv) + 0.5) * cell;
            let canonical = canonicalize_surface_point(point.face, source_u, source_v)
                .expect("vector stencil crosses at most one face edge");
            let pos = AtlasPos {
                face: canonical.point.face,
                u: (canonical.point.u / cell)
                    .floor()
                    .clamp(0.0, f64::from(side - 1)) as u16,
                v: (canonical.point.v / cell)
                    .floor()
                    .clamp(0.0, f64::from(side - 1)) as u16,
            };
            let vector = value(pos);
            let inverse = QuarterTurn::new(4 - canonical.rotation.turns());
            let rotated = inverse.rotate_vec3(Vec3::new(vector[0], 0.0, vector[1]));
            Vec2::new(rotated.x, rotated.z)
        };
        let a = sample(0, 0);
        let b = sample(1, 0);
        let c = sample(0, 1);
        let d = sample(1, 1);
        let vector = a.lerp(b, tx).lerp(c.lerp(d, tx), tz);
        vector.normalize_or_zero()
    }

    pub fn bounded_stencil(
        &self,
        center: AtlasPos,
        radius: u16,
        max_cells: usize,
    ) -> Vec<AtlasPos> {
        if max_cells == 0 {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        let mut queue = VecDeque::from([(center, 0u16)]);
        while let Some((pos, distance)) = queue.pop_front() {
            if !seen.insert(pos) {
                continue;
            }
            out.push(pos);
            if out.len() >= max_cells || distance >= radius {
                continue;
            }
            for neighbor in pos.neighbors4(self.side()) {
                queue.push_back((neighbor, distance + 1));
            }
        }
        out
    }

    pub fn radius_query(&self, center: SurfacePoint, radius_blocks: f64) -> Vec<AtlasPos> {
        if radius_blocks.is_nan() || radius_blocks < 0.0 {
            return Vec::new();
        }
        let start = AtlasPos::from_surface(
            SurfacePos::new(
                center.face,
                center.u.floor().clamp(0.0, f64::from(FACE_BLOCKS - 1)) as u16,
                center.v.floor().clamp(0.0, f64::from(FACE_BLOCKS - 1)) as u16,
            )
            .expect("clamped radius-query center is canonical"),
            self.side(),
        );
        let radius_cells = if radius_blocks.is_infinite() {
            u16::MAX
        } else {
            ((radius_blocks / f64::from(self.cell_blocks())).ceil() as u16).saturating_add(2)
        };
        let diameter = usize::from(radius_cells)
            .saturating_mul(2)
            .saturating_add(1);
        let max_cells = diameter
            .saturating_mul(diameter)
            .saturating_mul(2)
            .min(self.genesis.geometry.len());
        self.bounded_stencil(start, radius_cells, max_cells)
            .into_iter()
            .filter(|pos| geodesic_distance(center, pos.center(self.side())) <= radius_blocks)
            .collect()
    }

    pub fn connected_components<T>(
        &self,
        grid: &AtlasGrid<T>,
        included: impl Fn(&T) -> bool,
    ) -> Result<(AtlasGrid<u32>, u32), AtlasError> {
        if grid.side() != self.side() {
            return Err(AtlasError::InvalidDimensions {
                side: grid.side(),
                count: grid.len(),
            });
        }
        let mut labels = vec![0u32; grid.len()];
        let mut next_label = 0u32;
        for (pos, value) in grid.iter() {
            if !included(value) || labels[pos.index(self.side())] != 0 {
                continue;
            }
            next_label += 1;
            let mut queue = VecDeque::from([pos]);
            labels[pos.index(self.side())] = next_label;
            while let Some(at) = queue.pop_front() {
                for neighbor in at.neighbors4(self.side()) {
                    let index = neighbor.index(self.side());
                    if labels[index] == 0 && grid.get(neighbor).is_some_and(&included) {
                        labels[index] = next_label;
                        queue.push_back(neighbor);
                    }
                }
            }
        }
        Ok((AtlasGrid::from_values(self.side(), labels)?, next_label))
    }

    pub fn downstream(&self, start: AtlasPos, max_steps: usize) -> Vec<AtlasPos> {
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        let mut at = start;
        while out.len() < max_steps && seen.insert(at) {
            out.push(at);
            let receiver = self
                .genesis
                .hydrology
                .get(at)
                .map_or(u32::MAX, |cell| cell.drainage_receiver);
            let Some(next) = (receiver != u32::MAX)
                .then(|| AtlasPos::from_index(receiver as usize, self.side()))
                .flatten()
            else {
                break;
            };
            at = next;
        }
        out
    }

    pub fn upstream(&self, start: AtlasPos, max_cells: usize) -> Vec<AtlasPos> {
        let mut out = Vec::new();
        let mut queue = VecDeque::from([start]);
        let mut seen = BTreeSet::new();
        while let Some(at) = queue.pop_front() {
            if !seen.insert(at) || out.len() >= max_cells {
                continue;
            }
            out.push(at);
            let wanted = at.index(self.side()) as u32;
            for (candidate, cell) in self.genesis.hydrology.iter() {
                if cell.drainage_receiver == wanted {
                    queue.push_back(candidate);
                }
            }
        }
        out
    }
}
