//! Registration owns texture allocation until its outputs join the registry.

use std::collections::HashMap;
use std::path::PathBuf;
use crate::registry::Registry;

pub(super) struct Textures {
    slots: HashMap<String, u16>,
    next: u16,
    names: Vec<(String, u16)>,
    files: Vec<(u16, PathBuf)>,
}

impl Default for Textures {
    fn default() -> Self {
        Self { slots: crate::atlas::builtin_slots(), next: crate::atlas::FIRST_FREE_SLOT,
            names: Vec::new(), files: Vec::new() }
    }
}

impl Textures {
    pub(super) fn resolve(&mut self, spec: &str, mod_path: &Option<PathBuf>, errs: &mut Vec<String>) -> u16 {
        if let Some(name) = spec.strip_prefix('@') {
            return *self.slots.get(name).unwrap_or_else(|| {
                errs.push(format!("unknown builtin texture @{name}"));
                &crate::atlas::UNKNOWN_SLOT
            });
        }
        let key = format!(
            "{}/{}",
            mod_path
                .as_deref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            spec
        );
        if let Some(s) = self.slots.get(&key) {
            return *s;
        }
        let Some(dir) = mod_path else {
            errs.push(format!("texture {spec} needs a mod directory"));
            return crate::atlas::UNKNOWN_SLOT;
        };
        let path = dir.join("textures").join(spec);
        let stem = spec.strip_suffix(".png").unwrap_or(spec);
        let embedded =
            dir.as_os_str() == "base" && crate::atlas::embedded_base_tile(stem).is_some();
        if !path.exists() && !embedded {
            errs.push(format!("missing texture {spec}"));
            return crate::atlas::UNKNOWN_SLOT;
        }
        // Mod tiles own FIRST_FREE_SLOT up to the reserved player
        // rows at the top of the 32-wide atlas (a stale 256 cap from
        // the 16-wide era once lived here).
        if self.next >= crate::style::EXTRA_BASE {
            errs.push("texture atlas full".into());
            return crate::atlas::UNKNOWN_SLOT;
        }
        let slot = self.next;
        self.next += 1;
        self.slots.insert(key, slot);
        let mod_id = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let stem = spec.strip_suffix(".png").unwrap_or(spec);
        self.names.push((format!("{mod_id}/{stem}"), slot));
        self.files.push((slot, path));
        slot
    }

    pub(super) fn publish(self, registry: &mut Registry) {
        registry.tex_names = self.names;
        registry.tex_files = self.files;
    }
}
