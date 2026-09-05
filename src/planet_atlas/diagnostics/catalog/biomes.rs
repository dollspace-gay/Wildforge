//! Registered biomes diagnostic maps in export order.


use super::{LayerKind, LayerSpec, layer};
pub(super) const LAYERS: &[LayerSpec] = &[
    layer(
        "soil_parent_material",
        "soil parent family",
        LayerKind::Categorical,
    ),
    layer(
        "aquifer_capacity",
        "baseline aquifer capacity",
        LayerKind::Scalar,
    ),
    layer(
        "aquifer_permeability",
        "normalized aquifer permeability",
        LayerKind::Scalar,
    ),
    layer(
        "porosity",
        "normalized primary porosity seed",
        LayerKind::Scalar,
    ),
    layer(
        "groundwater_head",
        "baseline groundwater head",
        LayerKind::Scalar,
    ),
    layer("soil_depth", "soil depth in decimeters", LayerKind::Scalar),
    layer("soil_sand", "soil sand fraction", LayerKind::Scalar),
    layer("soil_silt", "soil silt fraction", LayerKind::Scalar),
    layer("soil_clay", "soil clay fraction", LayerKind::Scalar),
    layer("soil_organic", "soil organic content", LayerKind::Scalar),
    layer(
        "soil_fertility",
        "baseline soil fertility",
        LayerKind::Scalar,
    ),
    layer("soil_drainage", "soil drainage class", LayerKind::Scalar),
    layer("soil_salinity", "baseline soil salinity", LayerKind::Scalar),
    layer(
        "soil_freeze",
        "seasonal freeze and permafrost flags",
        LayerKind::Categorical,
    ),
    layer(
        "soil_erosion_susceptibility",
        "soil erosion susceptibility",
        LayerKind::Scalar,
    ),
    layer(
        "baseline_biome",
        "baseline biome identifier",
        LayerKind::Categorical,
    ),
    layer(
        "habitat_flags",
        "local habitat bit flags",
        LayerKind::Categorical,
    ),
    layer(
        "edaphic_flags",
        "soil and terrain modifier flags",
        LayerKind::Categorical,
    ),
    layer(
        "vegetation_potential",
        "water/energy-limited vegetation potential",
        LayerKind::Scalar,
    ),
    layer(
        "tree_line",
        "latitude-sensitive tree-line elevation",
        LayerKind::Scalar,
    ),
    layer(
        "succession_potential",
        "natural succession and regrowth potential",
        LayerKind::Scalar,
    ),
    layer(
        "province_id",
        "legacy alias for the geographic country identifier",
        LayerKind::Categorical,
    ),
    layer(
        "country_id",
        "geographic country identifier",
        LayerKind::Categorical,
    ),
    layer(
        "heart_assignment",
        "assigned country-heart identifier",
        LayerKind::Categorical,
    ),
    layer(
        "country_boundaries",
        "geographic country boundaries and traversable contacts",
        LayerKind::Categorical,
    ),
    layer(
        "hearts_edifices",
        "country heart sites colored by geographic form",
        LayerKind::Categorical,
    ),
    layer(
        "graft_compatibility_jungle",
        "example jungle graft: 1 compatible, 2 marginal, 3 incompatible",
        LayerKind::Categorical,
    ),
    layer(
        "animal_frog_suitability",
        "example freshwater-wetland animal suitability",
        LayerKind::Categorical,
    ),
    layer(
        "animal_seal_suitability",
        "example cold marine-coast animal suitability",
        LayerKind::Categorical,
    ),
];
