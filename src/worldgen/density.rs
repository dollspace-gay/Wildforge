//! Column estimates and lattice-sampled radial density fields.

use super::Generator;
use crate::chunk::{CHUNK_Y, ChunkPos};
use crate::planet::SurfacePos;

impl Generator {
    #[cfg(test)]
    pub(super) fn column_params(&self, wx: i32, wz: i32) -> (f32, f32) {
        let cl = self.climate(wx, wz);
        let pre = self.base_offset(wx, wz, &cl);
        let (carve, _, _) = self.hydro_raw(wx, wz, &cl, pre);
        let mut offset = (pre - carve).clamp(6.0, CHUNK_Y as f32 - 22.0);
        // A volcano stamps its cone onto the spline terrain, crater
        // bowl and all.
        if let Some(v) = self.volcano_near(wx, wz) {
            offset = (offset + v.cone(wx, wz)).min(CHUNK_Y as f32 - 18.0);
        }
        (offset, self.factor_spline.at(cl.e))
    }

    pub(super) fn column_params_at(&self, pos: SurfacePos) -> (f32, f32) {
        let cl = self.climate_at(pos);
        let pre = self.base_offset_at(pos, &cl);
        let (carve, _, _) = self.hydro_raw_at(pos, &cl, pre);
        (
            (pre - carve).clamp(6.0, CHUNK_Y as f32 - 22.0),
            self.factor_spline.at(cl.e),
        )
    }

    /// Cheap surface estimate (spline offset) for spawn search and tooling.
    #[cfg(test)]
    pub fn surface_estimate(&self, wx: i32, wz: i32) -> i32 {
        self.column_params(wx, wz).0 as i32
    }

    /// Seam-safe cheap surface estimate for planetary spawn search and
    /// diagnostics.
    pub fn surface_estimate_at(&self, pos: SurfacePos) -> i32 {
        self.column_params_at(pos).0 as i32
    }

    /// Atlas terrain plus bounded voxel-scale detail, before the temporary
    /// river/lake pass carves channels.  Kept test-only so geology tests can
    /// verify the fine relief envelope without conflating it with hydrology.
    #[cfg(test)]
    pub fn pre_hydrology_surface_estimate_at(&self, pos: SurfacePos) -> f32 {
        let climate = self.climate_at(pos);
        self.base_offset_at(pos, &climate)
    }

    /// Highest solid crossing of the production density field before caves,
    /// hydrology, and surface materials. This directly verifies that voxel
    /// shaping stays inside the atlas relief envelope.
    #[cfg(test)]
    pub fn density_surface_estimate_at(&self, pos: SurfacePos) -> i32 {
        let climate = self.climate_at(pos);
        let offset = self.base_offset_at(pos, &climate);
        let factor = self.factor_spline.at(climate.e);
        (1..CHUNK_Y as i32)
            .rev()
            .find(|height| self.density_at_planet(pos, f64::from(*height), offset, factor) > 0.0)
            .unwrap_or(0)
    }

    pub(super) fn density_at_planet(
        &self,
        pos: SurfacePos,
        y: f64,
        offset: f32,
        factor: f32,
    ) -> f32 {
        let mut noise = 0.0;
        let mut amplitude = 1.0;
        let mut frequency = 1.0;
        for octave in &self.base3d {
            noise += f64::from(Self::radial_noise_at(
                octave,
                pos,
                y,
                171.0 / frequency,
                [0.0, 0.0, 0.0],
            )) * amplitude;
            frequency *= 2.0;
            amplitude *= 0.5;
        }
        let noise = (noise / 1.75) as f32;
        let dy = offset - y as f32;
        let slope = if self.atlas.is_some() {
            // The atlas already owns ranges, trenches, rifts, and volcanoes.
            // Production 3D density is only the bounded voxel-scale skin: a
            // sufficiently steep vertical trend prevents a second surface or
            // floating shelf tens of blocks from the committed elevation.
            if dy < 0.0 {
                (0.09 + factor * 0.004).clamp(0.095, 0.125)
            } else {
                (0.12 + factor * 0.005).clamp(0.128, 0.16)
            }
        } else if dy < 0.0 {
            factor * 0.011
        } else {
            factor.max(3.0) * 0.026
        };
        noise * 0.62 + dy * slope
    }

    /// Sample density on a 4x8x4 lattice covering the chunk plus a 4-block
    /// apron, so border columns interpolate identically to their neighbors.
    /// The second channel is the granite intrusion margin: distance past
    /// the (depth-loosening) pluton threshold, baked in so interpolation
    /// carries the widening-with-depth shape for free.
    pub(super) fn sample_lattice(&self, pos: ChunkPos) -> (Vec<f32>, Vec<f32>) {
        const NX: usize = 7; // x/z: -4, 0, 4, 8, 12, 16, 20
        const NY: usize = CHUNK_Y / 8 + 1;
        let mut lat = vec![0f32; NX * NX * NY];
        let mut lat_g = vec![0f32; NX * NX * NY];
        for ix in 0..NX {
            for iz in 0..NX {
                let surface = Self::surface_in_chunk(pos, ix as i32 * 4 - 4, iz as i32 * 4 - 4);
                let (offset, factor) = self.column_params_at(surface);
                // Batholith provinces: a coarse gate over the pluton
                // noise. Inside a province intrusions abound; outside,
                // the threshold climbs out of reach — granite country
                // is a REGION you travel to (economy plan, leg 1),
                // not a backyard given.
                let prov = Self::radial_noise_at(
                    &self.geography.granite3d,
                    surface,
                    77.7,
                    1_400.0,
                    [0.0; 3],
                );
                let prov_pen = (0.44 - prov).max(0.0) * 1.8;
                for iy in 0..NY {
                    let y = (iy * 8) as f64;
                    let i = (ix * NX + iz) * NY + iy;
                    lat[i] = self.density_at_planet(surface, y, offset, factor);
                    if let Some(atlas) = &self.atlas {
                        lat_g[i] = atlas.intrusion_margin(surface.center(), y as f32);
                    } else {
                        let g = Self::radial_noise_at(
                            &self.geography.granite3d,
                            surface,
                            y,
                            230.0,
                            [0.0; 3],
                        );
                        // Legacy fixture path; production intrusions come
                        // exclusively from the persisted geological manifest.
                        let thr = 0.55 + prov_pen + y as f32 * 0.0012;
                        lat_g[i] = g - thr;
                    }
                }
            }
        }
        (lat, lat_g)
    }

    /// Trilinear interpolation of the lattice at block coords relative to the
    /// chunk origin (lx/lz may be -1..=16 for the apron ring).
    pub(super) fn lat_density(lat: &[f32], lx: i32, y: i32, lz: i32) -> f32 {
        const NX: usize = 7;
        const NY: usize = CHUNK_Y / 8 + 1;
        let fx = (lx + 4) as f32 / 4.0;
        let fz = (lz + 4) as f32 / 4.0;
        let fy = y as f32 / 8.0;
        let (ix, iy, iz) = (fx as usize, fy as usize, fz as usize);
        let (ix1, iy1, iz1) = (
            (ix + 1).min(NX - 1),
            (iy + 1).min(NY - 1),
            (iz + 1).min(NX - 1),
        );
        let (tx, ty, tz) = (fx - ix as f32, fy - iy as f32, fz - iz as f32);
        let g = |x: usize, z: usize, y: usize| lat[(x * NX + z) * NY + y];
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        let c00 = lerp(g(ix, iz, iy), g(ix1, iz, iy), tx);
        let c01 = lerp(g(ix, iz, iy1), g(ix1, iz, iy1), tx);
        let c10 = lerp(g(ix, iz1, iy), g(ix1, iz1, iy), tx);
        let c11 = lerp(g(ix, iz1, iy1), g(ix1, iz1, iy1), tx);
        lerp(lerp(c00, c10, tz), lerp(c01, c11, tz), ty)
    }
}
