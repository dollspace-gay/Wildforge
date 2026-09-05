//! Script source selection and hot-reload tree observation. No live publication.

use std::path::PathBuf;
use crate::registry::Registry;

/// (mod id, dir) pairs for mods that ship a main.rhai.
pub(super) fn script_mod_dirs(reg: &Registry) -> Vec<(String, PathBuf)> {
    reg.mods
        .iter()
        .filter(|m| m.has_script && m.error.is_none())
        .filter_map(|m| m.path.clone().map(|p| (m.id.clone(), p)))
        .collect()
}

/// Cheap fingerprint of the mods tree (file count + max mtime) for hot reload.
/// Newest-mtime + file-count stamp over the hot-reloadable content trees
/// (mods/ and packs/); a change re-triggers the 1 s reload poll.
pub(crate) fn content_tree_stamp_of(roots: &[&std::path::Path]) -> u64 {
    fn walk(dir: &std::path::Path, acc: &mut u64, count: &mut u64) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, acc, count);
            } else if let Ok(md) = e.metadata() {
                *count += 1;
                if let Ok(t) = md.modified()
                    && let Ok(d) = t.duration_since(std::time::UNIX_EPOCH)
                {
                    *acc = (*acc).max(d.as_secs() * 1000 + d.subsec_millis() as u64);
                }
            }
        }
    }
    let (mut acc, mut count) = (0u64, 0u64);
    for root in roots {
        walk(root, &mut acc, &mut count);
    }
    acc ^ (count << 48)
}

pub(super) fn content_tree_stamp() -> u64 {
    content_tree_stamp_of(&[std::path::Path::new("mods"), std::path::Path::new("packs")])
}

