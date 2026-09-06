//! Saved validation for the common spawn contract.

use crate::chunk::ChunkPos;
use crate::world::World;
use crate::world::storage;

pub(super) fn prepared_chunk_digest(
    store: &storage::RegionStore,
    chunks: &[ChunkPos],
) -> std::io::Result<u64> {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for position in chunks {
        for byte in [position.face() as u8]
            .into_iter()
            .chain(position.u().to_le_bytes())
            .chain(position.v().to_le_bytes())
        {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x1000_0000_01b3);
        }
        let payload = store.read(*position)?.0.ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("prepared spawn chunk {position:?} is missing"),
            )
        })?;
        for byte in payload {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x1000_0000_01b3);
        }
    }
    Ok(hash)
}

pub(super) fn validate_spawn_ledgers(world: &World) -> std::io::Result<()> {
    let weather = world
        .weather_state
        .live()
        .ok_or_else(|| std::io::Error::other("planetary spawn requires a water ledger"))?;
    let water = weather
        .water
        .audit(crate::planet_atlas::ReservoirMass::fresh(
            crate::planet_atlas::dynamic_water_total(&weather.cells) as u64,
        ));
    if water.unexplained_water_delta_hu != 0 || water.unexplained_salt_delta != 0 {
        return Err(std::io::Error::other(format!(
            "prepared homeland water audit failed: {} HU, {} salt",
            water.unexplained_water_delta_hu, water.unexplained_salt_delta
        )));
    }
    let materials = world
        .material_ledger
        .as_ref()
        .ok_or_else(|| std::io::Error::other("planetary spawn requires a material ledger"))?
        .audit();
    if !materials.is_balanced() {
        return Err(std::io::Error::other(
            "prepared homeland material audit has unexplained deltas",
        ));
    }
    if !materials.is_qualified() {
        return Err(std::io::Error::other(format!(
            "prepared homeland material qualification failed: {}",
            materials.qualification_failures.join("; ")
        )));
    }
    Ok(())
}
