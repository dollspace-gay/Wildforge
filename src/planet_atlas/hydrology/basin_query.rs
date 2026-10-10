//! Exact basin identities without unused pointwise channel geometry.

use std::collections::BTreeMap;

use super::{HYDRO_RIVER, WaterBodyKind};
use crate::planet::SurfacePos;
use crate::planet_atlas::{AtlasPos, PlanetAtlas};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct HydrologyBasinIds {
    ocean_basin_id: u16,
    lake_basin_id: u32,
}

impl HydrologyBasinIds {
    pub(crate) const fn ocean_basin_id(self) -> u16 {
        self.ocean_basin_id
    }

    pub(crate) const fn lake_basin_id(self) -> u32 {
        self.lake_basin_id
    }
}

#[derive(Clone, Copy, Debug)]
enum CellBasinClassification {
    Uniform(HydrologyBasinIds),
    Pointwise,
}

/// A read-only transaction's basin queries against one immutable atlas.
///
/// Only classifications proven uniform over an entire atlas cell are cached.
/// Mixed river/basin data retains the full sampler at every voxel center.
#[derive(Debug)]
pub(crate) struct HydrologyBasinQuery<'a> {
    atlas: &'a PlanetAtlas,
    cells: BTreeMap<AtlasPos, CellBasinClassification>,
}

impl<'a> HydrologyBasinQuery<'a> {
    pub(super) fn new(atlas: &'a PlanetAtlas) -> Self {
        Self {
            atlas,
            cells: BTreeMap::new(),
        }
    }

    /// Return the exact basin IDs from `hydrology_sample(surface.center())`.
    pub(crate) fn at(&mut self, surface: SurfacePos) -> HydrologyBasinIds {
        // A voxel center has the same integral home address as `surface`;
        // the full sampler's floor/clamp does not move a canonical center.
        let home = self.atlas.atlas_pos(surface);
        let classification = *self
            .cells
            .entry(home)
            .or_insert_with(|| classify_cell(self.atlas, home));
        match classification {
            CellBasinClassification::Uniform(ids) => ids,
            CellBasinClassification::Pointwise => {
                let sample = self.atlas.hydrology_sample(surface.center());
                HydrologyBasinIds {
                    ocean_basin_id: sample.ocean_basin_id,
                    lake_basin_id: sample.lake_basin_id,
                }
            }
        }
    }
}

fn classify_cell(atlas: &PlanetAtlas, home: AtlasPos) -> CellBasinClassification {
    let home_cell = atlas
        .genesis
        .hydrology
        .get(home)
        .expect("validated hydrology query");
    if matches!(
        home_cell.water_body,
        WaterBodyKind::Ocean | WaterBodyKind::Lake | WaterBodyKind::Playa
    ) {
        // These are exactly the full sampler's early-return classifications.
        return CellBasinClassification::Uniform(HydrologyBasinIds {
            ocean_basin_id: home_cell.ocean_basin_id,
            lake_basin_id: home_cell.lake_basin_id,
        });
    }

    for candidate in atlas.bounded_stencil(home, 2, 32) {
        let cell = atlas
            .genesis
            .hydrology
            .get(candidate)
            .expect("hydrology stencil");
        if cell.flags & HYDRO_RIVER != 0 && (cell.ocean_basin_id != 0 || cell.lake_basin_id != 0) {
            // Generation separates rivers from these basins, but loaded
            // data need not enforce that invariant. Keep the original
            // candidate order, score, discharge tie, and seam geometry.
            return CellBasinClassification::Pointwise;
        }
    }
    // The full sampler starts with zero basin IDs and can replace them only
    // from a river candidate. Every possible replacement here is also zero,
    // regardless of which pointwise score wins (or whether any does).
    CellBasinClassification::Uniform(HydrologyBasinIds::default())
}

#[cfg(test)]
#[path = "basin_query_tests.rs"]
mod tests;
