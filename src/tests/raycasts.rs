//! Real resident-world ray stages with independently specified target cells.
//!
//! Axis paths use discrete block topology, rather than copying the floating
//! point ray walker. Blank chunk stages isolate aiming from generated terrain;
//! both authority and replica still use their production storage/read paths.

use std::collections::BTreeSet;

use super::{base_reg, directed_planet_seams, tmp_dir};
use crate::chunk::{Chunk, ChunkPos};
use crate::planet::{BlockPos, Direction4, Direction6, EntityPos, FACE_BLOCKS, Face, step4, step6};
use crate::raycast::{PlanetHit, raycast_at, raycast_water_at};
use crate::registry::{AIR, BlockId};
use crate::world::{ReplicaWorld, ReplicationTarget, RevealKey, TerrainRead, World};
use glam::Vec3;

fn center(position: BlockPos) -> EntityPos {
    EntityPos::new(
        position.face(),
        f32::from(position.u()) + 0.5,
        f32::from(position.y()) + 0.5,
        f32::from(position.v()) + 0.5,
    )
    .unwrap()
}

fn heading(direction: Direction6) -> Vec3 {
    match direction {
        Direction6::East => Vec3::X,
        Direction6::North => Vec3::Z,
        Direction6::West => Vec3::NEG_X,
        Direction6::South => Vec3::NEG_Z,
        Direction6::Up => Vec3::Y,
        Direction6::Down => Vec3::NEG_Y,
    }
}

fn horizontal(direction: Direction4) -> Direction6 {
    match direction {
        Direction4::East => Direction6::East,
        Direction4::North => Direction6::North,
        Direction4::West => Direction6::West,
        Direction4::South => Direction6::South,
    }
}

fn axis_path(start: BlockPos, mut direction: Direction6, distance: usize) -> Vec<BlockPos> {
    let mut path = Vec::with_capacity(distance + 1);
    path.push(start);
    for _ in 0..distance {
        let next = step6(*path.last().unwrap(), direction).unwrap();
        path.push(next.pos);
        direction = next.direction;
    }
    path
}

fn stage(world: &mut World, cells: &[BlockPos]) {
    world.chunks_mut().clear();
    let positions: BTreeSet<ChunkPos> = cells.iter().map(|cell| cell.chunk()).collect();
    for position in positions {
        world.chunks_mut().insert(position, Chunk::new());
    }
}

fn stamp(world: &mut World, position: BlockPos, block: BlockId) {
    let (x, y, z) = position.local();
    world
        .chunks_mut()
        .get_mut(&position.chunk())
        .unwrap()
        .set(x, y, z, block);
}

fn assert_ray(
    world: &(impl TerrainRead + ?Sized),
    origin: EntityPos,
    direction: Vec3,
    reach: f32,
    expected: Option<PlanetHit>,
) {
    assert_eq!(
        raycast_at(world, origin, direction, reach),
        expected,
        "origin={origin:?}, direction={direction:?}, reach={reach}"
    );
}

fn last_hit(path: &[BlockPos]) -> PlanetHit {
    PlanetHit {
        block: path[path.len() - 1],
        adjacent: path[path.len() - 2],
    }
}

#[test]
fn generated_axis_targets_survive_chunk_transitions_on_every_face() {
    let registry = base_reg();
    let stone = registry.block_id("base:stone").unwrap();
    let plant = registry.block_id("base:berry_bush").unwrap();
    assert!(!registry.is_solid(plant));
    let mut world = World::new(61, tmp_dir("ray-axis-stages"), registry);

    // Both negative and positive centered chart coordinates, all six axis
    // directions, and targets before/after multiple 16-cell chunk boundaries.
    for face in Face::ALL.into_iter().chain([Face::Deep]) {
        for anchor in [48, 4128] {
            for direction in Direction6::ALL {
                let u = anchor + if direction == Direction6::East { 15 } else { 0 };
                let v = anchor
                    + if direction == Direction6::North {
                        15
                    } else {
                        0
                    };
                let start = BlockPos::new(face, u, 112, v).unwrap();
                for distance in [1, 7, 17, 33] {
                    let path = axis_path(start, direction, distance);
                    let expected = last_hit(&path);
                    stage(&mut world, &path);
                    stamp(
                        &mut world,
                        expected.block,
                        if distance == 7 { plant } else { stone },
                    );
                    let origin = center(start);
                    let aim = heading(direction);
                    assert_ray(&world, origin, aim, distance as f32, Some(expected));
                    assert_ray(&world.view(), origin, aim, distance as f32, Some(expected));
                    assert_ray(&world, origin, aim, distance as f32 - 1.0, None);
                }
            }
        }
    }
}

#[test]
fn every_directed_seam_retains_targets_after_resident_and_missing_chunks() {
    let registry = base_reg();
    let stone = registry.block_id("base:stone").unwrap();
    let mut world = World::new(62, tmp_dir("ray-seam-stages"), registry);
    for seam in directed_planet_seams() {
        let start = BlockPos::new(seam.face, seam.source.u(), 120, seam.source.v()).unwrap();
        let path = axis_path(start, horizontal(seam.direction), 35);
        let expected = last_hit(&path);
        assert_eq!(path[1].surface(), seam.across);
        stage(&mut world, &path);
        stamp(&mut world, expected.block, stone);
        assert_ray(&world, center(start), seam.heading, 35.0, Some(expected));

        // The ray leaves a resident source, crosses an absent destination
        // chunk as air, and must reacquire a later resident destination chunk.
        let missing = path[8].chunk();
        assert_ne!(missing, start.chunk());
        assert_ne!(missing, expected.block.chunk());
        world.chunks_mut().remove(&missing).unwrap();
        assert_ray(
            &world.view(),
            center(start),
            seam.heading,
            35.0,
            Some(expected),
        );

        // A fresh cast can also start without a resident reference at all.
        world.chunks_mut().remove(&start.chunk()).unwrap();
        assert_ray(&world, center(start), seam.heading, 35.0, Some(expected));
        world.chunks_mut().remove(&expected.block.chunk()).unwrap();
        assert_ray(&world, center(start), seam.heading, 35.0, None);
    }
}

#[test]
fn exact_cube_corner_targets_follow_discrete_u_before_v_topology() {
    let registry = base_reg();
    let stone = registry.block_id("base:stone").unwrap();
    let mut world = World::new(63, tmp_dir("ray-corner-stages"), registry);
    for face in Face::ALL {
        for (u, u_direction) in [(0, Direction4::West), (FACE_BLOCKS - 1, Direction4::East)] {
            for (v, v_direction) in [(0, Direction4::South), (FACE_BLOCKS - 1, Direction4::North)] {
                let start = BlockPos::new(face, u, 120, v).unwrap();
                // Independently compose two discrete cell edges in the
                // documented corner order; do not call translated() or DDA.
                let first = step4(start.surface(), u_direction);
                let diagonal = step4(first.pos, first.rotation.map_direction(v_direction)).pos;
                let target =
                    BlockPos::new(diagonal.face(), diagonal.u(), 120, diagonal.v()).unwrap();
                assert_ne!(target.face(), face);
                let expected = PlanetHit {
                    block: target,
                    adjacent: start,
                };
                stage(&mut world, &[start, target]);
                stamp(&mut world, target, stone);
                let aim = heading(horizontal(u_direction)) + heading(horizontal(v_direction));
                assert_ray(&world, center(start), aim, 2.0, Some(expected));
                assert_ray(&world.view(), center(start), aim, 2.0, Some(expected));
            }
        }
    }
}

#[test]
fn hidden_fluid_and_plant_targets_keep_aiming_rules_between_casts() {
    let registry = base_reg();
    let stone = registry.block_id("base:stone").unwrap();
    let water = registry.block_id("base:water").unwrap();
    let plant = registry.block_id("base:berry_bush").unwrap();
    let settlement = registry.settlement_id("base:elder_haven").unwrap();
    let mut world = World::new(64, tmp_dir("ray-visibility-stages"), registry);
    for face in Face::ALL {
        let start = BlockPos::new(face, 31, 120, 48).unwrap();
        let path = axis_path(start, Direction6::East, 23);
        stage(&mut world, &path);
        stamp(&mut world, path[3], stone);
        stamp(&mut world, path[5], water);
        stamp(&mut world, path[18], plant);
        stamp(&mut world, path[23], stone);
        world.hide_at(
            path[3],
            RevealKey {
                settlement,
                tier: 2,
            },
        );
        let origin = center(start);
        let plant_hit = PlanetHit {
            block: path[18],
            adjacent: path[17],
        };
        assert_ray(&world, origin, Vec3::X, 24.0, Some(plant_hit));
        assert_ray(&world.view(), origin, Vec3::X, 24.0, Some(plant_hit));
        assert_eq!(
            raycast_water_at(&world, origin, Vec3::X, 24.0),
            Some(PlanetHit {
                block: path[5],
                adjacent: path[4]
            })
        );

        // Each cast observes current residents and the live hidden overlay;
        // the cache is never retained across a reveal or a block mutation.
        assert_eq!(world.reveal_settlement(settlement, 2), 1);
        assert_ray(
            &world,
            origin,
            Vec3::X,
            24.0,
            Some(PlanetHit {
                block: path[3],
                adjacent: path[2],
            }),
        );
        stamp(&mut world, path[3], AIR);
        stamp(&mut world, path[5], AIR);
        stamp(&mut world, path[18], AIR);
        assert_ray(&world, origin, Vec3::X, 24.0, Some(last_hit(&path)));
    }
}

#[test]
fn ray_reach_starting_cells_and_vertical_shell_limits_remain_exact() {
    let registry = base_reg();
    let stone = registry.block_id("base:stone").unwrap();
    let mut world = World::new(65, tmp_dir("ray-limit-stages"), registry);
    for face in Face::ALL {
        let start = BlockPos::new(face, 31, 120, 48).unwrap();
        let path = axis_path(start, Direction6::East, 1);
        stage(&mut world, &path);
        stamp(&mut world, path[1], stone);
        let origin = center(start);
        assert_ray(&world, origin, Vec3::X, 0.5, Some(last_hit(&path)));
        assert_ray(
            &world,
            origin,
            Vec3::X,
            f32::from_bits(0.5_f32.to_bits() - 1),
            None,
        );
        stamp(&mut world, start, stone);
        assert_ray(
            &world,
            origin,
            Vec3::X,
            0.0,
            Some(PlanetHit {
                block: start,
                adjacent: start,
            }),
        );
        assert_ray(&world, origin, Vec3::ZERO, 1.0, None);
        assert_ray(&world, origin, Vec3::X, -1.0, None);

        stage(&mut world, &[start]);
        for (y, aim) in [
            (0.5, Vec3::NEG_Y),
            (255.5, Vec3::Y),
            (-0.5, Vec3::Y),
            (256.5, Vec3::NEG_Y),
        ] {
            let origin = EntityPos::new(face, 31.5, y, 48.5).unwrap();
            assert_ray(&world, origin, aim, 2.0, None);
        }
    }
}

#[test]
fn replica_and_replica_view_use_the_same_resident_ray_targets() {
    let registry = base_reg();
    let stone = registry.block_id("base:stone").unwrap();
    let water = registry.block_id("base:water").unwrap();
    let mut authority = World::new(66, tmp_dir("ray-replica-stages"), registry.clone());
    let remap: Vec<BlockId> = (0..registry.blocks.len())
        .map(|id| BlockId(u16::try_from(id).unwrap()))
        .collect();
    for face in Face::ALL {
        let start = BlockPos::new(face, 31, 120, 48).unwrap();
        let path = axis_path(start, Direction6::East, 35);
        let expected = last_hit(&path);
        stage(&mut authority, &path);
        stamp(&mut authority, path[3], water);
        stamp(&mut authority, expected.block, stone);
        let chunks: Vec<_> = authority
            .chunks()
            .iter()
            .map(|(&position, chunk)| (position, crate::world::encode_chunk_for_test(chunk)))
            .collect();
        let mut replica = ReplicaWorld::new(66, registry.clone(), 0.0);
        replica.insert_remote_chunks(
            chunks
                .iter()
                .map(|(position, bytes)| (*position, bytes.as_slice())),
            &remap,
        );
        assert_ray(&replica, center(start), Vec3::X, 35.0, Some(expected));
        assert_ray(
            &replica.view(),
            center(start),
            Vec3::X,
            35.0,
            Some(expected),
        );
        assert_eq!(
            raycast_water_at(&replica, center(start), Vec3::X, 35.0),
            Some(PlanetHit {
                block: path[3],
                adjacent: path[2]
            })
        );
    }
}
