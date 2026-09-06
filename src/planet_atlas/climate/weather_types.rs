//! Public weather observations and completed-hour transfer reports.

use crate::planet_atlas::AtlasPos;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[repr(u8)]
pub enum LocalWeather {
    Clear = 0,
    Overcast = 1,
    Precipitation = 2,
    Storm = 3,
}

impl LocalWeather {
    pub const fn precipitating(self) -> bool {
        matches!(self, Self::Precipitation | Self::Storm)
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::Overcast => "overcast",
            Self::Precipitation => "precipitation",
            Self::Storm => "storm",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[repr(u8)]
pub enum PrecipitationForm {
    None = 0,
    Rain = 1,
    Snow = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct LocalWeatherSample {
    pub kind: LocalWeather,
    pub precipitation: PrecipitationForm,
    pub temperature_c: f32,
    pub pressure_anomaly: i16,
    pub vapor: u32,
    pub cloud_water: u32,
    pub storm_energy: u16,
    pub precipitation_units: u16,
    pub wind: [f32; 2],
}

impl Default for LocalWeatherSample {
    fn default() -> Self {
        Self {
            kind: LocalWeather::Clear,
            precipitation: PrecipitationForm::None,
            temperature_c: 15.0,
            pressure_anomaly: 0,
            vapor: 0,
            cloud_water: 0,
            storm_energy: 0,
            precipitation_units: 0,
            wind: [0.0; 2],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WeatherStepReport {
    pub climate_hour: u64,
    pub processed_cells: usize,
    pub evaporation_units: u64,
    pub condensation_units: u64,
    pub precipitation_units: u64,
    /// Water removed from the coarse snow/runoff stores through the explicit
    /// voxel water-cycle handoff since the previous completed weather step.
    pub water_cycle_outflow_units: u64,
    pub atmospheric_water_before: i128,
    pub atmospheric_water_after: i128,
    pub unexplained_water_drift: i128,
}

/// Exact coarse runoff movement accepted by one weather hour. Environmental
/// solutes use the same source fraction and seam-aware receiver; evaporation
/// is absent because nonvolatile dross remains behind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct RunoffTransport {
    pub from: AtlasPos,
    pub to: AtlasPos,
    pub water_hu: u64,
    pub source_water_before_hu: u64,
}
