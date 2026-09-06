//! Entities machine_tick transaction coordination.

use crate::inventory::ItemStack;
use crate::planet::BlockPos;
use crate::world::BlockEntity;
use crate::world::FurnaceState;
use crate::world::World;

impl World {
    /// A running generator within reach: the field that lights lamps
    /// and turns the electric quern. Generators are shaft-driven
    /// machines; their markers make them findable.
    pub fn generator_near_at(&self, pos: BlockPos, r: i32) -> bool {
        let gens = [
            self.reg.block_id("base:generator"),
            self.reg.block_id("base:generator_run"),
        ];
        self.installations.iter().any(|(gpos, e)| {
            matches!(e, BlockEntity::Anvil(_))
                && gpos.entity_center().distance_to(pos.entity_center()) <= r as f32
                && gens.contains(&Some(self.get_block_at(*gpos)))
                && self.power_at_pos(*gpos) > 0.0
        })
    }

    pub fn tick_entities(&mut self, dt: f32) {
        // Capability E12: running machines feed regional ire.
        self.tick_industrial_ire(dt);
        self.tick_loose_items(dt);
        self.tick_steam(dt);
        self.tick_separators(dt);
        self.tick_bloomeries(dt);
        self.tick_kilns(dt);
        self.tick_forges(dt);
        self.tick_clamps(dt);
        self.tick_smokers(dt);
        self.tick_stations(dt);
        self.tick_perish(dt);
        let reg = self.reg.clone();
        // Byproducts pour out the furnace mouth (cupellation lead);
        // collected here because the entity map is borrowed.
        let mut spat: Vec<(BlockPos, ItemStack)> = Vec::new();
        let mut material_fuels = Vec::<(BlockPos, ItemStack)>::new();
        let mut arcane_inputs = Vec::<(BlockPos, ItemStack)>::new();
        let mut material_losses = Vec::<crate::registry::MaterialVector>::new();
        let mut secondary_recoveries = Vec::<crate::registry::MaterialVector>::new();
        for (&fpos, e) in self.installations.iter_mut() {
            let BlockEntity::Furnace(f) = e else { continue };
            let smelt = f.input.and_then(|s| reg.smelt_for(s.item)).cloned();
            let output_ok = |f: &FurnaceState, out: crate::registry::ItemId| match f.output {
                None => true,
                Some(o) => o.item == out && o.count < reg.item(out).max_stack,
            };
            let can_smelt = smelt.as_ref().is_some_and(|s| output_ok(f, s.output));

            if f.burn_left <= 0.0 && can_smelt {
                // Light more fuel (the forge feeds the wild's ire).
                if let Some(fs) = f.fuel
                    && let Some((burn, speed)) = reg.fuel_value(fs.item)
                {
                    f.burn_left = burn;
                    f.burn_total = burn;
                    f.burn_speed = speed;
                    material_fuels.push((fpos, ItemStack { count: 1, ..fs }));
                    let left = fs.count - 1;
                    f.fuel = if left > 0 {
                        Some(ItemStack { count: left, ..fs })
                    } else {
                        None
                    };
                    self.ire = (self.ire + 0.1).min(100.0);
                }
            }
            if f.burn_left > 0.0 {
                f.burn_left = (f.burn_left - dt).max(0.0);
                if can_smelt {
                    let s = smelt.as_ref().unwrap();
                    f.progress += dt * f.burn_speed.max(1.0);
                    if f.progress >= s.time {
                        f.progress = 0.0;
                        // Consume one input, emit output.
                        if let Some(inp) = f.input {
                            if inp.arcane_id != 0 {
                                arcane_inputs.push((fpos, ItemStack { count: 1, ..inp }));
                            }
                            if crate::materials::is_secondary_item(&reg, inp.item) {
                                secondary_recoveries.push(crate::materials::stack_materials(
                                    &reg,
                                    ItemStack::new(&reg, inp.item, 1),
                                ));
                            }
                            let left = inp.count - 1;
                            f.input = if left > 0 {
                                Some(ItemStack { count: left, ..inp })
                            } else {
                                None
                            };
                        }
                        f.output = Some(match f.output {
                            Some(o) => ItemStack {
                                count: o.count + 1,
                                ..o
                            },
                            None => ItemStack::new(&reg, s.output, 1),
                        });
                        if let Some((item, count)) = s.spit {
                            spat.push((fpos, ItemStack::new(&reg, item, count)));
                        }
                        material_losses.push(s.loss.clone());
                    }
                } else {
                    f.progress = 0.0;
                }
            } else if f.progress > 0.0 {
                f.progress = (f.progress - dt * 2.0).max(0.0);
            }
        }
        if let Some(ledger) = &mut self.material_ledger {
            for (_, stack) in &material_fuels {
                let materials = crate::materials::stack_materials(&reg, *stack);
                if let Err(error) = ledger.record_consumption(&materials) {
                    eprintln!("materials: furnace fuel accounting failed: {error}");
                }
            }
            for loss in material_losses {
                if let Err(error) = ledger.record_recipe_loss(&loss) {
                    eprintln!("materials: furnace process accounting failed: {error}");
                }
            }
            for materials in secondary_recoveries {
                if let Err(error) = ledger.record_secondary_recovery(&materials) {
                    eprintln!("materials: furnace secondary recovery failed: {error}");
                }
            }
        }
        for (pos, stack) in material_fuels {
            self.retire_arcane_stack_at(pos, stack, "furnace fuel consumed");
        }
        for (pos, stack) in arcane_inputs {
            self.retire_arcane_stack_at(pos, stack, "furnace input transformed");
        }
        for (pos, stack) in spat {
            self.push_drop_at(pos, stack);
        }
        self.tick_rail_motion(dt);
        self.tick_belts(dt);
        self.tick_local_structure_machines(dt);
    }
}
