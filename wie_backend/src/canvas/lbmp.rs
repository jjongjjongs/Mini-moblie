use alloc::{boxed::Box, format, vec::Vec};

use bytemuck::pod_collect_to_vec;

use wie_util::{Result, WieError};

use crate::canvas::{ArgbPixel, Color, Image, PixelType, Rgb332Pixel, Rgb565Pixel, VecImageBuffer};

// lcd bitmap file format for skvm
//
// A 24-byte header - `LBMP`, then the bit depth, width, height, the length of
// one plane and a mask flag, each a little-endian u32 - and the pixels the LCD
// would hold. The packed depths are one RGB332 byte or one RGB565 halfword a
// pixel, row by row.
//
// Two halves of the format were missing here, and the reference emulator
// (wfeature, `internal/wipic/lbmp.go`) settles both from a corpus of about nine
// hundred SKT files:
//
// - **A depth below 8 is stored as bit planes** in the LCD's own page layout:
//   byte `(y / 8) * width + x`, one row of pixels a bit, the top row of each
//   band in bit 0. `size` is then one plane, and a two-bit image is two of
//   them. The value is ink rather than light - zero is white and the widest
//   value is black - which a title shipping the same glyph sheets as a white
//   and a black font shows: the shapes and masks are the same and only the
//   value under the mask differs, 0 in the white one and 3 in the black one.
// - **The mask flag means a one-bit transparency plane follows the pixels**,
//   in the same page layout, and a set bit is transparent.
//
// Neither was read before, so a planar file failed to decode at all. 디지몬RPGII
// draws its text from two-bit glyph sheets (`img/xfont/*/cho1.lbm` and the
// rest); every one of them came back null, and the game thread died on its first
// line of text with the screen left on the last frame it had drawn.

const HEADER_SIZE: usize = 24;

/// What a header may claim before it is refused, so a corrupt one costs an
/// error rather than an allocation.
const MAX_PIXELS: u64 = 1 << 24;

struct LbmpHeader {
    depth: u32,
    width: u32,
    height: u32,
    size: u32,
    mask: u32,
}

impl LbmpHeader {
    fn parse(data: &[u8]) -> Option<Self> {
        let word = |index: usize| {
            let offset = 4 + index * 4;
            Some(u32::from_le_bytes(data.get(offset..offset + 4)?.try_into().ok()?))
        };

        Some(Self {
            depth: word(0)?,
            width: word(1)?,
            height: word(2)?,
            size: word(3)?,
            mask: word(4)?,
        })
    }
}

pub fn decode_lbmp(data: &[u8]) -> Result<Box<dyn Image>> {
    let header = LbmpHeader::parse(data).ok_or_else(|| WieError::Unimplemented("Truncated LBMP header".into()))?;
    let body = &data[HEADER_SIZE.min(data.len())..];

    let (width, height) = (header.width, header.height);
    if width == 0 || height == 0 || width as u64 * height as u64 > MAX_PIXELS {
        return Err(WieError::Unimplemented(format!("LBMP dimensions {width}x{height} are out of range")));
    }

    let pixel_count = width as usize * height as usize;
    let plane_bytes = width as usize * height.div_ceil(8) as usize;

    let (plane_size, planes) = match header.depth {
        1 | 2 | 4 => (plane_bytes, header.depth as usize),
        8 => (pixel_count, 1),
        16 => (pixel_count * 2, 1),
        depth => return Err(WieError::Unimplemented(format!("Unsupported type {depth}"))),
    };

    let pixel_bytes = plane_size * planes;
    if body.len() < pixel_bytes {
        return Err(WieError::Unimplemented(format!(
            "LBMP holds {} pixel bytes, want {pixel_bytes}",
            body.len()
        )));
    }
    let pixels = &body[..pixel_bytes];

    // A file may carry more than it declares - a sprite sheet's later frames
    // sit past the first image - so the mask is there only when the flag says
    // so and the bytes are too.
    let mask = (header.mask != 0).then(|| body.get(pixel_bytes..pixel_bytes + plane_bytes)).flatten();

    if header.size as usize != plane_size {
        tracing::warn!(
            "LBMP declares a {} byte plane for {width}x{height} at {} bpp, want {plane_size}",
            header.size,
            header.depth
        );
    }

    let planar = header.depth < 8;
    if !planar && mask.is_none() {
        return Ok(if header.depth == 8 {
            Box::new(VecImageBuffer::<Rgb332Pixel>::from_raw(width, height, pixels.to_vec()))
        } else {
            Box::new(VecImageBuffer::<Rgb565Pixel>::from_raw(width, height, pod_collect_to_vec(pixels)))
        });
    }

    let mut argb = Vec::with_capacity(pixel_count);
    for y in 0..height as usize {
        for x in 0..width as usize {
            let index = y * width as usize + x;
            let mut color = if planar {
                planar_pixel(pixels, header.depth, plane_bytes, width as usize, x, y)
            } else if header.depth == 8 {
                Rgb332Pixel::to_color(pixels[index])
            } else {
                Rgb565Pixel::to_color(u16::from_le_bytes([pixels[index * 2], pixels[index * 2 + 1]]))
            };

            if let Some(mask) = mask
                && plane_bit(mask, width as usize, x, y)
            {
                color.a = 0;
            }

            argb.push(ArgbPixel::from_color(color));
        }
    }

    Ok(Box::new(VecImageBuffer::<ArgbPixel>::from_raw(width, height, argb)))
}

/// One pixel out of the bit planes, as a grey level with zero as white.
///
/// Which plane is the low bit is not settled: every two-bit file in the corpus
/// has identical planes and uses only the two ends of its range.
fn planar_pixel(pixels: &[u8], depth: u32, plane_bytes: usize, width: usize, x: usize, y: usize) -> Color {
    let value = (0..depth as usize)
        .filter(|plane| plane_bit(&pixels[plane * plane_bytes..(plane + 1) * plane_bytes], width, x, y))
        .fold(0u32, |value, plane| value | (1 << plane));

    let levels = (1 << depth) - 1;
    let grey = (255 - value * 255 / levels) as u8;

    Color {
        a: 0xff,
        r: grey,
        g: grey,
        b: grey,
    }
}

/// One bit of a plane in the LCD's page layout: bytes run left to right along a
/// band of eight rows, and the topmost row of the band is bit 0.
fn plane_bit(plane: &[u8], width: usize, x: usize, y: usize) -> bool {
    plane.get((y / 8) * width + x).is_some_and(|byte| byte >> (y % 8) & 1 == 1)
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::decode_lbmp;
    use crate::canvas::{PixelType, Rgb332Pixel};

    fn lbmp(depth: u32, width: u32, height: u32, size: u32, mask: u32, body: &[u8]) -> Vec<u8> {
        let mut data = b"LBMP".to_vec();
        for word in [depth, width, height, size, mask] {
            data.extend_from_slice(&word.to_le_bytes());
        }
        data.extend_from_slice(body);
        data
    }

    /// A two-bit glyph: two identical planes and a mask, in page layout. Where
    /// both planes are set the pixel is black, where neither is it is white,
    /// and where the mask is set it is transparent whatever it holds.
    #[test]
    fn a_two_bit_image_reads_its_planes_as_ink_and_its_mask_as_transparency() {
        // 2x3: column 0 has rows 0 and 2 set, column 1 nothing. The mask
        // covers column 1, row 1.
        let planes = [0b101, 0b000, 0b101, 0b000];
        let mask = [0b000, 0b010];
        let data = lbmp(2, 2, 3, 2, 1, &[&planes[..], &mask[..]].concat());

        let image = decode_lbmp(&data).unwrap();
        assert_eq!((image.width(), image.height()), (2, 3));

        let black = image.get_pixel(0, 0);
        assert_eq!((black.a, black.r, black.g, black.b), (0xff, 0, 0, 0));

        let white = image.get_pixel(0, 1);
        assert_eq!((white.a, white.r, white.g, white.b), (0xff, 0xff, 0xff, 0xff));

        assert_eq!(image.get_pixel(1, 0).a, 0xff);
        assert_eq!(image.get_pixel(1, 1).a, 0);
    }

    /// A packed image with no mask decodes exactly as before, in its own
    /// pixel format.
    #[test]
    fn a_packed_image_without_a_mask_keeps_its_pixel_format() {
        let data = lbmp(8, 2, 1, 2, 0, &[0xe0, 0x1c, 0xaa, 0xaa]);

        let image = decode_lbmp(&data).unwrap();
        assert_eq!(image.bytes_per_pixel(), 1);
        assert_eq!(image.get_pixel(0, 0).r, Rgb332Pixel::to_color(0xe0).r);
    }

    /// A packed sprite with a mask comes back with the masked pixels
    /// transparent rather than as a rectangle of its background.
    #[test]
    fn a_masked_packed_image_is_transparent_under_its_mask() {
        let data = lbmp(8, 2, 1, 2, 1, &[0xe0, 0xe3, 0b0, 0b1]);

        let image = decode_lbmp(&data).unwrap();
        assert_eq!(image.get_pixel(0, 0).a, 0xff);
        assert_eq!(image.get_pixel(1, 0).a, 0);
    }

    #[test]
    fn a_truncated_file_is_refused_rather_than_read_past() {
        assert!(decode_lbmp(b"LBMP\x02\x00").is_err());
        assert!(decode_lbmp(&lbmp(2, 8, 8, 8, 0, &[0; 4])).is_err());
    }
}
