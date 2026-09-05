//! Stratigraphic bands, bedrock families, and contact metamorphism.

use super::{Climate, Generator};
use crate::planet::SurfacePos;
use crate::registry::BlockId;

impl Generator {
    pub(super) fn strata_bands_at(&self, pos: SurfacePos, cl: &Climate) -> [i32; 5] {
        let w1 = Self::noise_at(&self.bandwarp, pos, 260.0, [0.0, 0.0, 0.0]);
        let w2 = Self::noise_at(&self.bandwarp, pos, 170.0, [7.3, -2.1, 4.7]);
        if let Some(atlas) = &self.atlas {
            let geology = atlas.geology_sample(pos.center());
            let base = match geology.stratigraphic_stack {
                2 => [8.0, 29.0, 47.0, 68.0, 94.0],
                3 => [8.0, 36.0, 57.0, 76.0, 98.0],
                4 => [8.0, 31.0, 49.0, 71.0, 101.0],
                5 => [8.0, 27.0, 45.0, 66.0, 88.0],
                6 => [8.0, 34.0, 55.0, 76.0, 104.0],
                7 => [8.0, 42.0, 62.0, 79.0, 96.0],
                _ => [8.0, 34.0, 50.0, 68.0, 92.0],
            };
            // Equal-distance lines from a connected boundary run are parallel
            // to its strike, so this phase produces coherent fold trains on
            // both sides without relying on a planar world axis.
            let fold = if geology.boundary
                == crate::planet_atlas::DetailedBoundary::ContinentalCollision
            {
                let envelope = (-(geology.boundary_distance_blocks / 190.0).powi(2)).exp();
                geology.boundary_strength
                    * envelope
                    * 20.0
                    * (geology.boundary_distance_blocks / 28.0 + w1).sin()
            } else {
                0.0
            };
            return [
                (base[0] + w1 * 3.0) as i32,
                (base[1] + w1 * 6.0 + fold * 0.35) as i32,
                (base[2] + w2 * 5.0 + fold) as i32,
                (base[3] + w1 * 6.0 + fold) as i32,
                (base[4] + w2 * 8.0 + fold * 0.6) as i32,
            ];
        }
        let tec = &cl.tec;
        let fold = if tec.convergence > 0.12 && !tec.oceanic && !tec.neighbor_oceanic {
            let belt = (-(tec.boundary_dist / 110.0).powi(2)).exp();
            tec.convergence * belt * 26.0 * (tec.along / 24.0 + w1).sin()
        } else {
            0.0
        };
        let mesa = if Self::is_badlands(cl) { 42.0 } else { 0.0 };
        [
            (8.0 + w1 * 3.0) as i32,
            (34.0 + w1 * 7.0 + fold * 0.5) as i32,
            (50.0 + w2 * 5.0 + cl.h * 5.0 + fold) as i32,
            (68.0 + w1 * 6.0 + fold) as i32,
            (92.0 + w2 * 9.0 - cl.h * 6.0 + fold + mesa) as i32,
        ]
    }

    #[cfg(test)]
    pub fn strata_bands_probe(&self, pos: SurfacePos) -> [i32; 5] {
        let climate = self.climate_at(pos);
        self.strata_bands_at(pos, &climate)
    }

    /// The rock for a solid cell: volcanoes build in basalt (with
    /// carbonatite dikes threading their plumbing), granite
    /// intrusions override the stack, their contact halo cooks the
    /// sediment it touches, and the bands decide the rest.
    pub(super) fn bedrock_block(&self, family: crate::planet_atlas::BedrockFamily) -> BlockId {
        use crate::planet_atlas::BedrockFamily as B;
        match family {
            B::Sandstone | B::Evaporite => self.sandstone,
            B::Limestone => self.limestone,
            B::Shale => self.shale,
            B::Granite => self.granite,
            B::Marble => self.marble,
            B::Slate => self.slate,
            B::Quartzite => self.quartzite,
            B::Basalt | B::Ultramafic => self.basalt,
            B::MixedBasement => self.stone,
        }
    }

    pub(super) fn rock_at(
        &self,
        y: i32,
        bands: &[i32; 5],
        gm: f32,
        vol: f32,
        dike: bool,
        geology: Option<crate::planet_atlas::AtlasGeologySample>,
    ) -> BlockId {
        if vol > 0.24 && y > bands[1] {
            return if dike { self.carbonatite } else { self.basalt };
        }
        if gm > 0.0 {
            return self.granite;
        }
        if let Some(sample) = geology
            && y < bands[1]
        {
            return self.bedrock_block(sample.bedrock);
        }
        let sediment = if y < bands[0] {
            return self.basalt;
        } else if y < bands[1] {
            return self.stone;
        } else if y < bands[2] {
            self.shale
        } else if y < bands[3] {
            self.limestone
        } else if y < bands[4] {
            self.sandstone
        } else {
            return self.stone;
        };
        // Contact metamorphism: close enough to a pluton to bake.
        if gm > -0.08 || geology.is_some_and(|sample| sample.metamorphic_grade >= 2) {
            if sediment == self.shale {
                return self.slate;
            }
            if sediment == self.limestone {
                return self.marble;
            }
            return self.quartzite;
        }
        sediment
    }

}
