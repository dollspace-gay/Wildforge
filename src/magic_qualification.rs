//! One fail-closed operator gate for the complete magical practice.
//!
//! Subsystem audits remain useful while developing one arc.  Release
//! qualification needs one command that opens the same closed save through
//! every authoritative ledger and refuses a partial success.

use std::path::Path;
use std::time::Instant;

pub const MAGIC_QUALIFICATION_SCHEMA: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MagicQualificationReport {
    pub qualified: bool,
    pub elapsed_micros: u128,
    pub text: String,
}

impl MagicQualificationReport {
    pub fn render(&self) -> &str {
        &self.text
    }
}

pub fn audit_world(world: &Path, mods: &Path) -> Result<MagicQualificationReport, String> {
    let started = Instant::now();
    let registry = crate::registry::load(mods);
    if !registry.arcane_errors.is_empty() {
        return Err(format!(
            "loaded content violates the finite magic contract:\n{}",
            registry.arcane_errors.join("\n")
        ));
    }

    let arcane = crate::arcane::audit_world(world).map_err(|error| error.to_string())?;
    let (geography, geography_custody_matches) =
        crate::arcane_geography::audit_world(world).map_err(|error| error.to_string())?;
    let atlas = crate::planet_atlas::PlanetAtlas::load(world).map_err(|error| error.to_string())?;
    let geography_state = crate::arcane_geography::ArcaneGeography::load(world, &atlas)
        .map_err(|error| error.to_string())?;
    let ecology = crate::arcane_ecology::audit(&registry, &geography_state.dynamic.ecology)?;
    let discovery = crate::discovery::audit_world(world).map_err(|error| error.to_string())?;
    let implements = crate::implements::audit_world(world).map_err(|error| error.to_string())?;
    let workings = crate::workings::audit_world(world).map_err(|error| error.to_string())?;
    let alchemy = crate::alchemy::audit_world(world).map_err(|error| error.to_string())?;
    let dross = crate::dross::audit_world(world)?;
    let water = atlas.water_audit();
    let materials = crate::materials::audit_world(world).map_err(|error| error.to_string())?;

    let ecology_has_required_breadth =
        ecology.sites != 0 && ecology.roles.len() >= 7 && ecology.variants.len() >= 6;
    let qualified = arcane.is_balanced()
        && arcane.external_adjustment == 0
        && geography.is_balanced()
        && geography_custody_matches
        && ecology_has_required_breadth
        && discovery.is_qualified()
        && implements.is_qualified()
        && workings.is_qualified()
        && alchemy.is_qualified()
        && dross.is_balanced()
        && water.unexplained_water_delta_hu == 0
        && water.unexplained_salt_delta == 0
        && materials.is_balanced()
        && materials.is_qualified();
    let elapsed_micros = started.elapsed().as_micros();

    let mut text = format!(
        concat!(
            "Wildforge magic qualification schema {}\n",
            "World: {}\n",
            "Content hash: {:016x}\n",
            "Elapsed: {} us\n\n"
        ),
        MAGIC_QUALIFICATION_SCHEMA,
        world.display(),
        registry.content_hash,
        elapsed_micros,
    );
    text.push_str(&arcane.render());
    text.push('\n');
    text.push_str(&geography.render());
    text.push_str(&format!(
        "Ledger custody matches: {geography_custody_matches}\n\n"
    ));
    text.push_str(&ecology.render());
    text.push_str(&format!(
        "Required ecological breadth: {} roles, {} habitat expressions, status={}\n\n",
        ecology.roles.len(),
        ecology.variants.len(),
        if ecology_has_required_breadth {
            "qualified"
        } else {
            "FAILED"
        }
    ));
    text.push_str(&discovery.render());
    text.push('\n');
    text.push_str(&implements.render());
    text.push('\n');
    text.push_str(&workings.render());
    text.push('\n');
    text.push_str(&alchemy.render());
    text.push('\n');
    text.push_str(&dross.render());
    text.push('\n');
    text.push_str(&format!(
        concat!(
            "Planetary water/salt audit\n",
            "Water expected: {} HU\n",
            "Water accounted: {} HU\n",
            "Water unexplained delta: {} HU\n",
            "Salt expected: {}\n",
            "Salt accounted: {}\n",
            "Salt unexplained delta: {}\n\n",
        ),
        water.expected_water_hu,
        water.current_water_hu,
        water.unexplained_water_delta_hu,
        water.expected_salt_mass,
        water.current_salt_mass,
        water.unexplained_salt_delta,
    ));
    text.push_str(&materials.render());
    text.push_str(&format!(
        "\nMAGIC QUALIFICATION: {}\n",
        if qualified { "PASS" } else { "FAILED" }
    ));

    Ok(MagicQualificationReport {
        qualified,
        elapsed_micros,
        text,
    })
}
