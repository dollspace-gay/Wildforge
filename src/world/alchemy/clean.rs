//! Clean alchemy transaction coordination.

use crate::alchemy::AlchemyAuditEvent;
use crate::alchemy::AlchemyCueKind;
use crate::alchemy::AlchemyRequest;
use crate::alchemy::AlchemyResult;
use crate::arcane::ArcaneOwner;
use crate::world::BlockPos;
use crate::arcane::Current;
use crate::planet_atlas::HYDRO_UNITS_PER_BLOCK;
use crate::inventory::Inventory;
use crate::planet_atlas::ReservoirMass;
use crate::planet_atlas::WaterClass;
use crate::world::World;
use super::add_materials;
use super::produced;
use super::result_for;
use super::take_exact_slot;

impl World {
    pub(super) fn alchemy_clean(
        &mut self,
        pos: BlockPos,
        request: &AlchemyRequest,
        water_slot: usize,
        filter_slot: Option<usize>,
        state: &mut crate::alchemy::AlchemyState,
        inventory: &mut Inventory,
    ) -> Result<AlchemyResult, String> {
        let (
            installation_id,
            batch_snapshot,
            apparatus_residue,
            burden,
            mounted_filter,
            filter_owner_id,
            mounted_filter_materials,
        ) = {
            let apparatus = state
                .apparatus
                .get(&pos)
                .ok_or("The apparatus is not installed.")?;
            (
                apparatus.installation_id,
                apparatus.batch.clone(),
                apparatus.residue_materials.clone(),
                apparatus.filter_burden,
                apparatus.filter_medium.is_some(),
                apparatus.filter_owner_id,
                apparatus.filter_medium_materials.clone(),
            )
        };
        if batch_snapshot.as_ref().is_some_and(|batch| {
            batch.liquid.volume_units != 0 || batch.current_units != 0 || batch.dross_units != 0
        }) {
            return Err(
                "Decant or drain every liquid and Current remainder before cleaning.".into(),
            );
        }
        if batch_snapshot.is_none()
            && apparatus_residue.is_empty()
            && burden == 0
            && !mounted_filter
        {
            return Err("The apparatus is already clean.".into());
        }
        let water_item = self
            .reg
            .item_id("base:bucket_water")
            .ok_or("Fresh cleaning water is unavailable.")?;
        let _water_stack = take_exact_slot(inventory, water_slot, water_item)?;
        let filter_stack = if let Some(slot) = filter_slot {
            let filter_item = self
                .reg
                .item_id("base:filter_cloth")
                .ok_or("Filter cloth is unavailable.")?;
            Some(take_exact_slot(inventory, slot, filter_item)?)
        } else {
            None
        };
        if burden != 0 && filter_stack.is_none() && !mounted_filter {
            return Err(
                "This contaminated apparatus needs a physical filter cloth during cleaning.".into(),
            );
        }
        let empty_bucket = self
            .reg
            .item_id("base:bucket")
            .ok_or("The reusable empty bucket is unavailable.")?;
        if inventory.add(&self.reg, empty_bucket, 1) != 0 {
            return Err("Make room for the reusable empty bucket before cleaning.".into());
        }
        let definition = batch_snapshot
            .as_ref()
            .and_then(|batch| self.reg.preparations.get(&batch.preparation_id));
        let mut residue = apparatus_residue;
        if let Some(batch) = &batch_snapshot {
            for ingredient in &batch.ingredients {
                add_materials(&mut residue, &ingredient.retained_materials)?;
                add_materials(&mut residue, &ingredient.residue_materials)?;
            }
        }
        let mut consumed_materials = residue.clone();
        add_materials(&mut consumed_materials, &mounted_filter_materials)?;
        if let Some(stack) = filter_stack {
            add_materials(
                &mut consumed_materials,
                &crate::materials::stack_materials(&self.reg, stack),
            )?;
        }
        let residue_name = definition
            .map(|definition| definition.residue_item.clone())
            .unwrap_or_else(|| "base:spent_mash".into());
        let residue_count = if batch_snapshot.is_some() || !residue.is_empty() {
            definition.map_or(1, |definition| definition.residue_count)
        } else {
            0
        };
        if residue_count != 0 {
            let residue_item = self
                .reg
                .item_id(&residue_name)
                .ok_or("The declared residue item is unavailable.")?;
            if inventory.add(&self.reg, residue_item, u32::from(residue_count)) != 0 {
                return Err("Make room for the captured physical residue before cleaning.".into());
            }
        }
        let spent_filter = filter_stack.is_some() || mounted_filter;
        let captured_dross = if burden == 0 {
            Current::default()
        } else {
            let current = self
                .arcane_ledger
                .as_ref()
                .and_then(|ledger| ledger.account(&ArcaneOwner::AlchemyDross(filter_owner_id)))
                .map(|account| account.current.clone())
                .ok_or("The mounted filter burden has no finite dross custody.")?;
            if current.total() != burden {
                return Err("The mounted filter's burden and dross ledger disagree.".into());
            }
            current
        };
        let spent_filter_output = if spent_filter {
            let spent_id = if captured_dross.is_empty() {
                0
            } else {
                self.arcane_ledger
                    .as_mut()
                    .ok_or("The finite Current ledger is unavailable.")?
                    .allocate_item_id()
                    .map_err(|error| error.to_string())?
            };
            let output = produced(&self.reg, "base:spent_filter", 1, spent_id)?;
            if inventory.add_stack(
                &self.reg,
                output
                    .clone()
                    .into_stack(&self.reg)
                    .map_err(|error| error.to_string())?,
            ) != 0
            {
                return Err("Make room for the hazardous spent filter before cleaning.".into());
            }
            Some(output)
        } else {
            None
        };
        // Prove the complete portable -> industrial -> runoff exchange using
        // only exact parcel arithmetic. Cloning PlanetaryWeather here copied
        // every climate and hydrology cell for one bucket and was a major
        // laboratory hitch on live planets.
        let atlas_pos = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()))
            .ok_or("Cleaning needs the authoritative planet atlas.")?;
        let residue_water = batch_snapshot
            .as_ref()
            .map_or_else(ReservoirMass::default, |batch| batch.residue_water);
        let cleaning_water = self.weather_state.live()
            .ok_or("Cleaning needs the authoritative planetary water cycle.")?
            .preview_portable_exchange_to_runoff(
                atlas_pos,
                WaterClass::Fresh,
                HYDRO_UNITS_PER_BLOCK,
                residue_water,
            )
            .ok_or("The exact cleaning water and residue cannot enter local runoff.")?;
        let operation_id = state
            .allocate_operation_id()
            .map_err(|error| error.to_string())?;
        let now = self.alchemy_tick();
        let apparatus = state
            .apparatus
            .get_mut(&pos)
            .ok_or("The apparatus disappeared.")?;
        apparatus.batch = None;
        apparatus.residue_materials.clear();
        apparatus.filter_burden = 0;
        apparatus.filter_medium = None;
        apparatus.filter_owner_id = 0;
        apparatus.filter_medium_materials.clear();
        apparatus.cleanliness_permille = 1_000;
        apparatus.revision = apparatus.revision.saturating_add(1);
        apparatus.last_operator = request.actor;
        state.record(AlchemyAuditEvent {
            operation_id,
            installation_id,
            batch_id: batch_snapshot.as_ref().map_or(0, |batch| batch.id),
            actor: request.actor,
            action: "clean".into(),
            preparation_id: batch_snapshot.as_ref().map_or_else(
                || "base:apparatus_cleaning".into(),
                |batch| batch.preparation_id.clone(),
            ),
            volume_units: cleaning_water.water_hu.saturating_add(
                batch_snapshot
                    .as_ref()
                    .map_or(0, |batch| batch.residue_water.water_hu),
            ),
            current_units: 0,
            dross_units: burden,
            tick: now,
            note: "captured residue and spent media; wastewater entered runoff".into(),
        });
        state.validate().map_err(|error| error.to_string())?;
        if !captured_dross.is_empty() {
            let output_id = spent_filter_output
                .as_ref()
                .map(|output| output.arcane_id)
                .filter(|id| *id != 0)
                .ok_or("The captured filter burden has no physical spent-filter identity.")?;
            let staged_material = if consumed_materials.is_empty() {
                None
            } else {
                self.material_ledger
                    .as_ref()
                    .ok_or("The finite material ledger is unavailable.")?
                    .stage_linked_consumption(&consumed_materials)
                    .map_err(|error| error.to_string())?
            };
            self.commit_alchemy_current_with_material(
                state.clone(),
                operation_id,
                "base:spent_filter",
                "cleaning transferred captured dross into one physical spent filter",
                vec![(
                    ArcaneOwner::AlchemyDross(filter_owner_id),
                    captured_dross.clone(),
                )],
                vec![(
                    ArcaneOwner::ItemDross(output_id),
                    captured_dross,
                    Some("base:spent_filter".into()),
                )],
                staged_material,
            )?;
        } else if !consumed_materials.is_empty() {
            self.material_ledger
                .as_mut()
                .ok_or("The finite material ledger is unavailable.")?
                .record_consumption(&consumed_materials)
                .map_err(|error| error.to_string())?;
        }
        let moved = self.weather_state.live_mut()
            .and_then(|weather| {
                weather.portable_exchange_to_runoff(
                    atlas_pos,
                    WaterClass::Fresh,
                    HYDRO_UNITS_PER_BLOCK,
                    residue_water,
                )
            })
            .ok_or("Preflighted cleaning-water exchange unexpectedly failed.")?;
        debug_assert_eq!(moved, cleaning_water);
        result_for(
            &self.reg,
            state,
            pos,
            AlchemyCueKind::Clean,
            "The apparatus is clean; residue, spent media, glass, and wastewater all remain physical.",
            if residue_count != 0 {
                Some(produced(
                    &self.reg,
                    &residue_name,
                    u32::from(residue_count),
                    0,
                )?)
            } else {
                spent_filter_output
            },
        )
    }
}
