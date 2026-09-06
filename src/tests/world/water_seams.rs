//! Water seams scenarios.

use super::*;

#[test]
fn finite_water_crosses_a_real_planet_face_seam() {
    use crate::planet::{BlockPos, Direction6, step6};

    let reg = base_reg();
    let mut world = World::new(42, tmp_dir("planet-water-all-seams"), reg.clone());
    let stone = b(&reg, "base:stone");

    for seam in directed_planet_seams() {
        let source =
            BlockPos::new(seam.source.face(), seam.source.u(), 100, seam.source.v()).unwrap();
        let across =
            BlockPos::new(seam.across.face(), seam.across.u(), 100, seam.across.v()).unwrap();
        let missing: Vec<_> = [source.chunk(), across.chunk()]
            .into_iter()
            .filter(|chunk| !world.has_chunk(*chunk))
            .collect();
        world.insert_empty_chunks_for_test(missing);

        for cell in [source, across] {
            world.set_block_at(step6(cell, Direction6::Down).unwrap().pos, stone);
            for direction in [
                Direction6::East,
                Direction6::North,
                Direction6::West,
                Direction6::South,
            ] {
                let neighbor = step6(cell, direction).unwrap().pos;
                if neighbor != source && neighbor != across {
                    world.set_block_at(neighbor, stone);
                }
            }
        }

        world.set_block_at(source, reg.water_for_volume(8));
        for _ in 0..32 {
            if !world.tick_water(64) {
                break;
            }
        }
        let source_volume = reg.water_volume(world.get_block_at(source)).unwrap_or(0);
        let across_volume = reg.water_volume(world.get_block_at(across)).unwrap_or(0);
        assert_eq!(
            source_volume + across_volume,
            8,
            "volume is conserved at {:?} {:?}",
            seam.face,
            seam.direction
        );
        assert_eq!(
            (source_volume, across_volume),
            (4, 4),
            "fluid did not treat {:?} {:?} as an ordinary neighbor",
            seam.face,
            seam.direction
        );
    }
}

#[test]
fn finite_lava_crosses_every_directed_planet_seam() {
    use crate::planet::{BlockPos, Direction6, step6};

    let reg = base_reg();
    let mut world = World::new(53, tmp_dir("planet-lava-all-seams"), reg.clone());
    let stone = b(&reg, "base:stone");

    for seam in directed_planet_seams() {
        let source =
            BlockPos::new(seam.source.face(), seam.source.u(), 104, seam.source.v()).unwrap();
        let across =
            BlockPos::new(seam.across.face(), seam.across.u(), 104, seam.across.v()).unwrap();
        let missing: Vec<_> = [source.chunk(), across.chunk()]
            .into_iter()
            .filter(|chunk| !world.has_chunk(*chunk))
            .collect();
        world.insert_empty_chunks_for_test(missing);

        for cell in [source, across] {
            world.set_block_at(step6(cell, Direction6::Down).unwrap().pos, stone);
            for direction in [
                Direction6::East,
                Direction6::North,
                Direction6::West,
                Direction6::South,
            ] {
                let neighbor = step6(cell, direction).unwrap().pos;
                if neighbor != source && neighbor != across {
                    world.set_block_at(neighbor, stone);
                }
            }
        }

        world.set_block_at(source, reg.lava_for_volume(8));
        for _ in 0..32 {
            if !world.tick_lava(64) {
                break;
            }
        }
        let source_volume = reg.lava_volume(world.get_block_at(source)).unwrap_or(0);
        let across_volume = reg.lava_volume(world.get_block_at(across)).unwrap_or(0);
        assert_eq!(
            source_volume + across_volume,
            8,
            "lava volume changed at {:?} {:?}",
            seam.face,
            seam.direction
        );
        assert_eq!(
            (source_volume, across_volume),
            (4, 4),
            "lava did not cross {:?} {:?}",
            seam.face,
            seam.direction
        );
    }
}
