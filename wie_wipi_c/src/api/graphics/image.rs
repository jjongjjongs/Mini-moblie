use alloc::boxed::Box;
use alloc::collections::BTreeSet;
use alloc::vec;
use alloc::vec::Vec;

use wie_backend::canvas::{ArgbPixel, Color, Image, PixelType, Rgb565Pixel, VecImageBuffer, decode_image};
use wie_util::Result;

use wipi_types::wipic::{WIPICImage, WIPICIndirectPtr, WIPICWord};

use crate::{api::graphics::framebuffer::FrameBuffer, context::WIPICContext};

pub fn create_wipi_image(context: &mut dyn WIPICContext, buf: WIPICIndirectPtr, offset: WIPICWord, len: WIPICWord) -> Result<WIPICImage> {
    let ptr_image_data = context.data_ptr(buf)?;

    let mut data = vec![0; len as _];
    context.read_bytes(ptr_image_data + offset, &mut data)?;
    let image = decode_image(&data)?;
    let image = stretched_to_panel(context, image);

    // A title that blits straight out of image memory reads it at the display's
    // own depth - 16bpp RGB565, the depth MC_grpGetFrameBpp reports - so the
    // colour plane is stored there. Handing back a 32bpp buffer to be read at
    // that stride is what turned HYBRID 1's sprites into noise. RGB565 cannot
    // carry the decoder's 8-bit alpha, so the full ARGB is kept in the mask
    // plane, from which MC_grpDrawImage composites transparency; that path is
    // unchanged.
    let width = image.width();
    let height = image.height();
    let colors = image.colors();
    let has_alpha = colors.iter().any(|color| color.a != 0xff);
    let raw: Vec<u16> = colors.iter().map(|color| Rgb565Pixel::from_color(*color)).collect();
    let rgb565 = VecImageBuffer::<Rgb565Pixel>::from_raw(width, height, raw);

    let (mask_at, mask_bits) = transparency_bits(&colors);
    let img_framebuffer = FrameBuffer::from_image_with_trailer(context, &rgb565, mask_at, &mask_bits)?;
    register_trailer(img_framebuffer.0.buf.0);
    // Only an image that carries alpha needs the full ARGB kept in the mask
    // plane for MC_grpDrawImage to composite; a fully opaque image composites
    // straight from the 16bpp colour plane and is spared the second copy.
    let mask_framebuffer = if has_alpha {
        FrameBuffer::from_image(context, &*image)?
    } else {
        FrameBuffer::empty()
    };

    Ok(WIPICImage {
        img: img_framebuffer.0,
        mask: mask_framebuffer.0,
        loop_count: 0,
        delay: 0,
        animated: 0,
        buf,
        offset,
        current: 0,
        len,
    })
}

/// `image` enlarged to the panel, when it is the full-screen picture this title
/// draws at a smaller handset's size - see
/// [`wie_backend::TitleQuirks::stretched_picture`]. Every other picture is
/// returned as it was decoded.
fn stretched_to_panel(context: &mut dyn WIPICContext, image: Box<dyn Image>) -> Box<dyn Image> {
    let Some(size) = context.system().title_stretched_picture() else {
        return image;
    };
    if size != (image.width(), image.height()) {
        return image;
    }

    let screen = context.system().platform().screen();
    let (width, height) = (screen.width(), screen.height());
    if (width, height) == size || width == 0 || height == 0 {
        return image;
    }

    tracing::info!("enlarging a {}x{} picture to the {width}x{height} panel", size.0, size.1);

    Box::new(resample(&*image, width, height))
}

/// `image` resampled to `width` x `height`, each pixel taken from the source
/// pixel under its centre.
///
/// Nearest rather than blended: 크로이센 carries the picture over into a store
/// of its own, which takes a picture's colours as the ones it was drawn with -
/// handed a blended enlargement, with thousands of in-between colours, it drew
/// the backdrop as a dark speckle. Every colour this hands back is one the
/// picture already had.
fn resample(image: &dyn Image, width: u32, height: u32) -> VecImageBuffer<ArgbPixel> {
    let (source_width, source_height) = (image.width() as u64, image.height() as u64);
    let colors = image.colors();

    let mut raw = Vec::with_capacity((width * height) as usize);
    for y in 0..height as u64 {
        let source_y = ((2 * y + 1) * source_height / (2 * height as u64)).min(source_height - 1);
        for x in 0..width as u64 {
            let source_x = ((2 * x + 1) * source_width / (2 * width as u64)).min(source_width - 1);
            raw.push(ArgbPixel::from_color(colors[(source_y * source_width + source_x) as usize]));
        }
    }

    VecImageBuffer::<ArgbPixel>::from_raw(width, height, raw)
}

/// The handset's one-bit transparency mask for a decoded image, and where in
/// the colour plane's buffer it goes.
///
/// LGT keeps an image's transparency in the same buffer as its colour, right
/// after the pixels: a bit per pixel in row order, no padding between rows,
/// sixteen to a little-endian halfword with the first pixel in the low bit, set
/// where the pixel is transparent. It starts on the first four-byte boundary
/// past the pixels - the pixel count rounded up to an even number.
///
/// A title that draws text in a colour of its own reads that mask straight out
/// of image memory. 아무이유없어 fetches a glyph strip's buffer
/// (`MC_grpGetImageFrameBuffer`, `MC_grpGetFrameBufferPointer`) and, at
/// `0xc27a`, walks the mask past its `width * height` pixels, writing the
/// colour it wants wherever a bit is clear. With the mask kept only in a
/// separate ARGB plane, that memory held nothing, every bit read clear, and
/// every highlighted word came down as a solid block of its colour.
pub(crate) fn transparency_bits(colors: &[Color]) -> (u32, Vec<u8>) {
    let pixels = colors.len();
    let at = (pixels + (pixels & 1)) as u32 * 2;

    // Whole halfwords, because the title reads the mask a halfword at a time.
    let mut bits = vec![0u8; pixels.div_ceil(16) * 2];
    for (index, color) in colors.iter().enumerate() {
        if color.a < 0x80 {
            bits[index / 8] |= 1 << (index % 8);
        }
    }

    (at, bits)
}

/// The colour planes that carry the one-bit mask after their pixels - see
/// [`transparency_bits`] - by buffer address.
///
/// Only the images this runtime built itself are known to have one; an image a
/// test or another path put together does not, and the bytes past its pixels are
/// whatever the allocator left there. So the mask is only believed for these.
static TRAILERED: spin::Mutex<BTreeSet<WIPICWord>> = spin::Mutex::new(BTreeSet::new());

pub(crate) fn register_trailer(buf: WIPICWord) {
    TRAILERED.lock().insert(buf);
}

pub(crate) fn forget_trailer(buf: WIPICWord) {
    TRAILERED.lock().remove(&buf);
}

pub(crate) fn has_trailer(buf: WIPICWord) -> bool {
    TRAILERED.lock().contains(&buf)
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use wie_backend::canvas::{ArgbPixel, Color, Image, VecImageBuffer};

    use super::{resample, transparency_bits};

    /// Enlarging covers the panel with the picture's own colours and no others,
    /// each in its own share of the width.
    #[test]
    fn resampling_keeps_the_pictures_colours() {
        let picture = VecImageBuffer::<ArgbPixel>::from_raw(2, 1, vec![0xff000000, 0xffffffff]);
        let enlarged = resample(&picture, 8, 3);

        assert_eq!((enlarged.width(), enlarged.height()), (8, 3));
        for y in 0..3 {
            for x in 0..8 {
                assert_eq!(enlarged.get_pixel(x, y).r, if x < 4 { 0 } else { 0xff }, "({x}, {y})");
                assert_eq!(enlarged.get_pixel(x, y).a, 0xff);
            }
        }
    }

    const INK: Color = Color { a: 0xff, r: 0, g: 0, b: 0 };
    const CLEAR: Color = Color {
        a: 0,
        r: 0xff,
        g: 0xff,
        b: 0xff,
    };

    /// The layout 아무이유없어 reads at `0xc27a`: a bit per pixel in row order,
    /// first pixel in the low bit, set where the pixel is transparent, starting
    /// past the pixels rounded up to an even count.
    #[test]
    fn the_mask_follows_the_pixels_a_bit_each_set_where_clear() {
        // Three pixels by three: an odd count, so the mask starts one pixel on.
        let colors = [INK, CLEAR, INK, CLEAR, INK, CLEAR, INK, CLEAR, INK];
        let (at, bits) = transparency_bits(&colors);

        assert_eq!(at, 10 * 2, "nine pixels round up to ten, two bytes each");
        assert_eq!(bits.len(), 2, "read a halfword at a time, so a whole one");
        assert_eq!(u16::from_le_bytes([bits[0], bits[1]]), 0b0_1010_1010);
    }

    /// An even count starts the mask straight after the pixels.
    #[test]
    fn an_even_count_needs_no_padding() {
        let (at, bits) = transparency_bits(&[INK; 4]);

        assert_eq!(at, 4 * 2);
        assert_eq!(bits, [0, 0], "every pixel is ink");
    }
}
