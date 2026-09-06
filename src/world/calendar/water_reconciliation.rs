//! Water reconciliation calendar transaction coordination.

use crate::chunk::CHUNK_X;
use crate::chunk::CHUNK_Y;
use crate::chunk::CHUNK_Z;
use crate::world::World;

impl World {
    pub(super) fn reconcile_loaded_springs(&mut self) {
        let (Some(atlas), Some(weather)) = (&self.planet_atlas, self.weather_state.live()) else {
            return;
        };
        let candidates = weather
            .water
            .springs
            .iter()
            .filter(|spring| spring.active && spring.last_discharge_hu >= 32)
            .map(|spring| {
                let center = spring.pos.center(atlas.side());
                (
                    spring.pos,
                    crate::planet::SurfacePos::new(
                        center.face,
                        center.u.floor() as u16,
                        center.v.floor() as u16,
                    )
                    .expect("spring center is canonical"),
                    spring.outlet_milliblocks.div_euclid(1000),
                )
            })
            .collect::<Vec<_>>();
        for (atlas_pos, surface, outlet) in candidates {
            if !self
                .chunks
                .contains_key(&crate::planet::ChunkPos::from_surface(surface))
            {
                continue;
            }
            let y = outlet.clamp(1, CHUNK_Y as i32 - 2) as u8;
            let at = crate::planet::BlockPos::new(surface.face(), surface.u(), y, surface.v())
                .expect("spring outlet is in shell");
            let target = if self.get_block_at(at) == crate::registry::AIR {
                Some(at)
            } else {
                at.offset(0, 1, 0)
                    .filter(|above| self.get_block_at(*above) == crate::registry::AIR)
            };
            let Some(target) = target else { continue };
            let claimed = self.weather_state.live_mut().map_or(
                crate::planet_atlas::ReservoirMass::default(),
                |weather| {
                    weather.withdraw_water_cycle_mass(
                        atlas_pos,
                        crate::planet_atlas::PrecipitationForm::Rain,
                        crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL as u32,
                    )
                },
            );
            if claimed.water_hu == crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL {
                self.write_water_mass_at(target, claimed);
            }
        }
    }

    pub(super) fn reconcile_loaded_shores(&mut self) {
        let (Some(atlas), Some(weather)) = (&self.planet_atlas, self.weather_state.live()) else {
            return;
        };
        let levels = weather
            .water
            .reservoirs
            .iter()
            .map(|reservoir| (reservoir.id, reservoir.level_milliblocks))
            .collect::<std::collections::BTreeMap<_, _>>();
        let chunks = self
            .chunks
            .keys()
            .filter(|chunk| !self.player_touched.contains(chunk))
            .copied()
            .collect::<Vec<_>>();
        let mut operations = Vec::<(crate::planet::BlockPos, u64, bool)>::new();
        for chunk_pos in chunks {
            for lx in 0..CHUNK_X {
                for lz in 0..CHUNK_Z {
                    let surface = crate::planet::SurfacePos::new(
                        chunk_pos.face(),
                        chunk_pos.u() * CHUNK_X as u16 + lx as u16,
                        chunk_pos.v() * CHUNK_Z as u16 + lz as u16,
                    )
                    .expect("loaded chunk column is canonical");
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
                    } else {
                        None
                    };
                    let Some(reservoir) = reservoir else { continue };
                    let Some(level) = levels.get(&reservoir) else {
                        continue;
                    };
                    let desired = level.div_euclid(1000).clamp(1, CHUNK_Y as i32 - 2);
                    let mut highest = None;
                    for y in (1..CHUNK_Y).rev() {
                        let at = crate::planet::BlockPos::new(
                            surface.face(),
                            surface.u(),
                            y as u8,
                            surface.v(),
                        )
                        .expect("shore height is inside shell");
                        if self.reg.is_water(self.get_block_at(at)) {
                            highest = Some(y as i32);
                            break;
                        }
                    }
                    let current =
                        highest.unwrap_or_else(|| self.surface_height_at(surface).min(desired));
                    if current > desired {
                        for y in (desired + 1)..=current.min(desired + 2) {
                            let at = crate::planet::BlockPos::new(
                                surface.face(),
                                surface.u(),
                                y as u8,
                                surface.v(),
                            )
                            .expect("shore height");
                            if self.reg.is_water(self.get_block_at(at)) {
                                operations.push((at, reservoir, false));
                            }
                        }
                    } else if current < desired {
                        for y in (current + 1)..=desired.min(current + 2) {
                            let at = crate::planet::BlockPos::new(
                                surface.face(),
                                surface.u(),
                                y as u8,
                                surface.v(),
                            )
                            .expect("shore height");
                            if self.get_block_at(at) == crate::registry::AIR {
                                operations.push((at, reservoir, true));
                            }
                        }
                    }
                }
            }
        }
        for (at, reservoir, wet) in operations {
            if wet {
                let parcel = self.weather_state.live_mut().map_or(
                    crate::planet_atlas::ReservoirMass::default(),
                    |weather| {
                        weather.materialize_surface_water(
                            reservoir,
                            crate::planet_atlas::HYDRO_UNITS_PER_BLOCK,
                        )
                    },
                );
                if parcel.water_hu == crate::planet_atlas::HYDRO_UNITS_PER_BLOCK {
                    self.write_water_mass_at(at, parcel);
                }
            } else if let Some(mass) = self.water_mass_at(at) {
                let returned = self
                    .weather_state
                    .live_mut()
                    .is_some_and(|weather| weather.dematerialize_surface_water(reservoir, mass));
                if returned {
                    self.set_block_at(at, crate::registry::AIR);
                }
            }
        }
    }
}
