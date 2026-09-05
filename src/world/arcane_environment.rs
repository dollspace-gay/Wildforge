//! Arcane environment coordinator for the authoritative world.

use super::{World};

impl World {
    pub fn arcane_cue_at(&self, region: crate::planet_atlas::AtlasPos) -> [u8; 2] {
        let geographic = self
            .arcane_geography
            .as_ref()
            .map_or([0; 2], |geography| geography.local_bands(region));
        let sparse = self
            .arcane_ledger
            .as_ref()
            .map_or([0; 2], |ledger| ledger.local_bands(region));
        [geographic[0].max(sparse[0]), geographic[1].max(sparse[1])]
    }

    /// Complete ordinary-player perception packet. Strength and dross remain
    /// coarse bands, while dominant resonance is a category rather than an
    /// exact mixture or amount.
    pub fn arcane_sensory_cue_at(&self, region: crate::planet_atlas::AtlasPos) -> ([u8; 2], u8) {
        let bands = self.arcane_cue_at(region);
        let dominant = self
            .arcane_geography
            .as_ref()
            .map_or(0, |geography| geography.local_dominant_resonance(region));
        (bands, dominant)
    }

    pub fn arcane_survey_at(
        &self,
        region: crate::planet_atlas::AtlasPos,
        tuning_lens: bool,
    ) -> Option<crate::arcane_geography::ArcaneSurvey> {
        self.arcane_geography
            .as_ref()
            .map(|geography| geography.survey(region, tuning_lens))
    }

    pub fn tick_arcane_geography(&mut self, budget: usize) -> std::io::Result<usize> {
        let Some(geography) = &mut self.arcane_geography else {
            return Ok(0);
        };
        let before = geography.dynamic.completed_steps;
        let processed = geography
            .advance_toward(self.calendar_state.clock().max(0.0) as u64, budget)
            .map_err(std::io::Error::other)?;
        let advanced = geography.dynamic.completed_steps != before;
        if advanced {
            self.pressure_wards_from_arcane_environment();
        }
        Ok(processed)
    }

    /// Once per authoritative Current transport step, translate actual local
    /// wakes and dross custody into ward pressure. The ward consumes its own
    /// supply; it neither deletes the environmental load nor edits Ire.
    pub(super) fn pressure_wards_from_arcane_environment(&mut self) {
        let Some(atlas) = self.planet_atlas.as_ref() else {
            return;
        };
        let controllers = self
            .workings_state
            .as_ref()
            .into_iter()
            .flat_map(|state| state.active.values())
            .filter_map(|transaction| {
                if transaction.phase != crate::workings::WorkingPhase::Active {
                    return None;
                }
                match &transaction.effect {
                    crate::workings::WorkingEffect::Ward { controller, .. } => Some(*controller),
                    _ => None,
                }
            })
            .collect::<Vec<_>>();
        let pressures = controllers
            .into_iter()
            .filter_map(|controller| {
                let region = atlas.atlas_pos(controller.surface());
                let geography = self.arcane_geography.as_ref()?;
                let cell = geography
                    .dynamic
                    .cells
                    .get(region.index(geography.manifest.side))?;
                let wake = if cell.wake_id != 0 {
                    geography
                        .dynamic
                        .wakes
                        .iter()
                        .find(|wake| wake.id == u64::from(cell.wake_id))
                        .map(|wake| {
                            wake.charge
                                .into_iter()
                                .chain(wake.dross)
                                .map(u64::from)
                                .sum::<u64>()
                        })
                        .unwrap_or_default()
                } else {
                    0
                };
                let sparse_dross = self.arcane_ledger.as_ref().map_or(0, |ledger| {
                    [
                        crate::arcane::DrossMedium::Soil,
                        crate::arcane::DrossMedium::Water,
                        crate::arcane::DrossMedium::Air,
                    ]
                    .into_iter()
                    .filter_map(|medium| {
                        ledger.account(&crate::arcane::ArcaneOwner::Dross { region, medium })
                    })
                    .fold(0u64, |sum, account| {
                        sum.saturating_add(account.current.total())
                    })
                });
                Some((
                    controller,
                    wake,
                    cell.dross_total().saturating_add(sparse_dross),
                ))
            })
            .collect::<Vec<_>>();
        let bounded = |units: u64| {
            if units == 0 {
                0
            } else {
                units.div_ceil(64).clamp(1, 32)
            }
        };
        for (controller, wake, dross) in pressures {
            let wake = bounded(wake);
            if wake != 0 {
                self.resist_supernatural_pressure_at(controller, "wake", wake);
            }
            let dross = bounded(dross);
            if dross != 0 {
                self.resist_supernatural_pressure_at(controller, "dross", dross);
            }
        }
    }

    /// Sliced whole-planet magical succession. Residency never enters the
    /// decision: loaded blocks are a view of these persistent sites.
    pub fn tick_arcane_ecology(&mut self, budget: usize) -> std::io::Result<usize> {
        let Some(atlas) = self.planet_atlas.as_ref().cloned() else {
            return Ok(0);
        };
        let Some(geography) = self.arcane_geography.as_ref() else {
            return Ok(0);
        };
        let site_positions = geography
            .dynamic
            .ecology
            .sites
            .iter()
            .filter_map(|site| site.surface().map(|surface| (site.atlas_pos, surface)))
            .collect::<Vec<_>>();
        let positions = site_positions
            .iter()
            .map(|(pos, _)| *pos)
            .collect::<std::collections::BTreeSet<_>>();
        let storming = site_positions
            .iter()
            .filter(|(_, surface)| {
                self.weather_at_surface(*surface).kind == crate::planet_atlas::LocalWeather::Storm
            })
            .map(|(pos, _)| *pos)
            .collect::<std::collections::BTreeSet<_>>();
        let crystal_candidates = geography
            .dynamic
            .ecology
            .sites
            .iter()
            .filter(|site| {
                self.reg
                    .arcane_ecology
                    .get(&site.content_id)
                    .is_some_and(|definition| {
                        definition.kind == crate::registry::ArcaneEcologyKind::Crystal
                    })
            })
            .filter_map(|site| {
                Some((
                    site.id,
                    site.content_id.clone(),
                    site.materialized_y,
                    site.surface()?,
                    site.block_pos(),
                ))
            })
            .collect::<Vec<_>>();
        let blocked_crystal_sites = crystal_candidates
            .into_iter()
            .filter_map(|(id, content_id, materialized_y, surface, block_pos)| {
                let chunk = crate::planet::ChunkPos::from_surface(surface);
                let blocked = if materialized_y == 0 {
                    self.player_touched.contains(&chunk)
                } else if self.chunks.contains_key(&chunk) {
                    block_pos.is_some_and(|at| {
                        self.reg
                            .block_id(&content_id)
                            .is_none_or(|expected| self.get_block_at(at) != expected)
                    })
                } else {
                    false
                };
                blocked.then_some(id)
            })
            .collect::<std::collections::BTreeSet<_>>();
        let mut water_available = positions
            .into_iter()
            .map(|pos| {
                let available = self.weather_state.live()
                    .map_or(u64::MAX / 4, |weather| weather.ecology_soil_water_hu(pos));
                (pos, available)
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut living_hearts = atlas
            .biomes
            .countries
            .iter()
            .map(|country| country.id)
            .collect::<std::collections::BTreeSet<_>>();
        for heart in self.hearts.values().filter(|heart| heart.stage == 0) {
            if let Some(country) = atlas.country_at(heart.pos.surface()) {
                living_hearts.remove(&country.id);
            }
        }
        let geography = self
            .arcane_geography
            .as_mut()
            .expect("ecology geography was checked above");
        let previous_completed_day = geography.dynamic.ecology.completed_days;
        let report = crate::arcane_ecology::advance_toward(
            geography,
            &atlas,
            &self.reg,
            u64::from(self.calendar_state.day()),
            budget,
            crate::arcane_ecology::EcologyConditions::new(
                &mut water_available,
                &living_hearts,
                &storming,
                &blocked_crystal_sites,
            ),
        )
        .map_err(std::io::Error::other)?;
        let processed = report.processed;
        let completed_days = report.completed_days;
        let dross_harm = report.dross_harm;
        if let Some(weather) = self.weather_state.live_mut() {
            for (pos, requested) in report.transpiration {
                let moved = weather.transpire_ecology(pos, requested);
                if moved != requested {
                    return Err(std::io::Error::other(format!(
                        "ecology water snapshot promised {requested} HU at {pos:?}, moved {moved}"
                    )));
                }
            }
        }
        // Dross arithmetic is not Ire. Only population losses explicitly
        // reported by succession count as habitat damage; source identity
        // remains in the separate bounded dross evidence mixture.
        for (pos, harmed) in dross_harm {
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
            .expect("atlas ecology position has a canonical surface center");
            self.add_ire_at_surface(surface, harmed.min(8) as f32 * 0.25);
        }
        if completed_days > previous_completed_day {
            self.refresh_loaded_arcane_ecology();
        }
        Ok(processed)
    }

    pub fn arcane_ecology_observation_at(
        &self,
        surface: crate::planet::SurfacePos,
        radius: f32,
    ) -> Option<crate::arcane_ecology::EcologyObservation> {
        self.arcane_geography.as_ref().and_then(|geography| {
            crate::arcane_ecology::observation_at(geography, &self.reg, surface, radius)
        })
    }

    /// Persist a fire/explosion/wildlife loss before its representative voxel
    /// is removed. If the process stops after this linked commit, chunk
    /// reconciliation removes the stale block; if it stops before, neither
    /// state changed durably. Crystal charge therefore cannot rematerialize.
    pub(super) fn settle_arcane_ecology_destruction(
        &mut self,
        pos: crate::planet::BlockPos,
    ) -> Result<bool, String> {
        let Some(geography) = self.arcane_geography.as_mut() else {
            return Ok(false);
        };
        let Some(site_index) = geography.dynamic.ecology.sites.iter().position(|site| {
            site.block_pos() == Some(pos)
                && site.stage != crate::arcane_ecology::EcologyStage::Harvested
        }) else {
            return Ok(false);
        };
        let atlas_index = geography.dynamic.ecology.sites[site_index]
            .atlas_pos
            .index(geography.manifest.side);
        let old_site = geography.dynamic.ecology.sites[site_index].clone();
        let old_cell = geography.dynamic.cells[atlas_index];
        let old_sequence = geography.dynamic.ecology.event_sequence;
        let changed = crate::arcane_ecology::apply_destructive_loss(geography, &self.reg, pos)?;
        if !changed {
            return Ok(false);
        }
        let operation_id = geography.dynamic.ecology.event_sequence.max(1);
        let (manifest, files) =
            match geography.linked_dynamic_replacements(&self.save_dir, operation_id) {
                Ok(prepared) => prepared,
                Err(error) => {
                    geography.dynamic.ecology.sites[site_index] = old_site;
                    geography.dynamic.cells[atlas_index] = old_cell;
                    geography.dynamic.ecology.event_sequence = old_sequence;
                    return Err(error.to_string());
                }
            };
        let Some(ledger) = self.arcane_ledger.as_mut() else {
            geography.dynamic.ecology.sites[site_index] = old_site;
            geography.dynamic.cells[atlas_index] = old_cell;
            geography.dynamic.ecology.event_sequence = old_sequence;
            return Err("arcane ecology destruction requires the parent ledger".into());
        };
        if let Err(error) =
            ledger.commit_geography_state_linked("ecological biomass or crystal destroyed", files)
        {
            geography.dynamic.ecology.sites[site_index] = old_site;
            geography.dynamic.cells[atlas_index] = old_cell;
            geography.dynamic.ecology.event_sequence = old_sequence;
            return Err(error.to_string());
        }
        geography.accept_linked_manifest(manifest);
        Ok(true)
    }

    pub fn perceived_arcane_ecology_at(
        &self,
        surface: crate::planet::SurfacePos,
        radius: f32,
    ) -> Option<crate::arcane_ecology::EcologyObservation> {
        self.arcane_ecology_observation_at(surface, radius)
    }




    pub fn inspectable_item_current(&self, id: u64) -> Option<u64> {
        if id == 0 {
            return None;
        }
        self.arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.item_clean_total(id))

    }
}
