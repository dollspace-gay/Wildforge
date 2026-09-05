//! Registered climate diagnostic maps in export order.


use super::{LayerKind, LayerSpec, layer};
pub(super) const LAYERS: &[LayerSpec] = &[
    layer(
        "mean_temperature",
        "annual mean temperature in Celsius",
        LayerKind::Scalar,
    ),
    layer("seasonality", "annual temperature range", LayerKind::Scalar),
    layer(
        "ocean_temperature_anomaly",
        "surface-current coastal temperature anomaly",
        LayerKind::Scalar,
    ),
    layer(
        "continentality",
        "normalized geodesic distance from ocean influence",
        LayerKind::Scalar,
    ),
    layer(
        "mean_atmospheric_moisture",
        "equilibrium atmospheric moisture",
        LayerKind::Scalar,
    ),
    layer(
        "mean_precipitation",
        "annual precipitation",
        LayerKind::Scalar,
    ),
    layer(
        "precipitation_seasonality",
        "seasonal precipitation contrast",
        LayerKind::Scalar,
    ),
    layer(
        "potential_evapotranspiration",
        "potential evapotranspiration",
        LayerKind::Scalar,
    ),
    layer("aridity", "aridity index", LayerKind::Scalar),
    layer(
        "snow_persistence",
        "annual fraction of precipitation retained as snow",
        LayerKind::Scalar,
    ),
    layer(
        "prevailing_wind",
        "local tangent-frame wind direction",
        LayerKind::Direction,
    ),
    layer(
        "ocean_current",
        "wind-driven surface-current direction",
        LayerKind::Direction,
    ),
    layer(
        "spring_temperature",
        "spring temperature",
        LayerKind::Scalar,
    ),
    layer(
        "summer_temperature",
        "summer temperature",
        LayerKind::Scalar,
    ),
    layer(
        "autumn_temperature",
        "autumn temperature",
        LayerKind::Scalar,
    ),
    layer(
        "winter_temperature",
        "winter temperature",
        LayerKind::Scalar,
    ),
    layer(
        "spring_precipitation",
        "spring precipitation",
        LayerKind::Scalar,
    ),
    layer(
        "summer_precipitation",
        "summer precipitation",
        LayerKind::Scalar,
    ),
    layer(
        "autumn_precipitation",
        "autumn precipitation",
        LayerKind::Scalar,
    ),
    layer(
        "winter_precipitation",
        "winter precipitation",
        LayerKind::Scalar,
    ),
    layer(
        "spring_wind",
        "spring prevailing wind",
        LayerKind::Direction,
    ),
    layer(
        "summer_wind",
        "summer prevailing wind",
        LayerKind::Direction,
    ),
    layer(
        "autumn_wind",
        "autumn prevailing wind",
        LayerKind::Direction,
    ),
    layer(
        "winter_wind",
        "winter prevailing wind",
        LayerKind::Direction,
    ),
];
