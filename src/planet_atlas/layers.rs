//! Typed immutable atlas cells and genesis layer storage.

use crate::planet_atlas::{
    AtlasError, AtlasGrid, BasinKind, CLIMATE_SEASONS, DetailedBoundary, WaterBodyKind,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum BoundaryClass {
    #[default]
    Interior = 0,
    Convergent = 1,
    Divergent = 2,
    Transform = 3,
}

impl BoundaryClass {
    pub(super) fn from_u8(value: u8) -> Result<Self, AtlasError> {
        match value {
            0 => Ok(Self::Interior),
            1 => Ok(Self::Convergent),
            2 => Ok(Self::Divergent),
            3 => Ok(Self::Transform),
            _ => Err(AtlasError::Corrupt(format!(
                "unknown boundary classification {value}"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GeometryCell {
    pub unit_direction: [f32; 3],
    pub latitude_radians: f32,
    pub physical_area: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TectonicCell {
    pub plate_id: u16,
    pub boundary: BoundaryClass,
    pub boundary_detail: DetailedBoundary,
    pub neighbor_plate: u16,
    pub boundary_strength: f32,
    /// Distance to the nearest plate boundary in atlas cells.
    pub boundary_distance: u16,
    /// Unit boundary strike in planet-space, quantized to signed 16-bit.
    pub boundary_strike: [i16; 3],
    /// Normalized 0..=65535.
    pub continental_crust: u16,
    pub craton_id: u16,
    pub crust_age: u16,
    pub oceanic_age: u16,
    /// Approximate crust thickness in hectometres.
    pub crust_thickness: u16,
    pub bedrock_family: u16,
    pub geological_province: u16,
    pub stratigraphic_stack: u16,
    pub metamorphic_grade: u8,
    pub fault_intensity: u16,
    pub volcanic_history: u8,
    pub sediment_basin: BasinKind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TerrainCell {
    pub base_elevation: f32,
    pub eroded_elevation: f32,
    pub tectonic_contribution: f32,
    pub volcanic_contribution: f32,
    pub dynamic_topography: f32,
    /// Connected emerged landmass; zero is ocean.
    pub landmass_id: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClimateCell {
    pub mean_temperature: f32,
    pub seasonality: f32,
    pub ocean_temperature_anomaly: f32,
    pub continentality: f32,
    /// East/north components in this cell's atlas-chart tangent frame.
    pub prevailing_wind: [f32; 2],
    /// Surface-current direction in this cell's atlas-chart tangent frame.
    pub ocean_current: [f32; 2],
    pub mean_atmospheric_moisture: f32,
    pub mean_precipitation: f32,
    pub precipitation_seasonality: f32,
    pub potential_evapotranspiration: f32,
    pub aridity: f32,
    pub snow_persistence: f32,
    pub seasonal_temperature: [f32; CLIMATE_SEASONS],
    pub seasonal_precipitation: [f32; CLIMATE_SEASONS],
    pub seasonal_wind: [[f32; 2]; CLIMATE_SEASONS],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HydrologyCell {
    /// Global atlas index, or `u32::MAX` for a sink.
    pub drainage_receiver: u32,
    pub watershed_id: u32,
    pub ocean_basin_id: u16,
    pub lake_basin_id: u32,
    pub spill_elevation: f32,
    /// Priority-flood elevation used to resolve flats and numerical pits.
    pub filled_elevation: f32,
    /// Climate-normal runoff after infiltration, in millimetres per year.
    pub mean_runoff: f32,
    /// Normalized seasonal runoff shares; the four values sum to 65,535.
    pub seasonal_runoff_fraction: [u16; CLIMATE_SEASONS],
    /// Accumulated climate-normal flow in coarse block cubed per year.
    pub mean_discharge: f32,
    /// Normalized seasonal discharge shares; the four values sum to 65,535.
    pub seasonal_discharge_fraction: [u16; CLIMATE_SEASONS],
    /// Contributing spherical surface area in block squared.
    pub catchment_area: f32,
    pub channel_bed_elevation: f32,
    pub water_surface_elevation: f32,
    /// Bankfull dimensions in hundredths of a block.
    pub channel_width_centiblocks: u16,
    pub channel_depth_centiblocks: u16,
    /// Normalized transport/deposition energy.
    pub sediment_energy: u16,
    /// Continuous atlas baseline water volume, in eighth-block units.
    pub baseline_water_units: u64,
    /// Atlas volume minus quantized voxel volume for this coarse cell.
    pub voxel_volume_residual: i32,
    pub river_id: u32,
    /// Signed terrain change in hundredths of a block.
    pub erosion_centiblocks: i16,
    pub deposition_centiblocks: i16,
    pub seasonal_level_range_centiblocks: u16,
    pub flags: u16,
    pub stream_order: u8,
    pub salinity: u8,
    pub water_body: WaterBodyKind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GroundCell {
    pub soil_parent_material: u16,
    pub aquifer_capacity: u32,
    /// Normalized 0..=65535.
    pub aquifer_permeability: u16,
    /// Normalized primary porosity seed.
    pub porosity: u16,
    pub baseline_groundwater_head: f32,
    /// Compact soil-profile fields; texture fractions use 0..=255 and clay
    /// is the remainder after sand and silt.
    pub soil_depth_decimeters: u8,
    pub sand: u8,
    pub silt: u8,
    pub organic: u8,
    pub baseline_fertility: u8,
    /// 0 is saturated/poorly drained; 255 is excessively drained.
    pub drainage: u8,
    pub soil_salinity: u8,
    pub freeze_flags: u8,
    pub erosion_susceptibility: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BiomeCell {
    pub baseline_biome: u8,
    pub edaphic_flags: u16,
    pub habitat_flags: u32,
    pub vegetation_potential: u8,
    pub tree_line_y: u8,
    pub succession_potential: u8,
    pub country_id: u16,
    pub heart_assignment: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ResourceCell {
    /// Index into the later finite-site manifest; zero means no site yet.
    pub deposit_site_ref: u32,
    pub deposit_site_count: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GenesisLayers {
    pub geometry: AtlasGrid<GeometryCell>,
    pub tectonics: AtlasGrid<TectonicCell>,
    pub terrain: AtlasGrid<TerrainCell>,
    pub climate: AtlasGrid<ClimateCell>,
    pub hydrology: AtlasGrid<HydrologyCell>,
    pub ground: AtlasGrid<GroundCell>,
    pub biomes: AtlasGrid<BiomeCell>,
    pub resources: AtlasGrid<ResourceCell>,
}
