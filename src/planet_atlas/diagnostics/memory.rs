//! Observed process residency and explicit loaded atlas storage estimates.

use crate::planet_atlas::{BiomeCell, ChunkWaterCommitment, ClimateCell, CountryRecord, CountryRoute, DynamicCell, FluxInbox, GeometryCell, GroundCell, HydrologyCell, PlanetAtlas, ResourceCell, SparseAquiferState, SpringState, TectonicCell, TerrainCell, WaterCell};
use std::{fs};

pub(in crate::planet_atlas::diagnostics) fn estimated_loaded_bytes(atlas: &PlanetAtlas) -> u64 {
    let count = atlas.genesis.geometry.len() as u64;
    count
        * (std::mem::size_of::<GeometryCell>()
            + std::mem::size_of::<TectonicCell>()
            + std::mem::size_of::<TerrainCell>()
            + std::mem::size_of::<ClimateCell>()
            + std::mem::size_of::<HydrologyCell>()
            + std::mem::size_of::<GroundCell>()
            + std::mem::size_of::<BiomeCell>()
            + std::mem::size_of::<ResourceCell>()
            + std::mem::size_of::<DynamicCell>()
            + std::mem::size_of::<WaterCell>()) as u64
        + atlas.manifest.geology_bytes
        + (atlas.water_cycle.aquifers.capacity() * std::mem::size_of::<SparseAquiferState>()) as u64
        + (atlas.water_cycle.springs.capacity() * std::mem::size_of::<SpringState>()) as u64
        + (atlas.water_cycle.inboxes.capacity() * std::mem::size_of::<FluxInbox>()) as u64
        + (atlas.water_cycle.commitments.capacity() * std::mem::size_of::<ChunkWaterCommitment>())
            as u64
        + (atlas.biomes.countries.capacity() * std::mem::size_of::<CountryRecord>()) as u64
        + atlas
            .biomes
            .countries
            .iter()
            .map(|country| {
                (country.routes.capacity() * std::mem::size_of::<CountryRoute>()) as u64
                    + country
                        .habitat_cells
                        .keys()
                        .map(|name| name.capacity() as u64)
                        .sum::<u64>()
                    + country
                        .biome_cells
                        .keys()
                        .map(|name| name.capacity() as u64)
                        .sum::<u64>()
            })
            .sum::<u64>()
}

pub(in crate::planet_atlas::diagnostics) fn peak_resident_bytes() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let kib = status.lines().find_map(|line| {
        let value = line.strip_prefix("VmHWM:")?;
        value.split_whitespace().next()?.parse::<u64>().ok()
    })?;
    Some(kib * 1024)
}
