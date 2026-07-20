//! Player appearance: a palette-based style that packs to a u32 for
//! config.txt and the wire (docs/player-model-plan.md). The atlas
//! derives pre-tinted variant tiles for every palette entry, so the
//! renderer just picks slots — no per-vertex tint plumbing.

/// Skin tones, light to deep — plus one frankly unnatural, for the
/// ghosts among us.
pub const SKIN_TONES: [[f32; 3]; 6] = [
    [1.08, 0.94, 0.84],
    [1.02, 0.85, 0.70],
    [0.92, 0.72, 0.55],
    [0.72, 0.53, 0.38],
    [0.50, 0.36, 0.27],
    [0.72, 0.80, 0.92],
];

pub const HAIR_COLORS: [[f32; 3]; 8] = [
    [0.16, 0.13, 0.11], // near-black
    [0.35, 0.24, 0.15], // dark brown
    [0.55, 0.38, 0.22], // chestnut
    [0.80, 0.62, 0.32], // blond
    [0.62, 0.28, 0.16], // auburn
    [0.55, 0.55, 0.58], // silver
    [0.70, 0.30, 0.55], // rose
    [0.28, 0.45, 0.60], // slate blue
];

pub const SHIRT_COLORS: [[f32; 3]; 10] = [
    [0.36, 0.48, 0.60], // river blue
    [0.48, 0.56, 0.38], // sage
    [0.62, 0.40, 0.32], // clay
    [0.55, 0.45, 0.62], // heather
    [0.65, 0.58, 0.40], // straw
    [0.40, 0.52, 0.50], // spruce
    [0.60, 0.35, 0.45], // mulberry
    [0.42, 0.42, 0.46], // slate
    [0.70, 0.66, 0.60], // undyed
    [0.30, 0.30, 0.32], // charcoal
];

pub const TROUSER_COLORS: [[f32; 3]; 6] = [
    [0.42, 0.36, 0.30], // walnut
    [0.35, 0.38, 0.45], // indigo
    [0.30, 0.32, 0.30], // moss dark
    [0.52, 0.46, 0.38], // canvas
    [0.28, 0.26, 0.26], // soot
    [0.48, 0.40, 0.48], // plum grey
];

pub const HAIR_NAMES: [&str; 8] = [
    "RAVEN", "UMBER", "CHESTNUT", "WHEAT", "AUBURN", "SILVER", "ROSE", "SLATE",
];
pub const SHIRT_NAMES: [&str; 10] = [
    "RIVER", "SAGE", "CLAY", "HEATHER", "STRAW", "SPRUCE", "MULBERRY", "SLATE", "UNDYED",
    "CHARCOAL",
];
pub const TROUSER_NAMES: [&str; 6] = ["WALNUT", "INDIGO", "MOSS", "CANVAS", "SOOT", "PLUM"];

pub const HAIR_STYLE_NAMES: [&str; 4] = ["BALD", "CROPPED", "SHORT", "LONG"];
pub const BEARD_NAMES: [&str; 4] = ["NONE", "MOUSTACHE", "TRIMMED", "FULL"];
pub const LEGWEAR_NAMES: [&str; 2] = ["TROUSERS", "SKIRT"];
pub const BUILD_NAMES: [&str; 3] = ["SLIGHT", "STANDARD", "BROAD"];

/// A player's chosen look: palette indices plus shape choices.
/// Everything defaults neutral; gendered reads are opt-in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Style {
    pub skin: u8,
    pub hair: u8,
    pub shirt: u8,
    pub trousers: u8,
    /// 0 bald, 1 cropped, 2 short (default), 3 long.
    pub hair_style: u8,
    /// 0 none (default), 1 moustache, 2 trimmed, 3 full.
    pub beard: u8,
    /// 0 trousers (default), 1 skirt (over leggings).
    pub legwear: u8,
    /// 0 slight, 1 standard (default), 2 broad.
    pub build: u8,
}

impl Default for Style {
    /// The gender-neutral default: mid skin, dark-brown short hair,
    /// sage shirt, walnut trousers, no beard, standard build.
    fn default() -> Style {
        Style {
            skin: 2,
            hair: 1,
            shirt: 1,
            trousers: 0,
            hair_style: 2,
            beard: 0,
            legwear: 0,
            build: 1,
        }
    }
}

impl Style {
    /// Bit-packed: skin 3 | hair 3 | shirt 4 | trousers 3 |
    /// hair_style 2 | beard 2 | legwear 1 | build 2 (20 bits).
    pub fn pack(self) -> u32 {
        (self.skin as u32)
            | ((self.hair as u32) << 3)
            | ((self.shirt as u32) << 6)
            | ((self.trousers as u32) << 10)
            | ((self.hair_style as u32) << 13)
            | ((self.beard as u32) << 15)
            | ((self.legwear as u32) << 17)
            | ((self.build as u32) << 18)
    }

    /// Unpack, clamping out-of-range indices to valid entries.
    pub fn unpack(v: u32) -> Style {
        Style {
            skin: ((v & 0x7) as u8).min(SKIN_TONES.len() as u8 - 1),
            hair: (((v >> 3) & 0x7) as u8).min(HAIR_COLORS.len() as u8 - 1),
            shirt: (((v >> 6) & 0xf) as u8).min(SHIRT_COLORS.len() as u8 - 1),
            trousers: (((v >> 10) & 0x7) as u8).min(TROUSER_COLORS.len() as u8 - 1),
            hair_style: (((v >> 13) & 0x3) as u8).min(HAIR_STYLE_NAMES.len() as u8 - 1),
            beard: (((v >> 15) & 0x3) as u8).min(BEARD_NAMES.len() as u8 - 1),
            legwear: (((v >> 17) & 0x1) as u8).min(LEGWEAR_NAMES.len() as u8 - 1),
            build: (((v >> 18) & 0x3) as u8).min(BUILD_NAMES.len() as u8 - 1),
        }
    }
}

// ---- variant tile layout (top of the atlas, derived at build) ----
//
// The base player tiles are painted near-greyscale; build_atlas
// multiplies each by every palette color into these reserved slots.
// Extra base tiles (hair lengths, beards) also live in the reserved
// region, just below the variants. Mods allocate upward from
// FIRST_FREE_SLOT and never reach here.

pub const N_SKIN: u16 = SKIN_TONES.len() as u16;
pub const N_HAIR: u16 = HAIR_COLORS.len() as u16;
pub const N_SHIRT: u16 = SHIRT_COLORS.len() as u16;
pub const N_TROUSERS: u16 = TROUSER_COLORS.len() as u16;

/// Hair-color-tinted families: 3 side lengths + crown + 3 beards.
const HAIR_FAMS: u16 = 7;

/// skins ×6, faces ×6, hair-tinted ×(7×8), shirts ×10, trousers ×6
/// = 84 variant slots at the very top of the atlas.
pub const VARIANT_SLOTS: u16 = N_SKIN * 2 + HAIR_FAMS * N_HAIR + N_SHIRT + N_TROUSERS;
pub const VARIANT_BASE: u16 = 1024 - VARIANT_SLOTS;
/// Extra neutral base tiles (painters live here, below the variants).
pub const EXTRA_BASE: u16 = VARIANT_BASE - 8;

pub fn skin_tile(s: &Style) -> u16 {
    VARIANT_BASE + s.skin as u16
}
pub fn face_tile(s: &Style) -> u16 {
    VARIANT_BASE + N_SKIN + s.skin as u16
}
/// fam: 0 cropped side, 1 short side, 2 long side, 3 crown,
/// 4 moustache, 5 trimmed beard, 6 full beard.
fn hair_fam_tile(fam: u16, color: u8) -> u16 {
    VARIANT_BASE + N_SKIN * 2 + fam * N_HAIR + color as u16
}
/// The side-shell tile for the chosen hair length (None if bald).
pub fn hair_tile(s: &Style) -> Option<u16> {
    match s.hair_style {
        0 => None,
        1 => Some(hair_fam_tile(0, s.hair)),
        3 => Some(hair_fam_tile(2, s.hair)),
        _ => Some(hair_fam_tile(1, s.hair)),
    }
}
pub fn hair_top_tile(s: &Style) -> u16 {
    hair_fam_tile(3, s.hair)
}
/// The shell's face-side tile: long hair still wears the short
/// fringe up front — lengths fall at the sides and back only.
pub fn hair_front_tile(s: &Style) -> Option<u16> {
    match s.hair_style {
        0 => None,
        1 => Some(hair_fam_tile(0, s.hair)),
        _ => Some(hair_fam_tile(1, s.hair)),
    }
}
/// The face-band tile for the chosen facial hair (None if clean).
pub fn beard_tile(s: &Style) -> Option<u16> {
    match s.beard {
        1 => Some(hair_fam_tile(4, s.hair)),
        2 => Some(hair_fam_tile(5, s.hair)),
        3 => Some(hair_fam_tile(6, s.hair)),
        _ => None,
    }
}
pub fn shirt_tile(s: &Style) -> u16 {
    VARIANT_BASE + N_SKIN * 2 + HAIR_FAMS * N_HAIR + s.shirt as u16
}
pub fn trouser_tile(s: &Style) -> u16 {
    VARIANT_BASE + N_SKIN * 2 + HAIR_FAMS * N_HAIR + N_SHIRT + s.trousers as u16
}
