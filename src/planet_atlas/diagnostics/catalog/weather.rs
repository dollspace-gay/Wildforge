//! Registered weather diagnostic maps in export order.


use super::{LayerKind, LayerSpec, layer};
pub(super) const LAYERS: &[LayerSpec] = &[
    layer(
        "atmospheric_vapor",
        "mutable atmospheric water",
        LayerKind::Scalar,
    ),
    layer("cloud_water", "mutable cloud water", LayerKind::Scalar),
    layer("soil_moisture", "mutable soil water", LayerKind::Scalar),
    layer("snowpack", "mutable snow storage", LayerKind::Scalar),
    layer(
        "groundwater_volume",
        "mutable groundwater storage",
        LayerKind::Scalar,
    ),
    layer(
        "groundwater_head_anomaly",
        "mutable groundwater head anomaly",
        LayerKind::Scalar,
    ),
    layer(
        "surface_runoff",
        "mutable surface runoff",
        LayerKind::Scalar,
    ),
    layer(
        "groundwater_recharge",
        "water recharged into the shallow aquifer last hour",
        LayerKind::Scalar,
    ),
    layer(
        "spring_discharge",
        "pressure-driven spring discharge last hour",
        LayerKind::Scalar,
    ),
    layer(
        "water_salinity",
        "derived shallow-water salinity",
        LayerKind::Scalar,
    ),
    layer(
        "lake_storage_anomaly",
        "mutable lake storage anomaly",
        LayerKind::Scalar,
    ),
    layer(
        "ocean_storage_anomaly",
        "mutable ocean storage anomaly",
        LayerKind::Scalar,
    ),
    layer(
        "local_weather_anomaly",
        "mutable local weather anomaly",
        LayerKind::Scalar,
    ),
    layer(
        "weather_temperature_anomaly",
        "mutable near-surface temperature anomaly",
        LayerKind::Scalar,
    ),
    layer(
        "pressure_anomaly",
        "mutable pressure anomaly",
        LayerKind::Scalar,
    ),
    layer("storm_energy", "mutable storm energy", LayerKind::Scalar),
    layer(
        "precipitation_rate",
        "conservative cloud-water transfer in the last climate hour",
        LayerKind::Scalar,
    ),
    layer(
        "weather_wind_anomaly",
        "mutable local wind anomaly",
        LayerKind::Direction,
    ),
    layer(
        "fire_moisture_anomaly",
        "mutable fire moisture anomaly",
        LayerKind::Scalar,
    ),
    layer(
        "vegetation_moisture_anomaly",
        "mutable vegetation moisture anomaly",
        LayerKind::Scalar,
    ),
];
