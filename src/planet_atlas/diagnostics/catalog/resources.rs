//! Registered resources diagnostic maps in export order.


use super::{LayerKind, LayerSpec, layer};
pub(super) const LAYERS: &[LayerSpec] = &[
    layer(
        "deposit_site",
        "finite deposit-site reference",
        LayerKind::Categorical,
    ),
    layer(
        "deposit_site_count",
        "number of finite deposit sites centered in the cell",
        LayerKind::Scalar,
    ),
    layer(
        "geological_sites",
        "volcano, intrusion, and deposit site overlay",
        LayerKind::Categorical,
    ),
];
