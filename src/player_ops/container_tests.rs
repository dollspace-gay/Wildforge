//! Authority and replica storage obey the same cursor and refusal contract.

use super::container::{Click, Rejected, click};
use crate::inventory::ItemStack;
use crate::planet::BlockPos;
use crate::registry::{self, Registry};
use crate::world::{BlockEntity, ReplicaWorld, ReplicationTarget, World};
use std::sync::Arc;

fn state(entity: &BlockEntity) -> (Vec<Option<ItemStack>>, f32) {
    match entity {
        BlockEntity::Chest(value) => (value.slots.to_vec(), 0.0),
        BlockEntity::Furnace(value) => {
            (vec![value.input, value.fuel, value.output], value.progress)
        }
        BlockEntity::Stall(value) => (
            value
                .goods
                .into_iter()
                .chain([value.price])
                .chain(value.till)
                .collect(),
            0.0,
        ),
        _ => panic!("unsupported fixture"),
    }
}

fn reg() -> Arc<Registry> {
    Arc::new(registry::load(std::path::Path::new(
        "/nonexistent-mods-dir",
    )))
}

fn paired(
    reg: Arc<Registry>,
    entity: impl Fn() -> BlockEntity,
    initial: Option<ItemStack>,
    actions: &[Click],
) -> (Vec<Option<ItemStack>>, Option<ItemStack>) {
    let pos = BlockPos::of_world(8, 90, 8).unwrap();
    let mut authority = World::new(42, std::path::PathBuf::new(), reg.clone());
    let mut replica = ReplicaWorld::new(42, reg.clone(), 0.0);
    authority.insert_block_entity_at(pos, entity());
    replica.receive_block_entity(pos, entity());
    let (mut authoritative_cursor, mut predicted_cursor) = (initial, initial);
    for action in actions {
        let authoritative = click(
            &reg,
            authority.block_entity_mut_at(&pos).unwrap(),
            &mut authoritative_cursor,
            *action,
        );
        let predicted = replica.predict_container_click(pos, &mut predicted_cursor, *action);
        assert_eq!(authoritative, predicted);
        assert_eq!(authoritative_cursor, predicted_cursor);
        assert_eq!(
            state(authority.block_entity_at(&pos).unwrap()),
            state(replica.view().block_entity_at(&pos).unwrap())
        );
    }
    let (slots, _) = state(authority.block_entity_at(&pos).unwrap());
    (slots, authoritative_cursor)
}

fn request(slot: usize, right: bool) -> Click {
    Click {
        slot,
        right,
        actor: Some([7; 16]),
    }
}

#[test]
fn chest_split_recombine_and_invalid_slot_preserve_the_exact_inventory() {
    let reg = reg();
    let original = ItemStack::new(&reg, reg.item_id("base:stone").unwrap(), 7);
    let (slots, cursor) = paired(
        reg,
        || {
            let mut chest = crate::world::ChestState::default();
            chest.slots[0] = Some(original);
            BlockEntity::Chest(chest)
        },
        None,
        &[
            request(0, true),
            request(1, true),
            request(0, false),
            request(99, false),
        ],
    );
    assert_eq!(cursor, None);
    assert_eq!(
        slots[0],
        Some(ItemStack {
            count: 6,
            ..original
        })
    );
    assert_eq!(
        slots[1],
        Some(ItemStack {
            count: 1,
            ..original
        })
    );
}

#[test]
fn furnace_output_refuses_to_merge_different_physical_instances() {
    let reg = reg();
    let output = ItemStack {
        arcane_id: 71,
        durability: 13,
        ..ItemStack::new(&reg, reg.item_id("base:stone").unwrap(), 1)
    };
    let held = ItemStack {
        arcane_id: 72,
        durability: 17,
        ..output
    };
    let (slots, cursor) = paired(
        reg.clone(),
        || {
            BlockEntity::Furnace(crate::world::FurnaceState {
                output: Some(output),
                ..Default::default()
            })
        },
        Some(held),
        &[request(2, false)],
    );
    assert_eq!(slots[2], Some(output));
    assert_eq!(cursor, Some(held));
    let mut furnace = BlockEntity::Furnace(crate::world::FurnaceState {
        output: Some(output),
        ..Default::default()
    });
    assert_eq!(
        click(&reg, &mut furnace, &mut Some(held), request(2, false)),
        Err(Rejected::IncompatibleCursor)
    );
}

#[test]
fn stall_prediction_without_the_owner_cannot_edit_its_quote() {
    let reg = reg();
    let held = ItemStack::new(&reg, reg.item_id("base:stick").unwrap(), 3);
    let (slots, cursor) = paired(
        reg,
        || {
            BlockEntity::Stall(crate::world::StallState {
                owner: [7; 16],
                ..Default::default()
            })
        },
        Some(held),
        &[
            Click {
                actor: None,
                ..request(6, false)
            },
            Click {
                actor: Some([8; 16]),
                ..request(6, false)
            },
        ],
    );
    assert!(slots.iter().all(Option::is_none));
    assert_eq!(cursor, Some(held));
}
