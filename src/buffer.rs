//! The mutable text buffer shared by the `screen` and `gpu` components.
//! Mirrors `li.cil.oc.util.TextBuffer` and `TextBufferProxy`.

use crate::color::{self, Color, ColorDepth, Format};

pub struct TextBuffer {
    pub width: i32,
    pub height: i32,
    pub max_width: i32,
    pub max_height: i32,
    pub max_depth: ColorDepth,
    pub format: Format,
    /// Active foreground/background colors (values are RGB when not from the
    /// palette, otherwise palette indices).
    pub foreground: Color,
    pub background: Color,
    /// Row-major code points.
    pub chars: Vec<u32>,
    /// Row-major packed foreground/background pairs.
    pub colors: Vec<u16>,
    pub viewport_width: i32,
    pub viewport_height: i32,
    /// Bumped whenever the rendered content changes.
    pub revision: u64,
    pub on: bool,
}

impl TextBuffer {
    pub fn new(width: i32, height: i32, max_depth: ColorDepth) -> TextBuffer {
        let format = Format::new(ColorDepth::OneBit);
        let fg = Color::rgb(0xFFFFFF);
        let bg = Color::rgb(0x000000);
        let packed = format.pack(fg, bg);
        let n = (width * height) as usize;
        let mut buffer = TextBuffer {
            width,
            height,
            max_width: width,
            max_height: height,
            max_depth,
            format,
            foreground: fg,
            background: bg,
            chars: vec![0x20; n],
            colors: vec![packed; n],
            viewport_width: width,
            viewport_height: height,
            revision: 0,
            on: true,
        };
        buffer.set_foreground(0xFFFFFF, false).ok();
        buffer.set_background(0x000000, false).ok();
        buffer
    }

    fn idx(&self, col: i32, row: i32) -> usize {
        (row * self.width + col) as usize
    }

    pub fn get(&self, col: i32, row: i32) -> Option<u32> {
        if col < 0 || col >= self.width || row < 0 || row >= self.height {
            None
        } else {
            Some(self.chars[self.idx(col, row)])
        }
    }

    pub fn set_foreground(&mut self, value: i32, is_palette: bool) -> Result<(), String> {
        let c = Color {
            value,
            is_palette,
        };
        self.format.validate(c)?;
        if self.foreground != c {
            self.foreground = c;
            self.revision += 1;
        }
        Ok(())
    }

    pub fn set_background(&mut self, value: i32, is_palette: bool) -> Result<(), String> {
        let c = Color {
            value,
            is_palette,
        };
        self.format.validate(c)?;
        if self.background != c {
            self.background = c;
            self.revision += 1;
        }
        Ok(())
    }

    pub fn get_palette_color(&self, index: i32) -> Result<u32, String> {
        if self.format.depth == ColorDepth::OneBit {
            return Err("palette not available".into());
        }
        if !(0..16).contains(&index) {
            return Err("invalid palette index".into());
        }
        Ok(self.format.palette[index as usize])
    }

    pub fn set_palette_color(&mut self, index: i32, value: u32) -> Result<u32, String> {
        if self.format.depth == ColorDepth::OneBit {
            return Err("palette not available".into());
        }
        if !(0..16).contains(&index) {
            return Err("invalid palette index".into());
        }
        let old = self.format.palette[index as usize];
        self.format.palette[index as usize] = value & 0xFFFFFF;
        self.revision += 1;
        Ok(old)
    }

    /// Change the active color depth, converting existing cells.
    pub fn set_depth(&mut self, depth: ColorDepth) -> bool {
        if depth == self.format.depth {
            return false;
        }
        let old = self.format.clone();
        self.format = Format::new(depth);
        for packed in self.colors.iter_mut() {
            let fg_rgb = color::unpack_foreground(*packed, &old);
            let bg_rgb = color::unpack_background(*packed, &old);
            *packed = self
                .format
                .pack(Color::rgb(fg_rgb as i32), Color::rgb(bg_rgb as i32));
        }
        // Re-validate current colors against the new format.
        let fg = self.foreground;
        let bg = self.background;
        self.foreground = Color::rgb(color::unpack_foreground(
            self.format.pack(fg, bg) & 0xFF00,
            &self.format,
        ) as i32);
        self.background = Color::rgb(color::unpack_background(
            self.format.pack(fg, bg),
            &self.format,
        ) as i32);
        self.revision += 1;
        true
    }

    pub fn set_resolution(&mut self, w: i32, h: i32) -> Result<bool, String> {
        if w < 1 || h < 1 || w > self.max_width || h > self.max_width || w * h > self.max_width * self.max_height {
            return Err("unsupported resolution".into());
        }
        let size_changed = self.resize(w, h);
        let viewport_changed = self.set_viewport(w, h);
        Ok(size_changed || viewport_changed)
    }

    fn resize(&mut self, w: i32, h: i32) -> bool {
        if w == self.width && h == self.height {
            return false;
        }
        let mut new_chars = vec![0x20u32; (w * h) as usize];
        let mut new_colors = vec![self.format.pack(self.foreground, self.background); (w * h) as usize];
        for y in 0..h.min(self.height) {
            for x in 0..w.min(self.width) {
                new_chars[(y * w + x) as usize] = self.chars[(y * self.width + x) as usize];
                new_colors[(y * w + x) as usize] = self.colors[(y * self.width + x) as usize];
            }
        }
        self.chars = new_chars;
        self.colors = new_colors;
        self.width = w;
        self.height = h;
        self.revision += 1;
        true
    }

    pub fn set_viewport(&mut self, w: i32, h: i32) -> bool {
        if w == self.viewport_width && h == self.viewport_height {
            return false;
        }
        self.viewport_width = w;
        self.viewport_height = h;
        true
    }

    pub fn set(&mut self, col: i32, row: i32, s: &str, vertical: bool) -> bool {
        let chars: Vec<char> = s.chars().collect();
        let s_len = chars.len() as i32;
        if vertical {
            if col < 0 || col >= self.width {
                return false;
            }
            let mut changed = false;
            let mut cx = 0usize;
            for y in row..(row + s_len).min(self.height) {
                if y >= 0 {
                    let c = chars[cx] as u32;
                    changed |= self.set_char(col, y, c);
                    cx += 1;
                }
            }
            changed
        } else {
            if row < 0 || row >= self.height {
                return false;
            }
            let mut changed = false;
            let mut bx = col.max(0);
            let mut cx = 0usize;
            while bx < (col + s_len).min(self.width) {
                if cx >= chars.len() {
                    break;
                }
                let c = chars[cx] as u32;
                changed |= self.set_char(bx, row, c);
                bx += color::wcwidth(c).max(1);
                cx += 1;
            }
            changed
        }
    }

    fn set_char(&mut self, x: i32, y: i32, c: u32) -> bool {
        let width = color::wcwidth(c).max(1);
        if width > 1 && x >= self.width - 1 {
            return false;
        }
        let packed = self.format.pack(self.foreground, self.background);
        let mut changed = false;
        let i = self.idx(x, y);
        if self.chars[i] != c || self.colors[i] != packed {
            changed = true;
        }
        self.chars[i] = c;
        self.colors[i] = packed;
        for x1 in (x + 1)..(x + width).min(self.width) {
            let j = self.idx(x1, y);
            self.chars[j] = 0x20;
            self.colors[j] = packed;
        }
        if x > 0 {
            let prev = self.idx(x - 1, y);
            if color::wcwidth(self.chars[prev]) > 1 {
                self.chars[prev] = 0x20;
                changed = true;
            }
        }
        if changed {
            self.revision += 1;
        }
        changed
    }

    pub fn fill(&mut self, col: i32, row: i32, w: i32, h: i32, c: u32) -> bool {
        if w <= 0 || h <= 0 {
            return false;
        }
        if col + w < 0 || row + h < 0 || col >= self.width || row >= self.height {
            return false;
        }
        let mut changed = false;
        for y in row.max(0)..(row + h).min(self.height) {
            let mut bx = col.max(0);
            while bx < (col + w).min(self.width) {
                changed |= self.set_char(bx, y, c);
                bx += color::wcwidth(c).max(1);
            }
        }
        changed
    }

    pub fn copy(&mut self, col: i32, row: i32, w: i32, h: i32, tx: i32, ty: i32) -> bool {
        if w <= 0 || h <= 0 || (tx == 0 && ty == 0) {
            return false;
        }
        let mut changed = false;
        let sx = if tx > 0 { -1 } else { 1 };
        let sy = if ty > 0 { -1 } else { 1 };
        let y_start = if ty > 0 {
            (row + ty + h - 1).min(self.height - 1)
        } else {
            (row + ty).max(0)
        };
        let y_end = if ty > 0 {
            (row + ty).max(0)
        } else {
            (row + ty + h - 1).min(self.height - 1)
        };
        let mut ny = y_start;
        loop {
            let oy = ny - ty;
            if oy >= 0 && oy < self.height {
                let x_start = if tx > 0 {
                    (col + tx + w - 1).min(self.width - 1)
                } else {
                    (col + tx).max(0)
                };
                let x_end = if tx > 0 {
                    (col + tx).max(0)
                } else {
                    (col + tx + w - 1).min(self.width - 1)
                };
                let mut nx = x_start;
                loop {
                    let ox = nx - tx;
                    if ox >= 0 && ox < self.width {
                        let src = self.idx(ox, oy);
                        let dst = self.idx(nx, ny);
                        if self.chars[dst] != self.chars[src] || self.colors[dst] != self.colors[src] {
                            changed = true;
                        }
                        self.chars[dst] = self.chars[src];
                        self.colors[dst] = self.colors[src];
                        let width = color::wcwidth(self.chars[dst]);
                        for offset in 1..width {
                            let j = self.idx(nx + offset, ny);
                            if j < self.chars.len() {
                                self.chars[j] = 0x20;
                            }
                        }
                    }
                    if nx == x_end {
                        break;
                    }
                    nx += sx;
                }
                let left_edge = x_start.min(x_end) - 1;
                if left_edge >= 0 {
                    let edge = self.idx(left_edge, ny);
                    if color::wcwidth(self.chars[edge]) > 1 {
                        self.chars[edge] = 0x20;
                        changed = true;
                    }
                }
            }
            if ny == y_end {
                break;
            }
            ny += sy;
        }
        if changed {
            self.revision += 1;
        }
        changed
    }

    /// Per-cell foreground: palette index or RGB.
    pub fn cell_foreground(&self, col: i32, row: i32) -> (i32, Option<i32>) {
        let packed = self.colors[self.idx(col, row)];
        let value = ((packed >> 8) & 0xFF) as u8;
        if self.format.is_from_palette(value) {
            (value as i32, Some(value as i32))
        } else {
            (self.format.inflate(value) as i32, None)
        }
    }

    /// Per-cell background: palette index or RGB.
    pub fn cell_background(&self, col: i32, row: i32) -> (i32, Option<i32>) {
        let packed = self.colors[self.idx(col, row)];
        let value = (packed & 0xFF) as u8;
        if self.format.is_from_palette(value) {
            (value as i32, Some(value as i32))
        } else {
            (self.format.inflate(value) as i32, None)
        }
    }

    pub fn cell_rgb(&self, col: i32, row: i32) -> (u32, u32) {
        let packed = self.colors[self.idx(col, row)];
        (
            color::unpack_foreground(packed, &self.format),
            color::unpack_background(packed, &self.format),
        )
    }

    /// Returns (foreground RGB, background RGB, foreground palette index,
    /// background palette index) for a cell.
    pub fn cell_colors_full(&self, col: i32, row: i32) -> (i32, i32, Option<i32>, Option<i32>) {
        let packed = self.colors[self.idx(col, row)];
        let fv = ((packed >> 8) & 0xFF) as u8;
        let bv = (packed & 0xFF) as u8;
        (
            self.format.inflate(fv) as i32,
            self.format.inflate(bv) as i32,
            if self.format.is_from_palette(fv) {
                Some(fv as i32)
            } else {
                None
            },
            if self.format.is_from_palette(bv) {
                Some(bv as i32)
            } else {
                None
            },
        )
    }
}
