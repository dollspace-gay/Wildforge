//! Canonical atlas addressing, dense grids, and ordered cell construction.

use crate::planet::{
    Direction4, FACE_BLOCKS, Face, PLANET_RADIUS, QuarterTurn, SURFACE_FACES, SurfacePoint,
    SurfacePos, canonicalize_surface_point, surface_to_unit,
};
use crate::planet_atlas::{AtlasError, GenerationMode};
use glam::DVec3;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct AtlasPos {
    pub face: Face,
    pub u: u16,
    pub v: u16,
}

impl AtlasPos {
    pub fn new(face: Face, u: u16, v: u16, side: u16) -> Result<Self, AtlasError> {
        if side == 0 || !FACE_BLOCKS.is_multiple_of(side) {
            return Err(AtlasError::InvalidDimensions {
                side,
                count: usize::from(side) * usize::from(side) * SURFACE_FACES,
            });
        }
        if u < side && v < side {
            Ok(Self { face, u, v })
        } else {
            Err(AtlasError::InvalidPosition { face, u, v, side })
        }
    }

    #[inline]
    pub fn index(self, side: u16) -> usize {
        self.face.index() * usize::from(side) * usize::from(side)
            + usize::from(self.v) * usize::from(side)
            + usize::from(self.u)
    }

    pub fn from_index(index: usize, side: u16) -> Option<Self> {
        let face_len = usize::from(side) * usize::from(side);
        let face = Face::from_u8((index / face_len).try_into().ok()?)?;
        let local = index % face_len;
        Some(Self {
            face,
            u: (local % usize::from(side)) as u16,
            v: (local / usize::from(side)) as u16,
        })
    }

    pub fn from_surface(pos: SurfacePos, side: u16) -> Self {
        let cell = FACE_BLOCKS / side;
        Self {
            face: pos.face(),
            u: pos.u() / cell,
            v: pos.v() / cell,
        }
    }

    pub fn center(self, side: u16) -> SurfacePoint {
        let cell = f64::from(FACE_BLOCKS / side);
        SurfacePoint {
            face: self.face,
            u: (f64::from(self.u) + 0.5) * cell,
            v: (f64::from(self.v) + 0.5) * cell,
        }
    }

    pub fn step(self, direction: Direction4, side: u16) -> AtlasStep {
        let (du, dv) = match direction {
            Direction4::East => (1, 0),
            Direction4::North => (0, 1),
            Direction4::West => (-1, 0),
            Direction4::South => (0, -1),
        };
        self.offset_oriented(du, dv, side, direction)
    }

    fn offset_oriented(self, du: i32, dv: i32, side: u16, direction: Direction4) -> AtlasStep {
        let cell = f64::from(FACE_BLOCKS / side);
        let target_u = (f64::from(self.u) + 0.5 + f64::from(du)) * cell;
        let target_v = (f64::from(self.v) + 0.5 + f64::from(dv)) * cell;
        let canonical = canonicalize_surface_point(self.face, target_u, target_v)
            .expect("an atlas neighbor crosses at most one face edge");
        let u = (canonical.point.u / cell)
            .floor()
            .clamp(0.0, f64::from(side - 1)) as u16;
        let v = (canonical.point.v / cell)
            .floor()
            .clamp(0.0, f64::from(side - 1)) as u16;
        AtlasStep {
            pos: Self {
                face: canonical.point.face,
                u,
                v,
            },
            direction: canonical.rotation.map_direction(direction),
            rotation: canonical.rotation,
        }
    }

    pub fn neighbors4(self, side: u16) -> [Self; 4] {
        [
            self.step(Direction4::East, side).pos,
            self.step(Direction4::North, side).pos,
            self.step(Direction4::West, side).pos,
            self.step(Direction4::South, side).pos,
        ]
    }

    pub fn neighbors8(self, side: u16) -> [Self; 8] {
        // At each cube vertex two diagonal walks canonically reach the same
        // cell. That valence singularity has seven unique neighbors spanning
        // three charts; graph algorithms must deduplicate this fixed array.
        let cardinals = self.neighbors4(side);
        let diagonal = |u_direction: Direction4, v_direction: Direction4| {
            let first = self.step(u_direction, side);
            first
                .pos
                .step(first.rotation.map_direction(v_direction), side)
                .pos
        };
        [
            cardinals[0],
            cardinals[1],
            cardinals[2],
            cardinals[3],
            diagonal(Direction4::East, Direction4::North),
            diagonal(Direction4::West, Direction4::North),
            diagonal(Direction4::West, Direction4::South),
            diagonal(Direction4::East, Direction4::South),
        ]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AtlasStep {
    pub pos: AtlasPos,
    pub direction: Direction4,
    pub rotation: QuarterTurn,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtlasCell {
    pub pos: AtlasPos,
    pub unit_direction: [f32; 3],
    pub latitude_radians: f32,
    pub physical_area: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AtlasGrid<T> {
    side: u16,
    values: Vec<T>,
}

impl<T> AtlasGrid<T> {
    pub fn from_values(side: u16, values: Vec<T>) -> Result<Self, AtlasError> {
        let expected = atlas_count(side)?;
        if values.len() != expected {
            return Err(AtlasError::InvalidDimensions {
                side,
                count: values.len(),
            });
        }
        Ok(Self { side, values })
    }

    pub fn filled(side: u16, value: T) -> Result<Self, AtlasError>
    where
        T: Clone,
    {
        Self::from_values(side, vec![value; atlas_count(side)?])
    }

    /// Allocate a second grid with this validated shape, without exposing the
    /// backing vector or allowing a caller to change its length.
    pub(super) fn filled_like(&self, value: T) -> Self
    where
        T: Clone,
    {
        Self {
            side: self.side,
            values: vec![value; self.values.len()],
        }
    }

    #[inline]
    pub const fn side(&self) -> u16 {
        self.side
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    #[inline]
    pub fn get(&self, pos: AtlasPos) -> Option<&T> {
        // The Deep (capability E10) has no planetary cells: lookups there
        // miss like any out-of-range cell.
        (!pos.face.is_deep() && pos.u < self.side && pos.v < self.side)
            .then(|| &self.values[pos.index(self.side)])
    }

    #[inline]
    pub fn get_mut(&mut self, pos: AtlasPos) -> Option<&mut T> {
        (!pos.face.is_deep() && pos.u < self.side && pos.v < self.side).then(move || {
            let index = pos.index(self.side);
            &mut self.values[index]
        })
    }

    #[inline]
    pub fn values(&self) -> &[T] {
        &self.values
    }

    #[inline]
    pub fn values_mut(&mut self) -> &mut [T] {
        &mut self.values
    }

    pub fn iter(&self) -> impl Iterator<Item = (AtlasPos, &T)> {
        let side = self.side;
        self.values.iter().enumerate().map(move |(index, value)| {
            (
                AtlasPos::from_index(index, side).expect("grid index is in range"),
                value,
            )
        })
    }
}

pub(in crate::planet_atlas) fn atlas_count(side: u16) -> Result<usize, AtlasError> {
    if side == 0 || !FACE_BLOCKS.is_multiple_of(side) {
        return Err(AtlasError::InvalidDimensions { side, count: 0 });
    }
    Ok(SURFACE_FACES * usize::from(side) * usize::from(side))
}

fn triangle_area(a: DVec3, b: DVec3, c: DVec3) -> f64 {
    let numerator = a.dot(b.cross(c)).abs();
    let denominator = 1.0 + a.dot(b) + b.dot(c) + c.dot(a);
    2.0 * numerator.atan2(denominator) * PLANET_RADIUS * PLANET_RADIUS
}

pub(super) fn cell_area(pos: AtlasPos, side: u16) -> f32 {
    let cell = f64::from(FACE_BLOCKS / side);
    let u0 = f64::from(pos.u) * cell;
    let v0 = f64::from(pos.v) * cell;
    let u1 = u0 + cell;
    let v1 = v0 + cell;
    let at = |u, v| {
        surface_to_unit(SurfacePoint {
            face: pos.face,
            u,
            v,
        })
    };
    let (a, b, c, d) = (at(u0, v0), at(u1, v0), at(u1, v1), at(u0, v1));
    (triangle_area(a, b, c) + triangle_area(a, c, d)) as f32
}

pub(in crate::planet_atlas) fn generate_grid<T, F>(
    side: u16,
    mode: GenerationMode,
    make: F,
) -> Result<AtlasGrid<T>, AtlasError>
where
    T: Send,
    F: Fn(AtlasPos) -> T + Sync,
{
    let one_face = usize::from(side) * usize::from(side);
    let mut values = Vec::with_capacity(atlas_count(side)?);
    match mode {
        GenerationMode::Serial => {
            for face in Face::ALL {
                for v in 0..side {
                    for u in 0..side {
                        values.push(make(AtlasPos { face, u, v }));
                    }
                }
            }
        }
        GenerationMode::Parallel => {
            let faces = std::thread::scope(|scope| {
                let mut workers = Vec::with_capacity(SURFACE_FACES);
                let make = &make;
                for face in Face::ALL {
                    workers.push(scope.spawn(move || {
                        let mut face_values = Vec::with_capacity(one_face);
                        for v in 0..side {
                            for u in 0..side {
                                face_values.push(make(AtlasPos { face, u, v }));
                            }
                        }
                        face_values
                    }));
                }
                workers
                    .into_iter()
                    .map(|worker| worker.join().expect("atlas worker panicked"))
                    .collect::<Vec<_>>()
            });
            for face in faces {
                values.extend(face);
            }
        }
    }
    AtlasGrid::from_values(side, values)
}
