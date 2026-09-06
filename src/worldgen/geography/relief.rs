//! Continental and tectonic relief before hydrology.

use super::super::Climate;
use super::Geography;
use crate::planet::SurfacePos;
#[cfg(test)]
use noise::NoiseFn;

impl Geography {
    /// Spline-driven terrain parameters for a column: (offset, factor).
    /// Plate-driven relief for a column: fold ranges where continents
    /// collide, coastal ranges and offshore trenches at subduction
    /// zones, sunken valleys where plates part. Positive adds height,
    /// negative digs.
    pub(crate) fn plate_relief(&self, cl: &Climate) -> f32 {
        let tec = &cl.tec;
        let land = ((cl.c + 0.15) / 0.35).clamp(0.0, 1.0);
        let belt = (-(tec.boundary_dist / 80.0).powi(2)).exp();
        if tec.convergence > 0.12 {
            if !tec.oceanic && !tec.neighbor_oceanic {
                // Continent meets continent: the big fold ranges,
                // crests rippling along the boundary.
                let ripple = 0.8 + 0.2 * (tec.along / 90.0).sin();
                tec.convergence * 115.0 * belt * ripple * land
            } else if tec.oceanic {
                // The diving side dips into a trench offshore.
                -14.0 * belt * tec.convergence
            } else {
                // Subduction throws a coastal range on the overriding
                // plate (its volcano arc is weighted separately).
                tec.convergence * 70.0 * belt * land
            }
        } else if tec.convergence < -0.12 {
            // Rift valley: the land sags where plates part.
            tec.convergence * 16.0 * belt
        } else {
            0.0
        }
    }

    /// Terrain offset before hydrology: continents, worn highlands,
    /// and plate relief.
    /// Hot, dry, rugged inland climate: mesa country.
    pub(in crate::worldgen) fn is_badlands(cl: &Climate) -> bool {
        cl.t > 0.7 && cl.h < -0.4 && cl.c > 0.1
    }

    #[cfg(test)]
    pub(in crate::worldgen) fn base_offset(&self, wx: i32, wz: i32, cl: &Climate) -> f32 {
        let base = self.offset_base.at(cl.c);
        // Old erosion mountains stay as worn highlands; the young
        // dramatic ranges belong to the plate boundaries now.
        let land = ((cl.c + 0.15) / 0.35).clamp(0.0, 1.0);
        let mtn = self.mountain_amp.at(cl.e) * (0.35 + 0.65 * cl.r) * land * 0.45;
        let mut off = base + mtn + self.plate_relief(cl);
        if Self::is_badlands(cl) {
            // Stepped mesas: quantized plateaus whose bare walls show
            // the sandstone banding.
            let m = self.detail.get([wx as f64 / 140.0, wz as f64 / 140.0]) as f32;
            off += ((m * 3.0).floor().clamp(0.0, 2.0)) * 11.0;
        }
        off
    }

    pub(in crate::worldgen) fn base_offset_at(&self, pos: SurfacePos, cl: &Climate) -> f32 {
        if let Some(atlas) = &self.atlas {
            let terrain = atlas.terrain_sample(pos.center());
            let baseline = terrain.eroded_elevation - atlas.sampled_volcanic_relief(pos.center())
                + atlas.exact_volcanic_relief(pos.center());
            let fine_detail = Self::noise_at(&self.detail, pos, 115.0, [0.0, 0.0, 0.0]) * 2.5;
            return baseline + fine_detail;
        }
        let base = self.offset_base.at(cl.c);
        let land = ((cl.c + 0.15) / 0.35).clamp(0.0, 1.0);
        let mtn = self.mountain_amp.at(cl.e) * (0.35 + 0.65 * cl.r) * land * 0.45;
        let mut off = base + mtn + self.plate_relief(cl);
        if Self::is_badlands(cl) {
            let mesa = Self::noise_at(&self.detail, pos, 140.0, [0.0, 0.0, 0.0]);
            off += ((mesa * 3.0).floor().clamp(0.0, 2.0)) * 11.0;
        }
        off
    }
}
