//! Ordered registry of diagnostic map domains.

mod biomes;
mod climate;
mod geology;
mod hydrology;
mod resources;
mod weather;

#[derive(Clone, Copy)]
pub(in crate::planet_atlas::diagnostics) enum LayerKind {
    Scalar,
    Categorical,
    Direction,
}

#[derive(Clone, Copy)]
pub(in crate::planet_atlas::diagnostics) struct LayerSpec {
    pub(in crate::planet_atlas::diagnostics) id: &'static str,
    pub(in crate::planet_atlas::diagnostics) description: &'static str,
    pub(in crate::planet_atlas::diagnostics) kind: LayerKind,
}

const fn layer(id: &'static str, description: &'static str, kind: LayerKind) -> LayerSpec {
    LayerSpec {
        id,
        description,
        kind,
    }
}

const GROUPS: &[&[LayerSpec]] = &[
    geology::LAYERS,
    climate::LAYERS,
    hydrology::LAYERS,
    biomes::LAYERS,
    resources::LAYERS,
    weather::LAYERS,
];

pub(super) fn layers() -> impl Iterator<Item = &'static LayerSpec> {
    GROUPS.iter().flat_map(|group| group.iter())
}

pub(super) fn layer_count() -> usize {
    GROUPS.iter().map(|group| group.len()).sum()
}
