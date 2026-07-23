//! World persistence, block mutation, ticks, fluids, lighting, and weather.

use super::*;
use std::collections::HashMap;

#[test]
fn octant_meta_roundtrips_through_save() {
    let reg = base_reg();
    let mut world = test_world_with("octsave", reg.clone());
    let sand = b(&reg, "base:surface_sand");
    world.set_block_meta(2, 75, 2, sand, 0b1010_0101);
    world.set_block(3, 75, 3, sand);
    world.save_modified();

    let mut loaded = World::load_or_create(world.save_dir_for_test(), reg);
    for x in -2..=2 {
        for z in -2..=2 {
            loaded.ensure_chunk(ChunkPos { x, z });
        }
    }
    assert_eq!(loaded.get_meta(2, 75, 2), 0b1010_0101);
    assert_eq!(loaded.get_meta(3, 75, 3), 0xff);
}

#[test]
fn octant_mesh_emits_per_filled_octant() {
    let reg = base_reg();
    let mut world = test_world_with("octmesh", reg.clone());
    let sand = b(&reg, "base:surface_sand");
    let pos = ChunkPos { x: 0, z: 0 };
    let (x, y, z) = (8, 200, 8);
    let baseline = crate::mesher::mesh_chunk(&world, pos).opaque_verts.len();
    world.set_block_meta(x, y, z, sand, 1);
    let one = crate::mesher::mesh_chunk(&world, pos).opaque_verts.len();
    assert_eq!(one - baseline, 6 * 4);
    world.set_block(x, y, z, sand);
    let full = crate::mesher::mesh_chunk(&world, pos).opaque_verts.len();
    assert_eq!(full - baseline, 6 * 4);
}

#[test]
fn octant_collision_is_sub_cell() {
    let reg = base_reg();
    let mut world = test_world_with("octcol", reg.clone());
    let sand = b(&reg, "base:surface_sand");
    let idle = Input {
        forward: 0.0,
        strafe: 0.0,
        jump: false,
        sprint: false,
    };
    let y = 190;
    world.set_block(0, y, 0, sand);
    let mut player = Player::new(Vec3::new(0.5, y as f32 + 5.0, 0.5));
    for _ in 0..400 {
        player.update(&world, &idle, Vec3::Z, Vec3::X, 1.0 / 60.0);
    }
    assert!(player.on_ground);
    assert!((player.pos.y - (y as f32 + 1.0)).abs() < 0.05);

    world.set_block_meta(0, y, 0, sand, 0b0000_1111);
    let mut player = Player::new(Vec3::new(0.5, y as f32 + 5.0, 0.5));
    for _ in 0..400 {
        player.update(&world, &idle, Vec3::Z, Vec3::X, 1.0 / 60.0);
    }
    assert!(player.on_ground);
    assert!((player.pos.y - (y as f32 + 0.5)).abs() < 0.05);
}

fn sand_volume(world: &World, sand: crate::registry::BlockId, y: i32) -> u32 {
    let mut volume = 0;
    for x in -8..9 {
        for z in -8..9 {
            for yy in (y - 4)..(y + 6) {
                if world.get_block(x, yy, z) == sand {
                    volume += world.get_meta(x, yy, z).count_ones();
                }
            }
        }
    }
    volume
}

#[test]
fn relax_flat_sand_is_stable() {
    let reg = base_reg();
    let mut world = test_world_with("flatsand", reg.clone());
    let sand = b(&reg, "base:surface_sand");
    let y = 100;
    for x in -4..=4 {
        for z in -4..=4 {
            world.set_block(x, y, z, sand);
        }
    }
    let before = sand_volume(&world, sand, y);
    assert!(!world.relax_sand(sand, 0, 0, y, 1, 0));
    assert_eq!(sand_volume(&world, sand, y), before);
}

#[test]
fn relax_slope_flows_downhill_conserving() {
    let reg = base_reg();
    let mut world = test_world_with("slopesand", reg.clone());
    let sand = b(&reg, "base:surface_sand");
    let stone = b(&reg, "base:stone");
    let y = 100;
    for x in -1..=6 {
        for z in -1..=3 {
            world.set_block(x, y - 1, z, stone);
        }
    }
    for x in 0..=1 {
        for z in 0..=1 {
            for height in 0..3 {
                world.set_block(x, y + height, z, sand);
            }
        }
    }
    let before = sand_volume(&world, sand, y);
    for _ in 0..40 {
        world.relax_sand(sand, 2, 1, y, 5, 0);
    }
    assert_eq!(sand_volume(&world, sand, y), before);
    assert!((2..=5).any(|x| (0..=1).any(|z| world.get_block(x, y, z) == sand)));
    assert_ne!(world.get_block(0, y + 2, 0), sand);
}

#[test]
fn walking_player_slumps_a_lip_via_sim() {
    let reg = base_reg();
    let mut world = test_world_with("walklip", reg.clone());
    let sand = b(&reg, "base:surface_sand");
    let y = 100;
    for x in -3..=9 {
        for z in -3..=3 {
            world.set_block(x, y, z, sand);
        }
    }
    for x in 1..=2 {
        for z in -3..=3 {
            world.set_block(x, y + 1, z, sand);
        }
    }
    let lip_cells = |world: &World| {
        (1..=2)
            .flat_map(|x| (-3..=3).map(move |z| (x, z)))
            .filter(|&(x, z)| world.get_meta(x, y + 1, z) == 0xff)
            .count()
    };
    let before = sand_volume(&world, sand, y);
    let lip_before = lip_cells(&world);
    let mut server = crate::server::Server::new(world, 0.3, 42);
    let mut x = 0.5;
    for _ in 0..70 {
        x += 0.1;
        server.advance(
            crate::server::TICK,
            &[crate::server::PlayerCtx {
                pos: Vec3::new(x, y as f32 + 1.0, 0.5),
                spawn: Vec3::ZERO,
                attackable: false,
                aggro_mod: 0.0,
            }],
            &mut Vec::new(),
        );
    }
    assert_eq!(sand_volume(&server.world, sand, y), before);
    assert!(lip_cells(&server.world) < lip_before);
}

#[test]
fn airborne_player_does_not_disturb_sand() {
    let reg = base_reg();
    let mut world = test_world_with("airborne", reg.clone());
    let sand = b(&reg, "base:surface_sand");
    let stone = b(&reg, "base:stone");
    let y = 100;
    for x in -1..=3 {
        for z in -1..=3 {
            world.set_block(x, y - 1, z, stone);
        }
    }
    for height in 0..3 {
        world.set_block(1, y + height, 1, sand);
    }
    let before = sand_volume(&world, sand, y);
    let touched = HashMap::from([((1, 1), 1.0)]);
    assert!(!world.disturb_sand_touched(sand, Vec3::new(1.5, (y + 5) as f32, 1.5), &touched,));
    assert_eq!(sand_volume(&world, sand, y), before);
}

#[test]
fn wake_leaves_sand_ahead_untouched() {
    let reg = base_reg();
    let mut world = test_world_with("ahead", reg.clone());
    let sand = b(&reg, "base:surface_sand");
    let y = 100;
    for x in -2..=6 {
        for z in -2..=2 {
            world.set_block(x, y, z, sand);
        }
    }
    for z in -2..=2 {
        world.set_block(2, y + 1, z, sand);
    }
    let before = sand_volume(&world, sand, y);
    let lip_before = world.get_meta(2, y + 1, 0);
    let touched = HashMap::from([((0, 0), 1.0)]);
    let feet = Vec3::new(0.5, (y + 1) as f32, 0.5);
    for _ in 0..30 {
        world.disturb_sand_touched(sand, feet, &touched);
    }
    assert_eq!(sand_volume(&world, sand, y), before);
    assert_eq!(world.get_meta(2, y + 1, 0), lip_before);
}

#[test]
fn wake_leaves_a_tall_wall_standing() {
    let reg = base_reg();
    let mut world = test_world_with("wall", reg.clone());
    let sand = b(&reg, "base:surface_sand");
    let y = 100;
    for x in -3..=3 {
        for z in -3..=3 {
            world.set_block(x, y, z, sand);
        }
    }
    for height in 1..=3 {
        for z in -1..=1 {
            world.set_block(-1, y + height, z, sand);
        }
    }
    let before = sand_volume(&world, sand, y);
    let touched = HashMap::from([((0, 0), 1.0)]);
    let feet = Vec3::new(0.5, (y + 1) as f32, 0.5);
    for _ in 0..40 {
        world.disturb_sand_touched(sand, feet, &touched);
    }
    assert_eq!(sand_volume(&world, sand, y), before);
    assert_eq!(world.get_block(-1, y + 3, 0), sand);
}

#[test]
fn sand_does_not_flow_into_water() {
    let reg = base_reg();
    let mut world = test_world_with("sandwater", reg.clone());
    let sand = b(&reg, "base:surface_sand");
    let stone = b(&reg, "base:stone");
    let water = b(&reg, "base:water");
    let y = 100;
    for x in -1..=3 {
        for z in -1..=1 {
            world.set_block(x, y - 1, z, stone);
        }
    }
    for height in 0..3 {
        world.set_block(0, y + height, 0, sand);
    }
    world.set_block(1, y, 0, water);
    let before = sand_volume(&world, sand, y);
    let touched = HashMap::from([((0, 0), 1.0)]);
    let feet = Vec3::new(0.5, (y + 3) as f32, 0.5);
    for _ in 0..40 {
        world.disturb_sand_touched(sand, feet, &touched);
    }
    assert_eq!(world.get_block(1, y, 0), water);
    assert_eq!(sand_volume(&world, sand, y), before);
}

#[test]
fn flow_never_buries_the_player() {
    let reg = base_reg();
    let mut world = test_world_with("nobury", reg.clone());
    let sand = b(&reg, "base:surface_sand");
    let y = 100;
    for x in -3..=3 {
        for z in -3..=3 {
            world.set_block(x, y, z, sand);
        }
    }
    for z in -1..=1 {
        world.set_block(-1, y + 1, z, sand);
    }
    let before = sand_volume(&world, sand, y);
    let touched = HashMap::from([((0, 0), 1.0)]);
    let feet = Vec3::new(0.5, (y + 1) as f32, 0.5);
    for _ in 0..40 {
        world.disturb_sand_touched(sand, feet, &touched);
    }
    assert_eq!(sand_volume(&world, sand, y), before);
    assert!(world.get_meta(0, y + 1, 0) == 0 || world.get_block(0, y + 1, 0) != sand);
}

#[test]
fn block_edit_fans_out_through_one_authoritative_boundary() {
    use crate::world::{BlockEntity, ChestState};

    let reg = base_reg();
    let mut w = test_world_with("edit-side-effects", reg.clone());
    let here = ChunkPos { x: 0, z: 0 };
    let west = ChunkPos { x: -1, z: 0 };
    for pos in [here, west] {
        let chunk = w.chunks_mut().get_mut(&pos).unwrap();
        chunk.dirty = false;
        chunk.modified = false;
    }

    let stone = b(&reg, "base:stone");
    w.set_edit_logging(true);
    w.set_block(0, 200, 4, stone);
    assert_eq!(w.get_block(0, 200, 4), stone);
    assert!(w.chunks()[&here].dirty && w.chunks()[&west].dirty);
    assert_eq!(w.edits().last(), Some(&(0, 200, 4, stone, 0)));

    let chest = b(&reg, "base:chest");
    let stick = it(&reg, "base:stick");
    let mut state = ChestState::default();
    state.slots[0] = Some(ItemStack::new(&reg, stick, 3));
    w.set_block(2, 200, 4, chest);
    w.insert_block_entity((2, 200, 4), BlockEntity::Chest(state));
    w.set_block(2, 200, 4, AIR);
    assert!(!w.has_block_entity(&(2, 200, 4)));
    assert!(
        w.pending_drops()
            .iter()
            .any(|(pos, stack)| { *pos == (2, 200, 4) && stack.item == stick && stack.count == 3 })
    );

    let sand = b(&reg, "base:sand");
    w.set_block(4, 202, 4, sand);
    assert_eq!(w.get_block(4, 202, 4), AIR);
    assert!(
        w.falling_blocks()
            .iter()
            .any(|falling| falling.block == sand)
    );
}

#[test]
fn removing_a_torch_leaves_no_residual_block_light() {
    let reg = base_reg();
    let mut w = test_world_with("torch_residual", reg.clone());
    let torch = b(&reg, "base:torch");
    // A chunk corner: the torch (light 14) glows into all four chunks that
    // meet here, so removing it must drain every one of them.
    let (tx, ty, tz) = (15, 100, 15);

    // Baseline block light over the region the torch can possibly touch.
    let region = |f: &mut dyn FnMut(i32, i32, i32)| {
        for x in (tx - 16)..=(tx + 16) {
            for y in (ty - 16)..=(ty + 16) {
                for z in (tz - 16)..=(tz + 16) {
                    f(x, y, z);
                }
            }
        }
    };
    let mut before: HashMap<(i32, i32, i32), [u8; 3]> = HashMap::new();
    region(&mut |x, y, z| {
        before.insert((x, y, z), w.light_rgb_at(x, y, z).0);
    });

    w.set_block(tx, ty, tz, torch);
    assert!(
        w.light_rgb_at(tx, ty, tz).0[0] > 0,
        "torch lights its own cell"
    );
    assert!(
        w.light_rgb_at(tx + 3, ty, tz + 3).0[0] > 0,
        "glow crosses the seam into the diagonal chunk"
    );

    w.set_block(tx, ty, tz, AIR);

    // Every cell must return to exactly its pre-torch block light.
    region(&mut |x, y, z| {
        let now = w.light_rgb_at(x, y, z).0;
        assert_eq!(
            now,
            before[&(x, y, z)],
            "residual block light at {x},{y},{z}: {now:?}"
        );
    });
}

#[test]
fn remote_world_neither_generates_nor_saves_authoritative_state() {
    let reg = base_reg();
    let dir = tmp_dir("remote-authority");
    let mut w = World::new(7, dir.clone(), reg);
    w.set_remote(true);

    assert!(!w.ensure_chunk(ChunkPos { x: 0, z: 0 }));
    assert!(w.chunks().is_empty());
    w.save_modified();

    assert!(!dir.join("world.toml").exists());
    assert!(!dir.join("chunks").exists());
}

#[test]
fn save_v2_roundtrip_with_palette() {
    let reg = base_reg();
    let mut w = test_world_with("v2save", reg.clone());
    let log = b(&reg, "base:log");
    w.set_block(1, 80, 1, log);
    w.set_block(-20, 33, 7, b(&reg, "base:sand"));
    w.save_modified();
    assert!(w.save_dir_for_test().join("palette").exists());

    let mut w2 = World::load_or_create(w.save_dir_for_test(), reg.clone());
    for x in -2..=2 {
        for z in -2..=2 {
            w2.ensure_chunk(ChunkPos { x, z });
        }
    }
    assert_eq!(w2.get_block(1, 80, 1), log);
    assert_eq!(w2.get_block(-20, 33, 7), b(&reg, "base:sand"));
    assert_eq!(w2.get_block(0, 0, 0), b(&reg, "base:bedrock"));
}

#[test]
fn pre_v3_saves_regenerate_cleanly() {
    let reg = base_reg();
    let dir = tmp_dir("oldsave");
    std::fs::write(dir.join("seed"), "42").unwrap();
    // A stale v2 chunk file must be ignored (regenerated), not crash.
    std::fs::write(dir.join("c.0.0.wfc"), b"WFC2garbagegarbage").unwrap();
    let mut w = World::load_or_create(dir, reg.clone());
    w.ensure_chunk(ChunkPos { x: 0, z: 0 });
    assert_eq!(w.get_block(0, 0, 0), b(&reg, "base:bedrock"));
    w.save_modified();
    // ensure_chunk on fresh terrain marks modified=false, so force a write.
    w.set_block(1, 100, 1, b(&reg, "base:planks"));
    w.save_modified();
    let bytes = std::fs::read(w.save_dir_for_test().join("c.0.0.wfc")).unwrap();
    assert!(bytes.starts_with(b"WFC4"), "saves are written as v4 now");
}

#[test]
fn palette_less_v3_chunks_keep_their_legacy_numeric_ids() {
    let reg = base_reg();
    let dir = tmp_dir("palette-less-v3");
    std::fs::write(dir.join("seed"), "42").unwrap();
    let stone = b(&reg, "base:stone");
    let mut data = Vec::new();
    data.extend_from_slice(b"WFC3");
    let total = 16 * 16 * 256usize;
    let mut left = total;
    while left > 0 {
        let run = left.min(u16::MAX as usize) as u16;
        data.extend_from_slice(&run.to_le_bytes());
        data.extend_from_slice(&stone.0.to_le_bytes());
        left -= run as usize;
    }
    std::fs::write(dir.join("c.0.0.wfc"), data).unwrap();

    let mut w = World::load_or_create(dir, reg.clone());
    w.ensure_chunk(ChunkPos { x: 0, z: 0 });
    assert_eq!(
        w.get_block(4, 60, 4),
        stone,
        "a missing palette must not turn an entire legacy chunk into the placeholder"
    );
}

#[test]
fn unknown_palette_entries_become_placeholder() {
    let reg = base_reg();
    let dir = tmp_dir("unknown");
    std::fs::write(dir.join("seed"), "42").unwrap();
    // Palette maps id 1 to a mod block that no longer exists.
    std::fs::write(dir.join("palette"), "0 base:air\n1 gonemod:ore\n").unwrap();
    let mut data = Vec::new();
    data.extend_from_slice(b"WFC3");
    let total = 16 * 16 * 256usize;
    for (count, id) in [(60u16, 0u16), (1, 1), ((total - 61) as u16, 0)] {
        data.extend_from_slice(&count.to_le_bytes());
        data.extend_from_slice(&id.to_le_bytes());
    }
    let _ = std::fs::write(dir.join("c.0.0.wfc"), data);
    let mut w = World::load_or_create(dir, reg.clone());
    w.ensure_chunk(ChunkPos { x: 0, z: 0 });
    assert_eq!(
        w.get_block(0, 60, 0),
        reg.unknown_block,
        "missing mod blocks must become the placeholder, not corrupt"
    );
    assert_eq!(w.get_block(0, 61, 0), AIR);
}

#[test]
fn all_placeholder_chunks_regenerate_instead_of_becoming_obelisks() {
    let reg = base_reg();
    let dir = tmp_dir("placeholder-obelisk");
    std::fs::write(dir.join("seed"), "42").unwrap();
    std::fs::write(
        dir.join("palette"),
        format!("{} base:unknown\n", reg.unknown_block.0),
    )
    .unwrap();
    let mut data = Vec::new();
    data.extend_from_slice(b"WFC3");
    let total = 16 * 16 * 256usize;
    let mut left = total;
    while left > 0 {
        let run = left.min(u16::MAX as usize) as u16;
        data.extend_from_slice(&run.to_le_bytes());
        data.extend_from_slice(&reg.unknown_block.0.to_le_bytes());
        left -= run as usize;
    }
    std::fs::write(dir.join("c.0.0.wfc"), data).unwrap();

    let mut w = World::load_or_create(dir, reg.clone());
    w.ensure_chunk(ChunkPos { x: 0, z: 0 });
    assert_ne!(
        w.get_block(4, 60, 4),
        reg.unknown_block,
        "a chunk poisoned by the palette-less-save bug should regenerate"
    );
}

#[test]
fn set_block_roundtrip_and_cross_chunk_access() {
    let reg = base_reg();
    let mut w = test_world_with("set", reg.clone());
    let planks = b(&reg, "base:planks");
    w.set_block(3, 70, 3, planks);
    assert_eq!(w.get_block(3, 70, 3), planks);
    w.set_block(-1, 70, -1, b(&reg, "base:cobblestone"));
    assert_eq!(w.get_block(-1, 70, -1), b(&reg, "base:cobblestone"));
}

#[test]
fn world_remaps_after_registry_change() {
    let root = tmp_dir("reloadmod");
    write_demo_mod(&root);
    let reg_with = Arc::new(registry::load(&root));
    let ore = reg_with.block_id("testium:ore").unwrap();
    let mut w = test_world_with("reload-w", reg_with.clone());
    w.set_block(2, 70, 2, ore);
    // "Remove" the mod: rebuild registry from an empty dir, remap the world.
    let reg_without = Arc::new(registry::load(Path::new("/nonexistent-mods-dir")));
    w.reg = reg_without.clone();
    w.remap_from(&reg_with);
    assert_eq!(
        w.get_block(2, 70, 2),
        reg_without.unknown_block,
        "mod block becomes placeholder after mod removal"
    );
    // Vanilla blocks survive the remap unchanged (by name).
    assert_eq!(w.get_block(0, 0, 0), b(&reg_without, "base:bedrock"));
}

#[test]
fn crops_grow_on_farmland_via_random_ticks() {
    let reg = base_reg();
    let mut w = test_world_with("crops", reg.clone());
    let h = w.surface_height(4, 4);
    let farm = b(&reg, "base:farmland");
    let seed0 = b(&reg, "base:wheat_seeds");
    // A field on farmland grows; a control column on dirt doesn't.
    for x in 0..16 {
        for z in 0..16 {
            if (x, z) == (6, 6) {
                continue;
            }
            w.set_block(x, h, z, farm);
            w.set_block(x, h + 1, z, seed0);
        }
    }
    w.set_block(6, h, 6, b(&reg, "base:dirt"));
    w.set_block(6, h + 1, 6, seed0);
    let mut rng = 12345u32;
    for _ in 0..3000 {
        w.random_tick(&mut rng);
    }
    let mut advanced = 0;
    for x in 0..16 {
        for z in 0..16 {
            if (x, z) != (6, 6) && w.get_block(x, h + 1, z) != seed0 {
                advanced += 1;
            }
        }
    }
    assert!(advanced > 0, "farmland crops should advance");
    assert_eq!(w.get_block(6, h + 1, 6), seed0, "dirt crop must not grow");
    // Stage chain terminates at ripe (stage2) with a harvest def.
    let ripe = b(&reg, "base:wheat_seeds/stage2");
    assert!(reg.block(ripe).crop_next.is_none());
    let (item, _, becomes) = reg.block(ripe).harvest.expect("ripe wheat harvests");
    assert_eq!(item, it(&reg, "base:wheat"));
    assert_eq!(becomes, seed0);
    // Bushes regrow anywhere - but only in season (summer/autumn).
    w.day = crate::world::SEASON_DAYS; // summer
    let bare = b(&reg, "base:berry_bush");
    for x in 0..16 {
        for z in 8..11 {
            w.set_block(x, h + 3, z, bare);
        }
    }
    for _ in 0..30000 {
        w.random_tick(&mut rng);
    }
    let fruited = b(&reg, "base:berry_bush/stage1");
    let refruited = (0..16)
        .flat_map(|x| (8..11).map(move |z| (x, z)))
        .filter(|&(x, z)| w.get_block(x, h + 3, z) == fruited)
        .count();
    assert!(refruited > 0, "bushes should refruit anywhere");
    // Cross rendering flags.
    assert!(reg.block(seed0).cross);
    assert!(!reg.is_solid(seed0));
}

#[test]
fn world_meta_roundtrip_and_legacy() {
    use crate::world::{read_world_meta, write_world_meta};
    let dir = tmp_dir("meta");
    write_world_meta(&dir, 777, "creative", 0.0);
    assert_eq!(
        read_world_meta(&dir),
        (Some(777), "creative".to_string(), 0.0)
    );
    // Legacy: bare seed file means survival.
    let dir2 = tmp_dir("meta2");
    std::fs::write(dir2.join("seed"), "42").unwrap();
    assert_eq!(
        read_world_meta(&dir2),
        (Some(42), "survival".to_string(), 0.0)
    );
    // load_or_create upgrades legacy worlds to world.toml.
    let reg = base_reg();
    let _ = World::load_or_create(dir2.clone(), reg);
    assert!(dir2.join("world.toml").exists());
}

#[test]
fn water_conserves_and_spreads_finite() {
    let reg = base_reg();
    assert_eq!(reg.water_volume(reg.water_block(0)), Some(8));
    assert_eq!(reg.water_volume(reg.water_block(7)), Some(1));
    assert_eq!(reg.water_for_volume(8), reg.water_block(0));
    assert_eq!(reg.water_for_volume(0), AIR);

    let mut w = test_world_with("finitewater", reg.clone());
    let h = w.surface_height(4, 4);
    let y = h + 5;
    let stone = b(&reg, "base:stone");
    for x in -8..=16 {
        for z in -8..=16 {
            w.set_block(x, y - 1, z, stone);
        }
    }
    let before = total_water(&w);
    w.set_block(4, y, 4, reg.water_block(0));
    settle_water(&mut w);
    assert_eq!(total_water(&w), before + 8, "volume neither made nor lost");
    // One cell can't stay full on open ground: it spread into a film.
    assert!(reg.water_volume(w.get_block(4, y, 4)).unwrap_or(0) < 8);
}

#[test]
fn water_equalizes_within_the_band() {
    let reg = base_reg();
    let mut w = test_world_with("equalize", reg.clone());
    let h = w.surface_height(4, 4);
    let y = h + 5;
    let stone = b(&reg, "base:stone");
    // A sealed two-cell trench.
    for x in 3..=6 {
        for z in 3..=5 {
            for yy in (y - 1)..=y {
                w.set_block(x, yy, z, stone);
            }
        }
    }
    w.set_block(4, y, 4, AIR);
    w.set_block(5, y, 4, AIR);
    w.set_block(4, y, 4, reg.water_block(0));
    settle_water(&mut w);
    let a = reg.water_volume(w.get_block(4, y, 4)).unwrap_or(0);
    let c = reg.water_volume(w.get_block(5, y, 4)).unwrap_or(0);
    assert_eq!(a + c, 8, "the trench holds all 8 units");
    assert!(a.abs_diff(c) < 2, "levels equalized: {a} vs {c}");
}

#[test]
fn breached_pond_drains_only_what_left() {
    let reg = base_reg();
    let mut w = test_world_with("breach", reg.clone());
    let h = w.surface_height(8, 8);
    let y = h + 6;
    let stone = b(&reg, "base:stone");
    // A platform, a walled basin on it, a full 3x3 pond inside.
    for x in 0..=16 {
        for z in 0..=16 {
            w.set_block(x, y - 1, z, stone);
        }
    }
    for x in 6..=10 {
        for z in 6..=10 {
            if x == 6 || x == 10 || z == 6 || z == 10 {
                w.set_block(x, y, z, stone);
            }
        }
    }
    for x in 7..=9 {
        for z in 7..=9 {
            w.set_block(x, y, z, reg.water_block(0));
        }
    }
    settle_water(&mut w);
    assert_eq!(reg.water_volume(w.get_block(8, y, 8)), Some(8));
    let total = total_water(&w);
    // Breach the rim: the pond genuinely lowers, nothing duplicates.
    w.set_block(10, y, 8, AIR);
    settle_water(&mut w);
    assert_eq!(total_water(&w), total, "no volume created by the breach");
    assert!(
        reg.water_volume(w.get_block(8, y, 8)).unwrap_or(0) < 8,
        "the pond actually dropped"
    );
    assert!(
        (11..=14).any(|x| reg.is_water(w.get_block(x, y, 8))),
        "water escaped through the breach"
    );
}

#[test]
fn random_ticks_budget_stamps_and_persist() {
    let reg = base_reg();
    let dir = tmp_dir("stamps");
    let mut w = World::new(42, dir.clone(), reg.clone());
    for x in 0..3 {
        for z in 0..3 {
            w.ensure_chunk(ChunkPos { x, z });
        }
    }
    w.clock = 100.0;
    let mut rng = 7u32;
    let burst = w.random_tick(&mut rng);
    assert_eq!(burst, 9 * 256, "long-waited chunks catch up at the cap");
    let again = w.random_tick(&mut rng);
    assert_eq!(again, 9 * 8, "freshly stamped chunks take the floor burst");
    assert_eq!(w.chunk_stamp(0, 0), Some(100.0));
    w.save_modified();
    let w2 = World::load_or_create(dir, reg.clone());
    assert_eq!(w2.chunk_stamp(0, 0), Some(100.0), "stamps persist");
}

#[test]
fn random_ticks_visit_a_bounded_cohort() {
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("cohort"), reg.clone());
    for x in 0..9 {
        for z in 0..9 {
            w.ensure_chunk(ChunkPos { x, z });
        }
    }
    // 81 chunks loaded, all stamped at clock 0; K = 64 caps the visit.
    w.clock = 5.0;
    let mut rng = 3u32;
    assert_eq!(w.random_tick(&mut rng), 64 * 80, "K chunks, elapsed-scaled");
    assert_eq!(w.random_tick(&mut rng), 17 * 80 + 47 * 8, "oldest first");
}

#[test]
fn summer_dries_shallow_water_but_not_deep() {
    let reg = base_reg();
    let mut w = test_world_with("evap", reg.clone());
    w.day = crate::world::SEASON_DAYS; // summer
    let stone = b(&reg, "base:stone");
    let h = w.surface_height(4, 4);
    let y = h + 8;
    // A walled 2x2 shallow pan, sky open.
    for x in 2..=7 {
        for z in 2..=7 {
            w.set_block(x, y - 1, z, stone);
            w.set_block(x, y, z, stone);
        }
    }
    for (x, z) in [(4, 4), (5, 4), (4, 5), (5, 5)] {
        w.set_block(x, y, z, reg.water_block(0));
    }
    // A deep walled shaft: three stacked cells, sky open.
    for x in 10..=12 {
        for z in 3..=5 {
            for yy in (y - 3)..=y {
                w.set_block(x, yy, z, stone);
            }
        }
    }
    for yy in (y - 2)..=y {
        w.set_block(11, yy, 4, reg.water_block(0));
    }
    // An open spill: a lone film on flat ground.
    w.set_block(9, y - 1, 9, stone);
    w.set_block(9, y, 9, reg.water_for_volume(1));
    let mut rng = 11u32;
    for _ in 0..40_000 {
        w.random_tick(&mut rng);
        w.tick_water(1_000);
    }
    for (x, z) in [(4, 4), (5, 4), (4, 5), (5, 5)] {
        assert_eq!(
            reg.water_volume(w.get_block(x, y, z)),
            Some(1),
            "pan cell ({x},{z}) dried to a marshy film"
        );
    }
    assert_eq!(
        reg.water_volume(w.get_block(11, y, 4)),
        Some(8),
        "deep water is off the stove"
    );
    assert_eq!(w.get_block(9, y, 9), AIR, "open spills dry entirely");
}

#[test]
fn rain_refills_surface_water() {
    let reg = base_reg();
    let mut w = test_world_with("rain", reg.clone());
    w.day = crate::world::SEASON_DAYS; // summer: temperate columns rain
    let stone = b(&reg, "base:stone");
    let h = w.surface_height(4, 4);
    let y = h + 8;
    w.set_block(4, y - 1, 4, stone);
    for (x, z) in [(3, 4), (5, 4), (4, 3), (4, 5)] {
        w.set_block(x, y, z, stone);
    }
    w.set_block(4, y, 4, reg.water_for_volume(2));
    for _ in 0..12 {
        w.rain_fill(4, 4);
    }
    assert_eq!(
        reg.water_volume(w.get_block(4, y, 4)),
        Some(8),
        "rain topped the cell back up to full"
    );
}

#[test]
fn reconcile_catches_up_an_absent_chunk() {
    let reg = base_reg();
    let dir = tmp_dir("reconcile");
    let mut w = World::new(42, dir.clone(), reg.clone());
    for x in -1..=1 {
        for z in -1..=1 {
            w.ensure_chunk(ChunkPos { x, z });
        }
    }
    let b = |n: &str| reg.block_id(n).unwrap();
    let h = w.surface_height(4, 4);
    // A supported sky-open pool (the shelf the live winter test uses)
    // and a farmland strip about to miss three growing seasons.
    for x in 0..8 {
        w.set_block(x, h + 12, 12, b("base:planks"));
        w.set_block(x, h + 13, 12, reg.water_block(0));
        w.set_block(x, h + 6, 4, b("base:farmland"));
        w.set_block(x, h + 7, 4, b("base:wheat_seeds"));
    }
    w.save_modified();

    // Reopen the world a year later, in deep winter.
    let mut w2 = World::load_or_create(dir, reg.clone());
    w2.day = 3 * crate::world::SEASON_DAYS;
    w2.clock = w2.day as f64 * 600.0;
    for x in -1..=1 {
        for z in -1..=1 {
            w2.ensure_chunk(ChunkPos { x, z });
        }
    }
    let iced = (0..8)
        .filter(|&x| w2.get_block(x, h + 13, 12) == b("base:ice"))
        .count();
    assert!(iced >= 6, "the pool froze while you were away ({iced}/8)");
    let grown = (0..8)
        .filter(|&x| w2.get_block(x, h + 7, 4) != b("base:wheat_seeds"))
        .count();
    assert!(grown > 0, "crops advanced over the missed seasons");
}

#[test]
fn water_defers_at_the_worlds_edge() {
    let reg = base_reg();
    let mut w = World::new(42, tmp_dir("borderwater"), reg.clone());
    w.ensure_chunk(ChunkPos { x: 0, z: 0 });
    let stone = b(&reg, "base:stone");
    let y = 250;
    // A shelf against the +x seam, walled on every loaded side.
    w.set_block(15, y - 1, 4, stone);
    w.set_block(14, y, 4, stone);
    w.set_block(15, y, 3, stone);
    w.set_block(15, y, 5, stone);
    w.set_block(15, y, 4, reg.water_block(0));
    for _ in 0..50 {
        w.tick_water(10_000);
    }
    assert_eq!(
        reg.water_volume(w.get_block(15, y, 4)),
        Some(8),
        "water waits at the ungenerated seam instead of vanishing"
    );
    // The neighbor generates: the seam wakes and the flow resumes.
    w.ensure_chunk(ChunkPos { x: 1, z: 0 });
    let t1 = total_water(&w);
    settle_water(&mut w);
    assert_eq!(total_water(&w), t1, "crossing the seam conserved volume");
    assert!(
        reg.water_volume(w.get_block(15, y, 4)).unwrap_or(0) < 8,
        "the seam wake resumed the flow"
    );
}

#[test]
fn world_listing_sees_world_toml_and_legacy_seed() {
    // Regression: the title list only read the legacy `seed` file, so
    // world.toml worlds were invisible and their folder names got reused
    // by NEW WORLD — inheriting the old player.toml (inventory carryover).
    let root = tmp_dir("listworlds");
    crate::world::write_world_meta(&root.join("world1"), 42, "survival", 0.0);
    std::fs::create_dir_all(root.join("old")).unwrap();
    std::fs::write(root.join("old/seed"), "7").unwrap();
    std::fs::create_dir_all(root.join("junk")).unwrap();
    std::fs::write(root.join("stray.txt"), "x").unwrap();
    let worlds = crate::world::list_worlds(&root);
    assert_eq!(
        worlds,
        vec![("old".to_string(), 7), ("world1".to_string(), 42)],
        "world.toml and legacy worlds both list; junk doesn't"
    );
}

#[test]
fn new_world_name_never_reuses_existing_folder() {
    let root = tmp_dir("nextworld");
    crate::world::write_world_meta(&root.join("world1"), 1, "survival", 0.0);
    std::fs::write(root.join("world1/player.toml"), "leftover inventory").unwrap();
    let listed = crate::world::list_worlds(&root);
    assert_eq!(crate::next_world_name(&root, &listed), "world2");
    // Even a folder the listing can't see must not be adopted as "new".
    assert_eq!(crate::next_world_name(&root, &[]), "world2");
    assert_eq!(
        crate::next_world_name(&tmp_dir("nextworld-empty"), &[]),
        "world1"
    );
}

#[test]
fn torch_light_propagates_and_walls_block_it() {
    let reg = base_reg();
    let mut w = test_world("lighttorch");
    let stone = reg.block_id("base:stone").unwrap();
    let torch = reg.block_id("base:torch").unwrap();
    // Sealed 9x9x9 stone box, hollow interior, well above terrain.
    for x in 0..9 {
        for z in 0..9 {
            for y in 150..159 {
                let shell = x == 0 || x == 8 || z == 0 || z == 8 || y == 150 || y == 158;
                w.set_block(x, y, z, if shell { stone } else { AIR });
            }
        }
    }
    assert_eq!(w.light_at(4, 154, 4), (0, 0), "sealed box is pitch black");
    w.set_block(4, 151, 4, torch);
    assert_eq!(w.light_at(4, 151, 4).0, 14, "torch emits 14");
    assert_eq!(w.light_at(5, 151, 4).0, 13, "one step dims by one");
    assert_eq!(w.light_at(7, 151, 4).0, 11, "three steps");
    assert_eq!(w.light_at(4, 153, 4).0, 12, "propagates vertically too");
    assert_eq!(w.light_at(10, 151, 4).0, 0, "opaque wall stops it");
    w.set_block(4, 151, 4, AIR);
    assert_eq!(
        w.light_at(5, 151, 4).0,
        0,
        "removing the torch relights dark"
    );
}

#[test]
fn sky_light_surface_cave_and_roof_opening() {
    let reg = base_reg();
    let mut w = test_world("lightsky");
    let stone = reg.block_id("base:stone").unwrap();
    // Open surface reads full sky.
    let y = w.surface_height(2, 2);
    assert_eq!(w.light_at(2, y + 1, 2).1, 15, "surface is full daylight");
    // Sealed box: no sky inside; opening the roof floods it.
    for x in 20..29 {
        for z in 20..29 {
            for yy in 150..159 {
                let shell = x == 20 || x == 28 || z == 20 || z == 28 || yy == 150 || yy == 158;
                w.set_block(x, yy, z, if shell { stone } else { AIR });
            }
        }
    }
    assert_eq!(w.light_at(24, 154, 24).1, 0, "sealed roof blocks sky");
    w.set_block(24, 158, 24, AIR); // skylight hole
    assert_eq!(
        w.light_at(24, 154, 24).1,
        15,
        "column under the hole is lit"
    );
    assert_eq!(
        w.light_at(26, 154, 24).1,
        13,
        "and floods sideways, dimming"
    );
}

#[test]
fn light_crosses_chunk_borders() {
    let reg = base_reg();
    let mut w = test_world("lightseam");
    let torch = reg.block_id("base:torch").unwrap();
    // Torch on the last column of chunk (0,0); the neighbor chunk must see it.
    w.set_block(15, 200, 8, torch);
    assert_eq!(w.light_at(15, 200, 8).0, 14);
    assert_eq!(w.light_at(16, 200, 8).0, 13, "crosses the seam");
    assert_eq!(w.light_at(19, 200, 8).0, 10, "keeps dimming next door");
}

#[test]
fn water_dims_sky_and_mod_blocks_can_glow() {
    let reg = base_reg();
    let mut w = test_world("lightwater");
    let water = reg.water_ids[0];
    let stone = reg.block_id("base:stone").unwrap();
    // A water-filled shaft walled in stone: light only enters from above,
    // dimming one level per water block.
    for x in 2..7 {
        for z in 2..7 {
            for y in 179..183 {
                w.set_block(x, y, z, stone);
            }
        }
    }
    for y in 180..183 {
        w.set_block(4, y, 4, water);
    }
    assert_eq!(w.light_at(4, 182, 4).1, 14, "first water block dims to 14");
    assert_eq!(w.light_at(4, 180, 4).1, 12, "third dims to 12");

    // Mod block with light = 9.
    let root = tmp_dir("glowmod");
    let dir = root.join("glow");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mod.toml"), "id = \"glow\"\n").unwrap();
    std::fs::write(
        dir.join("blocks.toml"),
        "[[block]]\nid = \"lamp\"\ntexture = \"@stone\"\nlight = 9\n",
    )
    .unwrap();
    let reg2 = Arc::new(registry::load(&root));
    let lamp = reg2.block_id("glow:lamp").unwrap();
    assert_eq!(reg2.block(lamp).light_emit, 9);
    let mut w2 = test_world_with("lightmod", reg2);
    w2.set_block(4, 200, 4, lamp);
    assert_eq!(w2.light_at(4, 200, 4).0, 9, "emitter itself");
    assert_eq!(w2.light_at(4, 202, 4).0, 7, "two steps out");
}

#[test]
fn placing_a_roof_casts_shadow() {
    let reg = base_reg();
    let mut w = test_world("lightshadow");
    let stone = reg.block_id("base:stone").unwrap();
    let y = w.surface_height(8, 8);
    assert_eq!(w.light_at(8, y + 1, 8).1, 15);
    w.set_block(8, y + 3, 8, stone); // roof two above the ground cell
    let shaded = w.light_at(8, y + 1, 8).1;
    assert!(shaded < 15, "column shadowed, got {shaded}");
    assert!(shaded >= 12, "but side-lit by flood, got {shaded}");
}

#[test]
fn relight_perf_sane() {
    let mut w = test_world("lightperf");
    let t0 = std::time::Instant::now();
    for _ in 0..10 {
        w.relight_and_cascade(ChunkPos { x: 0, z: 0 });
    }
    let per = t0.elapsed().as_secs_f32() / 10.0;
    assert!(per < 0.05, "relight cascade averaged {per:.4}s");
}

#[test]
fn torch_needs_ground_and_pops_without_it() {
    let reg = base_reg();
    let mut w = test_world("torchpop");
    let stone = reg.block_id("base:stone").unwrap();
    let torch = reg.block_id("base:torch").unwrap();
    w.set_block(5, 150, 5, stone);
    w.set_block(5, 151, 5, torch);
    assert_eq!(w.get_block(5, 151, 5), torch);
    // Mining the support pops the torch off as a drop.
    w.set_block(5, 150, 5, AIR);
    assert_eq!(w.get_block(5, 151, 5), AIR, "torch popped");
    assert!(
        w.pending_drops()
            .iter()
            .any(|(_, s)| reg.item(s.item).name == "base:torch"),
        "torch dropped as an item"
    );
    // Torch recipe: charcoal over stick -> 4.
    let mut g = vec![None; 9];
    g[0] = Some(ItemStack::new(&reg, it(&reg, "base:charcoal"), 1));
    g[3] = Some(ItemStack::new(&reg, it(&reg, "base:stick"), 1));
    let r = crate::crafting::match_recipe(&reg, &g, 3).expect("torch recipe");
    assert_eq!(r.output, it(&reg, "base:torch"));
    assert_eq!(r.count, 4);
}

#[test]
fn ire_gains_decay_tiers_and_persistence() {
    let reg = base_reg();
    let dir = tmp_dir("iresave");
    let mut w = World::new(3, dir.clone(), reg.clone());
    // Block classes.
    assert_eq!(w.ire_for_block(reg.block_id("base:log").unwrap()), 0.3);
    assert_eq!(
        w.ire_for_block(reg.block_id("base:copper_ore").unwrap()),
        0.4
    );
    assert_eq!(w.ire_for_block(reg.block_id("base:stone").unwrap()), 0.05);
    assert_eq!(w.ire_for_block(reg.block_id("base:planks").unwrap()), 0.0);
    // Tier thresholds.
    w.add_ire(30.0);
    assert_eq!(w.ire_tier(), 1, "uneasy");
    w.add_ire(60.0);
    assert_eq!(w.ire_tier(), 3, "wrathful");
    w.add_ire(500.0);
    assert_eq!(w.ire, 100.0, "clamped");
    // Decay: -4 per day.
    w.tick_ire(0.5);
    assert!((w.ire - 98.0).abs() < 0.01);
    // Planting refunds, capped at 8/day.
    for _ in 0..100 {
        w.plant_ire(0.5);
    }
    assert!((w.ire - 90.0).abs() < 0.01, "daily cap of 8, got {}", w.ire);
    w.tick_ire(0.6); // day rolls over -> cap resets
    w.plant_ire(0.5);
    assert!(w.ire < 90.0 - 2.0, "cap reset next day");
    // Persistence via world.toml.
    w.save_modified();
    let w2 = World::load_or_create(dir, reg);
    assert!((w2.ire - w.ire).abs() < 0.01, "ire round-trips");
}

#[test]
fn saplings_parse_drop_and_grow() {
    let reg = base_reg();
    // Leaves carry sapling bonus drops of their own species.
    let oak_leaves = reg.block(reg.block_id("base:leaves").unwrap());
    let (bd_item, ch) = oak_leaves.bonus_drop.expect("leaves drop saplings");
    assert_eq!(reg.item(bd_item).name, "base:oak_sapling");
    assert!((ch - 0.1).abs() < 0.001);
    let spruce = reg.block(reg.block_id("base:spruce_leaves").unwrap());
    assert_eq!(
        reg.item(spruce.bonus_drop.unwrap().0).name,
        "base:spruce_sapling"
    );
    // Grow: sapling on dirt in open air becomes a real tree.
    let mut w = test_world("sapgrow");
    let dirt = reg.block_id("base:dirt").unwrap();
    let sap = reg.block_id("base:oak_sapling").unwrap();
    w.set_block(4, 199, 4, dirt);
    w.set_block(4, 200, 4, sap);
    let ire0 = {
        w.ire = 50.0;
        w.ire
    };
    assert!(w.try_grow_sapling(4, 200, 4, 7), "clear sky: grows");
    let log = reg.block_id("base:log").unwrap();
    assert_eq!(w.get_block(4, 200, 4), log, "trunk replaces the sapling");
    let leaves = reg.block_id("base:leaves").unwrap();
    let mut leaf_count = 0;
    for x in 0..9 {
        for z in 0..9 {
            for y in 200..212 {
                if w.get_block(x, y, z) == leaves {
                    leaf_count += 1;
                }
            }
        }
    }
    assert!(leaf_count > 8, "canopy grew ({leaf_count} leaves)");
    assert!(
        (w.ire - (ire0 - 2.0)).abs() < 0.01,
        "maturation refunds 2 ire"
    );
    // Blocked trunk: stays a sapling.
    let stone = reg.block_id("base:stone").unwrap();
    w.set_block(8, 199, 8, dirt);
    w.set_block(8, 200, 8, sap);
    w.set_block(8, 202, 8, stone);
    assert!(!w.try_grow_sapling(8, 200, 8, 7), "blocked: stays");
    assert_eq!(w.get_block(8, 200, 8), sap);
}

#[test]
fn offering_stone_values_and_dawn() {
    let reg = base_reg();
    let mut w = test_world("offer");
    let stone = reg.block_id("base:offering_stone").unwrap();
    assert_eq!(reg.block(stone).interaction.as_deref(), Some("offering"));
    assert_eq!(reg.block(stone).light_emit, 5, "faint wildlight");
    assert!(!reg.recipes_for(it(&reg, "base:offering_stone")).is_empty());
    // Value table: the wild's own materials 2.0, meat 1.0, bread hunger*0.25.
    let v = |name: &str, n: u32| w.offering_value(&ItemStack::new(&reg, it(&reg, name), n));
    assert_eq!(v("base:heartwood", 1), 2.0);
    assert_eq!(v("base:raw_venison", 2), 2.0);
    assert!(
        (v("base:bread", 1) - 1.5).abs() < 0.01,
        "bread hunger 6 * 0.25"
    );
    assert_eq!(v("base:oak_sapling", 1), 1.0);
    // Dawn: items taken, refund capped at 10.
    w.ire = 60.0;
    let mut st = crate::world::OfferingState::default();
    st.slots[0] = Some(ItemStack::new(&reg, it(&reg, "base:heartwood"), 4)); // 8.0
    st.slots[1] = Some(ItemStack::new(&reg, it(&reg, "base:raw_rabbit"), 5)); // 5.0
    w.insert_block_entity((3, 90, 3), crate::world::BlockEntity::Offering(st));
    let r = w.accept_offerings();
    assert!((r - 10.0).abs() < 0.01, "capped at 10, got {r}");
    assert!((w.ire - 50.0).abs() < 0.01);
    let Some(crate::world::BlockEntity::Offering(o)) = w.block_entity(&(3, 90, 3)) else {
        panic!()
    };
    assert!(
        o.slots.iter().all(|s| s.is_none()),
        "the wild took everything"
    );
    assert_eq!(w.accept_offerings(), 0.0, "empty stone gives nothing");
}

#[test]
fn server_ticks_at_fixed_rate_and_runs_the_world() {
    let reg = base_reg();
    let world = test_world("simsplit");
    let mut sv = crate::server::Server::new(world, 0.3, 42);
    let ctx = crate::server::PlayerCtx {
        pos: Vec3::new(8.0, 80.0, 8.0),
        spawn: Vec3::new(-500.0, 70.0, -500.0),
        attackable: true,
        aggro_mod: 0.0,
    };
    let t0 = sv.time_of_day;
    let mut evs = Vec::new();
    // 2 wall-seconds in odd chunks: the fixed tick must absorb it evenly.
    for _ in 0..120 {
        sv.advance(1.0 / 60.0, &[ctx], &mut evs);
    }
    let advanced = sv.time_of_day - t0;
    assert!(
        (advanced - 2.0 / crate::server::DAY_LENGTH).abs() < 0.0005,
        "clock advanced by the simulated time, got {advanced}"
    );
    // A hitch doesn't spiral the simulation.
    sv.advance(30.0, &[ctx], &mut evs);
    assert!(sv.time_of_day - t0 < 0.01, "hitch capped, not replayed");
    // Ire tier events flow through the server.
    sv.world.ire = 95.0;
    let mut evs2 = Vec::new();
    sv.advance(0.1, &[ctx], &mut evs2);
    assert!(
        evs2.iter()
            .any(|e| matches!(e, crate::server::SimEvent::IreTier { rose: true, .. })),
        "tier change surfaced as a SimEvent"
    );
    let _ = reg;
}

#[test]
fn weather_machine_rolls_legal_fronts_and_storms_lean_on_ire() {
    use crate::world::Weather;
    let reg = base_reg();
    let count_storms = |ire: f32, name: &str| -> (u32, bool) {
        let mut w = World::new(42, tmp_dir(name), reg.clone());
        w.ire = ire;
        let mut sim = crate::server::Server::new(w, 0.3, 5);
        let mut storms = 0;
        let mut legal = true;
        let mut prev = sim.world.weather;
        let mut events = Vec::new();
        for _ in 0..40_000 {
            sim.advance(1.0, &[], &mut events);
            sim.world.ire = ire; // hold it steady against decay
            for e in events.drain(..) {
                if let crate::server::SimEvent::WeatherChanged(next) = e {
                    assert_eq!(next, sim.world.weather, "event carries the new front");
                    legal &= match prev {
                        Weather::Clear => next == Weather::Overcast,
                        Weather::Overcast => next != Weather::Overcast,
                        Weather::Precip => next == Weather::Clear,
                        Weather::Storm => next == Weather::Overcast,
                    };
                    if next == Weather::Storm {
                        storms += 1;
                    }
                    prev = next;
                }
            }
        }
        (storms, legal)
    };
    let (calm_storms, calm_legal) = count_storms(0.0, "wx-calm");
    let (wrath_storms, wrath_legal) = count_storms(100.0, "wx-wrath");
    assert!(calm_legal && wrath_legal, "only legal transitions");
    assert!(
        wrath_storms > calm_storms,
        "storms lean on ire: {wrath_storms} vs {calm_storms}"
    );

    // The day advances when the clock wraps, and when the camp sleeps.
    let w = World::new(42, tmp_dir("wx-day"), reg.clone());
    let mut sim = crate::server::Server::new(w, 0.999, 5);
    let mut ev = Vec::new();
    for _ in 0..40 {
        sim.advance(0.1, &[], &mut ev); // the hitch cap swallows big steps
    }
    assert_eq!(sim.world.day, 1, "midnight rolls the calendar");
    sim.sleep_to_dawn();
    assert_eq!(sim.world.day, 2, "sleeping skips into tomorrow");
    assert!(sim.world.weather_timer <= 0.0, "the front re-rolls at dawn");

    // Calendar persistence rides world.toml.
    let dir = tmp_dir("wx-persist");
    let mut w = World::new(42, dir.clone(), reg.clone());
    w.day = 23;
    w.weather = Weather::Storm;
    w.save_modified();
    let w2 = World::load_or_create(dir, reg);
    assert_eq!(w2.day, 23);
    assert_eq!(w2.weather, Weather::Storm);
    assert_eq!(w2.season(), 1, "day 23 is summer");
}

#[test]
fn winter_gates_growth_and_freezes_exposed_water() {
    let reg = base_reg();
    let mut w = test_world_with("wx-winter", reg.clone());
    w.day = 3 * crate::world::SEASON_DAYS; // deep winter
    let b = |n: &str| reg.block_id(n).unwrap();
    let h = w.surface_height(4, 4);

    // A strip of sky-open wheat on farmland never advances in winter...
    for x in 0..16 {
        w.set_block(x, h + 6, 4, b("base:farmland"));
        w.set_block(x, h + 7, 4, b("base:wheat_seeds"));
    }
    // ...while a roofed, torchlit one still creeps (the greenhouse).
    for x in 0..16 {
        w.set_block(x, h + 6, 8, b("base:farmland"));
        w.set_block(x, h + 7, 8, b("base:wheat_seeds"));
        w.set_block(x, h + 9, 8, b("base:planks"));
        if x % 3 == 0 {
            w.set_block(x, h + 7, 9, b("base:torch"));
        }
    }
    let mut rng = 7u32;
    for _ in 0..30_000 {
        w.random_tick(&mut rng);
    }
    let open_grown = (0..16)
        .filter(|&x| w.get_block(x, h + 7, 4) != b("base:wheat_seeds"))
        .count();
    let roofed_grown = (0..16)
        .filter(|&x| {
            let g = w.get_block(x, h + 7, 8);
            g != b("base:wheat_seeds") && g != AIR
        })
        .count();
    assert_eq!(open_grown, 0, "winter halts sky-open crops");
    assert!(
        roofed_grown > 0,
        "roof + torchlight keeps a greenhouse alive"
    );

    // Exposed still water freezes over in winter...
    for x in 0..8 {
        w.set_block(x, h + 12, 12, b("base:planks"));
        w.set_block(x, h + 13, 12, reg.water_block(0));
    }
    // (support keeps it a still pool; sky above is open)
    for _ in 0..30_000 {
        w.random_tick(&mut rng);
    }
    let iced = (0..8)
        .filter(|&x| w.get_block(x, h + 13, 12) == b("base:ice"))
        .count();
    assert!(iced > 0, "winter freezes exposed pools, froze {iced}");

    // ...and spring gives them back.
    w.day = 0;
    for _ in 0..30_000 {
        w.random_tick(&mut rng);
    }
    let thawed = (0..8)
        .filter(|&x| w.get_block(x, h + 13, 12) == reg.water_block(0))
        .count();
    assert!(thawed > 0, "spring thaws the ice, thawed {thawed}");
}

#[test]
fn snow_settles_melts_and_snowballs_fly() {
    use glam::Vec3;
    let reg = base_reg();
    let mut w = test_world_with("wx-snow", reg.clone());
    let b = |n: &str| reg.block_id(n).unwrap();
    let layer = b("base:snow_layer");
    assert_eq!(
        reg.block(layer).height,
        Some(0.125),
        "snow layers render thin"
    );

    // Snowfall settles one layer on a cold, sky-open column - once.
    w.day = 3 * crate::world::SEASON_DAYS; // winter relaxes the snow line
    // Find LAND columns (not frozen ocean) in each climate.
    let find = |w: &mut World, lo: f32, hi: f32| -> Option<(i32, i32)> {
        for x in (-400..400).step_by(16) {
            for z in (-400..400).step_by(16) {
                let t = w.generator.climate(x, z).t;
                if t < lo || t > hi {
                    continue;
                }
                w.ensure_chunk(ChunkPos::of_world(x, z));
                let y = w.surface_height(x, z);
                if y > SEA_LEVEL + 1 && w.get_block(x, y + 1, z) == AIR {
                    return Some((x, z));
                }
            }
        }
        None
    };
    let (cx, cz) = find(&mut w, -1.0, -0.15).expect("cold land in range");
    let (wx, wz) = find(&mut w, 0.0, 0.3).expect("temperate land in range");
    let cy = w.surface_height(cx, cz);
    w.settle_snow(cx, cz);
    assert_eq!(
        w.get_block(cx, cy + 1, cz),
        layer,
        "snow settled on the cold column"
    );
    w.settle_snow(cx, cz);
    assert_eq!(w.get_block(cx, cy + 2, cz), AIR, "layers never stack");
    let wy = w.surface_height(wx, wz);
    w.settle_snow(wx, wz);
    assert_ne!(
        w.get_block(wx, wy + 1, wz),
        layer,
        "temperate columns shrug it off"
    );

    // Torchlight melts layers even in an arctic winter.
    w.set_block(cx + 1, cy + 1, cz, b("base:torch"));
    let mut rng = 9u32;
    for _ in 0..30_000 {
        w.random_tick(&mut rng);
        if w.get_block(cx, cy + 1, cz) == AIR {
            break;
        }
    }
    assert_eq!(w.get_block(cx, cy + 1, cz), AIR, "bright light clears snow");

    // Breaking a snow block yields snowballs; the crafting loop closes.
    assert_eq!(
        reg.block(b("base:snow")).drops,
        Some((reg.item_id("base:snowball").unwrap(), 4))
    );
    let ball = reg.item_id("base:snowball").unwrap();
    assert_eq!(
        reg.item(ball).throw_speed,
        Some(18.0),
        "snowballs are throwable"
    );
    let grid = [Some(ItemStack::new(&reg, ball, 1)); 4];
    let r = crate::crafting::match_recipe(&reg, &grid, 2).expect("4 snowballs pack a block");
    assert_eq!(r.output, reg.item_id("base:snow").unwrap());

    // A zero-damage projectile still shoves: snowball knockback.
    // Staged high in open sky so terrain can't intercept the shot.
    let sy = 140.0;
    let wild = reg.animals.iter().position(|a| !a.hostile).unwrap();
    let mi = w.mob_count();
    let mut m = crate::mobs::Mob::new(wild, Vec3::new(4.5, sy, 4.5), 0.0);
    m.health = 10.0;
    w.spawn_mob(m);
    w.spawn_projectile(crate::mobs::Projectile {
        pos: Vec3::new(4.5, sy + 0.4, 3.0),
        vel: Vec3::new(0.0, 0.0, 12.0),
        tile: 0,
        damage: 0.0,
        age: 0.0,
        from_player: true,
        drop_item: None,
        owner: 0,
    });
    for _ in 0..60 {
        w.tick_projectiles(&[], 1.0 / 30.0);
    }
    assert_eq!(w.mobs()[mi].health, 10.0, "a snowball draws no blood");
    assert!(
        w.mobs()[mi].hurt_flash > 0.0 || w.mobs()[mi].vel.length() > 0.1,
        "but it definitely lands"
    );

    // Removing a layer's support pops it as a drop.
    let py = w.surface_height(10, 10);
    w.set_block(10, py + 2, 10, b("base:planks"));
    w.set_block(10, py + 3, 10, layer);
    w.clear_pending_drops();
    w.set_block(10, py + 2, 10, AIR);
    assert_eq!(
        w.get_block(10, py + 3, 10),
        AIR,
        "unsupported layers fall away"
    );
    assert!(
        w.pending_drops().iter().any(|(_, s)| s.item == ball),
        "and hand back their snowball"
    );
}

#[test]
fn weather_and_season_touch_the_sim() {
    use crate::world::Weather;
    let reg = base_reg();
    // Rain speeds ire decay.
    let mut w = World::new(42, tmp_dir("wx-ire"), reg.clone());
    w.ire = 50.0;
    w.weather = Weather::Clear;
    w.tick_ire(0.5);
    let dry = w.ire;
    let mut w2 = World::new(42, tmp_dir("wx-ire2"), reg.clone());
    w2.ire = 50.0;
    w2.weather = Weather::Precip;
    w2.tick_ire(0.5);
    assert!(w2.ire < dry, "the land drinks: {} < {dry}", w2.ire);

    // Winter pauses breeding even for fed adults side by side.
    let mut w = test_world_with("wx-breed", reg.clone());
    w.day = 3 * crate::world::SEASON_DAYS;
    let wild = reg
        .animals
        .iter()
        .position(|a| !a.hostile && a.breed_food.is_some())
        .expect("breedable wildlife");
    let y = w.surface_height(4, 4) as f32 + 1.05;
    let before = w.mob_count();
    for dx in 0..2 {
        let mut m = crate::mobs::Mob::new(wild, glam::Vec3::new(4.5 + dx as f32, y, 4.5), 0.0);
        m.health = 10.0;
        m.fed = true;
        w.spawn_mob(m);
    }
    let mut rng = 3u32;
    for _ in 0..120 {
        w.tick_mobs(&[], 1.0, 1.0 / 30.0, &mut rng);
    }
    assert!(
        w.mobs().iter().all(|m| m.growth >= 1.0),
        "no winter litters"
    );
    assert!(w.mob_count() <= before + 2, "no winter births");
    // Summer: the same pair bears young.
    w.day = crate::world::SEASON_DAYS;
    for m in w.mobs_mut() {
        m.fed = true;
        m.breed_cd = 0.0;
    }
    let before = w.mob_count();
    for _ in 0..120 {
        w.tick_mobs(&[], 1.0, 1.0 / 30.0, &mut rng);
    }
    assert!(w.mob_count() > before, "summer births arrive");
}

#[test]
fn stained_glass_filters_torchlight_by_channel() {
    let reg = base_reg();
    let mut w = test_world_with("gw-stain", reg.clone());
    let b = |n: &str| reg.block_id(n).unwrap();
    let my = 120;
    // A sealed corridor: torch | red glass | probe cell.
    let stone = b("base:stone");
    for x in 8..15 {
        for y in my - 1..my + 3 {
            for z in 8..12 {
                w.set_block(x, y, z, stone);
            }
        }
    }
    for x in 9..14 {
        w.set_block(x, my, 10, AIR);
        w.set_block(x, my + 1, 10, AIR);
    }
    w.set_block(9, my, 10, b("base:torch"));
    w.set_block(11, my, 10, b("base:red_glass"));
    w.set_block(11, my + 1, 10, b("base:red_glass"));
    let (rgb, _) = w.light_rgb_at(13, my, 10);
    assert!(rgb[0] > 0, "red passes red glass: {rgb:?}");
    assert_eq!(rgb[1], 0, "green dies at red glass: {rgb:?}");
    assert_eq!(rgb[2], 0, "blue dies at red glass: {rgb:?}");
    // Clear glass passes everything.
    w.set_block(11, my, 10, b("base:glass"));
    w.set_block(11, my + 1, 10, b("base:glass"));
    let (rgb, _) = w.light_rgb_at(13, my, 10);
    // A torch burns warm: blue is already spent at this range, so the
    // proof is red and green surviving where red glass killed green.
    assert!(
        rgb[0] > 0 && rgb[1] > 0,
        "clear passes the torch's warmth: {rgb:?}"
    );
}

#[test]
fn bedrock_floor_is_unbreakable_and_reseals_on_load() {
    let reg = base_reg();
    let root = reg.block_id("base:bedrock").expect("bedrock registered");
    let dir = tmp_dir("floor");
    let mut w = World::new(42, dir.clone(), reg.clone());
    w.ensure_chunk(ChunkPos { x: 0, z: 0 });
    // Every column of a fresh chunk is floored.
    for x in 0..16 {
        for z in 0..16 {
            assert_eq!(w.get_block(x, 0, z), root, "floor at ({x},0,{z})");
        }
    }
    // No tool gives it a hardness: it cannot be mined.
    let pick = reg.item_id("base:wood_pickaxe");
    assert!(
        reg.effective_hardness(root, pick).is_none(),
        "bedrock unbreakable"
    );
    // A hole knocked in the floor (a creative dig, an old bug) heals
    // when the chunk loads again.
    w.set_block(4, 0, 4, AIR);
    w.save_modified();
    drop(w);
    let mut w2 = World::new(42, dir, reg);
    w2.ensure_chunk(ChunkPos { x: 0, z: 0 });
    assert_eq!(w2.get_block(4, 0, 4), root, "floor resealed on load");
}

#[test]
fn snow_trod_swaps_persists_melts_and_drops() {
    let reg = base_reg();
    let mut w = test_world_with("snow-trod", reg.clone());
    let layer = b(&reg, "base:snow_layer");
    let trod = b(&reg, "base:snow_layer_trod");
    let dirt = b(&reg, "base:dirt");
    let (x, y, z) = (3, 90, 3);
    w.set_block(x, y, z, dirt);
    w.set_block(x, y + 1, z, layer);

    // Walking through presses the layer into a print; treading again
    // (or treading air/dirt) changes nothing.
    w.tread(x, y + 1, z);
    assert_eq!(w.get_block(x, y + 1, z), trod, "layer pressed to trod");
    w.tread(x, y + 1, z);
    assert_eq!(w.get_block(x, y + 1, z), trod, "idempotent");
    w.tread(x, y + 5, z);
    assert_eq!(w.get_block(x, y + 5, z), AIR, "air stays air");

    // Same shovel yield as fresh snow — the content graph is unmoved.
    assert_eq!(
        reg.drops_for(trod, None),
        reg.drops_for(layer, None),
        "trodden snow drops the same snowball"
    );

    // The trail persists across save/load.
    w.save_modified();
    let mut w2 = World::load_or_create(w.save_dir_for_test(), reg.clone());
    w2.ensure_chunk(ChunkPos::of_world(x, z));
    assert_eq!(w2.get_block(x, y + 1, z), trod, "footprints persist");

    // And melts by the same rule as the untouched layer: torchlight.
    w2.set_block(x + 1, y + 1, z, b(&reg, "base:torch"));
    let mut rng = 5u32;
    for _ in 0..30_000 {
        w2.random_tick(&mut rng);
        if w2.get_block(x, y + 1, z) == AIR {
            break;
        }
    }
    assert_eq!(w2.get_block(x, y + 1, z), AIR, "prints melt like snow");

    // Guests never tread locally; the host stamps prints for them.
    let mut wr = test_world_with("snow-trod-remote", reg.clone());
    wr.set_remote(true);
    wr.set_block(x, y, z, dirt);
    wr.set_block(x, y + 1, z, layer);
    wr.tread(x, y + 1, z);
    assert_eq!(
        wr.get_block(x, y + 1, z),
        layer,
        "remote worlds wait for the echo"
    );
}
