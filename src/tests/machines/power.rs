//! Power scenarios.

use super::*;

#[test]
fn the_wheel_wants_live_water_and_shafts_carry_it() {
    let reg = base_reg();
    let mut w = test_world_with("millwork-power", reg.clone());
    let wheel = wheel_over_basin(&mut w, &reg);
    let (wx, wy, wz) = wheel;
    assert_eq!(
        w.wheel_live(wx, wy, wz),
        0.0,
        "a standing pool turns nothing"
    );
    breach_basin(&mut w, wheel);
    assert!(
        w.wheel_live(wx, wy, wz) > 0.0,
        "a breached lip is live water"
    );
    // Twelve wooden shafts carry the turn; a thirteenth refuses.
    let shaft = b(&reg, "base:shaft");
    for i in 1..=12 {
        w.set_block(wx, wy, wz + i, shaft);
    }
    assert!(w.power_at(wx, wy, wz + 13) > 0.0, "a 12-shaft run works");
    w.set_block(wx, wy, wz + 13, shaft);
    assert_eq!(
        w.power_at(wx, wy, wz + 14),
        0.0,
        "thirteen wooden shafts is one too many"
    );
    // A gear turns the corner that a shaft refuses.
    let gear = b(&reg, "base:gear");
    for i in 1..=13 {
        w.set_block(wx, wy, wz + i, AIR);
    }
    w.set_block(wx, wy, wz + 1, shaft);
    assert_eq!(
        w.power_at(wx + 1, wy, wz + 1),
        0.0,
        "a shaft never bends: nothing comes off its side"
    );
    w.set_block(wx, wy, wz + 2, gear);
    w.set_block(wx + 1, wy, wz + 2, shaft);
    assert!(
        w.power_at(wx + 2, wy, wz + 2) > 0.0,
        "the gear turns the corner"
    );
    // The wheel dresses itself: live water spins it to its run form.
    w.tick_entities(0.3);
    assert_eq!(
        w.get_block(wx, wy, wz),
        b(&reg, "base:water_wheel_run"),
        "a live wheel turns visibly"
    );
}

#[test]
fn multiblock_and_power_cross_a_rotated_planet_seam() {
    use crate::planet::{BlockPos, Direction6, FACE_BLOCKS, Face, step6};

    let reg = base_reg();
    let mut w = test_world_with("machine-seam", reg.clone());
    let y = 120;
    let mouth = BlockPos::new(Face::PosZ, FACE_BLOCKS - 1, y, 4100).unwrap();
    let core = step6(mouth, Direction6::East).unwrap().pos;

    let mut shell = Vec::new();
    for ly in 0..3 {
        for rx in -1..=1 {
            for rz in -1..=1 {
                if rx == 0 && rz == 0 {
                    continue;
                }
                shell.push(core.offset(rx, ly, rz).unwrap());
            }
        }
    }
    let mut chunks: std::collections::BTreeSet<_> = shell.iter().map(|pos| pos.chunk()).collect();
    chunks.insert(core.chunk());
    chunks.insert(mouth.chunk());
    for chunk in chunks {
        w.ensure_chunk(chunk);
    }
    let firebrick = reg.block_id("base:firebrick").unwrap();
    for at in shell {
        w.set_block_at(
            at,
            if at == mouth {
                reg.block_id("base:bloomery").unwrap()
            } else {
                firebrick
            },
        );
    }
    for ly in 0..3 {
        w.set_block_at(core.offset(0, ly, 0).unwrap(), AIR);
    }
    w.set_block_at(mouth, reg.block_id("base:bloomery").unwrap());
    assert_eq!(
        w.check_bloomery_at(mouth),
        Some(core),
        "the firebrick shell validates after its local axes rotate across a seam"
    );

    let source = BlockPos::new(Face::PosZ, FACE_BLOCKS - 2, y + 8, 4108).unwrap();
    let shaft = step6(source, Direction6::East).unwrap().pos;
    let receiver = step6(shaft, step6(source, Direction6::East).unwrap().direction)
        .unwrap()
        .pos;
    for chunk in [source.chunk(), shaft.chunk(), receiver.chunk()] {
        w.ensure_chunk(chunk);
    }
    w.set_block_at(source, reg.block_id("base:water_wheel").unwrap());
    w.set_block_at(shaft, reg.block_id("base:shaft").unwrap());
    let water = source.offset(0, -1, 0).unwrap();
    w.set_block_at(water, reg.water_block(0));
    w.set_block_at(water.offset(0, -1, 0).unwrap(), AIR);
    assert!(
        w.power_at_pos(receiver) > 0.0,
        "a straight shaft remains straight in the rotated destination chart"
    );
}

#[test]
fn shaft_power_crosses_every_directed_planet_seam() {
    use crate::planet::{BlockPos, Direction4, Direction6, step6};

    let reg = base_reg();
    let wheel = b(&reg, "base:water_wheel");
    let shaft_block = b(&reg, "base:shaft");
    let water = reg.water_block(0);
    let mut world = World::new(52, tmp_dir("planet-power-all-seams"), reg);

    for seam in directed_planet_seams() {
        let source =
            BlockPos::new(seam.source.face(), seam.source.u(), 120, seam.source.v()).unwrap();
        let outward = match seam.direction {
            Direction4::East => Direction6::East,
            Direction4::North => Direction6::North,
            Direction4::West => Direction6::West,
            Direction4::South => Direction6::South,
        };
        let first = step6(source, outward).unwrap();
        let shaft = first.pos;
        assert_eq!(shaft.surface(), seam.across);
        let receiver = step6(shaft, first.direction).unwrap().pos;
        let water_pos = source.offset(0, -1, 0).unwrap();
        let drain = water_pos.offset(0, -1, 0).unwrap();
        let missing: std::collections::BTreeSet<_> = [
            source.chunk(),
            shaft.chunk(),
            receiver.chunk(),
            water_pos.chunk(),
        ]
        .into_iter()
        .filter(|chunk| !world.has_chunk(*chunk))
        .collect();
        world.insert_empty_chunks_for_test(missing);

        let fixture_set = |world: &mut World, pos: BlockPos, block| {
            let (x, y, z) = pos.local();
            world
                .chunks_mut()
                .get_mut(&pos.chunk())
                .expect("power fixture chunk")
                .set(x, y, z, block);
        };
        fixture_set(&mut world, source, wheel);
        fixture_set(&mut world, shaft, shaft_block);
        fixture_set(&mut world, receiver, AIR);
        fixture_set(&mut world, water_pos, water);
        fixture_set(&mut world, drain, AIR);

        assert!(
            world.power_at_pos(receiver) > 0.0,
            "shaft power did not remain straight across {:?} {:?}",
            seam.face,
            seam.direction
        );
    }
}
