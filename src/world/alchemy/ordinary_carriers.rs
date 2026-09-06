//! Ordinary carriers alchemy transaction coordination.

use super::add_materials;
use super::produced;
use super::result_for;
use super::take_count;
use super::take_exact_slot;
use crate::alchemy::AlchemyCueKind;
use crate::alchemy::AlchemyRequest;
use crate::alchemy::AlchemyResult;
use crate::alchemy::ApparatusKind;
use crate::inventory::Inventory;
use crate::planet::BlockPos;
use crate::planet_atlas::HYDRO_UNITS_PER_BLOCK;
use crate::planet_atlas::ReservoirMass;
use crate::planet_atlas::WaterClass;
use crate::registry::MaterialVector;
use crate::world::World;

impl World {
    pub(super) fn alchemy_ferment(
        &mut self,
        pos: BlockPos,
        request: &AlchemyRequest,
        slots: [usize; 3],
        state: &mut crate::alchemy::AlchemyState,
        inventory: &mut Inventory,
    ) -> Result<AlchemyResult, String> {
        self.ordinary_carrier_job(
            pos,
            request,
            slots,
            crate::alchemy::OrdinaryProcessKind::FermentAlcohol,
            state,
            inventory,
        )
    }

    pub(super) fn alchemy_press_oil(
        &mut self,
        pos: BlockPos,
        request: &AlchemyRequest,
        seed_slot: usize,
        state: &mut crate::alchemy::AlchemyState,
        inventory: &mut Inventory,
    ) -> Result<AlchemyResult, String> {
        self.ordinary_carrier_job(
            pos,
            request,
            [seed_slot, seed_slot, seed_slot],
            crate::alchemy::OrdinaryProcessKind::PressOil,
            state,
            inventory,
        )
    }

    pub(super) fn ordinary_carrier_job(
        &mut self,
        pos: BlockPos,
        request: &AlchemyRequest,
        slots: [usize; 3],
        kind: crate::alchemy::OrdinaryProcessKind,
        state: &mut crate::alchemy::AlchemyState,
        inventory: &mut Inventory,
    ) -> Result<AlchemyResult, String> {
        let now = self.alchemy_tick();
        let apparatus = state
            .apparatus
            .get(&pos)
            .ok_or("The apparatus is not installed.")?;
        let required = match kind {
            crate::alchemy::OrdinaryProcessKind::FermentAlcohol => ApparatusKind::InfusionBasin,
            crate::alchemy::OrdinaryProcessKind::PressOil => ApparatusKind::Mortar,
        };
        if apparatus.kind != required || apparatus.batch.is_some() {
            return Err("That ordinary carrier process needs its empty declared apparatus.".into());
        }
        if let Some(job) = state.ordinary_jobs.get(&pos).cloned() {
            if job.kind != kind {
                return Err(
                    "A different ordinary carrier process already occupies this apparatus.".into(),
                );
            }
            if now < job.due_tick {
                return Err(format!(
                    "The ordinary process still needs {} ticks.",
                    job.due_tick - now
                ));
            }
            self.preflight_industrial_water_return(pos, job.process_water, false)?;
            let item = self
                .reg
                .item_id(&job.output_item)
                .ok_or("The ordinary carrier output is unavailable.")?;
            if inventory.add(&self.reg, item, u32::from(job.output_count)) != 0 {
                return Err("Make inventory room before collecting the carrier output.".into());
            }
            state.ordinary_jobs.remove(&pos);
            let apparatus = state.apparatus.get_mut(&pos).expect("apparatus existed");
            apparatus.cleanliness_permille = apparatus.cleanliness_permille.saturating_sub(40);
            apparatus.integrity_permille = apparatus.integrity_permille.saturating_sub(2);
            apparatus.revision = apparatus.revision.saturating_add(1);
            apparatus.last_operator = request.actor;
            let result = result_for(
                &self.reg,
                state,
                pos,
                AlchemyCueKind::Pour,
                "The timed ordinary process returns a measured finite carrier; no magical yield bonus applies.",
                Some(produced(
                    &self.reg,
                    &job.output_item,
                    u32::from(job.output_count),
                    0,
                )?),
            )?;
            state.validate().map_err(|error| error.to_string())?;
            if !job.input_materials.is_empty() {
                self.material_ledger
                    .as_mut()
                    .ok_or("The finite material ledger is unavailable.")?
                    .record_consumption(&job.input_materials)
                    .map_err(|error| error.to_string())?;
            }
            self.apply_industrial_water_return(pos, job.process_water, false)?;
            return Ok(result);
        }
        let mut input_materials = MaterialVector::new();
        let (process_water, portable_start) = match kind {
            crate::alchemy::OrdinaryProcessKind::FermentAlcohol => {
                let water_item = self
                    .reg
                    .item_id("base:bucket_water")
                    .ok_or("Fresh water is unavailable.")?;
                let wheat_item = self
                    .reg
                    .item_id("base:wheat")
                    .ok_or("Wheat is unavailable.")?;
                let berry_item = self
                    .reg
                    .item_id("base:berry")
                    .ok_or("Berries are unavailable.")?;
                let water = take_exact_slot(inventory, slots[0], water_item)?;
                let wheat = take_count(inventory, slots[1], wheat_item, 2)?;
                let berries = take_count(inventory, slots[2], berry_item, 2)?;
                // The water stack's ordinary matter is its reusable bucket;
                // that vessel returns below and therefore is not part of the
                // fermented feedstock material sink.
                let _ = water;
                add_materials(
                    &mut input_materials,
                    &crate::materials::stack_materials(&self.reg, wheat),
                )?;
                add_materials(
                    &mut input_materials,
                    &crate::materials::stack_materials(&self.reg, berries),
                )?;
                let bucket = self
                    .reg
                    .item_id("base:bucket")
                    .ok_or("The empty bucket is unavailable.")?;
                if inventory.add(&self.reg, bucket, 1) != 0 {
                    return Err("Make room for the reusable bucket before fermenting.".into());
                }
                let mass = self
                    .weather_state
                    .live()
                    .ok_or("Fermentation needs the authoritative planetary water cycle.")?
                    .preview_move_portable_to_industrial(WaterClass::Fresh, HYDRO_UNITS_PER_BLOCK)
                    .ok_or("The portable-water ledger cannot fund fermentation.")?;
                (mass, Some((WaterClass::Fresh, mass)))
            }
            crate::alchemy::OrdinaryProcessKind::PressOil => {
                let seeds = self
                    .reg
                    .item_id("base:wheat_seeds")
                    .ok_or("Wheat seed is unavailable.")?;
                let stack = take_count(inventory, slots[0], seeds, 4)?;
                add_materials(
                    &mut input_materials,
                    &crate::materials::stack_materials(&self.reg, stack),
                )?;
                (ReservoirMass::default(), None)
            }
        };
        let (output_item, due_tick) = match kind {
            crate::alchemy::OrdinaryProcessKind::FermentAlcohol => {
                ("base:fermented_alcohol", now.saturating_add(1_200))
            }
            crate::alchemy::OrdinaryProcessKind::PressOil => {
                ("base:plant_oil", now.saturating_add(400))
            }
        };
        state.ordinary_jobs.insert(
            pos,
            crate::alchemy::OrdinaryProcessJob {
                kind,
                installation_id: apparatus.installation_id,
                actor: request.actor,
                started_tick: now,
                due_tick,
                output_item: output_item.into(),
                output_count: 4,
                process_water,
                input_materials,
            },
        );
        let apparatus = state.apparatus.get_mut(&pos).expect("apparatus existed");
        apparatus.revision = apparatus.revision.saturating_add(1);
        apparatus.last_operator = request.actor;
        let result = result_for(
            &self.reg,
            state,
            pos,
            if kind == crate::alchemy::OrdinaryProcessKind::FermentAlcohol {
                AlchemyCueKind::Bubble
            } else {
                AlchemyCueKind::Grind
            },
            "The ordinary carrier process has begun and will not finish before its measured time.",
            None,
        )?;
        state.validate().map_err(|error| error.to_string())?;
        if let Some((class, expected)) = portable_start {
            let moved = self
                .weather_state
                .live_mut()
                .and_then(|weather| {
                    weather.move_portable_to_industrial(class, HYDRO_UNITS_PER_BLOCK)
                })
                .ok_or("Preflighted fermentation water unexpectedly failed to move.")?;
            debug_assert_eq!(moved, expected);
        }
        Ok(result)
    }
}
