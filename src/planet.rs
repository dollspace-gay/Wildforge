//! Finite cube-sphere topology and coordinate geometry.
//!
//! This is the only module allowed to know how the six surface charts join.
//! Simulation code deals in the validated positions below and crosses chart
//! edges through [`step4`], [`step6`], or [`EntityPos::canonicalized`].

use std::fmt;

use glam::{DVec3, Vec3};
use serde::{Deserialize, Deserializer, Serialize};

use crate::chunk::{CHUNK_X, CHUNK_Y, CHUNK_Z};

pub const FACE_BLOCKS: u16 = 8192;
pub const FACE_CHUNKS: u16 = FACE_BLOCKS / CHUNK_X as u16;
pub const SURFACE_FACES: usize = 6;
pub const PLANET_RADIUS: f64 = FACE_BLOCKS as f64 / std::f64::consts::FRAC_PI_2;
pub const BLOCK_CELL_COUNT: u64 =
    SURFACE_FACES as u64 * FACE_BLOCKS as u64 * FACE_BLOCKS as u64 * CHUNK_Y as u64;

/// Stable serialized order. Do not reorder: face ids occur in saves and on
/// the wire.
#[repr(u8)]
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
pub enum Face {
    #[default]
    PosX = 0,
    NegX = 1,
    PosY = 2,
    NegY = 3,
    PosZ = 4,
    NegZ = 5,
    /// The Deep (capability E10): a seventh wing of the same chunk store
    /// where instanced dungeon zones live. Not part of the planet's surface —
    /// worldgen emits void here, planetary geography has no cells here, and
    /// chunks on this face are never persisted, so every dungeon run starts
    /// fresh.
    Deep = 6,
}

impl Face {
    /// The six faces of the planet cube: everything worldgen, the atlas,
    /// and geography iterate. The Deep is deliberately excluded.
    pub const ALL: [Face; SURFACE_FACES] = [
        Face::PosX,
        Face::NegX,
        Face::PosY,
        Face::NegY,
        Face::PosZ,
        Face::NegZ,
    ];

    #[inline]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Whether this face is the Deep: outside the planetary geography.
    #[inline]
    pub const fn is_deep(self) -> bool {
        matches!(self, Self::Deep)
    }

    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::PosX),
            1 => Some(Self::NegX),
            2 => Some(Self::PosY),
            3 => Some(Self::NegY),
            4 => Some(Self::PosZ),
            5 => Some(Self::NegZ),
            6 => Some(Self::Deep),
            _ => None,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::PosX => "pos_x",
            Self::NegX => "neg_x",
            Self::PosY => "pos_y",
            Self::NegY => "neg_y",
            Self::PosZ => "pos_z",
            Self::NegZ => "neg_z",
            Self::Deep => "deep",
        }
    }

    pub fn from_name(value: &str) -> Option<Self> {
        if value == Self::Deep.name() {
            return Some(Self::Deep);
        }
        Self::ALL.into_iter().find(|face| face.name() == value)
    }
}

impl fmt::Display for Face {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoordError {
    Face(u8),
    Surface { u: u32, v: u32 },
    Height(u32),
    Chunk { u: u32, v: u32 },
    NonFinite,
    TooFarFromCanonical,
}

impl fmt::Display for CoordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Face(face) => write!(f, "invalid planet face {face}"),
            Self::Surface { u, v } => {
                write!(f, "surface coordinate ({u}, {v}) is outside one face")
            }
            Self::Height(y) => write!(f, "block height {y} is outside the world"),
            Self::Chunk { u, v } => {
                write!(f, "chunk coordinate ({u}, {v}) is outside one face")
            }
            Self::NonFinite => f.write_str("entity position contains a non-finite component"),
            Self::TooFarFromCanonical => {
                f.write_str("entity position crosses too many planet faces at once")
            }
        }
    }
}

impl std::error::Error for CoordError {}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct SurfacePos {
    face: Face,
    u: u16,
    v: u16,
}

impl<'de> Deserialize<'de> for SurfacePos {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            face: Face,
            u: u16,
            v: u16,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.face, wire.u, wire.v).map_err(serde::de::Error::custom)
    }
}

impl SurfacePos {
    pub fn new(face: Face, u: u16, v: u16) -> Result<Self, CoordError> {
        if u < FACE_BLOCKS && v < FACE_BLOCKS {
            Ok(Self { face, u, v })
        } else {
            Err(CoordError::Surface {
                u: u.into(),
                v: v.into(),
            })
        }
    }

    pub fn from_centered(face: Face, u: i32, v: i32) -> Result<Self, CoordError> {
        let half = i32::from(FACE_BLOCKS) / 2;
        let absolute_u = u + half;
        let absolute_v = v + half;
        if !(0..i32::from(FACE_BLOCKS)).contains(&absolute_u)
            || !(0..i32::from(FACE_BLOCKS)).contains(&absolute_v)
        {
            return Err(CoordError::Surface {
                u: absolute_u.max(0) as u32,
                v: absolute_v.max(0) as u32,
            });
        }
        Self::new(face, absolute_u as u16, absolute_v as u16)
    }

    /// Canonicalize an integer cell address that may lie just beyond a face.
    ///
    /// Sampling the cell center makes an out-of-range `u` or `v` unambiguous
    /// and preserves the same u-before-v corner rule used by moving entities.
    pub fn canonicalized(face: Face, u: i32, v: i32) -> Result<Self, CoordError> {
        let canonical = EntityPos::uncanonical(face, u as f32 + 0.5, 0.0, v as f32 + 0.5)
            .canonicalized()?
            .pos;
        Self::new(
            canonical.face(),
            canonical.u().floor() as u16,
            canonical.v().floor() as u16,
        )
    }

    #[inline]
    pub const fn centered_u(self) -> i32 {
        self.u as i32 - FACE_BLOCKS as i32 / 2
    }

    #[inline]
    pub const fn centered_v(self) -> i32 {
        self.v as i32 - FACE_BLOCKS as i32 / 2
    }

    #[inline]
    pub const fn face(self) -> Face {
        self.face
    }

    #[inline]
    pub const fn u(self) -> u16 {
        self.u
    }

    #[inline]
    pub const fn v(self) -> u16 {
        self.v
    }

    #[inline]
    pub fn center(self) -> SurfacePoint {
        SurfacePoint {
            face: self.face,
            u: f64::from(self.u) + 0.5,
            v: f64::from(self.v) + 0.5,
        }
    }
}

/// Stable seed/position roll shared by systems that must agree before and
/// after a surface chunk is materialized. Keeping this in the topology layer
/// prevents atlas genesis and lazy world generation from silently developing
/// different ideas of where deterministic features belong.
pub(crate) fn seeded_surface_roll(seed: u32, pos: SurfacePos, salt: u32) -> u32 {
    let mut hash = u32::from(pos.u()).wrapping_mul(0x85eb_ca6b)
        ^ u32::from(pos.v()).wrapping_mul(0xc2b2_ae35)
        ^ (pos.face() as u32).wrapping_mul(0x27d4_eb2d)
        ^ seed.wrapping_mul(0x9e37_79b9)
        ^ salt.wrapping_mul(0x2708_92cd);
    hash ^= hash >> 15;
    hash = hash.wrapping_mul(0x2c1b_3c6d);
    hash ^ (hash >> 12)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfacePoint {
    pub face: Face,
    /// Vertex/continuous coordinate. Unlike [`SurfacePos`], the far boundary
    /// `FACE_BLOCKS` is valid so either face can describe a seam vertex.
    pub u: f64,
    pub v: f64,
}

impl SurfacePoint {
    pub fn new(face: Face, u: f64, v: f64) -> Result<Self, CoordError> {
        if u.is_finite()
            && v.is_finite()
            && (0.0..=f64::from(FACE_BLOCKS)).contains(&u)
            && (0.0..=f64::from(FACE_BLOCKS)).contains(&v)
        {
            Ok(Self { face, u, v })
        } else if !u.is_finite() || !v.is_finite() {
            Err(CoordError::NonFinite)
        } else {
            Err(CoordError::Surface {
                u: u.max(0.0) as u32,
                v: v.max(0.0) as u32,
            })
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanonicalSurfacePoint {
    pub point: SurfacePoint,
    pub rotation: QuarterTurn,
}

/// Canonicalize a continuous chart coordinate for rendering and geometric
/// sampling. Exact far-edge vertices stay on their source chart so both
/// adjacent meshes feed bit-identical cube coordinates to the embedding.
pub fn canonicalize_surface_point(
    face: Face,
    u: f64,
    v: f64,
) -> Result<CanonicalSurfacePoint, CoordError> {
    if let Ok(point) = SurfacePoint::new(face, u, v) {
        return Ok(CanonicalSurfacePoint {
            point,
            rotation: QuarterTurn::IDENTITY,
        });
    }
    let canonical = EntityPos::uncanonical(face, u as f32, 0.0, v as f32).canonicalized()?;
    Ok(CanonicalSurfacePoint {
        point: SurfacePoint::new(
            canonical.pos.face(),
            f64::from(canonical.pos.u()),
            f64::from(canonical.pos.v()),
        )?,
        rotation: canonical.rotation,
    })
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BlockPos {
    surface: SurfacePos,
    y: u8,
}

impl Serialize for BlockPos {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("BlockPos", 4)?;
        state.serialize_field("face", &self.face())?;
        state.serialize_field("u", &self.u())?;
        state.serialize_field("y", &self.y())?;
        state.serialize_field("v", &self.v())?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for BlockPos {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Repr {
            face: Face,
            u: u16,
            y: u8,
            v: u16,
        }

        let repr = Repr::deserialize(deserializer)?;
        Self::new(repr.face, repr.u, repr.y, repr.v).map_err(serde::de::Error::custom)
    }
}

impl BlockPos {
    pub fn new(face: Face, u: u16, y: u8, v: u16) -> Result<Self, CoordError> {
        Ok(Self {
            surface: SurfacePos::new(face, u, v)?,
            y,
        })
    }

    pub fn from_centered(face: Face, u: i32, y: i32, v: i32) -> Result<Self, CoordError> {
        if !(0..CHUNK_Y as i32).contains(&y) {
            return Err(CoordError::Height(y.max(0) as u32));
        }
        Ok(Self {
            surface: SurfacePos::from_centered(face, u, v)?,
            y: y as u8,
        })
    }

    /// Temporary bridge for legacy face-local call sites. The horizontal
    /// coordinates are centered on `PosZ` and canonicalized across edges.
    #[cfg(test)]
    #[doc(hidden)]
    pub fn of_world(u: i32, y: i32, v: i32) -> Option<Self> {
        if !(0..CHUNK_Y as i32).contains(&y) {
            return None;
        }
        let half = i32::from(FACE_BLOCKS) / 2;
        let entity =
            EntityPos::uncanonical(Face::PosZ, (u + half) as f32, y as f32, (v + half) as f32)
                .canonicalized()
                .ok()?
                .pos;
        Self::new(
            entity.face(),
            entity.u().floor() as u16,
            y as u8,
            entity.v().floor() as u16,
        )
        .ok()
    }

    #[inline]
    pub const fn with_y(self, y: u8) -> Self {
        Self {
            surface: self.surface,
            y,
        }
    }

    #[inline]
    pub const fn surface(self) -> SurfacePos {
        self.surface
    }

    #[inline]
    pub const fn face(self) -> Face {
        self.surface.face
    }

    #[inline]
    pub const fn u(self) -> u16 {
        self.surface.u
    }

    #[inline]
    pub const fn y(self) -> u8 {
        self.y
    }

    #[inline]
    pub const fn v(self) -> u16 {
        self.surface.v
    }

    #[inline]
    pub const fn chunk(self) -> ChunkPos {
        ChunkPos {
            face: self.surface.face,
            u: self.surface.u / CHUNK_X as u16,
            v: self.surface.v / CHUNK_Z as u16,
        }
    }

    #[inline]
    pub const fn local(self) -> (usize, usize, usize) {
        (
            self.surface.u as usize % CHUNK_X,
            self.y as usize,
            self.surface.v as usize % CHUNK_Z,
        )
    }

    /// Face-local centered coordinates used by legacy machine arithmetic
    /// while its storage key remains the canonical planetary address.
    #[cfg(test)]
    #[inline]
    pub const fn centered(self) -> (i32, i32, i32) {
        (
            self.surface.centered_u(),
            self.y as i32,
            self.surface.centered_v(),
        )
    }

    pub fn offset(self, du: i32, dy: i32, dv: i32) -> Option<Self> {
        let y = i32::from(self.y) + dy;
        if !(0..CHUNK_Y as i32).contains(&y) {
            return None;
        }
        let surface = SurfacePos::canonicalized(
            self.face(),
            i32::from(self.u()) + du,
            i32::from(self.v()) + dv,
        )
        .ok()?;
        Self::new(surface.face(), surface.u(), y as u8, surface.v()).ok()
    }

    /// Center of this cell as a continuous entity coordinate.
    #[inline]
    pub fn entity_center(self) -> EntityPos {
        self.entity_at_height(0.5)
    }

    /// Horizontal cell center at a fractional height above its floor.
    #[inline]
    pub fn entity_at_height(self, height: f32) -> EntityPos {
        EntityPos::new(
            self.face(),
            f32::from(self.u()) + 0.5,
            f32::from(self.y()) + height,
            f32::from(self.v()) + 0.5,
        )
        .expect("a block center is always a canonical entity position")
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ChunkPos {
    face: Face,
    u: u16,
    v: u16,
}

impl<'de> Deserialize<'de> for ChunkPos {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            face: Face,
            u: u16,
            v: u16,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.face, wire.u, wire.v).map_err(serde::de::Error::custom)
    }
}

impl ChunkPos {
    pub fn new(face: Face, u: u16, v: u16) -> Result<Self, CoordError> {
        if u < FACE_CHUNKS && v < FACE_CHUNKS {
            Ok(Self { face, u, v })
        } else {
            Err(CoordError::Chunk {
                u: u.into(),
                v: v.into(),
            })
        }
    }

    #[inline]
    pub const fn face(self) -> Face {
        self.face
    }

    #[inline]
    pub const fn u(self) -> u16 {
        self.u
    }

    #[inline]
    pub const fn v(self) -> u16 {
        self.v
    }

    #[inline]
    pub const fn block_origin(self) -> SurfacePos {
        SurfacePos {
            face: self.face,
            u: self.u * CHUNK_X as u16,
            v: self.v * CHUNK_Z as u16,
        }
    }

    #[inline]
    pub const fn from_surface(surface: SurfacePos) -> Self {
        Self {
            face: surface.face,
            u: surface.u / CHUNK_X as u16,
            v: surface.v / CHUNK_Z as u16,
        }
    }

    /// Face-local coordinates relative to its center. This exists for the
    /// explicitly temporary goal-1 terrain adapter; authoritative addresses
    /// remain the bounded `u`/`v` fields.
    #[inline]
    pub const fn centered_u(self) -> i32 {
        self.u as i32 - FACE_CHUNKS as i32 / 2
    }

    #[inline]
    pub const fn centered_v(self) -> i32 {
        self.v as i32 - FACE_CHUNKS as i32 / 2
    }

    pub fn from_centered(face: Face, u: i32, v: i32) -> Result<Self, CoordError> {
        let half = FACE_CHUNKS as i32 / 2;
        let absolute_u = u + half;
        let absolute_v = v + half;
        if !(0..FACE_CHUNKS as i32).contains(&absolute_u)
            || !(0..FACE_CHUNKS as i32).contains(&absolute_v)
        {
            return Err(CoordError::Chunk {
                u: absolute_u.max(0) as u32,
                v: absolute_v.max(0) as u32,
            });
        }
        Self::new(face, absolute_u as u16, absolute_v as u16)
    }

    /// Temporary porting bridge for legacy local simulation call sites. Raw
    /// `(x,z)` block coordinates are interpreted relative to the center of
    /// `PosZ`, then canonicalized. Goal 1 removes all ordinary callers.
    #[cfg(test)]
    #[doc(hidden)]
    pub fn of_world(wx: i32, wz: i32) -> Self {
        let half = i32::from(FACE_BLOCKS) / 2;
        let entity =
            EntityPos::uncanonical(Face::PosZ, (wx + half) as f32, 0.0, (wz + half) as f32)
                .canonicalized()
                .expect("legacy local coordinate is within the bounded porting window")
                .pos;
        Self {
            face: entity.face(),
            u: entity.u().floor() as u16 / CHUNK_X as u16,
            v: entity.v().floor() as u16 / CHUNK_Z as u16,
        }
    }

    pub fn step(self, direction: Direction4) -> ChunkStep {
        let (inside, u, v) = match direction {
            Direction4::East if self.u + 1 < FACE_CHUNKS => (true, self.u + 1, self.v),
            Direction4::North if self.v + 1 < FACE_CHUNKS => (true, self.u, self.v + 1),
            Direction4::West if self.u > 0 => (true, self.u - 1, self.v),
            Direction4::South if self.v > 0 => (true, self.u, self.v - 1),
            _ => (false, self.u, self.v),
        };
        if inside {
            return ChunkStep {
                pos: Self {
                    face: self.face,
                    u,
                    v,
                },
                direction,
                rotation: QuarterTurn::IDENTITY,
            };
        }
        // The Deep's borders are walls (capability E10): a step off its
        // edge stays put rather than wrapping onto the planet cube.
        if self.face.is_deep() {
            return ChunkStep {
                pos: self,
                direction,
                rotation: QuarterTurn::IDENTITY,
            };
        }
        let edge = direction.edge();
        let transform = edge_transform(self.face, edge);
        let varying = match edge {
            Edge::West | Edge::East => self.v,
            Edge::South | Edge::North => self.u,
        };
        let varying = if transform.flip {
            FACE_CHUNKS - 1 - varying
        } else {
            varying
        };
        let last = FACE_CHUNKS - 1;
        let (u, v) = match transform.enter {
            Edge::West => (0, varying),
            Edge::East => (last, varying),
            Edge::South => (varying, 0),
            Edge::North => (varying, last),
        };
        let rotation = transform.rotation_for(edge);
        ChunkStep {
            pos: Self {
                face: transform.to,
                u,
                v,
            },
            direction: rotation.map_direction(direction),
            rotation,
        }
    }

    /// Move a small face-local chunk offset. `u` is applied before `v`, the
    /// same canonical corner order used by entity and eight-neighbor walks.
    pub fn offset(self, du: i32, dv: i32) -> Self {
        let mut at = self;
        let mut orientation = QuarterTurn::IDENTITY;
        let u_direction = if du >= 0 {
            Direction4::East
        } else {
            Direction4::West
        };
        for _ in 0..du.unsigned_abs() {
            let step = at.step(orientation.map_direction(u_direction));
            at = step.pos;
            orientation = step.rotation.then(orientation);
        }
        let v_direction = if dv >= 0 {
            Direction4::North
        } else {
            Direction4::South
        };
        for _ in 0..dv.unsigned_abs() {
            let step = at.step(orientation.map_direction(v_direction));
            at = step.pos;
            orientation = step.rotation.then(orientation);
        }
        at
    }

    pub fn neighbors4(self) -> [Self; 4] {
        [
            self.step(Direction4::East).pos,
            self.step(Direction4::North).pos,
            self.step(Direction4::West).pos,
            self.step(Direction4::South).pos,
        ]
    }

    /// Great-circle separation between chunk centers, in surface-block units.
    pub fn distance(self, other: Self) -> f64 {
        let center = |pos: Self| SurfacePoint {
            face: pos.face,
            u: f64::from(pos.u) * CHUNK_X as f64 + CHUNK_X as f64 * 0.5,
            v: f64::from(pos.v) * CHUNK_Z as f64 + CHUNK_Z as f64 * 0.5,
        };
        geodesic_distance(center(self), center(other))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityPos {
    face: Face,
    /// Centered face-local coordinates retained for ordinary local vector
    /// math. `u()` and `v()` expose the canonical bounded chart values.
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Serialize for EntityPos {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Wire {
            face: Face,
            u: f32,
            y: f32,
            v: f32,
        }
        Wire {
            face: self.face,
            u: self.u(),
            y: self.y,
            v: self.v(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for EntityPos {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            face: Face,
            u: f32,
            y: f32,
            v: f32,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.face, wire.u, wire.y, wire.v).map_err(serde::de::Error::custom)
    }
}

impl EntityPos {
    fn uncanonical(face: Face, u: f32, y: f32, v: f32) -> Self {
        let half = f32::from(FACE_BLOCKS) * 0.5;
        Self {
            face,
            x: u - half,
            y,
            z: v - half,
        }
    }

    pub fn new(face: Face, u: f32, y: f32, v: f32) -> Result<Self, CoordError> {
        let pos = Self::uncanonical(face, u, y, v);
        if pos.is_canonical() {
            Ok(pos)
        } else if !u.is_finite() || !y.is_finite() || !v.is_finite() {
            Err(CoordError::NonFinite)
        } else {
            Err(CoordError::Surface {
                u: u.max(0.0) as u32,
                v: v.max(0.0) as u32,
            })
        }
    }

    pub fn from_local(face: Face, local: Vec3) -> Result<Self, CoordError> {
        let half = f32::from(FACE_BLOCKS) * 0.5;
        Self::new(face, local.x + half, local.y, local.z + half)
    }

    pub fn relocated_local(self, local: Vec3) -> Result<Self, CoordError> {
        Self::from_local(self.face, local)
    }

    #[inline]
    pub const fn face(self) -> Face {
        self.face
    }

    #[inline]
    pub const fn u(self) -> f32 {
        self.x + FACE_BLOCKS as f32 * 0.5
    }

    #[inline]
    pub const fn y(self) -> f32 {
        self.y
    }

    #[inline]
    pub const fn v(self) -> f32 {
        self.z + FACE_BLOCKS as f32 * 0.5
    }

    #[inline]
    pub const fn local(self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }

    #[inline]
    pub const fn chart_local(self) -> Vec3 {
        Vec3::new(self.u(), self.y, self.v())
    }

    pub fn render_pos(self) -> Vec3 {
        block_to_render(
            SurfacePoint {
                face: self.face,
                u: f64::from(self.u()),
                v: f64::from(self.v()),
            },
            f64::from(self.y),
        )
        .as_vec3()
    }

    #[inline]
    pub fn surface_point(self) -> SurfacePoint {
        SurfacePoint {
            face: self.face,
            u: f64::from(self.u()),
            v: f64::from(self.v()),
        }
    }

    /// Great-circle horizontal distance, in block units.
    pub fn horizontal_distance_to(self, other: Self) -> f32 {
        geodesic_distance(self.surface_point(), other.surface_point()) as f32
    }

    /// Nearby displacement expressed in this position's local tangent frame.
    /// This remains continuous when `other` has just crossed a face seam.
    pub fn local_delta_to(self, other: Self) -> Vec3 {
        let distance = self.horizontal_distance_to(other);
        let Some(bearing) = great_circle_bearing(self.surface_point(), other.surface_point())
        else {
            return Vec3::new(0.0, other.y - self.y, 0.0);
        };
        Vec3::new(
            distance * bearing.sin() as f32,
            other.y - self.y,
            distance * bearing.cos() as f32,
        )
    }

    pub fn distance_to(self, other: Self) -> f32 {
        let delta = self.local_delta_to(other);
        delta.length()
    }

    #[inline]
    pub fn is_canonical(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.z.is_finite()
            && self.x >= -(FACE_BLOCKS as f32 * 0.5)
            && self.x < FACE_BLOCKS as f32 * 0.5
            && self.z >= -(FACE_BLOCKS as f32 * 0.5)
            && self.z < FACE_BLOCKS as f32 * 0.5
    }

    pub fn block(self) -> Option<BlockPos> {
        if !self.is_canonical() || self.y < 0.0 || self.y >= CHUNK_Y as f32 {
            return None;
        }
        BlockPos::new(
            self.face,
            self.u().floor() as u16,
            self.y.floor() as u8,
            self.v().floor() as u16,
        )
        .ok()
    }

    #[inline]
    pub fn surface(self) -> SurfacePos {
        SurfacePos {
            face: self.face,
            u: self.u().floor() as u16,
            v: self.v().floor() as u16,
        }
    }

    pub fn chunk(self) -> Option<ChunkPos> {
        self.block().map(BlockPos::chunk)
    }

    /// Move in the current face-local frame and canonicalize the result.
    pub fn translated(self, delta: Vec3) -> Result<CanonicalEntity, CoordError> {
        EntityPos {
            face: self.face,
            x: self.x + delta.x,
            y: self.y + delta.y,
            z: self.z + delta.z,
        }
        .canonicalized()
    }

    /// Canonicalize a finite position, crossing `u` before `v` when a single
    /// displacement exits through an exact cube corner. The returned
    /// [`QuarterTurn`] rotates horizontal velocity, facing, and yaw into the
    /// destination chart.
    pub fn canonicalized(mut self) -> Result<CanonicalEntity, CoordError> {
        if !self.x.is_finite() || !self.y.is_finite() || !self.z.is_finite() {
            return Err(CoordError::NonFinite);
        }
        let mut rotation = QuarterTurn::IDENTITY;
        // Ordinary movement crosses at most one edge. The generous bound also
        // supports debug/catch-up moves without allowing an attacker to make
        // canonicalization an unbounded loop.
        for _ in 0..32 {
            let u = self.u();
            let v = self.v();
            let edge = if u < 0.0 {
                Some(Edge::West)
            } else if u >= f32::from(FACE_BLOCKS) {
                Some(Edge::East)
            } else if v < 0.0 {
                Some(Edge::South)
            } else if v >= f32::from(FACE_BLOCKS) {
                Some(Edge::North)
            } else {
                return Ok(CanonicalEntity {
                    pos: self,
                    rotation,
                });
            };
            let edge = edge.expect("edge selected above");
            // The Deep's borders are walls (capability E10): drifting past
            // one clamps back inside instead of wrapping onto the cube.
            if self.face.is_deep() {
                self.x = self.x.clamp(
                    -f32::from(FACE_BLOCKS) * 0.5,
                    f32::from(FACE_BLOCKS) * 0.5,
                );
                self.z = self.z.clamp(
                    -f32::from(FACE_BLOCKS) * 0.5,
                    f32::from(FACE_BLOCKS) * 0.5,
                );
                return Ok(CanonicalEntity {
                    pos: self,
                    rotation,
                });
            }
            let transform = edge_transform(self.face, edge);
            let (u, v) = transform.cross_continuous(edge, u, v);
            self.face = transform.to;
            self.x = u - f32::from(FACE_BLOCKS) * 0.5;
            self.z = v - f32::from(FACE_BLOCKS) * 0.5;
            rotation = transform.rotation_for(edge).then(rotation);
        }
        Err(CoordError::TooFarFromCanonical)
    }
}

impl std::ops::Add<Vec3> for EntityPos {
    type Output = Vec3;

    fn add(self, rhs: Vec3) -> Self::Output {
        self.local() + rhs
    }
}

impl std::ops::Sub<Vec3> for EntityPos {
    type Output = Vec3;

    fn sub(self, rhs: Vec3) -> Self::Output {
        self.local() - rhs
    }
}

impl std::ops::Sub<EntityPos> for EntityPos {
    type Output = Vec3;

    fn sub(self, rhs: EntityPos) -> Self::Output {
        self.local() - rhs.local()
    }
}

impl std::ops::Sub<EntityPos> for Vec3 {
    type Output = Vec3;

    fn sub(self, rhs: EntityPos) -> Self::Output {
        self - rhs.local()
    }
}

#[cfg(test)]
impl From<Vec3> for EntityPos {
    fn from(local: Vec3) -> Self {
        EntityPos::from_local(Face::PosZ, local)
            .expect("test Vec3 position must fit on the finite PosZ face")
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanonicalEntity {
    pub pos: EntityPos,
    pub rotation: QuarterTurn,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChunkStep {
    pub pos: ChunkPos,
    pub direction: Direction4,
    pub rotation: QuarterTurn,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Direction4 {
    East = 0,
    North = 1,
    West = 2,
    South = 3,
}

impl Direction4 {
    pub const ALL: [Self; 4] = [Self::East, Self::North, Self::West, Self::South];

    #[inline]
    const fn from_index(index: u8) -> Self {
        match index & 3 {
            0 => Self::East,
            1 => Self::North,
            2 => Self::West,
            _ => Self::South,
        }
    }

    #[inline]
    pub const fn opposite(self) -> Self {
        Self::from_index(self as u8 + 2)
    }

    #[inline]
    pub const fn edge(self) -> Edge {
        match self {
            Self::East => Edge::East,
            Self::North => Edge::North,
            Self::West => Edge::West,
            Self::South => Edge::South,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Direction6 {
    East,
    North,
    West,
    South,
    Up,
    Down,
}

impl Direction6 {
    pub const ALL: [Self; 6] = [
        Self::East,
        Self::North,
        Self::West,
        Self::South,
        Self::Up,
        Self::Down,
    ];

    pub const fn opposite(self) -> Self {
        match self {
            Self::East => Self::West,
            Self::North => Self::South,
            Self::West => Self::East,
            Self::South => Self::North,
            Self::Up => Self::Down,
            Self::Down => Self::Up,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Edge {
    West = 0,
    East = 1,
    South = 2,
    North = 3,
}

impl Edge {
    #[inline]
    const fn inward(self) -> Direction4 {
        match self {
            Self::West => Direction4::East,
            Self::East => Direction4::West,
            Self::South => Direction4::North,
            Self::North => Direction4::South,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QuarterTurn(u8);

impl QuarterTurn {
    pub const IDENTITY: Self = Self(0);

    #[inline]
    pub const fn new(turns_counterclockwise: u8) -> Self {
        Self(turns_counterclockwise & 3)
    }

    #[inline]
    pub const fn turns(self) -> u8 {
        self.0
    }

    #[inline]
    pub const fn map_direction(self, direction: Direction4) -> Direction4 {
        Direction4::from_index(direction as u8 + self.0)
    }

    #[inline]
    pub const fn then(self, earlier: QuarterTurn) -> QuarterTurn {
        QuarterTurn::new(earlier.0 + self.0)
    }

    #[inline]
    pub fn rotate_vec3(self, vector: Vec3) -> Vec3 {
        match self.0 {
            0 => vector,
            1 => Vec3::new(-vector.z, vector.y, vector.x),
            2 => Vec3::new(-vector.x, vector.y, -vector.z),
            _ => Vec3::new(vector.z, vector.y, -vector.x),
        }
    }

    #[inline]
    pub fn rotate_yaw(self, yaw: f32) -> f32 {
        (yaw + f32::from(self.0) * std::f32::consts::FRAC_PI_2).rem_euclid(std::f32::consts::TAU)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct IVec3 {
    x: i8,
    y: i8,
    z: i8,
}

impl IVec3 {
    const fn new(x: i8, y: i8, z: i8) -> Self {
        Self { x, y, z }
    }

    const fn as_dvec3(self) -> DVec3 {
        DVec3::new(self.x as f64, self.y as f64, self.z as f64)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FaceBasis {
    normal: IVec3,
    u: IVec3,
    v: IVec3,
}

impl FaceBasis {
    pub const fn normal(self) -> [i8; 3] {
        [self.normal.x, self.normal.y, self.normal.z]
    }

    pub const fn u(self) -> [i8; 3] {
        [self.u.x, self.u.y, self.u.z]
    }

    pub const fn v(self) -> [i8; 3] {
        [self.v.x, self.v.y, self.v.z]
    }
}

/// The sole face-frame authority. For every entry `u × v = normal`.
pub const FACE_BASES: [FaceBasis; SURFACE_FACES] = [
    FaceBasis {
        normal: IVec3::new(1, 0, 0),
        u: IVec3::new(0, 0, -1),
        v: IVec3::new(0, 1, 0),
    },
    FaceBasis {
        normal: IVec3::new(-1, 0, 0),
        u: IVec3::new(0, 0, 1),
        v: IVec3::new(0, 1, 0),
    },
    FaceBasis {
        normal: IVec3::new(0, 1, 0),
        u: IVec3::new(1, 0, 0),
        v: IVec3::new(0, 0, -1),
    },
    FaceBasis {
        normal: IVec3::new(0, -1, 0),
        u: IVec3::new(1, 0, 0),
        v: IVec3::new(0, 0, 1),
    },
    FaceBasis {
        normal: IVec3::new(0, 0, 1),
        u: IVec3::new(1, 0, 0),
        v: IVec3::new(0, 1, 0),
    },
    FaceBasis {
        normal: IVec3::new(0, 0, -1),
        u: IVec3::new(-1, 0, 0),
        v: IVec3::new(0, 1, 0),
    },
];

#[derive(Clone, Copy, Debug)]
struct EdgeTransform {
    to: Face,
    enter: Edge,
    /// Source edge's increasing coordinate maps to decreasing destination
    /// edge coordinate.
    flip: bool,
}

impl EdgeTransform {
    const fn new(to: Face, enter: Edge, flip: bool) -> Self {
        Self { to, enter, flip }
    }

    fn map_varying_index(self, value: u16) -> u16 {
        if self.flip {
            FACE_BLOCKS - 1 - value
        } else {
            value
        }
    }

    fn cross_cell(self, source_edge: Edge, u: u16, v: u16) -> SurfacePos {
        let varying = match source_edge {
            Edge::West | Edge::East => v,
            Edge::South | Edge::North => u,
        };
        let varying = self.map_varying_index(varying);
        let last = FACE_BLOCKS - 1;
        let (u, v) = match self.enter {
            Edge::West => (0, varying),
            Edge::East => (last, varying),
            Edge::South => (varying, 0),
            Edge::North => (varying, last),
        };
        SurfacePos {
            face: self.to,
            u,
            v,
        }
    }

    fn cross_continuous(self, source_edge: Edge, u: f32, v: f32) -> (f32, f32) {
        let side = f32::from(FACE_BLOCKS);
        let overflow = match source_edge {
            Edge::West => -u,
            Edge::East => u - side,
            Edge::South => -v,
            Edge::North => v - side,
        };
        let varying = match source_edge {
            Edge::West | Edge::East => v,
            Edge::South | Edge::North => u,
        };
        let varying = if self.flip { side - varying } else { varying };
        match self.enter {
            Edge::West => (overflow, varying),
            Edge::East => (side - overflow, varying),
            Edge::South => (varying, overflow),
            Edge::North => (varying, side - overflow),
        }
    }

    fn rotation_for(self, source_edge: Edge) -> QuarterTurn {
        let source_crossing = match source_edge {
            Edge::East => Direction4::East,
            Edge::North => Direction4::North,
            Edge::West => Direction4::West,
            Edge::South => Direction4::South,
        };
        let destination_crossing = self.enter.inward();
        QuarterTurn::new((destination_crossing as u8 + 4 - source_crossing as u8) & 3)
    }
}

// Indexed [Face][Edge: west, east, south, north]. This table and FACE_BASES
// are the only face-specific topology data in the engine.
const EDGE_TRANSFORMS: [[EdgeTransform; 4]; SURFACE_FACES] = [
    // PosX
    [
        EdgeTransform::new(Face::PosZ, Edge::East, false),
        EdgeTransform::new(Face::NegZ, Edge::West, false),
        EdgeTransform::new(Face::NegY, Edge::East, true),
        EdgeTransform::new(Face::PosY, Edge::East, false),
    ],
    // NegX
    [
        EdgeTransform::new(Face::NegZ, Edge::East, false),
        EdgeTransform::new(Face::PosZ, Edge::West, false),
        EdgeTransform::new(Face::NegY, Edge::West, false),
        EdgeTransform::new(Face::PosY, Edge::West, true),
    ],
    // PosY
    [
        EdgeTransform::new(Face::NegX, Edge::North, true),
        EdgeTransform::new(Face::PosX, Edge::North, false),
        EdgeTransform::new(Face::PosZ, Edge::North, false),
        EdgeTransform::new(Face::NegZ, Edge::North, true),
    ],
    // NegY
    [
        EdgeTransform::new(Face::NegX, Edge::South, false),
        EdgeTransform::new(Face::PosX, Edge::South, true),
        EdgeTransform::new(Face::NegZ, Edge::South, true),
        EdgeTransform::new(Face::PosZ, Edge::South, false),
    ],
    // PosZ
    [
        EdgeTransform::new(Face::NegX, Edge::East, false),
        EdgeTransform::new(Face::PosX, Edge::West, false),
        EdgeTransform::new(Face::NegY, Edge::North, false),
        EdgeTransform::new(Face::PosY, Edge::South, false),
    ],
    // NegZ
    [
        EdgeTransform::new(Face::PosX, Edge::East, false),
        EdgeTransform::new(Face::NegX, Edge::West, false),
        EdgeTransform::new(Face::NegY, Edge::South, true),
        EdgeTransform::new(Face::PosY, Edge::North, true),
    ],
];

#[inline]
fn edge_transform(face: Face, edge: Edge) -> EdgeTransform {
    EDGE_TRANSFORMS[face.index()][edge as usize]
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SurfaceStep {
    pub pos: SurfacePos,
    /// The input heading expressed in the destination chart.
    pub direction: Direction4,
    /// Rotates any other direction/facing state through the same crossing.
    pub rotation: QuarterTurn,
}

pub fn step4(pos: SurfacePos, direction: Direction4) -> SurfaceStep {
    let (inside, u, v) = match direction {
        Direction4::East if pos.u + 1 < FACE_BLOCKS => (true, pos.u + 1, pos.v),
        Direction4::North if pos.v + 1 < FACE_BLOCKS => (true, pos.u, pos.v + 1),
        Direction4::West if pos.u > 0 => (true, pos.u - 1, pos.v),
        Direction4::South if pos.v > 0 => (true, pos.u, pos.v - 1),
        _ => (false, pos.u, pos.v),
    };
    if inside {
        return SurfaceStep {
            pos: SurfacePos {
                face: pos.face,
                u,
                v,
            },
            direction,
            rotation: QuarterTurn::IDENTITY,
        };
    }
    let edge = direction.edge();
    // The Deep has no neighbor faces: its borders are simply walls.
    if pos.face.is_deep() {
        return SurfaceStep {
            pos,
            direction,
            rotation: QuarterTurn::IDENTITY,
        };
    }
    let transform = edge_transform(pos.face, edge);
    let rotation = transform.rotation_for(edge);
    SurfaceStep {
        pos: transform.cross_cell(edge, pos.u, pos.v),
        direction: rotation.map_direction(direction),
        rotation,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockStep {
    pub pos: BlockPos,
    pub direction: Direction6,
    pub rotation: QuarterTurn,
}

pub fn step6(pos: BlockPos, direction: Direction6) -> Option<BlockStep> {
    match direction {
        Direction6::Up if usize::from(pos.y) + 1 < CHUNK_Y => Some(BlockStep {
            pos: BlockPos {
                surface: pos.surface,
                y: pos.y + 1,
            },
            direction,
            rotation: QuarterTurn::IDENTITY,
        }),
        Direction6::Down if pos.y > 0 => Some(BlockStep {
            pos: BlockPos {
                surface: pos.surface,
                y: pos.y - 1,
            },
            direction,
            rotation: QuarterTurn::IDENTITY,
        }),
        Direction6::Up | Direction6::Down => None,
        horizontal => {
            let direction4 = match horizontal {
                Direction6::East => Direction4::East,
                Direction6::North => Direction4::North,
                Direction6::West => Direction4::West,
                Direction6::South => Direction4::South,
                Direction6::Up | Direction6::Down => unreachable!(),
            };
            let stepped = step4(pos.surface, direction4);
            let direction = match stepped.direction {
                Direction4::East => Direction6::East,
                Direction4::North => Direction6::North,
                Direction4::West => Direction6::West,
                Direction4::South => Direction6::South,
            };
            Some(BlockStep {
                pos: BlockPos {
                    surface: stepped.pos,
                    y: pos.y,
                },
                direction,
                rotation: stepped.rotation,
            })
        }
    }
}

#[inline]
pub fn neighbors4(pos: SurfacePos) -> [SurfacePos; 4] {
    [
        step4(pos, Direction4::East).pos,
        step4(pos, Direction4::North).pos,
        step4(pos, Direction4::West).pos,
        step4(pos, Direction4::South).pos,
    ]
}

/// Cardinal neighbors followed by NE, NW, SW, SE. Diagonals always cross the
/// `u` edge first and rotate the subsequent `v` step through that edge.
pub fn neighbors8(pos: SurfacePos) -> [SurfacePos; 8] {
    let cardinals = neighbors4(pos);
    let diagonal = |u_direction: Direction4, v_direction: Direction4| {
        let first = step4(pos, u_direction);
        step4(first.pos, first.rotation.map_direction(v_direction)).pos
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

pub fn neighbors6(pos: BlockPos) -> impl Iterator<Item = BlockPos> {
    [
        Direction6::East,
        Direction6::North,
        Direction6::West,
        Direction6::South,
        Direction6::Up,
        Direction6::Down,
    ]
    .into_iter()
    .filter_map(move |direction| step6(pos, direction).map(|step| step.pos))
}

/// Stable cube coordinate before spherification. Equal seam vertices produce
/// the exact same three f64 components from either face.
fn cube_point(point: SurfacePoint) -> DVec3 {
    let side = f64::from(FACE_BLOCKS);
    let s = point.u.mul_add(2.0 / side, -1.0);
    let t = point.v.mul_add(2.0 / side, -1.0);
    // The Deep borrows the PosZ frame (see local_frame): its positions are
    // never on the cube, but the math must stay finite for callers like
    // daylight sampling.
    let basis_index = if point.face.is_deep() {
        Face::PosZ.index()
    } else {
        point.face.index()
    };
    let basis = FACE_BASES[basis_index];
    basis.normal.as_dvec3() + basis.u.as_dvec3() * s + basis.v.as_dvec3() * t
}

/// Low-distortion spherified-cube embedding.
pub fn surface_to_unit(point: SurfacePoint) -> DVec3 {
    let cube = cube_point(point);
    let x2 = cube.x * cube.x;
    let y2 = cube.y * cube.y;
    let z2 = cube.z * cube.z;
    let mapped = DVec3::new(
        cube.x * (1.0 - y2 * 0.5 - z2 * 0.5 + y2 * z2 / 3.0).max(0.0).sqrt(),
        cube.y * (1.0 - z2 * 0.5 - x2 * 0.5 + z2 * x2 / 3.0).max(0.0).sqrt(),
        cube.z * (1.0 - x2 * 0.5 - y2 * 0.5 + x2 * y2 / 3.0).max(0.0).sqrt(),
    );
    mapped.normalize()
}

#[inline]
pub fn block_to_render(point: SurfacePoint, radial_y: f64) -> DVec3 {
    surface_to_unit(point) * (PLANET_RADIUS + radial_y)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalFrame {
    pub east: DVec3,
    pub up: DVec3,
    pub north: DVec3,
}

pub fn local_frame(point: SurfacePoint) -> LocalFrame {
    let up = surface_to_unit(point);
    // The Deep is not on the cube: it borrows the PosZ tangent frame, which
    // only matters for rendering orientation of axis-aligned dungeon rooms.
    let basis_index = if point.face.is_deep() {
        Face::PosZ.index()
    } else {
        point.face.index()
    };
    let basis = FACE_BASES[basis_index];
    let mut east = basis.u.as_dvec3();
    east -= up * east.dot(up);
    east = east.normalize();
    let north = up.cross(east).normalize();
    LocalFrame { east, up, north }
}

pub fn geodesic_distance(a: SurfacePoint, b: SurfacePoint) -> f64 {
    let dot = surface_to_unit(a).dot(surface_to_unit(b)).clamp(-1.0, 1.0);
    dot.acos() * PLANET_RADIUS
}

/// Radians clockwise from local north. Identical and antipodal points have no
/// stable bearing and return `None`.
pub fn great_circle_bearing(a: SurfacePoint, b: SurfacePoint) -> Option<f64> {
    let from = surface_to_unit(a);
    let to = surface_to_unit(b);
    let dot = from.dot(to).clamp(-1.0, 1.0);
    if (1.0 - dot).abs() < 1.0e-12 || (1.0 + dot).abs() < 1.0e-10 {
        return None;
    }
    let tangent = (to - from * dot).normalize();
    let frame = local_frame(a);
    Some(tangent.dot(frame.east).atan2(tangent.dot(frame.north)))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Distortion {
    pub u_step: f64,
    pub v_step: f64,
    pub min_step: f64,
    pub max_step: f64,
}

/// Physical arc length represented by one logical step around a continuous
/// point. Used by diagnostics; logical distance remains simulation truth.
pub fn distortion_at(point: SurfacePoint) -> Distortion {
    let side = f64::from(FACE_BLOCKS);
    let lo_u = (point.u - 0.5).clamp(0.0, side);
    let hi_u = (point.u + 0.5).clamp(0.0, side);
    let lo_v = (point.v - 0.5).clamp(0.0, side);
    let hi_v = (point.v + 0.5).clamp(0.0, side);
    let u_span = (hi_u - lo_u).max(f64::EPSILON);
    let v_span = (hi_v - lo_v).max(f64::EPSILON);
    let u_step = geodesic_distance(
        SurfacePoint { u: lo_u, ..point },
        SurfacePoint { u: hi_u, ..point },
    ) / u_span;
    let v_step = geodesic_distance(
        SurfacePoint { v: lo_v, ..point },
        SurfacePoint { v: hi_v, ..point },
    ) / v_span;
    Distortion {
        u_step,
        v_step,
        min_step: u_step.min(v_step),
        max_step: u_step.max(v_step),
    }
}

/// Great-circle surface distance to the geometric horizon.
pub fn horizon_surface_distance(altitude: f64) -> f64 {
    if altitude <= 0.0 {
        return 0.0;
    }
    (PLANET_RADIUS / (PLANET_RADIUS + altitude))
        .clamp(-1.0, 1.0)
        .acos()
        * PLANET_RADIUS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip_save_and_wire<T>(value: T)
    where
        T: serde::Serialize + serde::de::DeserializeOwned + Copy + PartialEq + std::fmt::Debug,
    {
        let wire = postcard::to_allocvec(&value).unwrap();
        assert_eq!(postcard::from_bytes::<T>(&wire).unwrap(), value);

        let save = toml::to_string(&value).unwrap();
        assert_eq!(toml::from_str::<T>(&save).unwrap(), value);
    }

    fn pos(face: Face, u: u16, v: u16) -> SurfacePos {
        SurfacePos::new(face, u, v).unwrap()
    }

    #[test]
    fn dimensions_are_the_design_constants() {
        assert_eq!(FACE_BLOCKS, 8192);
        assert_eq!(FACE_CHUNKS, 512);
        assert_eq!(CHUNK_Y, 256);
        assert_eq!(BLOCK_CELL_COUNT, 103_079_215_104);
        assert!((PLANET_RADIUS - 5215.189).abs() < 0.001);
    }

    #[test]
    fn validated_coordinates_reject_the_finite_boundaries() {
        assert!(SurfacePos::new(Face::PosX, FACE_BLOCKS - 1, FACE_BLOCKS - 1).is_ok());
        assert!(SurfacePos::new(Face::PosX, FACE_BLOCKS, 0).is_err());
        assert!(ChunkPos::new(Face::NegZ, FACE_CHUNKS - 1, FACE_CHUNKS - 1).is_ok());
        assert!(ChunkPos::new(Face::NegZ, FACE_CHUNKS, 0).is_err());
        assert!(Face::from_u8(5).is_some());
        // Face 6 is the Deep (capability E10); 7 and beyond stay invalid.
        assert_eq!(Face::from_u8(6), Some(Face::Deep));
        assert!(Face::from_u8(7).is_none());
    }

    #[test]
    fn every_planet_coordinate_component_round_trips_through_save_and_wire_codecs() {
        // The Cartesian product contains 103 billion block cells, but each
        // codec treats fields independently. Exercise every legal value of
        // every bounded component on every face, with permutations ensuring
        // each value appears in both horizontal fields and every y appears
        // repeatedly. Entity fractions pin lossless f32 serialization too.
        for face in Face::ALL {
            for n in 0..FACE_BLOCKS {
                let reverse = FACE_BLOCKS - 1 - n;
                round_trip_save_and_wire(SurfacePos::new(face, n, reverse).unwrap());
                round_trip_save_and_wire(
                    BlockPos::new(face, reverse, (n % CHUNK_Y as u16) as u8, n).unwrap(),
                );
                round_trip_save_and_wire(
                    EntityPos::new(
                        face,
                        f32::from(n) + 0.25,
                        f32::from(n % CHUNK_Y as u16) + 0.5,
                        f32::from(reverse) + 0.75,
                    )
                    .unwrap(),
                );
            }
            for n in 0..FACE_CHUNKS {
                round_trip_save_and_wire(ChunkPos::new(face, n, FACE_CHUNKS - 1 - n).unwrap());
            }
        }
    }

    #[test]
    fn deserialization_rejects_invalid_and_noncanonical_positions() {
        #[derive(Serialize)]
        struct SurfaceWire {
            face: Face,
            u: u16,
            v: u16,
        }
        #[derive(Serialize)]
        struct EntityWire {
            face: Face,
            u: f32,
            y: f32,
            v: f32,
        }

        let bad_surface = postcard::to_allocvec(&SurfaceWire {
            face: Face::PosX,
            u: FACE_BLOCKS,
            v: 0,
        })
        .unwrap();
        assert!(postcard::from_bytes::<SurfacePos>(&bad_surface).is_err());

        let bad_chunk = postcard::to_allocvec(&SurfaceWire {
            face: Face::PosX,
            u: FACE_CHUNKS,
            v: 0,
        })
        .unwrap();
        assert!(postcard::from_bytes::<ChunkPos>(&bad_chunk).is_err());

        let bad_entity = postcard::to_allocvec(&EntityWire {
            face: Face::PosX,
            u: f32::NAN,
            y: 80.0,
            v: 10.0,
        })
        .unwrap();
        assert!(postcard::from_bytes::<EntityPos>(&bad_entity).is_err());

        let outside_entity = postcard::to_allocvec(&EntityWire {
            face: Face::PosX,
            u: f32::from(FACE_BLOCKS),
            y: 80.0,
            v: 10.0,
        })
        .unwrap();
        assert!(postcard::from_bytes::<EntityPos>(&outside_entity).is_err());
    }

    #[test]
    fn every_face_edge_is_reciprocal_at_every_coordinate() {
        for face in Face::ALL {
            for direction in Direction4::ALL {
                for varying in 0..FACE_BLOCKS {
                    let start = match direction {
                        Direction4::East => pos(face, FACE_BLOCKS - 1, varying),
                        Direction4::North => pos(face, varying, FACE_BLOCKS - 1),
                        Direction4::West => pos(face, 0, varying),
                        Direction4::South => pos(face, varying, 0),
                    };
                    let across = step4(start, direction);
                    let back = step4(across.pos, across.direction.opposite());
                    assert_eq!(back.pos, start, "{face:?} {direction:?} {varying}");
                    assert_eq!(back.direction, direction.opposite());
                }
            }
        }
    }

    #[test]
    fn elementary_loops_close_with_orientation() {
        // Walking a cell-sized square can cross an edge but not encircle one
        // of the eight extraordinary cube vertices; it must return with the
        // same heading. The separate corner test pins the documented
        // u-before-v canonical order at those vertices, where three faces
        // meet and an ordinary four-cell loop does not exist.
        for face in Face::ALL {
            for &(u, v) in &[
                (0, 1),
                (FACE_BLOCKS - 1, 1),
                (1, 0),
                (1, FACE_BLOCKS - 1),
                (FACE_BLOCKS / 2, FACE_BLOCKS / 2),
            ] {
                let start = pos(face, u, v);
                let mut here = start;
                let mut orientation = QuarterTurn::IDENTITY;
                for original in [
                    Direction4::East,
                    Direction4::North,
                    Direction4::West,
                    Direction4::South,
                ] {
                    let step = step4(here, orientation.map_direction(original));
                    here = step.pos;
                    orientation = step.rotation.then(orientation);
                }
                assert_eq!(here, start, "loop from {start:?}");
                assert_eq!(orientation, QuarterTurn::IDENTITY);
            }
        }
    }

    #[test]
    fn all_eight_corners_canonicalize_deterministically() {
        let side = f32::from(FACE_BLOCKS);
        let epsilon = 0.25;
        for face in Face::ALL {
            for &(u, v) in &[
                (-epsilon, -epsilon),
                (-epsilon, side + epsilon),
                (side + epsilon, -epsilon),
                (side + epsilon, side + epsilon),
            ] {
                let input = EntityPos::uncanonical(face, u, 80.0, v);
                let a = input.canonicalized().unwrap();
                let b = input.canonicalized().unwrap();
                assert_eq!(a, b);
                assert!(a.pos.is_canonical(), "{input:?} -> {a:?}");
            }
        }
    }

    #[test]
    fn no_valid_cardinal_step_leaves_the_address_space() {
        for face in Face::ALL {
            for u in [0, 1, FACE_BLOCKS / 2, FACE_BLOCKS - 2, FACE_BLOCKS - 1] {
                for v in [0, 1, FACE_BLOCKS / 2, FACE_BLOCKS - 2, FACE_BLOCKS - 1] {
                    for direction in Direction4::ALL {
                        let next = step4(pos(face, u, v), direction).pos;
                        assert!(next.u < FACE_BLOCKS && next.v < FACE_BLOCKS);
                    }
                }
            }
        }
    }

    #[test]
    fn equatorial_circuit_is_exact() {
        let start = pos(Face::PosZ, 0, FACE_BLOCKS / 2);
        let mut at = start;
        let mut heading = Direction4::East;
        for _ in 0..u32::from(FACE_BLOCKS) * 4 {
            let step = step4(at, heading);
            at = step.pos;
            heading = step.direction;
        }
        assert_eq!(at, start);
        assert_eq!(heading, Direction4::East);
    }

    #[test]
    fn seam_vertices_are_bit_identical() {
        for face in Face::ALL {
            for edge in [Edge::West, Edge::East, Edge::South, Edge::North] {
                let transform = edge_transform(face, edge);
                for varying in [0.0, 1.0, 257.5, 4096.0, 8191.0, 8192.0] {
                    let source = match edge {
                        Edge::West => SurfacePoint {
                            face,
                            u: 0.0,
                            v: varying,
                        },
                        Edge::East => SurfacePoint {
                            face,
                            u: f64::from(FACE_BLOCKS),
                            v: varying,
                        },
                        Edge::South => SurfacePoint {
                            face,
                            u: varying,
                            v: 0.0,
                        },
                        Edge::North => SurfacePoint {
                            face,
                            u: varying,
                            v: f64::from(FACE_BLOCKS),
                        },
                    };
                    let (u, v) = transform.cross_continuous(edge, source.u as f32, source.v as f32);
                    let destination = SurfacePoint {
                        face: transform.to,
                        u: u as f64,
                        v: v as f64,
                    };
                    assert_eq!(cube_point(source), cube_point(destination));
                    assert_eq!(surface_to_unit(source), surface_to_unit(destination));
                    assert_eq!(
                        block_to_render(source, 80.0),
                        block_to_render(destination, 80.0)
                    );
                }
            }
        }
    }

    #[test]
    fn normals_point_out_and_distortion_is_bounded() {
        let mut minimum = f64::MAX;
        let mut maximum: f64 = 0.0;
        for face in Face::ALL {
            for u in [0.5, 1024.5, 4096.0, 7168.5, 8191.5] {
                for v in [0.5, 1024.5, 4096.0, 7168.5, 8191.5] {
                    let point = SurfacePoint { face, u, v };
                    let unit = surface_to_unit(point);
                    let rendered = block_to_render(point, 80.0);
                    assert!(unit.dot(rendered) > 0.0);
                    let d = distortion_at(point);
                    minimum = minimum.min(d.min_step);
                    maximum = maximum.max(d.max_step);
                }
            }
        }
        assert!(minimum > 0.70, "minimum step {minimum}");
        assert!(maximum < 1.35, "maximum step {maximum}");
    }

    #[test]
    fn distance_bearing_and_horizon_are_stable() {
        let a = pos(Face::PosZ, 4096, 4096).center();
        let b = pos(Face::PosX, 4096, 4096).center();
        assert_eq!(geodesic_distance(a, a), 0.0);
        assert!((geodesic_distance(a, b) - geodesic_distance(b, a)).abs() < 1.0e-9);
        assert!(geodesic_distance(a, b) > 8000.0);
        assert!(great_circle_bearing(a, a).is_none());
        let anti = SurfacePoint {
            face: Face::NegZ,
            u: a.u,
            v: f64::from(FACE_BLOCKS) - a.v,
        };
        assert!(great_circle_bearing(a, anti).is_none());
        assert!(horizon_surface_distance(64.0) > horizon_surface_distance(1.6));
        assert!(horizon_surface_distance(256.0) > horizon_surface_distance(64.0));
    }

    #[test]
    fn block_vertical_neighbors_stop_at_shell_bounds() {
        let bottom = BlockPos::new(Face::PosX, 7, 0, 9).unwrap();
        let top = BlockPos::new(Face::PosX, 7, (CHUNK_Y - 1) as u8, 9).unwrap();
        assert!(step6(bottom, Direction6::Down).is_none());
        assert!(step6(top, Direction6::Up).is_none());
        assert_eq!(neighbors6(bottom).count(), 5);
        assert_eq!(neighbors6(top).count(), 5);
    }
}
