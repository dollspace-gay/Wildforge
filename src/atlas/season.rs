//! Seasonal color transforms applied to completed atlas pixels.

use super::*;

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
