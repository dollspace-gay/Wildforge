//! Containers, furnaces, production machines, and falling-block behavior.

use super::*;

fn clear_machine_outputs(world: &mut World) {
    world.clear_pending_drops();
    world.clear_loose_items();
}

fn take_machine_outputs(world: &mut World) -> Vec<ItemStack> {
    let mut outputs = world
        .take_pending_drops()
        .into_iter()
        .map(|(_, stack)| stack)
        .collect::<Vec<_>>();
    outputs.extend(world.take_loose_items().into_iter().map(|item| ItemStack {
        item: item.item,
        count: item.count,
        durability: item.durability,
        arcane_id: item.arcane_id,
    }));
    outputs
}

fn machine_output_count(world: &World, item: crate::registry::ItemId) -> u32 {
    world
        .pending_drops()
        .iter()
        .filter(|(_, stack)| stack.item == item)
        .map(|(_, stack)| stack.count)
        .chain(
            world
                .loose_items()
                .iter()
                .filter(|entity| entity.item == item)
                .map(|entity| entity.count),
        )
        .sum()
}

// ---- mechanization: millwork (rung 0-1) ----

/// Raise the standard test water site: a sealed stone basin holding
/// one full cell at (10,119,10), a wheel placed over it, and a breach
/// helper that opens a lip so the pool becomes live water.
fn wheel_over_basin(w: &mut World, reg: &Registry) -> (i32, i32, i32) {
    let stone = b(reg, "base:stone");
    let (wx, wy, wz) = (10, 120, 10);
    for dx in -1..=1 {
        for dz in -1..=1 {
            w.set_block(wx + dx, wy - 2, wz + dz, stone);
            if dx != 0 || dz != 0 {
                w.set_block(wx + dx, wy - 1, wz + dz, stone);
            }
        }
    }
    w.set_block(wx, wy - 1, wz, reg.water_block(0));
    assert!(w.place_block((wx, wy, wz), b(reg, "base:water_wheel")));
    (wx, wy, wz)
}

fn breach_basin(w: &mut World, wheel: (i32, i32, i32)) {
    let (wx, wy, wz) = wheel;
    w.set_block(wx + 1, wy - 1, wz, AIR);
    w.set_block(wx + 1, wy - 2, wz, AIR);
}

mod archaeology;
mod bloomery_multiblock_fires_batches_and_fears_the_rain;
mod containers;
mod falling;
mod firing;
mod food;
mod furnace;
mod modules;
mod power;
mod steam;
mod workshops;
