//! The script's drawing buffer: RGB332, one byte a pixel, with the 182-colour
//! script palette. The palette is generated, not copied: an RGB cube less its
//! four greys, after sixteen greys and the accent ramps, as wfeature's
//! `scriptColor` derives it for the earlier SGS versions (MIT); the bitmap
//! format is the same in version 4.

use alloc::{vec, vec::Vec};

use wie_backend::canvas::{Canvas, Clip, Color, Image, ImageBufferCanvas, PixelType, Rgb332Pixel, TextAlignment, VecImageBuffer};

/// The colour that is not drawn.
pub const TRANSPARENT: i32 = 4;

/// A drawing colour given as RGB rather than a script colour: this plus the
/// RGB332 byte.
pub const RAW_COLOR: i32 = 0x1000;

/// Script colour `c` (0..182) as RGB332.
pub fn script_color(c: i32) -> u8 {
    let c = c.rem_euclid(182);
    if c >= 140 {
        match c {
            140 => return 0,
            141 => return 0xe0,
            142 => return 0x1c,
            _ => {}
        }
        let (group, phase) = if c >= 158 {
            ((c - 158) / 4, (c - 158) % 4)
        } else {
            ((c - 143) / 3 + 1, (c - 143) % 3)
        };
        if phase == 0 {
            return 0;
        }
        let mut r = if matches!(group, 0 | 1 | 5) { 7 } else { 0 };
        let mut g = if matches!(group, 1..=3) { 7 } else { 0 };
        let mut b = if matches!(group, 3..=5) { 3 } else { 0 };
        if phase == 2 {
            r /= 3;
            g /= 3;
            b /= 3;
        }
        if group == 1 && phase == 3 {
            b = 1;
        }
        return (r << 5 | g << 2 | b) as u8;
    }
    if c < 16 {
        let level = match c {
            0 | 5 | 11 | 15 => 3,
            1 | 6 | 8 | 12 => 2,
            2 | 7 | 9 | 13 => 1,
            _ => 0,
        };
        let v = level * 7 / 3;
        return (v << 5 | v << 2 | level) as u8;
    }
    let mut index = 16;
    for r in [0, 2, 4, 7] {
        for g in 0..8 {
            for b in 0..4 {
                let v = r << 5 | g << 2 | b;
                if matches!(v, 0 | 73 | 146 | 255) {
                    continue;
                }
                if index == c {
                    return v as u8;
                }
                index += 1;
            }
        }
    }
    0
}

/// `c` in one of the seven brightness banks; bank 3 leaves it as it is.
/// A square root for `no_std`, by Newton's method; enough for corner radii.
fn sqrt(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    let mut r = x.max(1.0);
    for _ in 0..20 {
        r = 0.5 * (r + x / r);
    }
    r
}

pub fn bank_color(c: u8, bank: i32) -> u8 {
    let channel = |x: i32| match bank {
        0 => ((x + 10) / 2).min(7),
        1 => ((3 * x + 20) / 5).min(7),
        2 => ((3 * x + 10) / 4).min(7),
        4 => (x + 1) / 3,
        5 => (x + 2) / 5,
        6 => 0,
        _ => x,
    };
    let b = (c & 3) as i32;
    let b = match bank {
        0 => 3,
        1 => (b + 2).min(3),
        2 => (b + 1).min(3),
        4 => b / 2,
        5 | 6 => 0,
        _ => b,
    };
    (channel((c >> 5) as i32) << 5 | channel((c >> 2 & 7) as i32) << 2 | b) as u8
}

pub struct Graphics {
    pub width: i32,
    pub height: i32,
    pub pixels: Vec<u8>,
    /// The last frame the script presented.
    pub shown: Vec<u8>,
    pub backup: Vec<u8>,
    /// x0, y0, x1, y1, the far edges excluded.
    pub clip: (i32, i32, i32, i32),
    pub bank: i32,
    pub color: i32,
    pub text_fg: i32,
    pub text_bg: i32,
    pub text_align: i32,
    /// Whether a frame was presented since the host last took one.
    pub presented: bool,
}

impl Graphics {
    pub fn new(width: i32, height: i32) -> Self {
        let size = (width * height) as usize;
        Self {
            width,
            height,
            pixels: vec![255; size],
            shown: vec![255; size],
            backup: vec![255; size],
            clip: (0, 0, width, height),
            bank: 3,
            color: 3,
            text_fg: 3,
            text_bg: TRANSPARENT,
            text_align: 0,
            presented: false,
        }
    }

    pub fn mapped(&self, c: i32) -> u8 {
        if c >= RAW_COLOR {
            return (c - RAW_COLOR) as u8;
        }
        bank_color(script_color(c), self.bank)
    }

    pub fn point(&mut self, x: i32, y: i32, v: u8) {
        let (x0, y0, x1, y1) = self.clip;
        if x >= x0 && x < x1 && y >= y0 && y < y1 {
            self.pixels[(y * self.width + x) as usize] = v;
        }
    }

    pub fn fill_rect(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, c: i32) {
        if c == TRANSPARENT {
            return;
        }
        let v = self.mapped(c);
        let (x0, y0, xe, ye) = self.clip;
        for y in y1.min(y2).max(y0)..=y1.max(y2).min(ye - 1) {
            for x in x1.min(x2).max(x0)..=x1.max(x2).min(xe - 1) {
                self.pixels[(y * self.width + x) as usize] = v;
            }
        }
    }

    pub fn rect(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, c: i32) {
        if c == TRANSPARENT {
            return;
        }
        let v = self.mapped(c);
        for x in x1.min(x2)..=x1.max(x2) {
            self.point(x, y1, v);
            self.point(x, y2, v);
        }
        for y in y1.min(y2)..=y1.max(y2) {
            self.point(x1, y, v);
            self.point(x2, y, v);
        }
    }

    pub fn line(&mut self, mut x: i32, mut y: i32, x2: i32, y2: i32, c: i32) {
        if c == TRANSPARENT {
            return;
        }
        let v = self.mapped(c);
        let (dx, dy) = ((x2 - x).abs(), (y2 - y).abs());
        let (sx, sy) = (if x2 >= x { 1 } else { -1 }, if y2 >= y { 1 } else { -1 });
        let mut err = dx - dy;
        loop {
            self.point(x, y, v);
            if x == x2 && y == y2 {
                break;
            }
            let e2 = 2 * err;
            if e2 > -dy {
                err -= dy;
                x += sx;
            }
            if e2 < dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// A box in the drawing colour with corners rounded to `radius`, filled
    /// or outlined.
    pub fn round_rect(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, radius: i32, fill: bool) {
        let c = self.color;
        if c == TRANSPARENT {
            return;
        }
        let (x1, x2, y1, y2) = (x1.min(x2), x1.max(x2), y1.min(y2), y1.max(y2));
        let r = radius.abs().min((x2 - x1) / 2).min((y2 - y1) / 2).max(0);
        // How far a row `dy` rows in from the top or bottom edge is cut in.
        let inset = |dy: i32| -> i32 {
            if dy >= r {
                return 0;
            }
            let d = (r - dy) as f32 - 0.5;
            let inner = (r * r) as f32 - d * d;
            (r as f32 - sqrt(inner.max(0.0)) + 0.5) as i32
        };
        let v = self.mapped(c);
        for y in y1..=y2 {
            let k = inset((y - y1).min(y2 - y));
            if fill || y == y1 || y == y2 {
                for x in x1 + k..=x2 - k {
                    self.point(x, y, v);
                }
            } else {
                let previous = inset((y - 1 - y1).min(y2 - y + 1)).max(k);
                for x in x1 + k..=x1 + previous {
                    self.point(x, y, v);
                }
                for x in x2 - previous..=x2 - k {
                    self.point(x, y, v);
                }
            }
        }
    }

    /// Moves the drawing buffer (0) or the backdrop (1) by `dx`, `dy`,
    /// filling what is uncovered with white (mode 0) or wrapping (mode 1).
    pub fn scroll(&mut self, buffer: i32, dx: i32, dy: i32, mode: i32) {
        if !(0..=1).contains(&buffer) || !(0..=1).contains(&mode) {
            return;
        }
        let (w, h) = (self.width, self.height);
        let target = if buffer == 0 { &mut self.pixels } else { &mut self.backup };
        let old = target.clone();
        for y in 0..h {
            for x in 0..w {
                let (sx, sy) = (x - dx, y - dy);
                target[(y * w + x) as usize] = if mode == 1 {
                    old[(sy.rem_euclid(h) * w + sx.rem_euclid(w)) as usize]
                } else if (0..w).contains(&sx) && (0..h).contains(&sy) {
                    old[(sy * w + sx) as usize]
                } else {
                    255
                };
            }
        }
    }

    pub fn fill(&mut self, v: u8) {
        self.pixels.fill(v);
    }

    /// A script bitmap: type, width, height, signed origin x and y, then a
    /// palette of script colours (none for type 8) and the pixels as one
    /// MSB-first bitstream. Colour 4 is not drawn.
    pub fn bitmap(&mut self, data: &[u8], x: i32, y: i32, mirror: bool, palette_override: Option<&[u8]>) -> Result<(), &'static str> {
        if data.len() >= 2 && (2..=8).contains(&data[0]) && (data[1] == 0 || data.len() >= 3 && data[2] == 0) {
            return Ok(());
        }
        if data.len() < 5 {
            return Err("bitmap header truncated");
        }
        let (kind, w, h) = (data[0], data[1] as i32, data[2] as i32);
        let (bits, colors) = match kind {
            2 | 5 => (1, 2),
            3 | 6 => (2, 4),
            4 | 7 => (4, 16),
            8 => (8, 256),
            _ => return Err("bitmap type unsupported"),
        };
        let offset = match kind {
            2 | 3 => 5 + colors / 2,
            5..=7 => 5 + colors,
            _ => 5,
        };
        let needed = offset + ((w * h) as usize * bits).div_ceil(8);
        if needed > data.len() {
            return Err("bitmap truncated");
        }
        let palette_bytes = palette_override.unwrap_or(&data[5..offset]);
        let palette: Vec<i32> = (0..colors)
            .map(|i| match kind {
                2 | 3 => palette_bytes.get(i / 2).map_or(0, |b| (b >> (4 - 4 * (i % 2)) & 15) as i32),
                5..=7 => palette_bytes.get(i).map_or(0, |&b| b as i32),
                _ => i as i32,
            })
            .collect();
        let (ox, oy) = (data[3] as i8 as i32, data[4] as i8 as i32);
        let x = if mirror { x + ox - w } else { x - ox };
        let y = y - oy;
        for i in 0..(w * h) as usize {
            let bit = i * bits;
            let index = (data[offset + bit / 8] >> (8 - bits - bit % 8)) as usize & ((1 << bits) - 1);
            let c = palette[index];
            if c == TRANSPARENT {
                continue;
            }
            let v = if c < 182 { self.mapped(c) } else { 0 };
            let px = i as i32 % w;
            let px = if mirror { w - 1 - px } else { px };
            self.point(x + px, y + i as i32 / w, v);
        }
        Ok(())
    }

    /// `text` at `x`, `y` (its top), aligned on `x` as the script asked, in
    /// the text colour; over a filled background when `background` is set.
    pub fn text(&mut self, x: i32, y: i32, text: &str, background: bool) {
        const HEIGHT: f32 = 12.0;
        const BASELINE: f32 = 10.0;
        let alignment = match self.text_align {
            1 => TextAlignment::Center,
            2 => TextAlignment::Right,
            _ => TextAlignment::Left,
        };
        let (x0, y0, x1, y1) = self.clip;
        let clip = Clip {
            x: x0,
            y: y0,
            width: (x1 - x0).max(0) as u32,
            height: (y1 - y0).max(0) as u32,
        };
        let pixels = core::mem::take(&mut self.pixels);
        let mut canvas = ImageBufferCanvas::new(VecImageBuffer::<Rgb332Pixel>::from_raw(self.width as u32, self.height as u32, pixels));
        if background && self.text_bg != TRANSPARENT {
            let width = text.chars().count() as i32 * 6;
            let left = match self.text_align {
                1 => x - width / 2,
                2 => x - width,
                _ => x,
            };
            let bg = Rgb332Pixel::to_color(self.mapped(self.text_bg));
            canvas.fill_rect(left - 1, y - 1, (width + 2) as u32, HEIGHT as u32 + 2, bg, clip);
        }
        if self.text_fg != TRANSPARENT {
            let fg: Color = Rgb332Pixel::to_color(self.mapped(self.text_fg));
            canvas.draw_text(text, x, y, HEIGHT, BASELINE, alignment, fg, clip);
        }
        self.pixels = canvas.into_inner().raw().into_owned();
    }

    pub fn present(&mut self) {
        self.shown.copy_from_slice(&self.pixels);
        self.presented = true;
    }

    pub fn frame(&self) -> VecImageBuffer<Rgb332Pixel> {
        VecImageBuffer::from_raw(self.width as u32, self.height as u32, self.shown.clone())
    }
}
