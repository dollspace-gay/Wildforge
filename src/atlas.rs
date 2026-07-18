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

pub const ATLAS_TILES: u32 = 16;
/// Atlas slot = row * 16 + col. Rows 0-2 are built-in procedural tiles.
pub const UNKNOWN_SLOT: u16 = 15;
pub const CRACK_SLOT: u16 = 16; // stages 16..=19
pub const FIRST_FREE_SLOT: u16 = 64; // rows 0-3 are built-in tiles

/// Built-in procedural tile names usable as `@name` in mod TOML.
pub fn builtin_slots() -> std::collections::HashMap<String, u16> {
    [
        ("grass_top", 0u16), ("grass_side", 1), ("dirt", 2), ("stone", 3),
        ("cobblestone", 4), ("sand", 5), ("gravel", 6), ("log_side", 7),
        ("log_top", 8), ("leaves", 9), ("planks", 10), ("bedrock", 11),
        ("water", 12), ("table_top", 13), ("table_side", 14),
        ("stick", 32), ("wood_pickaxe", 33), ("stone_pickaxe", 34),
        ("wood_axe", 35), ("stone_axe", 36), ("wood_shovel", 37),
        ("stone_shovel", 38), ("snow", 39), ("ice", 40),
        ("cactus_side", 41), ("cactus_top", 42),
        ("birch_log", 43), ("birch_log_top", 44), ("birch_leaves", 45),
        ("spruce_log", 46), ("spruce_log_top", 47), ("spruce_leaves", 48),
        ("jungle_log", 49), ("jungle_log_top", 50), ("jungle_leaves", 51),
        ("acacia_log", 52), ("acacia_log_top", 53), ("acacia_leaves", 54),
        ("birch_planks", 55), ("spruce_planks", 56), ("jungle_planks", 57),
        ("acacia_planks", 58),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

/// Build the atlas: procedural tiles plus mod PNGs blitted into their slots.
pub fn build_with_mods(tex_files: &[(u16, std::path::PathBuf)]) -> (Vec<u8>, u32) {
    let (mut img, px) = load_or_build();
    let tp = px / ATLAS_TILES;
    for (slot, path) in tex_files {
        match load_tile_png(path) {
            Some((data, w, h)) => blit_tile(&mut img, px, tp, *slot, &data, w, h),
            None => eprintln!("atlas: failed to load {}", path.display()),
        }
    }
    (img, px)
}

fn load_tile_png(path: &std::path::Path) -> Option<(Vec<u8>, u32, u32)> {
    let f = std::fs::File::open(path).ok()?;
    let mut reader = png::Decoder::new(std::io::BufReader::new(f)).read_info().ok()?;
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
        match export_png(&path, &data, px) {
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

fn export_png(path: &str, data: &[u8], px: u32) -> Result<(), Box<dyn std::error::Error>> {
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
        h01((xi + dx).rem_euclid(period), (yi + dy).rem_euclid(period), salt)
    };
    let top = p(0, 0) + (p(1, 0) - p(0, 0)) * fx;
    let bot = p(0, 1) + (p(1, 1) - p(0, 1)) * fx;
    top + (bot - top) * fy
}

/// 2-octave tileable fBm in 0..1. `freq` is cells across the tile.
fn fbm(u: f32, v: f32, freq: i32, salt: u32) -> f32 {
    let a = vnoise(u * freq as f32, v * freq as f32, freq, salt);
    let b = vnoise(u * freq as f32 * 2.0, v * freq as f32 * 2.0, freq * 2, salt ^ 0x9e37);
    (a * 0.68 + b * 0.32).clamp(0.0, 1.0)
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
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

pub fn build_procedural(tp: u32) -> Vec<u8> {
    let atlas_px = ATLAS_TILES * tp;
    let mut img = vec![0u8; (atlas_px * atlas_px * 4) as usize];

    let mut tile = |tx: u32, ty: u32, f: &mut dyn FnMut(u32, u32, f32, f32) -> [u8; 4]| {
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
    tile(0, 0, &mut |px, py, u, v| {
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
    tile(2, 0, &mut |px, py, u, v| {
        let mut c = dirt_at(px, py, u, v);
        let f = emboss(px, py, tp);
        c[0] = (c[0] as f32 * f) as u8;
        c[1] = (c[1] as f32 * f) as u8;
        c[2] = (c[2] as f32 * f) as u8;
        c
    });

    // (1,0) grass side: dirt with an irregular turf overhang.
    tile(1, 0, &mut |px, py, u, v| {
        let depth = (0.14 + 0.12 * fbm(u, 0.0, 8, 5)) * tp as f32;
        let fy = py as f32;
        if fy < depth {
            let t = fbm(u, v, 5, 1);
            let c = mix3([80.0, 138.0, 44.0], [108.0, 176.0, 58.0], t);
            rgba(c, speck(px, py, 6, 0.06) * if py == 0 { 1.12 } else { 1.0 }, 255)
        } else if fy < depth + (tp / 16).max(1) as f32 {
            rgba([70.0, 50.0, 34.0], 1.0, 255) // shadow line under the turf
        } else {
            dirt_at(px, py, u, v)
        }
    });

    // (3,0) stone: soft blotches with darker veins.
    tile(3, 0, &mut |px, py, u, v| {
        let t = fbm(u, v, 4, 7);
        let mut c = mix3([112.0, 112.0, 116.0], [142.0, 142.0, 144.0], t);
        let vein = (vnoise(u * 5.0, v * 5.0, 5, 8) - 0.5).abs();
        if vein < 0.035 {
            c = mix3(c, [70.0, 70.0, 74.0], 0.7);
        }
        rgba(c, speck(px, py, 9, 0.05) * emboss(px, py, tp), 255)
    });

    // (4,0) cobblestone: rounded stones with dark mortar.
    tile(4, 0, &mut |px, py, u, v| {
        let (d1, d2, id) = voronoi(u, v, 4, 10);
        let mortar = d2 - d1 < 0.14;
        if mortar {
            rgba([62.0, 62.0, 62.0], speck(px, py, 11, 0.1), 255)
        } else {
            let tone = 0.82 + (id % 100) as f32 / 100.0 * 0.3;
            // Round shading: bright at stone center, darker toward mortar.
            let dome = 1.08 - d1 * 0.45;
            rgba([128.0, 126.0, 124.0], tone * dome * speck(px, py, 12, 0.05), 255)
        }
    });

    // (5,0) sand: fine grain with soft ripple bands.
    tile(5, 0, &mut |px, py, u, v| {
        let ripple = ((v + fbm(u, v, 3, 13) * 0.25) * std::f32::consts::TAU * 3.0).sin() * 0.05;
        let t = fbm(u, v, 8, 14);
        let c = mix3([206.0, 192.0, 148.0], [228.0, 216.0, 172.0], t);
        rgba(c, (1.0 + ripple) * speck(px, py, 15, 0.06) * emboss(px, py, tp), 255)
    });

    // (6,0) gravel: tightly packed pebbles.
    tile(6, 0, &mut |px, py, u, v| {
        let (d1, _, id) = voronoi(u, v, 6, 16);
        let grayish = 0.75 + (id % 100) as f32 / 100.0 * 0.5;
        let warm = (id >> 8) % 4 == 0;
        let base = if warm { [134.0, 116.0, 100.0] } else { [122.0, 120.0, 118.0] };
        let dome = 1.05 - d1 * 0.5;
        rgba(base, grayish * dome * speck(px, py, 17, 0.07), 255)
    });

    // (7,0) log side: vertical grain and bark ridges.
    tile(7, 0, &mut |px, py, u, v| {
        let grain = ((u * std::f32::consts::TAU * 6.0) + vnoise(v * 4.0, u * 2.0, 4, 18) * 3.0).sin();
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
    tile(8, 0, &mut |px, py, u, v| {
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
    tile(9, 0, &mut |px, py, u, v| {
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

    let plank_colored = |px: u32, py: u32, u: f32, salt: u32,
                         board: [f32; 3], seam: [f32; 3], nail: [f32; 3]| -> [u8; 4] {
        let boards = 4;
        let bh = tp / boards;
        let b = py / bh;
        let seam_row = py % bh == 0 || py % bh == bh - 1;
        let tone = 0.9 + h01(b as i32, 0, salt) * 0.18;
        let joint_u = h01(b as i32, 1, salt ^ 0x55) * 0.8 + 0.1;
        let at_joint = (u - joint_u).abs() < 0.5 / tp as f32 * 2.0;
        if seam_row || at_joint {
            return rgba(seam, 0.9, 255);
        }
        let grain = (vnoise(u * 9.0, (py as f32 / tp as f32) * 3.0, 9, salt ^ 0x99) - 0.5) * 0.16;
        let mut c = rgba(board, tone * (1.0 + grain) * speck(px, py, salt ^ 0x77, 0.04), 255);
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
        let seam = py % bh == 0 || py % bh == bh - 1;
        let tone = 0.9 + h01(b as i32, 0, salt) * 0.18;
        // End-of-board joints staggered per row.
        let joint_u = h01(b as i32, 1, salt ^ 0x55) * 0.8 + 0.1;
        let at_joint = (u - joint_u).abs() < 0.5 / tp as f32 * 2.0;
        if seam || at_joint {
            return rgba([92.0, 70.0, 40.0], 0.9, 255);
        }
        let grain = (vnoise(u * 9.0, (py as f32 / tp as f32) * 3.0, 9, salt ^ 0x99) - 0.5) * 0.16;
        let mut c = rgba([164.0, 132.0, 80.0], tone * (1.0 + grain) * speck(px, py, salt ^ 0x77, 0.04), 255);
        // Nails at board ends.
        let mid = b * bh + bh / 2;
        let e = (tp / 16).max(1);
        if py.abs_diff(mid) < e && (px < 2 * e && px >= e || px >= tp - 2 * e && px < tp - e) {
            c = rgba([80.0, 74.0, 64.0], 1.0, 255);
        }
        c
    };

    // (10,0) planks
    tile(10, 0, &mut |px, py, u, v| plank_at(px, py, u, v, 26));

    // (11,0) bedrock: harsh light/dark blotches.
    tile(11, 0, &mut |px, py, u, v| {
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
    tile(12, 0, &mut |px, py, u, v| {
        let band = ((v + fbm(u, v, 3, 29) * 0.4) * std::f32::consts::TAU * 2.0).sin() * 0.09;
        let t = fbm(u, v, 4, 30);
        let c = mix3([40.0, 78.0, 196.0], [70.0, 116.0, 236.0], t);
        rgba(c, 1.0 + band + (speck(px, py, 31, 0.03) - 1.0), 168)
    });

    // (13,0) crafting table top: planks with a dark tool-grid border.
    tile(13, 0, &mut |px, py, u, v| {
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
    tile(14, 0, &mut |px, py, u, v| {
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
    tile(7, 2, &mut |px, py, u, v| {
        // snow: bright white with faint blue shading.
        let t = fbm(u, v, 6, 50);
        let c = mix3([230.0, 236.0, 244.0], [250.0, 252.0, 255.0], t);
        rgba(c, speck(px, py, 51, 0.03) * emboss(px, py, tp), 255)
    });
    tile(8, 2, &mut |px, py, u, v| {
        // ice: pale glossy blue with lighter crack veins.
        let t = fbm(u, v, 4, 52);
        let mut c = mix3([148.0, 186.0, 224.0], [190.0, 220.0, 246.0], t);
        let vein = (vnoise(u * 5.0, v * 5.0, 5, 53) - 0.5).abs();
        if vein < 0.04 {
            c = mix3(c, [235.0, 245.0, 255.0], 0.8);
        }
        // Diagonal gloss band.
        let gloss = ((u + v) * std::f32::consts::TAU * 1.5).sin();
        rgba(c, (1.0 + gloss * 0.04) * speck(px, py, 54, 0.02) * emboss(px, py, tp), 255)
    });
    tile(9, 2, &mut |px, py, u, _v| {
        // cactus side: vertical ribs with pale spines.
        let rib = ((u * std::f32::consts::TAU * 4.0).sin() * 0.5 + 0.5) * 0.3;
        let c = mix3([44.0, 96.0, 36.0], [88.0, 148.0, 62.0], 0.4 + rib);
        let spine = hash(px as i32, py as i32, 55) % 37 == 0;
        if spine {
            rgba([220.0, 228.0, 190.0], 1.0, 255)
        } else {
            rgba(c, speck(px, py, 56, 0.06), 255)
        }
    });
    tile(10, 2, &mut |px, py, u, v| {
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
        Wood { slot: 43, bark: [201.0, 196.0, 182.0], birch_flecks: true,
               leaf_dark: [62.0, 110.0, 40.0], leaf_light: [112.0, 168.0, 66.0] },
        Wood { slot: 46, bark: [68.0, 50.0, 32.0], birch_flecks: false,
               leaf_dark: [26.0, 60.0, 38.0], leaf_light: [52.0, 96.0, 66.0] },
        Wood { slot: 49, bark: [94.0, 66.0, 40.0], birch_flecks: false,
               leaf_dark: [44.0, 124.0, 26.0], leaf_light: [88.0, 188.0, 50.0] },
        Wood { slot: 52, bark: [122.0, 108.0, 92.0], birch_flecks: false,
               leaf_dark: [86.0, 102.0, 46.0], leaf_light: [128.0, 148.0, 72.0] },
    ];
    for wd in woods {
        // Each tile's coords derive from its own slot — families may cross
        // atlas row boundaries (e.g. spruce leaves at slot 48 = row 3).
        let (tx, ty) = (wd.slot % 16, wd.slot / 16);
        let (tx1, ty1) = ((wd.slot + 1) % 16, (wd.slot + 1) / 16);
        let (tx2, ty2) = ((wd.slot + 2) % 16, (wd.slot + 2) / 16);
        let bark = wd.bark;
        let flecks = wd.birch_flecks;
        // Bark side.
        tile(tx, ty, &mut |px, py, u, v| {
            if flecks {
                // Birch: pale bark with short dark horizontal flecks.
                let dash = hash(px as i32 / 5, py as i32, 60) % 11 == 0
                    && px % 5 < 3;
                if dash {
                    return rgba([38.0, 34.0, 30.0], 1.0, 255);
                }
                let t = fbm(u, v, 5, 61);
                rgba(mix3(bark, [170.0, 166.0, 154.0], t * 0.5), speck(px, py, 62, 0.04), 255)
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
        tile(tx1, ty1, &mut |px, py, u, v| {
            let dx = u - 0.5;
            let dy = v - 0.5;
            let ang = dy.atan2(dx);
            let wob = vnoise(ang.cos() * 2.0 + 2.0, ang.sin() * 2.0 + 2.0, 4, 66) * 0.06;
            let r = (dx * dx + dy * dy).sqrt() + wob;
            if r > 0.46 {
                rgba([bark[0] * 0.9, bark[1] * 0.9, bark[2] * 0.9], speck(px, py, 67, 0.08), 255)
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
        tile(tx2, ty2, &mut |px, py, u, v| {
            let t = fbm(u, v, 6, 69);
            let mut c = mix3(wd.leaf_dark, wd.leaf_light, t);
            let pocket = h01(px as i32, py as i32, 70 + wd.slot as u32);
            if pocket > 0.93 {
                c = mix3(c, [wd.leaf_light[0] + 50.0, wd.leaf_light[1] + 40.0, wd.leaf_light[2] + 30.0], 0.8);
            } else if pocket < 0.10 {
                c = mix3(c, [8.0, 22.0, 8.0], 0.7);
            }
            rgba(c, 1.0, 255)
        });
    }

    // Per-wood planks (oak planks stay at slot 10).
    let plank_sets: [(u32, [f32; 3], [f32; 3]); 4] = [
        (55, [196.0, 178.0, 138.0], [128.0, 114.0, 86.0]),  // birch: pale
        (56, [104.0, 78.0, 48.0], [58.0, 42.0, 26.0]),      // spruce: dark
        (57, [156.0, 106.0, 76.0], [96.0, 60.0, 40.0]),     // jungle: ruddy
        (58, [168.0, 96.0, 54.0], [104.0, 56.0, 30.0]),     // acacia: orange
    ];
    for (slot, board, seam) in plank_sets {
        let (tx, ty) = (slot % 16, slot / 16);
        tile(tx, ty, &mut |px, py, u, _v| {
            plank_colored(px, py, u, 40 + slot, board, seam, [80.0, 74.0, 64.0])
        });
    }

    // (15,0) unknown/missing texture: magenta checkerboard.
    tile(15, 0, &mut |px, py, _u, _v| {
        let k = (tp / 8).max(1);
        if ((px / k) + (py / k)) % 2 == 0 { [230, 0, 230, 255] } else { [20, 0, 20, 255] }
    });

    // Row 1: crack overlay stages, radial cracks scaled to resolution.
    for stage in 0..4u32 {
        tile(stage, 1, &mut |px, py, u, v| {
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
                if d < 0.16 && r < max_r + h01(px as i32, py as i32, 202) as f32 * 0.12 {
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
        tile(tx, 2, &mut |px, py, _u, _v| {
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
