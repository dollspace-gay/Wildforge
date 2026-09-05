//! Density/strata shaping and the immutable column maps used by later stages.

use super::Generator;
use crate::chunk::{CHUNK_X, CHUNK_Y, CHUNK_Z, Chunk, ChunkPos, SEA_LEVEL};
use crate::registry::AIR;
use super::{ATLAS_SURFACE_MANTLE, RING};

pub(super) struct ShapeColumns {
    pub(super) top: [[i32; RING]; RING],
    pub(super) fills: [[i32; CHUNK_Z]; CHUNK_X],
    pub(super) armors: [[i32; CHUNK_Z]; CHUNK_X],
}

pub(super) struct ShapedTerrain {
    pub(super) chunk: Chunk,
    pub(super) columns: ShapeColumns,
}

impl Generator {
    pub(super) fn shape(&self, pos: ChunkPos) -> ShapedTerrain {
        let mut c = Chunk::new();
        let (lat, lat_g) = self.sample_lattice(pos);

        // Stage 1: shape. Track pre-carve solid tops for the 18x18 ring.
        let mut shape_top = [[0i32; RING]; RING];
        let mut fills = [[0i32; CHUNK_Z]; CHUNK_X];
        let mut armors = [[0i32; CHUNK_Z]; CHUNK_X];
        for rx in 0..RING as i32 {
            for rz in 0..RING as i32 {
                let (lx, lz) = (rx - 1, rz - 1);
                let mut top = 0;
                for y in (1..CHUNK_Y as i32).rev() {
                    if Self::lat_density(&lat, lx, y, lz) > 0.0 {
                        top = y;
                        break;
                    }
                }
                shape_top[rx as usize][rz as usize] = top;
                if !(0..CHUNK_X as i32).contains(&lx) || !(0..CHUNK_Z as i32).contains(&lz) {
                    continue;
                }
                let surface = Self::surface_in_chunk(pos, lx, lz);
                // One climate read serves bands, hydrology, and rock.
                let cl = self.climate_at(surface);
                let bands = self.strata_bands_at(surface, &cl);
                let geology = self
                    .atlas
                    .as_ref()
                    .map(|atlas| atlas.geology_sample(surface.center()));
                let vol = self.atlas.as_ref().map_or(0.0, |atlas| {
                    atlas.exact_volcanic_relief(surface.center()) / 40.0
                });
                let dike = geology.is_some_and(|sample| {
                    sample.volcanic_history != 0
                        && self.hash_surface(0xd1ce, surface).is_multiple_of(37)
                });
                // Rivers and lakes flood their carve as a local sea.
                let pre = self.base_offset_at(surface, &cl);
                let (_, fill, armor) = self.hydrology_at(surface, &cl, pre);
                // Atlas hydrology is authoritative about which columns are
                // wet.  Applying the legacy global sea-level fallback to an
                // atlas river's dry carved shoulders flooded a channel up to
                // four times wider than its finite reservoir allocation.
                // Atlas oceans and lakes already return their explicit fill
                // elevation; only the atlas-free legacy generator needs the
                // blanket sea-level fallback.
                let fill_y = if self.atlas.is_some() {
                    fill.unwrap_or(0)
                } else {
                    fill.unwrap_or(0).max(SEA_LEVEL)
                };
                let armor_y = armor.unwrap_or(0);
                fills[lx as usize][lz as usize] = fill_y;
                armors[lx as usize][lz as usize] = armor_y;
                for y in 1..CHUNK_Y as i32 {
                    let density_top = shape_top[(lx + 1) as usize][(lz + 1) as usize];
                    let coherent_surface = self.atlas.is_some()
                        && y <= density_top
                        && y > density_top - ATLAS_SURFACE_MANTLE;
                    let solid = Self::lat_density(&lat, lx, y, lz) > 0.0
                        || coherent_surface
                        || y <= armor_y;
                    let b = if solid {
                        self.rock_at(
                            y,
                            &bands,
                            Self::lat_density(&lat_g, lx, y, lz),
                            vol,
                            dike,
                            geology,
                        )
                    } else if y <= fill_y {
                        self.water
                    } else {
                        AIR
                    };
                    c.set(lx as usize, y as usize, lz as usize, b);
                }
            }
        }

        ShapedTerrain { chunk: c, columns: ShapeColumns { top: shape_top, fills, armors } }
    }
}
