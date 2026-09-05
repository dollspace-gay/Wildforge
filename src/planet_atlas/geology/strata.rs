//! Assign sediment basins, host rocks, and stable stratigraphic stacks.

use crate::planet_atlas::TectonicCell;
use crate::chunk::SEA_LEVEL;
use super::{BasinKind, BedrockFamily, DetailedBoundary, StratigraphicStackRecord};

pub(super) fn basin_for(cell: TectonicCell, elevation: f32) -> BasinKind {
    if elevation <= SEA_LEVEL as f32 - 18.0 {
        BasinKind::DeepMarine
    } else if elevation <= SEA_LEVEL as f32 + 5.0 {
        BasinKind::MarineShelf
    } else {
        match cell.boundary_detail {
            DetailedBoundary::ContinentalCollision if cell.boundary_distance <= 7 => {
                BasinKind::Foreland
            }
            DetailedBoundary::ContinentalRift if cell.boundary_distance <= 5 => BasinKind::Rift,
            DetailedBoundary::PassiveWeak if cell.boundary_distance <= 5 => {
                BasinKind::PassiveMargin
            }
            _ if elevation < SEA_LEVEL as f32 + 13.0 && cell.continental_crust > 40_000 => {
                BasinKind::Closed
            }
            _ => BasinKind::None,
        }
    }
}

pub(super) fn bedrock_for(cell: TectonicCell, basin: BasinKind, latitude: f32) -> BedrockFamily {
    if cell.continental_crust < 18_000 {
        if cell.oceanic_age < 35 || cell.volcanic_history != 0 {
            BedrockFamily::Basalt
        } else {
            BedrockFamily::Ultramafic
        }
    } else {
        match basin {
            BasinKind::DeepMarine => BedrockFamily::Shale,
            BasinKind::MarineShelf | BasinKind::PassiveMargin => {
                if cell.crust_age.is_multiple_of(2) {
                    BedrockFamily::Limestone
                } else {
                    BedrockFamily::Sandstone
                }
            }
            BasinKind::Foreland => BedrockFamily::Shale,
            BasinKind::Rift => BedrockFamily::Basalt,
            BasinKind::Closed
                if latitude.abs().to_degrees() > 15.0 && latitude.abs().to_degrees() < 42.0 =>
            {
                BedrockFamily::Evaporite
            }
            _ if cell.craton_id != 0 => BedrockFamily::Granite,
            _ => BedrockFamily::MixedBasement,
        }
    }
}

pub(super) fn stack_for(bedrock: BedrockFamily, basin: BasinKind) -> u16 {
    match basin {
        BasinKind::MarineShelf | BasinKind::PassiveMargin => 2,
        BasinKind::DeepMarine => 3,
        BasinKind::Foreland => 4,
        BasinKind::Rift => 5,
        BasinKind::Closed => 6,
        BasinKind::None => match bedrock {
            BedrockFamily::Basalt | BedrockFamily::Ultramafic => 7,
            _ => 1,
        },
    }
}

pub(super) fn default_stacks() -> Vec<StratigraphicStackRecord> {
    use BedrockFamily as B;
    [
        (
            1,
            "cratonic basement",
            vec![B::Granite, B::MixedBasement, B::Sandstone],
        ),
        (
            2,
            "passive shelf",
            vec![B::MixedBasement, B::Sandstone, B::Limestone, B::Shale],
        ),
        (
            3,
            "deep marine basin",
            vec![B::Basalt, B::Shale, B::Limestone],
        ),
        (
            4,
            "foreland basin",
            vec![B::MixedBasement, B::Sandstone, B::Shale, B::Sandstone],
        ),
        (
            5,
            "continental rift",
            vec![B::MixedBasement, B::Basalt, B::Shale, B::Sandstone],
        ),
        (
            6,
            "closed evaporite basin",
            vec![B::MixedBasement, B::Sandstone, B::Evaporite],
        ),
        (7, "oceanic crust", vec![B::Ultramafic, B::Basalt, B::Shale]),
    ]
    .into_iter()
    .map(|(id, name, layers)| StratigraphicStackRecord {
        id,
        name: name.into(),
        nominal_thicknesses: vec![28; layers.len()],
        layers_bottom_to_top: layers,
    })
    .collect()
}
