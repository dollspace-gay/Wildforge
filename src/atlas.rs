//! Texture atlas: 16x16 tiles of block/item art.
//!
//! By default the atlas is generated procedurally (resolution set by
//! `WILDFORGE_TILE_PX`, default 32). If `assets/atlas.png` exists it is
//! loaded instead, so hand-drawn or AI-generated packs drop in without a
//! recompile. `WILDFORGE_EXPORT_ATLAS=path.png` dumps the procedural atlas
//! as a template.
//!
//! All UV math in the game uses tile fractions (1/16 of the atlas), so any
//! square atlas whose side is a multiple of 16 works.

pub const ATLAS_TILES: u32 = 32;
/// Atlas slot = row * 16 + col. Rows 0-2 are built-in procedural tiles.
pub const UNKNOWN_SLOT: u16 = 15;
pub const CRACK_SLOT: u16 = 16; // stages 16..=19
pub const FIRST_FREE_SLOT: u16 = 240; // built-ins own slots 0-239 (32-wide grid)

/// Built-in procedural tile names usable as `@name` in mod TOML.
pub fn builtin_slots() -> std::collections::HashMap<String, u16> {
    [
        ("grass_top", 0u16),
        ("grass_side", 1),
        ("dirt", 2),
        ("stone", 3),
        ("cobblestone", 4),
        ("sand", 5),
        ("gravel", 6),
        ("log_side", 7),
        ("log_top", 8),
        ("leaves", 9),
        ("planks", 10),
        ("bedrock", 11),
        ("water", 12),
        ("table_top", 13),
        ("table_side", 14),
        ("stick", 32),
        ("wood_pickaxe", 33),
        ("stone_pickaxe", 34),
        ("wood_axe", 35),
        ("stone_axe", 36),
        ("wood_shovel", 37),
        ("stone_shovel", 38),
        ("snow", 39),
        ("ice", 40),
        ("cactus_side", 41),
        ("cactus_top", 42),
        ("birch_log", 43),
        ("birch_log_top", 44),
        ("birch_leaves", 45),
        ("spruce_log", 46),
        ("spruce_log_top", 47),
        ("spruce_leaves", 48),
        ("jungle_log", 49),
        ("jungle_log_top", 50),
        ("jungle_leaves", 51),
        ("acacia_log", 52),
        ("acacia_log_top", 53),
        ("acacia_leaves", 54),
        ("birch_planks", 55),
        ("spruce_planks", 56),
        ("jungle_planks", 57),
        ("acacia_planks", 58),
        ("copper_ore", 59),
        ("tin_ore", 60),
        ("copper_block", 61),
        ("bronze_block", 62),
        ("furnace", 63),
        ("raw_copper", 64),
        ("raw_tin", 65),
        ("copper_ingot", 66),
        ("tin_ingot", 67),
        ("bronze_ingot", 68),
        ("bronze_blend", 69),
        ("charcoal", 70),
        ("copper_pickaxe", 71),
        ("copper_axe", 72),
        ("copper_shovel", 73),
        ("bronze_pickaxe", 74),
        ("bronze_axe", 75),
        ("bronze_shovel", 76),
        ("farmland", 77),
        ("wheat_young", 78),
        ("wheat_ripe", 79),
        ("carrot_plant", 80),
        ("potato_plant", 81),
        ("bush_fruited", 82),
        ("bush_bare", 83),
        ("mushroom", 84),
        ("bread", 85),
        ("berry", 86),
        ("carrot", 87),
        ("potato", 88),
        ("baked_potato", 89),
        ("roasted_mushroom", 90),
        ("cactus_fruit", 91),
        ("jungle_fruit", 92),
        ("stew", 93),
        ("seeds", 94),
        ("wood_hoe", 96),
        ("stone_hoe", 97),
        ("copper_hoe", 98),
        ("bronze_hoe", 99),
        ("deer", 100),
        ("deer_face", 101),
        ("boar", 102),
        ("boar_face", 103),
        ("goat", 104),
        ("goat_face", 105),
        ("grouse", 106),
        ("grouse_face", 107),
        ("rabbit", 108),
        ("rabbit_face", 109),
        ("desert_hare", 110),
        ("snow_hare", 111),
        ("raw_venison", 112),
        ("cooked_venison", 113),
        ("raw_boar", 114),
        ("cooked_boar", 115),
        ("raw_chevon", 116),
        ("cooked_chevon", 117),
        ("raw_fowl", 118),
        ("cooked_fowl", 119),
        ("raw_rabbit", 120),
        ("cooked_rabbit", 121),
        ("hide", 122),
        ("leather", 123),
        ("feather", 124),
        ("hearty_stew", 125),
        ("wood_sword", 126),
        ("stone_sword", 127),
        ("copper_sword", 128),
        ("bronze_sword", 129),
        ("antler", 130),
        ("torch", 131),
        ("chest_side", 132),
        ("chest_top", 133),
        ("thornling", 134),
        ("dryad", 135),
        ("dryad_face", 136),
        ("emberkin", 137),
        ("rimewisp", 138),
        ("gravelurk", 139),
        ("wrathwood", 140),
        ("wrathwood_face", 141),
        ("thorn_bolt", 142),
        ("ember_bolt", 143),
        ("frost_bolt", 144),
        ("plant_fiber", 145),
        ("living_wood", 146),
        ("ember", 147),
        ("frost_shard", 148),
        ("heartwood", 149),
        ("hunting_bow", 150),
        ("warbow", 151),
        ("arrow", 152),
        ("leather_helmet", 153),
        ("leather_chestplate", 154),
        ("leather_leggings", 155),
        ("leather_boots", 156),
        ("bronze_helmet", 157),
        ("bronze_chestplate", 158),
        ("bronze_leggings", 159),
        ("bronze_boots", 160),
        ("oak_sapling", 161),
        ("birch_sapling", 162),
        ("spruce_sapling", 163),
        ("jungle_sapling", 164),
        ("acacia_sapling", 165),
        ("offering_stone", 166),
        ("bedroll", 167),
        ("iron_ore", 168),
        ("iron_block", 169),
        ("steel_block", 170),
        ("raw_iron", 171),
        ("iron_ingot", 172),
        ("steel_blend", 173),
        ("steel_ingot", 174),
        ("iron_pickaxe", 175),
        ("iron_axe", 176),
        ("iron_shovel", 177),
        ("iron_hoe", 178),
        ("iron_sword", 179),
        ("steel_pickaxe", 180),
        ("steel_axe", 181),
        ("steel_shovel", 182),
        ("steel_hoe", 183),
        ("steel_sword", 184),
        ("iron_helmet", 185),
        ("iron_chestplate", 186),
        ("iron_leggings", 187),
        ("iron_boots", 188),
        ("steel_helmet", 189),
        ("steel_chestplate", 190),
        ("steel_leggings", 191),
        ("steel_boots", 192),
        ("shears", 193),
        ("excavation_brush", 194),
        ("mossy_cobblestone", 195),
        ("cracked_masonry", 196),
        ("packed_earth", 197),
        ("old_coin", 198),
        ("etched_tablet", 199),
        ("charm_quiet", 200),
        ("charm_bark", 201),
        ("charm_hunger", 202),
        ("player_shirt", 203),
        ("player_face", 204),
        ("snowball", 205),
        ("rain_streak", 206),
        ("snow_flake", 207),
        ("firebrick", 208),
        ("bloomery", 209),
        ("bloomery_lit", 210),
        ("charcoal_block", 211),
        ("stone_anvil", 212),
        ("steel_bloom", 213),
        ("smith_hammer", 214),
        ("glass", 216),
        ("cobalt_ore", 217),
        ("cinnabar_ore", 218),
        ("manganese_ore", 219),
        ("verdigris_powder", 220),
        ("ochre_powder", 221),
        ("cobalt_powder", 222),
        ("cinnabar_powder", 223),
        ("manganese_powder", 224),
        ("teal_glass", 225),
        ("amber_glass", 226),
        ("blue_glass", 227),
        ("red_glass", 228),
        ("violet_glass", 229),
        ("kiln", 230),
        ("kiln_lit", 231),
        ("quern", 232),
        ("snow_trod", 233),
        ("player_skin", 234),
        ("player_hair", 235),
        ("player_trousers", 236),
        ("player_boot", 237),
        ("player_hair_top", 238),
        ("bucket", 239),
        ("bucket_water", crate::style::EXTRA_BASE + 5),
        // Extra player bases live in the reserved region (style.rs
        // EXTRA_BASE) — the low builtin rows are full.
        ("player_hair_cropped", crate::style::EXTRA_BASE),
        ("player_hair_long", crate::style::EXTRA_BASE + 1),
        ("player_moustache", crate::style::EXTRA_BASE + 2),
        ("player_beard_trim", crate::style::EXTRA_BASE + 3),
        ("player_beard_full", crate::style::EXTRA_BASE + 4),
        ("unknown", 15),
        ("crack1", 16),
        ("crack2", 17),
        ("crack3", 18),
        ("crack4", 19),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

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

/// A companion map: surface data authored alongside a tile's albedo, in its own
/// PNG next to it (`stone.png` + `stone_h.png` + `stone_n.png`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MapKind {
    /// Parallax height. Greyscale: white = surface, black = deepest recess.
    Height,
    /// Tangent-space normal, standard OpenGL / +Y ("green up") encoding.
    Normal,
}

/// Recognize a pack file stem as either a tile or one of its companion maps.
///
/// A real tile name always wins, so a hypothetical tile literally called
/// `foo_n` keeps its slot instead of being read as `foo`'s normal map.
fn classify_tile_file(
    stem: &str,
    names: &std::collections::HashMap<String, u16>,
) -> Option<(u16, Option<MapKind>)> {
    if let Some(slot) = names.get(stem) {
        return Some((*slot, None));
    }
    for (suffix, kind) in [
        ("_h", MapKind::Height),
        ("_height", MapKind::Height),
        ("_n", MapKind::Normal),
        ("_normal", MapKind::Normal),
    ] {
        if let Some(base) = stem.strip_suffix(suffix)
            && let Some(slot) = names.get(base)
        {
            return Some((*slot, Some(kind)));
        }
    }
    None
}

/// What a pack's `tiles/` folder turned out to hold.
#[derive(Default)]
pub struct PackFiles {
    /// Albedo tiles: the slot each PNG repaints.
    pub tiles: Vec<(u16, std::path::PathBuf)>,
    /// Companion maps authored beside an albedo (`stone_n.png`, `stone_h.png`).
    pub maps: Vec<(u16, MapKind, std::path::PathBuf)>,
    /// Files matching no known tile name.
    pub warnings: Vec<String>,
}

/// Find recognized tile PNGs under `<pack>/tiles/`.
pub fn scan_pack(
    pack_dir: &std::path::Path,
    names: &std::collections::HashMap<String, u16>,
) -> PackFiles {
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
    let mut out = PackFiles::default();
    for p in all {
        let rel = p
            .strip_prefix(&root)
            .unwrap_or(&p)
            .to_string_lossy()
            .replace('\\', "/");
        let Some(name) = rel.strip_suffix(".png") else {
            continue;
        };
        match classify_tile_file(name, names) {
            Some((slot, None)) => out.tiles.push((slot, p)),
            Some((slot, Some(kind))) => out.maps.push((slot, kind, p)),
            None => out
                .warnings
                .push(format!("{rel}: no tile named \"{name}\"")),
        }
    }
    out
}

/// Every texture the chunk shader binds, built together so they can never fall
/// out of layout sync. All three share the same tile grid and side length.
pub struct Atlas {
    /// Albedo, sampled as sRGB.
    pub color: Vec<u8>,
    /// Scalar surface data (height / interior / authored-normal flag). Linear.
    pub material: Vec<u8>,
    /// Tangent-space normals, OpenGL encoding. Linear. Flat where unauthored.
    pub normal: Vec<u8>,
    /// Side length of each atlas in px (all three are `px` x `px`).
    pub px: u32,
    pub warnings: Vec<String>,
}

/// Build the atlas in layers: procedural/assets base, then mod PNGs, then
/// the active texture pack's tiles last (the explicit user choice wins, but
/// only for tiles the pack ships). Companion maps (`<tile>_h`, `<tile>_n`) are
/// applied after all albedo layering, so they survive the material reset their
/// own albedo triggers.
pub fn build_atlas(
    tex_files: &[(u16, std::path::PathBuf)],
    pack: Option<PackSource>,
    tex_names: &[(String, u16)],
) -> Atlas {
    let (mut img, px) = load_or_build();
    let tp = px / ATLAS_TILES;
    // The material atlas (parallax height etc.) is procedural + pack-aware: any
    // slot a pack overrides with its own albedo gets its material reset to flat,
    // so our procedural grooves never land under mismatched hand-drawn art.
    let mut mat = build_material(px);
    let mut nrm = build_normal(px);
    // Companion maps found along the way; applied last, over the final albedo.
    let mut maps: Vec<(u16, MapKind, SrcTile)> = Vec::new();
    for (slot, path) in tex_files {
        match load_tile_png(path) {
            Some(src) => {
                blit_tile(&mut img, px, tp, *slot, &src);
                clear_material_slot(&mut mat, px, *slot);
            }
            None => eprintln!("atlas: failed to load {}", path.display()),
        }
    }
    let mut warnings = Vec::new();
    match pack {
        Some(PackSource::Dir(dir)) => {
            let names = tile_names(tex_names);
            let found = scan_pack(&dir, &names);
            warnings = found.warnings;
            for (slot, path) in found.tiles {
                match load_tile_png(&path) {
                    Some(src) => {
                        blit_tile(&mut img, px, tp, slot, &src);
                        clear_material_slot(&mut mat, px, slot);
                    }
                    None => warnings.push(format!("unreadable png: {}", path.display())),
                }
            }
            for (slot, kind, path) in found.maps {
                match load_tile_png(&path) {
                    Some(src) => maps.push((slot, kind, src)),
                    None => warnings.push(format!("unreadable png: {}", path.display())),
                }
            }
        }
        Some(PackSource::Embedded(tiles)) => {
            let names = tile_names(tex_names);
            for (name, bytes) in tiles {
                // Names the current registry doesn't know (e.g. a mod's
                // tile with that mod removed) skip silently.
                let Some((slot, kind)) = classify_tile_file(name, &names) else {
                    continue;
                };
                let Some(src) = load_tile_bytes(bytes) else {
                    continue;
                };
                match kind {
                    None => {
                        blit_tile(&mut img, px, tp, slot, &src);
                        clear_material_slot(&mut mat, px, slot);
                    }
                    Some(kind) => maps.push((slot, kind, src)),
                }
            }
        }
        None => {}
    }
    apply_player_variants(&mut img, px);
    // Authored companion maps land here, after every albedo layer has settled.
    let mut authored_height = std::collections::HashSet::new();
    for (slot, kind, src) in &maps {
        match kind {
            MapKind::Height => {
                blit_height(&mut mat, px, tp, *slot, src);
                authored_height.insert(*slot);
            }
            MapKind::Normal => blit_normal(&mut nrm, &mut mat, px, tp, *slot, src),
        }
    }
    // Rock/masonry get parallax relief from their own albedo luminance, so cave
    // walls and cobble read as 3D and catch light in their crevices. Derived from
    // the final atlas, so it tracks the active pack automatically — but an
    // authored height map is the better source, so it wins.
    for slot in [
        *builtin_slots().get("stone").unwrap_or(&3),
        *builtin_slots().get("cobblestone").unwrap_or(&4),
    ] {
        if !authored_height.contains(&slot) {
            derive_luminance_height(&img, &mut mat, px, slot);
        }
    }
    if let Ok(dir) = std::env::var("WILDFORGE_EXPORT_TILES") {
        match export_tiles(std::path::Path::new(&dir), &img, px, tex_names) {
            Ok(n) => eprintln!("atlas: exported {n} tiles to {dir}"),
            Err(e) => eprintln!("atlas: tile export failed: {e}"),
        }
    }
    Atlas {
        color: img,
        material: mat,
        normal: nrm,
        px,
        warnings,
    }
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

/// A decoded source PNG, at whatever resolution it was authored.
pub struct SrcTile {
    px: Vec<u8>,
    w: u32,
    h: u32,
}

fn load_tile_png(path: &std::path::Path) -> Option<SrcTile> {
    let f = std::fs::File::open(path).ok()?;
    load_tile_reader(png::Decoder::new(std::io::BufReader::new(f)))
}

fn load_tile_bytes(bytes: &[u8]) -> Option<SrcTile> {
    load_tile_reader(png::Decoder::new(std::io::Cursor::new(bytes)))
}

fn load_tile_reader<R: std::io::BufRead + std::io::Seek>(dec: png::Decoder<R>) -> Option<SrcTile> {
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
    Some(SrcTile {
        px: rgba,
        w: info.width,
        h: info.height,
    })
}

/// Resample an arbitrary-size RGBA tile to `tp` x `tp`.
///
/// Upscaling stays **nearest**, which is the whole chunky-pixel look. Downscaling
/// **box-averages** the source texels that map to each destination texel: point
/// sampling a 128px tile into a 32px atlas keeps only 1 texel in 16, which turns
/// fine detail into noise — merely ugly for albedo, but ruinous for a normal map,
/// where the discarded neighbours are exactly what defines the surface and the
/// surviving noise sparkles under a moving light. RGB is averaged weighted by
/// alpha so fully transparent texels can't bleed their color into an edge.
fn resample_tile(dst: &mut [u8], tp: u32, src: &SrcTile) {
    let (sw, sh) = (src.w, src.h);
    let src = &src.px;
    for y in 0..tp {
        for x in 0..tp {
            let (x0, x1) = (
                x * sw / tp,
                ((x + 1) * sw / tp).max(x * sw / tp + 1).min(sw),
            );
            let (y0, y1) = (
                y * sh / tp,
                ((y + 1) * sh / tp).max(y * sh / tp + 1).min(sh),
            );
            let di = ((y * tp + x) * 4) as usize;
            if x1 - x0 <= 1 && y1 - y0 <= 1 {
                let si = ((y0.min(sh - 1) * sw + x0.min(sw - 1)) * 4) as usize;
                dst[di..di + 4].copy_from_slice(&src[si..si + 4]);
                continue;
            }
            let (mut rgb, mut a, mut wsum, mut n) = ([0f32; 3], 0f32, 0f32, 0f32);
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let si = ((sy * sw + sx) * 4) as usize;
                    let w = src[si + 3] as f32;
                    for c in 0..3 {
                        rgb[c] += src[si + c] as f32 * w;
                    }
                    a += w;
                    wsum += w;
                    n += 1.0;
                }
            }
            for c in 0..3 {
                // All-transparent: fall back to the flat mean so the color is
                // still something sane rather than black.
                dst[di + c] = if wsum > 0.0 {
                    (rgb[c] / wsum) as u8
                } else {
                    let sum: f32 = (y0..y1)
                        .flat_map(|sy| (x0..x1).map(move |sx| (sy, sx)))
                        .map(|(sy, sx)| src[(((sy * sw + sx) * 4) as usize) + c] as f32)
                        .sum();
                    (sum / n) as u8
                };
            }
            dst[di + 3] = (a / n) as u8;
        }
    }
}

/// Blit an arbitrary-size tile into an atlas slot, resampled to fit.
fn blit_tile(img: &mut [u8], atlas_px: u32, tp: u32, slot: u16, src: &SrcTile) {
    let tx = (slot as u32 % ATLAS_TILES) * tp;
    let ty = (slot as u32 / ATLAS_TILES) * tp;
    let mut tile = vec![0u8; (tp * tp * 4) as usize];
    resample_tile(&mut tile, tp, src);
    for y in 0..tp {
        for x in 0..tp {
            let si = ((y * tp + x) * 4) as usize;
            let di = (((ty + y) * atlas_px + tx + x) * 4) as usize;
            img[di..di + 4].copy_from_slice(&tile[si..si + 4]);
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

// ---------------- procedural generation ----------------

fn hash(x: i32, y: i32, salt: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x85eb_ca6b) ^ (y as u32).wrapping_mul(0xc2b2_ae35) ^ salt;
    h ^= h >> 13;
    h = h.wrapping_mul(0x2708_92cd);
    h ^= h >> 16;
    h
}

fn h01(x: i32, y: i32, salt: u32) -> f32 {
    (hash(x, y, salt) & 0xffff) as f32 / 65535.0
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Value noise on a lattice that wraps every `period` cells — textures made
/// from it tile seamlessly.
fn vnoise(x: f32, y: f32, period: i32, salt: u32) -> f32 {
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let fx = smooth(x - xi as f32);
    let fy = smooth(y - yi as f32);
    let p = |dx: i32, dy: i32| {
        h01(
            (xi + dx).rem_euclid(period),
            (yi + dy).rem_euclid(period),
            salt,
        )
    };
    let top = p(0, 0) + (p(1, 0) - p(0, 0)) * fx;
    let bot = p(0, 1) + (p(1, 1) - p(0, 1)) * fx;
    top + (bot - top) * fy
}

/// 2-octave tileable fBm in 0..1. `freq` is cells across the tile.
fn fbm(u: f32, v: f32, freq: i32, salt: u32) -> f32 {
    let a = vnoise(u * freq as f32, v * freq as f32, freq, salt);
    let b = vnoise(
        u * freq as f32 * 2.0,
        v * freq as f32 * 2.0,
        freq * 2,
        salt ^ 0x9e37,
    );
    (a * 0.68 + b * 0.32).clamp(0.0, 1.0)
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn rgba(c: [f32; 3], f: f32, a: u8) -> [u8; 4] {
    [
        (c[0] * f).clamp(0.0, 255.0) as u8,
        (c[1] * f).clamp(0.0, 255.0) as u8,
        (c[2] * f).clamp(0.0, 255.0) as u8,
        a,
    ]
}

/// Minecraft-style bevel: light catches the top/left edge, shadow pools at
/// the bottom/right.
fn emboss(px: u32, py: u32, tp: u32) -> f32 {
    let e = (tp / 16).max(1);
    let mut f = 1.0;
    if py < e {
        f *= 1.07;
    } else if py < 2 * e {
        f *= 1.03;
    }
    if py >= tp - e {
        f *= 0.93;
    } else if py >= tp - 2 * e {
        f *= 0.97;
    }
    if px < e {
        f *= 1.03;
    }
    if px >= tp - e {
        f *= 0.95;
    }
    f
}

/// Distances to the two nearest jittered cell points (wrapped) — the basis
/// for cobblestone/gravel. Returns (d1, d2, cell hash of nearest).
fn voronoi(u: f32, v: f32, cells: i32, salt: u32) -> (f32, f32, u32) {
    let x = u * cells as f32;
    let y = v * cells as f32;
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let mut d1 = f32::MAX;
    let mut d2 = f32::MAX;
    let mut id = 0;
    for dy in -1..=1 {
        for dx in -1..=1 {
            let cx = xi + dx;
            let cy = yi + dy;
            let (wx, wy) = (cx.rem_euclid(cells), cy.rem_euclid(cells));
            let jx = cx as f32 + 0.2 + 0.6 * h01(wx, wy, salt);
            let jy = cy as f32 + 0.2 + 0.6 * h01(wx, wy, salt ^ 0xabcd);
            let d = ((x - jx).powi(2) + (y - jy).powi(2)).sqrt();
            if d < d1 {
                d2 = d1;
                d1 = d;
                id = hash(wx, wy, salt ^ 0x77);
            } else if d < d2 {
                d2 = d;
            }
        }
    }
    (d1, d2, id)
}

/// The **material atlas**: a second texture, same tile layout as the color
/// atlas, carrying per-texel surface data (not color). Linear `Rgba8`, so it
/// is NEVER sampled as sRGB.
///
/// Channel meanings (surface normals live in their OWN standard-format atlas —
/// a planned separate texture — so a dropped-in RGB normal map needs no channel
/// surgery; this atlas carries scalar surface data):
///   R = **height** for parallax. 1.0 (255) = surface top, 0.0 = deepest recess.
///       The default fill is 255 (flat), so a tile with no material data is a
///       parallax no-op — the opt-in is per-texture (author a non-flat R).
///   G = **interior/subsurface mask** for multilayer parallax. 0 = none (default).
///       A second stratum (ice bubbles) that parallaxes at a deeper offset than
///       the surface, so the layers slide past each other with the viewpoint.
///   B = **authored-normal strength**. 0 (default) = no authored normal, so the
///       shader falls back to the height-gradient normal it derives itself.
///       > 0 selects the normal atlas at this texel, scaled by this value. It
///       lives here rather than in the normal atlas's own alpha so the shader's
///       "is this tile plain?" early-out stays a single texture read.
///   A = reserved (future scalar: AO / emission-mask).
///
/// Populated procedurally here for built-in tiles that opt in (currently ice).
/// Pack authors will later supply companion maps for their own tiles; until
/// then, and for any pack-overridden slot, `build_atlas` resets the slot to
/// flat so procedural data never lands under a mismatched hand-drawn albedo.
fn material_default() -> [u8; 4] {
    [255, 0, 0, 0]
}

pub fn build_material(px: u32) -> Vec<u8> {
    let tp = px / ATLAS_TILES;
    let atlas_px = ATLAS_TILES * tp;
    let mut img = vec![0u8; (atlas_px * atlas_px * 4) as usize];
    for p in img.chunks_exact_mut(4) {
        p.copy_from_slice(&material_default());
    }
    let mut mtile = |slot: u32, f: &mut dyn FnMut(f32, f32) -> [u8; 4]| {
        let (tx, ty) = (slot % ATLAS_TILES, slot / ATLAS_TILES);
        for py in 0..tp {
            for px in 0..tp {
                let u = (px as f32 + 0.5) / tp as f32;
                let v = (py as f32 + 0.5) / tp as f32;
                let x = tx * tp + px;
                let y = ty * tp + py;
                let i = ((y * atlas_px + x) * 4) as usize;
                img[i..i + 4].copy_from_slice(&f(u, v));
            }
        }
    };

    // Ice (slot 40): the crack veins of the procedural @ice albedo cut down
    // into the surface, so parallax makes them read as real grooves that shift
    // with the viewpoint. Uses the SAME vein noise as the color tile so the
    // recesses land exactly under the visible cracks.
    mtile(40, &mut |u, v| {
        // R = smooth surface. G = the INTERNAL structure layer, present
        // EVERYWHERE (a > 0 floor also flags "this tile has an interior"): a soft
        // cloudy density with brighter fracture veins. Seen through the
        // translucent surface and parallaxed to depth. fbm/vnoise are periodic,
        // so it wraps seamlessly into one continuous layer across blocks.
        let cloud = fbm(u, v, 3, 61);
        let vein = (vnoise(u * 5.0, v * 5.0, 5, 53) - 0.5).abs();
        let crack = if vein < 0.04 { 1.0 - vein / 0.04 } else { 0.0 };
        let g = (0.30 + 0.40 * cloud + 0.60 * crack).min(1.0);
        [255, (g * 255.0) as u8, 0, 0]
    });
    img
}

/// The **normal atlas**: tangent-space surface normals, same tile layout as the
/// color atlas, linear `Rgba8` (never sRGB). Deliberately a plain standard-format
/// texture rather than channels borrowed from the material atlas, so a stock,
/// downloaded, or model-generated normal map drops in with no channel surgery.
///
/// Encoding is the common **OpenGL / +Y ("green up")** convention: green
/// brightens where the surface tilts toward the *top* of the tile image. Since
/// Wildforge stores tiles top-row-first — its bitangent runs *down* the image —
/// the shader negates green on decode. That negation is the entire difference
/// between the two conventions, so a DirectX map is a green flip away
/// (`tools/split_material_sheet.py --flip-green`).
///
/// Default fill is flat (+Z). Which texels are actually *authored* is recorded
/// in the material atlas's B channel, not here; see `build_material`.
fn normal_default() -> [u8; 4] {
    [128, 128, 255, 255]
}

pub fn build_normal(px: u32) -> Vec<u8> {
    let tp = px / ATLAS_TILES;
    let atlas_px = ATLAS_TILES * tp;
    let mut img = vec![0u8; (atlas_px * atlas_px * 4) as usize];
    for p in img.chunks_exact_mut(4) {
        p.copy_from_slice(&normal_default());
    }
    img
}

/// Apply an authored height map (`<tile>_h.png`) to one slot's material R.
/// Greyscale in, so any channel would do; luminance keeps a tinted map sane.
/// G/B/A are left alone — a tile can carry both a height and a normal map.
fn blit_height(mat: &mut [u8], atlas_px: u32, tp: u32, slot: u16, src: &SrcTile) {
    let mut tile = vec![0u8; (tp * tp * 4) as usize];
    resample_tile(&mut tile, tp, src);
    let (tx, ty) = (
        slot as u32 % ATLAS_TILES * tp,
        slot as u32 / ATLAS_TILES * tp,
    );
    for y in 0..tp {
        for x in 0..tp {
            let s = ((y * tp + x) * 4) as usize;
            let d = (((ty + y) * atlas_px + tx + x) * 4) as usize;
            let l =
                0.299 * tile[s] as f32 + 0.587 * tile[s + 1] as f32 + 0.114 * tile[s + 2] as f32;
            mat[d] = l as u8;
        }
    }
}

/// Apply an authored normal map (`<tile>_n.png`) to one slot, and flag those
/// texels in the material atlas's B channel so the shader knows to read it.
fn blit_normal(nrm: &mut [u8], mat: &mut [u8], atlas_px: u32, tp: u32, slot: u16, src: &SrcTile) {
    let mut tile = vec![0u8; (tp * tp * 4) as usize];
    resample_tile(&mut tile, tp, src);
    let (tx, ty) = (
        slot as u32 % ATLAS_TILES * tp,
        slot as u32 / ATLAS_TILES * tp,
    );
    for y in 0..tp {
        for x in 0..tp {
            let s = ((y * tp + x) * 4) as usize;
            let d = (((ty + y) * atlas_px + tx + x) * 4) as usize;
            nrm[d..d + 3].copy_from_slice(&tile[s..s + 3]);
            nrm[d + 3] = 255;
            mat[d + 2] = 255; // full-strength authored normal
        }
    }
}

/// Derive a parallax height field for a tile from its albedo **luminance**
/// (bright = raised surface, dark = recessed). A cheap, general way to give an
/// existing texture relief without authoring a height map — rock/cobble use it so
/// cave walls and masonry catch light in their crevices. Run on the FINAL albedo,
/// so it matches whatever pack tile is showing (no clear-on-override needed).
fn derive_luminance_height(img: &[u8], mat: &mut [u8], px: u32, slot: u16) {
    let tp = px / ATLAS_TILES;
    let (tx, ty) = (
        slot as u32 % ATLAS_TILES * tp,
        slot as u32 / ATLAS_TILES * tp,
    );
    for y in 0..tp {
        for x in 0..tp {
            let i = (((ty + y) * px + tx + x) * 4) as usize;
            let l = 0.299 * img[i] as f32 + 0.587 * img[i + 1] as f32 + 0.114 * img[i + 2] as f32;
            mat[i] = l as u8; // R = height; G/B/A stay flat (no interior layer)
            mat[i + 1] = 0;
        }
    }
}

/// Reset one slot of the material atlas to flat (used when a pack overrides a
/// slot's color, so procedural height never sits under a mismatched albedo).
fn clear_material_slot(mat: &mut [u8], px: u32, slot: u16) {
    let tp = px / ATLAS_TILES;
    let (tx, ty) = (
        slot as u32 % ATLAS_TILES * tp,
        slot as u32 / ATLAS_TILES * tp,
    );
    let d = material_default();
    for y in 0..tp {
        for x in 0..tp {
            let i = (((ty + y) * px + tx + x) * 4) as usize;
            mat[i..i + 4].copy_from_slice(&d);
        }
    }
}

pub fn build_procedural(tp: u32) -> Vec<u8> {
    let atlas_px = ATLAS_TILES * tp;
    let mut img = vec![0u8; (atlas_px * atlas_px * 4) as usize];

    let mut tile = |slot: u32, f: &mut dyn FnMut(u32, u32, f32, f32) -> [u8; 4]| {
        let (tx, ty) = (slot % ATLAS_TILES, slot / ATLAS_TILES);
        for py in 0..tp {
            for px in 0..tp {
                let u = (px as f32 + 0.5) / tp as f32;
                let v = (py as f32 + 0.5) / tp as f32;
                let c = f(px, py, u, v);
                let x = tx * tp + px;
                let y = ty * tp + py;
                let i = ((y * atlas_px + x) * 4) as usize;
                img[i..i + 4].copy_from_slice(&c);
            }
        }
    };

    let speck = |px: u32, py: u32, salt: u32, amt: f32| {
        1.0 + (h01(px as i32, py as i32, salt) - 0.5) * 2.0 * amt
    };

    // (0,0) grass top: mottled two-tone green clumps (no bevel — flat ground
    // shouldn't read as a grid).
    tile(0, &mut |px, py, u, v| {
        let t = fbm(u, v, 5, 1);
        let c = mix3([84.0, 145.0, 47.0], [116.0, 186.0, 64.0], t);
        rgba(c, speck(px, py, 2, 0.07), 255)
    });

    let dirt_at = |px: u32, py: u32, u: f32, v: f32| -> [u8; 4] {
        let t = fbm(u, v, 5, 3);
        let mut c = mix3([112.0, 78.0, 52.0], [148.0, 108.0, 72.0], t);
        // Occasional small stones.
        let (d1, _, id) = voronoi(u, v, 6, 40);
        if d1 < 0.18 && id % 7 == 0 {
            c = [138.0, 134.0, 128.0];
        }
        rgba(c, speck(px, py, 4, 0.09), 255)
    };

    // (2,0) dirt
    tile(2, &mut |px, py, u, v| {
        let mut c = dirt_at(px, py, u, v);
        let f = emboss(px, py, tp);
        c[0] = (c[0] as f32 * f) as u8;
        c[1] = (c[1] as f32 * f) as u8;
        c[2] = (c[2] as f32 * f) as u8;
        c
    });

    // (1,0) grass side: dirt with an irregular turf overhang.
    tile(1, &mut |px, py, u, v| {
        let depth = (0.14 + 0.12 * fbm(u, 0.0, 8, 5)) * tp as f32;
        let fy = py as f32;
        if fy < depth {
            let t = fbm(u, v, 5, 1);
            let c = mix3([80.0, 138.0, 44.0], [108.0, 176.0, 58.0], t);
            rgba(
                c,
                speck(px, py, 6, 0.06) * if py == 0 { 1.12 } else { 1.0 },
                255,
            )
        } else if fy < depth + (tp / 16).max(1) as f32 {
            rgba([70.0, 50.0, 34.0], 1.0, 255) // shadow line under the turf
        } else {
            dirt_at(px, py, u, v)
        }
    });

    // (3,0) stone: soft blotches with darker veins.
    tile(3, &mut |px, py, u, v| {
        let t = fbm(u, v, 4, 7);
        let mut c = mix3([112.0, 112.0, 116.0], [142.0, 142.0, 144.0], t);
        let vein = (vnoise(u * 5.0, v * 5.0, 5, 8) - 0.5).abs();
        if vein < 0.035 {
            c = mix3(c, [70.0, 70.0, 74.0], 0.7);
        }
        rgba(c, speck(px, py, 9, 0.05) * emboss(px, py, tp), 255)
    });

    // (4,0) cobblestone: rounded stones with dark mortar.
    tile(4, &mut |px, py, u, v| {
        let (d1, d2, id) = voronoi(u, v, 4, 10);
        let mortar = d2 - d1 < 0.14;
        if mortar {
            rgba([62.0, 62.0, 62.0], speck(px, py, 11, 0.1), 255)
        } else {
            let tone = 0.82 + (id % 100) as f32 / 100.0 * 0.3;
            // Round shading: bright at stone center, darker toward mortar.
            let dome = 1.08 - d1 * 0.45;
            rgba(
                [128.0, 126.0, 124.0],
                tone * dome * speck(px, py, 12, 0.05),
                255,
            )
        }
    });

    // (5,0) sand: fine grain with soft ripple bands.
    tile(5, &mut |px, py, u, v| {
        let ripple = ((v + fbm(u, v, 3, 13) * 0.25) * std::f32::consts::TAU * 3.0).sin() * 0.05;
        let t = fbm(u, v, 8, 14);
        let c = mix3([206.0, 192.0, 148.0], [228.0, 216.0, 172.0], t);
        rgba(
            c,
            (1.0 + ripple) * speck(px, py, 15, 0.06) * emboss(px, py, tp),
            255,
        )
    });

    // (6,0) gravel: tightly packed pebbles.
    tile(6, &mut |px, py, u, v| {
        let (d1, _, id) = voronoi(u, v, 6, 16);
        let grayish = 0.75 + (id % 100) as f32 / 100.0 * 0.5;
        let warm = (id >> 8) % 4 == 0;
        let base = if warm {
            [134.0, 116.0, 100.0]
        } else {
            [122.0, 120.0, 118.0]
        };
        let dome = 1.05 - d1 * 0.5;
        rgba(base, grayish * dome * speck(px, py, 17, 0.07), 255)
    });

    // (7,0) log side: vertical grain and bark ridges.
    tile(7, &mut |px, py, u, v| {
        let grain =
            ((u * std::f32::consts::TAU * 6.0) + vnoise(v * 4.0, u * 2.0, 4, 18) * 3.0).sin();
        let ridge = vnoise(u * 8.0, v * 2.0, 8, 19);
        let mut f = 0.9 + grain * 0.1;
        if ridge > 0.72 {
            f *= 0.68; // dark bark furrow
        } else if ridge < 0.2 {
            f *= 1.12;
        }
        rgba([106.0, 84.0, 50.0], f * speck(px, py, 20, 0.05), 255)
    });

    // (8,0) log top: wobbling growth rings inside a bark rim.
    tile(8, &mut |px, py, u, v| {
        let dx = u - 0.5;
        let dy = v - 0.5;
        let ang = dy.atan2(dx);
        let wob = vnoise(ang.cos() * 2.0 + 2.0, ang.sin() * 2.0 + 2.0, 4, 21) * 0.06;
        let r = (dx * dx + dy * dy).sqrt() + wob;
        if r > 0.46 {
            rgba([96.0, 76.0, 46.0], speck(px, py, 22, 0.08), 255) // bark rim
        } else {
            let ring = (r * 40.0).sin() * 0.5 + 0.5;
            let c = mix3([196.0, 164.0, 110.0], [156.0, 124.0, 78.0], ring);
            rgba(c, speck(px, py, 23, 0.04), 255)
        }
    });

    // (9,0) leaves: layered greens, deep shadow pockets, sun glints.
    tile(9, &mut |px, py, u, v| {
        let t = fbm(u, v, 6, 24);
        let mut c = mix3([34.0, 78.0, 24.0], [72.0, 136.0, 44.0], t);
        let pocket = h01(px as i32, py as i32, 25);
        if pocket > 0.93 {
            c = mix3(c, [130.0, 190.0, 80.0], 0.8); // glint
        } else if pocket < 0.10 {
            c = mix3(c, [10.0, 26.0, 8.0], 0.7); // shadow hole
        }
        rgba(c, 1.0, 255)
    });

    let plank_colored = |px: u32,
                         py: u32,
                         u: f32,
                         salt: u32,
                         board: [f32; 3],
                         seam: [f32; 3],
                         nail: [f32; 3]|
     -> [u8; 4] {
        let boards = 4;
        let bh = tp / boards;
        let b = py / bh;
        let seam_row = py.is_multiple_of(bh) || py % bh == bh - 1;
        let tone = 0.9 + h01(b as i32, 0, salt) * 0.18;
        let joint_u = h01(b as i32, 1, salt ^ 0x55) * 0.8 + 0.1;
        let at_joint = (u - joint_u).abs() < 0.5 / tp as f32 * 2.0;
        if seam_row || at_joint {
            return rgba(seam, 0.9, 255);
        }
        let grain = (vnoise(u * 9.0, (py as f32 / tp as f32) * 3.0, 9, salt ^ 0x99) - 0.5) * 0.16;
        let mut c = rgba(
            board,
            tone * (1.0 + grain) * speck(px, py, salt ^ 0x77, 0.04),
            255,
        );
        let mid = b * bh + bh / 2;
        let e = (tp / 16).max(1);
        if py.abs_diff(mid) < e && (px < 2 * e && px >= e || px >= tp - 2 * e && px < tp - e) {
            c = rgba(nail, 1.0, 255);
        }
        c
    };

    let plank_at = |px: u32, py: u32, u: f32, _v: f32, salt: u32| -> [u8; 4] {
        let boards = 4;
        let bh = tp / boards;
        let b = py / bh;
        let seam = py.is_multiple_of(bh) || py % bh == bh - 1;
        let tone = 0.9 + h01(b as i32, 0, salt) * 0.18;
        // End-of-board joints staggered per row.
        let joint_u = h01(b as i32, 1, salt ^ 0x55) * 0.8 + 0.1;
        let at_joint = (u - joint_u).abs() < 0.5 / tp as f32 * 2.0;
        if seam || at_joint {
            return rgba([92.0, 70.0, 40.0], 0.9, 255);
        }
        let grain = (vnoise(u * 9.0, (py as f32 / tp as f32) * 3.0, 9, salt ^ 0x99) - 0.5) * 0.16;
        let mut c = rgba(
            [164.0, 132.0, 80.0],
            tone * (1.0 + grain) * speck(px, py, salt ^ 0x77, 0.04),
            255,
        );
        // Nails at board ends.
        let mid = b * bh + bh / 2;
        let e = (tp / 16).max(1);
        if py.abs_diff(mid) < e && (px < 2 * e && px >= e || px >= tp - 2 * e && px < tp - e) {
            c = rgba([80.0, 74.0, 64.0], 1.0, 255);
        }
        c
    };

    // (10,0) planks
    tile(10, &mut |px, py, u, v| plank_at(px, py, u, v, 26));

    // (11,0) bedrock: harsh light/dark blotches.
    tile(11, &mut |px, py, u, v| {
        let t = fbm(u, v, 5, 27);
        let c = if t > 0.55 {
            [120.0, 120.0, 122.0]
        } else if t > 0.4 {
            [84.0, 84.0, 86.0]
        } else {
            [46.0, 46.0, 50.0]
        };
        rgba(c, speck(px, py, 28, 0.08), 255)
    });

    // (12,0) water: soft drifting bands, translucent.
    tile(12, &mut |px, py, u, v| {
        let band = ((v + fbm(u, v, 3, 29) * 0.4) * std::f32::consts::TAU * 2.0).sin() * 0.09;
        let t = fbm(u, v, 4, 30);
        let c = mix3([40.0, 78.0, 196.0], [70.0, 116.0, 236.0], t);
        rgba(c, 1.0 + band + (speck(px, py, 31, 0.03) - 1.0), 168)
    });

    // (13,0) crafting table top: planks with a dark tool-grid border.
    tile(13, &mut |px, py, u, v| {
        let e = (tp / 16).max(1);
        let border = px < e || px >= tp - e || py < e || py >= tp - e;
        let mid = px.abs_diff(tp / 2) < e || py.abs_diff(tp / 2) < e;
        if border || mid {
            rgba([70.0, 54.0, 34.0], speck(px, py, 32, 0.06), 255)
        } else {
            plank_at(px, py, u, v, 33)
        }
    });

    // (14,0) crafting table side: planks, dark top band, two "tool" squares.
    tile(14, &mut |px, py, u, v| {
        let e = (tp / 16).max(1);
        if py < 3 * e {
            return rgba([88.0, 66.0, 40.0], speck(px, py, 34, 0.06), 255);
        }
        let in_sq = |x0: u32, x1: u32| px >= x0 * e && px < x1 * e && py >= 6 * e && py < 10 * e;
        if in_sq(3, 7) || in_sq(9, 13) {
            let edge = px % (4 * e) < e || py % (4 * e) < e;
            let f = if edge { 0.62 } else { 0.85 };
            return rgba([120.0, 96.0, 60.0], f, 255);
        }
        plank_at(px, py, u, v, 35)
    });

    // (39..=42 => row 2 tiles 7..) biome blocks: snow, ice, cactus.
    tile(39, &mut |px, py, u, v| {
        // snow: bright white with faint blue shading.
        let t = fbm(u, v, 6, 50);
        let c = mix3([230.0, 236.0, 244.0], [250.0, 252.0, 255.0], t);
        rgba(c, speck(px, py, 51, 0.03) * emboss(px, py, tp), 255)
    });
    tile(40, &mut |px, py, u, v| {
        // ice: a smooth glacial surface. The cracks are NOT painted here — they
        // live in the material atlas as an internal layer (parallaxed deeper and
        // seen through this surface), so the albedo stays clean and the fractures
        // read as depth, not a flat overlay.
        let t = fbm(u, v, 4, 52);
        let c = mix3([150.0, 190.0, 226.0], [184.0, 214.0, 244.0], t);
        rgba(c, speck(px, py, 54, 0.02) * emboss(px, py, tp), 255)
    });
    tile(41, &mut |px, py, u, _v| {
        // cactus side: vertical ribs with pale spines.
        let rib = ((u * std::f32::consts::TAU * 4.0).sin() * 0.5 + 0.5) * 0.3;
        let c = mix3([44.0, 96.0, 36.0], [88.0, 148.0, 62.0], 0.4 + rib);
        let spine = hash(px as i32, py as i32, 55).is_multiple_of(37);
        if spine {
            rgba([220.0, 228.0, 190.0], 1.0, 255)
        } else {
            rgba(c, speck(px, py, 56, 0.06), 255)
        }
    });
    tile(42, &mut |px, py, u, v| {
        // cactus top: rib ring + pale center.
        let dx = u - 0.5;
        let dy = v - 0.5;
        let r = (dx * dx + dy * dy).sqrt();
        let c = if r < 0.12 {
            [150.0, 190.0, 110.0]
        } else if r < 0.34 {
            [70.0, 128.0, 50.0]
        } else {
            [52.0, 106.0, 40.0]
        };
        rgba(c, speck(px, py, 57, 0.05) * emboss(px, py, tp), 255)
    });

    // Wood families: bark side, ringed top, and leaves per species.
    struct Wood {
        slot: u32,
        bark: [f32; 3],
        birch_flecks: bool,
        leaf_dark: [f32; 3],
        leaf_light: [f32; 3],
    }
    let woods = [
        Wood {
            slot: 43,
            bark: [201.0, 196.0, 182.0],
            birch_flecks: true,
            leaf_dark: [62.0, 110.0, 40.0],
            leaf_light: [112.0, 168.0, 66.0],
        },
        Wood {
            slot: 46,
            bark: [68.0, 50.0, 32.0],
            birch_flecks: false,
            leaf_dark: [26.0, 60.0, 38.0],
            leaf_light: [52.0, 96.0, 66.0],
        },
        Wood {
            slot: 49,
            bark: [94.0, 66.0, 40.0],
            birch_flecks: false,
            leaf_dark: [44.0, 124.0, 26.0],
            leaf_light: [88.0, 188.0, 50.0],
        },
        Wood {
            slot: 52,
            bark: [122.0, 108.0, 92.0],
            birch_flecks: false,
            leaf_dark: [86.0, 102.0, 46.0],
            leaf_light: [128.0, 148.0, 72.0],
        },
    ];
    for wd in woods {
        // Painters take slots directly — families may cross atlas row
        // boundaries (e.g. spruce leaves at slot 48 = row 3).
        let bark = wd.bark;
        let flecks = wd.birch_flecks;
        // Bark side.
        tile(wd.slot, &mut |px, py, u, v| {
            if flecks {
                // Birch: pale bark with short dark horizontal flecks.
                let dash = hash(px as i32 / 5, py as i32, 60).is_multiple_of(11) && px % 5 < 3;
                if dash {
                    return rgba([38.0, 34.0, 30.0], 1.0, 255);
                }
                let t = fbm(u, v, 5, 61);
                rgba(
                    mix3(bark, [170.0, 166.0, 154.0], t * 0.5),
                    speck(px, py, 62, 0.04),
                    255,
                )
            } else {
                let grain = ((u * std::f32::consts::TAU * 6.0)
                    + vnoise(v * 4.0, u * 2.0, 4, 63) * 3.0)
                    .sin();
                let ridge = vnoise(u * 8.0, v * 2.0, 8, 64);
                let mut f = 0.9 + grain * 0.1;
                if ridge > 0.72 {
                    f *= 0.68;
                } else if ridge < 0.2 {
                    f *= 1.12;
                }
                rgba(bark, f * speck(px, py, 65, 0.05), 255)
            }
        });
        // Ringed top.
        tile(wd.slot + 1, &mut |px, py, u, v| {
            let dx = u - 0.5;
            let dy = v - 0.5;
            let ang = dy.atan2(dx);
            let wob = vnoise(ang.cos() * 2.0 + 2.0, ang.sin() * 2.0 + 2.0, 4, 66) * 0.06;
            let r = (dx * dx + dy * dy).sqrt() + wob;
            if r > 0.46 {
                rgba(
                    [bark[0] * 0.9, bark[1] * 0.9, bark[2] * 0.9],
                    speck(px, py, 67, 0.08),
                    255,
                )
            } else {
                let ring = (r * 40.0).sin() * 0.5 + 0.5;
                let light = [
                    (bark[0] * 1.6 + 40.0).min(235.0),
                    (bark[1] * 1.6 + 36.0).min(230.0),
                    (bark[2] * 1.5 + 30.0).min(220.0),
                ];
                let dark = [bark[0] * 1.2, bark[1] * 1.2, bark[2] * 1.15];
                rgba(mix3(light, dark, ring), speck(px, py, 68, 0.04), 255)
            }
        });
        // Leaves.
        tile(wd.slot + 2, &mut |px, py, u, v| {
            let t = fbm(u, v, 6, 69);
            let mut c = mix3(wd.leaf_dark, wd.leaf_light, t);
            let pocket = h01(px as i32, py as i32, 70 + wd.slot);
            if pocket > 0.93 {
                c = mix3(
                    c,
                    [
                        wd.leaf_light[0] + 50.0,
                        wd.leaf_light[1] + 40.0,
                        wd.leaf_light[2] + 30.0,
                    ],
                    0.8,
                );
            } else if pocket < 0.10 {
                c = mix3(c, [8.0, 22.0, 8.0], 0.7);
            }
            rgba(c, 1.0, 255)
        });
    }

    // Per-wood planks (oak planks stay at slot 10).
    let plank_sets: [(u32, [f32; 3], [f32; 3]); 4] = [
        (55, [196.0, 178.0, 138.0], [128.0, 114.0, 86.0]), // birch: pale
        (56, [104.0, 78.0, 48.0], [58.0, 42.0, 26.0]),     // spruce: dark
        (57, [156.0, 106.0, 76.0], [96.0, 60.0, 40.0]),    // jungle: ruddy
        (58, [168.0, 96.0, 54.0], [104.0, 56.0, 30.0]),    // acacia: orange
    ];
    for (slot, board, seam) in plank_sets {
        tile(slot, &mut |px, py, u, _v| {
            plank_colored(px, py, u, 40 + slot, board, seam, [80.0, 74.0, 64.0])
        });
    }

    // Metals: ores (stone + nuggets), polished blocks, items.
    let metal_sets: [(u32, u32, [f32; 3]); 2] = [
        (59, 61, [210.0, 118.0, 52.0]), // copper: ore slot, block slot, color
        (60, 0, [200.0, 204.0, 212.0]), // tin (no block tile; slot 0 unused)
    ];
    for (ore_slot, _blk, color) in metal_sets {
        tile(ore_slot, &mut |px, py, u, v| {
            // Stone base with metal nugget clusters.
            let t = fbm(u, v, 4, 7);
            let base = mix3([112.0, 112.0, 116.0], [142.0, 142.0, 144.0], t);
            let (d1, _, id) = voronoi(u, v, 5, 90 + ore_slot);
            if d1 < 0.22 && id % 3 == 0 {
                rgba(color, 0.8 + (id % 40) as f32 / 100.0, 255)
            } else {
                rgba(base, speck(px, py, 91, 0.05), 255)
            }
        });
    }
    // Polished metal blocks: copper 61, bronze 62.
    for (slot, color) in [(61u32, [206.0, 116.0, 52.0]), (62, [196.0, 148.0, 62.0])] {
        tile(slot, &mut |px, py, u, v| {
            let t = fbm(u, v, 3, 92);
            let c = mix3(color, [color[0] * 0.8, color[1] * 0.8, color[2] * 0.8], t);
            rgba(c, speck(px, py, 93, 0.04) * emboss(px, py, tp), 255)
        });
    }
    // Furnace side: cobble with a dark mouth.
    tile(63, &mut |px, py, u, v| {
        let mouth = u > 0.28 && u < 0.72 && v > 0.42 && v < 0.86;
        if mouth {
            let glow = ((u - 0.5).abs() < 0.14 && v > 0.55) as u8;
            if glow == 1 {
                return rgba([230.0, 120.0, 30.0], 0.9, 255);
            }
            return rgba([28.0, 24.0, 22.0], 1.0, 255);
        }
        let (d1, d2, id) = voronoi(u, v, 4, 10);
        if d2 - d1 < 0.14 {
            rgba([62.0, 62.0, 62.0], speck(px, py, 11, 0.1), 255)
        } else {
            let tone = 0.82 + (id % 100) as f32 / 100.0 * 0.3;
            rgba([128.0, 126.0, 124.0], tone * (1.08 - d1 * 0.45), 255)
        }
    });
    // Item icons row 4: raw lumps, ingots, blend, charcoal.
    #[allow(clippy::type_complexity)]
    let lump =
        |slot: u32,
         color: [f32; 3],
         img: &mut dyn FnMut(u32, &mut dyn FnMut(u32, u32, f32, f32) -> [u8; 4])| {
            img(slot, &mut |px, py, u, v| {
                let dx = u - 0.5;
                let dy = v - 0.5;
                let r = (dx * dx + dy * dy).sqrt();
                let wob = h01(px as i32 / 3, py as i32 / 3, 94 + slot) * 0.1;
                if r < 0.3 + wob {
                    rgba(
                        color,
                        0.75 + h01(px as i32, py as i32, 95 + slot) * 0.45,
                        255,
                    )
                } else {
                    [0, 0, 0, 0]
                }
            });
        };
    let mut tile_fn = |slot: u32, f: &mut dyn FnMut(u32, u32, f32, f32) -> [u8; 4]| tile(slot, f);
    lump(64, [200.0, 112.0, 50.0], &mut tile_fn); // raw copper
    lump(65, [196.0, 200.0, 208.0], &mut tile_fn); // raw tin
    lump(69, [186.0, 138.0, 70.0], &mut tile_fn); // bronze blend (powder pile)
    lump(70, [36.0, 32.0, 30.0], &mut tile_fn); // charcoal
    // Ingots: beveled bars.
    for (slot, color) in [
        (66u32, [214.0, 122.0, 56.0]),
        (67, [206.0, 210.0, 218.0]),
        (68, [200.0, 152.0, 64.0]),
    ] {
        tile(slot, &mut |px, py, u, v| {
            let inside = u > 0.12 && u < 0.88 && v > 0.36 && v < 0.68;
            if !inside {
                return [0, 0, 0, 0];
            }
            let top = v < 0.44 || u < 0.18;
            let bot = v > 0.6 || u > 0.82;
            let f = if top {
                1.25
            } else if bot {
                0.7
            } else {
                1.0
            };
            rgba(color, f * speck(px, py, 96 + slot, 0.03), 255)
        });
    }
    // Metal tools reuse the pixel art with tier head colors.
    let metal_tools: [(u32, &[&str; 16], [f32; 3]); 6] = [
        (71, &PICK_ART, [206.0, 116.0, 52.0]),
        (72, &AXE_ART, [206.0, 116.0, 52.0]),
        (73, &SHOVEL_ART, [206.0, 116.0, 52.0]),
        (74, &PICK_ART, [196.0, 148.0, 62.0]),
        (75, &AXE_ART, [196.0, 148.0, 62.0]),
        (76, &SHOVEL_ART, [196.0, 148.0, 62.0]),
    ];
    let k2 = (tp / 16).max(1);
    for (slot, art, head) in metal_tools {
        tile(slot, &mut |px, py, _u, _v| {
            let ax = (px / k2).min(15) as usize;
            let ay = (py / k2).min(15) as usize;
            match art[ay].as_bytes().get(ax) {
                Some(b'H') => {
                    let f = 0.85 + h01(ax as i32, ay as i32, 300 + slot) * 0.2;
                    rgba(head, f, 255)
                }
                Some(b'h') => rgba([104.0, 72.0, 42.0], 1.0, 255),
                _ => [0, 0, 0, 0],
            }
        });
    }

    let mut tf = |slot: u32, f: &mut dyn FnMut(u32, u32, f32, f32) -> [u8; 4]| tile(slot, f);
    // Farmland: dark tilled rows.
    tf(77, &mut |px, py, u, v| {
        let row = ((v * 8.0) as u32).is_multiple_of(2);
        let t = fbm(u, v, 5, 120);
        let c = mix3([78.0, 54.0, 36.0], [102.0, 72.0, 48.0], t);
        rgba(
            c,
            if row { 0.75 } else { 1.0 } * speck(px, py, 121, 0.08),
            255,
        )
    });
    // Plant sprites: stems with leaves/heads, transparent bg.
    #[allow(clippy::type_complexity)]
    let plant =
        |slot: u32,
         stem: [f32; 3],
         head: Option<[f32; 3]>,
         height: f32,
         tile: &mut dyn FnMut(u32, &mut dyn FnMut(u32, u32, f32, f32) -> [u8; 4])| {
            tile(slot, &mut |px, py, u, v| {
                let col = (u * 5.0) as i32;
                let cx = 0.1 + col as f32 * 0.2 + h01(col, 0, 130 + slot) * 0.08;
                let stem_here = (u - cx).abs() < 0.035 && v > 1.0 - height;
                let head_here = head.is_some()
                    && (u - cx).abs() < 0.09
                    && v > 1.0 - height
                    && v < 1.0 - height + 0.3;
                if head_here {
                    rgba(
                        head.unwrap(),
                        0.85 + h01(px as i32, py as i32, 131) * 0.3,
                        255,
                    )
                } else if stem_here {
                    rgba(stem, 0.85 + h01(px as i32, py as i32, 132) * 0.3, 255)
                } else {
                    [0, 0, 0, 0]
                }
            });
        };
    plant(78, [90.0, 160.0, 60.0], None, 0.5, &mut tf); // young wheat
    plant(
        79,
        [200.0, 170.0, 70.0],
        Some([222.0, 190.0, 90.0]),
        0.9,
        &mut tf,
    ); // ripe wheat
    // Carrot: leafy fan with orange crowns peeking at the soil line.
    tf(80, &mut |px, py, u, v| {
        let fan = (v > 0.45) && ((u - 0.5).abs() < (v - 0.4) * 0.65);
        let crown = v > 0.88 && ((u - 0.3).abs() < 0.05 || (u - 0.7).abs() < 0.05);
        if crown {
            rgba([225.0, 120.0, 40.0], 1.0, 255)
        } else if fan && !hash(px as i32, py as i32, 170).is_multiple_of(4) {
            rgba(
                [70.0, 150.0, 55.0],
                0.8 + h01(px as i32, py as i32, 171) * 0.4,
                255,
            )
        } else {
            [0, 0, 0, 0]
        }
    });
    // Potato: low bushy clump with white blossoms.
    tf(81, &mut |px, py, u, v| {
        let dx = u - 0.5;
        let dy = v - 0.75;
        if dx * dx + dy * dy * 2.2 < 0.09 {
            if hash(px as i32, py as i32, 172).is_multiple_of(17) {
                return rgba([235.0, 235.0, 220.0], 1.0, 255);
            }
            let t = fbm(u, v, 6, 173);
            rgba(mix3([55.0, 110.0, 45.0], [90.0, 150.0, 65.0], t), 1.0, 255)
        } else {
            [0, 0, 0, 0]
        }
    });
    // Bushes: leafy blob, fruited variant with red dots.
    for (slot, fruited) in [(82u32, true), (83, false)] {
        tf(slot, &mut |px, py, u, v| {
            let dx = u - 0.5;
            let dy = v - 0.62;
            if dx * dx + dy * dy < 0.14 {
                if fruited && hash(px as i32, py as i32, 140).is_multiple_of(13) {
                    return rgba([210.0, 40.0, 60.0], 1.0, 255);
                }
                let t = fbm(u, v, 6, 141);
                rgba(mix3([40.0, 90.0, 30.0], [70.0, 130.0, 50.0], t), 1.0, 255)
            } else {
                [0, 0, 0, 0]
            }
        });
    }
    // Mushroom sprite.
    tf(84, &mut |px, py, u, v| {
        let cap = (u - 0.5).abs() < 0.28 && v > 0.35 && v < 0.62;
        let stem = (u - 0.5).abs() < 0.08 && (0.62..0.95).contains(&v);
        if cap {
            rgba(
                [170.0, 90.0, 60.0],
                0.85 + h01(px as i32, py as i32, 150) * 0.3,
                255,
            )
        } else if stem {
            rgba([225.0, 215.0, 195.0], 1.0, 255)
        } else {
            [0, 0, 0, 0]
        }
    });
    // Food lumps.
    lump(85, [205.0, 160.0, 90.0], &mut tf); // bread
    lump(86, [200.0, 45.0, 70.0], &mut tf); // berry
    lump(87, [225.0, 120.0, 40.0], &mut tf); // carrot
    lump(88, [190.0, 160.0, 105.0], &mut tf); // potato
    lump(89, [222.0, 186.0, 120.0], &mut tf); // baked potato
    lump(90, [150.0, 95.0, 60.0], &mut tf); // roasted mushroom
    lump(91, [220.0, 90.0, 130.0], &mut tf); // cactus fruit
    lump(92, [235.0, 190.0, 60.0], &mut tf); // jungle fruit
    lump(93, [160.0, 110.0, 60.0], &mut tf); // stew
    // Seeds: cluster of dark-green kernels, readable on grass.
    tf(94, &mut |px, py, u, v| {
        let dx = u - 0.5;
        let dy = v - 0.5;
        if dx * dx + dy * dy < 0.11 && !hash(px as i32 / 2, py as i32 / 2, 180).is_multiple_of(3) {
            rgba(
                [28.0, 62.0, 22.0],
                0.8 + h01(px as i32, py as i32, 181) * 0.5,
                255,
            )
        } else {
            [0, 0, 0, 0]
        }
    });
    // Hoes: reuse shovel-ish art with thin blade — use SHOVEL_ART with tier colors.
    let hoe_sets: [(u32, [f32; 3]); 4] = [
        (96, [168.0, 122.0, 60.0]),
        (97, [130.0, 130.0, 130.0]),
        (98, [206.0, 116.0, 52.0]),
        (99, [196.0, 148.0, 62.0]),
    ];
    for (slot, head) in hoe_sets {
        tf(slot, &mut |px, py, _u, _v| {
            let ax = (px / k2).min(15) as usize;
            let ay = (py / k2).min(15) as usize;
            match AXE_ART[ay].as_bytes().get(ax) {
                Some(b'H') if ay <= 3 => {
                    let f = 0.85 + h01(ax as i32, ay as i32, 160 + slot) * 0.2;
                    rgba(head, f, 255)
                }
                Some(b'H') => [0, 0, 0, 0],
                Some(b'h') => rgba([104.0, 72.0, 42.0], 1.0, 255),
                _ => [0, 0, 0, 0],
            }
        });
    }

    // ---- animals: fur/hide tiles + faces (rows 6-7) ----
    let furs: [(u32, [f32; 3], [f32; 3], f32); 7] = [
        (100, [150.0, 105.0, 65.0], [120.0, 80.0, 50.0], 0.08), // deer
        (102, [95.0, 80.0, 70.0], [70.0, 58.0, 50.0], 0.22),    // boar (bristly)
        (104, [215.0, 210.0, 200.0], [175.0, 170.0, 160.0], 0.10), // goat
        (106, [140.0, 100.0, 70.0], [95.0, 70.0, 50.0], 0.10),  // grouse
        (108, [170.0, 140.0, 110.0], [140.0, 110.0, 85.0], 0.08), // rabbit
        (110, [205.0, 180.0, 135.0], [175.0, 150.0, 110.0], 0.08), // desert hare
        (111, [235.0, 235.0, 240.0], [205.0, 205.0, 215.0], 0.05), // snow hare
    ];
    for (slot, hi, lo, rough) in furs {
        tf(slot, &mut |px, py, u, v| {
            let t = fbm(u, v, 6, 400 + slot);
            let mut c = mix3(lo, hi, t);
            // Grouse: light feather dapple.
            if slot == 106 {
                let (d1, _, id) = voronoi(u, v, 6, 410);
                if d1 < 0.16 && id % 3 == 0 {
                    c = [200.0, 180.0, 140.0];
                }
            }
            rgba(c, speck(px, py, 420 + slot, rough), 255)
        });
    }
    // Faces: fur base + eyes + species snout.
    for (slot, fur_slot) in [
        (101u32, 100u32),
        (103, 102),
        (105, 104),
        (107, 106),
        (109, 108),
    ] {
        tf(slot, &mut |px, py, u, v| {
            let base_t = fbm(u, v, 6, 400 + fur_slot);
            let (hi, lo) = match fur_slot {
                100 => ([150.0, 105.0, 65.0], [120.0, 80.0, 50.0]),
                102 => ([95.0, 80.0, 70.0], [70.0, 58.0, 50.0]),
                104 => ([215.0, 210.0, 200.0], [175.0, 170.0, 160.0]),
                106 => ([140.0, 100.0, 70.0], [95.0, 70.0, 50.0]),
                _ => ([170.0, 140.0, 110.0], [140.0, 110.0, 85.0]),
            };
            let mut c = mix3(lo, hi, base_t);
            let eye = |cx: f32, cy: f32, w: f32, h: f32| (u - cx).abs() < w && (v - cy).abs() < h;
            // Eyes (goat gets wide horizontal pupils).
            let (ew, eh) = if fur_slot == 104 {
                (0.09, 0.045)
            } else {
                (0.055, 0.06)
            };
            if eye(0.28, 0.35, ew, eh) || eye(0.72, 0.35, ew, eh) {
                c = [15.0, 12.0, 10.0];
            }
            match fur_slot {
                100 => {
                    // Deer: dark nose.
                    if (u - 0.5).abs() < 0.10 && v > 0.82 {
                        c = [30.0, 22.0, 18.0];
                    }
                }
                102 => {
                    // Boar: pink snout disc + nostrils + tusks.
                    let dx = u - 0.5;
                    let dy = v - 0.78;
                    if dx * dx + dy * dy * 1.6 < 0.030 {
                        c = [214.0, 140.0, 130.0];
                        if eye(0.42, 0.78, 0.03, 0.035) || eye(0.58, 0.78, 0.03, 0.035) {
                            c = [120.0, 60.0, 55.0];
                        }
                    }
                    if eye(0.16, 0.85, 0.045, 0.06) || eye(0.84, 0.85, 0.045, 0.06) {
                        c = [235.0, 230.0, 215.0];
                    }
                }
                104 => {
                    // Goat: grey muzzle.
                    if v > 0.75 {
                        c = mix3(c, [150.0, 145.0, 138.0], 0.7);
                    }
                }
                106 => {
                    // Grouse: orange beak wedge.
                    if v > 0.62 && (u - 0.5).abs() < (0.95 - v) * 0.45 {
                        c = [210.0, 130.0, 40.0];
                    }
                }
                _ => {
                    // Rabbit: pink nose.
                    if (u - 0.5).abs() < 0.06 && (v - 0.72).abs() < 0.05 {
                        c = [220.0, 130.0, 130.0];
                    }
                }
            }
            rgba(c, speck(px, py, 430 + slot, 0.06), 255)
        });
    }
    // Meats: slab with fat marbling (raw) or browned surface + char rim (cooked).
    let meats: [(u32, [f32; 3], bool); 10] = [
        (112, [165.0, 45.0, 55.0], false), // venison
        (113, [125.0, 75.0, 45.0], true),
        (114, [210.0, 110.0, 120.0], false), // boar
        (115, [170.0, 105.0, 60.0], true),
        (116, [180.0, 60.0, 65.0], false), // chevon
        (117, [140.0, 85.0, 50.0], true),
        (118, [225.0, 170.0, 160.0], false), // fowl
        (119, [205.0, 140.0, 75.0], true),
        (120, [215.0, 140.0, 135.0], false), // rabbit
        (121, [185.0, 120.0, 70.0], true),
    ];
    for (slot, base, cooked) in meats {
        tf(slot, &mut |px, py, u, v| {
            let dx = (u - 0.5) * 1.15;
            let dy = (v - 0.55) * 1.5;
            let d = dx * dx + dy * dy;
            if d > 0.16 {
                return [0, 0, 0, 0];
            }
            let mut c = base;
            let m = fbm(u * 2.0, v * 2.0, 5, 500 + slot);
            if !cooked && m > 0.62 {
                c = [235.0, 225.0, 220.0]; // fat marbling
            }
            if cooked {
                c = mix3(c, [90.0, 55.0, 30.0], (m - 0.4).clamp(0.0, 1.0) * 0.5);
                if d > 0.11 {
                    c = mix3(c, [60.0, 38.0, 22.0], 0.7); // char rim
                }
            }
            rgba(c, speck(px, py, 510 + slot, 0.08), 255)
        });
    }
    // Hide: rough pelt rectangle with darker border.
    tf(122, &mut |px, py, u, v| {
        let dx = (u - 0.5).abs();
        let dy = (v - 0.5).abs();
        if dx > 0.38 || dy > 0.32 || (dx > 0.30 && dy > 0.24) {
            return [0, 0, 0, 0];
        }
        let t = fbm(u, v, 5, 520);
        let mut c = mix3([120.0, 85.0, 55.0], [155.0, 115.0, 75.0], t);
        if dx > 0.32 || dy > 0.26 {
            c = mix3(c, [80.0, 55.0, 35.0], 0.7);
        }
        rgba(c, speck(px, py, 521, 0.12), 255)
    });
    // Leather: smooth tanned rectangle.
    tf(123, &mut |px, py, u, v| {
        let dx = (u - 0.5).abs();
        let dy = (v - 0.5).abs();
        if dx > 0.36 || dy > 0.30 || (dx > 0.28 && dy > 0.22) {
            return [0, 0, 0, 0];
        }
        let t = fbm(u, v, 4, 530);
        rgba(
            mix3([170.0, 120.0, 70.0], [195.0, 145.0, 90.0], t),
            speck(px, py, 531, 0.05),
            255,
        )
    });
    // Feather: diagonal quill with pale barbs.
    tf(124, &mut |px, py, u, v| {
        // Line from (0.22, 0.82) to (0.78, 0.18).
        let t = ((u - 0.22) * 0.5 + (0.82 - v) * 0.5).clamp(0.0, 1.0);
        let (lx, ly) = (0.22 + 0.56 * t, 0.82 - 0.64 * t);
        let dist = ((u - lx) * (u - lx) + (v - ly) * (v - ly)).sqrt();
        let width = 0.16 * (1.0 - t * 0.8);
        if dist < width * 0.22 {
            rgba([150.0, 150.0, 155.0], 1.0, 255) // shaft
        } else if dist < width {
            let f = 0.85 + h01(px as i32, py as i32, 540) * 0.3;
            rgba([228.0, 228.0, 232.0], f, 255)
        } else {
            [0, 0, 0, 0]
        }
    });
    lump(125, [140.0, 75.0, 50.0], &mut tf); // hearty stew (dark meaty bowl)
    // Swords: diagonal blade + guard + handle, tier head colors.
    const SWORD_ART: [&str; 16] = [
        "..............HH",
        ".............HHH",
        "............HHH.",
        "...........HHH..",
        "..........HHH...",
        ".........HHH....",
        "........HHH.....",
        ".......HHH......",
        "......HHH.......",
        ".....HHH........",
        "..g.HHH.........",
        "..gggH..........",
        "...ggg..........",
        "..hh.gg.........",
        ".hh.............",
        "hh..............",
    ];
    let sword_sets: [(u32, [f32; 3]); 4] = [
        (126, [168.0, 122.0, 60.0]),
        (127, [130.0, 130.0, 130.0]),
        (128, [206.0, 116.0, 52.0]),
        (129, [196.0, 148.0, 62.0]),
    ];
    for (slot, head) in sword_sets {
        tf(slot, &mut |_px, _py, u, v| {
            let ax = ((u * 16.0) as usize).min(15);
            let ay = ((v * 16.0) as usize).min(15);
            match SWORD_ART[ay].as_bytes().get(ax) {
                Some(b'H') => {
                    let f = 0.85 + h01(ax as i32, ay as i32, 550 + slot) * 0.25;
                    rgba(head, f, 255)
                }
                Some(b'g') => rgba([70.0, 62.0, 55.0], 1.0, 255),
                Some(b'h') => rgba([104.0, 72.0, 42.0], 1.0, 255),
                _ => [0, 0, 0, 0],
            }
        });
    }

    // Antler: pale bone with faint darker ridges (deer antler boxes).
    tf(130, &mut |px, py, u, v| {
        let t = fbm(u, v, 4, 560);
        let mut c = mix3([225.0, 214.0, 192.0], [200.0, 186.0, 160.0], t);
        if (v * 6.0).fract() < 0.18 {
            c = mix3(c, [160.0, 145.0, 120.0], 0.5);
        }
        rgba(c, speck(px, py, 561, 0.05), 255)
    });

    // Torch: stick with a bright flame head (cross-rendered).
    tf(131, &mut |px, py, u, v| {
        let stick = (u - 0.5).abs() < 0.06 && v > 0.38 && v < 0.95;
        let dx = u - 0.5;
        let dy = v - 0.28;
        let flame = dx * dx + dy * dy * 1.4 < 0.014;
        let core = dx * dx + dy * dy * 1.4 < 0.005;
        if core {
            rgba([255.0, 240.0, 180.0], 1.0, 255)
        } else if flame {
            rgba(
                [255.0, 170.0, 60.0],
                0.9 + h01(px as i32, py as i32, 570) * 0.2,
                255,
            )
        } else if stick {
            rgba([120.0, 84.0, 50.0], 1.0, 255)
        } else {
            [0, 0, 0, 0]
        }
    });
    // Chest: plank panel with dark frame; side gets a latch.
    for (slot, latch) in [(132u32, true), (133, false)] {
        tf(slot, &mut |px, py, u, v| {
            let t = fbm(u, v * 3.0, 4, 580);
            let mut c = mix3([142.0, 100.0, 58.0], [168.0, 122.0, 72.0], t);
            let edge = !(0.06..=0.94).contains(&u) || !(0.06..=0.94).contains(&v);
            if edge {
                c = [92.0, 64.0, 38.0];
            }
            if latch && (u - 0.5).abs() < 0.08 && v > 0.30 && v < 0.52 {
                c = if (u - 0.5).abs() < 0.03 && v > 0.36 && v < 0.46 {
                    [60.0, 60.0, 64.0]
                } else {
                    [150.0, 150.0, 158.0]
                };
            }
            rgba(c, speck(px, py, 581 + slot, 0.06), 255)
        });
    }

    // ---- wardens (rows 8-9): the wild's own ----
    // Thornling: dark bristly shrub-hide with thorn glints.
    tf(134, &mut |px, py, u, v| {
        let t = fbm(u, v, 6, 600);
        let mut c = mix3([30.0, 62.0, 28.0], [58.0, 96.0, 44.0], t);
        if h01(px as i32, py as i32, 601) > 0.93 {
            c = [180.0, 190.0, 150.0]; // thorn tips
        }
        rgba(c, speck(px, py, 602, 0.2), 255)
    });
    // Dryad: mossy bark.
    tf(135, &mut |px, py, u, v| {
        let t = fbm(u * 1.5, v * 3.0, 5, 610);
        let mut c = mix3([74.0, 56.0, 40.0], [104.0, 82.0, 56.0], t);
        if fbm(u, v, 4, 611) > 0.62 {
            c = mix3(c, [60.0, 110.0, 50.0], 0.6); // moss veins
        }
        rgba(c, speck(px, py, 612, 0.1), 255)
    });
    // Dryad face: bark + amber eyes + slit mouth.
    tf(136, &mut |px, py, u, v| {
        let t = fbm(u * 1.5, v * 3.0, 5, 610);
        let mut c = mix3([74.0, 56.0, 40.0], [104.0, 82.0, 56.0], t);
        let eye = |cx: f32| (u - cx).abs() < 0.07 && (v - 0.38).abs() < 0.05;
        if eye(0.30) || eye(0.70) {
            c = [235.0, 180.0, 60.0]; // amber glow
        }
        if (u - 0.5).abs() < 0.16 && (v - 0.72).abs() < 0.025 {
            c = [30.0, 22.0, 16.0];
        }
        rgba(c, speck(px, py, 613, 0.08), 255)
    });
    // Emberkin: charred crust over glowing cracks.
    tf(137, &mut |px, py, u, v| {
        let t = fbm(u * 2.0, v * 2.0, 5, 620);
        if t > 0.58 {
            let g = 0.8 + h01(px as i32, py as i32, 621) * 0.4;
            rgba([255.0 * g, 140.0 * g, 30.0 * g], 1.0, 255)
        } else {
            rgba(
                mix3([28.0, 22.0, 20.0], [56.0, 44.0, 40.0], t * 1.6),
                1.0,
                255,
            )
        }
    });
    // Rimewisp: pale drifting frost.
    tf(138, &mut |px, py, u, v| {
        let t = fbm(u * 2.0, v * 2.0, 5, 630);
        let c = mix3([150.0, 190.0, 225.0], [225.0, 240.0, 252.0], t);
        rgba(c, speck(px, py, 631, 0.06), 255)
    });
    // Gravelurk: cracked granite.
    tf(139, &mut |px, py, u, v| {
        let t = fbm(u, v, 5, 640);
        let mut c = mix3([88.0, 86.0, 84.0], [128.0, 124.0, 118.0], t);
        let (d1, _, _) = voronoi(u, v, 5, 641);
        if d1 < 0.05 {
            c = [48.0, 46.0, 44.0]; // cracks
        }
        rgba(c, speck(px, py, 642, 0.12), 255)
    });
    // Wrathwood: gnarled ancient bark.
    tf(140, &mut |px, py, u, v| {
        let t = fbm(u * 1.2, v * 4.0, 5, 650);
        let c = mix3([46.0, 34.0, 24.0], [80.0, 60.0, 40.0], t);
        rgba(c, speck(px, py, 651, 0.14), 255)
    });
    // Wrathwood face: bark + jagged maw + burning eyes.
    tf(141, &mut |px, py, u, v| {
        let t = fbm(u * 1.2, v * 4.0, 5, 650);
        let mut c = mix3([46.0, 34.0, 24.0], [80.0, 60.0, 40.0], t);
        let eye = |cx: f32| (u - cx).abs() < 0.08 && (v - 0.30).abs() < 0.06;
        if eye(0.28) || eye(0.72) {
            c = [240.0, 120.0, 40.0];
        }
        // Jagged maw: triangle teeth along a dark gash.
        if (v - 0.68).abs() < 0.10 {
            let tooth = ((u * 10.0).fract() - 0.5).abs() * 0.25;
            if (v - 0.68).abs() < 0.09 - tooth {
                c = [18.0, 12.0, 10.0];
            }
        }
        rgba(c, speck(px, py, 652, 0.1), 255)
    });
    // Bolts: thorn / ember / frost.
    tf(142, &mut |_px, _py, u, v| {
        let d = (u - v).abs();
        if d < 0.10 && u > 0.2 && u < 0.85 {
            rgba([90.0, 150.0, 60.0], 1.0, 255)
        } else if d < 0.16 && u > 0.75 && u < 0.92 {
            rgba([200.0, 220.0, 170.0], 1.0, 255) // pale tip
        } else {
            [0, 0, 0, 0]
        }
    });
    tf(143, &mut |px, py, u, v| {
        let dx = u - 0.5;
        let dy = v - 0.5;
        let r = dx * dx + dy * dy;
        if r < 0.05 {
            rgba([255.0, 230.0, 120.0], 1.0, 255)
        } else if r < 0.11 {
            rgba(
                [255.0, 150.0, 40.0],
                0.9 + h01(px as i32, py as i32, 660) * 0.2,
                255,
            )
        } else {
            [0, 0, 0, 0]
        }
    });
    tf(144, &mut |_px, _py, u, v| {
        let dx = (u - 0.5).abs();
        let dy = (v - 0.5).abs();
        if dx * 1.6 + dy < 0.42 && dx < 0.16 {
            rgba([170.0, 215.0, 250.0], 1.0, 255)
        } else {
            [0, 0, 0, 0]
        }
    });
    // Drops: fiber coil, living wood, ember, frost shard, heartwood.
    tf(145, &mut |px, py, u, v| {
        let dx = u - 0.5;
        let dy = v - 0.5;
        let r = (dx * dx + dy * dy).sqrt();
        if r > 0.18 && r < 0.34 {
            rgba(
                [96.0, 140.0, 60.0],
                0.8 + h01(px as i32, py as i32, 670) * 0.4,
                255,
            )
        } else {
            [0, 0, 0, 0]
        }
    });
    tf(146, &mut |px, py, u, v| {
        if (u - 0.5).abs() < 0.28 && (v - 0.5).abs() < 0.22 {
            let t = fbm(u, v * 3.0, 4, 680);
            let mut c = mix3([104.0, 82.0, 56.0], [130.0, 104.0, 70.0], t);
            if fbm(u * 2.0, v, 4, 681) > 0.62 {
                c = [70.0, 150.0, 60.0]; // living veins
            }
            rgba(c, speck(px, py, 682, 0.1), 255)
        } else {
            [0, 0, 0, 0]
        }
    });
    tf(147, &mut |px, py, u, v| {
        let dx = u - 0.5;
        let dy = v - 0.55;
        let r = dx * dx + dy * dy * 1.4;
        if r < 0.07 {
            let t = fbm(u * 2.0, v * 2.0, 4, 690);
            if t > 0.55 {
                rgba([255.0, 170.0, 50.0], 1.0, 255)
            } else {
                rgba(
                    [50.0, 38.0, 34.0],
                    0.9 + h01(px as i32, py as i32, 691) * 0.2,
                    255,
                )
            }
        } else {
            [0, 0, 0, 0]
        }
    });
    tf(148, &mut |_px, _py, u, v| {
        let dx = (u - 0.5).abs();
        let dy = (v - 0.5).abs();
        if dx * 1.3 + dy < 0.36 && dx < 0.2 {
            rgba([185.0, 220.0, 248.0], 1.0, 255)
        } else {
            [0, 0, 0, 0]
        }
    });
    tf(149, &mut |px, py, u, v| {
        if (u - 0.5).abs() < 0.24 && (v - 0.5).abs() < 0.26 {
            let t = fbm(u, v * 2.0, 4, 700);
            let c = mix3([96.0, 34.0, 28.0], [140.0, 58.0, 40.0], t);
            rgba(c, speck(px, py, 701, 0.08), 255)
        } else {
            [0, 0, 0, 0]
        }
    });

    // ---- bows, arrows, armor (rows 9-10) ----
    // Bows: a curved limb along the left, string down the right.
    #[allow(clippy::type_complexity)]
    let bow_art =
        |slot: u32,
         limb: [f32; 3],
         img: &mut dyn FnMut(u32, &mut dyn FnMut(u32, u32, f32, f32) -> [u8; 4])| {
            img(slot, &mut |px, py, u, v| {
                // Limb: arc bulging left of a diagonal.
                let arc = ((u - 0.62) + (v - 0.5) * (v - 0.5) * 1.6).abs();
                if arc < 0.07 && v > 0.06 && v < 0.94 {
                    rgba(
                        limb,
                        0.85 + h01(px as i32, py as i32, 710 + slot) * 0.3,
                        255,
                    )
                } else if (u - 0.80).abs() < 0.025 && v > 0.10 && v < 0.90 {
                    rgba([210.0, 205.0, 185.0], 1.0, 255) // string
                } else {
                    [0, 0, 0, 0]
                }
            });
        };
    bow_art(150, [138.0, 100.0, 58.0], &mut tf);
    bow_art(151, [96.0, 120.0, 62.0], &mut tf); // living-wood green tint
    // Arrow: diagonal shaft, stone head, feather fletch.
    tf(152, &mut |_px, _py, u, v| {
        let d = (u - (1.0 - v)).abs();
        if d < 0.06 && u > 0.15 && u < 0.85 {
            rgba([150.0, 110.0, 62.0], 1.0, 255)
        } else if d < 0.12 && (0.78..0.95).contains(&u) {
            rgba([140.0, 140.0, 140.0], 1.0, 255) // stone tip
        } else if d < 0.14 && u > 0.08 && u <= 0.22 {
            rgba([235.0, 235.0, 238.0], 1.0, 255) // fletching
        } else {
            [0, 0, 0, 0]
        }
    });
    // Armor silhouettes, leather then bronze.
    #[allow(clippy::type_complexity)]
    let armor_art =
        |base: u32,
         c: [f32; 3],
         dark: [f32; 3],
         img: &mut dyn FnMut(u32, &mut dyn FnMut(u32, u32, f32, f32) -> [u8; 4])| {
            // helmet: dome with a face opening
            img(base, &mut |px, py, u, v| {
                let dx = u - 0.5;
                let dome = dx * dx + (v - 0.55) * (v - 0.55) * 1.6 < 0.09 && v < 0.72;
                let opening = dx.abs() < 0.16 && v > 0.48 && v < 0.72;
                if dome && !opening {
                    rgba(c, 0.85 + h01(px as i32, py as i32, 720 + base) * 0.25, 255)
                } else if dome {
                    rgba(dark, 1.0, 255)
                } else {
                    [0, 0, 0, 0]
                }
            });
            // chestplate: torso with shoulders
            img(base + 1, &mut |px, py, u, v| {
                let torso = (u - 0.5).abs() < 0.24 && v > 0.28 && v < 0.88;
                let arms = (u - 0.5).abs() > 0.24 && (u - 0.5).abs() < 0.38 && v > 0.28 && v < 0.52;
                let neck = (u - 0.5).abs() < 0.10 && v <= 0.36;
                if (torso || arms) && !neck {
                    rgba(c, 0.85 + h01(px as i32, py as i32, 730 + base) * 0.25, 255)
                } else {
                    [0, 0, 0, 0]
                }
            });
            // leggings: waist + two legs
            img(base + 2, &mut |px, py, u, v| {
                let waist = (u - 0.5).abs() < 0.22 && v > 0.18 && v < 0.40;
                let leg = ((u - 0.36).abs() < 0.09 || (u - 0.64).abs() < 0.09)
                    && (0.40..0.88).contains(&v);
                if waist || leg {
                    rgba(c, 0.85 + h01(px as i32, py as i32, 740 + base) * 0.25, 255)
                } else {
                    [0, 0, 0, 0]
                }
            });
            // boots: two ankle boxes
            img(base + 3, &mut |px, py, u, v| {
                let boot =
                    ((u - 0.32).abs() < 0.13 || (u - 0.68).abs() < 0.13) && v > 0.52 && v < 0.85;
                if boot {
                    rgba(c, 0.85 + h01(px as i32, py as i32, 750 + base) * 0.25, 255)
                } else {
                    [0, 0, 0, 0]
                }
            });
        };
    armor_art(153, [150.0, 106.0, 64.0], [70.0, 48.0, 30.0], &mut tf);
    armor_art(157, [196.0, 148.0, 62.0], [90.0, 66.0, 30.0], &mut tf);

    // ---- stewardship: saplings, offering stone, bedroll (row 10) ----
    #[allow(clippy::type_complexity)]
    let sapling_art =
        |slot: u32,
         leaf: [f32; 3],
         img: &mut dyn FnMut(u32, &mut dyn FnMut(u32, u32, f32, f32) -> [u8; 4])| {
            img(slot, &mut |px, py, u, v| {
                let stem = (u - 0.5).abs() < 0.05 && v > 0.55 && v < 0.95;
                let dx = u - 0.5;
                let dy = v - 0.42;
                let crown = dx * dx + dy * dy * 1.3 < 0.05;
                if crown && !hash(px as i32, py as i32, 760 + slot).is_multiple_of(5) {
                    rgba(leaf, 0.8 + h01(px as i32, py as i32, 761 + slot) * 0.4, 255)
                } else if stem {
                    rgba([110.0, 78.0, 46.0], 1.0, 255)
                } else {
                    [0, 0, 0, 0]
                }
            });
        };
    sapling_art(161, [70.0, 130.0, 50.0], &mut tf); // oak
    sapling_art(162, [140.0, 170.0, 80.0], &mut tf); // birch
    sapling_art(163, [40.0, 90.0, 55.0], &mut tf); // spruce
    sapling_art(164, [60.0, 160.0, 60.0], &mut tf); // jungle
    sapling_art(165, [120.0, 130.0, 60.0], &mut tf); // acacia
    // Offering stone: mossy rock with a glowing bowl.
    tf(166, &mut |px, py, u, v| {
        let t = fbm(u, v, 5, 770);
        let mut c = mix3([96.0, 96.0, 92.0], [130.0, 128.0, 120.0], t);
        if fbm(u * 2.0, v * 2.0, 4, 771) > 0.62 {
            c = mix3(c, [70.0, 120.0, 60.0], 0.6); // moss
        }
        let dx = u - 0.5;
        let dy = v - 0.4;
        if dx * dx + dy * dy * 2.0 < 0.03 {
            c = [190.0, 225.0, 170.0]; // wildlight pooling in the bowl
        }
        rgba(c, speck(px, py, 772, 0.1), 255)
    });
    // Bedroll: rolled hide with fiber ties.
    tf(167, &mut |px, py, u, v| {
        let dx = u - 0.5;
        let dy = v - 0.55;
        if dx * dx * 0.6 + dy * dy * 2.4 < 0.06 {
            let band = ((u * 7.0).fract() < 0.22) && dx.abs() < 0.32;
            if band {
                rgba([96.0, 140.0, 60.0], 1.0, 255)
            } else {
                let t = fbm(u * 2.0, v, 4, 780);
                rgba(
                    mix3([124.0, 88.0, 56.0], [156.0, 116.0, 76.0], t),
                    0.9 + h01(px as i32, py as i32, 781) * 0.2,
                    255,
                )
            }
        } else {
            [0, 0, 0, 0]
        }
    });

    // ---- iron & steel (rows 10-12) ----
    let iron_c = [178.0, 180.0, 188.0];
    let steel_c = [214.0, 218.0, 230.0];
    // Iron ore: stone + grey nuggets.
    tf(168, &mut |px, py, u, v| {
        let t = fbm(u, v, 4, 7);
        let base = mix3([112.0, 112.0, 116.0], [142.0, 142.0, 144.0], t);
        let (d1, _, id) = voronoi(u, v, 5, 800);
        if d1 < 0.20 && id % 3 == 0 {
            rgba([168.0, 156.0, 148.0], 0.8 + (id % 40) as f32 / 100.0, 255)
        } else {
            rgba(base, speck(px, py, 801, 0.05), 255)
        }
    });
    // Polished blocks.
    for (slot, color) in [(169u32, iron_c), (170, steel_c)] {
        tf(slot, &mut |px, py, u, v| {
            let t = fbm(u, v, 3, 810 + slot);
            let c = mix3(color, [color[0] * 0.8, color[1] * 0.8, color[2] * 0.8], t);
            rgba(c, speck(px, py, 811 + slot, 0.04), 255)
        });
    }
    // Raw lump, blend pile, ingot bars.
    lump(171, [150.0, 132.0, 120.0], &mut tf); // raw iron
    lump(173, [120.0, 118.0, 116.0], &mut tf); // steel blend
    for (slot, color) in [(172u32, iron_c), (174, steel_c)] {
        tf(slot, &mut |px, py, u, v| {
            let inside = u > 0.12 && u < 0.88 && v > 0.36 && v < 0.68;
            if inside {
                let edge = !(0.2..=0.8).contains(&u) || !(0.44..=0.6).contains(&v);
                let f = if edge {
                    0.72
                } else {
                    0.92 + h01(px as i32, py as i32, 820 + slot) * 0.2
                };
                rgba(color, f, 255)
            } else {
                [0, 0, 0, 0]
            }
        });
    }
    // Tool sets: pick/axe/shovel via the shared ASCII art, hoe via AXE rows.
    let metal2: [(u32, [f32; 3]); 6] = [
        (175, iron_c),
        (176, iron_c),
        (177, iron_c),
        (180, steel_c),
        (181, steel_c),
        (182, steel_c),
    ];
    for (i, &(slot, head)) in metal2.iter().enumerate() {
        let art: &[&str; 16] = match i % 3 {
            0 => &PICK_ART,
            1 => &AXE_ART,
            _ => &SHOVEL_ART,
        };
        tf(slot, &mut |_px, _py, u, v| {
            let ax = ((u * 16.0) as usize).min(15);
            let ay = ((v * 16.0) as usize).min(15);
            match art[ay].as_bytes().get(ax) {
                Some(b'H') => {
                    let f = 0.85 + h01(ax as i32, ay as i32, 830 + slot) * 0.2;
                    rgba(head, f, 255)
                }
                Some(b'h') => rgba([104.0, 72.0, 42.0], 1.0, 255),
                _ => [0, 0, 0, 0],
            }
        });
    }
    for (slot, head) in [(178u32, iron_c), (183, steel_c)] {
        tf(slot, &mut |_px, _py, u, v| {
            let ax = ((u * 16.0) as usize).min(15);
            let ay = ((v * 16.0) as usize).min(15);
            match AXE_ART[ay].as_bytes().get(ax) {
                Some(b'H') if ay <= 3 => rgba(head, 0.9, 255),
                Some(b'H') => [0, 0, 0, 0],
                Some(b'h') => rgba([104.0, 72.0, 42.0], 1.0, 255),
                _ => [0, 0, 0, 0],
            }
        });
    }
    for (slot, head) in [(179u32, iron_c), (184, steel_c)] {
        tf(slot, &mut |_px, _py, u, v| {
            let ax = ((u * 16.0) as usize).min(15);
            let ay = ((v * 16.0) as usize).min(15);
            match SWORD_ART[ay].as_bytes().get(ax) {
                Some(b'H') => {
                    let f = 0.85 + h01(ax as i32, ay as i32, 840 + slot) * 0.25;
                    rgba(head, f, 255)
                }
                Some(b'g') => rgba([70.0, 62.0, 55.0], 1.0, 255),
                Some(b'h') => rgba([104.0, 72.0, 42.0], 1.0, 255),
                _ => [0, 0, 0, 0],
            }
        });
    }
    armor_art(185, iron_c, [90.0, 92.0, 98.0], &mut tf);
    armor_art(189, steel_c, [120.0, 124.0, 134.0], &mut tf);
    // Shears: two crossed blades on a pivot.
    tf(193, &mut |_px, _py, u, v| {
        let d1 = (u - v).abs();
        let d2 = (u + v - 1.0).abs();
        if (d1 < 0.09 || d2 < 0.09) && u > 0.2 && u < 0.8 {
            rgba([200.0, 204.0, 212.0], 1.0, 255)
        } else if (u - 0.5).abs() < 0.06 && (v - 0.5).abs() < 0.06 {
            rgba([90.0, 70.0, 50.0], 1.0, 255)
        } else {
            [0, 0, 0, 0]
        }
    });
    // Excavation brush: stick with a fiber head.
    tf(194, &mut |px, py, u, v| {
        let d = (u - (1.0 - v)).abs();
        if d < 0.06 && u > 0.3 && u < 0.9 {
            rgba([120.0, 86.0, 50.0], 1.0, 255)
        } else if d < 0.16 && u <= 0.34 && u > 0.08 {
            rgba(
                [180.0, 200.0, 120.0],
                0.8 + h01(px as i32, py as i32, 850) * 0.4,
                255,
            )
        } else {
            [0, 0, 0, 0]
        }
    });

    // ---- ruins (row 12) ----
    // Mossy cobblestone: cobble with green growth.
    tf(195, &mut |px, py, u, v| {
        let (d1, d2, id) = voronoi(u, v, 4, 860);
        let mut c = if d2 - d1 < 0.14 {
            [62.0, 62.0, 62.0]
        } else {
            let tone = 0.82 + (id % 100) as f32 / 100.0 * 0.3;
            [128.0 * tone, 126.0 * tone, 124.0 * tone]
        };
        if fbm(u, v, 4, 861) > 0.55 {
            c = mix3(c, [72.0, 118.0, 58.0], 0.65);
        }
        rgba(c, speck(px, py, 862, 0.08), 255)
    });
    // Cracked masonry: dressed stone with a jagged crack.
    tf(196, &mut |px, py, u, v| {
        let brick =
            ((v * 4.0).fract() < 0.12) || ((u * 2.0 + (v * 4.0).floor() * 0.5).fract() < 0.06);
        let mut c = if brick {
            [82.0, 80.0, 78.0]
        } else {
            [124.0, 120.0, 116.0]
        };
        let crack =
            ((u - 0.2) * 2.0 - v).abs() < 0.05 || ((u - 0.75) + (v - 0.4) * 0.8).abs() < 0.04;
        if crack {
            c = [40.0, 38.0, 36.0];
        }
        rgba(c, speck(px, py, 870, 0.08), 255)
    });
    // Packed earth: dark trodden soil with flecks.
    tf(197, &mut |px, py, u, v| {
        let t = fbm(u, v, 5, 880);
        let mut c = mix3([84.0, 62.0, 44.0], [108.0, 82.0, 58.0], t);
        if h01(px as i32, py as i32, 881) > 0.94 {
            c = [140.0, 130.0, 110.0];
        }
        rgba(c, speck(px, py, 882, 0.07), 255)
    });
    // Old coin: worn disc.
    tf(198, &mut |px, py, u, v| {
        let dx = u - 0.5;
        let dy = v - 0.5;
        let r = (dx * dx + dy * dy).sqrt();
        if r < 0.24 {
            let f = if r > 0.19 {
                0.7
            } else {
                0.9 + h01(px as i32, py as i32, 890) * 0.2
            };
            rgba([180.0, 158.0, 92.0], f, 255)
        } else {
            [0, 0, 0, 0]
        }
    });
    // Etched tablet: stone slab with rune lines.
    tf(199, &mut |px, py, u, v| {
        if (u - 0.5).abs() < 0.3 && (v - 0.5).abs() < 0.36 {
            let mut c = [140.0, 136.0, 128.0];
            let row = (v * 8.0).floor() as i32;
            if (v * 8.0).fract() < 0.35
                && row % 2 == 0
                && (u - 0.5).abs() < 0.22
                && h01(row, (u * 10.0) as i32, 895) > 0.3
            {
                c = [70.0, 66.0, 60.0];
            }
            rgba(c, speck(px, py, 896, 0.06), 255)
        } else {
            [0, 0, 0, 0]
        }
    });
    // Charms: small knotted talismans in three tints.
    for (slot, tint) in [
        (200u32, [110.0, 160.0, 120.0]),
        (201, [150.0, 110.0, 70.0]),
        (202, [170.0, 150.0, 90.0]),
    ] {
        tf(slot, &mut |px, py, u, v| {
            let dx = u - 0.5;
            let dy = v - 0.6;
            let ring = (dx * dx + dy * dy).sqrt();
            if ring > 0.14 && ring < 0.24 {
                rgba(
                    tint,
                    0.85 + h01(px as i32, py as i32, 900 + slot) * 0.3,
                    255,
                )
            } else if dx.abs() < 0.03 && v > 0.15 && v < 0.42 {
                rgba([96.0, 140.0, 60.0], 1.0, 255) // fiber cord
            } else {
                [0, 0, 0, 0]
            }
        });
    }

    // Player skin + face (remote players in multiplayer).
    // Player bases are near-greyscale: build_atlas multiplies them by
    // the style palettes into the reserved variant slots (see style.rs),
    // so the default look is neutral and every part tints cleanly.
    tf(203, &mut |px, py, u, v| {
        // Shirt: woven fabric, soft fold shading, a hem line.
        let t = fbm(u, v, 5, 903);
        let mut g = 200.0 + t * 30.0;
        let fold = ((u * 9.0).sin() * 0.5 + (v * 6.0 + u * 2.0).sin() * 0.5) * 9.0;
        g += fold;
        if v > 0.9 {
            g *= 0.82; // hem
        }
        rgba(
            [g, g * 0.985, g * 0.96],
            0.9 + h01(px as i32, py as i32, 910) * 0.15,
            255,
        )
    });
    tf(204, &mut |px, py, u, v| {
        // Face: plain skin base, simple dark eyes, the faintest mouth.
        // Deliberately epicene — identity comes from the style palette.
        let base = 225.0 + fbm(u, v, 4, 904) * 12.0;
        let mut c = [base, base * 0.965, base * 0.93];
        let eye = |cx: f32| (u - cx).abs() < 0.075 && (v - 0.44).abs() < 0.05;
        if eye(0.31) || eye(0.69) {
            c = [38.0, 34.0, 34.0];
        }
        if (u - 0.5).abs() < 0.10 && (v - 0.76).abs() < 0.02 {
            c = [base * 0.72, base * 0.66, base * 0.63]; // soft mouth
        }
        rgba(c, speck(px, py, 911, 0.03), 255)
    });

    // (13,12) snowball: a packed white ball, blue-shadowed.
    tf(205, &mut |px, py, u, v| {
        let dx = u - 0.5;
        let dy = v - 0.52;
        let r = dx * dx + dy * dy;
        if r < 0.11 {
            let shade = 1.0 - (dx * 0.5 + dy * 1.1).clamp(0.0, 0.45);
            let c = [235.0 * shade, 242.0 * shade, 252.0 * shade];
            rgba(c, speck(px, py, 921, 0.06), 255)
        } else {
            [0, 0, 0, 0]
        }
    });
    // (14,12) rain streak: faint vertical strands on transparency.
    tf(206, &mut |px, py, u, _v| {
        let strand = h01((u * 8.0) as i32, 0, 923) > 0.55 && px % 2 == 0;
        if strand {
            let a = 120 + (h01(px as i32, py as i32, 925) * 60.0) as u8;
            [170, 190, 220, a]
        } else {
            [0, 0, 0, 0]
        }
    });
    // (15,12) snow flake: soft white dots drifting on transparency.
    tf(207, &mut |px, py, u, v| {
        let cellx = (u * 4.0) as i32;
        let celly = (v * 4.0) as i32;
        let cx = (cellx as f32 + 0.5) / 4.0 + (h01(cellx, celly, 927) - 0.5) * 0.12;
        let cy = (celly as f32 + 0.5) / 4.0 + (h01(cellx, celly, 929) - 0.5) * 0.12;
        let d = (u - cx) * (u - cx) + (v - cy) * (v - cy);
        if d < 0.0016 && h01(cellx, celly, 931) > 0.35 {
            [245, 248, 255, 235]
        } else {
            let _ = (px, py);
            [0, 0, 0, 0]
        }
    });

    // ---- steelworks (row 13) ----
    // (0,13) firebrick: deep red-brown bricks with ember-dark seams.
    tf(208, &mut |px, py, u, v| {
        let bh = (u * 4.0) as u32;
        let row = (v * 4.0) as u32;
        let off = if row.is_multiple_of(2) { 0.125 } else { 0.0 };
        let seam_v = (v * 4.0).fract() < 0.14;
        let seam_h = ((u + off) * 4.0).fract() < 0.14;
        let c = if seam_v || seam_h {
            [46.0, 26.0, 22.0]
        } else {
            let t = fbm(u, v, 4, 941);
            mix3([132.0, 58.0, 40.0], [170.0, 84.0, 52.0], t)
        };
        let _ = bh;
        rgba(c, speck(px, py, 943, 0.08), 255)
    });
    // (1,13) bloomery mouth, cold: firebrick around a dark arch.
    // (2,13) the same mouth, lit: the arch glows from within.
    for (tx2, lit) in [(1u32, false), (2u32, true)] {
        tf(13 * 16 + tx2, &mut |px, py, u, v| {
            let dx = u - 0.5;
            let arch = dx * dx * 2.2 + (v - 0.75) * (v - 0.75) < 0.055 && v > 0.4;
            if arch {
                if lit {
                    let t = fbm(u, v, 3, 947);
                    rgba(
                        mix3([255.0, 140.0, 30.0], [255.0, 210.0, 90.0], t),
                        1.0,
                        255,
                    )
                } else {
                    rgba([24.0, 18.0, 16.0], speck(px, py, 949, 0.1), 255)
                }
            } else {
                let t = fbm(u, v, 4, 941);
                let seam = (v * 4.0).fract() < 0.14 || (u * 4.0).fract() < 0.14;
                let c = if seam {
                    [46.0, 26.0, 22.0]
                } else {
                    mix3([132.0, 58.0, 40.0], [170.0, 84.0, 52.0], t)
                };
                rgba(c, speck(px, py, 943, 0.08), 255)
            }
        });
    }
    // (3,13) charcoal block: black chunks with faint sheen.
    tf(211, &mut |px, py, u, v| {
        let (d1, _, id) = voronoi(u, v, 5, 953);
        let edge = d1 < 0.05;
        let base = 26.0 + (id % 5) as f32 * 6.0;
        let c = if edge {
            [12.0, 11.0, 10.0]
        } else {
            [base, base, base + 2.0]
        };
        rgba(c, speck(px, py, 957, 0.12), 255)
    });
    // (4,13) anvil top: worn gray face with a darker working band.
    tf(212, &mut |px, py, u, v| {
        let t = fbm(u, v, 4, 959);
        let mut c = mix3([120.0, 120.0, 124.0], [156.0, 156.0, 160.0], t);
        if (v - 0.5).abs() < 0.18 && (u - 0.5).abs() < 0.36 {
            c = [88.0, 88.0, 94.0];
        }
        rgba(c, speck(px, py, 961, 0.07), 255)
    });
    // (5,13) steel bloom: a spongy slag-streaked lump (sprite).
    tf(213, &mut |px, py, u, v| {
        let dx = u - 0.5;
        let dy = v - 0.55;
        let wob = h01(px as i32 / 3, py as i32 / 3, 963) * 0.06;
        if dx * dx + dy * dy < 0.09 + wob {
            let t = fbm(u, v, 4, 965);
            let c = if h01(px as i32, py as i32, 967) > 0.8 {
                [70.0, 60.0, 52.0] // slag streaks
            } else {
                mix3([120.0, 122.0, 128.0], [168.0, 170.0, 176.0], t)
            };
            rgba(c, 1.0, 255)
        } else {
            [0, 0, 0, 0]
        }
    });
    // (6,13) smithing hammer: iron head, stout handle (sprite).
    tf(214, &mut |px, py, u, v| {
        let head = (0.25..0.75).contains(&u) && (0.12..0.38).contains(&v);
        let handle = (u - 0.5).abs() < 0.06 && (0.38..0.92).contains(&v);
        if head {
            rgba(
                [150.0, 152.0, 158.0],
                0.85 + h01(px as i32, py as i32, 969) * 0.3,
                255,
            )
        } else if handle {
            rgba(
                [110.0, 78.0, 48.0],
                0.9 + h01(px as i32, py as i32, 971) * 0.2,
                255,
            )
        } else {
            [0, 0, 0, 0]
        }
    });

    // (216) glass: transparent pane with a pale frame and glints.
    tf(216, &mut |px, py, u, v| {
        let edge = !(0.06..=0.94).contains(&u) || !(0.06..=0.94).contains(&v);
        let glint = ((u + v) * 10.0).fract() < 0.12 && u > 0.15 && u < 0.55 && v < 0.6;
        if edge {
            rgba([210.0, 224.0, 232.0], speck(px, py, 973, 0.05), 255)
        } else if glint {
            [235, 244, 250, 255]
        } else {
            [0, 0, 0, 0]
        }
    });

    // (217-219) color minerals: stone studded with vivid crystal.
    for (slot, c) in [
        (217u32, [52.0f32, 82.0, 200.0]), // cobalt
        (218, [200.0, 52.0, 48.0]),       // cinnabar
        (219, [148.0, 64.0, 190.0]),      // manganese
    ] {
        tf(slot, &mut |px, py, u, v| {
            let t = fbm(u, v, 4, 7);
            let base = mix3([112.0, 112.0, 116.0], [142.0, 142.0, 144.0], t);
            let (d1, _, id) = voronoi(u, v, 5, 977 + slot);
            if d1 < 0.2 && id % 3 == 0 {
                rgba(c, 0.8 + (id % 40) as f32 / 100.0, 255)
            } else {
                rgba(base, speck(px, py, 979, 0.05), 255)
            }
        });
    }
    // (220-224) pigment powders: soft heaps on transparency.
    for (slot, c) in [
        (220u32, [96.0f32, 180.0, 150.0]), // verdigris
        (221, [196.0, 140.0, 60.0]),       // ochre
        (222, [58.0, 92.0, 210.0]),        // cobalt
        (223, [210.0, 58.0, 52.0]),        // cinnabar
        (224, [156.0, 70.0, 200.0]),       // manganese
    ] {
        tf(slot, &mut |px, py, u, v| {
            let dx = u - 0.5;
            let heap = v > 0.45 && (v - 0.45) > dx * dx * 2.2 - 0.35 && v < 0.85;
            if heap {
                rgba(c, 0.8 + h01(px as i32, py as i32, 983 + slot) * 0.35, 255)
            } else {
                [0, 0, 0, 0]
            }
        });
    }
    // (225-229) colored glass: the clear pane, tinted.
    for (slot, c) in [
        (225u32, [110.0f32, 210.0, 190.0]),
        (226, [230.0, 180.0, 90.0]),
        (227, [90.0, 130.0, 235.0]),
        (228, [230.0, 90.0, 85.0]),
        (229, [190.0, 110.0, 230.0]),
    ] {
        tf(slot, &mut |px, py, u, v| {
            let edge = !(0.06..=0.94).contains(&u) || !(0.06..=0.94).contains(&v);
            let glint = ((u + v) * 10.0).fract() < 0.12 && u > 0.15 && u < 0.55 && v < 0.6;
            if edge {
                rgba(
                    [c[0] * 0.8, c[1] * 0.8, c[2] * 0.8],
                    speck(px, py, 987, 0.05),
                    255,
                )
            } else if glint {
                rgba([c[0] * 1.1, c[1] * 1.1, c[2] * 1.1], 1.0, 255)
            } else {
                // A held/placed tint pane: mostly open, faint film.
                [c[0] as u8, c[1] as u8, c[2] as u8, 90]
            }
        });
    }
    // (230/231) kiln mouth, cold and lit: brick around a wide slot.
    for (slot, lit) in [(230u32, false), (231, true)] {
        tf(slot, &mut |px, py, u, v| {
            let mouth = (u - 0.5).abs() < 0.3 && (0.45..0.8).contains(&v);
            if mouth {
                if lit {
                    let t = fbm(u, v, 3, 991);
                    rgba(
                        mix3([255.0, 220.0, 120.0], [255.0, 250.0, 210.0], t),
                        1.0,
                        255,
                    )
                } else {
                    rgba([26.0, 20.0, 18.0], speck(px, py, 993, 0.1), 255)
                }
            } else {
                let t = fbm(u, v, 4, 941);
                let seam = (v * 4.0).fract() < 0.14 || (u * 4.0).fract() < 0.14;
                let c = if seam {
                    [46.0, 26.0, 22.0]
                } else {
                    mix3([132.0, 58.0, 40.0], [170.0, 84.0, 52.0], t)
                };
                rgba(c, speck(px, py, 943, 0.08), 255)
            }
        });
    }
    // (233) trodden snow: the layer with bootprints pressed in.
    tf(233, &mut |px, py, u, v| {
        let t = fbm(u, v, 6, 50);
        let mut c = mix3([230.0, 236.0, 244.0], [250.0, 252.0, 255.0], t);
        // Two offset prints, each a pair of pad+heel ovals.
        let prints = [(0.32f32, 0.30f32, 1.0f32), (0.68, 0.68, -1.0)];
        let mut pressed = false;
        for (cx, cy, flip) in prints {
            for (oy, rw, rh) in [(0.0f32, 0.10f32, 0.13f32), (0.17, 0.07, 0.08)] {
                let dx = (u - cx) / rw;
                let dy = (v - (cy + oy * flip)) / rh;
                if dx * dx + dy * dy < 1.0 {
                    pressed = true;
                }
            }
        }
        if pressed {
            // The pressed floor sits lower: bluer, darker, smoother.
            c = mix3(c, [196.0, 206.0, 224.0], 0.75);
        }
        rgba(c, speck(px, py, 51, 0.03) * emboss(px, py, tp), 255)
    });

    // (234) plain skin: head sides/back/top under hair, hands.
    tf(234, &mut |px, py, u, v| {
        let base = 225.0 + fbm(u, v, 4, 934) * 12.0;
        rgba(
            [base, base * 0.965, base * 0.93],
            speck(px, py, 935, 0.03),
            255,
        )
    });
    // (235) hair, side band: strands over the brow, alpha-cut below.
    tf(235, &mut |_px, _py, u, v| {
        let edge = 0.33 + ((u * 24.0).sin() * 0.5 + (u * 7.0).sin() * 0.5) * 0.06;
        if v > edge {
            return [0, 0, 0, 0];
        }
        let strand = 195.0 + ((u * 40.0).sin() * 0.5 + 0.5) * 45.0 - v * 30.0;
        [
            strand as u8,
            (strand * 0.97) as u8,
            (strand * 0.94) as u8,
            255,
        ]
    });
    // (238) hair, crown: full coverage for the head's top face.
    tf(238, &mut |_px, _py, u, v| {
        let strand = 200.0 + (((u + v) * 34.0).sin() * 0.5 + 0.5) * 40.0;
        [
            strand as u8,
            (strand * 0.97) as u8,
            (strand * 0.94) as u8,
            255,
        ]
    });
    // (236) trousers: plain weave with a side seam.
    tf(236, &mut |px, py, u, v| {
        let t = fbm(u, v, 5, 936);
        let mut g = 195.0 + t * 25.0 + ((v * 14.0).sin()) * 5.0;
        if (u - 0.08).abs() < 0.03 {
            g *= 0.85; // seam
        }
        rgba(
            [g, g * 0.99, g * 0.97],
            0.9 + h01(px as i32, py as i32, 937) * 0.12,
            255,
        )
    });
    // (237) boots: worn leather with a darker sole — fixed, untinted.
    tf(237, &mut |px, py, u, v| {
        let t = fbm(u, v, 4, 938);
        let mut c = mix3([74.0, 56.0, 40.0], [96.0, 74.0, 52.0], t);
        if v > 0.72 {
            c = [46.0, 41.0, 37.0]; // sole
        }
        if v < 0.18 && (u * 10.0).sin() > 0.3 {
            c = [58.0, 45.0, 34.0]; // lacing hint
        }
        rgba(c, 0.9 + h01(px as i32, py as i32, 939) * 0.15, 255)
    });

    // (239, EXTRA_BASE+5) the bucket, empty and carrying its cell of
    // water: tin body tapering to the base, a wire handle over the top.
    let bucket_px = |u: f32, v: f32, water: bool| -> [u8; 4] {
        let du = (u - 0.5).abs();
        let half = 0.30 - (v - 0.28).max(0.0) * 0.16;
        let body = (0.28..=0.88).contains(&v) && du <= half;
        let r = (du * du + (v - 0.28) * (v - 0.28)).sqrt();
        let handle = v < 0.28 && (0.24..=0.30).contains(&r);
        if !(body || handle) {
            return [0, 0, 0, 0];
        }
        if handle {
            return [88, 84, 80, 255];
        }
        if water && v < 0.42 && du <= half - 0.03 {
            let g = 120.0 + ((u * 22.0).sin() * 0.5 + 0.5) * 30.0;
            return [
                (g * 0.35) as u8,
                (g * 0.55) as u8,
                (g * 1.5).min(255.0) as u8,
                255,
            ];
        }
        let sheen = 148.0 + ((u * 18.0).sin() * 0.5 + 0.5) * 34.0 - v * 40.0;
        let rim = if v < 0.34 { 26.0 } else { 0.0 };
        let g = (sheen + rim).clamp(0.0, 255.0);
        [g as u8, g as u8, (g * 1.04).min(255.0) as u8, 255]
    };
    tf(239, &mut |_px, _py, u, v| bucket_px(u, v, false));
    tf(
        crate::style::EXTRA_BASE as u32 + 5,
        &mut |_px, _py, u, v| bucket_px(u, v, true),
    );

    // (EXTRA_BASE..) hair lengths: a tight crop, strands to the collar.
    let xb = crate::style::EXTRA_BASE as u32;
    tf(xb, &mut |_px, _py, u, v| {
        let edge = 0.16 + ((u * 21.0).sin() * 0.5 + 0.5) * 0.05;
        if v > edge {
            return [0, 0, 0, 0];
        }
        let strand = 200.0 + ((u * 36.0).sin() * 0.5 + 0.5) * 40.0;
        [
            strand as u8,
            (strand * 0.97) as u8,
            (strand * 0.94) as u8,
            255,
        ]
    });
    tf(xb + 1, &mut |_px, _py, u, v| {
        let edge = 0.88 + ((u * 17.0).sin() * 0.5 + 0.5) * 0.10;
        if v > edge {
            return [0, 0, 0, 0];
        }
        let strand = 185.0 + ((u * 30.0).sin() * 0.5 + 0.5) * 50.0 - v * 25.0;
        [
            strand as u8,
            (strand * 0.97) as u8,
            (strand * 0.94) as u8,
            255,
        ]
    });
    // Facial hair bands, drawn in FACE tile coordinates so they sit
    // right on the head front: moustache, trimmed, full.
    tf(xb + 2, &mut |_px, _py, u, v| {
        let band = (0.30..=0.70).contains(&u) && (0.64..=0.745).contains(&v);
        if !band {
            return [0, 0, 0, 0];
        }
        let strand = 190.0 + ((u * 44.0).sin() * 0.5 + 0.5) * 40.0;
        [
            strand as u8,
            (strand * 0.96) as u8,
            (strand * 0.93) as u8,
            255,
        ]
    });
    tf(xb + 3, &mut |_px, _py, u, v| {
        // A jawline outline: thin sides + chin strip, mouth open.
        let side = !(0.16..=0.84).contains(&u) && v > 0.55;
        let chin = v > 0.86;
        let mouth = (0.36..=0.64).contains(&u) && v < 0.92;
        if !(side || (chin && !mouth)) {
            return [0, 0, 0, 0];
        }
        let strand = 185.0 + ((u * 40.0).sin() * 0.5 + 0.5) * 40.0;
        [
            strand as u8,
            (strand * 0.96) as u8,
            (strand * 0.93) as u8,
            255,
        ]
    });
    tf(xb + 4, &mut |_px, _py, u, v| {
        // The full beard: cheeks to chin, a notch for the mouth.
        let ragged = 0.58 + ((u * 26.0).sin() * 0.5 + 0.5) * 0.06;
        let mouth = (0.40..=0.60).contains(&u) && (0.70..=0.78).contains(&v);
        if v < ragged || mouth || !(0.10..=0.90).contains(&u) {
            return [0, 0, 0, 0];
        }
        let strand = 180.0 + ((u * 34.0).sin() * 0.5 + 0.5) * 45.0;
        [
            strand as u8,
            (strand * 0.96) as u8,
            (strand * 0.93) as u8,
            255,
        ]
    });

    // (232) quern top: a millstone with a center eye and sweep grooves.
    tf(232, &mut |px, py, u, v| {
        let dx = u - 0.5;
        let dy = v - 0.5;
        let r = (dx * dx + dy * dy).sqrt();
        if r > 0.48 {
            let t = fbm(u, v, 4, 959);
            return rgba(
                mix3([120.0, 120.0, 124.0], [150.0, 150.0, 154.0], t),
                1.0,
                255,
            );
        }
        if r < 0.08 {
            return rgba([50.0, 48.0, 46.0], 1.0, 255); // the eye
        }
        let ang = dy.atan2(dx);
        let groove = ((ang * 5.0 + r * 9.0).sin()).abs() < 0.16;
        let t = fbm(u, v, 4, 997);
        let mut c = mix3([138.0, 136.0, 132.0], [168.0, 166.0, 160.0], t);
        if groove {
            c = [104.0, 102.0, 98.0];
        }
        rgba(c, speck(px, py, 999, 0.06), 255)
    });

    // (15,0) unknown/missing texture: magenta checkerboard.
    tile(15, &mut |px, py, _u, _v| {
        let k = (tp / 8).max(1);
        if ((px / k) + (py / k)).is_multiple_of(2) {
            [230, 0, 230, 255]
        } else {
            [20, 0, 20, 255]
        }
    });

    // Row 1: crack overlay stages, radial cracks scaled to resolution.
    for stage in 0..4u32 {
        tile(16 + stage, &mut |px, py, u, v| {
            let dx = u - 0.5;
            let dy = v - 0.5;
            let r = (dx * dx + dy * dy).sqrt();
            let ang = dy.atan2(dx);
            let n_spokes = 3 + stage;
            for s in 0..n_spokes {
                let base = h01(s as i32, stage as i32, 200) * std::f32::consts::TAU;
                let wob = (vnoise(r * 8.0, s as f32 * 2.0, 8, 201 + stage) - 0.5) * 0.8;
                let d = (ang - base - wob).rem_euclid(std::f32::consts::TAU);
                let d = d.min(std::f32::consts::TAU - d);
                let max_r = 0.16 + stage as f32 * 0.12;
                if d < 0.16 && r < max_r + h01(px as i32, py as i32, 202) * 0.12 {
                    return [20, 16, 12, 200];
                }
            }
            [0, 0, 0, 0]
        });
    }

    // Row 2: item icons — 16px pixel art scaled up nearest-neighbor so it
    // stays crisp at any atlas resolution.
    let icons: [(u32, &[&str; 16], [f32; 3]); 7] = [
        (0, &STICK_ART, [168.0, 122.0, 60.0]),
        (1, &PICK_ART, [168.0, 122.0, 60.0]),
        (2, &PICK_ART, [130.0, 130.0, 130.0]),
        (3, &AXE_ART, [168.0, 122.0, 60.0]),
        (4, &AXE_ART, [130.0, 130.0, 130.0]),
        (5, &SHOVEL_ART, [168.0, 122.0, 60.0]),
        (6, &SHOVEL_ART, [130.0, 130.0, 130.0]),
    ];
    let k = tp / 16;
    for (tx, art, head) in icons {
        tile(2 * 16 + tx, &mut |px, py, _u, _v| {
            let ax = (px / k.max(1)).min(15) as usize;
            let ay = (py / k.max(1)).min(15) as usize;
            match art[ay].as_bytes().get(ax) {
                Some(b'H') => {
                    let f = 0.85 + h01(ax as i32, ay as i32, 300 + tx) * 0.2;
                    rgba(head, f, 255)
                }
                Some(b'h') => rgba([104.0, 72.0, 42.0], 1.0, 255),
                _ => [0, 0, 0, 0],
            }
        });
    }

    img
}

// Item icon pixel art: '.'=transparent, 'H'=head material, 'h'=handle wood.
const PICK_ART: [&str; 16] = [
    "................",
    "..HHHHHHHHHH....",
    ".HHHHHHHHHHHH...",
    ".HH........HH...",
    ".H...hh.....HH..",
    ".....hh......H..",
    "....hh..........",
    "....hh..........",
    "...hh...........",
    "...hh...........",
    "..hh............",
    "..hh............",
    ".hh.............",
    ".hh.............",
    "................",
    "................",
];

const AXE_ART: [&str; 16] = [
    "................",
    "....HHHHH.......",
    "..HHHHHHHH......",
    ".HHHHHHHHH......",
    ".HHHH..hh.......",
    ".HHH...hh.......",
    "..H...hh........",
    "......hh........",
    ".....hh.........",
    ".....hh.........",
    "....hh..........",
    "....hh..........",
    "...hh...........",
    "...hh...........",
    "................",
    "................",
];

const SHOVEL_ART: [&str; 16] = [
    "................",
    "......HHHH......",
    ".....HHHHHH.....",
    ".....HHHHHH.....",
    ".....HHHHHH.....",
    "......Hhh.......",
    "......hh........",
    ".....hh.........",
    ".....hh.........",
    "....hh..........",
    "....hh..........",
    "...hh...........",
    "...hh...........",
    "..hh............",
    "................",
    "................",
];

const STICK_ART: [&str; 16] = [
    "................",
    "..........hh....",
    ".........hhh....",
    ".........hh.....",
    "........hh......",
    "........hh......",
    ".......hh.......",
    ".......hh.......",
    "......hh........",
    "......hh........",
    ".....hh.........",
    ".....hh.........",
    "....hh..........",
    "...hhh..........",
    "................",
    "................",
];

/// Tint the foliage tiles for a season, applied to the finished atlas
/// (so texture packs get the same treatment). Only greenish pixels
/// shift, which spares the dirt band on grass sides.
pub fn season_tint(img: &mut [u8], px: u32, season: usize) {
    let mult: [f32; 3] = match season {
        0 => [0.92, 1.06, 0.86], // spring: vivid
        2 => [1.14, 0.92, 0.58], // autumn: amber
        3 => [0.86, 0.86, 0.90], // winter: drab
        _ => return,             // summer is the reference look
    };
    let slots = builtin_slots();
    let tp = px / ATLAS_TILES;
    for name in [
        "grass_top",
        "grass_side",
        "leaves",
        "birch_leaves",
        "spruce_leaves",
        "jungle_leaves",
        "acacia_leaves",
    ] {
        let Some(&slot) = slots.get(name) else {
            continue;
        };
        let tx = (slot as u32 % ATLAS_TILES) * tp;
        let ty = (slot as u32 / ATLAS_TILES) * tp;
        for y in ty..ty + tp {
            for x in tx..tx + tp {
                let i = ((y * px + x) * 4) as usize;
                let (r, g, b) = (img[i] as f32, img[i + 1] as f32, img[i + 2] as f32);
                if g >= r && g >= b {
                    img[i] = (r * mult[0]).min(255.0) as u8;
                    img[i + 1] = (g * mult[1]).min(255.0) as u8;
                    img[i + 2] = (b * mult[2]).min(255.0) as u8;
                }
            }
        }
    }
}
