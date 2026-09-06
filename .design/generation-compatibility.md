# Generation compatibility experiment

The baseline and final implementation produce the same 54 chunk SHA-256 values
and atlas layer/stage checksums in [generation-compatibility.json](generation-compatibility.json).
The corpus uses seeds 42, 303 and 0x51eed, all six faces, each face's center,
edge and corner chunks, atlas side 8, serial/parallel atlas generation, 1/2/4
chunk workers, and forward/reverse requests. Chunk hashes include every block,
metadata, water salt, soil salinity, block/sky light value and ordered hydrology
volume record. Timing fields are excluded from deterministic atlas comparisons.
This bounded corpus is evidence of preservation, not exhaustive equivalence of
all seeds or performance. Ordinary serial/parallel, chunk-order, ecology, codec,
conservation and gameplay tests complement it.

Two disposable detached checkouts contain the identical probe below. Their only
source changes are `src/refactor_generation_probe.rs` and an appended
`#[cfg(test)] mod refactor_generation_probe;` in `src/lib.rs`. Run this from each
checkout, writing a different output path, then compare the JSON `corpus` values:

```sh
WILDFORGE_DETERMINISM_OUTPUT=probe-output.json cargo test --locked --lib refactor_generation_probe:: -- --test-threads=1 --nocapture
```

Baseline: `8c1ec4082931d4414ef6e2daca2f1a3dbd5d1bb3`.
Candidate: `70de48ac1e455db2ff38abea674a9d0622ae2fcf`.
The instrumented test builds passed in 6.00 and 6.02 seconds respectively after
compilation. The original failed compilation (using BlockId without unwrapping
its numeric field) is retained as `baseline-attempt-1.log`; it produced no corpus.
The corrected probe bytes have the SHA-256 recorded in the JSON.

```rust
use std::collections::BTreeMap;
use std::sync::Arc;
use crate::planet::{ChunkPos, Face};
use crate::planet_atlas::{AtlasConfig, CancellationToken, GenerationMode, PlanetAtlas};

fn digest_chunk(chunk: &crate::chunk::Chunk) -> String {
    let mut bytes = Vec::new();
    for x in 0..crate::chunk::CHUNK_X {
        for z in 0..crate::chunk::CHUNK_Z {
            for y in 0..crate::chunk::CHUNK_Y {
                bytes.extend(chunk.get(x, y, z).0.to_le_bytes());
                bytes.push(chunk.meta(x, y, z));
                bytes.extend(chunk.water_salt(x, y, z).to_le_bytes());
                bytes.push(chunk.soil_salinity(x, y, z));
                let (block, sky) = chunk.light(x, y, z);
                bytes.extend(block);
                bytes.push(sky);
            }
        }
    }
    for record in chunk.hydrology_volumes() {
        bytes.extend(record.reservoir.to_le_bytes());
        bytes.extend(record.baseline_hu.to_le_bytes());
        bytes.extend(record.residual_hu.to_le_bytes());
        bytes.extend(record.salt_mass.to_le_bytes());
    }
    ring::digest::digest(&ring::digest::SHA256, &bytes)
        .as_ref().iter().map(|byte| format!("{byte:02x}")).collect()
}

fn chunk_set(seed: u32, atlas: Arc<PlanetAtlas>, reg: Arc<crate::registry::Registry>, workers: usize, reverse: bool) -> BTreeMap<String, String> {
    let mut positions = Vec::new();
    for face in [Face::PosX, Face::NegX, Face::PosY, Face::NegY, Face::PosZ, Face::NegZ] {
        for (u, v) in [(256, 256), (0, 255), (511, 511)] {
            positions.push(ChunkPos::new(face, u, v).unwrap());
        }
    }
    if reverse { positions.reverse(); }
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers).map(|worker| {
            let atlas = atlas.clone();
            let reg = reg.clone();
            let positions: Vec<_> = positions.iter().copied().skip(worker).step_by(workers).collect();
            scope.spawn(move || {
                let generator = crate::worldgen::Generator::with_atlas(seed, &reg, atlas);
                positions.into_iter().map(|pos| {
                    (format!("{pos:?}"), digest_chunk(&generator.generate(pos, &reg)))
                }).collect::<Vec<_>>()
            })
        }).collect();
        handles.into_iter().flat_map(|handle| handle.join().unwrap()).collect()
    })
}

#[test]
fn compare_refactor_generation_corpus() {
    let reg = Arc::new(crate::registry::load(std::path::Path::new(".probe-empty-mods")));
    let mut records = Vec::new();
    for seed in [42, 303, 0x51eed] {
        let mut previous = None;
        for mode in [GenerationMode::Serial, GenerationMode::Parallel] {
            let atlas = Arc::new(PlanetAtlas::generate(seed, 0xabc, AtlasConfig { side: 8, mode }, &CancellationToken::default(), |_| {}).unwrap());
            let checksums = serde_json::json!({
                "genesis": atlas.manifest.genesis_checksum,
                "dynamic": atlas.manifest.dynamic_checksum,
                "geology": atlas.manifest.geology_checksum,
                "hydrology": atlas.manifest.hydrology_checksum,
                "water_cycle": atlas.manifest.water_cycle_checksum,
                "biome": atlas.manifest.biome_checksum,
                "stages": atlas.manifest.stages.iter().map(|stage| (&stage.id, stage.schema_version, stage.algorithm_version, stage.checksum)).collect::<Vec<_>>()
            });
            let expected = chunk_set(seed, atlas.clone(), reg.clone(), 1, false);
            for workers in [1, 2, 4] {
                for reverse in [false, true] {
                    assert_eq!(expected, chunk_set(seed, atlas.clone(), reg.clone(), workers, reverse));
                }
            }
            let record = serde_json::json!({"seed": seed, "atlas": checksums, "chunks": expected});
            if let Some(previous) = &previous { assert_eq!(previous, &record); }
            previous = Some(record);
        }
        records.push(previous.unwrap());
    }
    let report = serde_json::json!({
        "revision": env!("WILDFORGE_BUILD_COMMIT"),
        "instrumented_with_identical_probe": true,
        "atlas_side": 8,
        "content_hash": 0xabc,
        "workers": [1, 2, 4],
        "request_orders": ["forward", "reverse"],
        "atlas_modes": ["serial", "parallel"],
        "corpus": records,
    });
    std::fs::write(std::env::var_os("WILDFORGE_DETERMINISM_OUTPUT").unwrap(), serde_json::to_vec_pretty(&report).unwrap()).unwrap();
}
```
