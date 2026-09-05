//! Living soil: fertility carried in farmland's meta byte.
//!
//! Encoding — bits 0..=5 hold fertility 0..=63, bits 6..=7 hold the
//! rotation stamp (the family of the last crop matured here; 0 =
//! rested or never cropped). The byte already persists with the
//! chunk and rides the edit log to guests, so the whole system costs
//! no new storage and no protocol.

use super::*;

/// Full fertility (six bits).
pub const FERT_MAX: u8 = 63;
/// Drained by a crop reaching its final stage on rested soil.
pub const FERT_DRAIN: u8 = 6;
/// The same family again takes half as much more; rotation takes less.
pub const FERT_DRAIN_MONO: u8 = 9;
pub const FERT_DRAIN_ROTATED: u8 = 4;
/// Freshly tilled grass and dirt; sand neighbors cost a step.
pub const FERT_TILL_GRASS: u8 = 42;
pub const FERT_TILL_DIRT: u8 = 27;
pub const FERT_SAND_PENALTY: u8 = 10;
/// Fallow recovery per winning random tick (winter doubles it).
pub const FERT_FALLOW: u8 = 2;
/// The fertilizer bag: dung from the pen, guano from the cave,
/// compost from the heap.
pub const DUNG_FEED: u8 = 12;
pub const GUANO_FEED: u8 = 16;
pub const COMPOST_FEED: u8 = 8;
/// A heap this full starts to cook.
pub const COMPOST_FULL: u8 = 8;

/// How much soil a fertilizer item is worth (0 = not a fertilizer).
pub fn fertilizer_value(name: &str) -> u8 {
    match name {
        "base:dung" => DUNG_FEED,
        "base:guano" => GUANO_FEED,
        "base:compost" => COMPOST_FEED,
        _ => 0,
    }
}

/// What a compost heap accepts, and how much of it one item fills.
pub fn compost_value(name: &str) -> u8 {
    match name {
        "base:spoiled_mush" => 3,
        "base:leaf_litter" => 2,
        "base:plant_fiber"
        | "base:berry"
        | "base:jungle_fruit"
        | "base:cactus_fruit"
        | "base:kelp"
        | "base:rainbell_dew"
        | "base:cairnbloom_flower"
        | "base:ashlace_tissue"
        | "base:pilgrim_root_cutting"
        | "base:lantern_reed_pith"
        | "base:nightglass_pod"
        | "base:frostlace_frond"
        | "base:tidekelp_blade"
        | "base:echo_cap_ring"
        | "base:ember_petal" => 1,
        n if n.ends_with("seeds") => 1,
        _ => 0,
    }
}

#[inline]
pub fn fert_of(meta: u8) -> u8 {
    meta & FERT_MAX
}

#[inline]
pub fn family_of(meta: u8) -> u8 {
    meta >> 6
}

#[inline]
pub fn soil_meta(fert: u8, family: u8) -> u8 {
    fert.min(FERT_MAX) | (family.min(3) << 6)
}

/// Growth multiplier for a crop on soil of this fertility: exhausted
/// dust crawls at 0.4x, full loam runs 1.6x.
#[inline]
pub fn fert_mult(fert: u8) -> f32 {
    0.4 + 1.2 * (fert as f32 / FERT_MAX as f32)
}

/// The soil byte after a crop of `family` matures on it: monoculture
/// drains hardest, rotation gentlest, and the new family is stamped.
pub fn soil_after_harvest(meta: u8, family: u8) -> u8 {
    let prev = family_of(meta);
    let drain = if prev == 0 {
        FERT_DRAIN
    } else if prev == family {
        FERT_DRAIN_MONO
    } else {
        FERT_DRAIN_ROTATED
    };
    soil_meta(fert_of(meta).saturating_sub(drain), family)
}

impl World {
    pub fn fertility_at_pos(&self, pos: BlockPos) -> u8 {
        super::TerrainRead::fertility_at_pos(self, pos)
    }

    /// Fertility of the soil block at a position (0 for non-soil).
    #[cfg(test)]
    pub fn fertility_at(&self, x: i32, y: i32, z: i32) -> u8 {
        if self.reg.block(self.get_block(x, y, z)).fert_tiles.is_some() {
            fert_of(self.get_meta(x, y, z))
        } else {
            0
        }
    }

    /// The meta byte a fresh till deserves: grass-fed loam beats bare
    /// dirt, and sand at the field's edge costs a step.
    #[cfg(test)]
    pub fn till_meta(&self, x: i32, y: i32, z: i32) -> u8 {
        let Some(pos) = BlockPos::of_world(x, y, z) else {
            return 0;
        };
        self.till_meta_at(pos)
    }

    pub fn till_meta_at(&self, pos: BlockPos) -> u8 {
        let name = self.reg.block(self.get_block_at(pos)).name.clone();
        let base = self.planet_atlas.as_ref().map_or_else(
            || {
                if name == "base:grass" {
                    FERT_TILL_GRASS
                } else {
                    FERT_TILL_DIRT
                }
            },
            |atlas| {
                let sample = atlas.biome_sample(pos.surface());
                let baseline = (u16::from(sample.fertility) * u16::from(FERT_MAX) / 255) as u8;
                if name == "base:grass" {
                    baseline.saturating_add(6).min(FERT_MAX)
                } else {
                    baseline
                }
            },
        );
        let sandy = [(1, 0), (-1, 0), (0, 1), (0, -1)]
            .iter()
            .filter_map(|&(du, dv)| pos.offset(du, 0, dv))
            .any(|neighbor| {
                self.reg
                    .block(self.get_block_at(neighbor))
                    .name
                    .contains("sand")
            });
        soil_meta(
            if sandy {
                base.saturating_sub(FERT_SAND_PENALTY)
            } else {
                base
            },
            0,
        )
    }

    pub fn initialize_tilled_soil_at(&mut self, pos: BlockPos) {
        let baseline = self
            .planet_atlas
            .as_ref()
            .map_or(0, |atlas| atlas.biome_sample(pos.surface()).salinity);
        self.set_soil_salinity_at(pos, baseline);
    }

    /// Drainage and accumulated salt are independent of managed fertility.
    /// Values around 96..=205 drain well; saturated or excessively sandy soil
    /// slows crops, while severe salinity stops them.
    pub fn crop_soil_multiplier_at(&self, pos: BlockPos) -> f32 {
        let drainage = self
            .planet_atlas
            .as_ref()
            .map_or(150, |atlas| atlas.biome_sample(pos.surface()).drainage);
        let drainage_mult = if drainage < 48 {
            f32::from(drainage) / 96.0
        } else if drainage > 220 {
            (1.0 - f32::from(drainage - 220) / 70.0).clamp(0.45, 1.0)
        } else {
            1.0
        };
        let salt = self.get_soil_salinity_at(pos);
        let salt_mult = if salt >= 176 {
            0.0
        } else {
            (1.0 - f32::from(salt) / 208.0).clamp(0.15, 1.0)
        };
        drainage_mult * salt_mult
    }

    /// Moisture available to managed roots. Planetary soil storage remains
    /// the broad background; a real detailed-water voxel in an adjacent
    /// channel supplies the field locally. That voxel has already been
    /// debited from its river, aquifer, tank, or precipitation owner by the
    /// finite-water machinery, so irrigation cannot work with painted water.
    pub fn managed_soil_moisture_at(&self, pos: BlockPos) -> f32 {
        let baseline = self.soil_moisture_at_surface(pos.surface());
        let irrigated = [
            (1, 0, 0),
            (-1, 0, 0),
            (0, 0, 1),
            (0, 0, -1),
            (1, 1, 0),
            (-1, 1, 0),
            (0, 1, 1),
            (0, 1, -1),
        ]
        .into_iter()
        .filter_map(|(du, dy, dv)| pos.offset(du, dy, dv))
        .filter_map(|neighbor| self.water_mass_at(neighbor))
        .any(|mass| mass.water_hu > 0);
        if !irrigated {
            return baseline;
        }
        let drainage = self
            .planet_atlas
            .as_ref()
            .map_or(150, |atlas| atlas.biome_sample(pos.surface()).drainage);
        baseline.max(if drainage < 48 { 1.35 } else { 1.0 })
    }

    pub fn soil_failure_at(&self, pos: BlockPos) -> Option<&'static str> {
        let sample = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.biome_sample(pos.surface()));
        if self.get_soil_salinity_at(pos) >= 128 {
            Some("The soil is white with salt; fresh water and drainage must leach it.")
        } else if sample.is_some_and(|soil| soil.drainage < 42) {
            Some("The ground is waterlogged; this crop needs drainage.")
        } else if sample
            .is_some_and(|soil| soil.habitat_flags & crate::planet_atlas::HABITAT_PERMAFROST != 0)
        {
            Some("The ground is frozen too deeply for these roots.")
        } else if self.managed_soil_moisture_at(pos) < 0.18 {
            Some("The soil is dry; irrigation must bring real water.")
        } else {
            None
        }
    }

    /// Feed one item into a compost heap; the meta byte counts the
    /// fill. Returns false when the heap is full or the item isn't
    /// compostable.
    #[cfg(test)]
    pub fn compost_fill(&mut self, x: i32, y: i32, z: i32, item_name: &str) -> bool {
        let Some(pos) = BlockPos::of_world(x, y, z) else {
            return false;
        };
        self.compost_fill_at(pos, item_name)
    }

    pub fn compost_fill_at(&mut self, pos: BlockPos, item_name: &str) -> bool {
        let b = self.get_block_at(pos);
        if self.reg.block(b).name != "base:compost_heap" {
            return false;
        }
        let v = compost_value(item_name);
        let meta = self.get_meta_at(pos);
        if v == 0 || meta >= COMPOST_FULL {
            return false;
        }
        self.set_block_meta_at(pos, b, (meta + v).min(COMPOST_FULL));
        true
    }

    /// Empty a ripened heap back to a fresh one; the caller hands
    /// over the compost items.
    #[cfg(test)]
    pub fn compost_take(&mut self, x: i32, y: i32, z: i32) -> bool {
        let Some(pos) = BlockPos::of_world(x, y, z) else {
            return false;
        };
        self.compost_take_at(pos)
    }

    pub fn compost_take_at(&mut self, pos: BlockPos) -> bool {
        let b = self.get_block_at(pos);
        if self.reg.block(b).name != "base:compost_heap_ready" {
            return false;
        }
        if let Some(fresh) = self.reg.block_id("base:compost_heap") {
            self.set_block_meta_at(pos, fresh, 0);
            return true;
        }
        false
    }

    /// Feed the soil block at a position (dung, guano, compost, rot).
    /// Returns false when there's no soil there to feed.
    #[cfg(test)]
    pub fn feed_soil(&mut self, x: i32, y: i32, z: i32, amount: u8) -> bool {
        let Some(pos) = BlockPos::of_world(x, y, z) else {
            return false;
        };
        self.feed_soil_at(pos, amount)
    }

    pub fn feed_soil_at(&mut self, pos: BlockPos, amount: u8) -> bool {
        let b = self.get_block_at(pos);
        if self.reg.block(b).fert_tiles.is_none() {
            return false;
        }
        let meta = self.get_meta_at(pos);
        let fed = (fert_of(meta) + amount).min(FERT_MAX);
        // Fully rested soil forgets its last crop.
        let fam = if fed == FERT_MAX { 0 } else { family_of(meta) };
        self.set_block_meta_at(pos, b, soil_meta(fed, fam));
        true
    }
}
