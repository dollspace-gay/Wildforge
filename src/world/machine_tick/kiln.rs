//! Shared kiln ticking over the physical BlockStore contract.

use crate::world::BlockEntity;
use crate::world::multiblock::BlockStore;
use crate::inventory::ItemStack;
use crate::machines::MachineHandler;

pub(in crate::world) fn tick_kiln_machines<B: BlockStore>(store: &mut B, dt: f32) {
    let keys: Vec<B::Pos> = store
        .block_entities()
        .iter()
        .filter(|(_, e)| {
            matches!(e, BlockEntity::Multiblock(m)
                    if m.kind.handler(store.reg()) == Some(MachineHandler::Kiln) && m.lit)
        })
        .map(|(k, _)| *k)
        .collect();
    for pos in keys {
        let Some(BlockEntity::Multiblock(mut k)) = store.block_entities_mut().remove(&pos) else {
            continue;
        };
        // A chimneyed kiln is a glassworks: rain can't reach the
        // fire, and the draft doubles what each fuel fires.
        let glassworks = k.stats.chimney;
        let unroofed = k.core.is_some_and(|core| store.open_sky_above(core));
        let local_weather = store
            .to_world(pos)
            .map(|w| store.weather_at(w))
            .unwrap_or_default();
        let wet = !glassworks
            && local_weather.precipitation == crate::planet_atlas::PrecipitationForm::Rain
            && unroofed;
        if wet && local_weather.kind == crate::planet_atlas::LocalWeather::Storm {
            k.lit = false;
            k.progress = 0.0;
            store.swap_block_keep_entity(pos, "base:kiln");
            store
                .block_entities_mut()
                .insert(pos, BlockEntity::Multiblock(k));
            continue;
        }
        let heat = k.stats.heat_multiplier();
        k.progress += dt * if wet { 0.5 } else { 1.0 } * heat;
        let fire_secs = store
            .reg()
            .machine(k.kind)
            .map(|def| def.fire_secs)
            .unwrap_or(crate::world::KILN_FIRE_SECS);
        if k.progress >= fire_secs {
            if let Some((_, fuel_item, clear)) = store.reg().kiln_base {
                let n_sand: u32 = k.charge.iter().flatten().map(|s| s.count).sum();
                let n_fuel: u32 = k.fuel.iter().flatten().map(|s| s.count).sum();
                let fuel_reach = if glassworks { n_fuel * 2 } else { n_fuel };
                let pairs = n_sand.min(fuel_reach) / 2;
                let out_n = pairs * 2;
                // One powder colors the whole batch.
                let colored = k.reagent.as_ref().and_then(|p| {
                    store
                        .reg()
                        .kiln
                        .iter()
                        .find(|recipe| recipe.powder == p.item)
                        .map(|recipe| recipe.glass)
                });
                let out_item = colored.unwrap_or(clear);
                let powder_materials = colored
                    .and(k.reagent)
                    .map(|stack| {
                        crate::materials::stack_materials(
                            store.reg(),
                            ItemStack { count: 1, ..stack },
                        )
                    })
                    .unwrap_or_default();
                if colored.is_some()
                    && let Some(p) = &mut k.reagent
                {
                    p.count -= 1;
                    if p.count == 0 {
                        k.reagent = None;
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
                eat(&mut k.charge, pairs * 2);
                let fuel_used = if glassworks {
                    (pairs * 2).div_ceil(2)
                } else {
                    pairs * 2
                };
                let mut fuel_materials = crate::registry::MaterialVector::new();
                let mut remaining = fuel_used;
                for stack in k.fuel.iter().flatten() {
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
                eat(&mut k.fuel, fuel_used);
                if let Some(ledger) = store.material_ledger() {
                    if let Err(error) = ledger.record_consumption(&powder_materials) {
                        eprintln!("materials: kiln pigment accounting failed: {error}");
                    }
                    if let Err(error) = ledger.record_consumption(&fuel_materials) {
                        eprintln!("materials: kiln fuel accounting failed: {error}");
                    }
                }
                let _ = fuel_item;
                if out_n > 0 {
                    let reg = store.reg().clone();
                    let mut out = ItemStack::new(&reg, out_item, 1);
                    out.count = out_n;
                    for s in k.charge.iter_mut() {
                        if s.is_none() {
                            *s = Some(out);
                            break;
                        }
                    }
                }
            }
            k.lit = false;
            k.progress = 0.0;
            store.swap_block_keep_entity(pos, "base:kiln");
        }
        store
            .block_entities_mut()
            .insert(pos, BlockEntity::Multiblock(k));
    }
}
