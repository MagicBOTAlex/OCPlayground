//! Color model mirroring OpenComputers' `PackedColor` / `util.TextBuffer`:
//! 1-bit monochrome, 4-bit mutable 16-color palette, and 8-bit (16 grayscale
//! palette entries + a 240-color RGB cube).

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ColorDepth {
    OneBit,
    FourBit,
    EightBit,
}

impl ColorDepth {
    pub fn bits(self) -> u8 {
        match self {
            ColorDepth::OneBit => 1,
            ColorDepth::FourBit => 4,
            ColorDepth::EightBit => 8,
        }
    }

    pub fn ordinal(self) -> u8 {
        match self {
            ColorDepth::OneBit => 0,
            ColorDepth::FourBit => 1,
            ColorDepth::EightBit => 2,
        }
    }

    pub fn from_ordinal(n: u8) -> ColorDepth {
        match n {
            0 => ColorDepth::OneBit,
            1 => ColorDepth::FourBit,
            _ => ColorDepth::EightBit,
        }
    }
}

/// The default 16-color palette used by 4-bit screens (matches OpenComputers).
pub const DEFAULT_PALETTE: [u32; 16] = [
    0xFFFFFF, 0xFFCC33, 0xCC66CC, 0x6699FF, 0xFFFF33, 0x33CC33, 0xFF6699, 0x333333, 0xCCCCCC,
    0x336699, 0x9933CC, 0x333399, 0x663300, 0x336600, 0xFF3333, 0x000000,
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Color {
    pub value: i32,
    pub is_palette: bool,
}

impl Color {
    pub fn rgb(value: i32) -> Color {
        Color {
            value,
            is_palette: false,
        }
    }
}

fn extract(value: u32) -> (i32, i32, i32) {
    (
        ((value >> 16) & 0xFF) as i32,
        ((value >> 8) & 0xFF) as i32,
        (value & 0xFF) as i32,
    )
}

fn delta(a: u32, b: u32) -> f64 {
    let (ra, ga, ba) = extract(a);
    let (rb, gb, bb) = extract(b);
    let dr = (ra - rb) as f64;
    let dg = (ga - gb) as f64;
    let db = (ba - bb) as f64;
    0.2126 * dr * dr + 0.7152 * dg * dg + 0.0722 * db * db
}

#[derive(Clone)]
pub struct Format {
    pub depth: ColorDepth,
    pub palette: [u32; 16],
    pub monochrome: u32,
    /// Static 240-entry color cube for 8-bit mode.
    cube: [u32; 240],
}

impl Format {
    pub fn new(depth: ColorDepth) -> Format {
        let mut palette = DEFAULT_PALETTE;
        if depth == ColorDepth::EightBit {
            // Hybrid format: palette indices become grayscale (black/white are
            // in the cube).
            for (i, slot) in palette.iter_mut().enumerate() {
                let shade = 0xFF * (i as u32 + 1) / (16 + 1);
                *slot = (shade << 16) | (shade << 8) | shade;
            }
        }
        let mut cube = [0u32; 240];
        let (reds, greens, blues) = (6u32, 8u32, 5u32);
        for (index, slot) in cube.iter_mut().enumerate() {
            let ib = index as u32 % blues;
            let ig = (index as u32 / blues) % greens;
            let ir = (index as u32 / blues / greens) % reds;
            let r = (ir * 0xFF) as f64 / (reds as f64 - 1.0) + 0.5;
            let g = (ig * 0xFF) as f64 / (greens as f64 - 1.0) + 0.5;
            let b = (ib * 0xFF) as f64 / (blues as f64 - 1.0) + 0.5;
            *slot = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
        }
        Format {
            depth,
            palette,
            monochrome: 0xFFFFFF,
            cube,
        }
    }

    pub fn validate(&self, value: Color) -> Result<(), String> {
        match self.depth {
            ColorDepth::OneBit => {
                if value.is_palette {
                    Err("color palette not supported".into())
                } else {
                    Ok(())
                }
            }
            _ => {
                if value.is_palette && (value.value < 0 || value.value >= 16) {
                    Err("invalid palette index".into())
                } else {
                    Ok(())
                }
            }
        }
    }

    pub fn is_from_palette(&self, value: u8) -> bool {
        match self.depth {
            ColorDepth::OneBit => false,
            ColorDepth::FourBit => true,
            ColorDepth::EightBit => value < 16,
        }
    }

    pub fn inflate(&self, value: u8) -> u32 {
        match self.depth {
            ColorDepth::OneBit => {
                if value == 0 {
                    0x000000
                } else {
                    self.monochrome
                }
            }
            ColorDepth::FourBit => self.palette[(value as usize) % 16],
            ColorDepth::EightBit => {
                if value < 16 {
                    self.palette[value as usize]
                } else {
                    self.cube[(value as usize - 16) % 240]
                }
            }
        }
    }

    pub fn deflate(&self, value: Color) -> u8 {
        let rgb = value.value as u32;
        match self.depth {
            ColorDepth::OneBit => {
                if rgb == 0 {
                    0
                } else {
                    1
                }
            }
            ColorDepth::FourBit => {
                if value.is_palette {
                    (value.value.max(0) as u8) % 16
                } else {
                    best_palette_index(&self.palette, rgb)
                }
            }
            ColorDepth::EightBit => {
                if value.is_palette {
                    (value.value.max(0) as u8) % 16
                } else {
                    let palette_index = best_palette_index(&self.palette, rgb);
                    let (r, g, b) = extract(rgb);
                    let ir = (r as f64 * 5.0 / 0xFF as f64 + 0.5) as u8;
                    let ig = (g as f64 * 7.0 / 0xFF as f64 + 0.5) as u8;
                    let ib = (b as f64 * 4.0 / 0xFF as f64 + 0.5) as u8;
                    let deflated = 16u8.wrapping_add(ir * 40 + ig * 5 + ib);
                    if delta(self.inflate(deflated), rgb) < delta(self.inflate(palette_index), rgb) {
                        deflated
                    } else {
                        palette_index
                    }
                }
            }
        }
    }

    pub fn pack(&self, fg: Color, bg: Color) -> u16 {
        ((self.deflate(fg) as u16) << 8) | (self.deflate(bg) as u16)
    }
}

fn best_palette_index(palette: &[u32; 16], rgb: u32) -> u8 {
    let mut best = 0usize;
    let mut best_delta = f64::MAX;
    for (i, color) in palette.iter().enumerate() {
        let d = delta(*color, rgb);
        if d < best_delta {
            best_delta = d;
            best = i;
        }
    }
    best as u8
}

pub fn unpack_foreground(packed: u16, format: &Format) -> u32 {
    format.inflate(((packed >> 8) & 0xFF) as u8)
}

pub fn unpack_background(packed: u16, format: &Format) -> u32 {
    format.inflate((packed & 0xFF) as u8)
}

/// Width of a code point, matching OpenComputers' `FontUtils.wcwidth`, which
/// returns 2 for double-width code points and 1 otherwise (never 0).
pub fn wcwidth(c: u32) -> i32 {
    use unicode_width::UnicodeWidthChar;
    match char::from_u32(c).and_then(|ch| ch.width()) {
        Some(w) if w >= 2 => 2,
        _ => 1,
    }
}
