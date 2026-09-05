//! Materialize finite atlas water/salt allocation after all terrain decoration.

use super::Generator;
use crate::chunk::{CHUNK_X, CHUNK_Y, CHUNK_Z, Chunk, ChunkPos};
use crate::registry::AIR;
use std::collections::BTreeMap;
use crate::chunk::HydrologyVolumeRecord;
use crate::registry::Registry;

impl Generator {
    pub(super) fn finish_water(&self, pos: ChunkPos, c: &mut Chunk, reg: &Registry) {
        // Natural water begins with the atlas concentration. Metadata is the
        // block-defined state plane already carried by chunk saves and chunk
        // streaming; Goal 6 will make fluid movement mix and conserve it.
        if let Some(atlas) = &self.atlas {
            let mut baseline = BTreeMap::<u64, f64>::new();
            let mut allocated_visible_units = BTreeMap::<u64, f64>::new();
            let mut materialized = BTreeMap::<u64, (u64, u64)>::new();
            for lx in 0..CHUNK_X {
                for lz in 0..CHUNK_Z {
                    let surface = Self::surface_in_chunk(pos, lx as i32, lz as i32);
                    let sample = atlas.hydrology_sample(surface.center());
                    let salinity = sample.salinity;
                    let reservoir = if sample.ocean_basin_id != 0 {
                        Some((1u64 << 62) | u64::from(sample.ocean_basin_id))
                    } else if sample.lake_basin_id != 0 {
                        Some((2u64 << 62) | u64::from(sample.lake_basin_id))
                    } else if sample.river_id != 0 {
                        Some((3u64 << 62) | u64::from(sample.river_id))
                    } else {
                        None
                    };
                    if let (Some(reservoir), Some(water_surface)) =
                        (reservoir, sample.water_surface_elevation)
                    {
                        let continuous_units = f64::from(
                            (water_surface - sample.channel_bed_elevation).max(0.0) * 8.0,
                        );
                        *baseline.entry(reservoir).or_default() += continuous_units;

                        // A generated source block used to turn every
                        // fractional depth into a whole voxel.  A 1.33-block
                        // river therefore materialized two full blocks per
                        // column even though the reservoir owned only 1.33.
                        // Error-diffuse the immutable continuous allocation
                        // into the registered 1/8-block fluid states.  The
                        // chunk keeps the sub-level remainder coarse, so the
                        // represented volume can never exceed its baseline.
                        let cumulative = allocated_visible_units.entry(reservoir).or_default();
                        let before = cumulative.floor() as u64;
                        *cumulative += continuous_units;
                        let target_units = cumulative.floor() as u64 - before;
                        let wet = (1..CHUNK_Y)
                            .filter(|&y| {
                                let block = c.get(lx, y, lz);
                                reg.is_water(block) || block == self.ice
                            })
                            .collect::<Vec<_>>();
                        let needed_cells = target_units.div_ceil(8) as usize;
                        let first_kept = wet.len().saturating_sub(needed_cells);
                        let mut remaining = target_units.min(wet.len() as u64 * 8);
                        for (index, y) in wet.into_iter().enumerate() {
                            if index < first_kept {
                                c.set(lx, y, lz, AIR);
                                c.set_water_salt(lx, y, lz, 0);
                                continue;
                            }
                            let units = remaining.min(8) as u8;
                            remaining -= u64::from(units);
                            let old = c.get(lx, y, lz);
                            let block = if old == self.ice && units == 8 {
                                self.ice
                            } else {
                                reg.water_for_volume(units)
                            };
                            c.set(lx, y, lz, block);
                            let salt = u64::from(units)
                                .saturating_mul(32)
                                .saturating_mul(u64::from(salinity));
                            c.set_water_salt(lx, y, lz, salt.min(u64::from(u16::MAX)) as u16);
                        }
                    }
                    for y in 1..CHUNK_Y {
                        let block = c.get(lx, y, lz);
                        if reg.is_water(block) {
                            c.set_meta(lx, y, lz, salinity);
                        }
                        if let Some(reservoir) = reservoir
                            && (reg.is_water(block) || self.ice == block)
                        {
                            let units = reg.water_volume(block).map_or(8, u64::from);
                            let materialized = materialized.entry(reservoir).or_default();
                            materialized.0 = materialized.0.saturating_add(units);
                            let salt = units.saturating_mul(32).saturating_mul(u64::from(salinity));
                            materialized.1 = materialized.1.saturating_add(salt);
                            if self.ice == block {
                                c.set_water_salt(lx, y, lz, salt.min(u64::from(u16::MAX)) as u16);
                            }
                        }
                    }
                }
            }
            let records = baseline
                .into_iter()
                .map(|(reservoir, continuous)| {
                    let baseline_hu = (continuous * 32.0).round().max(0.0) as u64;
                    let (materialized_units, salt_mass) =
                        materialized.get(&reservoir).copied().unwrap_or_default();
                    let materialized_hu = materialized_units.saturating_mul(32);
                    HydrologyVolumeRecord {
                        reservoir,
                        baseline_hu,
                        residual_hu: i128::from(baseline_hu)
                            .saturating_sub(i128::from(materialized_hu))
                            .clamp(i128::from(i64::MIN), i128::from(i64::MAX))
                            as i64,
                        salt_mass,
                    }
                })
                .collect();
            c.set_hydrology_volumes(records);
        }

    }
}
