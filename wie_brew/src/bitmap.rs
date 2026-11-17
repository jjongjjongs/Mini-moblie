use alloc::{format, vec, vec::Vec};

use wie_backend::canvas::{ArgbPixel, VecImageBuffer};
use wie_util::{Result, WieError};

/// The file header and the information header a BMP opens with.
pub const HEADER_SIZE: usize = 14 + 0x28;

const PIXEL_OFFSET_FIELD: usize = 0x0a;
const INFORMATION_SIZE_FIELD: usize = 0x0e;
const WIDTH_FIELD: usize = 0x12;
const HEIGHT_FIELD: usize = 0x16;
const BITS_PER_PIXEL_FIELD: usize = 0x1c;
const COMPRESSION_FIELD: usize = 0x1e;
const PALETTE_SIZE_FIELD: usize = 0x2e;

const MAX_BITMAP: usize = 4 << 20;

fn u16_at(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

fn u32_at(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
}

/// How many bytes the bitmap whose headers are `header` occupies: the larger
/// of what its file header names and what its pixels need.
///
/// A title hands an image over as a pointer into a block it loaded, and keeps
/// that block; this is how much of it the image is.
pub fn bitmap_length(header: &[u8]) -> Option<usize> {
    if header.len() < HEADER_SIZE || &header[..2] != b"BM" {
        return None;
    }

    let named = u32_at(header, 2) as usize;
    let pixels = u32_at(header, PIXEL_OFFSET_FIELD) as usize;
    let width = u32_at(header, WIDTH_FIELD) as i32;
    let height = (u32_at(header, HEIGHT_FIELD) as i32).unsigned_abs() as usize;
    let depth = u16_at(header, BITS_PER_PIXEL_FIELD) as usize;

    if width <= 0 || height == 0 || depth == 0 || width as usize > MAX_BITMAP || height > MAX_BITMAP {
        return None;
    }

    let stride = (width as usize * depth).div_ceil(32) * 4;
    let needed = (pixels + stride * height).max(named);

    (needed != 0 && needed <= MAX_BITMAP).then_some(needed)
}

/// Decodes an uncompressed 4, 8 or 24-bit BMP into ARGB pixels, every one
/// opaque. Transparency is the platform's colour key, magenta, applied when
/// the image is drawn.
pub fn decode(data: &[u8]) -> Result<VecImageBuffer<ArgbPixel>> {
    if data.len() < HEADER_SIZE || &data[..2] != b"BM" {
        return Err(WieError::FatalError("not a bitmap".into()));
    }

    let pixel_offset = u32_at(data, PIXEL_OFFSET_FIELD) as usize;
    let width = u32_at(data, WIDTH_FIELD) as i32;
    let raw_height = u32_at(data, HEIGHT_FIELD) as i32;
    let depth = u16_at(data, BITS_PER_PIXEL_FIELD) as usize;
    let compression = u32_at(data, COMPRESSION_FIELD);

    if compression != 0 {
        return Err(WieError::FatalError(format!("bitmap uses compression {compression}")));
    }
    if depth != 4 && depth != 8 && depth != 24 {
        return Err(WieError::FatalError(format!("bitmap is {depth} bits per pixel")));
    }

    let top_down = raw_height < 0;
    let height = raw_height.unsigned_abs() as usize;
    if width <= 0 || height == 0 || width as usize > MAX_BITMAP || height > MAX_BITMAP {
        return Err(WieError::FatalError(format!("bitmap is {width}x{raw_height}")));
    }
    let width = width as usize;

    let stride = (width * depth).div_ceil(32) * 4;
    if stride * height > MAX_BITMAP {
        return Err(WieError::FatalError("bitmap is too large".into()));
    }

    let mut palette = Vec::new();
    if depth != 24 {
        let mut count = u32_at(data, PALETTE_SIZE_FIELD) as usize;
        if count == 0 {
            count = 1 << depth;
        }
        if count > 1 << depth {
            return Err(WieError::FatalError(format!("bitmap names {count} palette entries")));
        }

        let start = 14 + u32_at(data, INFORMATION_SIZE_FIELD) as usize;
        let entries = data
            .get(start..start + count * 4)
            .ok_or_else(|| WieError::FatalError("bitmap palette runs past its data".into()))?;
        palette = entries
            .chunks_exact(4)
            .map(|x| 0xff00_0000 | ((x[2] as u32) << 16) | ((x[1] as u32) << 8) | x[0] as u32)
            .collect();
    }

    let rows = data
        .get(pixel_offset..pixel_offset + stride * height)
        .ok_or_else(|| WieError::FatalError("bitmap pixels run past its data".into()))?;

    let mut pixels = vec![0u32; width * height];
    for y in 0..height {
        let source = if top_down { y } else { height - 1 - y };
        let row = &rows[source * stride..];

        for x in 0..width {
            let pixel = match depth {
                24 => 0xff00_0000 | ((row[x * 3 + 2] as u32) << 16) | ((row[x * 3 + 1] as u32) << 8) | row[x * 3] as u32,
                _ => {
                    let index = if depth == 8 {
                        row[x] as usize
                    } else if x % 2 == 0 {
                        (row[x / 2] >> 4) as usize
                    } else {
                        (row[x / 2] & 0xf) as usize
                    };

                    *palette
                        .get(index)
                        .ok_or_else(|| WieError::FatalError(format!("bitmap indexes palette entry {index}")))?
                }
            };

            pixels[y * width + x] = pixel;
        }
    }

    Ok(VecImageBuffer::from_raw(width as u32, height as u32, pixels))
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use wie_backend::canvas::Image;

    use super::{HEADER_SIZE, bitmap_length, decode};

    /// A 2x2, 24-bit, bottom-up bitmap: red and green on the top row, blue and
    /// white under them.
    fn bitmap() -> alloc::vec::Vec<u8> {
        let mut data = vec![0u8; HEADER_SIZE];
        data[0] = b'B';
        data[1] = b'M';
        data[0x0a..0x0e].copy_from_slice(&(HEADER_SIZE as u32).to_le_bytes());
        data[0x0e..0x12].copy_from_slice(&0x28u32.to_le_bytes());
        data[0x12..0x16].copy_from_slice(&2u32.to_le_bytes());
        data[0x16..0x1a].copy_from_slice(&2u32.to_le_bytes());
        data[0x1c..0x1e].copy_from_slice(&24u16.to_le_bytes());
        // Bottom row first, each padded to a multiple of four bytes.
        data.extend_from_slice(&[0xff, 0, 0, 0xff, 0xff, 0xff, 0, 0]);
        data.extend_from_slice(&[0, 0, 0xff, 0, 0xff, 0, 0, 0]);
        data
    }

    #[test]
    fn a_bottom_up_bitmap_is_read_top_row_first() {
        let data = bitmap();
        assert_eq!(bitmap_length(&data), Some(HEADER_SIZE + 16));

        let image = decode(&data).unwrap();
        assert_eq!((image.width(), image.height()), (2, 2));

        let colours: alloc::vec::Vec<_> = image.colors().iter().map(|x| (x.r, x.g, x.b)).collect();
        assert_eq!(colours, [(0xff, 0, 0), (0, 0xff, 0), (0, 0, 0xff), (0xff, 0xff, 0xff)]);
    }
}
