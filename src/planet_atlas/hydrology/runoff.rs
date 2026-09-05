//! Bedrock response and climate-normal runoff partitioning.

use crate::planet_atlas::{BedrockFamily, ClimateCell, TectonicCell, CLIMATE_SEASONS};

pub(super) fn bedrock_resistance(cell: TectonicCell) -> f32 {
    match BedrockFamily::from_id(cell.bedrock_family) {
        BedrockFamily::Shale | BedrockFamily::Evaporite => 0.65,
        BedrockFamily::Sandstone | BedrockFamily::Limestone => 0.82,
        BedrockFamily::MixedBasement | BedrockFamily::Basalt => 1.0,
        BedrockFamily::Granite | BedrockFamily::Marble | BedrockFamily::Slate => 1.25,
        BedrockFamily::Quartzite | BedrockFamily::Ultramafic => 1.48,
    }
}

pub(super) fn infiltration_fraction(cell: TectonicCell) -> f32 {
    match BedrockFamily::from_id(cell.bedrock_family) {
        BedrockFamily::Limestone => 0.52,
        BedrockFamily::Sandstone => 0.34,
        BedrockFamily::Shale => 0.10,
        BedrockFamily::Evaporite => 0.18,
        BedrockFamily::Granite | BedrockFamily::Quartzite => 0.13,
        BedrockFamily::Basalt | BedrockFamily::Ultramafic => 0.26,
        _ => 0.22,
    }
}

pub(super) fn normalized_fractions(values: [f64; CLIMATE_SEASONS]) -> [u16; CLIMATE_SEASONS] {
    let total: f64 = values.iter().sum();
    if total <= f64::EPSILON {
        return [16_384, 16_384, 16_384, 16_383];
    }
    let mut out = [0u16; CLIMATE_SEASONS];
    let mut assigned = 0u32;
    for season in 0..CLIMATE_SEASONS - 1 {
        out[season] = ((values[season] / total) * 65_535.0)
            .round()
            .clamp(0.0, 65_535.0) as u16;
        assigned += u32::from(out[season]);
    }
    out[CLIMATE_SEASONS - 1] = (65_535u32.saturating_sub(assigned)).min(65_535) as u16;
    out
}

pub(super) fn local_runoff(climate: ClimateCell, tectonics: TectonicCell) -> (f32, [f64; 4]) {
    let infiltration = infiltration_fraction(tectonics);
    let mut seasonal = [0.0f64; CLIMATE_SEASONS];
    let mut snow_store = 0.0f64;
    for (season, seasonal_runoff) in seasonal.iter_mut().enumerate() {
        let precipitation = f64::from(climate.seasonal_precipitation[season].max(0.0));
        let temperature = f64::from(climate.seasonal_temperature[season]);
        let seasonal_pet = f64::from(climate.potential_evapotranspiration.max(0.0))
            * ((temperature + 12.0) / 44.0).clamp(0.08, 0.48);
        if temperature <= 0.0 {
            snow_store += precipitation * f64::from(climate.snow_persistence.max(0.2));
            *seasonal_runoff = precipitation * 0.04;
        } else {
            let melt = snow_store * (temperature / 12.0).clamp(0.2, 1.0);
            snow_store -= melt;
            let available = (precipitation + melt - seasonal_pet * 0.42).max(0.0);
            *seasonal_runoff =
                available * f64::from(1.0 - infiltration * 0.62) + precipitation * 0.035;
        }
    }
    seasonal[CLIMATE_SEASONS - 1] += snow_store * 0.08;
    let annual = seasonal.iter().sum::<f64>().max(0.5) as f32;
    (annual, seasonal)
}
