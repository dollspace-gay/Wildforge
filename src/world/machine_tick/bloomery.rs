//! Shared bloomery ticking over the physical BlockStore contract.

use crate::world::BlockEntity;
use crate::world::multiblock::BlockStore;
use crate::inventory::ItemStack;
use crate::machines::MachineHandler;

pub(in crate::world) fn tick_bloomery_machines<B: BlockStore>(store: &mut B, dt: f32) {
    let keys: Vec<B::Pos> = store
        .block_entities()
        .iter()
        .filter(|(_, e)| {
            matches!(e, BlockEntity::Multiblock(m)
                    if m.kind.handler(store.reg()) == Some(MachineHandler::Bloomery) && m.lit)
        })
        .map(|(k, _)| *k)
        .collect();
    for pos in keys {
        let Some(BlockEntity::Multiblock(mut b)) = store.block_entities_mut().remove(&pos) else {
            continue;
        };
        // An unroofed stack fights the rain and loses to a storm.
        let unroofed = b.core.is_some_and(|core| store.open_sky_above(core));
        let local_weather = store
            .to_world(pos)
            .map(|w| store.weather_at(w))
            .unwrap_or_default();
        let wet =
            local_weather.precipitation == crate::planet_atlas::PrecipitationForm::Rain && unroofed;
        if wet && local_weather.kind == crate::planet_atlas::LocalWeather::Storm {
            b.lit = false;
            b.progress = 0.0;
            store.swap_block_keep_entity(pos, "base:bloomery");
            store
                .block_entities_mut()
                .insert(pos, BlockEntity::Multiblock(b));
            continue;
        }
        let heat = b.stats.heat_multiplier();
        b.progress += dt * if wet { 0.5 } else { 1.0 } * heat;
        let fire_secs = store
            .reg()
            .machine(b.kind)
            .map(|def| def.fire_secs)
            .unwrap_or(crate::world::BLOOMERY_FIRE_SECS);
        if b.progress >= fire_secs {
            // Cash the batch: 2 charge + 2 fuel per bloom, +2 bonus
            // blooms on a full 8+8 firing.
            let chain = store.reg().bloomery.first().cloned();
            if let Some(chain) = chain {
                let n_charge: u32 = b.charge.iter().flatten().map(|s| s.count).sum();
                let n_fuel: u32 = b.fuel.iter().flatten().map(|s| s.count).sum();
                let units = n_charge.min(n_fuel) / 2;
                let blooms = units + if units >= 4 { 2 } else { 0 };
                let charge_used = units * 2;
                let fuel_used = units * 2;
                let input_materials = crate::materials::stack_materials(
                    store.reg(),
                    ItemStack::new(store.reg(), chain.charge, charge_used),
                );
                let output_materials = crate::materials::stack_materials(
                    store.reg(),
                    ItemStack::new(store.reg(), chain.bloom, blooms),
                );
                let mut process_loss = crate::registry::MaterialVector::new();
                for (material, input) in &input_materials {
                    let output = output_materials.get(material).copied().unwrap_or_default();
                    if *input > output {
                        process_loss.insert(material.clone(), input - output);
                    }
                }
                let mut fuel_materials = crate::registry::MaterialVector::new();
                let mut remaining = fuel_used;
                for stack in b.fuel.iter().flatten() {
                    let take = stack.count.min(remaining);
                    remaining -= take;
                    let materials = crate::materials::stack_materials(
                        store.reg(),
                        ItemStack {
                            count: take,
                            ..*stack
                        },
                    );
                    for (material, amount) in materials {
                        *fuel_materials.entry(material).or_default() += amount;
                    }
                    if remaining == 0 {
                        break;
                    }
                }
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
                eat(&mut b.charge, charge_used);
                eat(&mut b.fuel, fuel_used);
                if let Some(ledger) = store.material_ledger() {
                    if let Err(error) = ledger.record_consumption(&fuel_materials) {
                        eprintln!("materials: bloomery fuel accounting failed: {error}");
                    }
                    if let Err(error) = ledger.record_secondary_output(&process_loss) {
                        eprintln!("materials: bloomery slag accounting failed: {error}");
                    }
                }
                let reg = store.reg().clone();
                if blooms != 0 {
                    let out = ItemStack::new(&reg, chain.bloom, blooms);
                    // Blooms land in the first empty charge slot.
                    for s in b.charge.iter_mut() {
                        if s.is_none() {
                            *s = Some(out);
                            break;
                        }
                    }
                }
                if let Some(slag) = reg.item_id("base:iron_slag") {
                    let per_slag = reg.item(slag).materials.get("iron").copied().unwrap_or(1);
                    let slag_count =
                        process_loss.get("iron").copied().unwrap_or_default() / per_slag;
                    if slag_count != 0
                        && let Some(above) = store.to_world(pos).and_then(|w| w.offset(0, 1, 0))
                    {
                        store.push_drop_at(above, ItemStack::new(&reg, slag, slag_count as u32));
                    }
                }
            }
            b.lit = false;
            b.progress = 0.0;
            store.swap_block_keep_entity(pos, "base:bloomery");
        }
        store
            .block_entities_mut()
            .insert(pos, BlockEntity::Multiblock(b));
    }
}
