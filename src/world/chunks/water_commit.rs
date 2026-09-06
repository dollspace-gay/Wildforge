//! Water commit chunks transaction coordination.

use crate::chunk::CHUNK_X;
use crate::chunk::CHUNK_Y;
use crate::chunk::CHUNK_Z;
use crate::chunk::Chunk;
use crate::chunk::ChunkPos;
use crate::world::World;

impl World {
    pub(super) fn commit_fresh_chunk_water(&mut self, pos: ChunkPos, chunk: &mut Chunk) {
        let reg = self.reg.clone();
        let (Some(atlas), Some(weather)) = (&self.planet_atlas, self.weather_state.live_mut()) else {
            return;
        };
        let existing = weather
            .water
            .commitments
            .iter()
            .filter(|commitment| commitment.chunk == pos)
            .copied()
            .collect::<Vec<_>>();
        let mut records = chunk.hydrology_volumes().to_vec();
        for record in &mut records {
            let wanted_hu = i128::from(record.baseline_hu)
                .saturating_sub(i128::from(record.residual_hu))
                .clamp(0, i128::from(u64::MAX)) as u64;
            let parcel = if let Some(commitment) = existing
                .iter()
                .find(|commitment| commitment.reservoir == record.reservoir)
            {
                commitment.mass
            } else {
                let Some(reservoir) = weather.water.reservoir_mut(record.reservoir) else {
                    eprintln!(
                        "water: chunk {:?} references missing reservoir {}",
                        pos, record.reservoir
                    );
                    continue;
                };
                // The generated chunk already measured the local salinity of
                // every voxel. Debit that exact salt mass from the named
                // basin instead of taking a basin-average parcel, otherwise
                // a fresh river chunk changes salinity merely by loading.
                // Voxel fluid states are quantized in 32-HU visible units.
                // If a dynamically lowered reservoir cannot fund the
                // immutable baseline, leave its sub-level remainder coarse
                // and materialize only water it actually owns.
                let funded_hu = reservoir.coarse.water_hu.min(wanted_hu)
                    / crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL
                    * crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL;
                let wanted = crate::planet_atlas::ReservoirMass {
                    water_hu: funded_hu,
                    salt_mass: if funded_hu == wanted_hu {
                        record.salt_mass
                    } else {
                        0
                    },
                };
                let parcel = if funded_hu == wanted_hu {
                    reservoir
                        .coarse
                        .take_exact(wanted)
                        .unwrap_or_else(|| reservoir.coarse.take(funded_hu))
                } else {
                    reservoir.coarse.take(funded_hu)
                };
                if parcel.water_hu != wanted_hu {
                    eprintln!(
                        "water: reservoir {} supplied {} of {} HU for chunk {:?}",
                        record.reservoir, parcel.water_hu, wanted_hu, pos
                    );
                }
                if weather
                    .water
                    .credit_detailed_to(Some(record.reservoir), parcel)
                    .is_err()
                {
                    weather
                        .water
                        .reservoir_mut(record.reservoir)
                        .expect("source reservoir still exists")
                        .coarse
                        .add_assign(parcel)
                        .expect("rolled-back water commitment fits");
                    continue;
                }
                weather
                    .water
                    .commitments
                    .push(crate::planet_atlas::ChunkWaterCommitment {
                        chunk: pos,
                        reservoir: record.reservoir,
                        mass: parcel,
                    });
                parcel
            };
            record.salt_mass = parcel.salt_mass;
            record.residual_hu = i128::from(record.baseline_hu)
                .saturating_sub(i128::from(parcel.water_hu))
                .clamp(i128::from(i64::MIN), i128::from(i64::MAX))
                as i64;

            let mut cells = Vec::new();
            for lx in 0..CHUNK_X {
                for lz in 0..CHUNK_Z {
                    let surface = crate::planet::SurfacePos::new(
                        pos.face(),
                        pos.u() * CHUNK_X as u16 + lx as u16,
                        pos.v() * CHUNK_Z as u16 + lz as u16,
                    )
                    .expect("chunk column is canonical");
                    let hydro = atlas.hydrology_sample(surface.center());
                    let reservoir = if hydro.ocean_basin_id != 0 {
                        Some(crate::planet_atlas::surface_reservoir_id(
                            crate::planet_atlas::SurfaceReservoirKind::Ocean,
                            u32::from(hydro.ocean_basin_id),
                        ))
                    } else if hydro.lake_basin_id != 0 {
                        Some(crate::planet_atlas::surface_reservoir_id(
                            crate::planet_atlas::SurfaceReservoirKind::Lake,
                            hydro.lake_basin_id,
                        ))
                    } else if hydro.river_id != 0 {
                        Some(crate::planet_atlas::surface_reservoir_id(
                            crate::planet_atlas::SurfaceReservoirKind::River,
                            hydro.river_id,
                        ))
                    } else {
                        None
                    };
                    if reservoir != Some(record.reservoir) {
                        continue;
                    }
                    for y in 1..CHUNK_Y {
                        let block = chunk.get(lx, y, lz);
                        if let Some(units) = reg.water_volume(block) {
                            cells.push((lx, y, lz, units, false));
                        } else if reg.block(block).name == "base:ice" {
                            cells.push((lx, y, lz, 8, true));
                        }
                    }
                }
            }
            let represented_hu = cells.iter().fold(0u64, |total, cell| {
                total.saturating_add(
                    u64::from(cell.3)
                        .saturating_mul(crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL),
                )
            });
            if represented_hu != parcel.water_hu {
                cells.sort_by_key(|&(x, y, z, _, _)| (y, x, z));
                let mut remaining_units =
                    parcel.water_hu / crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL;
                for cell in &mut cells {
                    let (x, y, z, units, was_ice) = *cell;
                    let kept = u64::from(units).min(remaining_units) as u8;
                    remaining_units -= u64::from(kept);
                    cell.3 = kept;
                    let block = if kept == 0 {
                        crate::registry::AIR
                    } else if was_ice && kept == 8 {
                        chunk.get(x, y, z)
                    } else {
                        reg.water_for_volume(kept)
                    };
                    chunk.set(x, y, z, block);
                    if kept == 0 {
                        chunk.set_water_salt(x, y, z, 0);
                        chunk.set_meta(x, y, z, 0);
                    }
                }
                debug_assert_eq!(remaining_units, 0);
                cells.retain(|cell| cell.3 != 0);
            }
            if !cells.is_empty() {
                let existing_total = cells.iter().fold(0u64, |total, &(x, y, z, _, _)| {
                    total.saturating_add(u64::from(chunk.water_salt(x, y, z)))
                });
                // Usually these totals are identical and the atlas-authored
                // per-column concentrations remain byte-for-byte unchanged.
                // A recovered/legacy commitment can differ, so apportion its
                // exact total by the existing local weights rather than
                // flattening the whole chunk to one concentration.
                if existing_total != parcel.salt_mass {
                    let count = cells.len() as u64;
                    let mut previous_allocation = 0u64;
                    let mut cumulative_weight = 0u64;
                    let mut allocations = Vec::with_capacity(cells.len());
                    for (index, &(x, y, z, _, _)) in cells.iter().enumerate() {
                        cumulative_weight =
                            cumulative_weight.saturating_add(u64::from(chunk.water_salt(x, y, z)));
                        let cumulative_allocation = if existing_total == 0 {
                            (index as u64 + 1).saturating_mul(parcel.salt_mass) / count
                        } else {
                            (u128::from(parcel.salt_mass) * u128::from(cumulative_weight)
                                / u128::from(existing_total)) as u64
                        };
                        allocations.push(
                            cumulative_allocation
                                .saturating_sub(previous_allocation)
                                .min(u64::from(u16::MAX)),
                        );
                        previous_allocation = cumulative_allocation;
                    }
                    let mut remainder = parcel
                        .salt_mass
                        .saturating_sub(allocations.iter().copied().sum::<u64>());
                    for allocation in &mut allocations {
                        let extra = remainder.min(u64::from(u16::MAX) - *allocation);
                        *allocation += extra;
                        remainder -= extra;
                        if remainder == 0 {
                            break;
                        }
                    }
                    debug_assert_eq!(remainder, 0);
                    for (&(x, y, z, _, _), salt) in cells.iter().zip(allocations) {
                        let salt = salt as u16;
                        chunk.set_water_salt(x, y, z, salt);
                        chunk.set_meta(x, y, z, (u64::from(salt) / 256).min(255) as u8);
                    }
                }
            }
        }
        weather
            .water
            .commitments
            .sort_by_key(|commitment| (commitment.chunk, commitment.reservoir));
        chunk.set_hydrology_volumes(records);
    }
}
