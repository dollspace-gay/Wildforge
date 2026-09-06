//! Inventory scenarios.

use super::*;

#[test]
fn the_agent_crafts_places_and_deposits() {
    let host = TestHost::start("agent-craft");
    let mut a = Agent::connect_for_test(host.addr, "JOINER").expect("joins");
    a.pump_for(0.5);
    // The host gives raw logs the way any drop arrives.
    let gid = host.with(|sess, _| *sess.guests.keys().next().unwrap());
    host.with(|_, sim| {
        let log = sim.world.reg.item_id("base:log").unwrap();
        let stack = crate::inventory::ItemStack::new(&sim.world.reg, log, 4);
        sim.world.queue_give(gid, stack);
    });
    for _ in 0..100 {
        a.pump(0.02);
        if a.inventory.slots.iter().flatten().any(|s| s.count == 4) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    a.craft("planks", 3).expect("logs become planks");
    let planks: u32 = a
        .inventory
        .slots
        .iter()
        .flatten()
        .filter(|s| a.reg.item(s.item).name.contains("planks"))
        .map(|s| s.count)
        .sum();
    assert!(planks >= 12, "3 crafts x 4 planks ({planks})");
    let player_cell = crate::agent::cell_of(a.player.pos).expect("agent occupies a world cell");
    a.craft("crafting_table", 1).expect("table crafts");
    // Two cells out: the host (rightly) refuses placement into any
    // cell the placer's own body overlaps, and physics can settle an
    // agent right on a cell boundary.
    a.place_at(
        player_cell.offset(2, 0, 0).expect("table cell"),
        "base:crafting_table",
    )
    .expect("table places");
    a.craft("chest", 1).expect("a chest by the table");
    let chest = player_cell.offset(-2, 0, 0).expect("chest cell");
    a.place_at(chest, "base:chest").expect("chest places");
    let report = a.deposit(chest, None).expect("the pack empties into it");
    assert!(report.contains("stowed"), "{report}");
    // The host's chest — the authoritative one — holds the goods.
    let held: u32 = host.with(|_, sim| match sim.world.block_entity_at(&chest) {
        Some(crate::world::BlockEntity::Chest(c)) => {
            c.slots.iter().flatten().map(|s| s.count).sum()
        }
        _ => 0,
    });
    assert!(held > 0, "the authoritative chest holds the deposit");
}
