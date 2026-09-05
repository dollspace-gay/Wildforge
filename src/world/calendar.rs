//! Seasons, weather locality, ire, offerings, and renewable growth.

use super::*;

impl World {
    pub(super) fn calendar_view(&self) -> super::calendar_view::CalendarView {
        super::calendar_view::CalendarView::new(self.day, self.clock, self.long_winter)
    }

    fn orbital_day(&self) -> f64 {
        self.clock / f64::from(crate::server::DAY_LENGTH)
    }

    pub fn sun_direction(&self) -> glam::DVec3 {
        self.calendar_view().sun_direction()
    }

    pub fn latitude_at_surface(&self, pos: crate::planet::SurfacePos) -> f64 {
        super::calendar_view::CalendarView::latitude(pos)
    }

    /// The local temperature for a particular orbital day. Offline crop
    /// reconciliation uses this instead of applying today's weather to every
    /// season the unloaded field missed.
    pub(crate) fn temperature_at_surface_on_day(
        &self,
        pos: crate::planet::SurfacePos,
        day: f64,
    ) -> f32 {
        if let (Some(atlas), Some(weather)) = (&self.planet_atlas, &self.planetary_weather) {
            return weather
                .sample(atlas, pos, day, self.long_winter)
                .temperature_c;
        }
        if let Some(sample) = self.replica_observations.weather_at(pos) {
            return sample.temperature_c;
        }
        crate::climate::seasonal_temperature(self.generator.climate_at(pos).t, pos, day)
    }

    /// Local astronomical season. The Long Winter is a supernatural thermal
    /// anomaly, so it suppresses growth everywhere without freezing the orbit.
    pub fn season_at_surface(&self, pos: crate::planet::SurfacePos) -> usize {
        self.calendar_view().season_at(pos)
    }

    pub fn daylight_at_surface(&self, pos: crate::planet::SurfacePos) -> f32 {
        self.calendar_view().daylight_at(pos)
    }

    pub fn weather_at_surface(
        &self,
        pos: crate::planet::SurfacePos,
    ) -> crate::planet_atlas::LocalWeatherSample {
        if let (Some(atlas), Some(weather)) = (&self.planet_atlas, &self.planetary_weather) {
            return weather.sample(atlas, pos, self.orbital_day(), self.long_winter);
        }
        if let Some(sample) = self.replica_observations.weather_at(pos) {
            return sample;
        }
        // Atlas-free fixtures and development worlds still need a physically
        // sane local temperature. The old fixed +14 C spring/summer offset
        // overheated the equator above crop tolerance and applied the same
        // seasonal swing at every latitude. Preserve the generator's broad
        // latitude field as the annual mean, then scale the orbital anomaly
        // by signed latitude: no equatorial season spike, opposite
        // hemispheres, strongest variation toward the poles.
        let temperature_c = self.temperature_at_surface_on_day(pos, f64::from(self.day));
        if let Some(mut sample) = self.weather_override {
            sample.temperature_c = temperature_c;
            if sample.kind.precipitating()
                && sample.precipitation == crate::planet_atlas::PrecipitationForm::None
            {
                sample.precipitation = if temperature_c <= 0.0 {
                    crate::planet_atlas::PrecipitationForm::Snow
                } else {
                    crate::planet_atlas::PrecipitationForm::Rain
                };
            }
            return sample;
        }
        let kind = crate::planet_atlas::LocalWeather::Clear;
        crate::planet_atlas::LocalWeatherSample {
            kind,
            precipitation: if !kind.precipitating() {
                crate::planet_atlas::PrecipitationForm::None
            } else if temperature_c <= 0.0 {
                crate::planet_atlas::PrecipitationForm::Snow
            } else {
                crate::planet_atlas::PrecipitationForm::Rain
            },
            temperature_c,
            ..crate::planet_atlas::LocalWeatherSample::default()
        }
    }

    pub fn soil_moisture_at_surface(&self, pos: crate::planet::SurfacePos) -> f32 {
        let (Some(atlas), Some(weather)) = (&self.planet_atlas, &self.planetary_weather) else {
            return 1.0;
        };
        let atlas_pos = atlas.atlas_pos(pos);
        let index = atlas_pos.index(atlas.side());
        let baseline = (atlas.genesis.climate.values()[index].mean_precipitation * 4.0).max(256.0)
            * crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL as f32;
        (weather.water.cells.values()[index].soil.water_hu as f32 / baseline).clamp(0.0, 1.5)
    }

    /// Claim landed precipitation for materialization by the voxel water
    /// cycle. Atlas worlds debit the exact coarse runoff/snow reserve; small
    /// atlas-free fixtures retain their synthetic precipitation behavior.
    pub(super) fn claim_precipitation_transfer(
        &mut self,
        pos: crate::planet::SurfacePos,
        form: crate::planet_atlas::PrecipitationForm,
        requested: u32,
    ) -> u32 {
        if let Some(atlas) = &self.planet_atlas {
            let atlas_pos = atlas.atlas_pos(pos);
            return self.planetary_weather.as_mut().map_or(0, |weather| {
                weather.withdraw_water_cycle_transfer(atlas_pos, form, requested)
            });
        }
        let sample = self.weather_at_surface(pos);
        if sample.precipitation == form {
            requested
        } else {
            0
        }
    }

    /// Development/capture override. Water is only moved between vapor and
    /// cloud; even a forced storm cannot mint atmospheric mass.
    pub fn force_local_weather(&mut self, requested: &str) {
        let (requested, forced_form) = match requested {
            "overcast" => (
                crate::planet_atlas::LocalWeather::Overcast,
                crate::planet_atlas::PrecipitationForm::None,
            ),
            "rain" => (
                crate::planet_atlas::LocalWeather::Precipitation,
                crate::planet_atlas::PrecipitationForm::Rain,
            ),
            "snow" => (
                crate::planet_atlas::LocalWeather::Precipitation,
                crate::planet_atlas::PrecipitationForm::Snow,
            ),
            "precip" => (
                crate::planet_atlas::LocalWeather::Precipitation,
                crate::planet_atlas::PrecipitationForm::None,
            ),
            "storm" => (
                crate::planet_atlas::LocalWeather::Storm,
                crate::planet_atlas::PrecipitationForm::None,
            ),
            _ => (
                crate::planet_atlas::LocalWeather::Clear,
                crate::planet_atlas::PrecipitationForm::None,
            ),
        };
        let Some(weather) = &mut self.planetary_weather else {
            self.weather_override = Some(crate::planet_atlas::LocalWeatherSample {
                kind: requested,
                precipitation: forced_form,
                precipitation_units: u16::from(requested.precipitating()),
                ..crate::planet_atlas::LocalWeatherSample::default()
            });
            return;
        };
        self.weather_override = None;
        for cell in weather.cells.cells.values_mut() {
            let total = cell.atmospheric_vapor.saturating_add(cell.cloud_water);
            match requested {
                crate::planet_atlas::LocalWeather::Clear => {
                    cell.atmospheric_vapor = total;
                    cell.cloud_water = 0;
                    cell.storm_energy = 0;
                    cell.precipitation_rate = 0;
                }
                crate::planet_atlas::LocalWeather::Overcast => {
                    cell.cloud_water = total / 3;
                    cell.atmospheric_vapor = total - cell.cloud_water;
                    cell.storm_energy = 8_000;
                    cell.precipitation_rate = 0;
                }
                crate::planet_atlas::LocalWeather::Precipitation => {
                    cell.cloud_water = total / 2;
                    cell.atmospheric_vapor = total - cell.cloud_water;
                    cell.storm_energy = 20_000;
                    cell.precipitation_rate = 1;
                }
                crate::planet_atlas::LocalWeather::Storm => {
                    cell.cloud_water = total * 2 / 3;
                    cell.atmospheric_vapor = total - cell.cloud_water;
                    cell.storm_energy = 52_000;
                    cell.precipitation_rate = 1;
                }
            }
        }
    }

    /// Slice one authoritative climate-hour update. A production pass is
    /// spread over roughly three seconds of ordinary 30 Hz server ticks.
    pub fn tick_planetary_weather(
        &mut self,
        budget: usize,
    ) -> Result<Option<crate::planet_atlas::WeatherStepReport>, crate::planet_atlas::AtlasError>
    {
        let Some(atlas) = self.planet_atlas.clone() else {
            return Ok(None);
        };
        let dross_completed = self
            .arcane_geography
            .as_ref()
            .map(|geography| geography.dynamic.dross_state.completed_steps);
        let Some(weather) = self.planetary_weather.as_mut() else {
            return Ok(None);
        };
        let day = self.clock / f64::from(crate::server::DAY_LENGTH);
        let target_hour = (day * 24.0).floor().max(0.0) as u64;
        let dross_needs_previous_routes = dross_completed
            .is_some_and(|completed| weather.completed_hours > completed.saturating_add(1));
        if !weather.is_updating()
            && !dross_needs_previous_routes
            && weather.completed_hours <= target_hour
        {
            weather.begin_hour(weather.completed_hours);
        }
        let global_ire = self.ire;
        let regional_ire = &self.regional_ire;
        let report = weather.advance_slice(&atlas, day, budget, |pos| {
            let center = pos.center(atlas.side());
            let surface = crate::planet::SurfacePos::new(
                center.face,
                center
                    .u
                    .floor()
                    .clamp(0.0, f64::from(crate::planet::FACE_BLOCKS - 1)) as u16,
                center
                    .v
                    .floor()
                    .clamp(0.0, f64::from(crate::planet::FACE_BLOCKS - 1)) as u16,
            )
            .expect("atlas center is a canonical surface position");
            (global_ire
                + regional_ire
                    .get(&RegionCell::from_surface(surface))
                    .copied()
                    .unwrap_or(0.0)
                    * 3.0)
                .clamp(0.0, 100.0)
        });
        let report = match report {
            Ok(report) => report,
            Err(error) => {
                weather.abort_failed_hour();
                return Err(error);
            }
        };
        if report.is_some() {
            self.apply_loaded_water_inboxes();
            // Springs and changing shorelines can touch hundreds of loaded
            // water cells in one climate hour. Relighting after every cell
            // made the window appear permanently frozen while the same
            // connected chunks were rebuilt over and over. Preserve all
            // ordinary mutation behavior, but settle their shared light field
            // once after the complete water-cycle transaction.
            self.edit_batch(|world| {
                world.reconcile_loaded_springs();
                world.reconcile_loaded_shores();
            });
        }
        Ok(report)
    }

    pub(super) fn apply_loaded_water_inboxes(&mut self) {
        let (Some(atlas), Some(weather)) = (&self.planet_atlas, &mut self.planetary_weather) else {
            return;
        };
        // A sliced climate hour owns a second water-cell grid. Moving a flux
        // inbox into the live grid after that cell was processed would be
        // discarded by the final swap (the production cold-streaming loss).
        // Keep the parcel in its explicit inbox until the transaction
        // completes; `tick_planetary_weather` calls us immediately afterward.
        if weather.is_updating() {
            return;
        }
        let loaded = self
            .chunks
            .keys()
            .map(|chunk| {
                crate::planet_atlas::AtlasPos::from_surface(chunk.block_origin(), atlas.side())
            })
            .collect::<std::collections::BTreeSet<_>>();
        let mut index = 0;
        while index < weather.water.inboxes.len() {
            if !loaded.contains(&weather.water.inboxes[index].pos) {
                index += 1;
                continue;
            }
            let inbox = weather.water.inboxes.remove(index);
            let cell_index = inbox.pos.index(weather.water.cells.side());
            weather.water.cells.values_mut()[cell_index]
                .runoff
                .add_assign(inbox.mass)
                .expect("loaded flux inbox fits runoff reservoir");
        }
    }

    fn reconcile_loaded_springs(&mut self) {
        let (Some(atlas), Some(weather)) = (&self.planet_atlas, &self.planetary_weather) else {
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
            let claimed = self.planetary_weather.as_mut().map_or(
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

    fn reconcile_loaded_shores(&mut self) {
        let (Some(atlas), Some(weather)) = (&self.planet_atlas, &self.planetary_weather) else {
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
                let parcel = self.planetary_weather.as_mut().map_or(
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
                    .planetary_weather
                    .as_mut()
                    .is_some_and(|weather| weather.dematerialize_surface_water(reservoir, mass));
                if returned {
                    self.set_block_at(at, crate::registry::AIR);
                }
            }
        }
    }

    /// 0 spring, 1 summer, 2 autumn, 3 winter.
    #[cfg(test)]
    pub fn season(&self) -> usize {
        // The Long Winter: enough countries dead and the year stops
        // turning. Everything winter already means — crops at zero,
        // no breeding, halved repopulation, water freezing — arrives
        // for free, because it IS winter, world-wide, until enough
        // hearts are relit.
        if self.long_winter {
            return 3;
        }
        ((self.day / SEASON_DAYS) % 4) as usize
    }

    /// How many known countries have lost their spirit, and how many
    /// are known at all.
    pub fn dead_countries(&self) -> (usize, usize) {
        // Ancient scars do not count, on either side of the ratio. The
        // badlands died before anyone alive walked there; letting them
        // into the tally would stop the world's year over history the
        // player never touched — walk through three of them early and
        // the Long Winter would fall on a world you had done nothing
        // to. Relight one and it becomes a living country like any
        // other, which is the right way for it to help lift a winter.
        let counted = self
            .hearts
            .values()
            .filter(|h| !self.is_ancient_scar_at(h.pos.surface()));
        let (mut dead, mut known) = (0, 0);
        for h in counted {
            known += 1;
            if h.stage == 0 {
                dead += 1;
            }
        }
        (dead, known)
    }

    /// Re-read whether the world's year has stopped. Returns Some(true)
    /// when the Long Winter falls and Some(false) when it lifts.
    pub(super) fn refresh_long_winter(&mut self) -> Option<bool> {
        let (dead, known) = self.dead_countries();
        // A handful of dead countries is a tragedy, not a winter; it
        // takes both a real count and a real share of the known world.
        let falls = dead >= LONG_WINTER_MIN_DEAD
            && known > 0
            && dead as f32 >= known as f32 * LONG_WINTER_FRAC;
        if falls == self.long_winter {
            return None;
        }
        self.long_winter = falls;
        Some(falls)
    }

    /// 0..1 through the current season.
    pub fn season_progress(&self) -> f32 {
        self.calendar_view().season_progress()
    }

    /// Does the local atmospheric column currently deliver snow?
    pub fn snows_at_surface(&self, pos: crate::planet::SurfacePos) -> bool {
        self.weather_at_surface(pos).precipitation == crate::planet_atlas::PrecipitationForm::Snow
    }

    /// Is any conservative precipitation transfer active in this column?
    /// Deserts are not categorically vetoed; they simply receive little.
    pub fn rains_at_surface(&self, pos: crate::planet::SurfacePos) -> bool {
        self.weather_at_surface(pos).kind.precipitating()
    }

    // ---------------- ire (reciprocity) ----------------

    /// The land's local standing at a canonical planetary surface cell.
    pub fn regional_ire_at_surface(&self, pos: crate::planet::SurfacePos) -> f32 {
        self.regional_ire
            .get(&RegionCell::from_surface(pos))
            .copied()
            .unwrap_or(0.0)
    }

    /// The land's local standing: negative is tended, positive is
    /// aggrieved, clamped to a grudge the wild can actually hold.
    #[cfg(test)]
    pub fn regional_ire_at(&self, x: i32, z: i32) -> f32 {
        self.regional_ire_at_surface(
            crate::planet::SurfacePos::from_centered(crate::planet::Face::PosZ, x, z)
                .expect("legacy regional coordinate is within the bounded porting window"),
        )
    }

    fn charge_cell(&mut self, cell: RegionCell, amt: f32) {
        let e = self.regional_ire.entry(cell).or_insert(0.0);
        *e = (*e + amt).clamp(-20.0, 20.0);
        if e.abs() < 0.01 {
            self.regional_ire.remove(&cell);
        }
    }

    /// Taking, placed: the world remembers, and so does the valley.
    #[cfg(test)]
    pub fn add_ire_at(&mut self, x: i32, z: i32, amt: f32) {
        let pos = crate::planet::SurfacePos::from_centered(crate::planet::Face::PosZ, x, z)
            .expect("legacy regional coordinate is within the bounded porting window");
        self.add_ire_at_surface(pos, amt);
    }

    pub fn add_ire_at_surface(&mut self, pos: crate::planet::SurfacePos, amt: f32) {
        self.add_ire(amt);
        self.charge_cell(RegionCell::from_surface(pos), amt);
    }

    /// Mending, placed: the global refund keeps its daily cap, but the
    /// valley always notices the hands that tend it.
    #[cfg(test)]
    pub fn plant_ire_at(&mut self, x: i32, z: i32, amt: f32) {
        let pos = crate::planet::SurfacePos::from_centered(crate::planet::Face::PosZ, x, z)
            .expect("legacy regional coordinate is within the bounded porting window");
        self.plant_ire_at_surface(pos, amt);
    }

    pub fn plant_ire_at_surface(&mut self, pos: crate::planet::SurfacePos, amt: f32) {
        self.plant_ire(amt);
        self.charge_cell(RegionCell::from_surface(pos), -amt);
        // Tending is also how a cell earns back its bloom.
        self.ease_bloom_debt_at_surface(pos, amt);
    }

    /// Industrial response gradient (capability E12): running machines
    /// feed regional ire alongside extraction — the valley feels a
    /// bloomery's smoke as surely as the mine that fed it. Charged once
    /// per second per lit fire machine; gated behind `ire` and
    /// `industrial_ire` so modes repoint it off cleanly.
    pub const INDUSTRIAL_IRE_PER_SEC: f32 = 0.01;

    /// One-time ire for raising an industrial building (capability E12).
    pub const INDUSTRIAL_BUILDING_IRE: f32 = 0.5;

    /// The delivery contract of the depot at `pos` (capability E13): how
    /// many units of `item` the bound settlement still wants staged, and
    /// the reputation per unit. `None` when the cell is not a depot,
    /// nothing is bound, or the item is not one of its needs.
    pub fn depot_need_at(
        &self,
        pos: BlockPos,
        item: crate::registry::ItemId,
    ) -> Option<(u32, u32)> {
        let Some(BlockEntity::Depot(d)) = self.block_entity_at(&pos) else {
            return None;
        };
        let def = self.reg.settlements.iter().find(|s| s.id == d.settlement)?;
        let need = def.needs.iter().find(|need| need.item == item)?;
        // Staged stock counts against the appetite: a depot full of iron
        // has no more use for iron.
        let staged: u32 = d
            .storage
            .iter()
            .flatten()
            .filter(|stack| stack.item == item)
            .map(|stack| stack.count)
            .sum();
        let wanted = 64u32.saturating_sub(staged.min(64));
        (wanted > 0).then_some((wanted, need.rep_per_unit))
    }

    /// A belt's offer to the depot at `pos`: how many units of this stack
    /// the settlement currently needs (capability E13 belt port).
    pub fn depot_accept(&mut self, pos: BlockPos, stack: &crate::inventory::ItemStack) -> u32 {
        self.depot_deposit(pos, stack)
    }

    /// Transfer a player's held goods into a depot. Solo and networked play
    /// share this boundary: refused goods leave both inventories unchanged,
    /// and the accepted physical stack is debited from its exact source slot.
    pub(crate) fn deliver_to_depot(
        &mut self,
        pos: BlockPos,
        inventory: &mut crate::inventory::Inventory,
        slot: usize,
    ) -> Option<(String, crate::registry::ItemId, u32, u32)> {
        let held = inventory.slots.get(slot).copied().flatten()?;
        let (_, rep_per_unit) = self.depot_need_at(pos, held.item)?;
        let Some(BlockEntity::Depot(depot)) = self.block_entity_at(&pos) else {
            return None;
        };
        let settlement = depot.settlement.clone();
        let accepted = self.depot_deposit(pos, &held);
        if accepted == 0 {
            return None;
        }
        inventory.slots[slot] = (held.count > accepted).then_some(crate::inventory::ItemStack {
            count: held.count - accepted,
            ..held
        });
        Some((settlement, held.item, accepted, rep_per_unit))
    }

    /// Deposit the needed portion of `stack` into the depot at `pos`,
    /// respecting staging capacity. Returns how many units were accepted.
    pub fn depot_deposit(&mut self, pos: BlockPos, stack: &crate::inventory::ItemStack) -> u32 {
        let Some((wanted, _)) = self.depot_need_at(pos, stack.item) else {
            return 0;
        };
        let max_stack = self.reg.item(stack.item).max_stack;
        let Some(BlockEntity::Depot(d)) = self.block_entity_mut_at(&pos) else {
            return 0;
        };
        let offered = stack.count.min(wanted);
        let mut left = offered;
        // Top up part-stacks first.
        for slot in d.storage.iter_mut().flatten() {
            if slot.item == stack.item
                && slot.arcane_id == stack.arcane_id
                && slot.durability == stack.durability
            {
                let take = left.min(max_stack.saturating_sub(slot.count));
                slot.count += take;
                left -= take;
                if left == 0 {
                    break;
                }
            }
        }
        if left > 0 {
            for slot in d.storage.iter_mut() {
                if slot.is_none() {
                    let take = left.min(max_stack);
                    if take > 0 {
                        *slot = Some(crate::inventory::ItemStack {
                            count: take,
                            ..*stack
                        });
                        left -= take;
                    }
                    if left == 0 {
                        break;
                    }
                }
            }
        }
        offered - left
    }

    /// Charge the region for every lit fire machine on a one-second beat.
    pub fn tick_industrial_ire(&mut self, dt: f32) {
        if !self.ruleset().ire || !self.ruleset().industrial_ire {
            return;
        }
        self.industrial_ire_accum += dt;
        if self.industrial_ire_accum < 1.0 {
            return;
        }
        let step = std::mem::take(&mut self.industrial_ire_accum);
        let lit: Vec<crate::planet::SurfacePos> = self
            .block_entities
            .iter()
            .filter_map(|(pos, e)| {
                let BlockEntity::Multiblock(m) = e else {
                    return None;
                };
                let handler = m.kind.handler(&self.reg)?;
                (handler.has_fire() && m.lit).then(|| pos.surface())
            })
            .collect();
        let amt = step * Self::INDUSTRIAL_IRE_PER_SEC * lit.len() as f32;
        if amt <= 0.0 {
            return;
        }
        // One charge per distinct region cell: a workshop row smokes as
        // one chimney, not four.
        let mut cells: std::collections::HashSet<RegionCell> = std::collections::HashSet::new();
        for surface in &lit {
            cells.insert(RegionCell::from_surface(*surface));
        }
        for cell in cells {
            let Some(surface) = cell.any_surface() else {
                continue;
            };
            self.add_ire_at_surface(surface, amt);
        }
    }

    // ---------------- the bloom (wrath as renewal) ----------------

    /// Days of bloom left in a cell: lightning strikes and fallen
    /// wardens charge it; charged country erupts — the green tide
    /// runs hot, flowers and fungi sprout, bushes refruit. The titan
    /// levels the valley and the jungle follows it home.
    #[cfg(test)]
    pub fn bloom_at(&self, x: i32, z: i32) -> f32 {
        let pos = crate::planet::SurfacePos::from_centered(crate::planet::Face::PosZ, x, z)
            .expect("legacy regional coordinate is within the bounded porting window");
        self.bloom_at_surface(pos)
    }

    pub fn bloom_at_surface(&self, pos: crate::planet::SurfacePos) -> f32 {
        self.bloom
            .get(&RegionCell::from_surface(pos))
            .copied()
            .unwrap_or(0.0)
    }

    /// Bank a bloom — but the ground's willingness is finite. A cell
    /// bloomed over and over and never tended gives less each time,
    /// and finally nothing: the storm's gift is not a faucet, and
    /// farming the wild's rage spends something real.
    #[cfg(test)]
    pub fn add_bloom(&mut self, x: i32, z: i32, days: f32) {
        let pos = crate::planet::SurfacePos::from_centered(crate::planet::Face::PosZ, x, z)
            .expect("legacy regional coordinate is within the bounded porting window");
        self.add_bloom_at_surface(pos, days);
    }

    pub fn add_bloom_at_surface(&mut self, pos: crate::planet::SurfacePos, days: f32) {
        let cell = RegionCell::from_surface(pos);
        let spent = self.bloom_spent.get(&cell).copied().unwrap_or(0.0);
        let yield_frac = (1.0 - spent / BLOOM_EXHAUSTION).clamp(0.0, 1.0);
        let given = days * yield_frac;
        if given <= 0.01 {
            return;
        }
        *self.bloom_spent.entry(cell).or_insert(0.0) += given;
        let e = self.bloom.entry(cell).or_insert(0.0);
        *e = (*e + given).min(9.0);
    }

    /// Tending pays the ground back its willingness to bloom.
    pub fn ease_bloom_debt_at_surface(&mut self, pos: crate::planet::SurfacePos, amount: f32) {
        let cell = RegionCell::from_surface(pos);
        if let Some(v) = self.bloom_spent.get_mut(&cell) {
            *v = (*v - amount).max(0.0);
            if *v <= 0.01 {
                self.bloom_spent.remove(&cell);
            }
        }
    }

    /// A hostile fell here: the wild reclaims its own, extravagantly.
    /// Dryads put up a sapling where they stood.
    #[cfg(test)]
    pub fn wild_falls(&mut self, species_name: &str, x: i32, y: i32, z: i32) {
        if let Some(pos) = crate::planet::BlockPos::of_world(x, y, z) {
            self.wild_falls_at(species_name, pos);
        }
    }

    pub fn wild_falls_at(&mut self, species_name: &str, pos: crate::planet::BlockPos) {
        self.add_bloom_at_surface(pos.surface(), 1.0);
        if species_name.contains("dryad")
            && self.get_block_at(pos) == AIR
            && pos.offset(0, -1, 0).is_some_and(|below| {
                self.reg
                    .block(self.get_block_at(below))
                    .name
                    .contains("grass")
            })
            && let Some(sap) = self.reg.block_id("base:oak_sapling")
        {
            self.set_block_at(pos, sap);
        }
    }

    /// The wild's own hand: a bolt out of an ire storm. Strikes only
    /// natural, untouched country; chars grass or dirt to max-fertile
    /// scorch and banks bloom in the cell. Returns the struck cell.
    pub fn lightning_strike_at(
        &mut self,
        surface: crate::planet::SurfacePos,
    ) -> Option<crate::planet::BlockPos> {
        let cp = crate::planet::ChunkPos::from_surface(surface);
        // The invariant, absolute: the wild never touches what
        // players BUILT — a touched chunk is off the target list.
        if self.player_touched.contains(&cp) {
            return None;
        }
        let y = self.surface_height_at(surface);
        if y <= 2 {
            return None;
        }
        let struck =
            crate::planet::BlockPos::new(surface.face(), surface.u(), y as u8, surface.v()).ok()?;
        let name = self.reg.block(self.get_block_at(struck)).name.clone();
        self.add_bloom_at_surface(surface, 3.0);
        if (name == "base:grass" || name == "base:dirt")
            && let Some(ch) = self.reg.block_id("base:charred_soil")
        {
            self.set_block_at(struck, ch);
        }
        // And it starts a fire, which is the wild's to own: it pays
        // bloom where it burns and will not cross onto worked ground.
        if let Some(above) = struck.offset(0, 1, 0) {
            self.light_fire_at(above, false);
        }
        Some(struck)
    }

    #[cfg(test)]
    pub fn lightning_strike(&mut self, x: i32, z: i32) -> Option<(i32, i32, i32)> {
        let surface =
            crate::planet::SurfacePos::from_centered(crate::planet::Face::PosZ, x, z).ok()?;
        self.lightning_strike_at(surface)
            .map(crate::planet::BlockPos::centered)
    }

    pub fn ire_tier(&self) -> usize {
        Self::tier_of(self.ire)
    }

    fn tier_of(ire: f32) -> usize {
        super::calendar_view::ire_tier(ire)
    }

    /// The tier as this ground feels it: the world's mood shifted by
    /// the local ledger (±20 regional ≈ ±2 tiers — an angry forest is
    /// menacing, not lethal; a tended valley forgives a lot).
    #[cfg(test)]
    pub fn ire_tier_at(&self, x: i32, z: i32) -> usize {
        Self::tier_of((self.ire + self.regional_ire_at(x, z) * 3.0).clamp(0.0, 100.0))
    }

    pub fn ire_tier_at_surface(&self, pos: crate::planet::SurfacePos) -> usize {
        Self::tier_of((self.ire + self.regional_ire_at_surface(pos) * 3.0).clamp(0.0, 100.0))
    }

    pub fn add_ire(&mut self, amt: f32) {
        self.ire = (self.ire + amt).clamp(0.0, 100.0);
    }

    /// Planting refunds ire, capped per day — mending stays slower than
    /// taking; a clearcut can't be laundered with a seed drawer.
    pub fn plant_ire(&mut self, amt: f32) {
        let room = (8.0 - self.plant_ire_today).max(0.0);
        let refund = amt.min(room);
        if refund > 0.0 {
            self.plant_ire_today += refund;
            self.add_ire(-refund);
        }
    }

    /// Advance ire time by a fraction of a day: passive decay (-4/day)
    /// and the daily reset of the planting cap. Returns true at dawn
    /// (day rollover) — the moment offerings are accepted.
    pub fn tick_ire(&mut self, day_frac: f32) -> bool {
        // The wild breathes easier when the land drinks.
        let planetary_rain = self.planetary_weather.as_ref().is_some_and(|weather| {
            weather.last_report.precipitation_units > 0
                && weather.last_report.unexplained_water_drift == 0
        });
        let decay = if planetary_rain { 5.0 } else { 4.0 };
        self.add_ire(-decay * day_frac);
        // Grudges and gratitude both fade (2 per day toward zero).
        self.regional_ire.retain(|_, v| {
            *v -= v.signum() * (2.0 * day_frac).min(v.abs());
            v.abs() >= 0.01
        });
        if self.ruleset().hearts {
            self.tick_hearts(day_frac);
        }
        self.refresh_long_winter();
        self.tick_rooting(day_frac);
        self.tick_graft(day_frac);
        // Blooms burn down day by day.
        self.bloom.retain(|_, v| {
            *v -= day_frac;
            *v > 0.0
        });
        self.day_progress += day_frac;
        if self.day_progress >= 1.0 {
            self.day_progress -= 1.0;
            self.plant_ire_today = 0.0;
            // The wild forgives, slowly: a cell held deeply blessed
            // for a full season earns ONE wildlife reseed — its
            // hunted-out chunks roll again when next visited.
            let blessed: Vec<RegionCell> = self
                .regional_ire
                .iter()
                .filter(|(_, v)| **v < -10.0)
                .map(|(c, _)| *c)
                .collect();
            for cell in blessed {
                let streak = self.blessed_streak.entry(cell).or_insert(0);
                *streak += 1;
                if *streak >= SEASON_DAYS {
                    self.blessed_streak.remove(&cell);
                    let (cu0, cv0) = (u16::from(cell.u) * 16, u16::from(cell.v) * 16);
                    for du in 0..16 {
                        for dv in 0..16 {
                            let pos = ChunkPos::new(cell.face, cu0 + du, cv0 + dv)
                                .expect("regional ledger cells partition each face");
                            self.mob_seeded.remove(&pos);
                        }
                    }
                    self.whispers
                        .push("The land breathes. Something returns.".to_string());
                }
            }
            self.blessed_streak
                .retain(|c, _| self.regional_ire.get(c).is_some_and(|&v| v < -10.0));
            return true;
        }
        false
    }

    /// The season's appetite: what the wild wants brought this time
    /// of year, and how it says so. Fixed to the calendar — players
    /// learn the year, not a dice roll.
    #[cfg(test)]
    pub fn season_want(&self) -> (usize, &'static str) {
        Self::want_for_season(self.season())
    }

    pub fn season_want_at_surface(&self, pos: crate::planet::SurfacePos) -> (usize, &'static str) {
        Self::want_for_season(self.season_at_surface(pos))
    }

    fn want_for_season(season: usize) -> (usize, &'static str) {
        super::calendar_view::seasonal_want(season)
    }

    /// Does a stack satisfy the season's want?
    pub fn satisfies_want(&self, want: usize, s: &ItemStack) -> bool {
        let d = self.reg.item(s.item);
        match want {
            // Spring: things that grow — saplings and plantables.
            0 => {
                d.name.ends_with("_sapling")
                    || d.places
                        .is_some_and(|b| self.reg.block(b).crop_next.is_some())
            }
            // Summer: water, carried by hand.
            1 => d.name == "base:bucket_water",
            // Autumn: the harvest's produce (plant nutrition).
            2 => d
                .food
                .as_ref()
                .is_some_and(|f| f.nutrition[..4].iter().any(|&n| n > 0.0)),
            // Winter: anything that feeds.
            _ => d.food.is_some(),
        }
    }

    /// What the wild values: its own materials most, then life given.
    pub fn offering_value(&self, s: &ItemStack) -> f32 {
        let d = self.reg.item(s.item);
        let per = if d.name == "base:diamond" {
            // The wild prizes what the deep earth surrenders rarest.
            6.0
        } else if [
            "base:amethyst_shard",
            "base:gold_ingot",
            "base:silver_ingot",
        ]
        .contains(&d.name.as_str())
        {
            3.0
        } else if [
            "base:heartwood",
            "base:living_wood",
            "base:thorn_fiber",
            "base:dryad_heartwood",
            "base:ember",
            "base:frost_shard",
        ]
        .contains(&d.name.as_str())
        {
            2.0
        } else if d.name.ends_with("_sapling")
            || d.name.contains("raw_")
            || d.name.contains("cooked_")
        {
            1.0
        } else if let Some(f) = &d.food {
            f.hunger * 0.25
        } else {
            0.25
        };
        per * s.count as f32
    }

    /// Dawn: the wild takes everything left on offering stones. Items are
    /// consumed regardless; the refund is capped at 10 per dawn.
    pub fn accept_offerings(&mut self) -> f32 {
        // The ire cells whose country has no spirit left to hear.
        let dead_country: std::collections::HashSet<RegionCell> = self
            .hearts
            .values()
            .filter(|h| h.stage == 0)
            .map(|h| RegionCell::from_surface(h.pos.surface()))
            .collect();
        let mut taken: Vec<(crate::planet::BlockPos, RegionCell, usize, ItemStack)> = Vec::new();
        for (&pos, e) in self.block_entities.iter_mut() {
            let BlockEntity::Offering(o) = e else {
                continue;
            };
            let cell = RegionCell::from_surface(pos.surface());
            // In a country whose heart is dead the stone accepts
            // nothing. Not refused — unreceived. Nobody is home.
            if dead_country.contains(&cell) {
                continue;
            }
            let latitude = crate::planet_atlas::latitude_longitude(crate::planet::surface_to_unit(
                pos.surface().center(),
            ))
            .0;
            let season = if self.long_winter {
                3
            } else {
                crate::planet_atlas::local_season(self.day, latitude)
            };
            let (want, _) = Self::want_for_season(season);
            for slot in o.slots.iter_mut() {
                if let Some(s) = slot.take() {
                    taken.push((pos, cell, want, s));
                }
            }
        }
        if taken.is_empty() {
            return 0.0;
        }
        // Charged gifts remain part of the finite world: the listening
        // country's heart takes their exact mixture. If ledger state is not
        // available or rejects the transfer, give the physical item back as
        // a drop instead of allowing the offering path to destroy Current.
        let mut accepted = Vec::with_capacity(taken.len());
        for (pos, cell, want, stack) in taken {
            if stack.arcane_id != 0 {
                let destination = self.planet_atlas.as_ref().map(|atlas| {
                    atlas
                        .country_at(pos.surface())
                        .map(|country| crate::arcane::ArcaneOwner::Heart(country.id))
                        .unwrap_or_else(|| {
                            crate::arcane::ArcaneOwner::Ambient(atlas.atlas_pos(pos.surface()))
                        })
                });
                let transfer = destination
                    .and_then(|destination| {
                        self.arcane_ledger.as_mut().map(|ledger| {
                            ledger.move_all_item(
                                stack.arcane_id,
                                destination,
                                "charged offering accepted",
                            )
                        })
                    })
                    .transpose();
                if !matches!(transfer, Ok(Some(_))) {
                    if let Err(error) = transfer {
                        eprintln!("arcane: charged offering rejected: {error}");
                    } else {
                        eprintln!("arcane: charged offering rejected: ledger unavailable");
                    }
                    self.push_drop_at(pos, stack);
                    continue;
                }
            }
            accepted.push((pos, cell, want, stack));
        }
        if accepted.is_empty() {
            return 0.0;
        }
        // The season's want counts double — a bonus for listening,
        // never a penalty — and every stone credits its own valley.
        let mut value = 0.0f32;
        for (_, cell, want, s) in &accepted {
            let mut v = self.offering_value(s);
            if self.satisfies_want(*want, s) {
                v *= 2.0;
            }
            value += v;
            self.charge_cell(*cell, -v.min(6.0));
        }
        if let Err(error) =
            self.record_consumed_stacks(accepted.iter().map(|(_, _, _, stack)| *stack))
        {
            eprintln!("materials: offering consumption accounting failed: {error}");
        }
        let refund = value.min(10.0);
        self.add_ire(-refund);
        refund
    }

    /// Grow a planted sapling into a full tree, mirroring the worldgen
    /// shapes. Returns false (sapling stays) if the trunk is blocked.
    pub fn grow_tree_at(&mut self, pos: crate::planet::BlockPos, species: &str, rnd: u32) -> bool {
        let reg = self.reg.clone();
        let ids = |l: &str, f: &str| Some((reg.block_id(l)?, reg.block_id(f)?));
        let Some((log, leaf)) = (match species {
            "birch" => ids("base:birch_log", "base:birch_leaves"),
            "spruce" => ids("base:spruce_log", "base:spruce_leaves"),
            "jungle" => ids("base:jungle_log", "base:jungle_leaves"),
            "acacia" => ids("base:acacia_log", "base:acacia_leaves"),
            _ => ids("base:log", "base:leaves"),
        }) else {
            return false;
        };
        let trunk_h = match species {
            "acacia" => 1,
            "spruce" => 5 + (rnd % 3) as i32,
            "jungle" => 6 + (rnd % 3) as i32,
            _ => 4 + (rnd % 3) as i32,
        };
        // Clearance: the trunk column (above the sapling cell) must be open.
        for dy in 1..=trunk_h + 1 {
            if pos
                .offset(0, dy, 0)
                .is_none_or(|at| self.get_block_at(at) != AIR)
            {
                return false;
            }
        }
        let leaf_at = |w: &mut World, dx: i32, dy: i32, dz: i32| {
            if let Some(at) = pos.offset(dx, dy, dz)
                && at.y() > 0
                && w.get_block_at(at) == AIR
            {
                w.set_block_at(at, leaf);
            }
        };
        for dy in 0..trunk_h {
            if let Some(at) = pos.offset(0, dy, 0) {
                self.set_block_at(at, log);
            }
        }
        match species {
            "acacia" => {
                for dx in -1..=1 {
                    for dz in -1..=1 {
                        leaf_at(self, dx, trunk_h, dz);
                    }
                }
            }
            "spruce" => {
                for (dy, r) in [(-3i32, 2i32), (-2, 1), (-1, 2), (0, 1), (1, 1)] {
                    for dx in -r..=r {
                        for dz in -r..=r {
                            if dx.abs() == r && dz.abs() == r && r > 1 {
                                continue;
                            }
                            if dx == 0 && dz == 0 && dy < 0 {
                                continue;
                            }
                            leaf_at(self, dx, trunk_h + dy, dz);
                        }
                    }
                }
                leaf_at(self, 0, trunk_h + 2, 0);
            }
            _ => {
                let big: i32 = if species == "jungle" { 3 } else { 2 };
                for (dy, r) in [(-2i32, big), (-1, big), (0, 1), (1, 1)] {
                    for dx in -r..=r {
                        for dz in -r..=r {
                            if dx == 0 && dz == 0 && dy < 0 {
                                continue;
                            }
                            leaf_at(self, dx, trunk_h + dy, dz);
                        }
                    }
                }
            }
        }
        true
    }

    /// Ire cost of breaking a block, by what it is.
    pub fn ire_for_block(&self, b: BlockId) -> f32 {
        let name = &self.reg.block(b).name;
        if name.ends_with("_log") || name.ends_with(":log") {
            0.3
        } else if name.contains("ore") {
            0.4
        } else if name.ends_with("stone") && !name.contains("cobble") {
            0.05
        } else if name.contains("leaves") || name.ends_with("dirt") || name.ends_with("grass") {
            0.02
        } else {
            0.0
        }
    }
}
