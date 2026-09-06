//! Retain private asset storage with every clone of the registry that uses it.

use super::Registry;
use crate::content_files::AssetSnapshot;
use std::path::Path;
use std::sync::Arc;

impl Registry {
    pub(crate) fn retain_asset_snapshot(&mut self, source: &Path, snapshot: AssetSnapshot) {
        for info in &mut self.mods {
            if let Some(path) = &mut info.path
                && let Ok(relative) = path.strip_prefix(source)
            {
                *path = snapshot.root().join(relative);
            }
        }
        for (_, path) in &mut self.tex_files {
            if let Ok(relative) = path.strip_prefix(source) {
                *path = snapshot.root().join(relative);
            }
        }
        // Embedded base paths stay unchanged. Derived Clone shares this Arc,
        // including clones used for saved placeholders and immutable jobs.
        self.asset_snapshot = Some(Arc::new(snapshot));
    }
}
