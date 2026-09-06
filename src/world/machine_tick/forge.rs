//! Shared forge ticking over the physical BlockStore contract.

use crate::inventory::ItemStack;
use crate::machines::MachineHandler;
use crate::world::BlockEntity;
use crate::world::FORGE_ITEMS_PER_FUEL;
use crate::world::multiblock::BlockStore;

pub(in crate::world) fn tick_forge_machines<B: BlockStore>(store: &mut B, dt: f32) {
    let keys: Vec<B::Pos> = store
        .block_entities()
        .iter()
        .filter(|(_, e)| {
            matches!(e, BlockEntity::Multiblock(m)
                    if m.kind.handler(store.reg()) == Some(MachineHandler::Forge) && m.lit)
        })
        .map(|(k, _)| *k)
        .collect();
    for pos in keys {
        let Some(BlockEntity::Multiblock(mut f)) = store.block_entities_mut().remove(&pos) else {
            continue;
        };
        f.progress += dt * f.stats.heat_multiplier();
        let fire_secs = store
            .reg()
            .machine(f.kind)
            .map(|def| def.fire_secs)
            .unwrap_or(crate::world::FORGE_FIRE_SECS);
        if f.progress >= fire_secs {
            let reg = store.reg().clone();
            let items_per_fuel = reg
                .machine(f.kind)
                .map(|def| def.items_per_fuel)
                .unwrap_or(crate::world::FORGE_ITEMS_PER_FUEL);
            let n_fuel: u32 = f.fuel.iter().flatten().map(|s| s.count).sum();
            let mut budget = n_fuel * items_per_fuel;
            let mut burned = 0u32;
            let mut outputs: Vec<ItemStack> = Vec::new();
            for s in f.charge.iter_mut() {
                let Some(st) = s else { continue };
                let input_item = st.item;
                let salvage = reg
                    .forge_salvage
                    .iter()
                    .find(|salvage| salvage.input == st.item)
                    .cloned();
                let smelt = reg
                    .smelts
                    .iter()
                    .find(|sm| sm.input.matches(st.item))
                    .cloned();
                let reclaim = crate::materials::is_reclaimable_stock(&reg, st.item);
                if salvage.is_none() && smelt.is_none() && !reclaim {
                    continue; // not smeltable/salvageable: survives
                }
                let n = st.count.min(budget);
                if n == 0 {
                    continue;
                }
                budget -= n;
                burned += n;
                st.count -= n;
                if st.count == 0 {
                    *s = None;
                }
                if reclaim {
                    let materials = crate::materials::stack_materials(
                        &reg,
                        ItemStack::new(&reg, input_item, n),
                    );
                    for (material, units) in materials {
                        *f.reclaim.entry(material).or_default() += units;
                    }
                    continue;
                }
                let output = salvage
                    .as_ref()
                    .map_or_else(|| smelt.as_ref().unwrap().output, |recipe| recipe.output);
                let mut out = ItemStack::new(&reg, output, 1);
                out.count = n;
                outputs.push(out);
                if let Some(recipe) = salvage {
                    debug_assert!(matches!(recipe.recovery_permille, 900 | 950));
                    let mut scale = ItemStack::new(&reg, recipe.byproduct, 1);
                    scale.count = n;
                    outputs.push(scale);
                    if let Some(ledger) = store.material_ledger() {
                        let materials = crate::materials::stack_materials(
                            &reg,
                            ItemStack::new(&reg, recipe.byproduct, n),
                        );
                        if let Err(error) = ledger.record_secondary_output(&materials) {
                            eprintln!("materials: forge scale accounting failed: {error}");
                        }
                    }
                } else if let Some((spit, sn)) = smelt.as_ref().and_then(|smelt| smelt.spit) {
                    let mut sp = ItemStack::new(&reg, spit, 1);
                    sp.count = sn * n;
                    outputs.push(sp);
                }
                if let Some(smelt) = smelt
                    && let Some(ledger) = store.material_ledger()
                    && let Err(error) = ledger.record_recipe_loss_scaled(&smelt.loss, n)
                {
                    eprintln!("materials: forge loss accounting failed: {error}");
                }
                if crate::materials::is_secondary_item(&reg, input_item)
                    && let Some(ledger) = store.material_ledger()
                {
                    let materials = crate::materials::stack_materials(
                        &reg,
                        ItemStack::new(&reg, input_item, n),
                    );
                    if let Err(error) = ledger.record_secondary_recovery(&materials) {
                        eprintln!("materials: forge slag recovery accounting failed: {error}");
                    }
                }
            }
            outputs.extend(crate::materials::consolidate_reclaimed_stock(
                &reg,
                &mut f.reclaim,
            ));
            // Fuel burns only for work done (round up).
            let eat = |slots: &mut [Option<ItemStack>; 4], mut n: u32| {
                for s in slots.iter_mut() {
                    if n == 0 {
                        break;
                    }
                    if let Some(st) = s {
                        let take = st.count.min(n);
                        n -= take;
                        st.count -= take;
                        if st.count == 0 {
                            *s = None;
                        }
                    }
                }
            };
            let fuel_used = burned.div_ceil(FORGE_ITEMS_PER_FUEL);
            let mut fuel_materials = crate::registry::MaterialVector::new();
            let mut remaining_fuel = fuel_used;
            for stack in f.fuel.iter().flatten() {
                let take = stack.count.min(remaining_fuel);
                remaining_fuel -= take;
                let used = crate::materials::stack_materials(
                    &reg,
                    ItemStack {
                        count: take,
                        ..*stack
                    },
                );
                for (material, units) in used {
                    *fuel_materials.entry(material).or_default() += units;
                }
                if remaining_fuel == 0 {
                    break;
                }
            }
            eat(&mut f.fuel, fuel_used);
            if let Some(ledger) = store.material_ledger()
                && let Err(error) = ledger.record_consumption(&fuel_materials)
            {
                eprintln!("materials: forge fuel accounting failed: {error}");
            }
            let mouth = store.to_world(pos).and_then(|w| w.offset(0, 1, 0));
            for out in outputs {
                if let Some(above) = mouth {
                    store.push_drop_at(above, out);
                }
            }
            f.lit = false;
            f.progress = 0.0;
            store.swap_block_keep_entity(pos, "base:forge");
        }
        store
            .block_entities_mut()
            .insert(pos, BlockEntity::Multiblock(f));
    }
}
