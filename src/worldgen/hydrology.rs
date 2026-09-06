//! Raw waterlines and seam-consistent channel sealing.

use super::{Climate, Generator};
use crate::chunk::SEA_LEVEL;
use crate::planet::{Direction4, SurfacePos, step4};
#[cfg(test)]
use noise::NoiseFn;

impl Generator {
    /// Raw waterline math for one column, before sealing: the carve,
    /// the candidate water level, and whether the column sits close
    /// enough to a channel or lake basin that sealing must look at it
    /// (gates the neighbor probes — the margins cover the one-block
    /// noise gradient to the true water zones).
    #[cfg(test)]
    pub(super) fn hydro_raw(
        &self,
        wx: i32,
        wz: i32,
        cl: &Climate,
        pre: f32,
    ) -> (f32, Option<i32>, bool) {
        let mut carve = 0.0f32;
        let mut level: Option<i32> = None;
        let mut near = false;
        if pre > SEA_LEVEL as f32 - 2.0 && cl.c > -0.05 {
            let riv = self.rivernoise.get([wx as f64 / 620.0, wz as f64 / 620.0]) as f32;
            let w = 0.012 + 0.010 * (0.6 - cl.c).clamp(0.0, 1.0);
            let shoulder = w * 3.2;
            if riv.abs() < shoulder {
                near = true;
                let t = 1.0 - riv.abs() / shoulder;
                carve += t * t * 8.0;
                if riv.abs() < w {
                    carve += 3.0;
                    // Terraced reaches: quantize the fill so each
                    // stretch of river is dead level, dropping in
                    // discrete falls; on steep runs the level lands
                    // at or under the channel floor and the stretch
                    // stays a dry wash between step pools.
                    let floor = (pre - carve) as i32;
                    let f = floor + 3;
                    let f = f - f.rem_euclid(4);
                    if f > floor {
                        level = Some(f);
                    }
                }
            }
            let lk = self.lakenoise.get([wx as f64 / 300.0, wz as f64 / 300.0]) as f32;
            if lk > 0.56 {
                near = true;
            }
            if lk > 0.58 && pre > SEA_LEVEL as f32 + 2.0 && pre < 120.0 {
                let t = ((lk - 0.58) / 0.42).min(1.0);
                carve += t * 10.0;
                let f2 = (pre - 2.0) as i32;
                let f2 = f2 - f2.rem_euclid(4);
                level = Some(level.map_or(f2, |f| f.max(f2)));
            }
        }
        (carve, level, near)
    }

    pub(super) fn hydro_raw_at(
        &self,
        pos: SurfacePos,
        cl: &Climate,
        pre: f32,
    ) -> (f32, Option<i32>, bool) {
        if let Some(atlas) = &self.atlas {
            let water = atlas.hydrology_sample(pos.center());
            if !water.near_channel {
                return (0.0, None, false);
            }
            let half_width = (water.channel_width_blocks * 0.5).max(0.6);
            let shoulder = (half_width * 2.6).max(2.0);
            let profile = if water.channel_distance_blocks <= half_width {
                1.0
            } else {
                (1.0 - (water.channel_distance_blocks - half_width)
                    / (shoulder - half_width).max(0.1))
                .clamp(0.0, 1.0)
                .powi(2)
            };
            let carve = (pre - water.channel_bed_elevation).max(0.0) * profile;
            let fill = water
                .water_surface_elevation
                .map(|elevation| elevation.floor() as i32)
                .filter(|surface| *surface > (pre - carve).floor() as i32);
            return (carve, fill, true);
        }
        let mut carve = 0.0f32;
        let mut level = None;
        let mut near = false;
        if pre > SEA_LEVEL as f32 - 2.0 && cl.c > -0.05 {
            let riv = Self::noise_at(&self.rivernoise, pos, 620.0, [0.0, 0.0, 0.0]);
            let width = 0.012 + 0.010 * (0.6 - cl.c).clamp(0.0, 1.0);
            let shoulder = width * 3.2;
            if riv.abs() < shoulder {
                near = true;
                let t = 1.0 - riv.abs() / shoulder;
                carve += t * t * 8.0;
                if riv.abs() < width {
                    carve += 3.0;
                    let floor = (pre - carve) as i32;
                    let fill = floor + 3;
                    let fill = fill - fill.rem_euclid(4);
                    if fill > floor {
                        level = Some(fill);
                    }
                }
            }
            let lake = Self::noise_at(&self.lakenoise, pos, 300.0, [0.0, 0.0, 0.0]);
            if lake > 0.56 {
                near = true;
            }
            if lake > 0.58 && pre > SEA_LEVEL as f32 + 2.0 && pre < 120.0 {
                let t = ((lake - 0.58) / 0.42).min(1.0);
                carve += t * 10.0;
                let fill = (pre - 2.0) as i32;
                let fill = fill - fill.rem_euclid(4);
                level = Some(level.map_or(fill, |old: i32| old.max(fill)));
            }
        }
        (carve, level, near)
    }

    /// Rivers and lakes for a column: how deep the water has cut the
    /// terrain, the fill level (a river or lake acts as a local sea
    /// level in the shape pass), and the armor level. Every pool is
    /// sealed by construction: a column whose raw water level drops on
    /// any side becomes a rock weir instead of water, and a dry column
    /// beside water is armored — its non-solid cells below the tallest
    /// adjacent pool become native rock, so 3D-noise wobble and the
    /// shoulder carve can never leave a bank below the waterline. A
    /// woken pool has nowhere to shed: no thin films creeping over the
    /// sand, no floating shelves meeting edge-on. All decisions read
    /// only raw per-column math, so chunks agree without communication.
    #[cfg(test)]
    pub fn hydrology(
        &self,
        wx: i32,
        wz: i32,
        cl: &Climate,
        pre: f32,
    ) -> (f32, Option<i32>, Option<i32>) {
        let (carve, level, near) = self.hydro_raw(wx, wz, cl, pre);
        if !near {
            return (carve, None, None);
        }
        let mut step_down = false;
        let mut tallest: Option<i32> = None;
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, nz) = (wx + dx, wz + dz);
            let ncl = self.climate(nx, nz);
            let npre = self.base_offset(nx, nz, &ncl);
            let (_, nlevel, _) = self.hydro_raw(nx, nz, &ncl, npre);
            match (nlevel, level) {
                (Some(nf), Some(f)) if nf < f => step_down = true,
                (Some(nf), None) => tallest = Some(tallest.map_or(nf, |t: i32| t.max(nf))),
                _ => {}
            }
        }
        match level {
            Some(f) if step_down => (carve, None, Some(f)),
            Some(f) => (carve, Some(f), None),
            None => (carve, None, tallest),
        }
    }

    pub(super) fn hydrology_at(
        &self,
        pos: SurfacePos,
        cl: &Climate,
        pre: f32,
    ) -> (f32, Option<i32>, Option<i32>) {
        let (carve, level, near) = self.hydro_raw_at(pos, cl, pre);
        if !near {
            return (carve, None, None);
        }
        let mut step_down = false;
        let mut tallest = None;
        for direction in [
            Direction4::East,
            Direction4::North,
            Direction4::West,
            Direction4::South,
        ] {
            let neighbor = step4(pos, direction).pos;
            let ncl = self.climate_at(neighbor);
            let npre = self.base_offset_at(neighbor, &ncl);
            let (_, nlevel, _) = self.hydro_raw_at(neighbor, &ncl, npre);
            match (nlevel, level) {
                (Some(nf), Some(f)) if nf < f => step_down = true,
                (Some(nf), None) => tallest = Some(tallest.map_or(nf, |t: i32| t.max(nf))),
                _ => {}
            }
        }
        match level {
            Some(fill) if step_down => (carve, None, Some(fill)),
            Some(fill) => (carve, Some(fill), None),
            None => (carve, None, tallest),
        }
    }

    /// The local water level a river or lake gives a column, if any
    /// (tests and tooling; generate() computes the same inline).
    #[cfg(test)]
    pub fn water_features(&self, wx: i32, wz: i32) -> Option<i32> {
        let cl = self.climate(wx, wz);
        let pre = self.base_offset(wx, wz, &cl);
        self.hydrology(wx, wz, &cl, pre).1
    }

    #[cfg(test)]
    pub fn water_features_at(&self, pos: SurfacePos) -> Option<i32> {
        let climate = self.climate_at(pos);
        let pre = self.base_offset_at(pos, &climate);
        self.hydrology_at(pos, &climate, pre).1
    }
}
