//! Legacy fixture volcano and hydrology probes.

use super::{Generator, Volcano, hash2};
use crate::planet::{Face, SurfacePos};

impl Generator {
    #[cfg(test)]
    pub fn pluton_at(&self, wx: i32, wz: i32) -> bool {
        SurfacePos::from_centered(Face::PosZ, wx, wz).is_ok_and(|pos| self.pluton_at_surface(pos))
    }

    /// The armor level sealing a column, if any (tests and tooling).
    #[cfg(test)]
    pub fn armor_at(&self, wx: i32, wz: i32) -> Option<i32> {
        let cl = self.climate(wx, wz);
        let pre = self.base_offset(wx, wz, &cl);
        self.hydrology(wx, wz, &cl, pre).2
    }

    /// The volcano whose reach covers a column, if any: deterministic
    /// per region cell, so every chunk agrees without communication.
    /// Land and coastal shelves only — volcanic islands are welcome,
    /// the deep ocean floor is not.
    #[cfg(test)]
    pub fn volcano_near(&self, wx: i32, wz: i32) -> Option<Volcano> {
        const REGION: i32 = 384;
        let rx = wx.div_euclid(REGION);
        let rz = wz.div_euclid(REGION);
        for dx in -1..=1 {
            for dz in -1..=1 {
                let (cx, cz) = (rx + dx, rz + dz);
                let h = hash2(self.seed ^ 0x70_1ca0, cx, cz);
                let margin = 90;
                let ox = (h >> 8) % (REGION - 2 * margin) as u32 + margin as u32;
                let oz = (h >> 17) % (REGION - 2 * margin) as u32 + margin as u32;
                let center_x = cx * REGION + ox as i32;
                let center_z = cz * REGION + oz as i32;
                let v = Volcano {
                    x: center_x,
                    z: center_z,
                    radius: 44.0 + (h % 28) as f32,
                    height: 52.0 + ((h >> 4) % 32) as f32,
                };
                // Distance first: this runs for every column of every
                // chunk, and almost every candidate is out of reach —
                // nothing heavier than hashes may run before this line.
                // (An earlier version computed full climate per
                // candidate and singlehandedly tanked worldgen.)
                let d = v.dist(wx, wz);
                if d >= v.radius + 12.0 {
                    continue;
                }
                // Volcanoes follow the plate map: subduction arcs run
                // thick with them, rifts leak a few, plate interiors
                // almost none. Tectonics is hash-and-math (no perlin);
                // the deep-ocean gate rides the crust kind, which is
                // what continentalness mostly is anyway.
                let tec = self.tectonics(center_x, center_z);
                if tec.oceanic && tec.boundary_dist > 260.0 {
                    continue; // abyssal plate interior: no hotspots
                }
                let subduction = tec.boundary_dist < 260.0
                    && tec.convergence > 0.1
                    && (tec.oceanic || tec.neighbor_oceanic);
                let rift = tec.boundary_dist < 220.0 && tec.convergence < -0.1;
                // Regional-band odds (economy plan): volcanic arcs
                // stay volcanic, plate interiors go quiet — volcanic
                // goods (carbonatite, obsidian, sulfur) are what arc
                // country trades away.
                let odds = if subduction {
                    2
                } else if rift {
                    8
                } else {
                    48
                };
                if !h.is_multiple_of(odds) {
                    continue;
                }
                return Some(v);
            }
        }
        None
    }

}
