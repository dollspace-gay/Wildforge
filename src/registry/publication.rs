//! A complete content candidate must pass one gate before runtime publication.

use std::fmt;
use std::path::Path;

use super::Registry;

#[derive(Debug)]
pub struct ContentErrors {
    diagnostics: Vec<String>,
}

impl ContentErrors {
    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    fn result(diagnostics: Vec<String>) -> Result<(), Self> {
        if diagnostics.is_empty() {
            Ok(())
        } else {
            Err(Self { diagnostics })
        }
    }
}

impl fmt::Display for ContentErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("content validation failed:")?;
        for diagnostic in &self.diagnostics {
            write!(formatter, "\n{diagnostic}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ContentErrors {}

/// Runtime entry points use this; `load` remains available to content inspectors
/// that need rejected provider records. No candidate escapes on failure.
pub fn load_validated(mods_dir: &Path) -> Result<Registry, ContentErrors> {
    let candidate = super::load(mods_dir);
    candidate.validate()?;
    Ok(candidate)
}

impl Registry {
    /// Include provider/parse failures as well as both graph-validation families.
    pub fn validate(&self) -> Result<(), ContentErrors> {
        ContentErrors::result(self.diagnostics())
    }

    fn diagnostics(&self) -> Vec<String> {
        self.mods
            .iter()
            .filter_map(|provider| {
                provider
                    .error
                    .as_ref()
                    .map(|error| format!("{}: {error}", provider.id))
            })
            .chain(self.material_errors.iter().cloned())
            .chain(self.arcane_errors.iter().cloned())
            .collect()
    }

    /// Live physical identities and accepted quests require an explicit migration
    /// before removal or replacement. Inspection does not mutate either graph.
    pub(crate) fn validate_reload_from(
        &self,
        previous: &Registry,
        accepted_quests: &[String],
    ) -> Result<(), ContentErrors> {
        let mut errors = self.diagnostics();
        for old_item in &previous.items {
            match self.item_id(&old_item.name) {
                Some(item) if self.item(item).materials != old_item.materials => {
                    errors.push(format!(
                        "{} changes live-stack material identity; a migration is required",
                        old_item.name
                    ));
                }
                None if !old_item.materials.is_empty() => errors.push(format!(
                    "{} contains finite material and cannot be removed from a live world",
                    old_item.name
                )),
                _ => {}
            }
        }
        for old_block in &previous.blocks {
            match self.block_id(&old_block.name) {
                Some(block) if self.block(block).materials != old_block.materials => {
                    errors.push(format!(
                        "{} changes live-voxel material identity; a migration is required",
                        old_block.name
                    ));
                }
                None if !old_block.materials.is_empty() => errors.push(format!(
                    "{} contains finite material and requires a persistent placeholder",
                    old_block.name
                )),
                _ => {}
            }
        }
        for id in accepted_quests {
            if !self.quests.iter().any(|quest| &quest.id == id) {
                errors.push(format!(
                    "{id} is accepted in this world and its quest def cannot be removed"
                ));
            }
        }
        ContentErrors::result(errors)
    }
}
