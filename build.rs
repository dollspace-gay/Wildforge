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

fn embed(dir: &str, static_name: &str, doc: &str, out_file: &str, out: &str) {
    let mut entries = Vec::new();
    walk(Path::new(dir), Path::new(dir), &mut entries);
    let mut src = format!(
        "/// (tile name, png bytes) {doc}\npub static {static_name}: &[(&str, &[u8])] = &[\n"
    );
    for (name, path) in &entries {
        src.push_str(&format!("    ({name:?}, include_bytes!({path:?})),\n"));
    }
    src.push_str("];\n");
    fs::write(Path::new(out).join(out_file), src).unwrap();
}

fn main() {
    println!("cargo:rerun-if-changed=packs/gemini/tiles");
    println!("cargo:rerun-if-changed=base/textures");
    let out = env::var("OUT_DIR").unwrap();
    // Base-mod tiles ride inside the binary so a copied exe works from
    // any working directory; files on disk still override at load.
    embed(
        "base/textures",
        "BASE_TILES",
        "for the base mod's shipped art.",
        "base_tiles.rs",
        &out,
    );
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
