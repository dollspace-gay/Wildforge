//! Rootwake workings transaction coordination.

use crate::planet::BlockPos;
use crate::workings::PhysicalDebit;
use crate::workings::PhysicalDebitKind;
use crate::workings::PlantAdvance;
use crate::workings::WorkingEffect;
use crate::workings::WorkingResult;
use crate::workings::WorkingTargetSnapshot;
use crate::world::World;
use crate::world::soil;
use super::ROOTWAKE_NUTRIENT_UNITS;
use super::ROOTWAKE_WATER_HU;
use super::working_distance;

impl World {
    /// Rootwake is factored out of the random-tick path but validates the same
    /// climate/light/soil authority and additionally reserves explicit water
    /// and nutrients before changing one crop stage.
    #[cfg(test)]
    pub fn begin_rootwake_working(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        plant: BlockPos,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        self.begin_rootwake_working_definition(
            actor,
            actor_label,
            source,
            wand_id,
            "base:rootwake",
            plant,
            forced,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn begin_rootwake_working_definition(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        working_id: &str,
        plant: BlockPos,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        let (advance, targets) = self.prepare_rootwake(plant)?;
        let mut physical = vec![PhysicalDebit {
            kind: PhysicalDebitKind::SoilWater,
            source: format!("plant:{plant:?}"),
            content_id: "water".into(),
            units: advance.water_hu,
            expected_version: 0,
        }];
        if advance.nutrient_units != 0 {
            physical.push(PhysicalDebit {
                kind: PhysicalDebitKind::SoilNutrients,
                source: format!("soil:{:?}", advance.soil_pos),
                content_id: "soil_nutrients".into(),
                units: advance.nutrient_units,
                expected_version: 0,
            });
        }
        self.reserve_wand_effect(
            actor,
            actor_label,
            source,
            wand_id,
            working_id,
            targets,
            physical,
            WorkingEffect::AdvancePlant(advance),
            1,
            working_distance(source, plant),
            0,
            forced,
        )
    }

    pub(super) fn prepare_rootwake(
        &self,
        plant: BlockPos,
    ) -> Result<(PlantAdvance, Vec<WorkingTargetSnapshot>), String> {
        if self.environmental_dross_band_at(plant) >= crate::dross::DrossBand::Scar {
            return Err(
                "Scar pressure has stalled this living bed; isolate and remediate it before forcing growth."
                    .into(),
            );
        }
        let before = self.get_block_at(plant);
        let definition = self.reg.block(before);
        if definition.sapling.is_some() {
            return self.prepare_sapling_rootwake(plant);
        }
        let next = definition
            .crop_next
            .ok_or("Rootwake supports only a declared growing plant, crop, or sapling.")?;
        let soil_pos = plant
            .offset(0, -1, 0)
            .ok_or("The plant has no supporting soil cell.")?;
        let farmland = self.reg.block_id("base:farmland");
        if !definition.crop_any_soil && Some(self.get_block_at(soil_pos)) != farmland {
            return Err("The crop is not rooted in prepared soil.".into());
        }
        if definition.crop_any_soil && !self.heart_alive_at_surface(plant.surface()) {
            return Err("A dead heart remains authoritative over wild biological renewal.".into());
        }
        let season = self.season_at_surface(plant.surface());
        let (block_light, sky_light) = self.light_at_pos(plant);
        let protected = block_light >= 10
            && (sky_light < 15
                || (1..=16)
                    .filter_map(|dy| plant.offset(0, dy, 0))
                    .any(|pos| self.reg.block(self.get_block_at(pos)).glass));
        if !definition.crop_any_soil && season == 3 && !protected {
            return Err("Winter stops this crop outside a lit greenhouse.".into());
        }
        if definition.crop_any_soil && !matches!(season, 1 | 2) {
            return Err("This wild plant is outside its fruiting season.".into());
        }
        let effective_temperature = self.weather_at_surface(plant.surface()).temperature_c
            + if protected { 10.0 } else { 0.0 };
        if !(0.0..42.0).contains(&effective_temperature) {
            return Err("The habitat temperature cannot support this growth interval.".into());
        }
        if block_light.max(sky_light) < 9 {
            return Err("The plant does not have enough ordinary light.".into());
        }
        if !definition.crop_any_soil {
            if let Some(failure) = self.soil_failure_at(soil_pos) {
                return Err(format!("The soil refuses growth: {failure}."));
            }
            if self.fertility_at_pos(soil_pos) < ROOTWAKE_NUTRIENT_UNITS as u8 {
                return Err("The prepared soil has no nutrient budget left.".into());
            }
        }
        let (water_source, water_before) = crate::planet::neighbors6(plant)
            .filter_map(|pos| self.water_mass_at(pos).map(|mass| (pos, mass)))
            .filter(|(_, mass)| mass.water_hu >= ROOTWAKE_WATER_HU)
            .min_by_key(|(pos, _)| *pos)
            .ok_or("Rootwake needs a real adjacent water reservoir to debit.")?;
        let mut water_after = water_before;
        let parcel = water_after.take(ROOTWAKE_WATER_HU);
        let before_soil_meta = self.get_meta_at(soil_pos);
        let after_soil_meta = if definition.crop_any_soil {
            before_soil_meta
        } else {
            soil::soil_meta(
                soil::fert_of(before_soil_meta).saturating_sub(ROOTWAKE_NUTRIENT_UNITS as u8),
                soil::family_of(before_soil_meta),
            )
        };
        let advance = PlantAdvance {
            pos: plant,
            before_block: before.0,
            after_block: next.0,
            before_meta: self.get_meta_at(plant),
            after_meta: self.get_meta_at(plant),
            soil_pos: Some(soil_pos),
            before_soil_meta,
            after_soil_meta,
            water_source: Some(water_source),
            water_before_hu: water_before.water_hu,
            water_after_hu: water_after.water_hu,
            salt_before: water_before.salt_mass,
            salt_after: water_after.salt_mass,
            water_hu: parcel.water_hu,
            nutrient_units: if definition.crop_any_soil {
                0
            } else {
                ROOTWAKE_NUTRIENT_UNITS
            },
        };
        let targets = vec![
            self.block_snapshot(plant),
            self.block_snapshot(soil_pos),
            self.reservoir_snapshot(water_source, water_before),
        ];
        Ok((advance, targets))
    }

    pub(super) fn prepare_sapling_rootwake(
        &self,
        plant: BlockPos,
    ) -> Result<(PlantAdvance, Vec<WorkingTargetSnapshot>), String> {
        let before = self.get_block_at(plant);
        let definition = self.reg.block(before);
        if definition.sapling.is_none() {
            return Err("Rootwake target is not a declared sapling.".into());
        }
        let before_meta = self.get_meta_at(plant);
        if before_meta & 1 != 0 {
            return Err(
                "This sapling has already received its bounded accelerated interval.".into(),
            );
        }
        let soil_pos = plant
            .offset(0, -1, 0)
            .ok_or("The sapling has no supporting prepared soil.")?;
        if self
            .reg
            .block(self.get_block_at(soil_pos))
            .fert_tiles
            .is_none()
        {
            return Err(
                "Rootwake needs the sapling rooted in prepared finite-nutrient soil.".into(),
            );
        }
        if let Some(failure) = self.soil_failure_at(soil_pos) {
            return Err(format!("The sapling's soil refuses growth: {failure}."));
        }
        if self.fertility_at_pos(soil_pos) < ROOTWAKE_NUTRIENT_UNITS as u8 {
            return Err("The sapling's prepared soil has no nutrient budget left.".into());
        }
        let (block_light, sky_light) = self.light_at_pos(plant);
        if block_light.max(sky_light) < 9 {
            return Err("The sapling does not have enough ordinary light.".into());
        }
        let temperature = self.weather_at_surface(plant.surface()).temperature_c;
        if !(0.0..42.0).contains(&temperature) {
            return Err("The habitat temperature cannot support this sapling interval.".into());
        }
        if (1..=2)
            .filter_map(|dy| plant.offset(0, dy, 0))
            .any(|pos| !self.reg.is_replaceable(self.get_block_at(pos)))
        {
            return Err("The sapling lacks even the bounded space for its next interval.".into());
        }
        let (water_source, water_before) = crate::planet::neighbors6(plant)
            .filter_map(|pos| self.water_mass_at(pos).map(|mass| (pos, mass)))
            .filter(|(_, mass)| mass.water_hu >= ROOTWAKE_WATER_HU)
            .min_by_key(|(pos, _)| *pos)
            .ok_or("Rootwake needs a real adjacent water reservoir to debit.")?;
        let mut water_after = water_before;
        let parcel = water_after.take(ROOTWAKE_WATER_HU);
        let before_soil_meta = self.get_meta_at(soil_pos);
        let after_soil_meta = soil::soil_meta(
            soil::fert_of(before_soil_meta).saturating_sub(ROOTWAKE_NUTRIENT_UNITS as u8),
            soil::family_of(before_soil_meta),
        );
        let advance = PlantAdvance {
            pos: plant,
            before_block: before.0,
            after_block: before.0,
            before_meta,
            after_meta: before_meta | 1,
            soil_pos: Some(soil_pos),
            before_soil_meta,
            after_soil_meta,
            water_source: Some(water_source),
            water_before_hu: water_before.water_hu,
            water_after_hu: water_after.water_hu,
            salt_before: water_before.salt_mass,
            salt_after: water_after.salt_mass,
            water_hu: parcel.water_hu,
            nutrient_units: ROOTWAKE_NUTRIENT_UNITS,
        };
        Ok((
            advance,
            vec![
                self.block_snapshot(plant),
                self.block_snapshot(soil_pos),
                self.reservoir_snapshot(water_source, water_before),
            ],
        ))
    }
}
