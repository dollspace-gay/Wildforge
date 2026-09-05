//! Reciprocal country contacts exported in stable record order.

use crate::planet_atlas::{AtlasError, PlanetAtlas};
use std::path::{Path};

pub(in crate::planet_atlas::diagnostics) fn export_country_adjacency(atlas: &PlanetAtlas, path: &Path) -> Result<(), AtlasError> {
    let mut out = String::from(
        "country_id,neighbor_id,pass_face,pass_u,pass_v,barrier,heart_face,heart_u,heart_v\n",
    );
    for country in &atlas.biomes.countries {
        for route in &country.routes {
            if country.id >= route.neighbor_id {
                continue;
            }
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{},{}\n",
                country.id,
                route.neighbor_id,
                route.pass.face.name(),
                route.pass.u,
                route.pass.v,
                route.barrier,
                country.heart_site.face.name(),
                country.heart_site.u,
                country.heart_site.v,
            ));
        }
    }
    crate::persist::atomic_write(path, out.as_bytes(), false)?;
    Ok(())
}
