//! Name geological provinces and apply regional/contact metamorphism.

use super::strata::{basin_for, bedrock_for, stack_for};
use super::{
    BasinKind, BedrockFamily, DetailedBoundary, GeologicalProvinceRecord, IntrusionRecord,
};
use crate::planet::geodesic_distance;
use crate::planet_atlas::{AtlasGrid, AtlasPos, GeometryCell, TectonicCell, TerrainCell, mix64};
use std::collections::BTreeMap;

pub(super) fn province_name(seed: u32, id: u16) -> String {
    const A: [&str; 12] = [
        "Ar", "Bel", "Cor", "Dun", "Eld", "Fal", "Gor", "Hal", "Ith", "Kar", "Mor", "Nor",
    ];
    const B: [&str; 12] = [
        "adan", "bray", "cairn", "dell", "esh", "fold", "gard", "holm", "mere", "reach", "scar",
        "vale",
    ];
    let hash = mix64(u64::from(seed) ^ u64::from(id) ^ 0x7072_6f76_696e_6365);
    format!(
        "{}{} Province",
        A[hash as usize % A.len()],
        B[(hash >> 16) as usize % B.len()]
    )
}

pub(super) fn assign_provinces(
    seed: u32,
    geometry: &AtlasGrid<GeometryCell>,
    tectonics: &mut [TectonicCell],
    terrain: &[TerrainCell],
) -> Vec<GeologicalProvinceRecord> {
    let mut ids = BTreeMap::<(u16, u16, BasinKind, BedrockFamily), u16>::new();
    let mut records = Vec::new();
    for index in 0..tectonics.len() {
        let basin = basin_for(tectonics[index], terrain[index].eroded_elevation);
        let bedrock = bedrock_for(
            tectonics[index],
            basin,
            geometry.values()[index].latitude_radians,
        );
        let key = (
            tectonics[index].plate_id,
            tectonics[index].craton_id,
            basin,
            bedrock,
        );
        let id = *ids.entry(key).or_insert_with(|| {
            let id = records.len() as u16 + 1;
            records.push(GeologicalProvinceRecord {
                id,
                name: province_name(seed, id),
                dominant_bedrock: bedrock,
                plate_id: key.0,
                craton_id: key.1,
                basin,
            });
            id
        });
        tectonics[index].sediment_basin = basin;
        tectonics[index].bedrock_family = bedrock as u16;
        tectonics[index].geological_province = id;
        tectonics[index].stratigraphic_stack = stack_for(bedrock, basin);
    }
    records
}

pub(super) fn metamorphism(
    side: u16,
    geometry: &AtlasGrid<GeometryCell>,
    tectonics: &mut [TectonicCell],
    intrusions: &[IntrusionRecord],
) {
    for (index, cell) in tectonics.iter_mut().enumerate() {
        let pos = AtlasPos::from_index(index, side).unwrap();
        let contact = intrusions.iter().fold(0.0f32, |best, intrusion| {
            let distance = geodesic_distance(pos.center(side), intrusion.pos.center(side)) as f32;
            let value =
                (1.0 - distance / f32::from(intrusion.contact_radius_blocks)).clamp(0.0, 1.0);
            best.max(value)
        });
        let regional = if cell.boundary_detail == DetailedBoundary::ContinentalCollision {
            (1.0 - f32::from(cell.boundary_distance) / 11.0).clamp(0.0, 1.0)
        } else {
            0.0
        };
        cell.metamorphic_grade = ((contact.max(regional) * 5.0).round() as u8).min(5);
        if contact > 0.58 {
            cell.bedrock_family = match BedrockFamily::from_id(cell.bedrock_family) {
                BedrockFamily::Limestone => BedrockFamily::Marble as u16,
                BedrockFamily::Shale => BedrockFamily::Slate as u16,
                BedrockFamily::Sandstone => BedrockFamily::Quartzite as u16,
                other => other as u16,
            };
        }
        let _ = geometry;
    }
}
