//! Inventory, crafting, physics, survival, and player interaction behavior.

use super::*;

#[test]
fn raycast_hits_nonsolid_plants() {
    let reg = base_reg();
    let mut w = test_world_with("plantray", reg.clone());
    let h = w.surface_height(0, 0);
    let bush = b(&reg, "base:berry_bush");
    // Clear of the terrain: a lake-rim armor column near origin
    // stands a few blocks over the surface here.
    w.set_block(3, h + 10, 0, bush);
    let hit = raycast(&w, Vec3::new(0.5, h as f32 + 10.5, 0.5), Vec3::X, 6.0)
        .expect("plants must be targetable");
    assert_eq!(hit.block, (3, h + 10, 0));
    assert!(!reg.is_solid(bush), "bush stays non-solid for physics");
}

#[test]
fn raycast_hits_placed_block_with_correct_adjacent() {
    let reg = base_reg();
    let mut w = test_world_with("ray", reg.clone());
    let h = w.surface_height(0, 0);
    let y = h + 10;
    w.set_block(5, y, 0, b(&reg, "base:stone"));
    let hit = raycast(&w, Vec3::new(0.5, y as f32 + 0.5, 0.5), Vec3::X, 10.0).unwrap();
    assert_eq!(hit.block, (5, y, 0));
    assert_eq!(hit.adjacent, (4, y, 0));
    assert!(raycast(&w, Vec3::new(0.5, y as f32 + 0.5, 0.5), Vec3::X, 4.0).is_none());
}

#[test]
fn planetary_raycast_selects_a_block_on_the_neighboring_face() {
    use crate::planet::{BlockPos, Direction6, EntityPos, Face, step6};

    let reg = base_reg();
    let mut world = World::new(46, tmp_dir("planet-ray-seam"), reg.clone());
    let adjacent = BlockPos::new(Face::PosZ, crate::planet::FACE_BLOCKS - 1, 100, 4096).unwrap();
    let target = step6(adjacent, Direction6::East).unwrap().pos;
    world.insert_empty_chunks_for_test([adjacent.chunk(), target.chunk()]);
    world.set_block_at(target, b(&reg, "base:stone"));

    let origin = EntityPos::new(
        adjacent.face(),
        f32::from(adjacent.u()) + 0.5,
        f32::from(adjacent.y()) + 0.5,
        f32::from(adjacent.v()) + 0.5,
    )
    .unwrap();
    let hit = crate::raycast::raycast_at(&world, origin, Vec3::X, 3.0).unwrap();
    assert_eq!(hit.block, target);
    assert_eq!(hit.adjacent, adjacent);
}

#[test]
fn planetary_raycast_resolves_every_exact_cube_corner_deterministically() {
    use crate::planet::{EntityPos, FACE_BLOCKS, Face};

    let reg = base_reg();
    let stone = b(&reg, "base:stone");
    let side = f32::from(FACE_BLOCKS);
    for (face_index, face) in Face::ALL.into_iter().enumerate() {
        for (corner_index, (u, v, dir)) in [
            (0.5, 0.5, Vec3::new(-1.0, 0.0, -1.0)),
            (0.5, side - 0.5, Vec3::new(-1.0, 0.0, 1.0)),
            (side - 0.5, 0.5, Vec3::new(1.0, 0.0, -1.0)),
            (side - 0.5, side - 0.5, Vec3::new(1.0, 0.0, 1.0)),
        ]
        .into_iter()
        .enumerate()
        {
            let origin = EntityPos::new(face, u, 100.5, v).unwrap();
            let adjacent = origin.block().unwrap();
            let dir = dir.normalize();
            let corner_time = 0.5 / dir.x.abs();
            let delta = dir * corner_time
                + Vec3::new(2.0e-3_f32.copysign(dir.x), 0.0, 2.0e-3_f32.copysign(dir.z));
            let target = origin.translated(delta).unwrap().pos.block().unwrap();
            assert_ne!(
                target.face(),
                face,
                "a diagonal corner ray must enter the third incident face"
            );

            let mut world = World::new(
                54,
                tmp_dir(&format!("planet-ray-corner-{face_index}-{corner_index}")),
                reg.clone(),
            );
            world.insert_empty_chunks_for_test([adjacent.chunk(), target.chunk()]);
            world.set_block_at(target, stone);

            let first = crate::raycast::raycast_at(&world, origin, dir, 2.0).unwrap_or_else(|| {
                panic!("{face:?} corner {corner_index}: ray missed expected {target:?}")
            });
            let second = crate::raycast::raycast_at(&world, origin, dir, 2.0).unwrap();
            assert_eq!(first, second, "{face:?} corner {corner_index}");
            assert_eq!(first.block, target, "{face:?} corner {corner_index}");
            assert_eq!(first.adjacent, adjacent, "{face:?} corner {corner_index}");
        }
    }
}

#[test]
fn player_crossing_a_face_seam_rotates_velocity_and_yaw_frame() {
    use crate::planet::{Direction4, EntityPos, Face, QuarterTurn, SurfacePos, step4};

    let reg = base_reg();
    let mut world = World::new(48, tmp_dir("planet-player-seam"), reg);
    let (face, direction) = Face::ALL
        .into_iter()
        .flat_map(|face| Direction4::ALL.map(move |direction| (face, direction)))
        .find(|&(face, direction)| {
            let (u, v) = match direction {
                Direction4::East => (crate::planet::FACE_BLOCKS - 1, 4096),
                Direction4::North => (4096, crate::planet::FACE_BLOCKS - 1),
                Direction4::West => (0, 4096),
                Direction4::South => (4096, 0),
            };
            step4(SurfacePos::new(face, u, v).unwrap(), direction).rotation != QuarterTurn::IDENTITY
        })
        .expect("some cube edges exchange chart axes");
    let side = crate::planet::FACE_BLOCKS as f32;
    let (u, v, wish) = match direction {
        Direction4::East => (side - 0.15, 4096.5, Vec3::X),
        Direction4::North => (4096.5, side - 0.15, Vec3::Z),
        Direction4::West => (0.15, 4096.5, Vec3::NEG_X),
        Direction4::South => (4096.5, 0.15, Vec3::NEG_Z),
    };
    let start = EntityPos::new(face, u, 100.0, v).unwrap();
    let across = start.translated(wish).unwrap().pos;
    world.insert_empty_chunks_for_test([start.chunk().unwrap(), across.chunk().unwrap()]);

    let mut player = Player::new_at(start);
    player.fly(&world, wish * 4.0, 0.1);
    assert_ne!(player.pos.face(), face);
    assert!(player.pos.is_canonical());
    assert_ne!(player.frame_rotation, crate::planet::QuarterTurn::IDENTITY);
    assert_eq!(
        player.vel,
        player.frame_rotation.rotate_vec3(wish * 4.0),
        "horizontal velocity follows the destination face frame"
    );
    let yaw = 0.37;
    assert_eq!(
        player.frame_rotation.rotate_yaw(yaw),
        (yaw + player.frame_rotation.turns() as f32 * std::f32::consts::FRAC_PI_2)
            .rem_euclid(std::f32::consts::TAU)
    );
}

#[test]
fn every_planet_seam_carries_walk_sprint_swim_boat_and_flight() {
    use crate::planet::{BlockPos, Direction4, EntityPos, FACE_BLOCKS, Face, SurfacePos};

    fn fixture_set(world: &mut World, pos: BlockPos, block: crate::registry::BlockId) {
        let (x, y, z) = pos.local();
        world
            .chunks_mut()
            .get_mut(&pos.chunk())
            .expect("the all-seams fixture installs every touched chunk")
            .set(x, y, z, block);
    }

    #[derive(Clone, Copy, Debug)]
    enum Mode {
        Walk,
        Sprint,
        Swim,
        Boat,
        Fly,
    }

    let reg = base_reg();
    let mut world = World::new(49, tmp_dir("planet-player-all-seams"), reg.clone());
    let stone = b(&reg, "base:stone");
    let water = reg.water_block(0);
    let boat_species = reg.animal_id("base:boat").unwrap();
    let side = f32::from(FACE_BLOCKS);

    for (edge_index, (face, direction)) in Face::ALL
        .into_iter()
        .flat_map(|face| Direction4::ALL.map(move |direction| (face, direction)))
        .enumerate()
    {
        let varying = 320 + edge_index as u16 * 300;
        let (u, v, heading, cell_u, cell_v, du, dv, pu, pv) = match direction {
            Direction4::East => (
                side - 0.08,
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
                side - 0.08,
                Vec3::Z,
                i32::from(varying),
                i32::from(FACE_BLOCKS) - 1,
                0,
                1,
                1,
                0,
            ),
            Direction4::West => (
                0.08,
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
                0.08,
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
        for along in -2..=3 {
            for across in -1..=1 {
                let surface = SurfacePos::canonicalized(
                    face,
                    cell_u + along * du + across * pu,
                    cell_v + along * dv + across * pv,
                )
                .unwrap();
                lane.push(surface);
            }
        }
        let chunks: std::collections::BTreeSet<_> = lane
            .iter()
            .map(|surface| crate::planet::ChunkPos::from_surface(*surface))
            .filter(|chunk| !world.has_chunk(*chunk))
            .collect();
        world.insert_empty_chunks_for_test(chunks);
        for surface in &lane {
            fixture_set(
                &mut world,
                BlockPos::new(surface.face(), surface.u(), 99, surface.v()).unwrap(),
                stone,
            );
            for y in 100..=102 {
                fixture_set(
                    &mut world,
                    BlockPos::new(surface.face(), surface.u(), y, surface.v()).unwrap(),
                    AIR,
                );
            }
        }

        for mode in [Mode::Walk, Mode::Sprint, Mode::Swim, Mode::Boat, Mode::Fly] {
            let wet = matches!(mode, Mode::Swim | Mode::Boat);
            for surface in &lane {
                for y in 100..=101 {
                    fixture_set(
                        &mut world,
                        BlockPos::new(surface.face(), surface.u(), y, surface.v()).unwrap(),
                        if wet { water } else { AIR },
                    );
                }
            }

            let start = EntityPos::new(face, u, 100.0, v).unwrap();
            let mut player = Player::new_at(start);
            player.on_ground = !wet;
            let input = Input {
                forward: 1.0,
                strafe: 0.0,
                jump: false,
                sprint: matches!(mode, Mode::Sprint),
            };
            match mode {
                Mode::Fly => player.fly(&world, heading * 4.0, 0.1),
                _ => player.update(&world, &input, heading, Vec3::ZERO, 0.05),
            }
            assert_ne!(
                player.pos.face(),
                face,
                "{mode:?} did not cross {face:?} {direction:?}"
            );
            assert!(
                player.pos.is_canonical(),
                "{mode:?} left the finite address space at {face:?} {direction:?}"
            );

            if matches!(mode, Mode::Boat) {
                let mut boat = crate::mobs::Mob::new_at(boat_species, start, 0.0);
                boat.ridden_by = Some(0);
                boat.pos = player
                    .pos
                    .translated(Vec3::new(0.0, -0.35, 0.0))
                    .unwrap()
                    .pos;
                assert_eq!(
                    boat.pos.face(),
                    player.pos.face(),
                    "the ridden hull follows its rider across {face:?} {direction:?}"
                );
                assert!(boat.pos.is_canonical());
            }
        }
    }
}

#[test]
fn collision_and_auto_step_work_across_every_planet_seam() {
    use crate::planet::{BlockPos, Direction4, EntityPos, SurfacePos};

    let mods = tmp_dir("planet-step-mods");
    let step_mod = mods.join("planet_step_fixture");
    std::fs::create_dir_all(&step_mod).unwrap();
    std::fs::write(
        step_mod.join("mod.toml"),
        "id = \"planet_step_fixture\"\nname = \"Planet Step Fixture\"\nversion = \"1.0.0\"\nworld_api = 2\ndepends = [\"base\"]\n",
    )
    .unwrap();
    std::fs::write(
        step_mod.join("blocks.toml"),
        "[[block]]\nid = \"half_step\"\nname = \"Half Step\"\ntexture = \"@stone\"\nhardness = 1.0\nheight = 0.5\n",
    )
    .unwrap();

    let reg = Arc::new(crate::registry::load(&mods));
    let stone = b(&reg, "base:stone");
    let half_step = b(&reg, "planet_step_fixture:half_step");
    assert_eq!(reg.block(half_step).height, Some(0.5));
    assert!(reg.is_solid(half_step));
    let mut world = World::new(50, tmp_dir("planet-collision-all-seams"), reg.clone());

    let fixture_set = |world: &mut World, pos: BlockPos, block| {
        let (x, y, z) = pos.local();
        world
            .chunks_mut()
            .get_mut(&pos.chunk())
            .expect("collision fixture chunk")
            .set(x, y, z, block);
    };

    for seam in directed_planet_seams() {
        let (du, dv, pu, pv) = match seam.direction {
            Direction4::East => (1, 0, 0, 1),
            Direction4::North => (0, 1, 1, 0),
            Direction4::West => (-1, 0, 0, 1),
            Direction4::South => (0, -1, 1, 0),
        };
        let mut lane = Vec::new();
        for along in -2..=3 {
            for across in -1..=1 {
                lane.push(
                    SurfacePos::canonicalized(
                        seam.face,
                        i32::from(seam.source.u()) + along * du + across * pu,
                        i32::from(seam.source.v()) + along * dv + across * pv,
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
        for surface in &lane {
            fixture_set(
                &mut world,
                BlockPos::new(surface.face(), surface.u(), 99, surface.v()).unwrap(),
                stone,
            );
            for y in 100..=102 {
                fixture_set(
                    &mut world,
                    BlockPos::new(surface.face(), surface.u(), y, surface.v()).unwrap(),
                    AIR,
                );
            }
        }

        // A two-block-tall wall on the destination face must stop even a
        // deliberately oversized creative sweep before its center crosses.
        let wall_feet =
            BlockPos::new(seam.across.face(), seam.across.u(), 100, seam.across.v()).unwrap();
        fixture_set(&mut world, wall_feet, stone);
        fixture_set(&mut world, wall_feet.offset(0, 1, 0).unwrap(), stone);
        let start = EntityPos::new(
            seam.source.face(),
            f32::from(seam.source.u()) + 0.5,
            100.0,
            f32::from(seam.source.v()) + 0.5,
        )
        .unwrap();
        let mut blocked = Player::new_at(start);
        blocked.fly(&world, seam.heading * 8.0, 0.25);
        assert_eq!(
            blocked.pos.face(),
            seam.face,
            "collision leaked through {:?} {:?}",
            seam.face,
            seam.direction
        );
        assert!(!blocked.collides(&world, blocked.pos));

        // Replacing the wall with a half-height solid exercises the auto-step
        // branch while the lift, horizontal sweep, and settle straddle charts.
        fixture_set(&mut world, wall_feet, half_step);
        fixture_set(&mut world, wall_feet.offset(0, 1, 0).unwrap(), AIR);
        let mut stepped = Player::new_at(start);
        stepped.on_ground = true;
        let input = Input {
            forward: 1.0,
            strafe: 0.0,
            jump: false,
            sprint: false,
        };
        let mut heading = seam.heading;
        for _ in 0..16 {
            stepped.update(&world, &input, heading, Vec3::ZERO, 0.05);
            heading = stepped.frame_rotation.rotate_vec3(heading);
            if stepped.pos.face() == seam.across.face() {
                break;
            }
        }
        assert_eq!(
            stepped.pos.face(),
            seam.across.face(),
            "auto-step did not cross {:?} {:?}",
            seam.face,
            seam.direction
        );
        assert!(
            stepped.pos.y() >= 100.49,
            "auto-step did not settle on the half block at {:?} {:?}: y={}",
            seam.face,
            seam.direction,
            stepped.pos.y()
        );
        assert!(!stepped.collides(&world, stepped.pos));
    }
}

#[test]
fn player_falls_lands_and_jumps() {
    let mut w = test_world("fall");
    // A built platform in open sky: the plate map decides where the
    // sea is, so physics tests bring their own ground.
    let reg = w.reg.clone();
    let stone = reg.block_id("base:stone").unwrap();
    for x in 2..=6 {
        for z in 2..=6 {
            w.set_block(x, 119, z, stone);
        }
    }
    let h = 119;
    let mut p = Player::new(Vec3::new(4.5, h as f32 + 6.0, 4.5));
    let idle = Input {
        forward: 0.0,
        strafe: 0.0,
        jump: false,
        sprint: false,
    };
    for _ in 0..300 {
        p.update(&w, &idle, Vec3::Z, Vec3::X, 1.0 / 60.0);
    }
    assert!(p.on_ground);
    let ground = p.pos.y;
    let jump = Input { jump: true, ..idle };
    let mut peak = ground;
    for _ in 0..60 {
        p.update(&w, &jump, Vec3::Z, Vec3::X, 1.0 / 60.0);
        peak = peak.max(p.pos.y);
    }
    let gain = peak - ground;
    assert!(gain > 1.0 && gain < 2.0, "jump height {gain}");
}

#[test]
fn inventory_and_clicks() {
    let reg = base_reg();
    let dirt = it(&reg, "base:dirt");
    let stone = it(&reg, "base:stone");
    let pick = it(&reg, "base:wood_pickaxe");
    let mut inv = Inventory::new();
    assert_eq!(inv.add(&reg, dirt, 70), 0);
    assert_eq!(inv.slots[0].unwrap().count, 64);
    assert_eq!(inv.slots[1].unwrap().count, 6);
    inv.add(&reg, pick, 1);
    inv.add(&reg, pick, 1);
    assert_eq!(inv.slots[2].unwrap().count, 1);
    assert_eq!(inv.slots[3].unwrap().count, 1, "tools must not stack");
    // Wear the tool out.
    let uses = reg.item(pick).durability;
    for _ in 0..uses {
        inv.wear_tool(&reg, 2);
    }
    assert!(inv.slots[2].is_none(), "tool breaks at zero durability");
    // Click matrix.
    let d = |n| Some(ItemStack::new(&reg, dirt, n));
    let s = |n| Some(ItemStack::new(&reg, stone, n));
    assert_eq!(click_stack(&reg, d(5), None, false), (None, d(5)));
    assert_eq!(click_stack(&reg, d(60), d(10), false), (d(64), d(6)));
    assert_eq!(click_stack(&reg, s(3), d(5), false), (d(5), s(3)));
    assert_eq!(click_stack(&reg, None, d(5), true), (d(1), d(4)));
    assert_eq!(click_stack(&reg, d(5), None, true), (d(2), d(3)));
}

#[test]
fn crafting_matches_shapes_and_grids() {
    let reg = base_reg();
    let planks = it(&reg, "base:planks");
    let stick = it(&reg, "base:stick");
    let cobble = it(&reg, "base:cobblestone");
    let grid = |size: usize, cells: &[(usize, crate::registry::ItemId)]| {
        let mut g = vec![None; size * size];
        for &(i, item) in cells {
            g[i] = Some(ItemStack::new(&reg, item, 1));
        }
        g
    };
    // Log -> planks in 2x2.
    let log = it(&reg, "base:log");
    let g = grid(2, &[(3, log)]);
    let r = crate::crafting::match_recipe(&reg, &g, 2).expect("log->planks");
    assert_eq!((r.output, r.count), (planks, 4));
    // Pickaxe in 3x3, not in 2x2.
    let g = grid(
        3,
        &[
            (0, planks),
            (1, planks),
            (2, planks),
            (4, stick),
            (7, stick),
        ],
    );
    assert_eq!(
        crate::crafting::match_recipe(&reg, &g, 3).unwrap().output,
        it(&reg, "base:wood_pickaxe")
    );
    let g2 = grid(2, &[(0, planks), (1, planks), (2, stick)]);
    assert!(crate::crafting::match_recipe(&reg, &g2, 2).is_none());
    // Mirrored axe.
    let g = grid(
        3,
        &[
            (0, cobble),
            (1, cobble),
            (4, cobble),
            (3, stick),
            (6, stick),
        ],
    );
    assert_eq!(
        crate::crafting::match_recipe(&reg, &g, 3).unwrap().output,
        it(&reg, "base:stone_axe")
    );
    // Mod recipe works too (loaded in data_mod test above; here just consume).
    let mut g = grid(2, &[(0, planks), (1, planks)]);
    crate::crafting::consume(&mut g);
    assert!(g[0].is_none() && g[1].is_none());
}

#[test]
fn armor_reduction_curve() {
    assert_eq!(crate::reduced_damage(10.0, 0), 10.0);
    assert!(
        (crate::reduced_damage(10.0, 7) - 7.2).abs() < 0.01,
        "full leather: 28%"
    );
    assert!(
        (crate::reduced_damage(10.0, 11) - 5.6).abs() < 0.01,
        "full bronze: 44%"
    );
    assert!(
        (crate::reduced_damage(10.0, 50) - 4.0).abs() < 0.001,
        "capped at 60%"
    );
}

#[test]
fn player_arrows_strike_mobs_and_stick_in_walls() {
    let reg = base_reg();
    let mut w = test_world("arrows");
    let deer_i = reg.animal_id("base:deer").unwrap();
    let mut deer = crate::mobs::Mob::new(deer_i, Vec3::new(8.5, 220.0, 8.5), 0.0);
    deer.health = 10.0;
    let di = w.mob_count(); // natural wildlife is seeded too — track ours
    w.spawn_mob(deer);
    let arrow_item = it(&reg, "base:arrow");
    // Arrow flying at the deer: hits through the normal hurt path.
    w.spawn_projectile(crate::mobs::Projectile {
        stable_id: 0,
        pos: ep(Vec3::new(8.5, 220.5, 5.0)),
        vel: Vec3::new(0.0, 0.5, 18.0),
        tile: 0,
        damage: 6.0,
        damage_type: None,
        age: 0.0,
        from_player: true,
        drop_item: Some(arrow_item),
        preparation_payload: None,
        owner: 0,
    });
    let far = Vec3::new(300.0, 80.0, 300.0);
    let mut player_dmg = 0.0;
    for _ in 0..40 {
        player_dmg += w
            .tick_projectiles(
                &[crate::server::PlayerCtx {
                    id: 0,
                    pos: ep(far),
                    spawn: ep(Vec3::ZERO),
                    attackable: true,
                    aggro_mod: 0.0,
                    quiet_charm: None,
                }],
                1.0 / 30.0,
            )
            .iter()
            .map(|(_, d, _)| d)
            .sum::<f32>();
    }
    assert!(
        w.mobs()[di].health < 10.0,
        "arrow connected (health {})",
        w.mobs()[di].health
    );
    assert_eq!(player_dmg, 0.0, "player arrows never hit the player");
    assert!(w.pending_drops().is_empty(), "flesh hits consume the arrow");
    // Arrow into a wall drops a recoverable arrow item.
    let stone = reg.block_id("base:stone").unwrap();
    for y in 220..226 {
        w.set_block(2, y, 20, stone);
    }
    w.spawn_projectile(crate::mobs::Projectile {
        stable_id: 0,
        pos: ep(Vec3::new(2.5, 222.5, 16.0)),
        vel: Vec3::new(0.0, 0.0, 16.0),
        tile: 0,
        damage: 6.0,
        damage_type: None,
        age: 0.0,
        from_player: true,
        drop_item: Some(arrow_item),
        preparation_payload: None,
        owner: 0,
    });
    for _ in 0..40 {
        w.tick_projectiles(
            &[crate::server::PlayerCtx {
                id: 0,
                pos: ep(far),
                spawn: ep(Vec3::ZERO),
                attackable: true,
                aggro_mod: 0.0,
                quiet_charm: None,
            }],
            1.0 / 30.0,
        );
    }
    assert!(
        w.pending_drops().iter().any(|(_, s)| s.item == arrow_item),
        "wall hit dropped the arrow for recovery"
    );
}

#[test]
fn footstep_materials_map_by_surface() {
    use crate::audio::{BreakMat, StepMat, step_mat};
    // Specials win on name alone.
    assert_eq!(step_mat("base:snow", BreakMat::Soft), StepMat::Snow);
    assert_eq!(step_mat("base:snow_layer", BreakMat::Leafy), StepMat::Snow);
    assert_eq!(step_mat("base:sand", BreakMat::Soft), StepMat::Loose);
    assert_eq!(step_mat("base:gravel", BreakMat::Soft), StepMat::Loose);
    // Everything else follows the break-material family.
    assert_eq!(step_mat("base:stone", BreakMat::Stone), StepMat::Stone);
    assert_eq!(step_mat("base:planks", BreakMat::Wood), StepMat::Wood);
    assert_eq!(step_mat("base:dirt", BreakMat::Soft), StepMat::Soft);
    assert_eq!(step_mat("base:leaves", BreakMat::Leafy), StepMat::Leafy);

    // And the real registry agrees on the interesting surfaces.
    let reg = base_reg();
    for (name, want) in [
        ("base:snow", StepMat::Snow),
        ("base:snow_layer", StepMat::Snow),
        ("base:sand", StepMat::Loose),
    ] {
        let b = reg.block_id(name).expect(name);
        let fallback = match reg.block(b).tool {
            Some(crate::registry::ToolKind::Pickaxe) => BreakMat::Stone,
            Some(crate::registry::ToolKind::Axe) => BreakMat::Wood,
            Some(crate::registry::ToolKind::Shovel) => BreakMat::Soft,
            _ => BreakMat::Leafy,
        };
        assert_eq!(step_mat(&reg.block(b).name, fallback), want, "{name}");
    }
}

#[test]
fn pickup_ramp_steps_and_caps() {
    use crate::audio::pickup_pitch;
    assert_eq!(pickup_pitch(0), 1.0);
    // Monotonic rise, one near-semitone per step.
    for s in 0..7 {
        let step = pickup_pitch(s + 1) / pickup_pitch(s);
        assert!((step - 2.0f32.powf(1.0 / 12.0)).abs() < 1e-4);
    }
    // Caps at +7 semitones no matter the streak.
    assert_eq!(pickup_pitch(7), pickup_pitch(100));
    assert!((pickup_pitch(7) - 2.0f32.powf(7.0 / 12.0)).abs() < 1e-4);
}

#[test]
fn a_fast_step_cannot_pass_through_a_wall() {
    // Movement used to test only where a step ENDED. A step long enough to
    // clear a wall therefore went straight through it: both ends in open air,
    // the wall between them never consulted. The server simulates up to a
    // quarter second in one go and a sprint covers well over a block in that,
    // so a frame hitch was enough to walk through a house.
    let mut w = test_world("tunnel");
    let reg = w.reg.clone();
    let stone = reg.block_id("base:stone").unwrap();
    let y = 119;
    // A floor to stand on, and a one-block-thick wall standing on it at x = 8.
    for x in 0..=16 {
        for z in 2..=6 {
            w.set_block(x, y, z, stone);
        }
    }
    for z in 2..=6 {
        for dy in 1..=3 {
            w.set_block(8, y + dy, z, stone);
        }
    }

    let start = Vec3::new(4.5, y as f32 + 1.0, 4.5);
    let sprint = Input {
        forward: 1.0,
        strafe: 0.0,
        jump: false,
        sprint: true,
    };
    // One enormous step, the shape a hitch produces: far enough that the
    // destination is open ground on the far side of the wall.
    let mut p = Player::new(start);
    for _ in 0..40 {
        p.update(&w, &sprint, Vec3::X, Vec3::Z, 0.25);
    }
    assert!(
        p.pos.x < 8.0,
        "the player is at x={} — through a solid wall at x=8",
        p.pos.x
    );

    // And the invariant that always held must still hold: wherever a move
    // finishes, the player is not standing inside a solid block.
    assert!(
        !p.collides(&w, p.pos),
        "the player finished inside a solid block at {:?}",
        p.pos
    );
}
