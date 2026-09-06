//! Qualification scenarios.

use super::*;

#[test]
#[ignore = "operator probe for a production save named by WILDFORGE_PROBE_WORLD"]
fn production_river_entitlement_probe() {
    let root = std::env::var_os("WILDFORGE_PROBE_WORLD")
        .map(std::path::PathBuf::from)
        .expect("set WILDFORGE_PROBE_WORLD to a production world directory");
    let river_id = std::env::var("WILDFORGE_PROBE_RIVER")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(641);
    let atlas = Arc::new(PlanetAtlas::load(&root).unwrap());
    let reservoir_id = surface_reservoir_id(SurfaceReservoirKind::River, river_id);
    let reservoir = atlas
        .water_cycle
        .reservoirs
        .iter()
        .find(|reservoir| reservoir.id == reservoir_id)
        .unwrap();
    let initial_total_hu = reservoir.initial_total_hu;
    let mut faces = std::collections::BTreeMap::new();
    let mut atlas_hu = 0u64;
    let mut members = 0usize;
    let mut bounds = None::<(crate::planet::Face, f64, f64, f64, f64, f32)>;
    for (index, cell) in atlas.genesis.hydrology.values().iter().enumerate() {
        if cell.river_id != river_id || cell.baseline_water_units == 0 {
            continue;
        }
        let pos = crate::planet_atlas::AtlasPos::from_index(index, atlas.side()).unwrap();
        let receiver = crate::planet_atlas::AtlasPos::from_index(
            cell.drainage_receiver as usize,
            atlas.side(),
        );
        eprintln!(
            "member={pos:?} receiver={receiver:?} width={} depth={} surface={} bed={} units={}",
            f32::from(cell.channel_width_centiblocks) / 100.0,
            f32::from(cell.channel_depth_centiblocks) / 100.0,
            cell.water_surface_elevation,
            cell.channel_bed_elevation,
            cell.baseline_water_units,
        );
        if let Some(receiver) = receiver
            && receiver.face == pos.face
        {
            let a = pos.center(atlas.side());
            let b = receiver.center(atlas.side());
            let width = f32::from(cell.channel_width_centiblocks) / 100.0;
            bounds = Some(bounds.map_or(
                (
                    pos.face,
                    a.u.min(b.u),
                    a.u.max(b.u),
                    a.v.min(b.v),
                    a.v.max(b.v),
                    width,
                ),
                |(face, min_u, max_u, min_v, max_v, old_width)| {
                    assert_eq!(face, pos.face, "probe river crosses a face");
                    (
                        face,
                        min_u.min(a.u).min(b.u),
                        max_u.max(a.u).max(b.u),
                        min_v.min(a.v).min(b.v),
                        max_v.max(a.v).max(b.v),
                        old_width.max(width),
                    )
                },
            ));
        }
        *faces.entry(pos.face).or_insert(0usize) += 1;
        atlas_hu = atlas_hu.saturating_add(
            cell.baseline_water_units
                .saturating_mul(crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL),
        );
        members += 1;
    }
    eprintln!(
        "river={river_id} members={members} faces={faces:?} atlas_hu={atlas_hu} initial={} coarse={} committed={}",
        reservoir.initial_total_hu, reservoir.coarse.water_hu, reservoir.committed.water_hu
    );
    assert_eq!(atlas_hu, reservoir.initial_total_hu);

    let (face, min_u, max_u, min_v, max_v, width) = bounds.unwrap();
    let margin = f64::from(width * 2.2 + 28.0);
    let min_u = (min_u - margin).max(0.0) as u16 / CHUNK_X as u16;
    let max_u = (max_u + margin).min(f64::from(FACE_BLOCKS - 1)) as u16 / CHUNK_X as u16;
    let min_v = (min_v - margin).max(0.0) as u16 / CHUNK_Z as u16;
    let max_v = (max_v + margin).min(f64::from(FACE_BLOCKS - 1)) as u16 / CHUNK_Z as u16;
    let reg = base_reg();
    let generator =
        crate::worldgen::Generator::with_atlas(atlas.manifest.seed, &reg, atlas.clone());
    let mut requested_hu = 0u64;
    for u in min_u..=max_u {
        for v in min_v..=max_v {
            let chunk = generator.generate(ChunkPos::new(face, u, v).unwrap(), &reg);
            requested_hu += chunk
                .hydrology_volumes()
                .iter()
                .filter(|record| record.reservoir == reservoir_id)
                .map(|record| {
                    (i128::from(record.baseline_hu) - i128::from(record.residual_hu)) as u64
                })
                .sum::<u64>();
        }
    }
    eprintln!(
        "raster chunks={} requested_hu={requested_hu} available_hu={}",
        usize::from(max_u - min_u + 1) * usize::from(max_v - min_v + 1),
        initial_total_hu,
    );
    assert!(requested_hu <= initial_total_hu);
}

#[test]
fn atlas_names_rivers_lakes_seas_and_watersheds_deterministically() {
    let atlas = atlas();
    for river in atlas.hydrology.rivers.iter().take(8) {
        let surface = surface_at(river.mouth, atlas.side());
        assert!(atlas.hydrological_name_at(surface).is_some());
    }
    assert!(
        atlas
            .hydrology
            .lakes
            .iter()
            .all(|lake| !lake.name.trim().is_empty())
    );
    assert!(
        atlas
            .hydrology
            .oceans
            .iter()
            .all(|ocean| !ocean.name.trim().is_empty())
    );
    assert!(
        atlas
            .hydrology
            .watersheds
            .iter()
            .all(|watershed| !watershed.name.trim().is_empty())
    );
}

/// Operator/visual qualification probe. Run serially to keep atlas creation
/// inside the documented memory envelope:
/// `WILDFORGE_ATLAS_OUTPUT=/tmp/wildforge-hydrology CARGO_BUILD_JOBS=1 cargo
/// test --locked --release --lib
/// tests::hydrology::export_production_hydrology_qualification -- --ignored
/// --exact --nocapture --test-threads=1`
#[test]
#[ignore]
fn export_production_hydrology_qualification() {
    let peak_rss_kib = || {
        std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|status| {
                status.lines().find_map(|line| {
                    line.strip_prefix("VmHWM:")?
                        .split_whitespace()
                        .next()?
                        .parse::<u64>()
                        .ok()
                })
            })
    };
    let output = std::env::var_os("WILDFORGE_ATLAS_OUTPUT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| tmp_dir("production-hydrology-qualification"));
    std::fs::create_dir_all(&output).unwrap();
    let started = std::time::Instant::now();
    let atlas = PlanetAtlas::generate(
        1_337,
        0,
        crate::planet_atlas::AtlasConfig {
            side: crate::planet_atlas::ATLAS_FACE_SIDE,
            mode: crate::planet_atlas::GenerationMode::Serial,
        },
        &crate::planet_atlas::CancellationToken::default(),
        |_| {},
    )
    .unwrap();
    let generated = started.elapsed();
    let generation_peak_rss_kib = peak_rss_kib();
    atlas.write_new(&output).unwrap();
    let report = crate::planet_atlas::export_diagnostics(&atlas, &output).unwrap();
    let export_peak_rss_kib = peak_rss_kib();
    println!(
        "output={} generation={generated:?} generation_peak_rss_kib={generation_peak_rss_kib:?} export_peak_rss_kib={export_peak_rss_kib:?} cells={} maps={} hydrology_bytes={} oceans={} lakes={} rivers={} watersheds={}",
        output.display(),
        report.cell_count,
        report.exported_maps.len(),
        report.hydrology_bytes,
        atlas.hydrology.oceans.len(),
        atlas.hydrology.lakes.len(),
        atlas.hydrology.rivers.len(),
        atlas.hydrology.watersheds.len()
    );
}
