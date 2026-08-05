//! Embeds shipped texture assets and the exact source identity used by visual
//! qualification captures.

use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

fn walk(dir: &Path, root: &Path, entries: &mut Vec<(String, String)>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut paths: Vec<_> = rd.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            walk(&path, root, entries);
        } else if path.extension().is_some_and(|extension| extension == "png") {
            let name = path.strip_prefix(root).unwrap().with_extension("");
            let name = name.to_string_lossy().replace('\\', "/");
            let absolute = fs::canonicalize(&path).unwrap();
            entries.push((name, absolute.to_string_lossy().into_owned()));
        }
    }
}

fn embed(dir: &str, static_name: &str, doc: &str, out_file: &str, out: &str) {
    let mut entries = Vec::new();
    walk(Path::new(dir), Path::new(dir), &mut entries);
    let mut source = format!(
        "/// (tile name, png bytes) {doc}\npub static {static_name}: &[(&str, &[u8])] = &[\n"
    );
    for (name, path) in &entries {
        source.push_str(&format!("    ({name:?}, include_bytes!({path:?})),\n"));
    }
    source.push_str("];\n");
    fs::write(Path::new(out).join(out_file), source).unwrap();
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn emit_build_identity() {
    let commit = env::var("GITHUB_SHA")
        .ok()
        .filter(|value| value.len() == 40)
        .or_else(|| git(&["rev-parse", "HEAD"]))
        .unwrap_or_else(|| "unknown".into());
    let dirty = git(&["status", "--porcelain=v1", "--untracked-files=normal"])
        .is_some_and(|status| !status.is_empty());
    println!("cargo:rustc-env=WILDFORGE_BUILD_COMMIT={commit}");
    println!(
        "cargo:rustc-env=WILDFORGE_BUILD_DIRTY={}",
        if dirty { "true" } else { "false" }
    );
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");
    println!("cargo:rerun-if-changed=.git/HEAD");
    if let Some(reference) = git(&["symbolic-ref", "-q", "HEAD"])
        && let Some(path) = reference.strip_prefix("refs/")
    {
        println!("cargo:rerun-if-changed=.git/refs/{path}");
    }
}

fn main() {
    println!("cargo:rerun-if-changed=packs/gemini/tiles");
    println!("cargo:rerun-if-changed=base/textures");
    let out = env::var("OUT_DIR").unwrap();
    // Base-mod tiles ride inside the binary so a copied executable works from
    // any working directory; files on disk still override at load.
    embed(
        "base/textures",
        "BASE_TILES",
        "for the base mod's shipped art.",
        "base_tiles.rs",
        &out,
    );
    embed(
        "packs/gemini/tiles",
        "GEMINI_TILES",
        "for the built-in gemini pack.",
        "gemini_pack.rs",
        &out,
    );
    emit_build_identity();
}
