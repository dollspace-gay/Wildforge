use crate::world::{ReplicaWorld, ReplicationTarget, TerrainRead};
use std::collections::BTreeSet;
use std::sync::{Arc, OnceLock};

use super::*;
use crate::chunk::{CHUNK_X, CHUNK_Y, CHUNK_Z, ChunkPos, SEA_LEVEL};
use crate::planet::{FACE_BLOCKS, SurfacePos, geodesic_distance};
use crate::planet_atlas::{
    HYDRO_DELTA, HYDRO_ESTUARY, HYDRO_INTERMITTENT, HYDRO_LAKE, HYDRO_PERENNIAL, HYDRO_RIVER,
    LakeClass, PlanetAtlas, PlanetaryWeather, ReservoirMass, SurfaceReservoirKind,
    dynamic_water_total, surface_reservoir_id,
};

fn atlas() -> &'static Arc<PlanetAtlas> {
    static ATLAS: OnceLock<Arc<PlanetAtlas>> = OnceLock::new();
    ATLAS.get_or_init(|| Arc::new(PlanetAtlas::fixture(1_337, 64).unwrap()))
}

fn average(values: impl Iterator<Item = f64>) -> f64 {
    let values: Vec<_> = values.collect();
    values.iter().sum::<f64>() / values.len().max(1) as f64
}

fn surface_at(pos: crate::planet_atlas::AtlasPos, side: u16) -> SurfacePos {
    let center = pos.center(side);
    SurfacePos::new(
        center.face,
        center.u.floor().clamp(0.0, f64::from(FACE_BLOCKS - 1)) as u16,
        center.v.floor().clamp(0.0, f64::from(FACE_BLOCKS - 1)) as u16,
    )
    .unwrap()
}

mod basins;
mod climate;
mod deposits;
mod drainage;
mod ecology;
mod geometry;
mod persistence;
mod qualification;
mod voxel_custody;
