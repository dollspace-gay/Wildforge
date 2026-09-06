//! Compile-time WGSL composition in one explicit, stable order.
//!
//! Units retain their authored whitespace and comments. The renderer and shader
//! validators consume this same aggregate, with no runtime file I/O or fallback.

pub(crate) const WORLD: &str = concat!(
    include_str!("bindings.wgsl"),
    include_str!("geometry.wgsl"),
    include_str!("shadows.wgsl"),
    include_str!("lighting.wgsl"),
    include_str!("sky.wgsl"),
    include_str!("materials.wgsl"),
    include_str!("terrain.wgsl"),
    include_str!("diagnostics.wgsl"),
    include_str!("lines.wgsl"),
    include_str!("ui.wgsl"),
);

/// Point-light distance and glass-transmission passes use their own uniform ABI.
pub(crate) const POINT_SHADOW: &str = include_str!("point_shadow.wgsl");
/// Cascaded sun depth pass uses one dynamically selected matrix.
pub(crate) const CASCADE_SHADOW: &str = include_str!("cascade_shadow.wgsl");
