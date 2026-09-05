//! Scalar, categorical, and direction atlas map export.

use crate::planet_atlas::{AtlasError, PlanetAtlas};
use crate::planet_atlas::diagnostics::{FACE_LAYOUT};
use std::path::{Path};
use super::catalog::{LayerKind, LayerSpec};
use super::values::layer_value;
use super::images::{write_png, scalar_color, categorical_color, direction_color};
pub(in crate::planet_atlas::diagnostics) fn export_layer(atlas: &PlanetAtlas, spec: LayerSpec, output: &Path) -> Result<(), AtlasError> {
    let side = u32::from(atlas.side());
    let width = side * 3;
    let height = side * 2;
    let values: Vec<f64> = atlas
        .genesis
        .geometry
        .iter()
        .map(|(pos, _)| layer_value(atlas, spec.id, pos))
        .collect();
    let (minimum, maximum) = values.iter().fold(
        (f64::INFINITY, f64::NEG_INFINITY),
        |(minimum, maximum), value| (minimum.min(*value), maximum.max(*value)),
    );
    let mut pixels = vec![0u8; width as usize * height as usize * 3];
    for (pos, _) in atlas.genesis.geometry.iter() {
        let face_index = pos.face.index() as u32;
        let origin_x = (face_index % 3) * side;
        let origin_y = (face_index / 3) * side;
        let target =
            ((origin_y + u32::from(pos.v)) * width + origin_x + u32::from(pos.u)) as usize * 3;
        let value = values[pos.index(atlas.side())];
        let color = match spec.kind {
            LayerKind::Scalar => scalar_color(value, minimum, maximum),
            LayerKind::Categorical => categorical_color(value as u64),
            LayerKind::Direction => direction_color(value),
        };
        pixels[target..target + 3].copy_from_slice(&color);
    }
    write_png(
        &output.join(format!("{}.png", spec.id)),
        width,
        height,
        &pixels,
    )?;
    let legend = format!(
        "layer = {}\ndescription = {}\nlayout = {}\nkind = {}\nminimum = {:.9}\nmaximum = {:.9}\ncolor = scalar: navy-cyan-yellow-red; categorical: stable id hash (zero black); direction: cyclic hue\n",
        spec.id,
        spec.description,
        FACE_LAYOUT,
        match spec.kind {
            LayerKind::Scalar => "scalar",
            LayerKind::Categorical => "categorical",
            LayerKind::Direction => "direction",
        },
        minimum,
        maximum
    );
    crate::persist::atomic_write(
        &output.join(format!("{}.legend.txt", spec.id)),
        legend.as_bytes(),
        false,
    )?;
    Ok(())
}
