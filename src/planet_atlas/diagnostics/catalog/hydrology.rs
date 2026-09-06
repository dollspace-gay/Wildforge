//! Registered hydrology diagnostic maps in export order.

use super::{LayerKind, LayerSpec, layer};
pub(super) const LAYERS: &[LayerSpec] = &[
    layer(
        "drainage_receiver",
        "global downstream cell index",
        LayerKind::Categorical,
    ),
    layer(
        "watershed_id",
        "watershed identifier",
        LayerKind::Categorical,
    ),
    layer(
        "ocean_basin_id",
        "ocean basin identifier",
        LayerKind::Categorical,
    ),
    layer(
        "lake_basin_id",
        "lake basin identifier",
        LayerKind::Categorical,
    ),
    layer(
        "spill_elevation",
        "basin spill elevation",
        LayerKind::Scalar,
    ),
    layer(
        "depression_depth",
        "priority-flood depression depth",
        LayerKind::Scalar,
    ),
    layer("mean_runoff", "mean annual runoff", LayerKind::Scalar),
    layer(
        "mean_discharge",
        "accumulated channel discharge",
        LayerKind::Scalar,
    ),
    layer(
        "catchment_area",
        "upstream contributing area",
        LayerKind::Scalar,
    ),
    layer("river_id", "named river identifier", LayerKind::Categorical),
    layer("stream_order", "Strahler stream order", LayerKind::Scalar),
    layer("channel_width", "bankfull channel width", LayerKind::Scalar),
    layer("channel_depth", "bankfull channel depth", LayerKind::Scalar),
    layer(
        "channel_bed_elevation",
        "monotonic channel bed elevation",
        LayerKind::Scalar,
    ),
    layer(
        "water_surface_elevation",
        "baseline water-surface elevation",
        LayerKind::Scalar,
    ),
    layer(
        "water_body",
        "ocean, river, lake, playa, delta, estuary, or wetland",
        LayerKind::Categorical,
    ),
    layer(
        "salinity",
        "baseline salinity concentration",
        LayerKind::Scalar,
    ),
    layer(
        "sediment_energy",
        "normalized sediment transport energy",
        LayerKind::Scalar,
    ),
    layer("erosion", "hydrological erosion depth", LayerKind::Scalar),
    layer(
        "deposition",
        "hydrological deposition depth",
        LayerKind::Scalar,
    ),
    layer(
        "seasonal_water_range",
        "seasonal lake-level range",
        LayerKind::Scalar,
    ),
    layer(
        "baseline_water_volume",
        "baseline water volume in eighth-block units",
        LayerKind::Scalar,
    ),
    layer(
        "voxel_volume_residual",
        "coarse storage retained after voxel quantization",
        LayerKind::Scalar,
    ),
    layer(
        "floodplain",
        "floodplain habitat mask",
        LayerKind::Categorical,
    ),
    layer("wetland", "wetland habitat mask", LayerKind::Categorical),
    layer("delta", "deltaic deposition mask", LayerKind::Categorical),
    layer("estuary", "brackish estuary mask", LayerKind::Categorical),
];
