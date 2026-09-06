//! Physical implement identity, resolution, and lifecycle qualification.

use super::*;
use crate::world::{ReplicaWorld, ReplicationTarget};

fn summed_materials<'a>(
    reg: &crate::registry::Registry,
    names: impl IntoIterator<Item = &'a str>,
) -> crate::registry::MaterialVector {
    let mut total = crate::registry::MaterialVector::new();
    for name in names {
        let stack = ItemStack::new(reg, it(reg, name), 1);
        for (material, units) in crate::materials::stack_materials(reg, stack) {
            *total.entry(material).or_default() += units;
        }
    }
    total
}

fn write_implement_mod(root: &Path, id: &str, items: &str) {
    let dir = root.join(id);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("mod.toml"),
        format!("id = \"{id}\"\nworld_api = 2\ndepends = [\"base\"]\n"),
    )
    .unwrap();
    std::fs::write(dir.join("items.toml"), items).unwrap();
}

pub(super) fn embodied_implements_world(tag: &str) -> World {
    let reg = base_reg();
    let dir = tmp_dir(tag);
    let atlas = std::sync::Arc::new(crate::planet_atlas::PlanetAtlas::fixture(8_805, 16).unwrap());
    atlas.write_new(&dir).unwrap();
    World::new_with_atlas(8_805, dir, reg, atlas)
}

pub(super) fn install_frame_fixture(
    world: &mut World,
) -> (crate::planet::BlockPos, crate::planet::BlockPos) {
    let frame = bp(8, 100, 8);
    let vessel = bp(7, 100, 8);
    let center = frame.chunk();
    let missing = (-1..=1)
        .flat_map(|du| (-1..=1).map(move |dv| center.offset(du, dv)))
        .filter(|chunk| !world.has_chunk(*chunk))
        .collect::<Vec<_>>();
    world.insert_empty_chunks_for_test(missing);
    for (pos, block) in [
        (frame, b(&world.reg, "base:binding_frame")),
        (bp(9, 100, 8), b(&world.reg, "base:focus_mount")),
        (bp(8, 100, 9), b(&world.reg, "base:arcane_conductor")),
        (bp(8, 100, 7), b(&world.reg, "base:containment_post")),
    ] {
        world.set_block_authored_at(pos, block, "implements integration fixture");
    }
    let vessel_stack = ItemStack::new(&world.reg, it(&world.reg, "base:charge_vessel"), 1);
    assert!(world.place_item_block_at(vessel, vessel_stack));
    let layout = world.binding_frame_layout(frame);
    assert!(layout.valid, "{:?}", layout.problems);
    (frame, vessel)
}

pub(super) fn assemble_fixture_wand(
    world: &mut World,
    frame: crate::planet::BlockPos,
) -> (ItemStack, u64) {
    use crate::implements::FrameAction;

    let mut inventory = Inventory::new();
    let calibrated = world
        .operate_binding_frame(
            frame,
            &mut inventory,
            0,
            FrameAction::Calibrate,
            None,
            "test",
        )
        .unwrap();
    let mut revision = calibrated.revision;
    for component in [
        "base:seasoned_wand_body",
        "base:ritual_rod_socket",
        "base:echo_slate",
        "base:bronze_wand_binding",
    ] {
        inventory.slots[0] = Some(ItemStack::new(&world.reg, it(&world.reg, component), 1));
        revision = world
            .operate_binding_frame(
                frame,
                &mut inventory,
                0,
                FrameAction::ExchangeSelected,
                Some(revision),
                "test",
            )
            .unwrap()
            .revision;
    }
    let assembled = world
        .operate_binding_frame(
            frame,
            &mut inventory,
            0,
            FrameAction::Assemble,
            Some(revision),
            "test",
        )
        .unwrap();
    let stack = match world.block_entity_at(&frame).unwrap() {
        crate::world::BlockEntity::BindingFrame(state) => state.output.unwrap(),
        _ => panic!("wrong frame entity"),
    };
    (stack, assembled.revision)
}

pub(super) fn drain_fixture_item_to_spark(
    world: &mut World,
    pos: crate::planet::BlockPos,
    id: u64,
) {
    use crate::arcane::{ArcaneAuthority, ArcaneOwner, ArcaneTransaction};

    let owner = ArcaneOwner::Item(id);
    let (version, mut current) = {
        let account = world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&owner)
            .unwrap();
        (account.version, account.current.clone())
    };
    let usable = crate::implements::usable_charge(current.total());
    if usable == 0 {
        return;
    }
    let moved = current.take_units(usable, std::iter::empty()).unwrap();
    let region = world.planet_atlas().unwrap().atlas_pos(pos.surface());
    let destination = ArcaneOwner::Ambient(region);
    let ledger = world.arcane_ledger.as_mut().unwrap();
    let transaction = ArcaneTransaction::transfer(
        ledger.system_transaction_id().unwrap(),
        owner,
        version,
        destination.clone(),
        ledger.version_of(&destination),
        moved,
        ArcaneAuthority::System,
        "implements test fixture drained to structural spark",
    );
    ledger.commit(transaction).unwrap();
}

mod charms;
mod conductors;
mod disposition;
mod embodied_frame_calibrates_assembles_saves_disassembles_and_dismantles_conservatively;
mod frame;
mod identity;
mod persistence;
mod presentation;
mod resolver;
mod vessels;
