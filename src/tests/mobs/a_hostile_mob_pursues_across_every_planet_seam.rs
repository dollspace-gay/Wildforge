//! A hostile mob pursues across every planet seam scenarios.

use super::*;

#[test]
fn a_hostile_mob_pursues_across_every_planet_seam() {
    use crate::planet::{BlockPos, Direction4, EntityPos, FACE_BLOCKS, Face, SurfacePos};

    fn fixture_set(world: &mut World, pos: BlockPos, block: crate::registry::BlockId) {
        let (x, y, z) = pos.local();
        world
            .chunks_mut()
            .get_mut(&pos.chunk())
            .expect("the pursuit fixture installs every touched chunk")
            .set(x, y, z, block);
    }

    let reg = base_reg();
    let mut world = World::new(50, tmp_dir("planet-mob-all-seams"), reg.clone());
    let stone = b(&reg, "base:stone");
    let species = reg.animal_id("base:thornling").unwrap();
    let def = reg.animals[species].clone();
    let side = f32::from(FACE_BLOCKS);

    for (edge_index, (face, direction)) in Face::ALL
        .into_iter()
        .flat_map(|face| Direction4::ALL.map(move |direction| (face, direction)))
        .enumerate()
    {
        let varying = 360 + edge_index as u16 * 300;
        let (u, v, chase, cell_u, cell_v, du, dv, pu, pv) = match direction {
            Direction4::East => (
                side - 1.2,
                f32::from(varying) + 0.5,
                Vec3::X,
                i32::from(FACE_BLOCKS) - 1,
                i32::from(varying),
                1,
                0,
                0,
                1,
            ),
            Direction4::North => (
                f32::from(varying) + 0.5,
                side - 1.2,
                Vec3::Z,
                i32::from(varying),
                i32::from(FACE_BLOCKS) - 1,
                0,
                1,
                1,
                0,
            ),
            Direction4::West => (
                1.2,
                f32::from(varying) + 0.5,
                Vec3::NEG_X,
                0,
                i32::from(varying),
                -1,
                0,
                0,
                1,
            ),
            Direction4::South => (
                f32::from(varying) + 0.5,
                1.2,
                Vec3::NEG_Z,
                i32::from(varying),
                0,
                0,
                -1,
                1,
                0,
            ),
        };
        let mut lane = Vec::new();
        for along in -3..=4 {
            for across in -1..=1 {
                lane.push(
                    SurfacePos::canonicalized(
                        face,
                        cell_u + along * du + across * pu,
                        cell_v + along * dv + across * pv,
                    )
                    .unwrap(),
                );
            }
        }
        let chunks: std::collections::BTreeSet<_> = lane
            .iter()
            .map(|surface| crate::planet::ChunkPos::from_surface(*surface))
            .filter(|chunk| !world.has_chunk(*chunk))
            .collect();
        world.insert_empty_chunks_for_test(chunks);
        for surface in lane {
            fixture_set(
                &mut world,
                BlockPos::new(surface.face(), surface.u(), 99, surface.v()).unwrap(),
                stone,
            );
            for y in 100..=103 {
                fixture_set(
                    &mut world,
                    BlockPos::new(surface.face(), surface.u(), y, surface.v()).unwrap(),
                    AIR,
                );
            }
        }

        let start = EntityPos::new(face, u, 100.0, v).unwrap();
        let player = start.translated(chase * 3.0).unwrap().pos;
        assert_ne!(face, player.face());
        let players = [crate::server::PlayerCtx {
            id: 0,
            pos: player,
            spawn: player,
            attackable: true,
            aggro_mod: 0.0,
            quiet_charm: None,
        }];
        let mut mob = crate::mobs::Mob::new_at(species, start, 0.0);
        mob.health = def.health;
        mob.on_ground = true;
        let mut rng = edge_index as u32 + 1;
        let mut events = Vec::new();
        let mut crossed = false;
        for _ in 0..120 {
            mob.tick(&world, &def, &players, 0.05, &mut rng, &mut events);
            crossed |= mob.pos.face() != face;
            if crossed && mob.pos.distance_to(player) < 1.5 {
                break;
            }
        }
        assert!(
            crossed,
            "thornling did not pursue across {face:?} {direction:?}; stopped at {:?}",
            mob.pos
        );
        assert!(mob.pos.is_canonical());
    }
}
