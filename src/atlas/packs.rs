//! Texture-pack discovery, layering, PNG I/O, and tile export.

use super::*;

include!(concat!(env!("OUT_DIR"), "/gemini_pack.rs"));

/// Built-in packs compiled into the binary (currently just gemini).
pub fn embedded_pack(id: &str) -> Option<&'static [(&'static str, &'static [u8])]> {
    match id {
        "gemini" if !GEMINI_TILES.is_empty() => Some(GEMINI_TILES),
        _ => None,
    }
}

/// Where an active texture pack's tiles come from: a folder under packs/
/// (editable, hot-reloads) or a table compiled into the binary.
pub enum PackSource {
    Dir(std::path::PathBuf),
    Embedded(&'static [(&'static str, &'static [u8])]),
}

/// Full tile-name -> slot map: built-in names plus mod-registered textures
/// (`<mod_id>/<file stem>` keys, from `Registry.tex_names`).
pub fn tile_names(tex_names: &[(String, u16)]) -> std::collections::HashMap<String, u16> {
    let mut m = builtin_slots();
    for (name, slot) in tex_names {
        m.insert(name.clone(), *slot);
    }
    m
}

/// A discovered texture pack (`packs/<id>/`, optional pack.toml metadata).
#[derive(Clone, Debug)]
pub struct PackInfo {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(serde::Deserialize, Default)]
struct PackToml {
    name: Option<String>,
    description: Option<String>,
}

/// List texture packs under `root`, sorted by id.
pub fn discover_packs_in(root: &std::path::Path) -> Vec<PackInfo> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(root) else {
        return out;
    };
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let id = e.file_name().to_string_lossy().to_string();
        let meta: PackToml = std::fs::read_to_string(p.join("pack.toml"))
            .ok()
            .and_then(|t| toml::from_str(&t).ok())
            .unwrap_or_default();
        out.push(PackInfo {
            name: meta.name.unwrap_or_else(|| id.clone()),
            description: meta.description.unwrap_or_default(),
            id,
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

pub fn discover_packs() -> Vec<PackInfo> {
    let mut packs = discover_packs_in(std::path::Path::new("packs"));
    // The built-in pack is always available, folder or not; a real folder
    // of the same id wins (it's editable and hot-reloads).
    if embedded_pack("gemini").is_some() && !packs.iter().any(|p| p.id == "gemini") {
        packs.push(PackInfo {
            id: "gemini".into(),
            name: "Gemini".into(),
            description: "AI-generated tiles (built in)".into(),
        });
        packs.sort_by(|a, b| a.id.cmp(&b.id));
    }
    packs
}

/// Find recognized tile PNGs under `<pack>/tiles/`: (slot, path), plus
/// warnings for files that match no known tile name.
pub fn scan_pack(
    pack_dir: &std::path::Path,
    names: &std::collections::HashMap<String, u16>,
) -> (Vec<(u16, std::path::PathBuf)>, Vec<String>) {
    fn walk(dir: &std::path::Path, acc: &mut Vec<std::path::PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, acc);
            } else {
                acc.push(p);
            }
        }
    }
    let root = pack_dir.join("tiles");
    let mut all = Vec::new();
    walk(&root, &mut all);
    all.sort();
    let (mut files, mut warns) = (Vec::new(), Vec::new());
    for p in all {
        let rel = p
            .strip_prefix(&root)
            .unwrap_or(&p)
            .to_string_lossy()
            .replace('\\', "/");
        let Some(name) = rel.strip_suffix(".png") else {
            continue;
        };
        match names.get(name) {
            Some(slot) => files.push((*slot, p)),
            None => warns.push(format!("{rel}: no tile named \"{name}\"")),
        }
    }
    (files, warns)
}

/// Build the atlas in layers: procedural/assets base, then mod PNGs, then
/// the active texture pack's tiles last (the explicit user choice wins, but
/// only for tiles the pack ships). Returns (pixels, side px, pack warnings).
pub fn build_atlas(
    tex_files: &[(u16, std::path::PathBuf)],
    pack: Option<PackSource>,
    tex_names: &[(String, u16)],
) -> (Vec<u8>, u32, Vec<String>) {
    let (mut img, px) = load_or_build();
    let tp = px / ATLAS_TILES;
    for (slot, path) in tex_files {
        match load_tile_png(path) {
            Some((data, w, h)) => blit_tile(&mut img, px, tp, *slot, &data, w, h),
            None => eprintln!("atlas: failed to load {}", path.display()),
        }
    }
    let mut warnings = Vec::new();
    match pack {
        Some(PackSource::Dir(dir)) => {
            let names = tile_names(tex_names);
            let (files, warns) = scan_pack(&dir, &names);
            warnings = warns;
            for (slot, path) in files {
                match load_tile_png(&path) {
                    Some((data, w, h)) => blit_tile(&mut img, px, tp, slot, &data, w, h),
                    None => warnings.push(format!("unreadable png: {}", path.display())),
                }
            }
        }
        Some(PackSource::Embedded(tiles)) => {
            let names = tile_names(tex_names);
            for (name, bytes) in tiles {
                // Names the current registry doesn't know (e.g. a mod's
                // tile with that mod removed) skip silently.
                let Some(slot) = names.get(*name) else {
                    continue;
                };
                if let Some((data, w, h)) = load_tile_bytes(bytes) {
                    blit_tile(&mut img, px, tp, *slot, &data, w, h);
                }
            }
        }
        None => {}
    }
    apply_player_variants(&mut img, px);
    if let Ok(dir) = std::env::var("WILDFORGE_EXPORT_TILES") {
        match export_tiles(std::path::Path::new(&dir), &img, px, tex_names) {
            Ok(n) => eprintln!("atlas: exported {n} tiles to {dir}"),
            Err(e) => eprintln!("atlas: tile export failed: {e}"),
        }
    }
    (img, px, warnings)
}

/// Derive the pre-tinted player variant tiles (style.rs layout) from
/// the neutral bases: each palette color multiplies the base tile's
/// pixels into a reserved slot at the top of the atlas. Runs after
/// pack layering, so repainted bases carry their look into every
/// variant; idempotent (reads bases, writes variants).
pub fn apply_player_variants(img: &mut [u8], px: u32) {
    use crate::style;
    let tp = px / ATLAS_TILES;
    let slots = builtin_slots();
    let base = |name: &str| *slots.get(name).unwrap_or(&0);
    let mut tint = |src: u16, dst: u16, c: [f32; 3]| {
        let (sx, sy) = (src as u32 % ATLAS_TILES * tp, src as u32 / ATLAS_TILES * tp);
        let (dx, dy) = (dst as u32 % ATLAS_TILES * tp, dst as u32 / ATLAS_TILES * tp);
        for y in 0..tp {
            for x in 0..tp {
                let si = (((sy + y) * px + sx + x) * 4) as usize;
                let di = (((dy + y) * px + dx + x) * 4) as usize;
                for ch in 0..3 {
                    img[di + ch] = (img[si + ch] as f32 * c[ch]).min(255.0) as u8;
                }
                img[di + 3] = img[si + 3];
            }
        }
    };
    for (i, c) in style::SKIN_TONES.iter().enumerate() {
        let s = style::Style {
            skin: i as u8,
            ..Default::default()
        };
        tint(base("player_skin"), style::skin_tile(&s), *c);
        tint(base("player_face"), style::face_tile(&s), *c);
    }
    for (i, c) in style::HAIR_COLORS.iter().enumerate() {
        let s = style::Style {
            hair: i as u8,
            ..Default::default()
        };
        let cropped = style::Style { hair_style: 1, ..s };
        let long = style::Style { hair_style: 3, ..s };
        tint(
            base("player_hair_cropped"),
            style::hair_tile(&cropped).unwrap(),
            *c,
        );
        tint(base("player_hair"), style::hair_tile(&s).unwrap(), *c);
        tint(
            base("player_hair_long"),
            style::hair_tile(&long).unwrap(),
            *c,
        );
        tint(base("player_hair_top"), style::hair_top_tile(&s), *c);
        for (bi, bname) in [
            (1u8, "player_moustache"),
            (2, "player_beard_trim"),
            (3, "player_beard_full"),
        ] {
            let bs = style::Style { beard: bi, ..s };
            tint(base(bname), style::beard_tile(&bs).unwrap(), *c);
        }
    }
    for (i, c) in style::SHIRT_COLORS.iter().enumerate() {
        let s = style::Style {
            shirt: i as u8,
            ..Default::default()
        };
        tint(base("player_shirt"), style::shirt_tile(&s), *c);
    }
    for (i, c) in style::TROUSER_COLORS.iter().enumerate() {
        let s = style::Style {
            trousers: i as u8,
            ..Default::default()
        };
        tint(base("player_trousers"), style::trouser_tile(&s), *c);
    }
}

/// Dump every named tile as `<dir>/tiles/<name>.png` plus a stub pack.toml —
/// a ready-to-edit texture-pack skeleton. Returns the tile count.
pub fn export_tiles(
    dir: &std::path::Path,
    img: &[u8],
    px: u32,
    tex_names: &[(String, u16)],
) -> Result<usize, Box<dyn std::error::Error>> {
    let tp = px / ATLAS_TILES;
    let mut named: Vec<(String, u16)> = tile_names(tex_names).into_iter().collect();
    named.sort();
    std::fs::create_dir_all(dir)?;
    let toml_path = dir.join("pack.toml");
    if !toml_path.exists() {
        std::fs::write(
            &toml_path,
            "name = \"My Pack\"\ndescription = \"Repaint tiles/, delete what you keep stock\"\n",
        )?;
    }
    for (name, slot) in &named {
        let path = dir.join("tiles").join(format!("{name}.png"));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut tile = vec![0u8; (tp * tp * 4) as usize];
        let tx = (*slot as u32 % ATLAS_TILES) * tp;
        let ty = (*slot as u32 / ATLAS_TILES) * tp;
        let row = (tp * 4) as usize;
        for y in 0..tp {
            let si = (((ty + y) * px + tx) * 4) as usize;
            tile[y as usize * row..(y as usize + 1) * row].copy_from_slice(&img[si..si + row]);
        }
        export_png(&path, &tile, tp)?;
    }
    Ok(named.len())
}

fn load_tile_png(path: &std::path::Path) -> Option<(Vec<u8>, u32, u32)> {
    let f = std::fs::File::open(path).ok()?;
    load_tile_reader(png::Decoder::new(std::io::BufReader::new(f)))
}

fn load_tile_bytes(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    load_tile_reader(png::Decoder::new(std::io::Cursor::new(bytes)))
}

fn load_tile_reader<R: std::io::BufRead + std::io::Seek>(
    dec: png::Decoder<R>,
) -> Option<(Vec<u8>, u32, u32)> {
    let mut reader = dec.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let n = (info.width * info.height) as usize;
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf[..n * 4].to_vec(),
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity(n * 4);
            for p in buf[..n * 3].chunks_exact(3) {
                out.extend_from_slice(&[p[0], p[1], p[2], 255]);
            }
            out
        }
        _ => return None,
    };
    Some((rgba, info.width, info.height))
}

/// Nearest-neighbor blit of an arbitrary-size tile into an atlas slot.
fn blit_tile(img: &mut [u8], atlas_px: u32, tp: u32, slot: u16, src: &[u8], sw: u32, sh: u32) {
    let tx = (slot as u32 % ATLAS_TILES) * tp;
    let ty = (slot as u32 / ATLAS_TILES) * tp;
    for y in 0..tp {
        for x in 0..tp {
            let sx = (x * sw / tp).min(sw - 1);
            let sy = (y * sh / tp).min(sh - 1);
            let si = ((sy * sw + sx) * 4) as usize;
            let di = (((ty + y) * atlas_px + tx + x) * 4) as usize;
            img[di..di + 4].copy_from_slice(&src[si..si + 4]);
        }
    }
}

/// Returns (RGBA8 pixels, atlas side length in px).
pub fn load_or_build() -> (Vec<u8>, u32) {
    if let Some((data, px)) = try_load_png("assets/atlas.png") {
        eprintln!("atlas: loaded assets/atlas.png ({px}x{px})");
        return (data, px);
    }
    let tile_px: u32 = std::env::var("WILDFORGE_TILE_PX")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|v| [16, 32, 64, 128].contains(v))
        .unwrap_or(32);
    let data = build_procedural(tile_px);
    let px = ATLAS_TILES * tile_px;
    if let Ok(path) = std::env::var("WILDFORGE_EXPORT_ATLAS") {
        match export_png(std::path::Path::new(&path), &data, px) {
            Ok(()) => eprintln!("atlas: exported to {path}"),
            Err(e) => eprintln!("atlas: export failed: {e}"),
        }
    }
    (data, px)
}

fn try_load_png(path: &str) -> Option<(Vec<u8>, u32)> {
    let file = std::fs::File::open(path).ok()?;
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.width != info.height || info.width % ATLAS_TILES != 0 {
        eprintln!(
            "atlas: {path} must be square with side a multiple of {ATLAS_TILES} (got {}x{})",
            info.width, info.height
        );
        return None;
    }
    let n = (info.width * info.height) as usize;
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf[..n * 4].to_vec(),
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity(n * 4);
            for p in buf[..n * 3].chunks_exact(3) {
                out.extend_from_slice(&[p[0], p[1], p[2], 255]);
            }
            out
        }
        other => {
            eprintln!("atlas: {path} has unsupported color type {other:?} (use RGB/RGBA)");
            return None;
        }
    };
    Some((rgba, info.width))
}

fn export_png(
    path: &std::path::Path,
    data: &[u8],
    px: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let file = std::fs::File::create(path)?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), px, px);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(data)?;
    Ok(())
}
