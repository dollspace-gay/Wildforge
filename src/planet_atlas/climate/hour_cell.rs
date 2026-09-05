//! Prepare one cell against the unchanged previous weather grid.

use glam::DVec3;
use crate::chunk::SEA_LEVEL;
use crate::planet::Direction4;
use crate::planet_atlas::{AtlasPos, PlanetAtlas, AtlasError, ReservoirMass, FluxInbox, SurfaceReservoirKind, HYDRO_UNITS_PER_VISIBLE_LEVEL, surface_reservoir_id};
use super::{PlanetaryWeather, RunoffTransport, seasonal_scalar, seasonal_vector};
use super::basins::{hydrology_surface_id, take_river_baseflow};
use super::circulation::downstream_neighbor;
use super::transport::{chart_vector, transport_stencil, distribute_u32, spill_vapor, spill_cloud, add_i16};

impl PlanetaryWeather {
    pub(super) fn advance_cell(
        &mut self,
        atlas: &PlanetAtlas,
        day: f64,
        climate_hour: u64,
        index: usize,
        local_ire: &mut impl FnMut(AtlasPos) -> f32,
    ) -> Result<(), AtlasError> {
        let side = atlas.side();
        let pos = AtlasPos::from_index(index, side).expect("weather index");
        let climate = atlas.genesis.climate.values()[index];
        let terrain = atlas.genesis.terrain.values()[index];
        let ground = atlas.genesis.ground.values()[index];
        let hydro = atlas.genesis.hydrology.values()[index];
        let mut source = self.cells.cells.values()[index];
        let mut water = self.water.cells.values()[index];
        let temperature = seasonal_scalar(climate.seasonal_temperature, day)
            + f32::from(source.weather_temperature_anomaly) / 100.0;
        let target_vapor = ((climate.mean_atmospheric_moisture * 180.0).max(24.0) as u32)
            .saturating_mul(HYDRO_UNITS_PER_VISIBLE_LEVEL as u32);

        let evaporation = if source.atmospheric_vapor < target_vapor {
            ((target_vapor - source.atmospheric_vapor) / 12
                + HYDRO_UNITS_PER_VISIBLE_LEVEL as u32)
                .min(96 * HYDRO_UNITS_PER_VISIBLE_LEVEL as u32)
        } else {
            0
        };
        let surface_id = hydrology_surface_id(hydro);
        let evaporation_mass = if let Some(id) = surface_id {
            self.plan_surface_evaporation(id, u64::from(evaporation))
        } else {
            water.soil.take_fresh_water(u64::from(evaporation))
        };
        let transpiration_request = if terrain.eroded_elevation > SEA_LEVEL as f32 {
            u64::from(evaporation / 4).saturating_mul(u64::from(
                atlas.genesis.biomes.values()[index].baseline_biome != 2,
            ))
        } else {
            0
        };
        let transpired = water.soil.take_fresh_water(transpiration_request);
        let actual_evaporation = evaporation_mass
            .water_hu
            .saturating_add(transpired.water_hu)
            .min(u64::from(u32::MAX)) as u32;
        source.atmospheric_vapor = source
            .atmospheric_vapor
            .checked_add(actual_evaporation)
            .ok_or_else(|| AtlasError::Corrupt("atmospheric vapor overflow".into()))?;
        water.last_evaporation_hu = actual_evaporation;
        self.evaporation_units += u64::from(actual_evaporation);

        let saturation = ((target_vapor as f32)
            * (1.0 + ((temperature - climate.mean_temperature) * 0.025).clamp(-0.35, 0.45)))
        .max(12.0) as u32;
        let condensation = source
            .atmospheric_vapor
            .saturating_sub(saturation)
            .saturating_div(3)
            .min(512 * HYDRO_UNITS_PER_VISIBLE_LEVEL as u32)
            .min(u32::MAX - source.cloud_water);
        source.atmospheric_vapor -= condensation;
        source.cloud_water += condensation;
        self.condensation_units += u64::from(condensation);

        let east = pos.step(Direction4::East, side).pos.index(side);
        let west = pos.step(Direction4::West, side).pos.index(side);
        let north = pos.step(Direction4::North, side).pos.index(side);
        let south = pos.step(Direction4::South, side).pos.index(side);
        let pressure_gradient = i32::from(self.cells.cells.values()[west].pressure_anomaly)
            - i32::from(self.cells.cells.values()[east].pressure_anomaly)
            + i32::from(self.cells.cells.values()[south].pressure_anomaly)
            - i32::from(self.cells.cells.values()[north].pressure_anomaly);
        let unit = DVec3::from_array(
            atlas.genesis.geometry.values()[index]
                .unit_direction
                .map(f64::from),
        );
        let wave_axis = DVec3::new(
            (climate_hour as f64 * 0.071).cos(),
            0.37,
            (climate_hour as f64 * 0.071).sin(),
        )
        .normalize();
        let wave = (unit.dot(wave_axis) * 9.0 + climate_hour as f64 * 0.31).sin();
        let pressure = (i32::from(source.pressure_anomaly) * 3 / 4 + (wave * 420.0) as i32)
            .clamp(i32::from(i16::MIN), i32::from(i16::MAX));
        let temperature_anomaly = (i32::from(source.weather_temperature_anomaly) * 4 / 5
            - pressure / 18
            + (wave * 32.0) as i32)
            .clamp(-2400, 2400);
        let ire = local_ire(pos).clamp(0.0, 100.0);
        let cloud_fraction =
            (source.cloud_water as f32 / (target_vapor as f32 + 1.0)).clamp(0.0, 2.0);
        let instability = (pressure_gradient.unsigned_abs() as f32 / 1200.0).clamp(0.0, 1.0);
        let storm_energy = ((cloud_fraction * 18_000.0 + instability * 18_000.0 + ire * 180.0)
            .clamp(0.0, 65_535.0)) as u16;
        let seasonal_precip = seasonal_scalar(climate.seasonal_precipitation, day);
        let precipitation_fraction = (0.012
            + seasonal_precip / climate.mean_precipitation.max(1.0) * 0.028
            + f32::from(storm_energy) / 65_535.0 * 0.12)
            .clamp(0.006, 0.22);
        let precipitation_threshold =
            (target_vapor * 7 / 100).max(24 * HYDRO_UNITS_PER_VISIBLE_LEVEL as u32);
        let precipitable = source.cloud_water.saturating_sub(precipitation_threshold);
        let precipitation =
            ((precipitable as f32 * precipitation_fraction) as u32).min(precipitable);
        source.cloud_water -= precipitation;
        if temperature <= 0.0 {
            water
                .snow
                .add_assign(ReservoirMass::fresh(u64::from(precipitation)))?;
        } else {
            let soil_capacity = u64::from(
                ground
                    .aquifer_capacity
                    .saturating_div(8)
                    .max((climate.mean_precipitation * 3.0) as u32)
                    .max(256),
            )
            .saturating_mul(HYDRO_UNITS_PER_VISIBLE_LEVEL);
            let accepted =
                u64::from(precipitation).min(soil_capacity.saturating_sub(water.soil.water_hu));
            water.soil.add_assign(ReservoirMass::fresh(accepted))?;
            water.runoff.add_assign(ReservoirMass::fresh(
                u64::from(precipitation).saturating_sub(accepted),
            ))?;
        }
        self.precipitation_units += u64::from(precipitation);

        // Hourly infiltration/recharge is bounded by permeability and
        // storage. Frozen ground slows but never disables the path.
        let groundwater_capacity =
            u64::from(ground.aquifer_capacity).saturating_mul(HYDRO_UNITS_PER_VISIBLE_LEVEL);
        let permeability_rate = (u64::from(ground.aquifer_permeability) / 2048).max(1);
        let frozen_divisor = if temperature <= 0.0 { 8 } else { 1 };
        let recharge_request = permeability_rate / frozen_divisor;
        let recharge = water.soil.take(
            recharge_request
                .min(groundwater_capacity.saturating_sub(water.groundwater.water_hu)),
        );
        water.groundwater.add_assign(recharge)?;
        water.last_recharge_hu = recharge.water_hu.min(u64::from(u32::MAX)) as u32;

        // Springs and perennial river baseflow are pressure-dependent
        // transfers from the shallow aquifer, never perpetual sources.
        let spring_index = self
            .water
            .springs
            .binary_search_by_key(&pos, |spring| spring.pos)
            .ok();
        let spring_active = spring_index.and_then(|spring_index| {
            let spring = self.water.springs[spring_index];
            (water.groundwater_head_milliblocks > spring.outlet_milliblocks)
                .then_some((spring_index, spring))
        });
        if let Some((spring_index, spring)) = spring_active {
            let pressure = water
                .groundwater_head_milliblocks
                .saturating_sub(spring.outlet_milliblocks)
                as u64;
            let discharge = water.groundwater.take((pressure / 250).clamp(1, 256));
            water.runoff.add_assign(discharge)?;
            water.last_spring_hu = discharge.water_hu.min(u64::from(u32::MAX)) as u32;
            self.water.springs[spring_index].last_discharge_hu = water.last_spring_hu;
            self.water.springs[spring_index].active = discharge.water_hu != 0;
        } else {
            water.last_spring_hu = 0;
            if let Some(spring_index) = spring_index {
                self.water.springs[spring_index].last_discharge_hu = 0;
                self.water.springs[spring_index].active = false;
            }
        }
        if let Some((river_id, baseflow)) = take_river_baseflow(hydro, &mut water) {
            self.plan_surface_credit(
                surface_reservoir_id(SurfaceReservoirKind::River, river_id),
                baseflow,
            );
        }

        // Route a bounded parcel of standing runoff through the immutable
        // seam-aware drainage graph. Incoming parcels are applied after
        // the complete old-state pass.
        let runoff_before = water.runoff.water_hu;
        let routed = water.runoff.take((water.runoff.water_hu / 4).max(u64::from(
            water.runoff.water_hu >= HYDRO_UNITS_PER_VISIBLE_LEVEL,
        )));
        if routed.water_hu != 0 {
            if let Some(id) = surface_id {
                self.plan_surface_credit(id, routed);
            } else if hydro.drainage_receiver != u32::MAX {
                let receiver = hydro.drainage_receiver as usize;
                let receiver_pos = AtlasPos::from_index(receiver, side)
                    .expect("drainage receiver is validated");
                self.active_runoff_routes.push(RunoffTransport {
                    from: pos,
                    to: receiver_pos,
                    water_hu: routed.water_hu,
                    source_water_before_hu: runoff_before,
                });
                let materialized = self.water.commitments.iter().any(|commitment| {
                    AtlasPos::from_surface(commitment.chunk.block_origin(), side)
                        == receiver_pos
                });
                if materialized {
                    if let Some(inbox) = self
                        .water
                        .inboxes
                        .iter_mut()
                        .find(|inbox| inbox.pos == receiver_pos && inbox.reservoir == 0)
                    {
                        inbox.mass.add_assign(routed)?;
                    } else {
                        self.water.inboxes.push(FluxInbox {
                            pos: receiver_pos,
                            reservoir: 0,
                            mass: routed,
                        });
                    }
                } else {
                    self.water_inbound[receiver].add_assign(routed)?;
                }
            } else {
                water.runoff.add_assign(routed)?;
            }
        }

        let season_wind = seasonal_vector(climate.seasonal_wind, day);
        let wind_anomaly = [
            (-pressure_gradient / 5).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
            ((i32::from(self.cells.cells.values()[south].pressure_anomaly)
                - i32::from(self.cells.cells.values()[north].pressure_anomaly))
                / 3)
            .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
        ];
        let wind = [
            season_wind[0] + f32::from(wind_anomaly[0]) / 16_384.0,
            season_wind[1] + f32::from(wind_anomaly[1]) / 16_384.0,
        ];
        let wind_vector = chart_vector(pos, side, wind);
        let receiver = downstream_neighbor(pos, side, wind_vector);
        let receiver_index = receiver.index(side);
        let stencil = transport_stencil(pos, side, wind_vector);

        let vapor_transport = source.atmospheric_vapor / 4;
        let cloud_transport = source.cloud_water * 3 / 10;
        source.atmospheric_vapor -= vapor_transport;
        source.cloud_water -= cloud_transport;
        let vapor_room = u32::MAX - self.scratch.values()[index].atmospheric_vapor;
        let local_vapor = source.atmospheric_vapor.min(vapor_room);
        self.scratch.values_mut()[index].atmospheric_vapor += local_vapor;
        let mut rejected_vapor = source.atmospheric_vapor - local_vapor;
        let cloud_room = u32::MAX - self.scratch.values()[index].cloud_water;
        let local_cloud = source.cloud_water.min(cloud_room);
        self.scratch.values_mut()[index].cloud_water += local_cloud;
        let mut rejected_cloud = source.cloud_water - local_cloud;
        distribute_u32(vapor_transport, stencil, |target, share| {
            let room = u32::MAX - self.scratch.values()[target].atmospheric_vapor;
            let accepted = share.min(room);
            self.scratch.values_mut()[target].atmospheric_vapor += accepted;
            rejected_vapor += share - accepted;
        });
        spill_vapor(self.scratch.values_mut(), index, rejected_vapor)?;
        distribute_u32(cloud_transport, stencil, |target, share| {
            let room = u32::MAX - self.scratch.values()[target].cloud_water;
            let accepted = share.min(room);
            self.scratch.values_mut()[target].cloud_water += accepted;
            rejected_cloud += share - accepted;
        });
        spill_cloud(self.scratch.values_mut(), index, rejected_cloud)?;

        let temperature_transport = temperature_anomaly * 3 / 10;
        add_i16(
            &mut self.scratch.values_mut()[index].weather_temperature_anomaly,
            temperature_anomaly - temperature_transport,
        );
        add_i16(
            &mut self.scratch.values_mut()[receiver_index].weather_temperature_anomaly,
            temperature_transport,
        );
        let pressure_transport = pressure * 3 / 10;
        add_i16(
            &mut self.scratch.values_mut()[index].pressure_anomaly,
            pressure - pressure_transport,
        );
        add_i16(
            &mut self.scratch.values_mut()[receiver_index].pressure_anomaly,
            pressure_transport,
        );

        let destination = &mut self.scratch.values_mut()[index];
        self.water_scratch.values_mut()[index] = water;
        destination.local_weather_anomaly = temperature_anomaly;
        destination.storm_energy = storm_energy;
        destination.precipitation_rate = precipitation.min(u32::from(u16::MAX)) as u16;
        destination.wind_anomaly = wind_anomaly;
        destination.fire_moisture_anomaly =
            (source.fire_moisture_anomaly + precipitation.min(i32::MAX as u32) as i32 - 2)
                .clamp(-20_000, 20_000);
        destination.vegetation_moisture_anomaly = (source.vegetation_moisture_anomaly
            + precipitation.min(i32::MAX as u32) as i32
            - actual_evaporation.min(i32::MAX as u32) as i32)
            .clamp(-20_000, 20_000);
        Ok(())
    }
}
