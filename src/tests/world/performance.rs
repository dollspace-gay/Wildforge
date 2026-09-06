//! Performance scenarios.

use super::*;

#[test]
#[ignore = "operator probe for WILDFORGE_PROBE_WORLD production save"]
fn production_idle_tick_performance_probe() {
    let root = std::env::var("WILDFORGE_PROBE_WORLD")
        .map(std::path::PathBuf::from)
        .expect("set WILDFORGE_PROBE_WORLD to a qualified production save");
    let mut world = World::load_or_create(root, base_reg()).unwrap();
    world.prepare_common_spawn(|_, _, _| {}).unwrap();
    let resident = world.chunk_count();
    let mut server = crate::server::Server::new(world, 0.3, 0x1d1e);
    let mut events = Vec::new();
    let start = std::time::Instant::now();
    for _ in 0..1_200 {
        server.advance(1.0 / 60.0, &[], &mut events);
        std::hint::black_box(&events);
        events.clear();
    }
    let each = start.elapsed().as_nanos() / 1_200;
    println!("idle profile: resident_chunks={resident} tick={each} ns");
}

#[test]
#[ignore = "measurement probe, not an assertion"]
fn measure_chunk_composition() {
    use crate::chunk::CHUNK_CELLS;
    use std::collections::HashSet;
    let mut w = test_world("compose");
    let (mut ids, mut meta_nz, mut sky_vals, mut lb_nz, mut n) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    let mut worst_ids = 0usize;
    for cx in -3..=3 {
        for cz in -3..=3 {
            let pos = tchunk(cx, cz);
            w.ensure_chunk(pos);
            let c = &w.chunks()[&pos];
            let mut set: HashSet<u16> = HashSet::new();
            let mut sky: HashSet<u8> = HashSet::new();
            let (mut mnz, mut lnz) = (0usize, 0usize);
            for x in 0..16 {
                for z in 0..16 {
                    for y in 0..256 {
                        set.insert(c.get(x, y, z).0);
                        if c.meta(x, y, z) != 0 {
                            mnz += 1;
                        }
                        let (lb, ls) = c.light(x, y, z);
                        sky.insert(ls);
                        if lb != [0, 0, 0] {
                            lnz += 1;
                        }
                    }
                }
            }
            ids += set.len();
            worst_ids = worst_ids.max(set.len());
            meta_nz += mnz;
            sky_vals += sky.len();
            lb_nz += lnz;
            n += 1;
        }
    }
    println!("PROBE over {n} chunks ({CHUNK_CELLS} cells each):");
    println!(
        "  distinct block ids/chunk: avg {:.1}, worst {worst_ids}  -> palette bits {}",
        ids as f64 / n as f64,
        (worst_ids as f64).log2().ceil() as u32
    );
    println!(
        "  meta non-zero: avg {:.2}% of cells",
        100.0 * meta_nz as f64 / (n * CHUNK_CELLS) as f64
    );
    println!(
        "  block-light non-zero: avg {:.3}% of cells",
        100.0 * lb_nz as f64 / (n * CHUNK_CELLS) as f64
    );
    println!(
        "  distinct sky-light values/chunk: avg {:.1} (needs 4 bits)",
        sky_vals as f64 / n as f64
    );
}
