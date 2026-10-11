//! Drawing the port's own screens - the list, the menus, the settings - at
//! the screen's own resolution.
//!
//! Every screen is laid out on a canvas of at least 320x240 (see
//! `App::menu_size`): its rows, panels and the mouse's hits are in those
//! units. It is drawn `scale` times that size, the whole multiple the screen
//! is of it, so text and edges land on the screen's real pixels instead of
//! being blown up from a small picture. The text is MiniMobile Sans (see
//! `fonts/README.md`), with neodgm for what that lacks. The games keep their
//! own handset fonts; nothing here touches them.

use std::sync::{
    LazyLock,
    atomic::{AtomicU32, Ordering},
};

use ab_glyph::{Font, FontRef, GlyphId, PxScale, ScaleFont, point};
use wie_backend::canvas::{Color, TextAlignment};

pub(crate) const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color { a: 0xff, r, g, b }
}

const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color { a, r, g, b }
}

pub(crate) const LINE: i32 = 18;
pub(crate) const BAR: i32 = 20;

pub(crate) const BACKGROUND: Color = rgb(0x12, 0x16, 0x1c);
/// The fill of a header bar.
pub(crate) const BAR_COLOR: Color = rgb(0x18, 0x1e, 0x25);
pub(crate) const HIGHLIGHT: Color = rgb(0x2e, 0x7d, 0x5b);
pub(crate) const TEXT: Color = rgb(0xf2, 0xf4, 0xf5);
pub(crate) const MUTED: Color = rgb(0x8c, 0x96, 0x9e);
pub(crate) const ROW: Color = rgb(0x22, 0x2a, 0x33);
pub(crate) const ACCENT: Color = rgb(0x7d, 0xe0, 0xa8);
pub(crate) const GREEN: Color = rgb(0x2e, 0x8b, 0x57);
pub(crate) const PANEL: Color = rgb(0x1a, 0x20, 0x28);
pub(crate) const EDGE: Color = rgb(0x2f, 0x39, 0x43);
pub(crate) const DIM: Color = rgba(0, 0, 0, 0xb0);
/// What cannot be undone easily: deleting.
pub(crate) const DANGER: Color = rgb(0xf0, 0x8a, 0x80);
/// Dark text, on a white pill or key.
pub(crate) const INK: Color = rgb(0x11, 0x14, 0x17);
pub(crate) const WHITE: Color = rgb(0xff, 0xff, 0xff);
/// A key cap in the hints that is not the main one.
const CAP: Color = rgb(0x3a, 0x43, 0x4b);
const HINT_TEXT: Color = rgb(0xd6, 0xdc, 0xe0);

static REGULAR: LazyLock<FontRef<'static>> =
    LazyLock::new(|| FontRef::try_from_slice(include_bytes!("../fonts/MiniMobileSans-Regular.ttf")).expect("MiniMobile Sans Regular"));
static SEMIBOLD: LazyLock<FontRef<'static>> =
    LazyLock::new(|| FontRef::try_from_slice(include_bytes!("../fonts/MiniMobileSans-SemiBold.ttf")).expect("MiniMobile Sans SemiBold"));
/// For the syllables and symbols MiniMobile Sans was cut down without.
static FALLBACK: LazyLock<FontRef<'static>> = LazyLock::new(|| FontRef::try_from_slice(include_bytes!("../../fonts/neodgm.ttf")).expect("neodgm"));

/// How many screen pixels a unit of the layout is.
static SCALE: AtomicU32 = AtomicU32::new(1);

pub(crate) fn set_scale(scale: u32) {
    SCALE.store(scale.max(1), Ordering::Relaxed);
}

pub(crate) fn scale() -> u32 {
    SCALE.load(Ordering::Relaxed).max(1)
}

/// A size and weight of text: `size` the font's size and `line` the height of
/// the box it is centred in, both in layout units.
#[derive(Clone, Copy)]
pub(crate) struct Style {
    pub size: f32,
    pub line: f32,
    pub bold: bool,
}

/// What every row and label is in, in a 16-unit box on an 18-unit line.
pub(crate) const BODY: Style = Style {
    size: 14.0,
    line: 16.0,
    bold: true,
};
pub(crate) const TITLE: Style = Style {
    size: 15.5,
    line: 18.0,
    bold: true,
};
pub(crate) const SMALL: Style = Style {
    size: 11.5,
    line: 13.0,
    bold: true,
};
pub(crate) const TINY: Style = Style {
    size: 9.5,
    line: 12.0,
    bold: false,
};
pub(crate) const TINY_BOLD: Style = Style {
    size: 9.5,
    line: 12.0,
    bold: true,
};

/// The face and glyph `c` is drawn with.
fn glyph(c: char, bold: bool) -> (&'static FontRef<'static>, GlyphId) {
    let face: &'static FontRef<'static> = if bold { &SEMIBOLD } else { &REGULAR };
    let id = face.glyph_id(c);
    if id.0 != 0 || c == ' ' {
        return (face, id);
    }
    let fallback: &'static FontRef<'static> = &FALLBACK;
    (fallback, fallback.glyph_id(c))
}

/// How wide `text` is in `style`, in layout units.
pub(crate) fn text_width(text: &str, style: Style) -> f32 {
    text.chars()
        .map(|c| {
            let (face, id) = glyph(c, style.bold);
            face.as_scaled(PxScale::from(style.size)).h_advance(id)
        })
        .sum()
}

/// [`text_width`] of body text.
pub(crate) fn width(text: &str) -> f32 {
    text_width(text, BODY)
}

/// `text`, cut short with an ellipsis if it is wider than `width`.
pub(crate) fn fit(text: &str, width: f32) -> String {
    fit_with(text, width, BODY)
}

pub(crate) fn fit_with(text: &str, width: f32, style: Style) -> String {
    if text_width(text, style) <= width {
        return text.to_owned();
    }
    let mut cut: String = text.to_owned();
    while !cut.is_empty() && text_width(&format!("{cut}…"), style) > width {
        cut.pop();
    }
    format!("{cut}…")
}

/// `text` broken into lines no wider than `width`, at its own line breaks and
/// then wherever a line runs out of room.
pub(crate) fn wrap(text: &str, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for c in paragraph.chars() {
            line.push(c);
            if self::width(&line) > width {
                let last = line.pop().unwrap();
                // A space that does not fit is the break itself.
                if last == ' ' {
                    lines.push(std::mem::take(&mut line));
                    continue;
                }
                // Break at the last space when there is one, else mid-word.
                match line.rfind(' ') {
                    Some(space) if space > 0 => {
                        let rest = line.split_off(space + 1);
                        lines.push(line.trim_end().to_owned());
                        line = rest;
                    }
                    _ => lines.push(std::mem::take(&mut line)),
                }
                line.push(last);
            }
        }
        lines.push(line);
    }
    lines
}

/// A picture in RGBA, a game's icon.
#[derive(Clone)]
pub(crate) struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Picture {
    /// Its colours averaged, to tint what is behind it.
    pub(crate) fn average(&self) -> Color {
        let (mut r, mut g, mut b, mut count) = (0u64, 0u64, 0u64, 0u64);
        for pixel in self.rgba.chunks_exact(4).step_by(3) {
            if pixel[3] > 0x80 {
                r += pixel[0] as u64;
                g += pixel[1] as u64;
                b += pixel[2] as u64;
                count += 1;
            }
        }
        if count == 0 {
            return BACKGROUND;
        }
        rgb((r / count) as u8, (g / count) as u8, (b / count) as u8)
    }

    /// The pixel at `x`, `y` (clamped), as floats.
    fn at(&self, x: i32, y: i32) -> [f32; 4] {
        let x = x.clamp(0, self.width as i32 - 1) as usize;
        let y = y.clamp(0, self.height as i32 - 1) as usize;
        let at = (y * self.width as usize + x) * 4;
        [
            self.rgba[at] as f32,
            self.rgba[at + 1] as f32,
            self.rgba[at + 2] as f32,
            self.rgba[at + 3] as f32,
        ]
    }

    /// Bilinear sample at `u`, `v` in its pixels.
    fn sample(&self, u: f32, v: f32) -> [f32; 4] {
        let (x0, y0) = (u.floor(), v.floor());
        let (fx, fy) = (u - x0, v - y0);
        let (x0, y0) = (x0 as i32, y0 as i32);
        let (a, b, c, d) = (self.at(x0, y0), self.at(x0 + 1, y0), self.at(x0, y0 + 1), self.at(x0 + 1, y0 + 1));
        let mut out = [0.0; 4];
        for i in 0..4 {
            let top = a[i] + (b[i] - a[i]) * fx;
            let bottom = c[i] + (d[i] - c[i]) * fx;
            out[i] = top + (bottom - top) * fy;
        }
        out
    }
}

/// How much of the pixel centred at `px`, `py` a rounded rectangle covers.
fn round_coverage(px: f32, py: f32, x0: f32, y0: f32, x1: f32, y1: f32, radius: f32) -> f32 {
    let horizontal = ((px - x0).min(x1 - px) + 0.5).clamp(0.0, 1.0);
    let vertical = ((py - y0).min(y1 - py) + 0.5).clamp(0.0, 1.0);
    let mut coverage = horizontal.min(vertical);
    if radius > 0.0 {
        let cx = if px < x0 + radius {
            Some(x0 + radius)
        } else if px > x1 - radius {
            Some(x1 - radius)
        } else {
            None
        };
        let cy = if py < y0 + radius {
            Some(y0 + radius)
        } else if py > y1 - radius {
            Some(y1 - radius)
        } else {
            None
        };
        if let (Some(cx), Some(cy)) = (cx, cy) {
            let distance = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
            coverage = coverage.min((radius - distance + 0.5).clamp(0.0, 1.0));
        }
    }
    coverage
}

pub(crate) struct Screen {
    width: u32,
    height: u32,
    scale: u32,
    pixels: Vec<u8>,
}

impl Screen {
    /// A blank screen `width` by `height` layout units, drawn at the screen's
    /// scale.
    pub(crate) fn new(width: u32, height: u32) -> Screen {
        Screen::with_scale(width, height, scale())
    }

    pub(crate) fn with_scale(width: u32, height: u32, scale: u32) -> Screen {
        let scale = scale.max(1);
        let mut screen = Screen {
            width,
            height,
            scale,
            pixels: vec![0; (width * scale * height * scale * 4) as usize],
        };
        screen.fill(0, 0, width, height, BACKGROUND);
        screen
    }

    pub(crate) fn width(&self) -> u32 {
        self.width
    }

    pub(crate) fn height(&self) -> u32 {
        self.height
    }

    /// Its size in the screen's pixels.
    pub(crate) fn pixel_size(&self) -> (u32, u32) {
        (self.width * self.scale, self.height * self.scale)
    }

    fn units(&self, value: f32) -> f32 {
        value * self.scale as f32
    }

    /// Puts `color` over the pixel at `x`, `y`, `coverage` of it.
    fn blend(&mut self, x: i32, y: i32, color: Color, coverage: f32) {
        let (width, height) = self.pixel_size();
        if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
            return;
        }
        let alpha = coverage.clamp(0.0, 1.0) * color.a as f32 / 255.0;
        if alpha <= 0.0 {
            return;
        }
        let at = ((y as u32 * width + x as u32) * 4) as usize;
        let pixel = &mut self.pixels[at..at + 4];
        for (channel, value) in [color.r, color.g, color.b].into_iter().enumerate() {
            pixel[channel] = (pixel[channel] as f32 + (value as f32 - pixel[channel] as f32) * alpha).round() as u8;
        }
        pixel[3] = 0xff;
    }

    pub(crate) fn fill(&mut self, x: i32, y: i32, width: u32, height: u32, color: Color) {
        let scale = self.scale as i32;
        let (screen_width, screen_height) = self.pixel_size();
        let (x0, y0) = ((x * scale).max(0), (y * scale).max(0));
        let x1 = ((x + width as i32) * scale).min(screen_width as i32);
        let y1 = ((y + height as i32) * scale).min(screen_height as i32);
        if color.a == 0xff {
            for py in y0..y1 {
                let row = (py as u32 * screen_width) as usize * 4;
                for px in x0..x1 {
                    let at = row + px as usize * 4;
                    self.pixels[at..at + 4].copy_from_slice(&[color.r, color.g, color.b, 0xff]);
                }
            }
        } else {
            for py in y0..y1 {
                for px in x0..x1 {
                    self.blend(px, py, color, 1.0);
                }
            }
        }
    }

    /// A rounded rectangle, smooth at its corners.
    pub(crate) fn round(&mut self, x: f32, y: f32, width: f32, height: f32, radius: f32, color: Color) {
        let (x0, y0, x1, y1) = (self.units(x), self.units(y), self.units(x + width), self.units(y + height));
        let radius = self.units(radius).min((x1 - x0) / 2.0).min((y1 - y0) / 2.0);
        for py in y0.floor() as i32..y1.ceil() as i32 {
            for px in x0.floor() as i32..x1.ceil() as i32 {
                let coverage = round_coverage(px as f32 + 0.5, py as f32 + 0.5, x0, y0, x1, y1, radius);
                if coverage > 0.0 {
                    self.blend(px, py, color, coverage);
                }
            }
        }
    }

    /// The edge of a rounded rectangle, `thickness` units wide, inside it.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn outline(&mut self, x: f32, y: f32, width: f32, height: f32, radius: f32, thickness: f32, color: Color) {
        let (x0, y0, x1, y1) = (self.units(x), self.units(y), self.units(x + width), self.units(y + height));
        let radius = self.units(radius).min((x1 - x0) / 2.0).min((y1 - y0) / 2.0);
        let inset = self.units(thickness).max(1.0);
        for py in y0.floor() as i32..y1.ceil() as i32 {
            for px in x0.floor() as i32..x1.ceil() as i32 {
                let (cx, cy) = (px as f32 + 0.5, py as f32 + 0.5);
                let outer = round_coverage(cx, cy, x0, y0, x1, y1, radius);
                let inner = round_coverage(cx, cy, x0 + inset, y0 + inset, x1 - inset, y1 - inset, (radius - inset).max(0.0));
                if outer - inner > 0.0 {
                    self.blend(px, py, color, outer - inner);
                }
            }
        }
    }

    /// The whole screen shading from `top` to `bottom`.
    pub(crate) fn gradient(&mut self, top: Color, bottom: Color) {
        let (width, height) = self.pixel_size();
        for py in 0..height {
            let t = py as f32 / (height.max(2) - 1) as f32;
            let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
            let color = [mix(top.r, bottom.r), mix(top.g, bottom.g), mix(top.b, bottom.b), 0xff];
            let row = (py * width) as usize * 4;
            for px in 0..width as usize {
                self.pixels[row + px * 4..row + px * 4 + 4].copy_from_slice(&color);
            }
        }
    }

    /// A raised panel: a soft shadow, the fill, a fine edge.
    pub(crate) fn panel(&mut self, x: i32, y: i32, width: u32, height: u32) {
        let (x, y, width, height) = (x as f32, y as f32, width as f32, height as f32);
        for spread in (1..=4).rev() {
            let spread = spread as f32;
            self.round(
                x - spread,
                y - spread + 2.0,
                width + 2.0 * spread,
                height + 2.0 * spread,
                8.0 + spread,
                rgba(0, 0, 0, 0x22),
            );
        }
        self.round(x, y, width, height, 8.0, PANEL);
        self.outline(x, y, width, height, 8.0, 0.5, EDGE);
    }

    /// Body text in a 16-unit box whose top is `y`.
    pub(crate) fn text(&mut self, text: &str, x: i32, y: i32, alignment: TextAlignment, color: Color) {
        self.text_styled(text, x as f32, y as f32, BODY, alignment, color);
    }

    /// `text` in `style`, centred in the style's box from `y` down; how wide it
    /// came out, in layout units.
    pub(crate) fn text_styled(&mut self, text: &str, x: f32, y: f32, style: Style, alignment: TextAlignment, color: Color) -> f32 {
        let width = text_width(text, style);
        let left = match alignment {
            TextAlignment::Left => x,
            TextAlignment::Center => x - width / 2.0,
            TextAlignment::Right => x - width,
        };
        let size = self.units(style.size);
        // The middle of a Hangul syllable sits about 0.36 of the size over
        // the baseline: putting that on the middle of the box centres the
        // text by its letters rather than by the face's ascent and descent.
        let baseline = self.units(y + style.line / 2.0) + 0.36 * size;
        let mut pen = self.units(left);
        for c in text.chars() {
            let (face, id) = glyph(c, style.bold);
            let scaled = face.as_scaled(PxScale::from(size));
            let glyph = id.with_scale_and_position(size, point(pen, baseline));
            if let Some(outlined) = face.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                let (left, top) = (bounds.min.x as i32, bounds.min.y as i32);
                outlined.draw(|gx, gy, coverage| self.blend(left + gx as i32, top + gy as i32, color, coverage));
            }
            pen += scaled.h_advance(id);
        }
        width
    }

    /// A bar across the screen at `y`: the screen's title at the top, the
    /// keys it answers to at the bottom.
    pub(crate) fn bar(&mut self, y: i32, left: &str, right: &str) {
        let width = self.width;
        if y == 0 {
            self.header(0, 0, width, left, right);
        } else {
            self.fill(0, y, width, BAR as u32, BAR_COLOR);
            self.fill(0, y, width, 1, EDGE);
            self.hints(0, y, width, left, right);
        }
    }

    /// A title bar: `left` in bold, `right` muted, a hairline under them.
    pub(crate) fn header(&mut self, x: i32, y: i32, width: u32, left: &str, right: &str) {
        self.fill(x, y, width, BAR as u32, BAR_COLOR);
        self.fill(x, y + BAR - 1, width, 1, EDGE);
        let right_width = if right.is_empty() { 0.0 } else { text_width(right, SMALL) + 12.0 };
        let left = fit_with(left, width as f32 - 16.0 - right_width, TITLE);
        self.text_styled(&left, x as f32 + 8.0, y as f32 + 1.0, TITLE, TextAlignment::Left, TEXT);
        if !right.is_empty() {
            self.text_styled(right, (x + width as i32) as f32 - 8.0, y as f32 + 3.5, SMALL, TextAlignment::Right, MUTED);
        }
    }

    /// The keys a screen answers to, at `y` across `width`: each key in a cap
    /// with what it does beside it, `left` from the left and `right` from the
    /// right, as a hint like "Enter 실행  Esc 메뉴" names them.
    pub(crate) fn hints(&mut self, x: i32, y: i32, width: u32, left: &str, right: &str) {
        let top = y as f32 + (BAR as f32 - 13.0) / 2.0;
        let mut at = x as f32 + 8.0;
        for (index, (key, label)) in hint_parts(left).into_iter().enumerate() {
            at = self.keycap(at, top, &key, &label, index == 0);
        }
        let parts = hint_parts(right);
        let total: f32 = parts.iter().map(|(key, label)| keycap_width(key, label)).sum();
        let mut at = (x + width as i32) as f32 - 8.0 - total + 9.0;
        for (key, label) in parts {
            at = self.keycap(at, top, &key, &label, false);
        }
    }

    /// One key in a cap and what it does; where the next one goes.
    pub(crate) fn keycap(&mut self, x: f32, y: f32, key: &str, label: &str, main: bool) -> f32 {
        let mut at = x;
        if !key.is_empty() {
            let cap = (text_width(key, TINY_BOLD) + 8.0).max(14.0);
            self.round(at, y, cap, 13.0, 3.5, if main { WHITE } else { CAP });
            self.text_styled(
                key,
                at + cap / 2.0,
                y + 0.5,
                TINY_BOLD,
                TextAlignment::Center,
                if main { INK } else { WHITE },
            );
            at += cap + 3.5;
        }
        if label.is_empty() {
            // A key that goes with the next one, as L with R: close to it.
            return at;
        }
        at += self.text_styled(label, at, y + 0.5, TINY, TextAlignment::Left, HINT_TEXT);
        at + 9.0
    }

    /// `rgba`, `width` by `height` pixels, fitted to the whole screen at its
    /// own shape by whole pixels: the game behind a menu, or the screen a box
    /// is over.
    pub(crate) fn backdrop(&mut self, rgba: &[u8], width: u32, height: u32) {
        if width == 0 || height == 0 || rgba.len() < (width * height * 4) as usize {
            return;
        }
        let (screen_width, screen_height) = self.pixel_size();
        let scale = (screen_width as f32 / width as f32).min(screen_height as f32 / height as f32);
        let (shown_width, shown_height) = ((width as f32 * scale) as u32, (height as f32 * scale) as u32);
        let (left, top) = ((screen_width - shown_width) / 2, (screen_height - shown_height) / 2);
        for y in 0..shown_height {
            let source_y = ((y as f32 / scale) as u32).min(height - 1);
            let row = ((top + y) * screen_width + left) as usize * 4;
            for x in 0..shown_width {
                let source_x = ((x as f32 / scale) as u32).min(width - 1);
                let from = ((source_y * width + source_x) * 4) as usize;
                let to = row + x as usize * 4;
                self.pixels[to..to + 3].copy_from_slice(&rgba[from..from + 3]);
                self.pixels[to + 3] = 0xff;
            }
        }
    }

    /// `picture` into the box at `x`, `y`, `width` by `height`, cut to the
    /// box's shape about its middle, its corners rounded by `radius` and its
    /// colours times `bright`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn picture(&mut self, picture: &Picture, x: f32, y: f32, width: f32, height: f32, radius: f32, bright: f32) {
        if picture.width == 0 || picture.height == 0 {
            return;
        }
        let (x0, y0, x1, y1) = (self.units(x), self.units(y), self.units(x + width), self.units(y + height));
        let radius = self.units(radius);
        // The part of the picture the box shows: all of it one way, the middle
        // of it the other.
        let (source_width, source_height) = (picture.width as f32, picture.height as f32);
        let box_shape = (x1 - x0) / (y1 - y0);
        let (crop_width, crop_height) = if source_width / source_height > box_shape {
            (source_height * box_shape, source_height)
        } else {
            (source_width, source_width / box_shape)
        };
        let (crop_x, crop_y) = ((source_width - crop_width) / 2.0, (source_height - crop_height) / 2.0);
        let step = crop_width / (x1 - x0);
        // Shrinking by much, average a few samples a pixel so it does not
        // shimmer.
        let samples = step.ceil().clamp(1.0, 4.0) as i32;
        for py in y0.floor() as i32..y1.ceil() as i32 {
            for px in x0.floor() as i32..x1.ceil() as i32 {
                let coverage = round_coverage(px as f32 + 0.5, py as f32 + 0.5, x0, y0, x1, y1, radius);
                if coverage <= 0.0 {
                    continue;
                }
                let mut sum = [0.0f32; 4];
                for sy in 0..samples {
                    for sx in 0..samples {
                        let fx = px as f32 + (sx as f32 + 0.5) / samples as f32;
                        let fy = py as f32 + (sy as f32 + 0.5) / samples as f32;
                        let u = crop_x + (fx - x0) * step - 0.5;
                        let v = crop_y + (fy - y0) * step - 0.5;
                        let pixel = picture.sample(u, v);
                        for i in 0..4 {
                            sum[i] += pixel[i];
                        }
                    }
                }
                let count = (samples * samples) as f32;
                let alpha = sum[3] / count / 255.0;
                let color = Color {
                    a: 0xff,
                    r: (sum[0] / count * bright).min(255.0) as u8,
                    g: (sum[1] / count * bright).min(255.0) as u8,
                    b: (sum[2] / count * bright).min(255.0) as u8,
                };
                self.blend(px, py, color, coverage * alpha);
            }
        }
    }

    pub(crate) fn paragraph(&mut self, text: &str, top: i32, color: Color) {
        for (index, line) in wrap(text, self.width as f32 - 24.0).iter().enumerate() {
            self.text(line, self.width as i32 / 2, top + index as i32 * LINE, TextAlignment::Center, color);
        }
    }

    /// `text` in a small box over the bottom of the screen, `above` units up
    /// from it.
    pub(crate) fn toast(&mut self, text: &str, above: i32) {
        let text = fit_with(text, self.width as f32 - 32.0, SMALL);
        let box_width = text_width(&text, SMALL) + 20.0;
        let x = (self.width as f32 - box_width) / 2.0;
        let y = self.height as f32 - above as f32 - 22.0;
        self.round(x, y + 1.0, box_width, 18.0, 9.0, rgba(0, 0, 0, 0x60));
        self.round(x, y, box_width, 18.0, 9.0, rgb(0x2b, 0x30, 0x2c));
        self.text_styled(&text, self.width as f32 / 2.0, y + 2.5, SMALL, TextAlignment::Center, TEXT);
    }

    pub(crate) fn rgba(self) -> Vec<u8> {
        self.pixels
    }
}

/// A hint like "Enter 실행  Esc 메뉴" as its keys and what each does: a word
/// of plain ASCII letters, digits, `+` or arrows is a key, and the words after
/// it, up to the next key, are its label.
fn hint_parts(hint: &str) -> Vec<(String, String)> {
    let is_key = |word: &str| {
        !word.is_empty()
            && word
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '◀' | '▶' | '▲' | '▼'))
            && !word.chars().all(|c| c.is_ascii_digit() || c == '.')
    };
    let mut parts: Vec<(String, String)> = Vec::new();
    for word in hint.split_whitespace() {
        if is_key(word) {
            parts.push((word.to_owned(), String::new()));
        } else if let Some((_, label)) = parts.last_mut() {
            if !label.is_empty() {
                label.push(' ');
            }
            label.push_str(word);
        } else {
            parts.push((String::new(), word.to_owned()));
        }
    }
    parts
}

fn keycap_width(key: &str, label: &str) -> f32 {
    let mut width = 0.0;
    if !key.is_empty() {
        width += (text_width(key, TINY_BOLD) + 8.0).max(14.0) + 3.5;
    }
    if label.is_empty() {
        return width;
    }
    width + text_width(label, TINY) + 9.0
}

/// `rgba`, `width` by `height` pixels - a game's frame - with `text` in a
/// small box over its bottom.
pub(crate) fn with_toast(rgba: &[u8], width: u32, height: u32, text: &str) -> Vec<u8> {
    if rgba.len() < (width * height * 4) as usize || width < 64 || height < 40 {
        return rgba.to_vec();
    }
    let mut screen = Screen::with_scale(width, height, 1);
    screen.pixels.copy_from_slice(&rgba[..(width * height * 4) as usize]);
    screen.toast(text, 4);
    screen.rgba()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_breaks_at_spaces_and_newlines() {
        let lines = wrap("가나다 라마바 사아자\n끝", width("가나다 라마바") + 1.0);
        assert_eq!(lines, vec!["가나다 라마바", "사아자", "끝"]);
    }

    #[test]
    fn fit_cuts_long_names() {
        assert_eq!(fit("레전드 오브 마스터", 1000.0), "레전드 오브 마스터");
        assert_eq!(fit("레전드 오브 마스터", width("레전드…")), "레전드…");
    }

    #[test]
    fn hints_split_into_keys_and_what_they_do() {
        assert_eq!(
            hint_parts("Enter 실행  Esc 메뉴"),
            vec![("Enter".to_owned(), "실행".to_owned()), ("Esc".to_owned(), "메뉴".to_owned())]
        );
        assert_eq!(
            hint_parts("◀▶ 0.1씩  F5 F6"),
            vec![
                ("◀▶".to_owned(), "0.1씩".to_owned()),
                ("F5".to_owned(), String::new()),
                ("F6".to_owned(), String::new())
            ]
        );
        assert_eq!(hint_parts("SELECT+START 종료"), vec![("SELECT+START".to_owned(), "종료".to_owned())]);
    }

    #[test]
    fn text_is_drawn_at_the_screen_scale() {
        let mut screen = Screen::with_scale(40, 20, 3);
        screen.text("가", 0, 0, TextAlignment::Left, WHITE);
        assert_eq!(screen.pixel_size(), (120, 60));
        let lit = screen.rgba().chunks_exact(4).filter(|pixel| pixel[0] > 0x80).count();
        assert!(lit > 50, "{lit} pixels lit");
    }
}
