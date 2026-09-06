//! Finite mineral placement and per-chunk extraction envelopes.

use super::{
    BasinKind, BedrockFamily, ContinentRecord, DepositRecord, IntrusionRecord, MineralKind,
};
use crate::chunk::CHUNK_X;
use crate::planet::geodesic_distance;
use crate::planet_atlas::{
    AtlasGrid, AtlasPos, GeometryCell, ResourceCell, TectonicCell, TerrainCell, cell_hash,
};
use std::cmp::Ordering as CmpOrdering;
use std::collections::BTreeMap;

pub(super) fn host_allows(kind: MineralKind, cell: TectonicCell, latitude: f32) -> bool {
    let rock = BedrockFamily::from_id(cell.bedrock_family);
    match kind {
        MineralKind::Copper => cell.volcanic_history != 0 || matches!(rock, BedrockFamily::Basalt),
        MineralKind::Tin => matches!(rock, BedrockFamily::Granite | BedrockFamily::Quartzite),
        MineralKind::Iron => matches!(
            rock,
            BedrockFamily::MixedBasement | BedrockFamily::Shale | BedrockFamily::Basalt
        ),
        MineralKind::Cobalt | MineralKind::Manganese => matches!(
            rock,
            BedrockFamily::Basalt | BedrockFamily::Ultramafic | BedrockFamily::MixedBasement
        ),
        MineralKind::Cinnabar => cell.volcanic_history != 0 || cell.fault_intensity > 8_000,
        MineralKind::Coal => matches!(
            cell.sediment_basin,
            BasinKind::MarineShelf
                | BasinKind::Foreland
                | BasinKind::Closed
                | BasinKind::PassiveMargin
        ),
        MineralKind::Gold => cell.fault_intensity > 9_000 || cell.volcanic_history != 0,
        MineralKind::Galena => matches!(rock, BedrockFamily::Limestone | BedrockFamily::Marble),
        MineralKind::Chromite => matches!(rock, BedrockFamily::Basalt | BedrockFamily::Ultramafic),
        MineralKind::Diamond => cell.craton_id != 0 && cell.crust_age > 1_800,
        MineralKind::RareEarth => {
            matches!(rock, BedrockFamily::Granite) && cell.metamorphic_grade > 0
        }
        MineralKind::Halite => {
            cell.sediment_basin == BasinKind::Closed
                && latitude.abs().to_degrees() > 12.0
                && latitude.abs().to_degrees() < 48.0
        }
        MineralKind::Pitchblende => matches!(rock, BedrockFamily::Granite) && cell.craton_id != 0,
        MineralKind::Geode => matches!(rock, BedrockFamily::Limestone | BedrockFamily::Marble),
        MineralKind::Other => cell.continental_crust >= 24_000,
    }
}

struct DepositPlacement<'a> {
    seed: u32,
    side: u16,
    geometry: &'a AtlasGrid<GeometryCell>,
    tectonics: &'a [TectonicCell],
    terrain: &'a [TerrainCell],
}

impl DepositPlacement<'_> {
    fn choose_site(
        &self,
        salt: u64,
        kind: MineralKind,
        landmass: Option<u16>,
        occupied: &[AtlasPos],
    ) -> Option<AtlasPos> {
        let mut best: Option<(u64, AtlasPos)> = None;
        for index in 0..self.tectonics.len() {
            if self.terrain[index].landmass_id == 0
                || landmass.is_some_and(|wanted| self.terrain[index].landmass_id != wanted)
                || !host_allows(
                    kind,
                    self.tectonics[index],
                    self.geometry.values()[index].latitude_radians,
                )
            {
                continue;
            }
            let pos = AtlasPos::from_index(index, self.side).unwrap();
            if occupied.iter().any(|other| {
                geodesic_distance(pos.center(self.side), other.center(self.side)) < 210.0
            }) {
                continue;
            }
            let score = cell_hash(self.seed, pos, salt ^ kind as u64);
            if best.is_none_or(|candidate| score < candidate.0) {
                best = Some((score, pos));
            }
        }
        best.map(|(_, pos)| pos)
    }
}

pub(super) fn deposits(
    seed: u32,
    side: u16,
    geometry: &AtlasGrid<GeometryCell>,
    tectonics: &[TectonicCell],
    terrain: &[TerrainCell],
    continents: &[ContinentRecord],
    intrusions: &[IntrusionRecord],
) -> (Vec<DepositRecord>, Vec<ResourceCell>) {
    let mut out = Vec::<DepositRecord>::new();
    let mut occupied = BTreeMap::<MineralKind, Vec<AtlasPos>>::new();
    let major: Vec<u16> = continents
        .iter()
        .filter(|c| c.major)
        .map(|c| c.id)
        .collect();
    let placement = DepositPlacement {
        seed,
        side,
        geometry,
        tectonics,
        terrain,
    };

    let mut add = |kind: MineralKind, landmass: Option<u16>, ordinal: u64| {
        let pos = placement.choose_site(
            0x6465_706f_7369_7400 ^ ordinal,
            kind,
            landmass,
            occupied.get(&kind).map(Vec::as_slice).unwrap_or_default(),
        );
        let Some(pos) = pos else { return };
        occupied.entry(kind).or_default().push(pos);
        let cell = tectonics[pos.index(side)];
        let hash = cell_hash(seed, pos, 0x0067_7261_6465 ^ kind as u64);
        let (radius, quota) = match kind {
            MineralKind::Diamond | MineralKind::RareEarth | MineralKind::Pitchblende => {
                (64u16, 3u16)
            }
            MineralKind::Tin | MineralKind::Gold | MineralKind::Galena | MineralKind::Halite => {
                (112, 6)
            }
            _ => (176, 12),
        };
        let chunk_radius = u32::from(radius).div_ceil(CHUNK_X as u32).saturating_add(2);
        let upper = chunk_radius
            .saturating_mul(2)
            .saturating_add(1)
            .saturating_pow(2);
        let source_body_id = intrusions
            .iter()
            .min_by(|a, b| {
                geodesic_distance(pos.center(side), a.pos.center(side))
                    .partial_cmp(&geodesic_distance(pos.center(side), b.pos.center(side)))
                    .unwrap_or(CmpOrdering::Equal)
            })
            .filter(|intrusion| {
                geodesic_distance(pos.center(side), intrusion.pos.center(side)) < 420.0
            })
            .map_or(0, |intrusion| intrusion.id);
        out.push(DepositRecord {
            id: out.len() as u32 + 1,
            mineral: kind,
            pos,
            host: BedrockFamily::from_id(cell.bedrock_family),
            geological_province: cell.geological_province,
            landmass_id: terrain[pos.index(side)].landmass_id,
            source_body_id,
            radius_blocks: radius,
            depth_min: match kind {
                MineralKind::Coal | MineralKind::Halite => 28,
                _ => 4,
            },
            depth_max: match kind {
                MineralKind::Diamond => 90,
                _ => 120,
            },
            grade_ppm: 800 + (hash % 22_000) as u32,
            tonnage_blocks: u64::from(quota) * u64::from(upper),
            max_blocks_per_chunk: quota,
            eligible_chunk_upper_bound: upper,
        });
    };

    // Copper/iron/coal progression is present on every major continent.
    for (ordinal, landmass) in major.iter().copied().enumerate() {
        for (offset, kind) in [MineralKind::Copper, MineralKind::Iron, MineralKind::Coal]
            .into_iter()
            .enumerate()
        {
            add(kind, Some(landmass), (ordinal * 8 + offset) as u64);
        }
    }
    // Regional and treasure classes have separated redundant sites.
    for kind in MineralKind::ALL_TRACKED {
        for ordinal in 0..2 {
            add(kind, None, 0x1000 + kind as u64 * 16 + ordinal as u64);
        }
        if kind == MineralKind::Tin {
            add(kind, None, 0x1000 + kind as u64 * 16 + 2);
        }
    }
    // Unknown data-pack ores receive finite generic host provinces too.
    for ordinal in 0..major.len().max(2) {
        add(
            MineralKind::Other,
            major.get(ordinal).copied(),
            0x9000 + ordinal as u64,
        );
    }

    let mut resources = vec![ResourceCell::default(); tectonics.len()];
    for site in &out {
        let cell = &mut resources[site.pos.index(side)];
        if cell.deposit_site_ref == 0 {
            cell.deposit_site_ref = site.id;
        }
        cell.deposit_site_count = cell.deposit_site_count.saturating_add(1);
    }
    (out, resources)
}
