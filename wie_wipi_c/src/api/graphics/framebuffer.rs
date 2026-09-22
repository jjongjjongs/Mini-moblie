use alloc::{boxed::Box, vec, vec::Vec};
use core::ops::{Deref, DerefMut};

use bytemuck::pod_collect_to_vec;

use wipi_types::wipic::{WIPICFramebuffer, WIPICIndirectPtr, WIPICWord};

use wie_backend::canvas::{ArgbPixel, Canvas, Color, Image, ImageBufferCanvas, PixelType, Rgb8Pixel, Rgb565Pixel, VecImageBuffer};
use wie_util::{Result, WieError};

use crate::context::WIPICContext;

// same 256MB as wie_core_arm's HEAP_SIZE; not referenced directly to avoid the dependency
const MAX_FRAMEBUFFER_BYTES: u32 = 0x1000_0000;

pub(crate) fn buffer_size(width: u32, height: u32, bytes_per_pixel: u32) -> Result<(u32, u32)> {
    let bpl = width.checked_mul(bytes_per_pixel).ok_or(WieError::AllocationFailure)?;
    let size = bpl.checked_mul(height).ok_or(WieError::AllocationFailure)?;
    if size > MAX_FRAMEBUFFER_BYTES {
        return Err(WieError::AllocationFailure);
    }

    Ok((size, bpl))
}

/// `source` composed over `under`, exactly as `ImageBufferCanvas::blend_pixel`
/// composes them - the same f32 factor and the same truncation, so a blit that
/// goes the direct way lands on the same byte as one that went through the
/// canvas.
fn blend(source: Color, under: Color) -> Color {
    let factor = source.a as f32 / 255.0;

    Color {
        a: 0xff,
        r: (source.r as f32 * factor + under.r as f32 * (1.0 - factor)) as u8,
        g: (source.g as f32 * factor + under.g as f32 * (1.0 - factor)) as u8,
        b: (source.b as f32 * factor + under.b as f32 * (1.0 - factor)) as u8,
    }
}

pub struct FrameBuffer(pub WIPICFramebuffer);

impl FrameBuffer {
    pub fn empty() -> Self {
        Self(WIPICFramebuffer {
            width: 0,
            height: 0,
            bpl: 0,
            bpp: 0,
            buf: WIPICIndirectPtr(0),
        })
    }

    pub fn new(context: &mut dyn WIPICContext, width: WIPICWord, height: WIPICWord, bpp: WIPICWord) -> Result<Self> {
        let bytes_per_pixel = bpp / 8;

        let (size, bpl) = buffer_size(width, height, bytes_per_pixel)?;
        let buf = context.alloc(size)?;

        Ok(Self(WIPICFramebuffer {
            width,
            height,
            bpl,
            bpp: bytes_per_pixel * 8,
            buf,
        }))
    }

    pub fn from_image(context: &mut dyn WIPICContext, image: &dyn Image) -> Result<Self> {
        let (size, bpl) = buffer_size(image.width(), image.height(), image.bytes_per_pixel())?;
        let buf = context.alloc(size)?;

        context.write_bytes(context.data_ptr(buf)?, &image.raw())?;

        Ok(Self(WIPICFramebuffer {
            width: image.width(),
            height: image.height(),
            bpl,
            bpp: image.bytes_per_pixel() * 8,
            buf,
        }))
    }

    pub fn data(&self, context: &dyn WIPICContext) -> Result<Vec<u8>> {
        let (size, _) = buffer_size(self.0.width, self.0.height, self.0.bpp / 8)?;
        let mut buf = vec![0; size as _];
        context.read_bytes(context.data_ptr(self.0.buf)?, &mut buf)?;

        Ok(buf)
    }

    pub fn image(&self, context: &mut dyn WIPICContext) -> Result<Box<dyn Image>> {
        let data = self.data(context)?;

        Ok(match self.0.bpp {
            16 => Box::new(VecImageBuffer::<Rgb565Pixel>::from_raw(
                self.0.width as _,
                self.0.height as _,
                pod_collect_to_vec(&data),
            )),
            32 => Box::new(VecImageBuffer::<ArgbPixel>::from_raw(
                self.0.width as _,
                self.0.height as _,
                pod_collect_to_vec(&data),
            )),
            _ => unimplemented!("Unsupported pixel format: {}", self.0.bpp),
        })
    }

    pub fn canvas<'a>(&'a self, context: &'a mut dyn WIPICContext) -> Result<FramebufferCanvas<'a>> {
        let data = self.data(context)?;

        let canvas: Box<dyn Canvas> = match self.0.bpp {
            16 => Box::new(ImageBufferCanvas::new(VecImageBuffer::<Rgb565Pixel>::from_raw(
                self.0.width as _,
                self.0.height as _,
                pod_collect_to_vec(&data),
            ))),
            32 => Box::new(ImageBufferCanvas::new(VecImageBuffer::<ArgbPixel>::from_raw(
                self.0.width as _,
                self.0.height as _,
                pod_collect_to_vec(&data),
            ))),
            _ => unimplemented!("Unsupported pixel format: {}", self.0.bpp),
        };

        Ok(FramebufferCanvas {
            framebuffer: self,
            context,
            canvas,
            flushed: false,
            snapshot: data,
        })
    }

    pub fn write(&self, context: &mut dyn WIPICContext, data: &[u8]) -> Result<()> {
        context.write_bytes(context.data_ptr(self.0.buf)?, data)
    }

    /// Writes back only the bytes that differ between the snapshot the canvas
    /// started from and what it drew, leaving every other pixel exactly as guest
    /// memory holds it now.
    ///
    /// A whole-buffer write of the drawn image would re-stamp the snapshot over
    /// pixels the title wrote straight into the same buffer - its own decoded
    /// artwork, or a blit another thread made after we took the snapshot - which
    /// is why backgrounds came out partly black behind our text and shapes.
    /// Restaging only the pixels a primitive actually changed keeps those
    /// direct writes intact, and matches how the reference draws each primitive
    /// straight into the framebuffer rather than through a full-frame copy.
    /// Fills a solid rectangle by writing only the rows it covers.
    ///
    /// The canvas path a primitive normally takes stages the whole surface:
    /// it reads every byte of it out of guest memory, collects that into a
    /// pixel buffer, draws, and diffs the result back row by row. That is the
    /// right shape for a primitive whose coverage is hard to predict ahead of
    /// the draw, and much the wrong one for a solid rectangle - 엑시온2 draws
    /// its scene, and its minimap, as tens of thousands of 2x2 fills a frame,
    /// and each one was paying two full 240x320 copies out and a whole-surface
    /// comparison back for four pixels of work.
    ///
    /// The rectangle is clamped exactly the way the canvas clamps it, to the
    /// reported width and height, and `MC_grpFillRect` passes the rectangle
    /// itself as the clip so nothing else narrows it. The colour is stored
    /// rather than composed, which is what the canvas does for a fully opaque
    /// one - the caller checks that before coming here.
    ///
    /// `false` when the surface's depth is not one a colour can be packed for,
    /// or its geometry does not fit the addressing; the caller should take the
    /// canvas path instead. Nothing has been written when it returns `false`.
    pub fn fill_rect_direct(&self, context: &mut dyn WIPICContext, x: i32, y: i32, w: u32, h: u32, color: Color) -> Result<bool> {
        let pixel: Vec<u8> = match self.0.bpp {
            16 => Rgb565Pixel::from_color(color).to_le_bytes().to_vec(),
            32 => ArgbPixel::from_color(color).to_le_bytes().to_vec(),
            _ => return Ok(false),
        };

        let bpp = (self.0.bpp / 8).max(1) as i64;
        let bpl = self.0.bpl as i64;
        if bpl <= 0 || pixel.len() as i64 != bpp {
            return Ok(false);
        }

        let left = (x as i64).max(0);
        let right = (x as i64 + w as i64).min(self.0.width as i64);
        let top = (y as i64).max(0);
        let bottom = (y as i64 + h as i64).min(self.0.height as i64);
        if left >= right || top >= bottom {
            return Ok(true);
        }

        // The furthest byte the loop would touch, checked before any of it is
        // written so a surface this cannot address is refused whole rather
        // than half filled.
        let row_bytes = (right - left) * bpp;
        let last = (bottom - 1) * bpl + left * bpp + row_bytes;
        if u32::try_from(last).is_err() {
            return Ok(false);
        }

        let row = pixel.repeat((right - left) as usize);
        let base = context.data_ptr(self.0.buf)?;

        for py in top..bottom {
            let offset = (py * bpl + left * bpp) as u32;
            context.write_bytes(base + offset, &row)?;
        }

        Ok(true)
    }

    /// Sets one pixel by writing that pixel, and nothing else.
    ///
    /// The canvas path stages the whole surface for every primitive - the
    /// bytes out of guest memory, a pixel buffer collected from them, and a
    /// whole-surface comparison back. For a single pixel that is four passes
    /// over a quarter of a megabyte to move two bytes. 드래곤하트2's menus draw
    /// that way, two and a half thousand `MC_grpPutPixel` a second beside two
    /// thousand fills, and a Y700 held its AP at 3.2GHz for as long as one was
    /// open while the same title in play sat at its floor.
    ///
    /// The colour is stored rather than composed, as `fill_rect_direct` stores
    /// it - the caller checks it is fully opaque first. Outside the surface is
    /// no-op, the way drawing on a canvas is.
    ///
    /// `false` when the surface's depth or geometry is not one this can
    /// address; the caller should take the canvas path. Nothing has been
    /// written when it returns `false`.
    pub fn put_pixel_direct(&self, context: &mut dyn WIPICContext, x: i32, y: i32, color: Color) -> Result<bool> {
        let pixel: Vec<u8> = match self.0.bpp {
            16 => Rgb565Pixel::from_color(color).to_le_bytes().to_vec(),
            32 => ArgbPixel::from_color(color).to_le_bytes().to_vec(),
            _ => return Ok(false),
        };

        let Some(offset) = self.byte_offset(x, y, pixel.len()) else {
            return Ok(true);
        };

        let base = context.data_ptr(self.0.buf)?;
        context.write_bytes(base + offset, &pixel)?;

        Ok(true)
    }

    /// Blits `src` onto this surface touching only the rows the blit covers.
    ///
    /// The canvas path stages the whole surface for every primitive, and for a
    /// blit it stages the whole source as well: both read out of guest memory,
    /// both collected into pixel buffers, a snapshot of the destination kept,
    /// and the destination compared back byte for byte. That cost is set by
    /// the surfaces, not by the sprite, which is why 드래곤하트2 spends the same
    /// hundred microseconds on a 10x7 sprite as on a 29x28 one while a fill of
    /// similar size next to it in the same log costs eleven. In combat it asks
    /// for three thousand of them a second, and a Y700 holds its AP at the top
    /// clock for as long as the fight lasts.
    ///
    /// Only the rows of the overlap are read, from each surface, and only
    /// those rows are written back.
    ///
    /// Two shapes are covered, matching `MC_grpDrawImage`'s two:
    ///
    /// - `keyed`: an image with no alpha draws from its 16bpp colour plane and
    ///   the magenta key stands in for transparency, pixel stored rather than
    ///   composed. Both surfaces being RGB565, the pixel is copied raw: the
    ///   565 round trip through `Color` is exact, so the copy is what the
    ///   canvas would have written.
    /// - otherwise: an image carrying alpha draws from its 32bpp mask plane,
    ///   composed onto the destination the way `blend_pixel` composes it.
    ///
    /// `false` for any other combination of depths, or geometry this cannot
    /// address; the caller should take the canvas path. Nothing has been
    /// written when it returns `false`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_image_direct(
        &self,
        context: &mut dyn WIPICContext,
        dx: i32,
        dy: i32,
        w: i32,
        h: i32,
        src: &FrameBuffer,
        sx: i32,
        sy: i32,
        keyed: bool,
    ) -> Result<bool> {
        if self.0.bpp != 16 || (keyed && src.0.bpp != 16) || (!keyed && src.0.bpp != 32) {
            return Ok(false);
        }

        let (dst_bpl, src_bpl) = (self.0.bpl as i64, src.0.bpl as i64);
        let src_bytes = (src.0.bpp / 8) as i64;
        if dst_bpl <= 0 || src_bpl <= 0 || src.0.buf.0 == 0 {
            return Ok(false);
        }

        // The overlap of destination, source and blit rectangle, worked out
        // exactly as `Canvas::draw` works it out. `MC_grpDrawImage` passes the
        // blit rectangle itself as the clip, so the clip narrows nothing
        // further and is not applied again here.
        let x_start = 0i64.max(-(dx as i64)).max(-(sx as i64));
        let x_end = (w as i64).min(self.0.width as i64 - dx as i64).min(src.0.width as i64 - sx as i64);
        let y_start = 0i64.max(-(dy as i64)).max(-(sy as i64));
        let y_end = (h as i64).min(self.0.height as i64 - dy as i64).min(src.0.height as i64 - sy as i64);
        if x_start >= x_end || y_start >= y_end {
            return Ok(true);
        }

        let cols = (x_end - x_start) as usize;
        let dst_row_bytes = cols * 2;
        let src_row_bytes = cols * src_bytes as usize;

        // The furthest byte either side would touch, checked before anything is
        // written so a surface this cannot address is refused whole.
        let dst_last = (dy as i64 + y_end - 1) * dst_bpl + (dx as i64 + x_end) * 2;
        let src_last = (sy as i64 + y_end - 1) * src_bpl + (sx as i64 + x_end) * src_bytes;
        if u32::try_from(dst_last).is_err() || u32::try_from(src_last).is_err() {
            return Ok(false);
        }

        let dst_base = context.data_ptr(self.0.buf)?;
        let src_base = context.data_ptr(src.0.buf)?;

        let mut dst_row = vec![0u8; dst_row_bytes];
        let mut src_row = vec![0u8; src_row_bytes];

        for y in y_start..y_end {
            let dst_off = ((dy as i64 + y) * dst_bpl + (dx as i64 + x_start) * 2) as u32;
            let src_off = ((sy as i64 + y) * src_bpl + (sx as i64 + x_start) * src_bytes) as u32;

            context.read_bytes(src_base + src_off, &mut src_row)?;
            context.read_bytes(dst_base + dst_off, &mut dst_row)?;

            let mut touched = false;
            for i in 0..cols {
                if keyed {
                    let raw = u16::from_le_bytes([src_row[i * 2], src_row[i * 2 + 1]]);
                    // `is_transparent_key` in 565 terms: red and blue at full
                    // scale, green no higher than the 7 it lets through - which
                    // is the bottom two of its six bits.
                    if (raw >> 11) == 0x1f && (raw & 0x1f) == 0x1f && ((raw >> 5) & 0x3f) <= 1 {
                        continue;
                    }
                    dst_row[i * 2] = src_row[i * 2];
                    dst_row[i * 2 + 1] = src_row[i * 2 + 1];
                } else {
                    let raw = u32::from_le_bytes([src_row[i * 4], src_row[i * 4 + 1], src_row[i * 4 + 2], src_row[i * 4 + 3]]);
                    let source = ArgbPixel::to_color(raw);
                    let under = Rgb565Pixel::to_color(u16::from_le_bytes([dst_row[i * 2], dst_row[i * 2 + 1]]));
                    let blended = blend(source, under);
                    let packed = Rgb565Pixel::from_color(blended).to_le_bytes();
                    dst_row[i * 2] = packed[0];
                    dst_row[i * 2 + 1] = packed[1];
                }
                touched = true;
            }

            if touched {
                context.write_bytes(dst_base + dst_off, &dst_row)?;
            }
        }

        Ok(true)
    }

    /// The byte a pixel starts at, or `None` when it is off the surface or the
    /// surface cannot be addressed this way.
    fn byte_offset(&self, x: i32, y: i32, bytes_per_pixel: usize) -> Option<u32> {
        let bpp = (self.0.bpp / 8).max(1) as i64;
        let bpl = self.0.bpl as i64;
        if bpl <= 0 || bytes_per_pixel as i64 != bpp {
            return None;
        }
        if x < 0 || y < 0 || x as i64 >= self.0.width as i64 || y as i64 >= self.0.height as i64 {
            return None;
        }

        u32::try_from(y as i64 * bpl + x as i64 * bpp).ok()
    }

    /// The part of `x, y, w, h` that is on this 16-bit surface, and its pixels,
    /// read row by row rather than by staging the whole surface.
    ///
    /// `None` when the surface is not 16-bit or cannot be addressed this way,
    /// and an empty rectangle when none of it is on the surface. Pixels come
    /// back row-major, `cols` to a row.
    #[allow(clippy::type_complexity)]
    pub fn read_rect_rgb565(&self, context: &dyn WIPICContext, x: i32, y: i32, w: i32, h: i32) -> Result<Option<(i32, i32, i32, i32, Vec<u16>)>> {
        if self.0.bpp != 16 {
            return Ok(None);
        }

        let bpl = self.0.bpl as i64;
        if bpl <= 0 {
            return Ok(None);
        }

        let left = (x as i64).max(0);
        let right = (x as i64 + w as i64).min(self.0.width as i64);
        let top = (y as i64).max(0);
        let bottom = (y as i64 + h as i64).min(self.0.height as i64);
        if left >= right || top >= bottom {
            return Ok(Some((0, 0, 0, 0, Vec::new())));
        }

        let cols = (right - left) as usize;
        let last = (bottom - 1) * bpl + right * 2;
        if u32::try_from(last).is_err() {
            return Ok(None);
        }

        let base = context.data_ptr(self.0.buf)?;
        let mut pixels = Vec::with_capacity(cols * (bottom - top) as usize);
        let mut row = vec![0u8; cols * 2];

        for py in top..bottom {
            context.read_bytes(base + (py * bpl + left * 2) as u32, &mut row)?;
            pixels.extend(row.as_chunks::<2>().0.iter().copied().map(u16::from_le_bytes));
        }

        Ok(Some((left as i32, top as i32, cols as i32, (bottom - top) as i32, pixels)))
    }

    /// Writes back what [`Self::read_rect_rgb565`] read, over the same
    /// rectangle.
    pub fn write_rect_rgb565(&self, context: &mut dyn WIPICContext, left: i32, top: i32, cols: i32, rows: i32, pixels: &[u16]) -> Result<()> {
        if cols <= 0 || rows <= 0 {
            return Ok(());
        }

        let bpl = self.0.bpl as i64;
        let base = context.data_ptr(self.0.buf)?;

        for (index, row) in pixels.chunks_exact(cols as usize).take(rows as usize).enumerate() {
            let bytes: Vec<u8> = row.iter().flat_map(|pixel| pixel.to_le_bytes()).collect();
            let offset = ((top as i64 + index as i64) * bpl + left as i64 * 2) as u32;
            context.write_bytes(base + offset, &bytes)?;
        }

        Ok(())
    }

    pub fn write_diff(&self, context: &mut dyn WIPICContext, snapshot: &[u8], drawn: &[u8]) -> Result<()> {
        let bpl = self.0.bpl as usize;
        let bpp = (self.0.bpp / 8).max(1) as usize;
        if bpl == 0 || snapshot.len() != drawn.len() {
            // Layout we cannot reason about row-wise; fall back to a full write.
            return self.write(context, drawn);
        }

        let base = context.data_ptr(self.0.buf)?;
        for (row, (snap_row, drawn_row)) in snapshot.chunks_exact(bpl).zip(drawn.chunks_exact(bpl)).enumerate() {
            // The changed span within the row - nothing outside it is touched, so
            // a direct write elsewhere in the row survives.
            let Some(first) = (0..bpl).find(|&i| snap_row[i] != drawn_row[i]) else {
                continue;
            };
            let last = (first..bpl).rev().find(|&i| snap_row[i] != drawn_row[i]).unwrap();
            // Snap the span out to whole-pixel boundaries. A 16bpp pixel splits
            // green across its two bytes, so writing a half pixel (when only one
            // of the two bytes changed) would corrupt the colour - the green
            // fringing along drawn edges. Rounding down to the pixel start and up
            // past the pixel end always writes complete pixels.
            let start = first - (first % bpp);
            let end = (bpl).min(last + bpp - (last % bpp));
            let byte_off = row * bpl + start;
            if let Ok(dst) = u32::try_from(byte_off) {
                context.write_bytes(base + dst, &drawn_row[start..end])?;
            }
        }

        Ok(())
    }

    pub fn pixel_to_color(&self, pixel: WIPICWord) -> Color {
        match self.0.bpp {
            16 => Rgb565Pixel::to_color(pixel as u16),
            _ => Rgb8Pixel::to_color(pixel),
        }
    }
}

pub struct FramebufferCanvas<'a> {
    framebuffer: &'a FrameBuffer,
    context: &'a mut dyn WIPICContext,
    canvas: Box<dyn Canvas>,
    flushed: bool,
    /// The framebuffer bytes as they were when this canvas was taken, so
    /// `flush` can write back only what the primitive actually changed.
    snapshot: Vec<u8>,
}

impl FramebufferCanvas<'_> {
    pub fn flush(mut self) -> Result<()> {
        self.flushed = true;

        let drawn = self.canvas.image().raw();
        self.framebuffer.write_diff(self.context, &self.snapshot, &drawn)
    }
}

// best-effort fallback for canvases dropped without an explicit flush
impl Drop for FramebufferCanvas<'_> {
    fn drop(&mut self) {
        if self.flushed {
            return;
        }

        // Named, because this is the one line that says a surface was staged
        // whole. A canvas reads every pixel of its surface out of guest memory,
        // collects them, keeps a snapshot, and here reads them back and diffs
        // the lot - and the paths that reach it are the ones a rectangle-sized
        // read refused. Which surface refused is the whole question, and a
        // bare line could not answer it: 드래곤하트2's menus produce twelve
        // hundred of these a second and nothing said what they were staging.
        tracing::warn!(
            "framebuffer canvas dropped without explicit flush: {}x{} at {}bpp, bpl {}; write-back errors will be lost",
            self.framebuffer.0.width,
            self.framebuffer.0.height,
            self.framebuffer.0.bpp,
            self.framebuffer.0.bpl,
        );

        let drawn = self.canvas.image().raw();
        if let Err(err) = self.framebuffer.write_diff(self.context, &self.snapshot, &drawn) {
            tracing::error!("Failed to flush framebuffer canvas: {err}");
        }
    }
}

impl Deref for FramebufferCanvas<'_> {
    type Target = Box<dyn Canvas>;

    fn deref(&self) -> &Self::Target {
        &self.canvas
    }
}

impl DerefMut for FramebufferCanvas<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.canvas
    }
}

#[cfg(test)]
mod test {
    use alloc::{vec, vec::Vec};

    use wie_util::{ByteRead, ByteWrite, WieError};

    use wie_backend::canvas::{Clip, Color};

    use crate::WIPICContext;
    use crate::context::test::TestContext;

    use super::FrameBuffer;
    use wie_backend::canvas::{PixelType, Rgb565Pixel};

    /// write_diff restages only the pixels a primitive changed, so a byte the
    /// title wrote straight into the framebuffer after the canvas snapshot (its
    /// own decoded artwork) survives our write-back instead of being re-stamped
    /// with the stale snapshot.
    #[test]
    fn write_diff_preserves_pixels_the_primitive_did_not_touch() {
        let mut context = TestContext::new();
        // 4x2 @ 16bpp -> bpl 8, 16 bytes.
        let fb = FrameBuffer::new(&mut context, 4, 2, 16).unwrap();
        let base = context.data_ptr(fb.0.buf).unwrap();

        // The snapshot the canvas started from.
        let snapshot = [0x11u8; 16];
        context.write_bytes(base, &snapshot).unwrap();

        // Our primitive changed exactly one pixel (bytes 4..6 of row 0).
        let mut drawn = snapshot;
        drawn[4] = 0xAA;
        drawn[5] = 0xBB;

        // Meanwhile the title blitted its own pixel straight into row 1,
        // *after* the snapshot was taken.
        context.write_bytes(base + 12, &[0xCC, 0xDD]).unwrap();

        fb.write_diff(&mut context, &snapshot, &drawn).unwrap();

        let mut out = [0u8; 16];
        context.read_bytes(base, &mut out).unwrap();
        // Our drawn pixel landed.
        assert_eq!(&out[4..6], &[0xAA, 0xBB]);
        // The title's direct write survived (not clobbered by the snapshot).
        assert_eq!(&out[12..14], &[0xCC, 0xDD]);
        // Everything else is still the snapshot.
        assert_eq!(out[0], 0x11);
        assert_eq!(out[6], 0x11);
        assert_eq!(out[14], 0x11);
    }

    /// When only one byte of a 16bpp pixel changes, write_diff still restages the
    /// whole pixel (both bytes), so green - which straddles the two bytes - is
    /// never left half-written.
    #[test]
    fn fill_rect_direct_matches_the_canvas_it_replaces() {
        // Every rectangle worth disagreeing about: inside, hanging off each
        // edge, straddling a corner, and entirely outside.
        for (x, y, w, h) in [
            (1i32, 1i32, 2u32, 2u32),
            (-2, 1, 4, 2),
            (6, 0, 4, 3),
            (1, -3, 2, 5),
            (-9, -9, 3, 3),
            (9, 9, 2, 2),
        ] {
            let mut direct = TestContext::new();
            let fb = FrameBuffer::new(&mut direct, 8, 6, 16).unwrap();
            let base = direct.data_ptr(fb.0.buf).unwrap();
            direct.write_bytes(base, &[0x5au8; 8 * 6 * 2]).unwrap();

            let color = Color {
                a: 0xff,
                r: 0x12,
                g: 0x34,
                b: 0x56,
            };
            assert!(fb.fill_rect_direct(&mut direct, x, y, w, h, color).unwrap());

            // The same fill through the canvas, which is what it stands in for.
            let mut staged = TestContext::new();
            let other = FrameBuffer::new(&mut staged, 8, 6, 16).unwrap();
            staged.write_bytes(base, &[0x5au8; 8 * 6 * 2]).unwrap();
            let mut canvas = other.canvas(&mut staged).unwrap();
            let clip = Clip { x, y, width: w, height: h };
            canvas.fill_rect(x, y, w, h, color, clip);
            canvas.flush().unwrap();

            let mut got = [0u8; 8 * 6 * 2];
            let mut want = [0u8; 8 * 6 * 2];
            direct.read_bytes(base, &mut got).unwrap();
            staged.read_bytes(base, &mut want).unwrap();
            assert_eq!(got, want, "fill ({x}, {y}, {w}, {h})");
        }
    }

    /// Fills `fb` with a pattern and returns the bytes, so the direct blit and
    /// the canvas blit start from identical surfaces.
    fn seed(context: &mut TestContext, fb: &FrameBuffer, seed: u8) -> Vec<u8> {
        let (size, _) = super::buffer_size(fb.0.width, fb.0.height, fb.0.bpp / 8).unwrap();
        let bytes: Vec<u8> = (0..size as usize).map(|i| (i as u8).wrapping_mul(7).wrapping_add(seed)).collect();
        let base = context.data_ptr(fb.0.buf).unwrap();
        context.write_bytes(base, &bytes).unwrap();
        bytes
    }

    /// A blit that goes the direct way has to land on the same bytes as one
    /// that went through the canvas - the sprite inside the surface, hanging
    /// off each edge, and entirely off it.
    ///
    /// Both shapes are checked: the keyed one, where an image without alpha
    /// draws from its 16bpp colour plane and magenta stands in for
    /// transparency, and the composed one, where an image with alpha draws
    /// from its 32bpp mask plane.
    #[test]
    fn draw_image_direct_matches_the_canvas_it_replaces() {
        for keyed in [true, false] {
            for (dx, dy, w, h, sx, sy) in [
                (1i32, 1i32, 3i32, 3i32, 0i32, 0i32),
                (-2, 1, 4, 3, 0, 0),
                (6, 0, 4, 3, 0, 0),
                (1, -2, 3, 4, 0, 0),
                (1, 1, 3, 3, 1, 1),
                (-9, -9, 3, 3, 0, 0),
                (9, 9, 2, 2, 0, 0),
                (0, 0, 8, 6, 0, 0),
            ] {
                let src_bpp = if keyed { 16 } else { 32 };

                let mut direct = TestContext::new();
                let dst = FrameBuffer::new(&mut direct, 8, 6, 16).unwrap();
                let src = FrameBuffer::new(&mut direct, 4, 4, src_bpp).unwrap();
                let dst_base = direct.data_ptr(dst.0.buf).unwrap();
                seed(&mut direct, &dst, 0x20);
                let src_bytes = seed(&mut direct, &src, 0x91);
                if keyed {
                    // One magenta pixel, so the key is exercised rather than
                    // just asserted about.
                    let key = Rgb565Pixel::from_color(Color {
                        a: 0xff,
                        r: 0xff,
                        g: 0,
                        b: 0xff,
                    });
                    let sb = direct.data_ptr(src.0.buf).unwrap();
                    direct.write_bytes(sb + 2, &key.to_le_bytes()).unwrap();
                }
                assert!(
                    dst.draw_image_direct(&mut direct, dx, dy, w, h, &src, sx, sy, keyed).unwrap(),
                    "direct blit refused ({dx}, {dy}, {w}, {h}) keyed={keyed}"
                );

                let mut staged = TestContext::new();
                let dst2 = FrameBuffer::new(&mut staged, 8, 6, 16).unwrap();
                let src2 = FrameBuffer::new(&mut staged, 4, 4, src_bpp).unwrap();
                seed(&mut staged, &dst2, 0x20);
                let sb2 = staged.data_ptr(src2.0.buf).unwrap();
                staged.write_bytes(sb2, &src_bytes).unwrap();
                if keyed {
                    let key = Rgb565Pixel::from_color(Color {
                        a: 0xff,
                        r: 0xff,
                        g: 0,
                        b: 0xff,
                    });
                    staged.write_bytes(sb2 + 2, &key.to_le_bytes()).unwrap();
                }
                let src_image = src2.image(&mut staged).unwrap();
                let mut canvas = dst2.canvas(&mut staged).unwrap();
                if keyed {
                    crate::api::graphics::blit_magenta_keyed(&mut **canvas, dx, dy, w, h, &*src_image, sx, sy);
                } else {
                    let clip = Clip {
                        x: dx,
                        y: dy,
                        width: w as _,
                        height: h as _,
                    };
                    canvas.draw(dx, dy, w as _, h as _, &*src_image, sx, sy, clip);
                }
                canvas.flush().unwrap();

                let mut got = [0u8; 8 * 6 * 2];
                let mut want = [0u8; 8 * 6 * 2];
                direct.read_bytes(dst_base, &mut got).unwrap();
                staged.read_bytes(dst_base, &mut want).unwrap();
                assert_eq!(got, want, "blit ({dx}, {dy}, {w}, {h}) from ({sx}, {sy}) keyed={keyed}");
            }
        }
    }

    /// A depth the direct blit cannot address is refused rather than written
    /// wrong, so the caller still has the canvas to fall back to.
    #[test]
    fn draw_image_direct_refuses_a_depth_it_cannot_address() {
        let mut context = TestContext::new();
        let dst32 = FrameBuffer::new(&mut context, 4, 4, 32).unwrap();
        let src16 = FrameBuffer::new(&mut context, 4, 4, 16).unwrap();
        assert!(!dst32.draw_image_direct(&mut context, 0, 0, 4, 4, &src16, 0, 0, true).unwrap());

        // Keyed wants a 16bpp colour plane, composed wants a 32bpp mask plane;
        // neither takes the other's.
        let dst16 = FrameBuffer::new(&mut context, 4, 4, 16).unwrap();
        assert!(!dst16.draw_image_direct(&mut context, 0, 0, 4, 4, &src16, 0, 0, false).unwrap());
        assert!(!dst16.draw_image_direct(&mut context, 0, 0, 4, 4, &dst32, 0, 0, true).unwrap());
    }

    /// One pixel, written as one pixel, has to land exactly where the canvas
    /// would have put it - and nowhere else.
    #[test]
    fn put_pixel_direct_matches_the_canvas_it_replaces() {
        for (x, y) in [(0i32, 0i32), (3, 2), (7, 5), (-1, 2), (2, -1), (8, 2), (2, 6)] {
            let mut direct = TestContext::new();
            let fb = FrameBuffer::new(&mut direct, 8, 6, 16).unwrap();
            let base = direct.data_ptr(fb.0.buf).unwrap();
            direct.write_bytes(base, &[0x5au8; 8 * 6 * 2]).unwrap();

            let color = Color {
                a: 0xff,
                r: 0x12,
                g: 0x34,
                b: 0x56,
            };
            assert!(fb.put_pixel_direct(&mut direct, x, y, color).unwrap());

            let mut staged = TestContext::new();
            let other = FrameBuffer::new(&mut staged, 8, 6, 16).unwrap();
            staged.write_bytes(base, &[0x5au8; 8 * 6 * 2]).unwrap();
            let mut canvas = other.canvas(&mut staged).unwrap();
            canvas.put_pixel(x, y, color);
            canvas.flush().unwrap();

            let mut got = [0u8; 8 * 6 * 2];
            let mut want = [0u8; 8 * 6 * 2];
            direct.read_bytes(base, &mut got).unwrap();
            staged.read_bytes(base, &mut want).unwrap();
            assert_eq!(got, want, "pixel ({x}, {y})");
        }
    }

    /// A line along an axis, written as a run, has to land exactly where the
    /// canvas's Bresenham walk put it - including when it hangs off an edge,
    /// runs backwards, or is a single point.
    #[test]
    fn an_axis_aligned_line_written_as_a_run_matches_the_canvas() {
        for (x1, y1, x2, y2) in [
            (1i32, 2i32, 6i32, 2i32),
            (6, 2, 1, 2),
            (3, 0, 3, 5),
            (3, 5, 3, 0),
            (-3, 2, 4, 2),
            (2, -4, 2, 3),
            (4, 1, 20, 1),
            (1, 1, 1, 20),
            (3, 3, 3, 3),
            (-5, -5, -5, 2),
            (0, 7, 7, 7),
        ] {
            let color = Color {
                a: 0xff,
                r: 0x12,
                g: 0x34,
                b: 0x56,
            };

            let mut direct = TestContext::new();
            let fb = FrameBuffer::new(&mut direct, 8, 6, 16).unwrap();
            let base = direct.data_ptr(fb.0.buf).unwrap();
            direct.write_bytes(base, &[0x5au8; 8 * 6 * 2]).unwrap();

            let (left, top) = (x1.min(x2), y1.min(y2));
            let width = (x1.max(x2) - left + 1) as u32;
            let height = (y1.max(y2) - top + 1) as u32;
            assert!(fb.fill_rect_direct(&mut direct, left, top, width, height, color).unwrap());

            let mut staged = TestContext::new();
            let other = FrameBuffer::new(&mut staged, 8, 6, 16).unwrap();
            staged.write_bytes(base, &[0x5au8; 8 * 6 * 2]).unwrap();
            let mut canvas = other.canvas(&mut staged).unwrap();
            let clip = Clip {
                x: 0,
                y: 0,
                width: 8,
                height: 6,
            };
            canvas.draw_line(x1, y1, x2, y2, color, clip);
            canvas.flush().unwrap();

            let mut got = [0u8; 8 * 6 * 2];
            let mut want = [0u8; 8 * 6 * 2];
            direct.read_bytes(base, &mut got).unwrap();
            staged.read_bytes(base, &mut want).unwrap();
            assert_eq!(got, want, "line ({x1}, {y1}) -> ({x2}, {y2})");
        }
    }

    #[test]
    fn put_pixel_direct_refuses_a_depth_it_cannot_pack() {
        let mut context = TestContext::new();
        let fb = FrameBuffer::new(&mut context, 4, 2, 8).unwrap();

        let color = Color { a: 0xff, r: 0, g: 0, b: 0 };
        assert!(!fb.put_pixel_direct(&mut context, 0, 0, color).unwrap());
    }

    /// Reading a rectangle row by row and writing it back has to leave the
    /// surface exactly as reading the whole thing and putting each pixel back
    /// would - including for a rectangle that hangs off an edge.
    #[test]
    fn a_rect_read_and_written_back_is_the_pixels_it_covers() {
        for (x, y, w, h) in [(1i32, 1i32, 2i32, 2i32), (-2, 1, 4, 2), (6, 0, 4, 3), (1, -3, 2, 5), (9, 9, 2, 2)] {
            let mut context = TestContext::new();
            let fb = FrameBuffer::new(&mut context, 8, 6, 16).unwrap();
            let base = context.data_ptr(fb.0.buf).unwrap();

            // A surface where every pixel is different, so a misplaced row or
            // column cannot pass unnoticed.
            let original: Vec<u8> = (0..8u16 * 6).flat_map(|pixel| pixel.to_le_bytes()).collect();
            context.write_bytes(base, &original).unwrap();

            let (left, top, cols, rows, pixels) = fb.read_rect_rgb565(&context, x, y, w, h).unwrap().unwrap();

            // What it read is what is there.
            for row in 0..rows {
                for col in 0..cols {
                    let expected = (top + row) as u16 * 8 + (left + col) as u16;
                    assert_eq!(pixels[(row * cols + col) as usize], expected, "rect ({x}, {y}, {w}, {h})");
                }
            }

            // And writing it back changes nothing at all.
            fb.write_rect_rgb565(&mut context, left, top, cols, rows, &pixels).unwrap();

            let mut out = vec![0u8; original.len()];
            context.read_bytes(base, &mut out).unwrap();
            assert_eq!(out, original, "rect ({x}, {y}, {w}, {h})");
        }
    }

    #[test]
    fn a_rect_read_refuses_a_depth_it_cannot_pack() {
        let mut context = TestContext::new();
        let fb = FrameBuffer::new(&mut context, 4, 2, 32).unwrap();

        assert!(fb.read_rect_rgb565(&context, 0, 0, 4, 2).unwrap().is_none());
    }

    #[test]
    fn fill_rect_direct_refuses_a_depth_it_cannot_pack() {
        let mut context = TestContext::new();
        let fb = FrameBuffer::new(&mut context, 4, 2, 8).unwrap();

        let color = Color { a: 0xff, r: 0, g: 0, b: 0 };
        assert!(!fb.fill_rect_direct(&mut context, 0, 0, 4, 2, color).unwrap());
    }

    #[test]
    fn write_diff_restages_whole_pixels() {
        let mut context = TestContext::new();
        let fb = FrameBuffer::new(&mut context, 4, 1, 16).unwrap();
        let base = context.data_ptr(fb.0.buf).unwrap();

        let snapshot = [0x11u8; 8];
        context.write_bytes(base, &snapshot).unwrap();

        // Our primitive changed only the low byte of pixel 1 (bytes 2..4).
        let mut drawn = snapshot;
        drawn[2] = 0x77;

        fb.write_diff(&mut context, &snapshot, &drawn).unwrap();

        let mut out = [0u8; 8];
        context.read_bytes(base, &mut out).unwrap();
        // Both bytes of pixel 1 were written (the high byte re-stamped from what
        // we drew), so the pixel is a complete, uncorrupted value.
        assert_eq!(&out[2..4], &[0x77, 0x11]);
        // Neighbouring pixels untouched.
        assert_eq!(&out[0..2], &[0x11, 0x11]);
        assert_eq!(&out[4..6], &[0x11, 0x11]);
    }

    #[test]
    fn test_new_overflow_returns_error() {
        let mut context = TestContext::new();

        assert!(matches!(
            FrameBuffer::new(&mut context, 0x10000, 0x10000, 32),
            Err(WieError::AllocationFailure)
        ));
    }

    #[test]
    fn test_new_over_heap_limit_returns_error() {
        let mut context = TestContext::new();

        assert!(matches!(
            FrameBuffer::new(&mut context, 0x4000, 0x4000, 32),
            Err(WieError::AllocationFailure)
        ));
    }

    #[test]
    fn test_new_zero_height_bpl_overflow_returns_error() {
        let mut context = TestContext::new();

        assert!(matches!(
            FrameBuffer::new(&mut context, 0xffff_ffff, 0, 32),
            Err(WieError::AllocationFailure)
        ));
    }

    #[test]
    fn test_new_normal_size_ok() {
        let mut context = TestContext::new();

        let framebuffer = FrameBuffer::new(&mut context, 100, 100, 16).unwrap();
        assert_eq!(framebuffer.0.width, 100);
        assert_eq!(framebuffer.0.height, 100);
        assert_eq!(framebuffer.0.bpl, 200);
        assert_eq!(framebuffer.0.bpp, 16);
        assert_eq!(framebuffer.data(&context).unwrap().len(), 20000);
    }
}
