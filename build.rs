//! Embeds the bundled gemini texture pack into the binary so a compiled
//! game ships with it even without a packs/ folder on disk.

use std::env;
use std::fs;
use std::path::Path;

fn walk(dir: &Path, root: &Path, entries: &mut Vec<(String, String)>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut paths: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() {
            walk(&p, root, entries);
        } else if p.extension().is_some_and(|e| e == "png") {
            let name = p.strip_prefix(root).unwrap().with_extension("");
            let name = name.to_string_lossy().replace('\\', "/");
            let abs = fs::canonicalize(&p).unwrap();
            entries.push((name, abs.to_string_lossy().into_owned()));
        }
    }
}

fn main() {
    println!("cargo:rerun-if-changed=packs/gemini/tiles");
    let out = env::var("OUT_DIR").unwrap();
    let mut entries = Vec::new();
    walk(
        Path::new("packs/gemini/tiles"),
        Path::new("packs/gemini/tiles"),
        &mut entries,
    );
    let mut src = String::from(
        "/// (tile name, png bytes) for the built-in gemini pack.\n\
         pub static GEMINI_TILES: &[(&str, &[u8])] = &[\n",
    );
    for (name, path) in &entries {
        src.push_str(&format!("    ({name:?}, include_bytes!({path:?})),\n"));
    }
    src.push_str("];\n");
    fs::write(Path::new(&out).join("gemini_pack.rs"), src).unwrap();
}
