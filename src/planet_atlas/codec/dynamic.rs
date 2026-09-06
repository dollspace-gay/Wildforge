//! Fixed-width atmosphere checkpoints and completed-hour markers.

use crate::planet_atlas::codec::primitives::{
    ByteReader, put_i16, put_i32, put_u16, put_u32, put_u64,
};
use crate::planet_atlas::codec::{DYNAMIC_PREFIX_BYTES, DYNAMIC_RECORD_BYTES, FILE_HEADER_BYTES};
use crate::planet_atlas::grid::atlas_count;
use crate::planet_atlas::{AtlasError, AtlasGrid, DynamicCell, DynamicLayers};

pub(in crate::planet_atlas) fn decode_dynamic(
    side: u16,
    payload: &[u8],
) -> Result<DynamicLayers, AtlasError> {
    let count = atlas_count(side)?;
    if payload.len() != DYNAMIC_PREFIX_BYTES + count * DYNAMIC_RECORD_BYTES {
        return Err(AtlasError::Corrupt("dynamic payload width mismatch".into()));
    }
    let mut reader = ByteReader::new(payload);
    let completed_climate_hours = reader.u64()?;
    let mut cells = Vec::with_capacity(count);
    for _ in 0..count {
        cells.push(DynamicCell {
            atmospheric_vapor: reader.u32()?,
            cloud_water: reader.u32()?,
            local_weather_anomaly: reader.i32()?,
            weather_temperature_anomaly: reader.i16()?,
            pressure_anomaly: reader.i16()?,
            storm_energy: reader.u16()?,
            precipitation_rate: reader.u16()?,
            wind_anomaly: [reader.i16()?, reader.i16()?],
            fire_moisture_anomaly: reader.i32()?,
            vegetation_moisture_anomaly: reader.i32()?,
        });
    }
    Ok(DynamicLayers {
        completed_climate_hours,
        cells: AtlasGrid::from_values(side, cells)?,
    })
}

pub(in crate::planet_atlas) fn encode_dynamic(
    dynamic: &DynamicLayers,
) -> Result<Vec<u8>, AtlasError> {
    let mut out = Vec::with_capacity(
        DYNAMIC_PREFIX_BYTES + dynamic.cells.len() * DYNAMIC_RECORD_BYTES + FILE_HEADER_BYTES,
    );
    put_u64(&mut out, dynamic.completed_climate_hours);
    for cell in dynamic.cells.values() {
        put_u32(&mut out, cell.atmospheric_vapor);
        put_u32(&mut out, cell.cloud_water);
        put_i32(&mut out, cell.local_weather_anomaly);
        put_i16(&mut out, cell.weather_temperature_anomaly);
        put_i16(&mut out, cell.pressure_anomaly);
        put_u16(&mut out, cell.storm_energy);
        put_u16(&mut out, cell.precipitation_rate);
        put_i16(&mut out, cell.wind_anomaly[0]);
        put_i16(&mut out, cell.wind_anomaly[1]);
        put_i32(&mut out, cell.fire_moisture_anomaly);
        put_i32(&mut out, cell.vegetation_moisture_anomaly);
    }
    if out.len() != DYNAMIC_PREFIX_BYTES + dynamic.cells.len() * DYNAMIC_RECORD_BYTES {
        return Err(AtlasError::Corrupt(
            "internal dynamic record width mismatch".into(),
        ));
    }
    Ok(out)
}
