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

/// A player's chosen look: indices into the palettes above.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Style {
    pub skin: u8,
    pub hair: u8,
    pub shirt: u8,
    pub trousers: u8,
}

impl Default for Style {
    /// The gender-neutral default: mid skin, dark-brown hair, sage
    /// shirt, walnut trousers.
    fn default() -> Style {
        Style {
            skin: 2,
            hair: 1,
            shirt: 1,
            trousers: 0,
        }
    }
}

impl Style {
    pub fn pack(self) -> u32 {
        (self.skin as u32)
            | ((self.hair as u32) << 8)
            | ((self.shirt as u32) << 16)
            | ((self.trousers as u32) << 24)
    }

    /// Unpack, clamping out-of-range indices to valid palette entries.
    pub fn unpack(v: u32) -> Style {
        Style {
            skin: ((v & 0xff) as u8).min(SKIN_TONES.len() as u8 - 1),
            hair: (((v >> 8) & 0xff) as u8).min(HAIR_COLORS.len() as u8 - 1),
            shirt: (((v >> 16) & 0xff) as u8).min(SHIRT_COLORS.len() as u8 - 1),
            trousers: (((v >> 24) & 0xff) as u8).min(TROUSER_COLORS.len() as u8 - 1),
        }
    }
}

// ---- variant tile layout (top of the atlas, derived at build) ----
//
// The base player tiles are painted near-greyscale; build_atlas
// multiplies each by every palette color into these reserved slots.
// Mods allocate upward from FIRST_FREE_SLOT and never reach here.

pub const N_SKIN: u16 = SKIN_TONES.len() as u16;
pub const N_HAIR: u16 = HAIR_COLORS.len() as u16;
pub const N_SHIRT: u16 = SHIRT_COLORS.len() as u16;
pub const N_TROUSERS: u16 = TROUSER_COLORS.len() as u16;

/// skins ×6, faces ×6, hair sides ×8, hair tops ×8, shirts ×10,
/// trousers ×6 = 44 slots: 980..=1023.
pub const VARIANT_SLOTS: u16 = N_SKIN * 2 + N_HAIR * 2 + N_SHIRT + N_TROUSERS;
pub const VARIANT_BASE: u16 = 1024 - VARIANT_SLOTS;

pub fn skin_tile(s: &Style) -> u16 {
    VARIANT_BASE + s.skin as u16
}
pub fn face_tile(s: &Style) -> u16 {
    VARIANT_BASE + N_SKIN + s.skin as u16
}
pub fn hair_tile(s: &Style) -> u16 {
    VARIANT_BASE + N_SKIN * 2 + s.hair as u16
}
pub fn hair_top_tile(s: &Style) -> u16 {
    VARIANT_BASE + N_SKIN * 2 + N_HAIR + s.hair as u16
}
pub fn shirt_tile(s: &Style) -> u16 {
    VARIANT_BASE + N_SKIN * 2 + N_HAIR * 2 + s.shirt as u16
}
pub fn trouser_tile(s: &Style) -> u16 {
    VARIANT_BASE + N_SKIN * 2 + N_HAIR * 2 + N_SHIRT + s.trousers as u16
}
