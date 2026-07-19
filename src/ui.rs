//! Immediate-mode 2D UI: colored/atlas-textured quads and a 5x7 pixel font.
//! Coordinates are window pixels, origin top-left.

use bytemuck::{Pod, Zeroable};

use crate::atlas::ATLAS_TILES;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct UiVertex {
    pub pos: [f32; 2],
    /// uv.x < 0 means "solid color, no texture"
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

pub struct UiBatch {
    pub verts: Vec<UiVertex>,
}

impl UiBatch {
    pub fn new() -> UiBatch {
        UiBatch {
            verts: Vec::with_capacity(1024),
        }
    }

    pub fn clear(&mut self) {
        self.verts.clear();
    }

    /// Solid-color rectangle.
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        self.quad([x, y], [x + w, y + h], [-1.0, -1.0], [-1.0, -1.0], color);
    }

    /// Rectangle textured with an atlas tile.
    /// Draw an atlas tile by slot; coordinates derive here so no call
    /// site ever hardcodes the atlas grid width.
    pub fn tile(&mut self, x: f32, y: f32, w: f32, h: f32, slot: u16, tint: [f32; 4]) {
        let (tx, ty) = (slot as u32 % ATLAS_TILES, slot as u32 / ATLAS_TILES);
        let ts = 1.0 / ATLAS_TILES as f32;
        let inset = ts / 32.0;
        let u0 = tx as f32 * ts + inset;
        let v0 = ty as f32 * ts + inset;
        let u1 = (tx + 1) as f32 * ts - inset;
        let v1 = (ty + 1) as f32 * ts - inset;
        self.quad([x, y], [x + w, y + h], [u0, v0], [u1, v1], tint);
    }

    fn quad(
        &mut self,
        min: [f32; 2],
        max: [f32; 2],
        uv0: [f32; 2],
        uv1: [f32; 2],
        color: [f32; 4],
    ) {
        let v = |x: f32, y: f32, u: f32, vv: f32| UiVertex {
            pos: [x, y],
            uv: [u, vv],
            color,
        };
        let (a, b, c, d) = (
            v(min[0], min[1], uv0[0], uv0[1]),
            v(max[0], min[1], uv1[0], uv0[1]),
            v(max[0], max[1], uv1[0], uv1[1]),
            v(min[0], max[1], uv0[0], uv1[1]),
        );
        self.verts.extend_from_slice(&[a, b, c, a, c, d]);
    }

    /// Draw text (A-Z, 0-9, space, and a few symbols). `s` is pixel size of
    /// one font pixel. Returns the width drawn.
    pub fn text(&mut self, x: f32, y: f32, s: f32, msg: &str, color: [f32; 4]) -> f32 {
        let mut cx = x;
        for ch in msg.chars() {
            let glyph = glyph(ch.to_ascii_uppercase());
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..5 {
                    if bits & (0b10000 >> col) != 0 {
                        self.rect(cx + col as f32 * s, y + row as f32 * s, s, s, color);
                    }
                }
            }
            cx += 6.0 * s;
        }
        cx - x
    }

    pub fn text_width(s: f32, msg: &str) -> f32 {
        msg.chars().count() as f32 * 6.0 * s
    }

    /// Text with a dark drop shadow (Minecraft style).
    pub fn text_shadow(&mut self, x: f32, y: f32, s: f32, msg: &str, color: [f32; 4]) {
        self.text(x + s, y + s, s, msg, [0.15, 0.15, 0.15, color[3]]);
        self.text(x, y, s, msg, color);
    }

    /// 7x7 pixel heart. kind: 0 = empty, 1 = half, 2 = full.
    pub fn heart(&mut self, x: f32, y: f32, s: f32, kind: u8) {
        const HEART: [u8; 7] = [
            0b0110110, 0b1111111, 0b1111111, 0b1111111, 0b0111110, 0b0011100, 0b0001000,
        ];
        let bg = [0.25, 0.05, 0.05, 0.9];
        let fg = [0.85, 0.1, 0.1, 1.0];
        for (row, bits) in HEART.iter().enumerate() {
            for col in 0..7 {
                if bits & (0b1000000 >> col) != 0 {
                    let filled = match kind {
                        2 => true,
                        1 => col < 4,
                        _ => false,
                    };
                    let c = if filled { fg } else { bg };
                    self.rect(x + col as f32 * s, y + row as f32 * s, s, s, c);
                }
            }
        }
    }

    /// 7x7 air bubble.
    pub fn bubble(&mut self, x: f32, y: f32, s: f32) {
        const BUBBLE: [u8; 7] = [
            0b0011100, 0b0100010, 0b1000001, 0b1001001, 0b1000001, 0b0100010, 0b0011100,
        ];
        let c = [0.75, 0.85, 1.0, 1.0];
        for (row, bits) in BUBBLE.iter().enumerate() {
            for col in 0..7 {
                if bits & (0b1000000 >> col) != 0 {
                    self.rect(x + col as f32 * s, y + row as f32 * s, s, s, c);
                }
            }
        }
    }
}

/// 5x7 font. Each byte is one row, low 5 bits used (bit 4 = leftmost).
fn glyph(ch: char) -> [u8; 7] {
    match ch {
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        '3' => [
            0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
        ],
        '6' => [
            0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
        ],
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'C' => [
            0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110,
        ],
        'D' => [
            0b11100, 0b10010, 0b10001, 0b10001, 0b10001, 0b10010, 0b11100,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'G' => [
            0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'I' => [
            0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        'J' => [
            0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001,
        ],
        'X' => [
            0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b01010, 0b10001,
        ],
        'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'Z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        ':' => [
            0b00000, 0b00100, 0b00100, 0b00000, 0b00100, 0b00100, 0b00000,
        ],
        '?' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b00000, 0b00100,
        ],
        '-' => [
            0b00000, 0b00000, 0b00000, 0b01110, 0b00000, 0b00000, 0b00000,
        ],
        '.' => [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00110, 0b00110,
        ],
        '/' => [
            0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000,
        ],
        _ => [0; 7], // space / unknown
    }
}
