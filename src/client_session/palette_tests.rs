//! Wire identities and instance payloads survive content-order changes.

use std::path::Path;
use std::sync::Arc;

use glam::Vec3;

use super::ContentMap;
use crate::inventory::ItemStack;
use crate::net::{LooseItemSnap, StackSnap};
use crate::planet::{EntityPos, Face};
use crate::registry::{self, BlockId, ItemId, Registry};

fn registry() -> Arc<Registry> {
    Arc::new(registry::load(Path::new("/nonexistent-mods-dir")))
}

fn stack(item: u16) -> StackSnap {
    StackSnap {
        item,
        count: 73,
        durability: 41,
        arcane_id: 987,
        current_units: 12,
    }
}

#[test]
fn wire_order_and_unknown_entries_do_not_become_local_numeric_ids() {
    let reg = registry();
    let blocks: Vec<_> = reg.blocks.iter().take(3).map(|b| b.name.clone()).collect();
    let items: Vec<_> = reg.items.iter().take(3).map(|i| i.name.clone()).collect();
    for order in [[2, 0, 1], [1, 2, 0], [0, 2, 1]] {
        let mut host_blocks: Vec<_> = order.iter().map(|&i| blocks[i].clone()).collect();
        let mut host_items: Vec<_> = order.iter().map(|&i| items[i].clone()).collect();
        host_blocks.push("missing:block".into());
        host_items.push("missing:item".into());
        let map = ContentMap::new(Arc::clone(&reg), host_blocks, host_items);
        for (wire, local) in order.into_iter().enumerate() {
            assert_eq!(map.block(wire as u16), BlockId(local as u16));
            assert_eq!(map.item(wire as u16), Some(ItemId(local as u16)));
        }
        assert_eq!(map.block(3), reg.unknown_block);
        assert_eq!(map.block(u16::MAX), reg.unknown_block);
        assert_eq!(map.item(3), None);
        assert_eq!(map.item(u16::MAX), None);
    }
}

#[test]
fn bounded_slots_preserve_stack_fields_and_clear_absent_or_unknown_items() {
    let reg = registry();
    let name = reg.items[1].name.clone();
    let map = ContentMap::new(reg, Vec::new(), vec![name, "missing:item".into()]);
    let expected = Some(ItemStack {
        item: ItemId(1),
        count: 73,
        durability: 41,
        arcane_id: 987,
    });
    let wires = vec![Some(stack(0)), None, Some(stack(1)), Some(stack(u16::MAX))];
    assert_eq!(map.stack(&stack(0)), expected);
    assert_eq!(
        map.slots::<6>(&wires),
        [expected, None, None, None, None, None]
    );
    assert_eq!(map.slots::<1>(&wires), [expected]);
    assert_eq!(map.slots::<0>(&wires), []);
    assert_eq!(map.slots::<3>(&[]), [None, None, None]);
}

#[test]
fn loose_items_keep_host_identity_and_clamp_only_to_local_tool_durability() {
    let reg = registry();
    let tool = reg
        .items
        .iter()
        .position(|item| item.durability > 0)
        .unwrap();
    let name = reg.items[tool].name.clone();
    let maximum = reg.items[tool].durability;
    let map = ContentMap::new(reg, Vec::new(), vec![name]);
    let mut wire = LooseItemSnap {
        id: 123,
        pos: EntityPos::new(Face::PosZ, 100.0, 80.0, 100.0).unwrap(),
        vel: Vec3::new(1.0, 2.0, 3.0),
        item: 0,
        count: 7,
        age: 4.25,
        durability: u32::MAX,
        arcane_id: 567,
    };
    let entity = map.loose_item(&wire).unwrap();
    assert_eq!(entity.item, ItemId(tool as u16));
    assert_eq!(entity.stable_id, wire.id);
    assert_eq!(entity.pos, wire.pos);
    assert_eq!(entity.vel, wire.vel);
    assert_eq!(entity.count, wire.count);
    assert_eq!(entity.age, wire.age);
    assert_eq!(entity.durability, maximum);
    assert_eq!(entity.arcane_id, wire.arcane_id);
    wire.durability = 1;
    assert_eq!(map.loose_item(&wire).unwrap().durability, 1);
    wire.item = u16::MAX;
    assert!(map.loose_item(&wire).is_none());
}

#[test]
fn rebinding_resolves_original_host_names_after_reordering_and_reinstallation() {
    let old = registry();
    let block = old.blocks[1].name.clone();
    let item = old.items[1].name.clone();
    let mut map = ContentMap::new(Arc::clone(&old), vec![block.clone()], vec![item.clone()]);
    let mut changed = (*old).clone();
    changed.blocks.swap(1, 2);
    changed.items.swap(1, 2);
    for (i, definition) in changed.blocks.iter().enumerate() {
        changed
            .block_by_name
            .insert(definition.name.clone(), BlockId(i as u16));
    }
    for (i, definition) in changed.items.iter().enumerate() {
        changed
            .item_by_name
            .insert(definition.name.clone(), ItemId(i as u16));
    }
    assert_eq!(map.block(0), BlockId(1));
    assert_eq!(map.item(0), Some(ItemId(1)));
    map.rebind(Arc::new(changed.clone()));
    assert_eq!(map.block(0), BlockId(2));
    assert_eq!(map.item(0), Some(ItemId(2)));
    changed.block_by_name.remove(&block);
    changed.item_by_name.remove(&item);
    changed.unknown_block = BlockId(3);
    map.rebind(Arc::new(changed));
    assert_eq!(map.block(0), BlockId(3));
    assert_eq!(map.block(u16::MAX), BlockId(3));
    assert_eq!(map.item(0), None);
    map.rebind(old);
    assert_eq!(map.block(0), BlockId(1));
    assert_eq!(map.item(0), Some(ItemId(1)));
}

#[test]
fn reconnecting_with_an_empty_map_cannot_reuse_previous_host_ids() {
    let reg = registry();
    let first = ContentMap::new(
        Arc::clone(&reg),
        vec![reg.blocks[1].name.clone()],
        vec![reg.items[1].name.clone()],
    );
    let next = ContentMap::empty(Arc::clone(&reg));
    assert_eq!(first.item(0), Some(ItemId(1)));
    assert_eq!(next.item(0), None);
    assert_eq!(next.block(0), reg.unknown_block);
    assert!(next.blocks().is_empty());
    assert!(next.items().is_empty());
}
