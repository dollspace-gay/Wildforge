//! Rooting bed workings transaction coordination.

use crate::arcane::ArcaneOwner;
use std::collections::BTreeMap;
use crate::world::BlockPos;
use crate::arcane::Current;
use crate::workings::PhysicalDebit;
use crate::workings::PhysicalDebitKind;
use crate::workings::WorkingEffect;
use crate::workings::WorkingResult;
use crate::workings::WorkingTargetSnapshot;
use crate::world::World;
use crate::world::soil;

impl World {
    /// Plan a small prepared bed against staged shared water and soil budgets.
    /// Planning temporarily mirrors the exact sequential debits in memory and
    /// restores the world before reservation; the durable advances then replay
    /// in the same sorted order even after chunk unload or a crash.
    #[cfg(test)]
    pub fn begin_rooting_bed_ritual(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        controller: BlockPos,
    ) -> Result<WorkingResult, String> {
        self.begin_rooting_bed_ritual_definition(actor, actor_label, "base:rooting_bed", controller)
    }

    pub fn begin_rooting_bed_ritual_definition(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        working_id: &str,
        controller: BlockPos,
    ) -> Result<WorkingResult, String> {
        let layout = self.binding_frame_layout(controller);
        if !layout.valid {
            return Err(format!(
                "The rooting bed controller is incomplete: {}",
                layout.problems.join(" ")
            ));
        }
        let focus_posts = (-2..=2)
            .flat_map(|du| (-2..=2).map(move |dv| (du, dv)))
            .filter(|(du, dv)| *du != 0 || *dv != 0)
            .filter_map(|(du, dv)| controller.offset(du, 0, dv))
            .filter(|pos| {
                self.reg
                    .block(self.get_block_at(*pos))
                    .interaction
                    .as_deref()
                    == Some("containment")
            })
            .count();
        if focus_posts < 2 {
            return Err(
                "A rooting bed needs two visible focus posts around its prepared soil.".into(),
            );
        }
        let source = self
            .ritual_vessels(controller, 2)
            .into_iter()
            .max_by_key(|(_, stack, _, _)| {
                self.arcane_ledger
                    .as_ref()
                    .and_then(|ledger| ledger.account(&ArcaneOwner::Item(stack.arcane_id)))
                    .map_or(0, |account| account.current.total())
            })
            .ok_or("The rooting bed needs a mounted charge vessel.")?;
        let mut candidates = Vec::new();
        for du in -2..=2 {
            for dv in -2..=2 {
                let Some(pos) = controller.offset(du, 0, dv) else {
                    continue;
                };
                let definition = self.reg.block(self.get_block_at(pos));
                if definition.crop_next.is_some() || definition.sapling.is_some() {
                    candidates.push(pos);
                }
            }
        }
        candidates.sort();
        candidates.truncate(16);
        if candidates.is_empty() {
            return Err("The prepared bed contains no declared crop or sapling.".into());
        }

        let mut advances = Vec::new();
        let mut initial_water = BTreeMap::new();
        let mut initial_soil = BTreeMap::new();
        let mut initial_blocks = BTreeMap::new();
        for plant in candidates {
            let Ok((advance, _)) = self.prepare_rootwake(plant) else {
                continue;
            };
            initial_blocks
                .entry(plant)
                .or_insert_with(|| self.block_snapshot(plant));
            if let Some(soil) = advance.soil_pos {
                initial_blocks
                    .entry(soil)
                    .or_insert_with(|| self.block_snapshot(soil));
                initial_soil.entry(soil).or_insert(advance.before_soil_meta);
                let block = self.get_block_at(soil);
                self.set_block_meta_at(soil, block, advance.after_soil_meta);
            }
            if let Some(water) = advance.water_source {
                initial_water
                    .entry(water)
                    .or_insert_with(|| crate::planet_atlas::ReservoirMass {
                        water_hu: advance.water_before_hu,
                        salt_mass: advance.salt_before,
                    });
                self.write_water_mass_at(
                    water,
                    crate::planet_atlas::ReservoirMass {
                        water_hu: advance.water_after_hu,
                        salt_mass: advance.salt_after,
                    },
                );
            }
            advances.push(advance);
        }
        for (soil, meta) in &initial_soil {
            let block = self.get_block_at(*soil);
            self.set_block_meta_at(*soil, block, *meta);
        }
        for (water, mass) in &initial_water {
            self.write_water_mass_at(*water, *mass);
        }
        if advances.is_empty() {
            return Err(
                "No plant in the bed presently passes habitat, space, water, and nutrient checks."
                    .into(),
            );
        }
        let mut targets = vec![WorkingTargetSnapshot::Area {
            controller,
            revision: self.binding_frame_revision(controller)?,
            cells: advances.iter().map(|advance| advance.pos).collect(),
        }];
        targets.extend(initial_blocks.into_values());
        targets.extend(
            initial_water
                .iter()
                .map(|(pos, mass)| self.reservoir_snapshot(*pos, *mass)),
        );
        let mut physical = Vec::new();
        for (water, before) in &initial_water {
            let after = advances
                .iter()
                .filter(|advance| advance.water_source == Some(*water))
                .fold(*before, |_, advance| crate::planet_atlas::ReservoirMass {
                    water_hu: advance.water_after_hu,
                    salt_mass: advance.salt_after,
                });
            physical.push(PhysicalDebit {
                kind: PhysicalDebitKind::SoilWater,
                source: format!("bed_water:{water:?}"),
                content_id: "water".into(),
                units: before.water_hu.saturating_sub(after.water_hu),
                expected_version: 0,
            });
        }
        for (soil, before) in &initial_soil {
            let after = advances
                .iter()
                .rev()
                .find(|advance| advance.soil_pos == Some(*soil))
                .map_or(*before, |advance| advance.after_soil_meta);
            physical.push(PhysicalDebit {
                kind: PhysicalDebitKind::SoilNutrients,
                source: format!("bed_soil:{soil:?}"),
                content_id: "soil_nutrients".into(),
                units: u64::from(soil::fert_of(*before).saturating_sub(soil::fert_of(after))),
                expected_version: 0,
            });
        }
        physical.retain(|debit| debit.units != 0);
        physical.push(PhysicalDebit {
            kind: PhysicalDebitKind::Durability,
            source: format!("focus_posts:{controller:?}"),
            content_id: "focus_posts".into(),
            units: focus_posts as u64,
            expected_version: 0,
        });
        self.reserve_ritual_effect(
            actor,
            actor_label,
            controller,
            source.1.arcane_id,
            self.binding_frame_revision(controller)?,
            source.3,
            working_id,
            targets,
            physical,
            WorkingEffect::AdvanceBed {
                controller,
                plants: advances.clone(),
                scheduled_tick: self.working_tick().saturating_add(20 * 12),
            },
            advances.len() as u32,
            2,
            20 * 12,
            Current::default(),
        )
    }
}
