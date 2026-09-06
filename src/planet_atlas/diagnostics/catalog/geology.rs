//! Registered geology diagnostic maps in export order.

use super::{LayerKind, LayerSpec, layer};
pub(super) const LAYERS: &[LayerSpec] = &[
    layer("latitude", "latitude in radians", LayerKind::Scalar),
    layer(
        "physical_area",
        "spherical cell area in block squared",
        LayerKind::Scalar,
    ),
    layer(
        "plate_id",
        "tectonic plate identifier",
        LayerKind::Categorical,
    ),
    layer(
        "boundary",
        "0 interior, 1 convergent, 2 divergent, 3 transform",
        LayerKind::Categorical,
    ),
    layer(
        "boundary_type",
        "causal connected boundary class",
        LayerKind::Categorical,
    ),
    layer(
        "boundary_strength",
        "relative plate motion magnitude",
        LayerKind::Scalar,
    ),
    layer(
        "boundary_distance",
        "distance to nearest plate boundary in cells",
        LayerKind::Scalar,
    ),
    layer(
        "boundary_strike",
        "local strike of the nearest connected boundary run",
        LayerKind::Direction,
    ),
    layer(
        "plate_velocity",
        "Euler-pole plate velocity in the local tangent frame",
        LayerKind::Direction,
    ),
    layer(
        "euler_poles",
        "explicit Euler rotation-pole locations, colored by plate identifier",
        LayerKind::Categorical,
    ),
    layer(
        "continental_crust",
        "continental crust fraction",
        LayerKind::Scalar,
    ),
    layer(
        "crust_age",
        "crust age in millions of years",
        LayerKind::Scalar,
    ),
    layer(
        "oceanic_age",
        "oceanic crust age away from spreading ridges",
        LayerKind::Scalar,
    ),
    layer(
        "crust_thickness",
        "causal crust thickness",
        LayerKind::Scalar,
    ),
    layer(
        "craton_id",
        "ancient continental craton group",
        LayerKind::Categorical,
    ),
    layer(
        "bedrock_family",
        "bedrock family identifier",
        LayerKind::Categorical,
    ),
    layer(
        "geological_province",
        "geological province identifier",
        LayerKind::Categorical,
    ),
    layer(
        "base_elevation",
        "pre-erosion elevation in blocks",
        LayerKind::Scalar,
    ),
    layer(
        "eroded_elevation",
        "committed elevation in blocks",
        LayerKind::Scalar,
    ),
    layer(
        "tectonic_contribution",
        "elevation contribution from boundary deformation",
        LayerKind::Scalar,
    ),
    layer(
        "volcanic_contribution",
        "elevation contribution from volcanic construction",
        LayerKind::Scalar,
    ),
    layer(
        "dynamic_topography",
        "long-wavelength mantle topography contribution",
        LayerKind::Scalar,
    ),
    layer(
        "landmass_id",
        "connected emerged landmass",
        LayerKind::Categorical,
    ),
    layer(
        "stratigraphic_stack",
        "causal stratigraphic stack identifier",
        LayerKind::Categorical,
    ),
    layer(
        "metamorphic_grade",
        "regional or contact metamorphic grade",
        LayerKind::Scalar,
    ),
    layer(
        "fault_intensity",
        "fault and fracture intensity",
        LayerKind::Scalar,
    ),
    layer(
        "sediment_basin",
        "sedimentary environment and basin history",
        LayerKind::Categorical,
    ),
    layer(
        "volcanic_history",
        "tectonic volcanic-history flag",
        LayerKind::Categorical,
    ),
    layer(
        "ocean_depth",
        "depth below sea level in blocks",
        LayerKind::Scalar,
    ),
];
