//! Falling scenarios.

use super::*;

#[test]
fn sand_falls_lands_chains_and_crushes() {
    let reg = base_reg();
    let mut w = test_world_with("gw-fall", reg.clone());
    let sand = reg.block_id("base:sand").unwrap();
    let plank = reg.block_id("base:planks").unwrap();
    let my = 120;
    // The ground it will land on (before we scaffold anything).
    let gy = w.surface_height(10, 10);
    // A supported sand column: pull the support and it all comes down.
    w.set_block(10, my, 10, plank);
    w.set_block(10, my + 1, 10, sand);
    w.set_block(10, my + 2, 10, sand);
    assert!(w.falling_blocks().is_empty(), "supported sand stays put");
    w.set_block(10, my, 10, AIR);
    assert_eq!(w.falling_blocks().len(), 2, "the whole column detaches");
    assert_eq!(
        w.get_block(10, my + 1, 10),
        AIR,
        "detached cells empty atomically"
    );
    for _ in 0..600 {
        w.tick_falling(1.0 / 30.0);
    }
    assert!(w.falling_blocks().is_empty(), "everything lands");
    assert_eq!(
        w.get_block(10, gy + 1, 10),
        sand,
        "first sand rests on ground"
    );
    assert_eq!(
        w.get_block(10, gy + 2, 10),
        sand,
        "second stacks on the first"
    );

    // Placing sand over air drops it immediately.
    w.set_block(12, my + 3, 12, sand);
    assert_eq!(
        w.get_block(12, my + 3, 12),
        AIR,
        "unsupported placement detaches"
    );
    assert_eq!(w.falling_blocks().len(), 1);
    w.settle_falling();
    assert!(w.falling_blocks().is_empty());

    // A crushed crop pops as its drop.
    let torch = reg.block_id("base:torch").unwrap();
    let ty2 = w.surface_height(14, 14);
    w.set_block(14, ty2 + 1, 14, torch);
    w.clear_pending_drops();
    w.set_block(14, ty2 + 6, 14, sand);
    w.settle_falling();
    assert_eq!(w.get_block(14, ty2 + 1, 14), sand, "sand took the cell");
    assert!(
        w.pending_drops()
            .iter()
            .any(|(_, st)| Some(st.item) == reg.item_id("base:torch")),
        "the torch popped as a drop"
    );
}

#[test]
fn falling_blocks_detach_and_land_on_both_sides_of_every_planet_seam() {
    use crate::planet::BlockPos;

    let reg = base_reg();
    let sand = b(&reg, "base:sand");
    let stone = b(&reg, "base:stone");
    let mut world = World::new(51, tmp_dir("planet-falling-all-seams"), reg);
    let seams = directed_planet_seams();
    let columns: Vec<_> = seams
        .iter()
        .flat_map(|seam| [seam.source, seam.across])
        .collect();
    let chunks: std::collections::BTreeSet<_> = columns
        .iter()
        .map(|surface| crate::planet::ChunkPos::from_surface(*surface))
        .collect();
    world.insert_empty_chunks_for_test(chunks);

    let fixture_set = |world: &mut World, pos: BlockPos, block| {
        let (x, y, z) = pos.local();
        world
            .chunks_mut()
            .get_mut(&pos.chunk())
            .expect("falling fixture chunk")
            .set(x, y, z, block);
    };
    for surface in &columns {
        // Save-time settling advances in coarse vertical steps.  Model the
        // planet's solid shell rather than a one-voxel floating platform, so
        // a coarse sample always finds rock below the landing surface.
        for y in 1..=95 {
            fixture_set(
                &mut world,
                BlockPos::new(surface.face(), surface.u(), y, surface.v()).unwrap(),
                stone,
            );
        }
        fixture_set(
            &mut world,
            BlockPos::new(surface.face(), surface.u(), 100, surface.v()).unwrap(),
            stone,
        );
        fixture_set(
            &mut world,
            BlockPos::new(surface.face(), surface.u(), 101, surface.v()).unwrap(),
            sand,
        );
    }

    for surface in &columns {
        world.set_block_at(
            BlockPos::new(surface.face(), surface.u(), 100, surface.v()).unwrap(),
            AIR,
        );
    }
    assert_eq!(
        world.falling_blocks().len(),
        columns.len(),
        "every seam-side column detaches exactly once"
    );
    for surface in &columns {
        let launched = BlockPos::new(surface.face(), surface.u(), 101, surface.v()).unwrap();
        assert_eq!(world.get_block_at(launched), AIR);
        assert!(world.falling_blocks().iter().any(|falling| {
            falling.pos.face() == surface.face()
                && falling.pos.u() == f32::from(surface.u())
                && falling.pos.v() == f32::from(surface.v())
        }));
    }

    world.settle_falling();
    assert!(world.falling_blocks().is_empty());
    for (index, seam) in seams.iter().enumerate() {
        for surface in [seam.source, seam.across] {
            assert_eq!(
                world.get_block_at(
                    BlockPos::new(surface.face(), surface.u(), 96, surface.v()).unwrap()
                ),
                sand,
                "falling block landed on the wrong chart for seam {index}: {:?} {:?}",
                seam.face,
                seam.direction
            );
        }
    }
}
