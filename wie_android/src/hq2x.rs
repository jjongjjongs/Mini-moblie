// SPDX-License-Identifier: LGPL-2.1-only
//
// hq2x was devised by Maxim Stepin. Its pattern table here is carried over
// from hqx_python by WhoAteMyButter (https://gitlab.com/whoatemybutter/hqx_python),
// which is licensed under the GNU Lesser General Public License 2.1 - and so
// is this file, unlike the rest of the crate, which is MIT.

//! hq2x: doubles a frame, smoothing the steps along its edges.
//!
//! Every source pixel becomes a 2x2 block. Which of its eight neighbours are
//! a different colour - compared in YUV, so two shades a player would call
//! the same count as one - makes an eight-bit pattern, and the pattern picks
//! how each of the four output pixels blends the centre with its neighbours.
//! The 256 patterns are the hqx table, carried over unchanged.
//!
//! The frame is RGB565, so a colour's YUV is looked up in a table of every
//! one of its 65536 values made once. Each pixel's colour and YUV, and
//! whether it differs from each neighbour, are worked out once for the whole
//! frame rather than once for every pixel that reads them.

use std::sync::OnceLock;

const TL: usize = 0;
const T: usize = 1;
const TR: usize = 2;
const L: usize = 3;
const C: usize = 4;
const R: usize = 5;
const BL: usize = 6;
const B: usize = 7;
const BR: usize = 8;

const MASK_RB: u32 = 0x00ff_00ff;
const MASK_G: u32 = 0x0000_ff00;

/// Doubles a `width` by `height` RGB565 frame, returning its 0x00RRGGBB
/// pixels row by row, `width * 2` by `height * 2`.
pub fn hq2x(pixels: &[u16], width: usize, height: usize) -> Vec<u32> {
    let mut out = vec![0u32; width * height * 4];
    if width == 0 || height == 0 || pixels.len() < width * height {
        return out;
    }

    // The frame with a one-pixel border repeating its edge, so every pixel
    // has eight neighbours to read, and each pixel's colour and YUV worked out
    // once rather than once for every pixel it neighbours.
    let yuv_of = yuv_table();
    let stride = width + 2;
    let mut colour = vec![0u32; stride * (height + 2)];
    let mut yuv = vec![0u32; stride * (height + 2)];
    for row in 0..height + 2 {
        let source = row.saturating_sub(1).min(height - 1) * width;
        for column in 0..width + 2 {
            let pixel = pixels[source + column.saturating_sub(1).min(width - 1)];
            colour[row * stride + column] = rgb888(pixel);
            yuv[row * stride + column] = yuv_of[pixel as usize];
        }
    }

    // Whether each pixel differs from the one to its right, below it, below
    // and right, and below and left. A pixel's other four comparisons are
    // these same ones, read from the neighbour on the other side.
    let cells = stride * (height + 2);
    let mut right = vec![false; cells];
    let mut down = vec![false; cells];
    let mut down_right = vec![false; cells];
    let mut down_left = vec![false; cells];
    for at in 0..cells - stride - 1 {
        let here = yuv[at];
        right[at] = !similar(here, yuv[at + 1]);
        down[at] = !similar(here, yuv[at + stride]);
        down_right[at] = !similar(here, yuv[at + stride + 1]);
        if at > 0 {
            down_left[at] = !similar(here, yuv[at + stride - 1]);
        }
    }

    let row_out = width * 2;
    let mut o = [0u32; 4];
    for row in 0..height {
        for column in 0..width {
            let at = (row + 1) * stride + column + 1;
            let pattern = u8::from(down_right[at - stride - 1])
                | u8::from(down[at - stride]) << 1
                | u8::from(down_left[at - stride + 1]) << 2
                | u8::from(right[at - 1]) << 3
                | u8::from(right[at]) << 4
                | u8::from(down_left[at]) << 5
                | u8::from(down[at]) << 6
                | u8::from(down_right[at]) << 7;
            let out_at = row * 2 * row_out + column * 2;

            if pattern == 0 {
                // No different neighbour - most of any frame. The table blends
                // the centre with its edge neighbours here, which differ from
                // it by no more than the eye can tell.
                let c = [colour[at - 1], colour[at - stride], colour[at], colour[at + 1], colour[at + stride]];
                o = [
                    mix_2_1_1(c[2], c[0], c[1]),
                    mix_2_1_1(c[2], c[1], c[3]),
                    mix_2_1_1(c[2], c[4], c[0]),
                    mix_2_1_1(c[2], c[3], c[4]),
                ];
            } else {
                let c = [
                    colour[at - stride - 1],
                    colour[at - stride],
                    colour[at - stride + 1],
                    colour[at - 1],
                    colour[at],
                    colour[at + 1],
                    colour[at + stride - 1],
                    colour[at + stride],
                    colour[at + stride + 1],
                ];
                let y = [
                    yuv[at - stride - 1],
                    yuv[at - stride],
                    yuv[at - stride + 1],
                    yuv[at - 1],
                    yuv[at],
                    yuv[at + 1],
                    yuv[at + stride - 1],
                    yuv[at + stride],
                    yuv[at + stride + 1],
                ];
                blend(pattern, &c, &y, &mut o);
            }

            out[out_at] = o[0];
            out[out_at + 1] = o[1];
            out[out_at + row_out] = o[2];
            out[out_at + row_out + 1] = o[3];
        }
    }

    out
}

/// An RGB565 colour as 0x00RRGGBB, each channel stretched the way the rest of
/// the player shows the frame.
#[inline]
fn rgb888(pixel: u16) -> u32 {
    let pixel = u32::from(pixel);
    let (r, g, b) = ((pixel >> 11) & 0x1f, (pixel >> 5) & 0x3f, pixel & 0x1f);

    (r * 255 / 31) << 16 | (g * 255 / 63) << 8 | (b * 255 / 31)
}

/// Every RGB565 colour's YUV, packed Y, U, V from the top byte down.
fn yuv_table() -> &'static [u32] {
    static TABLE: OnceLock<Vec<u32>> = OnceLock::new();

    TABLE.get_or_init(|| {
        (0..=u16::MAX)
            .map(|pixel| {
                let colour = rgb888(pixel);
                let (r, g, b) = ((colour >> 16) as i32, ((colour >> 8) & 0xff) as i32, (colour & 0xff) as i32);
                let y = (r + g + b) >> 2;
                let u = 128 + ((r - b) >> 2);
                let v = 128 + ((g * 2 - r - b) >> 2);

                (y as u32) << 16 | (u as u32) << 8 | v as u32
            })
            .collect()
    })
}

/// Whether two colours are close enough, in YUV, to be the same one.
#[inline]
fn similar(a: u32, b: u32) -> bool {
    let channel = |shift: u32| (((a >> shift) & 0xff) as i32 - ((b >> shift) & 0xff) as i32).abs();

    (channel(16) <= 48) & (channel(8) <= 7) & (channel(0) <= 6)
}

/// `(a * wa + b * wb + c * wc) >> shift`, channel by channel. Red and blue
/// ride together in one word and green in another, with room between them
/// for the sum to carry into.
#[inline]
fn weigh(a: u32, wa: u32, b: u32, wb: u32, c: u32, wc: u32, shift: u32) -> u32 {
    let rb = ((a & MASK_RB) * wa + (b & MASK_RB) * wb + (c & MASK_RB) * wc) >> shift;
    let g = ((a & MASK_G) * wa + (b & MASK_G) * wb + (c & MASK_G) * wc) >> shift;

    (rb & MASK_RB) | (g & MASK_G)
}

#[inline]
fn mix_3_1(a: u32, b: u32) -> u32 {
    if a == b { a } else { weigh(a, 3, b, 1, 0, 0, 2) }
}

#[inline]
fn mix_2_1_1(a: u32, b: u32, c: u32) -> u32 {
    weigh(a, 2, b, 1, c, 1, 2)
}

#[inline]
fn mix_5_2_1(a: u32, b: u32, c: u32) -> u32 {
    weigh(a, 5, b, 2, c, 1, 3)
}

#[inline]
fn mix_2_3_3(a: u32, b: u32, c: u32) -> u32 {
    weigh(a, 2, b, 3, c, 3, 3)
}

#[inline]
fn mix_6_1_1(a: u32, b: u32, c: u32) -> u32 {
    weigh(a, 6, b, 1, c, 1, 3)
}

#[inline]
fn mix_14_1_1(a: u32, b: u32, c: u32) -> u32 {
    weigh(a, 14, b, 1, c, 1, 4)
}

/// The four output pixels for one source pixel: the hqx table.
#[rustfmt::skip]
fn blend(pattern: u8, c: &[u32; 9], y: &[u32; 9], o: &mut [u32; 4]) {
    match pattern {
        0 | 1 | 4 | 32 | 128 | 5 | 132 | 160 | 33 | 129 | 36 | 133 | 164 | 161 | 37 | 165 => {
            o[0] = mix_2_1_1(c[C], c[L], c[T]);
            o[1] = mix_2_1_1(c[C], c[T], c[R]);
            o[2] = mix_2_1_1(c[C], c[B], c[L]);
            o[3] = mix_2_1_1(c[C], c[R], c[B]);
        }
        2 | 34 | 130 | 162 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[L]);
            o[1] = mix_2_1_1(c[C], c[TR], c[R]);
            o[2] = mix_2_1_1(c[C], c[B], c[L]);
            o[3] = mix_2_1_1(c[C], c[R], c[B]);
        }
        16 | 17 | 48 | 49 => {
            o[0] = mix_2_1_1(c[C], c[L], c[T]);
            o[1] = mix_2_1_1(c[C], c[TR], c[T]);
            o[2] = mix_2_1_1(c[C], c[B], c[L]);
            o[3] = mix_2_1_1(c[C], c[BR], c[B]);
        }
        64 | 65 | 68 | 69 => {
            o[0] = mix_2_1_1(c[C], c[L], c[T]);
            o[1] = mix_2_1_1(c[C], c[T], c[R]);
            o[2] = mix_2_1_1(c[C], c[BL], c[L]);
            o[3] = mix_2_1_1(c[C], c[BR], c[R]);
        }
        8 | 12 | 136 | 140 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[T]);
            o[1] = mix_2_1_1(c[C], c[T], c[R]);
            o[2] = mix_2_1_1(c[C], c[BL], c[B]);
            o[3] = mix_2_1_1(c[C], c[R], c[B]);
        }
        3 | 35 | 131 | 163 => {
            o[0] = mix_3_1(c[C], c[L]);
            o[1] = mix_2_1_1(c[C], c[TR], c[R]);
            o[2] = mix_2_1_1(c[C], c[B], c[L]);
            o[3] = mix_2_1_1(c[C], c[R], c[B]);
        }
        6 | 38 | 134 | 166 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[L]);
            o[1] = mix_3_1(c[C], c[R]);
            o[2] = mix_2_1_1(c[C], c[B], c[L]);
            o[3] = mix_2_1_1(c[C], c[R], c[B]);
        }
        20 | 21 | 52 | 53 => {
            o[0] = mix_2_1_1(c[C], c[L], c[T]);
            o[1] = mix_3_1(c[C], c[T]);
            o[2] = mix_2_1_1(c[C], c[B], c[L]);
            o[3] = mix_2_1_1(c[C], c[BR], c[B]);
        }
        144 | 145 | 176 | 177 => {
            o[0] = mix_2_1_1(c[C], c[L], c[T]);
            o[1] = mix_2_1_1(c[C], c[TR], c[T]);
            o[2] = mix_2_1_1(c[C], c[B], c[L]);
            o[3] = mix_3_1(c[C], c[B]);
        }
        192 | 193 | 196 | 197 => {
            o[0] = mix_2_1_1(c[C], c[L], c[T]);
            o[1] = mix_2_1_1(c[C], c[T], c[R]);
            o[2] = mix_2_1_1(c[C], c[BL], c[L]);
            o[3] = mix_3_1(c[C], c[R]);
        }
        96 | 97 | 100 | 101 => {
            o[0] = mix_2_1_1(c[C], c[L], c[T]);
            o[1] = mix_2_1_1(c[C], c[T], c[R]);
            o[2] = mix_3_1(c[C], c[L]);
            o[3] = mix_2_1_1(c[C], c[BR], c[R]);
        }
        40 | 44 | 168 | 172 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[T]);
            o[1] = mix_2_1_1(c[C], c[T], c[R]);
            o[2] = mix_3_1(c[C], c[B]);
            o[3] = mix_2_1_1(c[C], c[R], c[B]);
        }
        9 | 13 | 137 | 141 => {
            o[0] = mix_3_1(c[C], c[T]);
            o[1] = mix_2_1_1(c[C], c[T], c[R]);
            o[2] = mix_2_1_1(c[C], c[BL], c[B]);
            o[3] = mix_2_1_1(c[C], c[R], c[B]);
        }
        18 | 50 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[L]);
            if !similar(y[T], y[R]) {
                o[1] = mix_3_1(c[C], c[TR]);
            } else {
                o[1] = mix_2_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_2_1_1(c[C], c[B], c[L]);
            o[3] = mix_2_1_1(c[C], c[BR], c[B]);
        }
        80 | 81 => {
            o[0] = mix_2_1_1(c[C], c[L], c[T]);
            o[1] = mix_2_1_1(c[C], c[TR], c[T]);
            o[2] = mix_2_1_1(c[C], c[BL], c[L]);
            if !similar(y[R], y[B]) {
                o[3] = mix_3_1(c[C], c[BR]);
            } else {
                o[3] = mix_2_1_1(c[C], c[R], c[B]);
            }
        }
        72 | 76 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[T]);
            o[1] = mix_2_1_1(c[C], c[T], c[R]);
            if !similar(y[B], y[L]) {
                o[2] = mix_3_1(c[C], c[BL]);
            } else {
                o[2] = mix_2_1_1(c[C], c[B], c[L]);
            }
            o[3] = mix_2_1_1(c[C], c[BR], c[R]);
        }
        10 | 138 => {
            if !similar(y[L], y[T]) {
                o[0] = mix_3_1(c[C], c[TL]);
            } else {
                o[0] = mix_2_1_1(c[C], c[L], c[T]);
            }
            o[1] = mix_2_1_1(c[C], c[TR], c[R]);
            o[2] = mix_2_1_1(c[C], c[BL], c[B]);
            o[3] = mix_2_1_1(c[C], c[R], c[B]);
        }
        66 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[L]);
            o[1] = mix_2_1_1(c[C], c[TR], c[R]);
            o[2] = mix_2_1_1(c[C], c[BL], c[L]);
            o[3] = mix_2_1_1(c[C], c[BR], c[R]);
        }
        24 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[T]);
            o[1] = mix_2_1_1(c[C], c[TR], c[T]);
            o[2] = mix_2_1_1(c[C], c[BL], c[B]);
            o[3] = mix_2_1_1(c[C], c[BR], c[B]);
        }
        7 | 39 | 135 => {
            o[0] = mix_3_1(c[C], c[L]);
            o[1] = mix_3_1(c[C], c[R]);
            o[2] = mix_2_1_1(c[C], c[B], c[L]);
            o[3] = mix_2_1_1(c[C], c[R], c[B]);
        }
        148 | 149 | 180 => {
            o[0] = mix_2_1_1(c[C], c[L], c[T]);
            o[1] = mix_3_1(c[C], c[T]);
            o[2] = mix_2_1_1(c[C], c[B], c[L]);
            o[3] = mix_3_1(c[C], c[B]);
        }
        224 | 228 | 225 => {
            o[0] = mix_2_1_1(c[C], c[L], c[T]);
            o[1] = mix_2_1_1(c[C], c[T], c[R]);
            o[2] = mix_3_1(c[C], c[L]);
            o[3] = mix_3_1(c[C], c[R]);
        }
        41 | 169 | 45 => {
            o[0] = mix_3_1(c[C], c[T]);
            o[1] = mix_2_1_1(c[C], c[T], c[R]);
            o[2] = mix_3_1(c[C], c[B]);
            o[3] = mix_2_1_1(c[C], c[R], c[B]);
        }
        22 | 54 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[L]);
            if !similar(y[T], y[R]) {
                o[1] = c[C];
            } else {
                o[1] = mix_2_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_2_1_1(c[C], c[B], c[L]);
            o[3] = mix_2_1_1(c[C], c[BR], c[B]);
        }
        208 | 209 => {
            o[0] = mix_2_1_1(c[C], c[L], c[T]);
            o[1] = mix_2_1_1(c[C], c[TR], c[T]);
            o[2] = mix_2_1_1(c[C], c[BL], c[L]);
            if !similar(y[R], y[B]) {
                o[3] = c[C];
            } else {
                o[3] = mix_2_1_1(c[C], c[R], c[B]);
            }
        }
        104 | 108 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[T]);
            o[1] = mix_2_1_1(c[C], c[T], c[R]);
            if !similar(y[B], y[L]) {
                o[2] = c[C];
            } else {
                o[2] = mix_2_1_1(c[C], c[B], c[L]);
            }
            o[3] = mix_2_1_1(c[C], c[BR], c[R]);
        }
        11 | 139 => {
            if !similar(y[L], y[T]) {
                o[0] = c[C];
            } else {
                o[0] = mix_2_1_1(c[C], c[L], c[T]);
            }
            o[1] = mix_2_1_1(c[C], c[TR], c[R]);
            o[2] = mix_2_1_1(c[C], c[BL], c[B]);
            o[3] = mix_2_1_1(c[C], c[R], c[B]);
        }
        19 | 51 => {
            if !similar(y[T], y[R]) {
                o[0] = mix_3_1(c[C], c[L]);
                o[1] = mix_3_1(c[C], c[TR]);
            } else {
                o[0] = mix_5_2_1(c[C], c[T], c[L]);
                o[1] = mix_2_3_3(c[C], c[T], c[R]);
            }
            o[2] = mix_2_1_1(c[C], c[B], c[L]);
            o[3] = mix_2_1_1(c[C], c[BR], c[B]);
        }
        146 | 178 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[L]);
            if !similar(y[T], y[R]) {
                o[1] = mix_3_1(c[C], c[TR]);
                o[3] = mix_3_1(c[C], c[B]);
            } else {
                o[1] = mix_2_3_3(c[C], c[T], c[R]);
                o[3] = mix_5_2_1(c[C], c[R], c[B]);
            }
            o[2] = mix_2_1_1(c[C], c[B], c[L]);
        }
        84 | 85 => {
            o[0] = mix_2_1_1(c[C], c[L], c[T]);
            if !similar(y[R], y[B]) {
                o[1] = mix_3_1(c[C], c[T]);
                o[3] = mix_3_1(c[C], c[BR]);
            } else {
                o[1] = mix_5_2_1(c[C], c[R], c[T]);
                o[3] = mix_2_3_3(c[C], c[R], c[B]);
            }
            o[2] = mix_2_1_1(c[C], c[BL], c[L]);
        }
        112 | 113 => {
            o[0] = mix_2_1_1(c[C], c[L], c[T]);
            o[1] = mix_2_1_1(c[C], c[TR], c[T]);
            if !similar(y[R], y[B]) {
                o[2] = mix_3_1(c[C], c[L]);
                o[3] = mix_3_1(c[C], c[BR]);
            } else {
                o[2] = mix_5_2_1(c[C], c[B], c[L]);
                o[3] = mix_2_3_3(c[C], c[R], c[B]);
            }
        }
        200 | 204 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[T]);
            o[1] = mix_2_1_1(c[C], c[T], c[R]);
            if !similar(y[B], y[L]) {
                o[2] = mix_3_1(c[C], c[BL]);
                o[3] = mix_3_1(c[C], c[R]);
            } else {
                o[2] = mix_2_3_3(c[C], c[B], c[L]);
                o[3] = mix_5_2_1(c[C], c[B], c[R]);
            }
        }
        73 | 77 => {
            if !similar(y[B], y[L]) {
                o[0] = mix_3_1(c[C], c[T]);
                o[2] = mix_3_1(c[C], c[BL]);
            } else {
                o[0] = mix_5_2_1(c[C], c[L], c[T]);
                o[2] = mix_2_3_3(c[C], c[B], c[L]);
            }
            o[1] = mix_2_1_1(c[C], c[T], c[R]);
            o[3] = mix_2_1_1(c[C], c[BR], c[R]);
        }
        42 | 170 => {
            if !similar(y[L], y[T]) {
                o[0] = mix_3_1(c[C], c[TL]);
                o[2] = mix_3_1(c[C], c[B]);
            } else {
                o[0] = mix_2_3_3(c[C], c[L], c[T]);
                o[2] = mix_5_2_1(c[C], c[L], c[B]);
            }
            o[1] = mix_2_1_1(c[C], c[TR], c[R]);
            o[3] = mix_2_1_1(c[C], c[R], c[B]);
        }
        14 | 142 => {
            if !similar(y[L], y[T]) {
                o[0] = mix_3_1(c[C], c[TL]);
                o[1] = mix_3_1(c[C], c[R]);
            } else {
                o[0] = mix_2_3_3(c[C], c[L], c[T]);
                o[1] = mix_5_2_1(c[C], c[T], c[R]);
            }
            o[2] = mix_2_1_1(c[C], c[BL], c[B]);
            o[3] = mix_2_1_1(c[C], c[R], c[B]);
        }
        67 => {
            o[0] = mix_3_1(c[C], c[L]);
            o[1] = mix_2_1_1(c[C], c[TR], c[R]);
            o[2] = mix_2_1_1(c[C], c[BL], c[L]);
            o[3] = mix_2_1_1(c[C], c[BR], c[R]);
        }
        70 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[L]);
            o[1] = mix_3_1(c[C], c[R]);
            o[2] = mix_2_1_1(c[C], c[BL], c[L]);
            o[3] = mix_2_1_1(c[C], c[BR], c[R]);
        }
        28 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[T]);
            o[1] = mix_3_1(c[C], c[T]);
            o[2] = mix_2_1_1(c[C], c[BL], c[B]);
            o[3] = mix_2_1_1(c[C], c[BR], c[B]);
        }
        152 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[T]);
            o[1] = mix_2_1_1(c[C], c[TR], c[T]);
            o[2] = mix_2_1_1(c[C], c[BL], c[B]);
            o[3] = mix_3_1(c[C], c[B]);
        }
        194 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[L]);
            o[1] = mix_2_1_1(c[C], c[TR], c[R]);
            o[2] = mix_2_1_1(c[C], c[BL], c[L]);
            o[3] = mix_3_1(c[C], c[R]);
        }
        98 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[L]);
            o[1] = mix_2_1_1(c[C], c[TR], c[R]);
            o[2] = mix_3_1(c[C], c[L]);
            o[3] = mix_2_1_1(c[C], c[BR], c[R]);
        }
        56 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[T]);
            o[1] = mix_2_1_1(c[C], c[TR], c[T]);
            o[2] = mix_3_1(c[C], c[B]);
            o[3] = mix_2_1_1(c[C], c[BR], c[B]);
        }
        25 => {
            o[0] = mix_3_1(c[C], c[T]);
            o[1] = mix_2_1_1(c[C], c[TR], c[T]);
            o[2] = mix_2_1_1(c[C], c[BL], c[B]);
            o[3] = mix_2_1_1(c[C], c[BR], c[B]);
        }
        26 | 31 => {
            if !similar(y[L], y[T]) {
                o[0] = c[C];
            } else {
                o[0] = mix_2_1_1(c[C], c[L], c[T]);
            }
            if !similar(y[T], y[R]) {
                o[1] = c[C];
            } else {
                o[1] = mix_2_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_2_1_1(c[C], c[BL], c[B]);
            o[3] = mix_2_1_1(c[C], c[BR], c[B]);
        }
        82 | 214 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[L]);
            if !similar(y[T], y[R]) {
                o[1] = c[C];
            } else {
                o[1] = mix_2_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_2_1_1(c[C], c[BL], c[L]);
            if !similar(y[R], y[B]) {
                o[3] = c[C];
            } else {
                o[3] = mix_2_1_1(c[C], c[R], c[B]);
            }
        }
        88 | 248 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[T]);
            o[1] = mix_2_1_1(c[C], c[TR], c[T]);
            if !similar(y[B], y[L]) {
                o[2] = c[C];
            } else {
                o[2] = mix_2_1_1(c[C], c[B], c[L]);
            }
            if !similar(y[R], y[B]) {
                o[3] = c[C];
            } else {
                o[3] = mix_2_1_1(c[C], c[R], c[B]);
            }
        }
        74 | 107 => {
            if !similar(y[L], y[T]) {
                o[0] = c[C];
            } else {
                o[0] = mix_2_1_1(c[C], c[L], c[T]);
            }
            o[1] = mix_2_1_1(c[C], c[TR], c[R]);
            if !similar(y[B], y[L]) {
                o[2] = c[C];
            } else {
                o[2] = mix_2_1_1(c[C], c[B], c[L]);
            }
            o[3] = mix_2_1_1(c[C], c[BR], c[R]);
        }
        27 => {
            if !similar(y[L], y[T]) {
                o[0] = c[C];
            } else {
                o[0] = mix_2_1_1(c[C], c[L], c[T]);
            }
            o[1] = mix_3_1(c[C], c[TR]);
            o[2] = mix_2_1_1(c[C], c[BL], c[B]);
            o[3] = mix_2_1_1(c[C], c[BR], c[B]);
        }
        86 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[L]);
            if !similar(y[T], y[R]) {
                o[1] = c[C];
            } else {
                o[1] = mix_2_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_2_1_1(c[C], c[BL], c[L]);
            o[3] = mix_3_1(c[C], c[BR]);
        }
        216 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[T]);
            o[1] = mix_2_1_1(c[C], c[TR], c[T]);
            o[2] = mix_3_1(c[C], c[BL]);
            if !similar(y[R], y[B]) {
                o[3] = c[C];
            } else {
                o[3] = mix_2_1_1(c[C], c[R], c[B]);
            }
        }
        106 => {
            o[0] = mix_3_1(c[C], c[TL]);
            o[1] = mix_2_1_1(c[C], c[TR], c[R]);
            if !similar(y[B], y[L]) {
                o[2] = c[C];
            } else {
                o[2] = mix_2_1_1(c[C], c[B], c[L]);
            }
            o[3] = mix_2_1_1(c[C], c[BR], c[R]);
        }
        30 => {
            o[0] = mix_3_1(c[C], c[TL]);
            if !similar(y[T], y[R]) {
                o[1] = c[C];
            } else {
                o[1] = mix_2_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_2_1_1(c[C], c[BL], c[B]);
            o[3] = mix_2_1_1(c[C], c[BR], c[B]);
        }
        210 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[L]);
            o[1] = mix_3_1(c[C], c[TR]);
            o[2] = mix_2_1_1(c[C], c[BL], c[L]);
            if !similar(y[R], y[B]) {
                o[3] = c[C];
            } else {
                o[3] = mix_2_1_1(c[C], c[R], c[B]);
            }
        }
        120 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[T]);
            o[1] = mix_2_1_1(c[C], c[TR], c[T]);
            if !similar(y[B], y[L]) {
                o[2] = c[C];
            } else {
                o[2] = mix_2_1_1(c[C], c[B], c[L]);
            }
            o[3] = mix_3_1(c[C], c[BR]);
        }
        75 => {
            if !similar(y[L], y[T]) {
                o[0] = c[C];
            } else {
                o[0] = mix_2_1_1(c[C], c[L], c[T]);
            }
            o[1] = mix_2_1_1(c[C], c[TR], c[R]);
            o[2] = mix_3_1(c[C], c[BL]);
            o[3] = mix_2_1_1(c[C], c[BR], c[R]);
        }
        29 => {
            o[0] = mix_3_1(c[C], c[T]);
            o[1] = mix_3_1(c[C], c[T]);
            o[2] = mix_2_1_1(c[C], c[BL], c[B]);
            o[3] = mix_2_1_1(c[C], c[BR], c[B]);
        }
        198 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[L]);
            o[1] = mix_3_1(c[C], c[R]);
            o[2] = mix_2_1_1(c[C], c[BL], c[L]);
            o[3] = mix_3_1(c[C], c[R]);
        }
        184 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[T]);
            o[1] = mix_2_1_1(c[C], c[TR], c[T]);
            o[2] = mix_3_1(c[C], c[B]);
            o[3] = mix_3_1(c[C], c[B]);
        }
        99 => {
            o[0] = mix_3_1(c[C], c[L]);
            o[1] = mix_2_1_1(c[C], c[TR], c[R]);
            o[2] = mix_3_1(c[C], c[L]);
            o[3] = mix_2_1_1(c[C], c[BR], c[R]);
        }
        57 => {
            o[0] = mix_3_1(c[C], c[T]);
            o[1] = mix_2_1_1(c[C], c[TR], c[T]);
            o[2] = mix_3_1(c[C], c[B]);
            o[3] = mix_2_1_1(c[C], c[BR], c[B]);
        }
        71 => {
            o[0] = mix_3_1(c[C], c[L]);
            o[1] = mix_3_1(c[C], c[R]);
            o[2] = mix_2_1_1(c[C], c[BL], c[L]);
            o[3] = mix_2_1_1(c[C], c[BR], c[R]);
        }
        156 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[T]);
            o[1] = mix_3_1(c[C], c[T]);
            o[2] = mix_2_1_1(c[C], c[BL], c[B]);
            o[3] = mix_3_1(c[C], c[B]);
        }
        226 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[L]);
            o[1] = mix_2_1_1(c[C], c[TR], c[R]);
            o[2] = mix_3_1(c[C], c[L]);
            o[3] = mix_3_1(c[C], c[R]);
        }
        60 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[T]);
            o[1] = mix_3_1(c[C], c[T]);
            o[2] = mix_3_1(c[C], c[B]);
            o[3] = mix_2_1_1(c[C], c[BR], c[B]);
        }
        195 => {
            o[0] = mix_3_1(c[C], c[L]);
            o[1] = mix_2_1_1(c[C], c[TR], c[R]);
            o[2] = mix_2_1_1(c[C], c[BL], c[L]);
            o[3] = mix_3_1(c[C], c[R]);
        }
        102 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[L]);
            o[1] = mix_3_1(c[C], c[R]);
            o[2] = mix_3_1(c[C], c[L]);
            o[3] = mix_2_1_1(c[C], c[BR], c[R]);
        }
        153 => {
            o[0] = mix_3_1(c[C], c[T]);
            o[1] = mix_2_1_1(c[C], c[TR], c[T]);
            o[2] = mix_2_1_1(c[C], c[BL], c[B]);
            o[3] = mix_3_1(c[C], c[B]);
        }
        58 => {
            if !similar(y[L], y[T]) {
                o[0] = mix_3_1(c[C], c[TL]);
            } else {
                o[0] = mix_6_1_1(c[C], c[L], c[T]);
            }
            if !similar(y[T], y[R]) {
                o[1] = mix_3_1(c[C], c[TR]);
            } else {
                o[1] = mix_6_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_3_1(c[C], c[B]);
            o[3] = mix_2_1_1(c[C], c[BR], c[B]);
        }
        83 => {
            o[0] = mix_3_1(c[C], c[L]);
            if !similar(y[T], y[R]) {
                o[1] = mix_3_1(c[C], c[TR]);
            } else {
                o[1] = mix_6_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_2_1_1(c[C], c[BL], c[L]);
            if !similar(y[R], y[B]) {
                o[3] = mix_3_1(c[C], c[BR]);
            } else {
                o[3] = mix_6_1_1(c[C], c[R], c[B]);
            }
        }
        92 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[T]);
            o[1] = mix_3_1(c[C], c[T]);
            if !similar(y[B], y[L]) {
                o[2] = mix_3_1(c[C], c[BL]);
            } else {
                o[2] = mix_6_1_1(c[C], c[B], c[L]);
            }
            if !similar(y[R], y[B]) {
                o[3] = mix_3_1(c[C], c[BR]);
            } else {
                o[3] = mix_6_1_1(c[C], c[R], c[B]);
            }
        }
        202 => {
            if !similar(y[L], y[T]) {
                o[0] = mix_3_1(c[C], c[TL]);
            } else {
                o[0] = mix_6_1_1(c[C], c[L], c[T]);
            }
            o[1] = mix_2_1_1(c[C], c[TR], c[R]);
            if !similar(y[B], y[L]) {
                o[2] = mix_3_1(c[C], c[BL]);
            } else {
                o[2] = mix_6_1_1(c[C], c[B], c[L]);
            }
            o[3] = mix_3_1(c[C], c[R]);
        }
        78 => {
            if !similar(y[L], y[T]) {
                o[0] = mix_3_1(c[C], c[TL]);
            } else {
                o[0] = mix_6_1_1(c[C], c[L], c[T]);
            }
            o[1] = mix_3_1(c[C], c[R]);
            if !similar(y[B], y[L]) {
                o[2] = mix_3_1(c[C], c[BL]);
            } else {
                o[2] = mix_6_1_1(c[C], c[B], c[L]);
            }
            o[3] = mix_2_1_1(c[C], c[BR], c[R]);
        }
        154 => {
            if !similar(y[L], y[T]) {
                o[0] = mix_3_1(c[C], c[TL]);
            } else {
                o[0] = mix_6_1_1(c[C], c[L], c[T]);
            }
            if !similar(y[T], y[R]) {
                o[1] = mix_3_1(c[C], c[TR]);
            } else {
                o[1] = mix_6_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_2_1_1(c[C], c[BL], c[B]);
            o[3] = mix_3_1(c[C], c[B]);
        }
        114 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[L]);
            if !similar(y[T], y[R]) {
                o[1] = mix_3_1(c[C], c[TR]);
            } else {
                o[1] = mix_6_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_3_1(c[C], c[L]);
            if !similar(y[R], y[B]) {
                o[3] = mix_3_1(c[C], c[BR]);
            } else {
                o[3] = mix_6_1_1(c[C], c[R], c[B]);
            }
        }
        89 => {
            o[0] = mix_3_1(c[C], c[T]);
            o[1] = mix_2_1_1(c[C], c[TR], c[T]);
            if !similar(y[B], y[L]) {
                o[2] = mix_3_1(c[C], c[BL]);
            } else {
                o[2] = mix_6_1_1(c[C], c[B], c[L]);
            }
            if !similar(y[R], y[B]) {
                o[3] = mix_3_1(c[C], c[BR]);
            } else {
                o[3] = mix_6_1_1(c[C], c[R], c[B]);
            }
        }
        90 => {
            if !similar(y[L], y[T]) {
                o[0] = mix_3_1(c[C], c[TL]);
            } else {
                o[0] = mix_6_1_1(c[C], c[L], c[T]);
            }
            if !similar(y[T], y[R]) {
                o[1] = mix_3_1(c[C], c[TR]);
            } else {
                o[1] = mix_6_1_1(c[C], c[T], c[R]);
            }
            if !similar(y[B], y[L]) {
                o[2] = mix_3_1(c[C], c[BL]);
            } else {
                o[2] = mix_6_1_1(c[C], c[B], c[L]);
            }
            if !similar(y[R], y[B]) {
                o[3] = mix_3_1(c[C], c[BR]);
            } else {
                o[3] = mix_6_1_1(c[C], c[R], c[B]);
            }
        }
        55 | 23 => {
            if !similar(y[T], y[R]) {
                o[0] = mix_3_1(c[C], c[L]);
                o[1] = c[C];
            } else {
                o[0] = mix_5_2_1(c[C], c[T], c[L]);
                o[1] = mix_2_3_3(c[C], c[T], c[R]);
            }
            o[2] = mix_2_1_1(c[C], c[B], c[L]);
            o[3] = mix_2_1_1(c[C], c[BR], c[B]);
        }
        182 | 150 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[L]);
            if !similar(y[T], y[R]) {
                o[1] = c[C];
                o[3] = mix_3_1(c[C], c[B]);
            } else {
                o[1] = mix_2_3_3(c[C], c[T], c[R]);
                o[3] = mix_5_2_1(c[C], c[R], c[B]);
            }
            o[2] = mix_2_1_1(c[C], c[B], c[L]);
        }
        213 | 212 => {
            o[0] = mix_2_1_1(c[C], c[L], c[T]);
            if !similar(y[R], y[B]) {
                o[1] = mix_3_1(c[C], c[T]);
                o[3] = c[C];
            } else {
                o[1] = mix_5_2_1(c[C], c[R], c[T]);
                o[3] = mix_2_3_3(c[C], c[R], c[B]);
            }
            o[2] = mix_2_1_1(c[C], c[BL], c[L]);
        }
        241 | 240 => {
            o[0] = mix_2_1_1(c[C], c[L], c[T]);
            o[1] = mix_2_1_1(c[C], c[TR], c[T]);
            if !similar(y[R], y[B]) {
                o[2] = mix_3_1(c[C], c[L]);
                o[3] = c[C];
            } else {
                o[2] = mix_5_2_1(c[C], c[B], c[L]);
                o[3] = mix_2_3_3(c[C], c[R], c[B]);
            }
        }
        236 | 232 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[T]);
            o[1] = mix_2_1_1(c[C], c[T], c[R]);
            if !similar(y[B], y[L]) {
                o[2] = c[C];
                o[3] = mix_3_1(c[C], c[R]);
            } else {
                o[2] = mix_2_3_3(c[C], c[B], c[L]);
                o[3] = mix_5_2_1(c[C], c[B], c[R]);
            }
        }
        109 | 105 => {
            if !similar(y[B], y[L]) {
                o[0] = mix_3_1(c[C], c[T]);
                o[2] = c[C];
            } else {
                o[0] = mix_5_2_1(c[C], c[L], c[T]);
                o[2] = mix_2_3_3(c[C], c[B], c[L]);
            }
            o[1] = mix_2_1_1(c[C], c[T], c[R]);
            o[3] = mix_2_1_1(c[C], c[BR], c[R]);
        }
        171 | 43 => {
            if !similar(y[L], y[T]) {
                o[0] = c[C];
                o[2] = mix_3_1(c[C], c[B]);
            } else {
                o[0] = mix_2_3_3(c[C], c[L], c[T]);
                o[2] = mix_5_2_1(c[C], c[L], c[B]);
            }
            o[1] = mix_2_1_1(c[C], c[TR], c[R]);
            o[3] = mix_2_1_1(c[C], c[R], c[B]);
        }
        143 | 15 => {
            if !similar(y[L], y[T]) {
                o[0] = c[C];
                o[1] = mix_3_1(c[C], c[R]);
            } else {
                o[0] = mix_2_3_3(c[C], c[L], c[T]);
                o[1] = mix_5_2_1(c[C], c[T], c[R]);
            }
            o[2] = mix_2_1_1(c[C], c[BL], c[B]);
            o[3] = mix_2_1_1(c[C], c[R], c[B]);
        }
        124 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[T]);
            o[1] = mix_3_1(c[C], c[T]);
            if !similar(y[B], y[L]) {
                o[2] = c[C];
            } else {
                o[2] = mix_2_1_1(c[C], c[B], c[L]);
            }
            o[3] = mix_3_1(c[C], c[BR]);
        }
        203 => {
            if !similar(y[L], y[T]) {
                o[0] = c[C];
            } else {
                o[0] = mix_2_1_1(c[C], c[L], c[T]);
            }
            o[1] = mix_2_1_1(c[C], c[TR], c[R]);
            o[2] = mix_3_1(c[C], c[BL]);
            o[3] = mix_3_1(c[C], c[R]);
        }
        62 => {
            o[0] = mix_3_1(c[C], c[TL]);
            if !similar(y[T], y[R]) {
                o[1] = c[C];
            } else {
                o[1] = mix_2_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_3_1(c[C], c[B]);
            o[3] = mix_2_1_1(c[C], c[BR], c[B]);
        }
        211 => {
            o[0] = mix_3_1(c[C], c[L]);
            o[1] = mix_3_1(c[C], c[TR]);
            o[2] = mix_2_1_1(c[C], c[BL], c[L]);
            if !similar(y[R], y[B]) {
                o[3] = c[C];
            } else {
                o[3] = mix_2_1_1(c[C], c[R], c[B]);
            }
        }
        118 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[L]);
            if !similar(y[T], y[R]) {
                o[1] = c[C];
            } else {
                o[1] = mix_2_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_3_1(c[C], c[L]);
            o[3] = mix_3_1(c[C], c[BR]);
        }
        217 => {
            o[0] = mix_3_1(c[C], c[T]);
            o[1] = mix_2_1_1(c[C], c[TR], c[T]);
            o[2] = mix_3_1(c[C], c[BL]);
            if !similar(y[R], y[B]) {
                o[3] = c[C];
            } else {
                o[3] = mix_2_1_1(c[C], c[R], c[B]);
            }
        }
        110 => {
            o[0] = mix_3_1(c[C], c[TL]);
            o[1] = mix_3_1(c[C], c[R]);
            if !similar(y[B], y[L]) {
                o[2] = c[C];
            } else {
                o[2] = mix_2_1_1(c[C], c[B], c[L]);
            }
            o[3] = mix_2_1_1(c[C], c[BR], c[R]);
        }
        155 => {
            if !similar(y[L], y[T]) {
                o[0] = c[C];
            } else {
                o[0] = mix_2_1_1(c[C], c[L], c[T]);
            }
            o[1] = mix_3_1(c[C], c[TR]);
            o[2] = mix_2_1_1(c[C], c[BL], c[B]);
            o[3] = mix_3_1(c[C], c[B]);
        }
        188 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[T]);
            o[1] = mix_3_1(c[C], c[T]);
            o[2] = mix_3_1(c[C], c[B]);
            o[3] = mix_3_1(c[C], c[B]);
        }
        185 => {
            o[0] = mix_3_1(c[C], c[T]);
            o[1] = mix_2_1_1(c[C], c[TR], c[T]);
            o[2] = mix_3_1(c[C], c[B]);
            o[3] = mix_3_1(c[C], c[B]);
        }
        61 => {
            o[0] = mix_3_1(c[C], c[T]);
            o[1] = mix_3_1(c[C], c[T]);
            o[2] = mix_3_1(c[C], c[B]);
            o[3] = mix_2_1_1(c[C], c[BR], c[B]);
        }
        157 => {
            o[0] = mix_3_1(c[C], c[T]);
            o[1] = mix_3_1(c[C], c[T]);
            o[2] = mix_2_1_1(c[C], c[BL], c[B]);
            o[3] = mix_3_1(c[C], c[B]);
        }
        103 => {
            o[0] = mix_3_1(c[C], c[L]);
            o[1] = mix_3_1(c[C], c[R]);
            o[2] = mix_3_1(c[C], c[L]);
            o[3] = mix_2_1_1(c[C], c[BR], c[R]);
        }
        227 => {
            o[0] = mix_3_1(c[C], c[L]);
            o[1] = mix_2_1_1(c[C], c[TR], c[R]);
            o[2] = mix_3_1(c[C], c[L]);
            o[3] = mix_3_1(c[C], c[R]);
        }
        230 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[L]);
            o[1] = mix_3_1(c[C], c[R]);
            o[2] = mix_3_1(c[C], c[L]);
            o[3] = mix_3_1(c[C], c[R]);
        }
        199 => {
            o[0] = mix_3_1(c[C], c[L]);
            o[1] = mix_3_1(c[C], c[R]);
            o[2] = mix_2_1_1(c[C], c[BL], c[L]);
            o[3] = mix_3_1(c[C], c[R]);
        }
        220 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[T]);
            o[1] = mix_3_1(c[C], c[T]);
            if !similar(y[B], y[L]) {
                o[2] = mix_3_1(c[C], c[BL]);
            } else {
                o[2] = mix_6_1_1(c[C], c[B], c[L]);
            }
            if !similar(y[R], y[B]) {
                o[3] = c[C];
            } else {
                o[3] = mix_2_1_1(c[C], c[R], c[B]);
            }
        }
        158 => {
            if !similar(y[L], y[T]) {
                o[0] = mix_3_1(c[C], c[TL]);
            } else {
                o[0] = mix_6_1_1(c[C], c[L], c[T]);
            }
            if !similar(y[T], y[R]) {
                o[1] = c[C];
            } else {
                o[1] = mix_2_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_2_1_1(c[C], c[BL], c[B]);
            o[3] = mix_3_1(c[C], c[B]);
        }
        234 => {
            if !similar(y[L], y[T]) {
                o[0] = mix_3_1(c[C], c[TL]);
            } else {
                o[0] = mix_6_1_1(c[C], c[L], c[T]);
            }
            o[1] = mix_2_1_1(c[C], c[TR], c[R]);
            if !similar(y[B], y[L]) {
                o[2] = c[C];
            } else {
                o[2] = mix_2_1_1(c[C], c[B], c[L]);
            }
            o[3] = mix_3_1(c[C], c[R]);
        }
        242 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[L]);
            if !similar(y[T], y[R]) {
                o[1] = mix_3_1(c[C], c[TR]);
            } else {
                o[1] = mix_6_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_3_1(c[C], c[L]);
            if !similar(y[R], y[B]) {
                o[3] = c[C];
            } else {
                o[3] = mix_2_1_1(c[C], c[R], c[B]);
            }
        }
        59 => {
            if !similar(y[L], y[T]) {
                o[0] = c[C];
            } else {
                o[0] = mix_2_1_1(c[C], c[L], c[T]);
            }
            if !similar(y[T], y[R]) {
                o[1] = mix_3_1(c[C], c[TR]);
            } else {
                o[1] = mix_6_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_3_1(c[C], c[B]);
            o[3] = mix_2_1_1(c[C], c[BR], c[B]);
        }
        121 => {
            o[0] = mix_3_1(c[C], c[T]);
            o[1] = mix_2_1_1(c[C], c[TR], c[T]);
            if !similar(y[B], y[L]) {
                o[2] = c[C];
            } else {
                o[2] = mix_2_1_1(c[C], c[B], c[L]);
            }
            if !similar(y[R], y[B]) {
                o[3] = mix_3_1(c[C], c[BR]);
            } else {
                o[3] = mix_6_1_1(c[C], c[R], c[B]);
            }
        }
        87 => {
            o[0] = mix_3_1(c[C], c[L]);
            if !similar(y[T], y[R]) {
                o[1] = c[C];
            } else {
                o[1] = mix_2_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_2_1_1(c[C], c[BL], c[L]);
            if !similar(y[R], y[B]) {
                o[3] = mix_3_1(c[C], c[BR]);
            } else {
                o[3] = mix_6_1_1(c[C], c[R], c[B]);
            }
        }
        79 => {
            if !similar(y[L], y[T]) {
                o[0] = c[C];
            } else {
                o[0] = mix_2_1_1(c[C], c[L], c[T]);
            }
            o[1] = mix_3_1(c[C], c[R]);
            if !similar(y[B], y[L]) {
                o[2] = mix_3_1(c[C], c[BL]);
            } else {
                o[2] = mix_6_1_1(c[C], c[B], c[L]);
            }
            o[3] = mix_2_1_1(c[C], c[BR], c[R]);
        }
        122 => {
            if !similar(y[L], y[T]) {
                o[0] = mix_3_1(c[C], c[TL]);
            } else {
                o[0] = mix_6_1_1(c[C], c[L], c[T]);
            }
            if !similar(y[T], y[R]) {
                o[1] = mix_3_1(c[C], c[TR]);
            } else {
                o[1] = mix_6_1_1(c[C], c[T], c[R]);
            }
            if !similar(y[B], y[L]) {
                o[2] = c[C];
            } else {
                o[2] = mix_2_1_1(c[C], c[B], c[L]);
            }
            if !similar(y[R], y[B]) {
                o[3] = mix_3_1(c[C], c[BR]);
            } else {
                o[3] = mix_6_1_1(c[C], c[R], c[B]);
            }
        }
        94 => {
            if !similar(y[L], y[T]) {
                o[0] = mix_3_1(c[C], c[TL]);
            } else {
                o[0] = mix_6_1_1(c[C], c[L], c[T]);
            }
            if !similar(y[T], y[R]) {
                o[1] = c[C];
            } else {
                o[1] = mix_2_1_1(c[C], c[T], c[R]);
            }
            if !similar(y[B], y[L]) {
                o[2] = mix_3_1(c[C], c[BL]);
            } else {
                o[2] = mix_6_1_1(c[C], c[B], c[L]);
            }
            if !similar(y[R], y[B]) {
                o[3] = mix_3_1(c[C], c[BR]);
            } else {
                o[3] = mix_6_1_1(c[C], c[R], c[B]);
            }
        }
        218 => {
            if !similar(y[L], y[T]) {
                o[0] = mix_3_1(c[C], c[TL]);
            } else {
                o[0] = mix_6_1_1(c[C], c[L], c[T]);
            }
            if !similar(y[T], y[R]) {
                o[1] = mix_3_1(c[C], c[TR]);
            } else {
                o[1] = mix_6_1_1(c[C], c[T], c[R]);
            }
            if !similar(y[B], y[L]) {
                o[2] = mix_3_1(c[C], c[BL]);
            } else {
                o[2] = mix_6_1_1(c[C], c[B], c[L]);
            }
            if !similar(y[R], y[B]) {
                o[3] = c[C];
            } else {
                o[3] = mix_2_1_1(c[C], c[R], c[B]);
            }
        }
        91 => {
            if !similar(y[L], y[T]) {
                o[0] = c[C];
            } else {
                o[0] = mix_2_1_1(c[C], c[L], c[T]);
            }
            if !similar(y[T], y[R]) {
                o[1] = mix_3_1(c[C], c[TR]);
            } else {
                o[1] = mix_6_1_1(c[C], c[T], c[R]);
            }
            if !similar(y[B], y[L]) {
                o[2] = mix_3_1(c[C], c[BL]);
            } else {
                o[2] = mix_6_1_1(c[C], c[B], c[L]);
            }
            if !similar(y[R], y[B]) {
                o[3] = mix_3_1(c[C], c[BR]);
            } else {
                o[3] = mix_6_1_1(c[C], c[R], c[B]);
            }
        }
        229 => {
            o[0] = mix_2_1_1(c[C], c[L], c[T]);
            o[1] = mix_2_1_1(c[C], c[T], c[R]);
            o[2] = mix_3_1(c[C], c[L]);
            o[3] = mix_3_1(c[C], c[R]);
        }
        167 => {
            o[0] = mix_3_1(c[C], c[L]);
            o[1] = mix_3_1(c[C], c[R]);
            o[2] = mix_2_1_1(c[C], c[B], c[L]);
            o[3] = mix_2_1_1(c[C], c[R], c[B]);
        }
        173 => {
            o[0] = mix_3_1(c[C], c[T]);
            o[1] = mix_2_1_1(c[C], c[T], c[R]);
            o[2] = mix_3_1(c[C], c[B]);
            o[3] = mix_2_1_1(c[C], c[R], c[B]);
        }
        181 => {
            o[0] = mix_2_1_1(c[C], c[L], c[T]);
            o[1] = mix_3_1(c[C], c[T]);
            o[2] = mix_2_1_1(c[C], c[B], c[L]);
            o[3] = mix_3_1(c[C], c[B]);
        }
        186 => {
            if !similar(y[L], y[T]) {
                o[0] = mix_3_1(c[C], c[TL]);
            } else {
                o[0] = mix_6_1_1(c[C], c[L], c[T]);
            }
            if !similar(y[T], y[R]) {
                o[1] = mix_3_1(c[C], c[TR]);
            } else {
                o[1] = mix_6_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_3_1(c[C], c[B]);
            o[3] = mix_3_1(c[C], c[B]);
        }
        115 => {
            o[0] = mix_3_1(c[C], c[L]);
            if !similar(y[T], y[R]) {
                o[1] = mix_3_1(c[C], c[TR]);
            } else {
                o[1] = mix_6_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_3_1(c[C], c[L]);
            if !similar(y[R], y[B]) {
                o[3] = mix_3_1(c[C], c[BR]);
            } else {
                o[3] = mix_6_1_1(c[C], c[R], c[B]);
            }
        }
        93 => {
            o[0] = mix_3_1(c[C], c[T]);
            o[1] = mix_3_1(c[C], c[T]);
            if !similar(y[B], y[L]) {
                o[2] = mix_3_1(c[C], c[BL]);
            } else {
                o[2] = mix_6_1_1(c[C], c[B], c[L]);
            }
            if !similar(y[R], y[B]) {
                o[3] = mix_3_1(c[C], c[BR]);
            } else {
                o[3] = mix_6_1_1(c[C], c[R], c[B]);
            }
        }
        206 => {
            if !similar(y[L], y[T]) {
                o[0] = mix_3_1(c[C], c[TL]);
            } else {
                o[0] = mix_6_1_1(c[C], c[L], c[T]);
            }
            o[1] = mix_3_1(c[C], c[R]);
            if !similar(y[B], y[L]) {
                o[2] = mix_3_1(c[C], c[BL]);
            } else {
                o[2] = mix_6_1_1(c[C], c[B], c[L]);
            }
            o[3] = mix_3_1(c[C], c[R]);
        }
        205 | 201 => {
            o[0] = mix_3_1(c[C], c[T]);
            o[1] = mix_2_1_1(c[C], c[T], c[R]);
            if !similar(y[B], y[L]) {
                o[2] = mix_3_1(c[C], c[BL]);
            } else {
                o[2] = mix_6_1_1(c[C], c[B], c[L]);
            }
            o[3] = mix_3_1(c[C], c[R]);
        }
        174 | 46 => {
            if !similar(y[L], y[T]) {
                o[0] = mix_3_1(c[C], c[TL]);
            } else {
                o[0] = mix_6_1_1(c[C], c[L], c[T]);
            }
            o[1] = mix_3_1(c[C], c[R]);
            o[2] = mix_3_1(c[C], c[B]);
            o[3] = mix_2_1_1(c[C], c[R], c[B]);
        }
        179 | 147 => {
            o[0] = mix_3_1(c[C], c[L]);
            if !similar(y[T], y[R]) {
                o[1] = mix_3_1(c[C], c[TR]);
            } else {
                o[1] = mix_6_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_2_1_1(c[C], c[B], c[L]);
            o[3] = mix_3_1(c[C], c[B]);
        }
        117 | 116 => {
            o[0] = mix_2_1_1(c[C], c[L], c[T]);
            o[1] = mix_3_1(c[C], c[T]);
            o[2] = mix_3_1(c[C], c[L]);
            if !similar(y[R], y[B]) {
                o[3] = mix_3_1(c[C], c[BR]);
            } else {
                o[3] = mix_6_1_1(c[C], c[R], c[B]);
            }
        }
        189 => {
            o[0] = mix_3_1(c[C], c[T]);
            o[1] = mix_3_1(c[C], c[T]);
            o[2] = mix_3_1(c[C], c[B]);
            o[3] = mix_3_1(c[C], c[B]);
        }
        231 => {
            o[0] = mix_3_1(c[C], c[L]);
            o[1] = mix_3_1(c[C], c[R]);
            o[2] = mix_3_1(c[C], c[L]);
            o[3] = mix_3_1(c[C], c[R]);
        }
        126 => {
            o[0] = mix_3_1(c[C], c[TL]);
            if !similar(y[T], y[R]) {
                o[1] = c[C];
            } else {
                o[1] = mix_2_1_1(c[C], c[T], c[R]);
            }
            if !similar(y[B], y[L]) {
                o[2] = c[C];
            } else {
                o[2] = mix_2_1_1(c[C], c[B], c[L]);
            }
            o[3] = mix_3_1(c[C], c[BR]);
        }
        219 => {
            if !similar(y[L], y[T]) {
                o[0] = c[C];
            } else {
                o[0] = mix_2_1_1(c[C], c[L], c[T]);
            }
            o[1] = mix_3_1(c[C], c[TR]);
            o[2] = mix_3_1(c[C], c[BL]);
            if !similar(y[R], y[B]) {
                o[3] = c[C];
            } else {
                o[3] = mix_2_1_1(c[C], c[R], c[B]);
            }
        }
        125 => {
            if !similar(y[B], y[L]) {
                o[0] = mix_3_1(c[C], c[T]);
                o[2] = c[C];
            } else {
                o[0] = mix_5_2_1(c[C], c[L], c[T]);
                o[2] = mix_2_3_3(c[C], c[B], c[L]);
            }
            o[1] = mix_3_1(c[C], c[T]);
            o[3] = mix_3_1(c[C], c[BR]);
        }
        221 => {
            o[0] = mix_3_1(c[C], c[T]);
            if !similar(y[R], y[B]) {
                o[1] = mix_3_1(c[C], c[T]);
                o[3] = c[C];
            } else {
                o[1] = mix_5_2_1(c[C], c[R], c[T]);
                o[3] = mix_2_3_3(c[C], c[R], c[B]);
            }
            o[2] = mix_3_1(c[C], c[BL]);
        }
        207 => {
            if !similar(y[L], y[T]) {
                o[0] = c[C];
                o[1] = mix_3_1(c[C], c[R]);
            } else {
                o[0] = mix_2_3_3(c[C], c[L], c[T]);
                o[1] = mix_5_2_1(c[C], c[T], c[R]);
            }
            o[2] = mix_3_1(c[C], c[BL]);
            o[3] = mix_3_1(c[C], c[R]);
        }
        238 => {
            o[0] = mix_3_1(c[C], c[TL]);
            o[1] = mix_3_1(c[C], c[R]);
            if !similar(y[B], y[L]) {
                o[2] = c[C];
                o[3] = mix_3_1(c[C], c[R]);
            } else {
                o[2] = mix_2_3_3(c[C], c[B], c[L]);
                o[3] = mix_5_2_1(c[C], c[B], c[R]);
            }
        }
        190 => {
            o[0] = mix_3_1(c[C], c[TL]);
            if !similar(y[T], y[R]) {
                o[1] = c[C];
                o[3] = mix_3_1(c[C], c[B]);
            } else {
                o[1] = mix_2_3_3(c[C], c[T], c[R]);
                o[3] = mix_5_2_1(c[C], c[R], c[B]);
            }
            o[2] = mix_3_1(c[C], c[B]);
        }
        187 => {
            if !similar(y[L], y[T]) {
                o[0] = c[C];
                o[2] = mix_3_1(c[C], c[B]);
            } else {
                o[0] = mix_2_3_3(c[C], c[L], c[T]);
                o[2] = mix_5_2_1(c[C], c[L], c[B]);
            }
            o[1] = mix_3_1(c[C], c[TR]);
            o[3] = mix_3_1(c[C], c[B]);
        }
        243 => {
            o[0] = mix_3_1(c[C], c[L]);
            o[1] = mix_3_1(c[C], c[TR]);
            if !similar(y[R], y[B]) {
                o[2] = mix_3_1(c[C], c[L]);
                o[3] = c[C];
            } else {
                o[2] = mix_5_2_1(c[C], c[B], c[L]);
                o[3] = mix_2_3_3(c[C], c[R], c[B]);
            }
        }
        119 => {
            if !similar(y[T], y[R]) {
                o[0] = mix_3_1(c[C], c[L]);
                o[1] = c[C];
            } else {
                o[0] = mix_5_2_1(c[C], c[T], c[L]);
                o[1] = mix_2_3_3(c[C], c[T], c[R]);
            }
            o[2] = mix_3_1(c[C], c[L]);
            o[3] = mix_3_1(c[C], c[BR]);
        }
        237 | 233 => {
            o[0] = mix_3_1(c[C], c[T]);
            o[1] = mix_2_1_1(c[C], c[T], c[R]);
            if !similar(y[B], y[L]) {
                o[2] = c[C];
            } else {
                o[2] = mix_14_1_1(c[C], c[B], c[L]);
            }
            o[3] = mix_3_1(c[C], c[R]);
        }
        175 | 47 => {
            if !similar(y[L], y[T]) {
                o[0] = c[C];
            } else {
                o[0] = mix_14_1_1(c[C], c[L], c[T]);
            }
            o[1] = mix_3_1(c[C], c[R]);
            o[2] = mix_3_1(c[C], c[B]);
            o[3] = mix_2_1_1(c[C], c[R], c[B]);
        }
        183 | 151 => {
            o[0] = mix_3_1(c[C], c[L]);
            if !similar(y[T], y[R]) {
                o[1] = c[C];
            } else {
                o[1] = mix_14_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_2_1_1(c[C], c[B], c[L]);
            o[3] = mix_3_1(c[C], c[B]);
        }
        245 | 244 => {
            o[0] = mix_2_1_1(c[C], c[L], c[T]);
            o[1] = mix_3_1(c[C], c[T]);
            o[2] = mix_3_1(c[C], c[L]);
            if !similar(y[R], y[B]) {
                o[3] = c[C];
            } else {
                o[3] = mix_14_1_1(c[C], c[R], c[B]);
            }
        }
        250 => {
            o[0] = mix_3_1(c[C], c[TL]);
            o[1] = mix_3_1(c[C], c[TR]);
            if !similar(y[B], y[L]) {
                o[2] = c[C];
            } else {
                o[2] = mix_2_1_1(c[C], c[B], c[L]);
            }
            if !similar(y[R], y[B]) {
                o[3] = c[C];
            } else {
                o[3] = mix_2_1_1(c[C], c[R], c[B]);
            }
        }
        123 => {
            if !similar(y[L], y[T]) {
                o[0] = c[C];
            } else {
                o[0] = mix_2_1_1(c[C], c[L], c[T]);
            }
            o[1] = mix_3_1(c[C], c[TR]);
            if !similar(y[B], y[L]) {
                o[2] = c[C];
            } else {
                o[2] = mix_2_1_1(c[C], c[B], c[L]);
            }
            o[3] = mix_3_1(c[C], c[BR]);
        }
        95 => {
            if !similar(y[L], y[T]) {
                o[0] = c[C];
            } else {
                o[0] = mix_2_1_1(c[C], c[L], c[T]);
            }
            if !similar(y[T], y[R]) {
                o[1] = c[C];
            } else {
                o[1] = mix_2_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_3_1(c[C], c[BL]);
            o[3] = mix_3_1(c[C], c[BR]);
        }
        222 => {
            o[0] = mix_3_1(c[C], c[TL]);
            if !similar(y[T], y[R]) {
                o[1] = c[C];
            } else {
                o[1] = mix_2_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_3_1(c[C], c[BL]);
            if !similar(y[R], y[B]) {
                o[3] = c[C];
            } else {
                o[3] = mix_2_1_1(c[C], c[R], c[B]);
            }
        }
        252 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[T]);
            o[1] = mix_3_1(c[C], c[T]);
            if !similar(y[B], y[L]) {
                o[2] = c[C];
            } else {
                o[2] = mix_2_1_1(c[C], c[B], c[L]);
            }
            if !similar(y[R], y[B]) {
                o[3] = c[C];
            } else {
                o[3] = mix_14_1_1(c[C], c[R], c[B]);
            }
        }
        249 => {
            o[0] = mix_3_1(c[C], c[T]);
            o[1] = mix_2_1_1(c[C], c[TR], c[T]);
            if !similar(y[B], y[L]) {
                o[2] = c[C];
            } else {
                o[2] = mix_14_1_1(c[C], c[B], c[L]);
            }
            if !similar(y[R], y[B]) {
                o[3] = c[C];
            } else {
                o[3] = mix_2_1_1(c[C], c[R], c[B]);
            }
        }
        235 => {
            if !similar(y[L], y[T]) {
                o[0] = c[C];
            } else {
                o[0] = mix_2_1_1(c[C], c[L], c[T]);
            }
            o[1] = mix_2_1_1(c[C], c[TR], c[R]);
            if !similar(y[B], y[L]) {
                o[2] = c[C];
            } else {
                o[2] = mix_14_1_1(c[C], c[B], c[L]);
            }
            o[3] = mix_3_1(c[C], c[R]);
        }
        111 => {
            if !similar(y[L], y[T]) {
                o[0] = c[C];
            } else {
                o[0] = mix_14_1_1(c[C], c[L], c[T]);
            }
            o[1] = mix_3_1(c[C], c[R]);
            if !similar(y[B], y[L]) {
                o[2] = c[C];
            } else {
                o[2] = mix_2_1_1(c[C], c[B], c[L]);
            }
            o[3] = mix_2_1_1(c[C], c[BR], c[R]);
        }
        63 => {
            if !similar(y[L], y[T]) {
                o[0] = c[C];
            } else {
                o[0] = mix_14_1_1(c[C], c[L], c[T]);
            }
            if !similar(y[T], y[R]) {
                o[1] = c[C];
            } else {
                o[1] = mix_2_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_3_1(c[C], c[B]);
            o[3] = mix_2_1_1(c[C], c[BR], c[B]);
        }
        159 => {
            if !similar(y[L], y[T]) {
                o[0] = c[C];
            } else {
                o[0] = mix_2_1_1(c[C], c[L], c[T]);
            }
            if !similar(y[T], y[R]) {
                o[1] = c[C];
            } else {
                o[1] = mix_14_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_2_1_1(c[C], c[BL], c[B]);
            o[3] = mix_3_1(c[C], c[B]);
        }
        215 => {
            o[0] = mix_3_1(c[C], c[L]);
            if !similar(y[T], y[R]) {
                o[1] = c[C];
            } else {
                o[1] = mix_14_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_2_1_1(c[C], c[BL], c[L]);
            if !similar(y[R], y[B]) {
                o[3] = c[C];
            } else {
                o[3] = mix_2_1_1(c[C], c[R], c[B]);
            }
        }
        246 => {
            o[0] = mix_2_1_1(c[C], c[TL], c[L]);
            if !similar(y[T], y[R]) {
                o[1] = c[C];
            } else {
                o[1] = mix_2_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_3_1(c[C], c[L]);
            if !similar(y[R], y[B]) {
                o[3] = c[C];
            } else {
                o[3] = mix_14_1_1(c[C], c[R], c[B]);
            }
        }
        254 => {
            o[0] = mix_3_1(c[C], c[TL]);
            if !similar(y[T], y[R]) {
                o[1] = c[C];
            } else {
                o[1] = mix_2_1_1(c[C], c[T], c[R]);
            }
            if !similar(y[B], y[L]) {
                o[2] = c[C];
            } else {
                o[2] = mix_2_1_1(c[C], c[B], c[L]);
            }
            if !similar(y[R], y[B]) {
                o[3] = c[C];
            } else {
                o[3] = mix_14_1_1(c[C], c[R], c[B]);
            }
        }
        253 => {
            o[0] = mix_3_1(c[C], c[T]);
            o[1] = mix_3_1(c[C], c[T]);
            if !similar(y[B], y[L]) {
                o[2] = c[C];
            } else {
                o[2] = mix_14_1_1(c[C], c[B], c[L]);
            }
            if !similar(y[R], y[B]) {
                o[3] = c[C];
            } else {
                o[3] = mix_14_1_1(c[C], c[R], c[B]);
            }
        }
        251 => {
            if !similar(y[L], y[T]) {
                o[0] = c[C];
            } else {
                o[0] = mix_2_1_1(c[C], c[L], c[T]);
            }
            o[1] = mix_3_1(c[C], c[TR]);
            if !similar(y[B], y[L]) {
                o[2] = c[C];
            } else {
                o[2] = mix_14_1_1(c[C], c[B], c[L]);
            }
            if !similar(y[R], y[B]) {
                o[3] = c[C];
            } else {
                o[3] = mix_2_1_1(c[C], c[R], c[B]);
            }
        }
        239 => {
            if !similar(y[L], y[T]) {
                o[0] = c[C];
            } else {
                o[0] = mix_14_1_1(c[C], c[L], c[T]);
            }
            o[1] = mix_3_1(c[C], c[R]);
            if !similar(y[B], y[L]) {
                o[2] = c[C];
            } else {
                o[2] = mix_14_1_1(c[C], c[B], c[L]);
            }
            o[3] = mix_3_1(c[C], c[R]);
        }
        127 => {
            if !similar(y[L], y[T]) {
                o[0] = c[C];
            } else {
                o[0] = mix_14_1_1(c[C], c[L], c[T]);
            }
            if !similar(y[T], y[R]) {
                o[1] = c[C];
            } else {
                o[1] = mix_2_1_1(c[C], c[T], c[R]);
            }
            if !similar(y[B], y[L]) {
                o[2] = c[C];
            } else {
                o[2] = mix_2_1_1(c[C], c[B], c[L]);
            }
            o[3] = mix_3_1(c[C], c[BR]);
        }
        191 => {
            if !similar(y[L], y[T]) {
                o[0] = c[C];
            } else {
                o[0] = mix_14_1_1(c[C], c[L], c[T]);
            }
            if !similar(y[T], y[R]) {
                o[1] = c[C];
            } else {
                o[1] = mix_14_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_3_1(c[C], c[B]);
            o[3] = mix_3_1(c[C], c[B]);
        }
        223 => {
            if !similar(y[L], y[T]) {
                o[0] = c[C];
            } else {
                o[0] = mix_2_1_1(c[C], c[L], c[T]);
            }
            if !similar(y[T], y[R]) {
                o[1] = c[C];
            } else {
                o[1] = mix_14_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_3_1(c[C], c[BL]);
            if !similar(y[R], y[B]) {
                o[3] = c[C];
            } else {
                o[3] = mix_2_1_1(c[C], c[R], c[B]);
            }
        }
        247 => {
            o[0] = mix_3_1(c[C], c[L]);
            if !similar(y[T], y[R]) {
                o[1] = c[C];
            } else {
                o[1] = mix_14_1_1(c[C], c[T], c[R]);
            }
            o[2] = mix_3_1(c[C], c[L]);
            if !similar(y[R], y[B]) {
                o[3] = c[C];
            } else {
                o[3] = mix_14_1_1(c[C], c[R], c[B]);
            }
        }
        255 => {
            if !similar(y[L], y[T]) {
                o[0] = c[C];
            } else {
                o[0] = mix_14_1_1(c[C], c[L], c[T]);
            }
            if !similar(y[T], y[R]) {
                o[1] = c[C];
            } else {
                o[1] = mix_14_1_1(c[C], c[T], c[R]);
            }
            if !similar(y[B], y[L]) {
                o[2] = c[C];
            } else {
                o[2] = mix_14_1_1(c[C], c[B], c[L]);
            }
            if !similar(y[R], y[B]) {
                o[3] = c[C];
            } else {
                o[3] = mix_14_1_1(c[C], c[R], c[B]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{hq2x, rgb888};

    #[test]
    fn a_flat_frame_stays_flat() {
        let frame = vec![0xf800u16; 6 * 4];
        let out = hq2x(&frame, 6, 4);

        assert_eq!(out.len(), 12 * 8);
        assert!(out.iter().all(|&pixel| pixel == 0xff0000));
    }

    /// A white pixel on black is rounded off: its corners blend toward the
    /// black around it, so none of its four is pure white or pure black.
    #[test]
    fn a_lone_pixel_is_rounded() {
        let mut frame = vec![0u16; 3 * 3];
        frame[4] = 0xffff;
        let out = hq2x(&frame, 3, 3);

        for at in [2 * 6 + 2, 2 * 6 + 3, 3 * 6 + 2, 3 * 6 + 3] {
            assert_ne!(out[at], rgb888(0xffff));
            assert_ne!(out[at], 0);
        }
        assert_eq!(out[0], 0);
    }
}

#[cfg(test)]
mod reference {
    /// Compares against a frame doubled by the reference implementation:
    /// `HQ2X_IN` raw little-endian RGB565, `HQ2X_REF` raw RGB888 of twice its
    /// size, `HQ2X_SIZE` "WxH".
    #[test]
    #[ignore]
    fn matches_the_reference() {
        let (Ok(input), Ok(reference), Ok(size)) = (std::env::var("HQ2X_IN"), std::env::var("HQ2X_REF"), std::env::var("HQ2X_SIZE")) else {
            return;
        };
        let (width, height) = size.split_once('x').unwrap();
        let (width, height): (usize, usize) = (width.parse().unwrap(), height.parse().unwrap());
        let pixels: Vec<u16> = std::fs::read(input)
            .unwrap()
            .chunks(2)
            .map(|x| u16::from_le_bytes([x[0], x[1]]))
            .collect();
        let expected = std::fs::read(reference).unwrap();

        let mut out = super::hq2x(&pixels, width, height);
        let started = std::time::Instant::now();
        for _ in 0..20 {
            out = super::hq2x(&pixels, width, height);
        }
        eprintln!("hq2x {width}x{height}: {:?} a frame", started.elapsed() / 20);

        let mut mismatched = 0;
        for (at, &pixel) in out.iter().enumerate() {
            let want = u32::from(expected[at * 3]) << 16 | u32::from(expected[at * 3 + 1]) << 8 | u32::from(expected[at * 3 + 2]);
            if pixel != want {
                if mismatched < 10 {
                    eprintln!("({}, {}): {pixel:06x} != {want:06x}", at % (width * 2), at / (width * 2));
                }
                mismatched += 1;
            }
        }
        assert_eq!(mismatched, 0);
    }
}
