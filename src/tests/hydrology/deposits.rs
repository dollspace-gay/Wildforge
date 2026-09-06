//! Deposits scenarios.

use super::*;

#[test]
fn deltas_estuaries_and_resistant_valleys_follow_energy_and_substrate() {
    let atlas = atlas();
    let hydro = atlas.genesis.hydrology.values();
    let mean_cell_area = atlas
        .genesis
        .geometry
        .values()
        .iter()
        .map(|cell| f64::from(cell.physical_area))
        .sum::<f64>()
        / atlas.genesis.geometry.len() as f64;
    assert!(hydro.iter().any(|cell| cell.flags & HYDRO_DELTA != 0));
    assert!(hydro.iter().any(|cell| cell.flags & HYDRO_ESTUARY != 0));
    assert!(
        hydro
            .iter()
            .any(|cell| { cell.flags & crate::planet_atlas::HYDRO_FLOODPLAIN != 0 })
    );
    assert!(
        hydro
            .iter()
            .any(|cell| cell.flags & crate::planet_atlas::HYDRO_WATERFALL != 0)
    );
    assert!(
        hydro
            .iter()
            .any(|cell| cell.flags & crate::planet_atlas::HYDRO_WETLAND != 0)
    );
    let mut soft = Vec::new();
    let mut hard = Vec::new();
    for (index, cell) in hydro.iter().copied().enumerate() {
        let equivalent_discharge = f64::from(cell.mean_discharge) / mean_cell_area;
        if cell.flags & HYDRO_RIVER == 0 || !(1.0..=20.0).contains(&equivalent_discharge) {
            continue;
        }
        match crate::planet_atlas::BedrockFamily::from_id(
            atlas.genesis.tectonics.values()[index].bedrock_family,
        ) {
            crate::planet_atlas::BedrockFamily::Shale
            | crate::planet_atlas::BedrockFamily::Sandstone => {
                soft.push(f64::from(cell.channel_width_centiblocks))
            }
            crate::planet_atlas::BedrockFamily::Quartzite
            | crate::planet_atlas::BedrockFamily::Ultramafic
            | crate::planet_atlas::BedrockFamily::Granite => {
                hard.push(f64::from(cell.channel_width_centiblocks))
            }
            _ => {}
        }
    }
    assert!(!soft.is_empty() && !hard.is_empty());
    assert!(average(soft.into_iter()) > average(hard.into_iter()));
}

#[test]
fn gold_and_monazite_placers_follow_sources_into_depositional_reaches() {
    let atlas = atlas();
    let placers: Vec<_> = atlas
        .geology
        .deposits
        .iter()
        .filter(|deposit| {
            matches!(
                deposit.mineral,
                crate::planet_atlas::MineralKind::Gold
                    | crate::planet_atlas::MineralKind::RareEarth
            ) && deposit.depth_min == SEA_LEVEL as u16
                && deposit.max_blocks_per_chunk == 2
        })
        .collect();
    assert!(
        !placers.is_empty(),
        "the fixture routes at least one placer"
    );
    assert!(
        placers
            .iter()
            .any(|deposit| { deposit.mineral == crate::planet_atlas::MineralKind::RareEarth })
    );
    for placer in placers {
        let source = atlas
            .geology
            .deposits
            .iter()
            .find(|source| source.id == placer.source_body_id && source.mineral == placer.mineral)
            .expect("placer retains its geological source deposit");
        let target = atlas.genesis.hydrology.get(placer.pos).unwrap();
        assert_eq!(
            source.tonnage_blocks + placer.tonnage_blocks,
            u64::from(source.max_blocks_per_chunk + 1)
                * u64::from(source.eligible_chunk_upper_bound),
            "placer tonnage is removed from, not added beside, its source"
        );
        assert!(
            target.flags
                & (crate::planet_atlas::HYDRO_FLOODPLAIN | crate::planet_atlas::HYDRO_DELTA)
                != 0
        );
        let mut at = source.pos;
        let mut reached = false;
        for _ in 0..256 {
            if at == placer.pos {
                reached = true;
                break;
            }
            let cell = atlas.genesis.hydrology.get(at).unwrap();
            let Some(next) = (cell.drainage_receiver != u32::MAX)
                .then(|| {
                    crate::planet_atlas::AtlasPos::from_index(
                        cell.drainage_receiver as usize,
                        atlas.side(),
                    )
                })
                .flatten()
            else {
                break;
            };
            at = next;
        }
        assert!(reached, "placer is downstream from its source");
    }
}
