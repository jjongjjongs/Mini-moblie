mod bitmap_font;
mod framebuffer;
mod grp_context;
mod image;
mod pixel_op;

use core::mem::size_of;

use bytemuck::pod_collect_to_vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use alloc::{boxed::Box, string::String, vec, vec::Vec};

use wie_backend::{
    Event, System,
    canvas::{Canvas, Clip, Color, Image, PixelType, Rgb8Pixel, Rgb332Pixel, Rgb565Pixel, TextAlignment, VecImageBuffer, string_width_px},
};
use wie_util::{ByteRead, ByteWrite, Result, WieError, read_generic, read_null_terminated_string_bytes, write_generic};

use wipi_types::wipic::{WIPICDisplayInfo, WIPICFramebuffer, WIPICImage, WIPICIndirectPtr, WIPICWord};

use crate::context::WIPICContext;

use self::framebuffer::buffer_size;

use self::{
    bitmap_font::BitmapFace,
    framebuffer::FrameBuffer,
    grp_context::{BUILT_IN_XOR, WIPICGraphicsContext},
    image::create_wipi_image,
};

pub use self::grp_context::{ContextLayout, WIPICGraphicsContextIdx};

pub use self::bitmap_font::{clear as clear_bios_font, install_from_bios as install_bios_font};

pub const FRAMEBUFFER_DEPTH: u32 = 16; // XXX hardcode to 16bpp as some game requires 16bpp framebuffer
const SCREEN_FRAMEBUFFER_PTR: u32 = 0x7fff1000;
/// Guest word holding the height of the handset's status strip (the WIPI
/// "annunciator"), which sits above the drawing area a title is given. Zero
/// unless the platform stores one, and nothing below changes while it is zero.
pub const ANNUNCIATOR_ROWS_PTR: u32 = 0x7fff2000;

/// Read a WIPI-C string. `length == -1` means NUL-terminated; `length > 0`
/// reads exactly that many bytes; `length == 0` and other negatives yield
/// an empty string.
///
/// The bytes are EUC-KR, which is what a Korean handset's toolchain put in the
/// binary. Reading them as UTF-8 turns every Hangul syllable into U+FFFD, and
/// the font has no glyph for that, so a title's text silently drew nothing at
/// all - which is what dialogue boxes with no words in them were.
///
/// A null pointer is the empty string, not a fault. A handset reads the byte at
/// address zero like any other and finds the zero that terminates a string
/// there, so a title that measures or draws a label it has not set yet gets a
/// width of nothing and draws nothing. 드래곤로드EX's loading screen asks for
/// the width of one on the frame it starts its intro music, and this runtime
/// maps nothing at zero, so the read faulted and took the title with it.
fn read_wipi_string(context: &mut dyn WIPICContext, ptr: WIPICWord, length: i32) -> Result<String> {
    if ptr == 0 {
        return Ok(String::new());
    }

    let bytes = if length > 0 {
        let mut buf = vec![0u8; length as usize];
        context.read_bytes(ptr, &mut buf)?;
        buf
    } else if length == -1 {
        read_null_terminated_string_bytes(context, ptr)?
    } else {
        Vec::new()
    };

    Ok(encoding_rs::EUC_KR.decode(&bytes).0.into_owned())
}

pub async fn get_screen_framebuffer(context: &mut dyn WIPICContext, a0: WIPICWord) -> Result<WIPICIndirectPtr> {
    tracing::debug!("MC_grpGetScreenFrameBuffer({a0:#x})");

    let framebuffer_ptr: u32 = read_generic(context, SCREEN_FRAMEBUFFER_PTR)?;
    if framebuffer_ptr != 0 {
        return Ok(WIPICIndirectPtr(framebuffer_ptr));
    }

    // A title asking for the screen it has not been given yet is a title
    // starting, and the surfaces [`trace_offscreen_surfaces`] knows about
    // belong to the one before it. They live in a static, so nothing else
    // forgets them; left behind, they name allocations this title now owns for
    // something else.
    OFFSCREEN_SURFACES.lock().clear();

    // And the panel, for the same reason: what the last title left on it is not
    // under this one's first frame. Whether the title flushes is the last
    // title's answer too.
    forget_panel();
    TITLE_FLUSHES_LCD.store(false, Ordering::Relaxed);

    let (width, height) = {
        let platform = context.system().platform();
        let screen = platform.screen();
        (screen.width(), screen.height())
    };

    // Guard the screen surface too: a title blits its scene straight into it
    // with the default (unclamped) clip and overruns the bottom edge, and the
    // list-allocator header of the very next block sits 4 bytes past this
    // buffer's end - so an unpadded screen buffer lets the overdraw corrupt the
    // heap. `new_screen_surface` pads it the same way `new_guarded_surface`
    // does, and also splits the panel from the drawing area.
    let framebuffer = new_screen_surface(context, width, height)?;

    let memory = context.alloc(size_of::<WIPICFramebuffer>() as WIPICWord)?;
    write_generic(context, context.data_ptr(memory)?, framebuffer.0)?;
    write_generic(context, SCREEN_FRAMEBUFFER_PTR, memory.0)?;

    Ok(memory)
}

/// The clip a context carries when nobody has given it one: the whole plane.
///
/// `MC_grpInitContext` (@0x1abc0c) plants this rather than zeroing, and it is
/// therefore also what a caller means by handing `MC_grpSetContext` no rectangle
/// at all.
const WHOLE_PLANE_CLIP: [u32; 4] = [0, 0, 0x7fff, 0x7fff];

pub async fn init_context(context: &mut dyn WIPICContext, p_grp_ctx: WIPICWord) -> Result<()> {
    tracing::debug!("MC_grpInitContext({p_grp_ctx:#x})");

    let layout = context.graphics_context_layout();

    init_context_in(context, layout, p_grp_ctx)
}

/// `MC_grpInitContext` against guest memory alone.
///
/// The three context calls read and write one struct in the title's own memory
/// and touch nothing else, which is what lets the emulator answer them on its
/// synchronous fast path without a second copy of the rules living there. See
/// `wie_lgt::runtime::wipi_c::try_fast_wipic_getter`.
/// A context read out of guest memory and put into this runtime's field order.
///
/// Everything below reads a context through here rather than through
/// `read_generic`, because the two handsets do not keep the same word first -
/// see [`ContextLayout`].
fn read_context<M>(memory: &M, layout: ContextLayout, p_grp_ctx: WIPICWord) -> Result<WIPICGraphicsContext>
where
    M: ByteRead + ?Sized,
{
    let at = layout.offsets();
    let word = |offset: WIPICWord| read_generic::<WIPICWord, _>(memory, p_grp_ctx + offset);

    Ok(WIPICGraphicsContext {
        clip: [word(at.clip)?, word(at.clip + 4)?, word(at.clip + 8)?, word(at.clip + 12)?],
        fgpxl: word(at.fgpxl)?,
        bgpxl: word(at.bgpxl)?,
        alpha: word(at.alpha)?,
        transparent: word(at.transparent)?,
        param1: word(at.param1)?,
        font: word(at.font)?,
        style: word(at.style)?,
        pixel_op_func_ptr: word(at.pixel_op_func_ptr)?,
        offset: [word(at.offset)?, word(at.offset + 4)?],
    })
}

/// A context written back into the handset's own words - see [`read_context`].
///
/// Only the words the layout names are written. A word the handset keeps for
/// itself - KTF's leading one - is left as the title left it, because nothing
/// here knows what it is for.
fn write_context<M>(memory: &mut M, layout: ContextLayout, p_grp_ctx: WIPICWord, grp_ctx: WIPICGraphicsContext) -> Result<()>
where
    M: ByteWrite + ?Sized,
{
    let at = layout.offsets();
    let mut word = |offset: WIPICWord, value: WIPICWord| write_generic(memory, p_grp_ctx + offset, value);

    for (index, corner) in grp_ctx.clip.iter().enumerate() {
        word(at.clip + 4 * index as WIPICWord, *corner)?;
    }
    word(at.fgpxl, grp_ctx.fgpxl)?;
    word(at.bgpxl, grp_ctx.bgpxl)?;
    word(at.alpha, grp_ctx.alpha)?;
    word(at.transparent, grp_ctx.transparent)?;
    word(at.param1, grp_ctx.param1)?;
    word(at.font, grp_ctx.font)?;
    word(at.style, grp_ctx.style)?;
    word(at.pixel_op_func_ptr, grp_ctx.pixel_op_func_ptr)?;
    word(at.offset, grp_ctx.offset[0])?;
    word(at.offset + 4, grp_ctx.offset[1])?;

    Ok(())
}

pub fn init_context_in<M>(memory: &mut M, layout: ContextLayout, p_grp_ctx: WIPICWord) -> Result<()>
where
    M: ByteRead + ByteWrite + ?Sized,
{
    // Reference MC_grpInitContext (@0x1abc0c) does not zero the whole context;
    // it plants non-zero defaults that drawing then relies on when the game
    // never calls SetContext for a given field. Porting them keeps our output
    // consistent with the firmware:
    //   clip   = whole plane (0,0)-(0x7fff,0x7fff)
    //   bgpxl  = 0x00ffffff (opaque white)
    //   alpha  = 0xff       (fully opaque)
    //   param1 = 0xff
    //   font   = MC_grpGetFont(0,0,0)  (the 12px default face)
    // Everything else (fg, transparent, pixelop, style, offset) stays zero.
    let grp_ctx = WIPICGraphicsContext {
        clip: WHOLE_PLANE_CLIP,
        bgpxl: 0x00ff_ffff,
        alpha: 0xff,
        param1: 0xff,
        font: font_size_px(0) as WIPICWord,
        ..Default::default()
    };
    write_context(memory, layout, p_grp_ctx, grp_ctx)?;
    Ok(())
}

pub async fn set_context(context: &mut dyn WIPICContext, p_grp_ctx: WIPICWord, op: WIPICGraphicsContextIdx, pv: WIPICWord) -> Result<()> {
    tracing::trace!("MC_grpSetContext({p_grp_ctx:#x}, {op:?}, {pv:#x})");

    let layout = context.graphics_context_layout();

    set_context_in(context, layout, p_grp_ctx, op, pv)
}

/// `MC_grpSetContext` against guest memory alone - see [`init_context_in`].
pub fn set_context_in<M>(memory: &mut M, layout: ContextLayout, p_grp_ctx: WIPICWord, op: WIPICGraphicsContextIdx, pv: WIPICWord) -> Result<()>
where
    M: ByteRead + ByteWrite + ?Sized,
{
    let mut grp_ctx = read_context(memory, layout, p_grp_ctx)?;
    match op {
        WIPICGraphicsContextIdx::ClipIdx => {
            // The clip rectangle is passed as four 32-bit words (x1, y1, x2, y2),
            // not four 16-bit ones - reading it as `[u16; 4]` took only the low
            // halves of x1/y1 as the whole rect and dropped x2/y2 entirely, so a
            // title that saved the clip with GetContext and restored it here got a
            // degenerate rectangle back and every later blit clipped to nothing
            // (MapleStory 도적편's sprites vanished). The reference stores the
            // bottom-right corner decremented; GetContext re-adds the 1.
            //
            // No rectangle at all clears the clip rather than faulting. A title's
            // own clip setter works out the rectangle it wants, compares it against
            // the surface's width and height, and when the two are equal - the
            // whole surface - calls this with the array argument zeroed instead of
            // with an array. Reading sixteen bytes at address zero for that is this
            // side inventing a requirement the caller never had: what it asked for
            // is the clip a context has before anyone sets one.
            if pv == 0 {
                grp_ctx.clip = WHOLE_PLANE_CLIP;
            } else {
                let x1: u32 = read_generic(memory, pv)?;
                let y1: u32 = read_generic(memory, pv + 4)?;
                let x2: u32 = read_generic(memory, pv + 8)?;
                let y2: u32 = read_generic(memory, pv + 12)?;
                grp_ctx.clip = [x1, y1, x2.wrapping_sub(1), y2.wrapping_sub(1)];
            }
        }
        WIPICGraphicsContextIdx::FgPixelIdx => {
            grp_ctx.fgpxl = pv as _;
        }
        WIPICGraphicsContextIdx::BgPixelIdx => {
            grp_ctx.bgpxl = pv as _;
        }
        // Only where the handset has a word for it. KTF does, and a title's
        // own blitter keys against what it reads back from there; LGT has none
        // and the reference drops the call - see [`ContextOffsets`].
        WIPICGraphicsContextIdx::TransPixelIdx => {
            if layout.offsets().keeps_transparent {
                grp_ctx.transparent = pv;
            }
        }
        // The reference ignores an alpha outside 0..=0xff rather than storing it.
        WIPICGraphicsContextIdx::AlphaIdx => {
            if pv <= 0xff {
                grp_ctx.alpha = pv;
            }
        }
        WIPICGraphicsContextIdx::PixelopIdx => {
            grp_ctx.pixel_op_func_ptr = pv;
        }
        WIPICGraphicsContextIdx::PixelParam1Idx => {
            grp_ctx.param1 = pv;
        }
        WIPICGraphicsContextIdx::FontIdx => {
            grp_ctx.font = pv;
        }
        WIPICGraphicsContextIdx::StyleIdx => {
            grp_ctx.style = pv;
        }
        // XOR mode is not a flag of its own: the reference drives the pixel-op
        // slot from it, zeroing the alpha and installing its built-in XOR
        // operation, and turning it off clears the slot again. We have no guest
        // address for that operation, so `BUILT_IN_XOR` stands in the slot -
        // recognised where an operation is read, and answered as no operation
        // to a title that reads the slot back, which is what it saw before.
        WIPICGraphicsContextIdx::XorModeIdx => {
            if pv == 0 {
                grp_ctx.pixel_op_func_ptr = 0;
            } else {
                grp_ctx.alpha = 0;
                grp_ctx.pixel_op_func_ptr = BUILT_IN_XOR;
            }
        }
        WIPICGraphicsContextIdx::OffsetIdx => {
            // Same 32-bit-word pair as the clip corners, and the counterpart to
            // GetContext's `OffsetIdx`, which writes two 32-bit words back.
            let x: u32 = read_generic(memory, pv)?;
            let y: u32 = read_generic(memory, pv + 4)?;
            grp_ctx.offset = [x, y];
        }
        _ => {
            tracing::warn!("MC_grpSetContext({p_grp_ctx:#x}, {op:?}, {pv:#x}): ignoring invalid op");
        }
    }
    write_context(memory, layout, p_grp_ctx, grp_ctx)?;

    Ok(())
}

/// `MC_grpGetContext(p_grp_ctx, op, out_ptr)` - the read counterpart to
/// `MC_grpSetContext`. A clet's blitter reads the live drawing state back
/// (foreground colour, alpha, font, clip, offset...) so it can save and restore
/// it around each primitive. Left a stub returning nothing, it zeroed the game's
/// saved context, and the restore then corrupted every following draw - Demon
/// Hunter smeared each screen over the last one and spun redrawing.
///
/// Behaviour mirrors `liblgt_system.so`'s `MC_grpGetContext`: the value is
/// written *through* `out_ptr` (unlike `SetContext`, which passes scalars by
/// value), the call is a no-op when either pointer is null, `TransPixelIdx`
/// reads nothing back, and the clip is reported as `(x1, y1, x2 + 1, y2 + 1)`
/// (the reference stores the bottom-right corner decremented and re-adds it).
pub async fn get_context(context: &mut dyn WIPICContext, p_grp_ctx: WIPICWord, op: WIPICGraphicsContextIdx, out_ptr: WIPICWord) -> Result<()> {
    tracing::trace!("MC_grpGetContext({p_grp_ctx:#x}, {op:?}, {out_ptr:#x})");

    let layout = context.graphics_context_layout();

    get_context_in(context, layout, p_grp_ctx, op, out_ptr)
}

/// `MC_grpGetContext` against guest memory alone - see [`init_context_in`].
pub fn get_context_in<M>(memory: &mut M, layout: ContextLayout, p_grp_ctx: WIPICWord, op: WIPICGraphicsContextIdx, out_ptr: WIPICWord) -> Result<()>
where
    M: ByteRead + ByteWrite + ?Sized,
{
    if p_grp_ctx == 0 || out_ptr == 0 {
        return Ok(());
    }

    let grp_ctx = read_context(memory, layout, p_grp_ctx)?;
    match op {
        WIPICGraphicsContextIdx::ClipIdx => {
            let clip = grp_ctx.clip;
            write_generic(memory, out_ptr, clip[0])?;
            write_generic(memory, out_ptr + 4, clip[1])?;
            write_generic(memory, out_ptr + 8, clip[2].wrapping_add(1))?;
            write_generic(memory, out_ptr + 12, clip[3].wrapping_add(1))?;
        }
        WIPICGraphicsContextIdx::FgPixelIdx => write_generic(memory, out_ptr, grp_ctx.fgpxl)?,
        WIPICGraphicsContextIdx::BgPixelIdx => write_generic(memory, out_ptr, grp_ctx.bgpxl)?,
        // Read back where the handset keeps one - see the setter.
        WIPICGraphicsContextIdx::TransPixelIdx => {
            if layout.offsets().keeps_transparent {
                write_generic(memory, out_ptr, grp_ctx.transparent)?
            }
        }
        WIPICGraphicsContextIdx::AlphaIdx => write_generic(memory, out_ptr, grp_ctx.alpha)?,
        // The stand-in for XOR mode is ours, not an address: a title reading
        // the slot is told there is no operation, the same as before.
        WIPICGraphicsContextIdx::PixelopIdx => {
            let function = grp_ctx.pixel_op_func_ptr;
            write_generic(memory, out_ptr, if function == BUILT_IN_XOR { 0 } else { function })?
        }
        WIPICGraphicsContextIdx::PixelParam1Idx => write_generic(memory, out_ptr, grp_ctx.param1)?,
        WIPICGraphicsContextIdx::FontIdx => write_generic(memory, out_ptr, grp_ctx.font)?,
        WIPICGraphicsContextIdx::StyleIdx => write_generic(memory, out_ptr, grp_ctx.style)?,
        WIPICGraphicsContextIdx::XorModeIdx => write_generic(memory, out_ptr, u32::from(grp_ctx.pixel_op_func_ptr == BUILT_IN_XOR))?,
        WIPICGraphicsContextIdx::OffsetIdx => {
            let offset = grp_ctx.offset;
            write_generic(memory, out_ptr, offset[0])?;
            write_generic(memory, out_ptr + 4, offset[1])?;
        }
        _ => {
            tracing::warn!("MC_grpGetContext({p_grp_ctx:#x}, {op:?}, {out_ptr:#x}): unsupported op");
        }
    }

    Ok(())
}

/// The colour a primitive paints with: the context's foreground pixel, carrying
/// the context's alpha so a title that asked for a translucent shape gets one.
///
/// 테일즈위버 이스핀편 (`00026308`) draws every one of its panels this way. A
/// two-call helper of its own at `0x160c` sets the foreground pixel and an alpha
/// together - `MC_grpSetContext(gc, 1, pixel)` then `MC_grpSetContext(gc, 4,
/// alpha)`, with alphas from `0x50` to `0xc8` - and fills a rectangle with it;
/// thirty of its thirty-four calls to that helper are followed by
/// `MC_grpFillRect`. Painted opaque, its menu panels came out solid white and
/// its in-game panels solid black, and the white text it then drew on them
/// disappeared into the fill.
///
/// XOR mode is the exception: the reference zeroes the alpha when it turns XOR
/// on, so reading it there would paint nothing at all.
fn context_color(framebuffer: &FrameBuffer, gctx: &WIPICGraphicsContext) -> Color {
    let mut color = framebuffer.pixel_to_color(gctx.fgpxl);

    if gctx.pixel_op_func_ptr != BUILT_IN_XOR && gctx.alpha <= 0xff {
        color.a = gctx.alpha as u8;
    }

    color
}

/// Where a primitive's coordinates land, once the context's drawing offset is
/// added.
///
/// Op 10 is not a note a title leaves for itself: the reference installs it on
/// the graphics object it converts the context into -
/// `wipic_grpContext_to_dgraphics` (@0x1aa2e8) ends by handing `[gc+0x30]` and
/// `[gc+0x34]` to the origin setter at `0x1979e4` - so every primitive drawn
/// through that context is drawn relative to it. Stored and never added, a
/// title that moves its origin instead of moving every coordinate drew its
/// whole screen in the corner: 액션히어로3D sets one 600 times on the way into
/// its menu, and its menu list came out at the top left of the panel it
/// belongs in.
///
/// The clip is not moved with it. The reference sets the clip on the graphics
/// object from the context's own rectangle *before* it sets the origin, so the
/// rectangle stays in the surface's coordinates whatever the origin is.
fn context_offset(gctx: &WIPICGraphicsContext) -> (i32, i32) {
    (gctx.offset[0] as i32, gctx.offset[1] as i32)
}

/// The rectangle a context's clip lets through.
///
/// The reference stores the bottom-right corner decremented - `MC_grpGetContext`
/// re-adds the 1 - so the corner is inside the rectangle and the width is
/// `x2 - x1 + 1`. A corner behind its own origin lets nothing through.
///
/// Every `MC_grp*` call that takes a context draws through this. A clet does
/// not always pass the rectangle it wants as the call's own arguments: it sets
/// the clip to the cell it wants and hands the whole sheet to `MC_grpDrawImage`,
/// which is how 짜요짜요타이쿤3 picks one label out of the 2x9 grid its menu
/// is stored as. Ignored, all nine labels landed on the screen at once, five
/// times over.
fn context_clip(gctx: &WIPICGraphicsContext) -> Clip {
    let (x1, y1) = (gctx.clip[0] as i32, gctx.clip[1] as i32);
    let (x2, y2) = (gctx.clip[2] as i32, gctx.clip[3] as i32);

    Clip {
        x: x1,
        y: y1,
        width: (x2 as i64 - x1 as i64 + 1).clamp(0, u32::MAX as i64) as u32,
        height: (y2 as i64 - y1 as i64 + 1).clamp(0, u32::MAX as i64) as u32,
    }
}

/// A rectangle cut down to what a clip allows, or `None` when nothing is left.
fn clipped_rect(clip: &Clip, x: i32, y: i32, w: i32, h: i32) -> Option<(i32, i32, i32, i32)> {
    let left = x.max(clip.x);
    let top = y.max(clip.y);
    let right = (x as i64 + w as i64).min(clip.x as i64 + clip.width as i64);
    let bottom = (y as i64 + h as i64).min(clip.y as i64 + clip.height as i64);

    let width = right - left as i64;
    let height = bottom - top as i64;
    if width <= 0 || height <= 0 {
        return None;
    }

    Some((left, top, width as i32, height as i32))
}

/// A blit cut down to what a clip allows: the destination corner, the size and
/// the source corner it now starts from.
///
/// Nothing here scales, so an edge the clip moves in by one moves the source
/// edge with it and the pixels that survive are the ones that were going to
/// land inside the rectangle anyway.
#[allow(clippy::too_many_arguments)]
fn clipped_blit(clip: &Clip, dx: i32, dy: i32, w: i32, h: i32, sx: i32, sy: i32) -> Option<(i32, i32, i32, i32, i32, i32)> {
    let (left, top, width, height) = clipped_rect(clip, dx, dy, w, h)?;

    Some((left, top, width, height, sx + (left - dx), sy + (top - dy)))
}

pub async fn put_pixel(context: &mut dyn WIPICContext, dst_fb: WIPICIndirectPtr, x: i32, y: i32, p_gctx: WIPICWord) -> Result<()> {
    tracing::debug!("MC_grpPutPixel({:#x}, {x}, {y}, {p_gctx:?})", dst_fb.0);

    let framebuffer = FrameBuffer(read_generic(context, context.data_ptr(dst_fb)?)?);
    let gctx = read_context(context, context.graphics_context_layout(), p_gctx)?;
    let (offset_x, offset_y) = context_offset(&gctx);
    let (x, y) = (x + offset_x, y + offset_y);

    if !context_clip(&gctx).allows(x, y) {
        return Ok(());
    }

    let color = context_color(&framebuffer, &gctx);

    // One pixel is two bytes; staging the surface to move them is what held a
    // handset's AP at its top clock through 드래곤하트2's menus. A colour that
    // is not opaque is composed with what is under it, which is the canvas
    // path's business. See `FrameBuffer::put_pixel_direct`.
    if color.a == 0xff && framebuffer.put_pixel_direct(context, x, y, color)? {
        return Ok(());
    }

    let mut canvas = framebuffer.canvas(context)?;
    canvas.put_pixel(x as _, y as _, color);
    canvas.flush()?;

    Ok(())
}

pub async fn fill_rect(context: &mut dyn WIPICContext, dst_fb: WIPICIndirectPtr, x: i32, y: i32, w: i32, h: i32, p_gctx: WIPICWord) -> Result<()> {
    tracing::debug!("MC_grpFillRect({:#x}, {x}, {y}, {w}, {h}, {p_gctx:#x})", dst_fb.0);

    if w <= 0 || h <= 0 {
        return Ok(());
    }

    let framebuffer = FrameBuffer(read_generic(context, context.data_ptr(dst_fb)?)?);
    let gctx = read_context(context, context.graphics_context_layout(), p_gctx)?;
    let (offset_x, offset_y) = context_offset(&gctx);
    let (x, y) = (x + offset_x, y + offset_y);

    // Only the part of the rectangle the context's clip allows, so every path
    // below - the operation, the direct write and the canvas - covers the same
    // pixels.
    let Some((x, y, w, h)) = clipped_rect(&context_clip(&gctx), x, y, w, h) else {
        return Ok(());
    };

    let color = context_color(&framebuffer, &gctx);

    // A fill goes through the title's own operation too - 드래곤하트2 lays two
    // hundred of them through a live one in a single capture - so the colour
    // meets what is already there rather than covering it.
    if let Some((kind, function)) = pixel_op::of_context(context, gctx.pixel_op_func_ptr, gctx.param1).await? {
        let source = Rgb565Pixel::from_color(color);
        let source_first = context.pixel_op_takes_source_first();

        // Read and write back only the rectangle. The two canvas round trips
        // this used to make - one to look at what was under the fill, one to
        // put the result back - staged the whole surface twice for a rectangle
        // that is usually a few pixels across, and 드래곤하트2 lays two
        // thousand of these a second in its menus. See
        // `FrameBuffer::read_rect_rgb565`.
        if let Some((left, top, cols, rows, mut pixels)) = framebuffer.read_rect_rgb565(context, x, y, w, h)? {
            for destination in pixels.iter_mut() {
                *destination = match pixel_op::apply(kind, *destination, source, source_first) {
                    Some(result) => result,
                    None => {
                        let (a, b) = pixel_op::arguments(source_first, *destination, source);

                        context.call_function(function, &[a as WIPICWord, b as WIPICWord, gctx.param1]).await? as u16
                    }
                };
            }

            framebuffer.write_rect_rgb565(context, left, top, cols, rows, &pixels)?;

            return Ok(());
        }

        let existing = {
            let canvas = framebuffer.canvas(context)?;
            let surface = canvas.image();
            let (width, height) = (surface.width() as i32, surface.height() as i32);

            let mut existing = Vec::new();
            for row in y..y + h {
                for col in x..x + w {
                    if col < 0 || col >= width || row < 0 || row >= height {
                        continue;
                    }

                    existing.push((col, row, Rgb565Pixel::from_color(surface.get_pixel(col, row))));
                }
            }

            existing
        };

        let mut filled = Vec::with_capacity(existing.len());
        for &(col, row, destination) in &existing {
            let result = match pixel_op::apply(kind, destination, source, source_first) {
                Some(result) => result,
                None => {
                    let (a, b) = pixel_op::arguments(source_first, destination, source);

                    context.call_function(function, &[a as WIPICWord, b as WIPICWord, gctx.param1]).await? as u16
                }
            };

            filled.push((col, row, result));
        }

        let mut canvas = framebuffer.canvas(context)?;
        for (col, row, pixel) in filled {
            canvas.put_pixel(col, row, Rgb565Pixel::to_color(pixel));
        }
        canvas.flush()?;

        return Ok(());
    }

    // A solid, fully opaque rectangle is just bytes, and the clip this passes
    // is the rectangle itself, so nothing about the result needs the surface
    // staged: write the rows it covers straight into the framebuffer. A title
    // that draws by the pixel - 엑시온2 fills its scene and its minimap 2x2 at
    // a time, tens of thousands of times a frame - was paying two full-surface
    // copies out and a whole-surface diff back for each of them. A colour that
    // is not opaque is composed with what is under it, which is the canvas
    // path's business.
    if color.a == 0xff && framebuffer.fill_rect_direct(context, x, y, w as _, h as _, color)? {
        return Ok(());
    }

    let mut canvas = framebuffer.canvas(context)?;

    let clip = Clip {
        x: x as _,
        y: y as _,
        width: w as _,
        height: h as _,
    };

    canvas.fill_rect(x as _, y as _, w as _, h as _, color, clip);
    canvas.flush()?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn draw_arc(
    context: &mut dyn WIPICContext,
    dst: WIPICIndirectPtr,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    start_angle: i32,
    end_angle: i32,
    p_gctx: WIPICWord,
) -> Result<()> {
    tracing::debug!("MC_grpDrawArc({:#x}, {x}, {y}, {w}, {h}, {start_angle}, {end_angle}, {p_gctx:#x})", dst.0);

    if dst.0 == 0 || p_gctx == 0 || w <= 0 || h <= 0 {
        return Ok(());
    }

    let framebuffer = FrameBuffer(read_generic(context, context.data_ptr(dst)?)?);
    let gctx = read_context(context, context.graphics_context_layout(), p_gctx)?;
    let (offset_x, offset_y) = context_offset(&gctx);
    let (x, y) = (x + offset_x, y + offset_y);
    let mut canvas = framebuffer.canvas(context)?;

    let clip = Clip {
        x: x as _,
        y: y as _,
        width: w as _,
        height: h as _,
    }
    .intersect(&context_clip(&gctx));

    let color = context_color(&framebuffer, &gctx);
    canvas.draw_arc(
        x as _,
        y as _,
        w as _,
        h as _,
        start_angle,
        end_angle.wrapping_sub(start_angle),
        color,
        clip,
    );
    canvas.flush()?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn fill_arc(
    context: &mut dyn WIPICContext,
    dst: WIPICIndirectPtr,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    start_angle: i32,
    end_angle: i32,
    p_gctx: WIPICWord,
) -> Result<()> {
    tracing::debug!("MC_grpFillArc({:#x}, {x}, {y}, {w}, {h}, {start_angle}, {end_angle}, {p_gctx:#x})", dst.0);

    if dst.0 == 0 || p_gctx == 0 || w <= 0 || h <= 0 {
        return Ok(());
    }

    let framebuffer = FrameBuffer(read_generic(context, context.data_ptr(dst)?)?);
    let gctx = read_context(context, context.graphics_context_layout(), p_gctx)?;
    let (offset_x, offset_y) = context_offset(&gctx);
    let (x, y) = (x + offset_x, y + offset_y);
    let mut canvas = framebuffer.canvas(context)?;

    let clip = Clip {
        x: x as _,
        y: y as _,
        width: w as _,
        height: h as _,
    }
    .intersect(&context_clip(&gctx));

    let color = context_color(&framebuffer, &gctx);
    canvas.fill_arc(
        x as _,
        y as _,
        w as _,
        h as _,
        start_angle,
        end_angle.wrapping_sub(start_angle),
        color,
        clip,
    );
    canvas.flush()?;

    Ok(())
}

/// Reads `n` (x, y) vertices from two parallel `M_Int32` arrays, the way the
/// WIPI polygon calls pass them.
fn read_polygon_points(context: &mut dyn WIPICContext, x_points: WIPICWord, y_points: WIPICWord, n: usize) -> Result<Vec<(i32, i32)>> {
    let mut points = Vec::with_capacity(n);
    for i in 0..n {
        let offset = (i * size_of::<i32>()) as WIPICWord;
        let x: i32 = read_generic(context, x_points + offset)?;
        let y: i32 = read_generic(context, y_points + offset)?;
        points.push((x, y));
    }
    Ok(points)
}

/// The bounding box of a set of points as `(min_x, min_y, max_x, max_y)`. Used
/// as the draw clip so a stray vertex cannot paint outside the shape's extent.
/// `Clip` is neither `Copy` nor `Clone`, so callers rebuild one per draw from
/// these bounds.
fn polygon_bounds(points: &[(i32, i32)]) -> (i32, i32, i32, i32) {
    let min_x = points.iter().map(|p| p.0).min().unwrap_or(0);
    let min_y = points.iter().map(|p| p.1).min().unwrap_or(0);
    let max_x = points.iter().map(|p| p.0).max().unwrap_or(0);
    let max_y = points.iter().map(|p| p.1).max().unwrap_or(0);
    (min_x, min_y, max_x, max_y)
}

fn bounds_clip(bounds: (i32, i32, i32, i32)) -> Clip {
    let (min_x, min_y, max_x, max_y) = bounds;
    Clip {
        x: min_x,
        y: min_y,
        width: (max_x - min_x + 1).max(0) as u32,
        height: (max_y - min_y + 1).max(0) as u32,
    }
}

pub async fn draw_polygon(
    context: &mut dyn WIPICContext,
    dst: WIPICIndirectPtr,
    x_points: WIPICWord,
    y_points: WIPICWord,
    n_points: i32,
    p_gctx: WIPICWord,
) -> Result<()> {
    tracing::debug!("MC_grpDrawPolygon({:#x}, {x_points:#x}, {y_points:#x}, {n_points}, {p_gctx:#x})", dst.0);

    if n_points < 2 || x_points == 0 || y_points == 0 {
        return Ok(());
    }

    let framebuffer = FrameBuffer(read_generic(context, context.data_ptr(dst)?)?);
    let gctx = read_context(context, context.graphics_context_layout(), p_gctx)?;
    let (offset_x, offset_y) = context_offset(&gctx);
    let points = read_polygon_points(context, x_points, y_points, n_points as usize)?
        .into_iter()
        .map(|(x, y)| (x + offset_x, y + offset_y))
        .collect::<Vec<_>>();

    let bounds = polygon_bounds(&points);
    let clip = bounds_clip(bounds).intersect(&context_clip(&gctx));
    let color = context_color(&framebuffer, &gctx);
    let mut canvas = framebuffer.canvas(context)?;

    // Close the outline back to the first vertex, which is what a polygon is.
    for i in 0..points.len() {
        let (x1, y1) = points[i];
        let (x2, y2) = points[(i + 1) % points.len()];
        canvas.draw_line(x1, y1, x2, y2, color, clip);
    }
    canvas.flush()?;

    Ok(())
}

pub async fn fill_polygon(
    context: &mut dyn WIPICContext,
    dst: WIPICIndirectPtr,
    x_points: WIPICWord,
    y_points: WIPICWord,
    n_points: i32,
    p_gctx: WIPICWord,
) -> Result<()> {
    tracing::debug!("MC_grpFillPolygon({:#x}, {x_points:#x}, {y_points:#x}, {n_points}, {p_gctx:#x})", dst.0);

    if n_points < 3 || x_points == 0 || y_points == 0 {
        return Ok(());
    }

    let framebuffer = FrameBuffer(read_generic(context, context.data_ptr(dst)?)?);
    let gctx = read_context(context, context.graphics_context_layout(), p_gctx)?;
    let (offset_x, offset_y) = context_offset(&gctx);
    let points = read_polygon_points(context, x_points, y_points, n_points as usize)?
        .into_iter()
        .map(|(x, y)| (x + offset_x, y + offset_y))
        .collect::<Vec<_>>();

    let bounds = polygon_bounds(&points);
    let clip = bounds_clip(bounds).intersect(&context_clip(&gctx));
    let color = context_color(&framebuffer, &gctx);
    let (min_y, max_y) = (bounds.1, bounds.3);
    let mut canvas = framebuffer.canvas(context)?;

    // Even-odd scanline fill: for each row, gather where the edges cross it,
    // sort, and paint the interior between successive crossing pairs.
    let mut crossings: Vec<i32> = Vec::with_capacity(points.len());
    for y in min_y..=max_y {
        crossings.clear();
        for i in 0..points.len() {
            let (x1, y1) = points[i];
            let (x2, y2) = points[(i + 1) % points.len()];
            // A half-open edge test counts each vertex once, so a scanline
            // passing exactly through a vertex is not filled twice.
            let (lo, hi, xa, xb) = if y1 <= y2 { (y1, y2, x1, x2) } else { (y2, y1, x2, x1) };
            if y >= lo && y < hi {
                let x = xa + (xb - xa) * (y - lo) / (hi - lo);
                crossings.push(x);
            }
        }
        crossings.sort_unstable();
        for &[from, to] in crossings.as_chunks::<2>().0 {
            canvas.draw_line(from, y, to, y, color, clip);
        }
    }
    canvas.flush()?;

    Ok(())
}

/// Which images the platform is still holding a title's encoded bytes for.
///
/// `MC_grpCreateImage` is handed a block the title allocated, and the two
/// titles that say what happens to it next say opposite things. LOA-혼돈의
/// 서곡's loader at `0x123a20` frees the block only on the branch the call
/// failed on, so a success hands it over and nothing would ever give it back -
/// 6645 allocations against 701 frees in one fifty second capture. 겟앰프드
/// frees it itself, on the very next call, after every single create.
///
/// Both are served by remembering which blocks are still the platform's to
/// free. An image is entered here when it is made, and it is forgotten the
/// moment the title frees that block itself. `MC_grpDestroyImage` then gives
/// the block back only if it is still listed - LOA's is, 겟앰프드's is not.
///
/// Freeing it unconditionally is what this did before, and the tolerance for
/// the double free that followed was not enough. The block a title has freed
/// does not stay free: the allocator hands the same address out again, and
/// 겟앰프드 reads its next resource into it and makes its next image from it.
/// The free that came later then released a block that was live, two live
/// objects shared an address, and the title died on a double free of a
/// framebuffer plane several frames afterwards - the 240 tolerated warnings in
/// that run were the harmless half of the same mistake.
///
/// Keyed by the image handle, which `create_image` allocates and
/// `destroy_image` is given, so it names one image for as long as it exists. A
/// handle entered twice without being destroyed replaces its own entry, so an
/// address the allocator reuses cannot make one image answer for another.
/// Remember that `image` is holding `source`, which is the platform's to free.
fn hold_image_source(context: &dyn WIPICContext, image: WIPICWord, source: WIPICWord) {
    if source == 0 {
        return;
    }

    context.kernel_state().lock().image_sources.insert(image, source);
}

/// Take `image`'s source back out, and say whether it was still the platform's.
fn take_image_source(context: &dyn WIPICContext, image: WIPICWord) -> Option<WIPICWord> {
    context.kernel_state().lock().image_sources.remove(&image)
}

/// The title has freed this block itself, so no image may give it back again.
///
/// Called from `MC_knlFree`, which is how a title gives a block back.
pub fn forget_image_source(context: &dyn WIPICContext, source: WIPICWord) {
    if source == 0 {
        return;
    }

    context.kernel_state().lock().image_sources.retain(|_, &mut held| held != source);
}

pub async fn create_image(
    context: &mut dyn WIPICContext,
    ptr_image: WIPICWord,
    image_data: WIPICIndirectPtr,
    offset: u32,
    len: u32,
) -> Result<WIPICWord> {
    tracing::debug!("MC_grpCreateImage({ptr_image:#x}, {:#x}, {offset}, {len})", image_data.0);

    // The source pointer can be one the title read out of never-initialised
    // memory - a NULL/valid handle on the reference's zeroed heap, a stray
    // address here - so a read straight from it faults the whole VM instead of
    // failing the one call. The reference returns "not done" for a source it
    // cannot read; do the same and let the title carry on rather than die.
    let image = match create_wipi_image(context, image_data, offset, len) {
        Ok(image) => image,
        Err(WieError::InvalidMemoryAccess(address)) => {
            tracing::warn!(
                "MC_grpCreateImage: unreadable source {:#x} (+{offset}, faulted at {address:#x}); reporting not-done",
                image_data.0
            );
            return Ok(0); // not MC_GRP_IMAGE_DONE
        }
        Err(other) => return Err(other),
    };

    let memory = context.alloc(size_of::<WIPICImage>() as WIPICWord)?;
    write_generic(context, ptr_image, memory)?;
    write_generic(context, context.data_ptr(memory)?, image)?;

    hold_image_source(context, memory.0, image_data.0);

    Ok(1) // MC_GRP_IMAGE_DONE
}

pub async fn destroy_image(context: &mut dyn WIPICContext, image: WIPICIndirectPtr) -> Result<()> {
    tracing::debug!("MC_grpDestroyImage({:#x})", image.0);

    if image.0 == 0 {
        return Ok(());
    }

    // Free the pixel planes `create_wipi_image` allocated: the colour plane
    // (`img.buf`) always, and the mask plane (`mask.buf`) only when the source
    // carried alpha. Freeing just the WIPICImage struct - as this did before -
    // leaks both planes, and a title that creates and destroys a scratch image
    // every frame (MapleStory 도적편 does this ~100x/frame) then exhausts the
    // heap.
    let wipi_image: WIPICImage = read_generic(context, context.data_ptr(image)?)?;
    if wipi_image.img.buf.0 != 0 {
        context.free(wipi_image.img.buf)?;
    }
    if wipi_image.mask.buf.0 != 0 {
        context.free(wipi_image.mask.buf)?;
    }

    // And the encoded bytes the title handed `MC_grpCreateImage`, but only
    // while they are still the platform's to give back. See [`IMAGE_SOURCES`]:
    // a title that has freed the block itself has been taken off that list, and
    // the address it had is by now somebody else's.
    if let Some(source) = take_image_source(context, image.0)
        && source != 0
        && let Err(error) = context.free(WIPICIndirectPtr(source))
    {
        tracing::warn!("MC_grpDestroyImage: could not free the source buffer {source:#x}: {error}");
    }

    context.free(image)?;

    Ok(())
}

/// Copy the `(x, y, w, h)` region of `fb` into a freshly allocated framebuffer
/// of the same depth, clamping reads to the source bounds. Used to materialise a
/// sub-image as an independent pixel plane.
fn crop_framebuffer(context: &mut dyn WIPICContext, fb: &FrameBuffer, x: i32, y: i32, w: i32, h: i32) -> Result<FrameBuffer> {
    let bpp = (fb.0.bpp / 8).max(1) as i64;
    let src = fb.data(context)?;
    let src_bpl = fb.0.bpl as i64;
    let (src_w, src_h) = (fb.0.width as i64, fb.0.height as i64);
    let new_bpl = w as i64 * bpp;
    let mut out = vec![0u8; (new_bpl * h as i64) as usize];

    for row in 0..h as i64 {
        let sy = y as i64 + row;
        if sy < 0 || sy >= src_h {
            continue;
        }
        for col in 0..w as i64 {
            let sx = x as i64 + col;
            if sx < 0 || sx >= src_w {
                continue;
            }
            let src_off = (sy * src_bpl + sx * bpp) as usize;
            let dst_off = (row * new_bpl + col * bpp) as usize;
            out[dst_off..dst_off + bpp as usize].copy_from_slice(&src[src_off..src_off + bpp as usize]);
        }
    }

    let dst = FrameBuffer::new(context, w as u32, h as u32, fb.0.bpp)?;
    context.write_bytes(context.data_ptr(dst.0.buf)?, &out)?;
    Ok(dst)
}

/// `MC_grpCreateSubImage(parent, x, y, w, h)` - a view of a rectangular region
/// of an existing image, as its own image handle. The reference copies the
/// region out into an independent plane, which is what a title relies on when it
/// slices a sprite sheet into individual frames; left unmapped it returned the
/// diagnostic stub 0, so every sub-image was null and its draws were skipped.
pub async fn create_sub_image(context: &mut dyn WIPICContext, parent: WIPICIndirectPtr, x: i32, y: i32, w: i32, h: i32) -> Result<WIPICWord> {
    tracing::debug!("MC_grpCreateSubImage({:#x}, {x}, {y}, {w}, {h})", parent.0);

    if parent.0 == 0 || w <= 0 || h <= 0 {
        return Ok(0);
    }

    let parent_image: WIPICImage = read_generic(context, context.data_ptr(parent)?)?;

    let sub_img = crop_framebuffer(context, &FrameBuffer(parent_image.img), x, y, w, h)?;
    let sub_mask = if parent_image.mask.buf.0 != 0 {
        crop_framebuffer(context, &FrameBuffer(parent_image.mask), x, y, w, h)?
    } else {
        FrameBuffer::empty()
    };

    let image = WIPICImage {
        img: sub_img.0,
        mask: sub_mask.0,
        loop_count: 0,
        delay: 0,
        animated: 0,
        buf: WIPICIndirectPtr(0),
        offset: 0,
        current: 0,
        len: 0,
    };

    let memory = context.alloc(size_of::<WIPICImage>() as WIPICWord)?;
    write_generic(context, context.data_ptr(memory)?, image)?;

    Ok(memory.0)
}

/// `MC_grpDecodeNextImage(image)` - advance an animated image to its next frame.
/// `create_wipi_image` already decodes the first (and, for the still images WIE
/// currently produces, only) frame, so the image is ready to draw; report frame
/// 0 as available rather than the diagnostic stub's 0-that-meant-nothing. Real
/// multi-frame stepping is not modelled yet.
pub async fn decode_next_image(context: &mut dyn WIPICContext, image: WIPICIndirectPtr) -> Result<i32> {
    tracing::debug!("MC_grpDecodeNextImage({:#x})", image.0);

    if image.0 == 0 {
        return Ok(-1);
    }

    let _wipi_image: WIPICImage = read_generic(context, context.data_ptr(image)?)?;
    Ok(0)
}

#[allow(clippy::too_many_arguments)]
pub async fn draw_image(
    context: &mut dyn WIPICContext,
    framebuffer: WIPICIndirectPtr,
    dx: i32,
    dy: i32,
    w: i32,
    h: i32,
    image: WIPICIndirectPtr,
    sx: i32,
    sy: i32,
    graphics_context: WIPICWord,
) -> Result<()> {
    tracing::debug!(
        "MC_grpDrawImage({:#x}, {dx}, {dy}, {w}, {h}, {:#x}, {sx}, {sy}, {graphics_context:#x})",
        framebuffer.0,
        image.0
    );

    // The slot a title left empty is drawn as nothing, the same way
    // `get_image_property` measures it as nothing.
    if image.0 == 0 {
        return Ok(());
    }

    let framebuffer = FrameBuffer(read_generic(context, context.data_ptr(framebuffer)?)?);
    let image: WIPICImage = read_generic(context, context.data_ptr(image)?)?;

    // An image that carries alpha keeps the full colour in the mask plane, and
    // its per-pixel alpha composites straight. One without a mask is a 16bpp
    // colour plane whose transparency is the magenta key instead, so it is
    // keyed rather than blended.
    let keyed = image.mask.buf.0 == 0;
    let source = if keyed { image.img } else { image.mask };
    // A title's own pixel operation decides what every pixel becomes, and it is
    // read before the canvas takes the context.
    let grp_ctx = read_context(context, context.graphics_context_layout(), graphics_context)?;

    // Only the part of the blit the context's clip allows. A clet that wants
    // one cell of a sprite sheet sets the clip to where that cell is to land
    // and hands over the whole sheet, so this is what picks the cell out.
    let (offset_x, offset_y) = context_offset(&grp_ctx);
    let (dx, dy) = (dx + offset_x, dy + offset_y);
    let Some((dx, dy, w, h, sx, sy)) = clipped_blit(&context_clip(&grp_ctx), dx, dy, w, h, sx, sy) else {
        return Ok(());
    };

    // Asked before anything takes the context, because asking runs the title's
    // own code. Without it 드래곤하트2's glow lands as an opaque disc.
    let operation = pixel_op::of_context(context, grp_ctx.pixel_op_func_ptr, grp_ctx.param1).await?;
    let source_first = context.pixel_op_takes_source_first();

    let clip = Clip {
        x: dx as _,
        y: dy as _,
        width: w as _,
        height: h as _,
    };

    let Some((kind, function)) = operation else {
        // Only the rows the sprite covers, rather than staging both surfaces
        // whole for every blit. Refused for a depth or geometry it cannot
        // address, and the canvas takes it then.
        let src_fb = FrameBuffer(source);
        if framebuffer.draw_image_direct(context, dx, dy, w, h, &src_fb, sx, sy, keyed)? {
            return Ok(());
        }

        let src_image = src_fb.image(context)?;
        let mut canvas = framebuffer.canvas(context)?;

        if keyed {
            blit_magenta_keyed(&mut **canvas, dx, dy, w, h, &*src_image, sx, sy);
        } else {
            canvas.draw(dx as _, dy as _, w as _, h as _, &*src_image, sx as _, sy as _, clip);
        }
        canvas.flush()?;

        return Ok(());
    };

    let source = FrameBuffer(source);
    let pairs = pixel_op_pairs(context, &framebuffer, dx, dy, w, h, &source, sx, sy, keyed)?;

    // A recognised operation is done here; anything else is asked, a pixel at a
    // time, which is what the reference does for all of them.
    let mut blended = Vec::with_capacity(pairs.len());
    for &(x, y, destination, source_pixel) in &pairs {
        let result = match pixel_op::apply(kind, destination, source_pixel, source_first) {
            Some(result) => result,
            None => {
                // Which pixel goes first is the handset's, not ours - see
                // `pixel_op_takes_source_first`. The recognised operations are
                // replayed through the same ordering, so an operation that
                // reads only one of the two is answered the same way whether it
                // was recognised or asked.
                let (a, b) = pixel_op::arguments(source_first, destination, source_pixel);

                context.call_function(function, &[a as WIPICWord, b as WIPICWord, grp_ctx.param1]).await? as u16
            }
        };

        blended.push((x, y, result));
    }

    write_blended(context, &framebuffer, &blended)?;

    Ok(())
}

/// Stores the results of a pixel operation over the rows they fall in.
///
/// The canvas path this replaces staged the whole surface to put the pixels
/// back - out of guest memory, into a pixel buffer, kept a third time as the
/// snapshot - and compared the whole of it back afterwards. `blend_pairs` has
/// just staged the same surface for the same call, so a blit through an
/// operation paid for the destination twice over, plus the source whole. LOA-
/// 혼돈의 서곡's opening asks for 224 of them a second on a 240x320 screen and
/// its scroll stops dead.
///
/// The rows the results fall in are read once, the results are placed into
/// them, and those rows go back. Pixels the operation was never asked about -
/// the transparent ones `blend_pairs` skips - keep what they already held,
/// because what is written back is what was read.
///
/// Falls back to the canvas for a surface `read_rect_rgb565` cannot address.
fn write_blended(context: &mut dyn WIPICContext, framebuffer: &FrameBuffer, blended: &[(i32, i32, u16)]) -> Result<()> {
    let Some((&(first_x, first_y, _), rest)) = blended.split_first() else {
        return Ok(());
    };

    let (mut left, mut right) = (first_x, first_x);
    let (mut top, mut bottom) = (first_y, first_y);
    for &(x, y, _) in rest {
        left = left.min(x);
        right = right.max(x);
        top = top.min(y);
        bottom = bottom.max(y);
    }
    let (cols, rows) = (right - left + 1, bottom - top + 1);

    if let Some((read_left, read_top, read_cols, read_rows, mut pixels)) = framebuffer.read_rect_rgb565(context, left, top, cols, rows)?
        && (read_left, read_top, read_cols, read_rows) == (left, top, cols, rows)
    {
        for &(x, y, pixel) in blended {
            pixels[((y - top) * cols + (x - left)) as usize] = pixel;
        }
        framebuffer.write_rect_rgb565(context, left, top, cols, rows, &pixels)?;

        return Ok(());
    }

    let mut canvas = framebuffer.canvas(context)?;
    for &(x, y, pixel) in blended {
        canvas.put_pixel(x, y, Rgb565Pixel::to_color(pixel));
    }
    canvas.flush()?;

    Ok(())
}

/// What the title's pixel operation will be asked about, with the destination
/// read over the rows the blit covers.
///
/// The canvas this replaces staged the whole surface - read out of guest
/// memory, collected into a pixel buffer, kept a third time as a snapshot -
/// only to read back the pixels under a sprite. `write_blended` already puts
/// the answers back a row at a time; this is the same surface, read the same
/// way, on the way in.
///
/// Falls back to the canvas for a surface `read_rect_rgb565` cannot address.
#[allow(clippy::too_many_arguments)]
fn pixel_op_pairs(
    context: &mut dyn WIPICContext,
    framebuffer: &FrameBuffer,
    dx: i32,
    dy: i32,
    w: i32,
    h: i32,
    source: &FrameBuffer,
    sx: i32,
    sy: i32,
    keyed: bool,
) -> Result<Vec<(i32, i32, u16, u16)>> {
    let src_image = source.image(context)?;
    let (dst_w, dst_h) = (framebuffer.0.width as i64, framebuffer.0.height as i64);

    if let Some((left, top, cols, rows, pixels)) = framebuffer.read_rect_rgb565(context, dx, dy, w, h)? {
        let read = move |x: i32, y: i32| -> u16 {
            // Outside what was read is outside the blit, and `blend_pairs`
            // never asks about it.
            if x < left || y < top || x >= left + cols || y >= top + rows {
                return 0;
            }
            pixels[((y - top) * cols + (x - left)) as usize]
        };

        return Ok(blend_pairs(read, dst_w, dst_h, dx, dy, w, h, &*src_image, sx, sy, keyed));
    }

    let canvas = framebuffer.canvas(context)?;
    let image = canvas.image();
    let read = |x: i32, y: i32| Rgb565Pixel::from_color(image.get_pixel(x, y));

    Ok(blend_pairs(read, dst_w, dst_h, dx, dy, w, h, &*src_image, sx, sy, keyed))
}

/// The destination and source pixel of everything a blit would write, as
/// RGB565 and in the order it would write them.
///
/// Gathered in one pass because asking the title what a pixel becomes needs the
/// destination let go of first.
///
/// `destination` answers for one pixel of the surface being drawn on. It is a
/// reader rather than the surface itself so that the caller can serve it from
/// the rows the blit covers, read on their own, instead of staging the whole
/// surface for every call - see [`pixel_op_pairs`].
#[allow(clippy::too_many_arguments)]
fn blend_pairs(
    destination: impl Fn(i32, i32) -> u16,
    dst_w: i64,
    dst_h: i64,
    dx: i32,
    dy: i32,
    w: i32,
    h: i32,
    src: &dyn Image,
    sx: i32,
    sy: i32,
    keyed: bool,
) -> Vec<(i32, i32, u16, u16)> {
    let src_w = src.width() as i64;
    let src_h = src.height() as i64;

    let mut pairs = Vec::new();

    for row in 0..h as i64 {
        let sy_px = sy as i64 + row;
        let dy_px = dy as i64 + row;
        if sy_px < 0 || sy_px >= src_h || dy_px < 0 || dy_px >= dst_h {
            continue;
        }
        for col in 0..w as i64 {
            let sx_px = sx as i64 + col;
            let dx_px = dx as i64 + col;
            if sx_px < 0 || sx_px >= src_w || dx_px < 0 || dx_px >= dst_w {
                continue;
            }

            let source = src.get_pixel(sx_px as i32, sy_px as i32);
            // A pixel the image does not have is not a pixel the operation is
            // asked about. Drawing through an operation used to write the whole
            // rectangle, transparent corners included, so 마스터오브소드4's
            // glyphs came down as magenta blocks the moment they came down at
            // all - the plain path has always skipped these.
            if source.a == 0 || (keyed && is_transparent_key(source)) {
                continue;
            }

            pairs.push((
                dx_px as i32,
                dy_px as i32,
                destination(dx_px as i32, dy_px as i32),
                Rgb565Pixel::from_color(source),
            ));
        }
    }

    pairs
}

pub async fn flush_lcd(
    context: &mut dyn WIPICContext,
    i: WIPICWord,
    framebuffer: WIPICIndirectPtr,
    x: WIPICWord,
    y: WIPICWord,
    w: WIPICWord,
    h: WIPICWord,
) -> Result<()> {
    tracing::debug!("MC_grpFlushLcd({i:#x}, {:#x}, {x:#x}, {y:#x}, {w:#x}, {h:#x})", framebuffer.0);

    let framebuffer = FrameBuffer(read_generic(context, context.data_ptr(framebuffer)?)?);

    let src_canvas = framebuffer.image(context)?;

    // DIAGNOSTIC: summarise the frame we are about to present so a device log
    // shows whether an image actually reached the draw buffer. A text-only
    // screen carries a handful of colours; a decoded image carries dozens or
    // hundreds. Distinguishes "the image never got drawn" from "it was drawn
    // but is not reaching the display". Logged at info so it survives a normal
    // capture without turning on the per-primitive flood.
    //
    // Asked first whether anyone is listening: the summary walks every pixel of
    // the frame and builds a set of its colours, and a session with the log off
    // was paying that for a line nobody reads. It does not show up against the
    // cost of a frame, so this is not a speed-up - it is work that had no
    // reason to happen.
    if tracing::enabled!(tracing::Level::INFO) {
        let (colours, non_black) = surface_content(&*src_canvas);

        tracing::info!(
            "FRAME flush fb={:#x} {}x{} region=({x},{y},{w},{h}) colours={colours} non_black={non_black}",
            framebuffer.0.buf.0,
            src_canvas.width(),
            src_canvas.height(),
        );

        // And the frame itself, on the rounds the off-screen surfaces are drawn
        // on. This is the one picture a reader can hold a screenshot against,
        // which is what says whether a surface reached the screen.
        if FLUSHES.load(Ordering::Relaxed).is_multiple_of(OFFSCREEN_TRACE_EVERY) {
            for line in surface_thumbnail(&*src_canvas) {
                tracing::info!("FRAME |{line}|");
            }
        }
    }

    TITLE_FLUSHES_LCD.store(true, Ordering::Relaxed);

    // A title that flushes is driving the LCD, which is what keeps the MIDP
    // layer from flushing its own screen image over the top - see
    // `javax.microedition.lcdui.Display`. `KtfEmulator::present_lcd` used to be
    // the only thing that said so, and it stands down for a title that flushes.
    context.system().set_title_drives_lcd();

    present_region(context.system(), &*src_canvas, x as i32, y as i32, w as i32, h as i32);

    // What is on the surfaces the title drew into but never handed back. After
    // the paint, so the frame is on its way before this reads anything, and the
    // canvas it borrowed is done with.
    drop(src_canvas);
    trace_offscreen_surfaces(context);

    Ok(())
}

/// Whether the title has flushed the LCD itself.
///
/// A platform has two ways of learning that a frame is ready. A title that
/// composes through the `MC_grp*` calls says so with `MC_grpFlushLcd`; one
/// whose C engine writes the frame buffer directly says nothing at all, and
/// `KtfEmulator::present_lcd` watches the buffer for it instead. Once a title
/// has flushed, it is the first kind, and the watcher must stand down: it shows
/// the whole buffer, which is exactly what a partial flush is asking it not to.
static TITLE_FLUSHES_LCD: AtomicBool = AtomicBool::new(false);

/// Whether `MC_grpFlushLcd` has been called since the title started.
pub fn title_flushes_lcd() -> bool {
    TITLE_FLUSHES_LCD.load(Ordering::Relaxed)
}

/// The panel, as the last flush left it.
///
/// Width, height, bytes per pixel and the pixels themselves. A title's frame
/// buffer and the panel are two surfaces on a handset, and `MC_grpFlushLcd`
/// moves a rectangle from the one to the other - so what the call does not name
/// stays on the panel. This is that panel.
static PANEL: spin::Mutex<Option<Panel>> = spin::Mutex::new(None);

/// Width, height, bytes per pixel, and the pixels.
type Panel = (u32, u32, u32, Vec<u8>);

/// Forgets the panel, so a title starting does not inherit the last one's.
fn forget_panel() {
    *PANEL.lock() = None;
}

/// Shows `image`, with only `(x, y, w, h)` of it reaching the panel.
///
/// LOA-혼돈의 서곡 is why this is not the whole frame every time. It composes
/// its scene in a back buffer, copies the whole 240x320 of it over the screen
/// buffer, draws the HP and SP bars, and then flushes `240x295` - the rows it
/// changed. The status bar below them it drew on an earlier frame and flushed
/// in full, and it expects the panel to still be holding it. Shown the whole
/// frame buffer instead, the bar came and went with whichever frame had last
/// redrawn it, which on the handset is a row of item icons and an experience
/// bar that flicker.
///
/// A rectangle that covers the frame, or one that names nothing at all, goes
/// straight through: the first has nothing to keep and the second is a title
/// whose arguments this cannot read, which is better shown its frame than an
/// empty panel.
fn present_region(system: &mut System, image: &dyn Image, x: i32, y: i32, w: i32, h: i32) {
    let (width, height, bpp) = (image.width(), image.height(), image.bytes_per_pixel());

    let kept = {
        let mut panel = PANEL.lock();

        panel_after_flush(&mut panel, &image.raw(), width, height, bpp, x, y, w, h)
    };

    let Some(pixels) = kept else {
        wie_backend::present(system, image);

        return;
    };

    let shown: Box<dyn Image> = match bpp {
        1 => Box::new(VecImageBuffer::<Rgb332Pixel>::from_raw(width, height, pod_collect_to_vec(&pixels))),
        2 => Box::new(VecImageBuffer::<Rgb565Pixel>::from_raw(width, height, pod_collect_to_vec(&pixels))),
        4 => Box::new(VecImageBuffer::<Rgb8Pixel>::from_raw(width, height, pod_collect_to_vec(&pixels))),
        _ => {
            wie_backend::present(system, image);

            return;
        }
    };

    wie_backend::present(system, &*shown);
}

/// Lays `(x, y, w, h)` of a frame onto the panel and answers with the panel, or
/// with `None` where the frame is what should be shown as it is.
///
/// `None` covers a rectangle that spans the frame - nothing is kept, so there
/// is no panel to build - and the cases this cannot read: a depth it cannot
/// address, a rectangle that names nothing, a frame whose bytes do not amount
/// to the size it reports. Each of those forgets the panel, because what it was
/// holding is no longer known to line up with what is being shown.
#[allow(clippy::too_many_arguments)]
fn panel_after_flush(panel: &mut Option<Panel>, raw: &[u8], width: u32, height: u32, bpp: u32, x: i32, y: i32, w: i32, h: i32) -> Option<Vec<u8>> {
    let covers_all = x <= 0 && y <= 0 && x.saturating_add(w) >= width as i32 && y.saturating_add(h) >= height as i32;
    let addressable = matches!(bpp, 1 | 2 | 4);

    let stride = width as usize * bpp as usize;
    if w <= 0 || h <= 0 || !addressable || raw.len() < stride * height as usize {
        *panel = None;

        return None;
    }

    // A frame that covers the panel becomes the panel and is shown as it is.
    // Keeping it is the whole point: the rows a later partial flush does not
    // name are the rows this one left there.
    if covers_all {
        *panel = Some((width, height, bpp, raw.to_vec()));

        return None;
    }

    // A panel of another shape is another title's, or another screen size:
    // nothing on it belongs under this frame.
    let fits = matches!(panel.as_ref(), Some((panel_width, panel_height, panel_bpp, _)) if (*panel_width, *panel_height, *panel_bpp) == (width, height, bpp));
    if !fits {
        *panel = Some((width, height, bpp, vec![0; stride * height as usize]));
    }

    let pixels = &mut panel.as_mut().unwrap().3;

    let left = x.max(0) as usize;
    let right = x.saturating_add(w).clamp(0, width as i32) as usize;
    let top = y.max(0) as usize;
    let bottom = y.saturating_add(h).clamp(0, height as i32) as usize;

    for row in top..bottom {
        let from = row * stride + left * bpp as usize;
        let to = row * stride + right * bpp as usize;

        pixels[from..to].copy_from_slice(&raw[from..to]);
    }

    Some(pixels.clone())
}

pub async fn get_pixel_from_rgb(_context: &mut dyn WIPICContext, r: i32, g: i32, b: i32) -> Result<WIPICWord> {
    tracing::debug!("MC_grpGetPixelFromRGB({r:#x}, {g:#x}, {b:#x})");
    if (r > 0xff) || (g > 0xff) | (b > 0xff) {
        tracing::debug!("MC_grpGetPixelFromRGB({r:#x}, {g:#x}, {b:#x}): value clipped to 8 bits");
    }

    let color = Rgb565Pixel::from_color(Color {
        a: 0xff,
        r: r as u8,
        g: g as u8,
        b: b as u8,
    });

    Ok(color as WIPICWord)
}

pub async fn get_rgb_from_pixel(context: &mut dyn WIPICContext, pixel: i32, r: WIPICWord, g: WIPICWord, b: WIPICWord) -> Result<i32> {
    tracing::debug!("MC_grpGetRGBFromPixel({pixel}, {r:#x}, {g:#x}, {b:#x})");

    let color = Rgb565Pixel::to_color(pixel as u16);

    write_generic(context, r, color.r as i32)?;
    write_generic(context, g, color.g as i32)?;
    write_generic(context, b, color.b as i32)?;

    Ok(pixel)
}

pub async fn get_display_info(context: &mut dyn WIPICContext, reserved: WIPICWord, out_ptr: WIPICWord) -> Result<WIPICWord> {
    tracing::debug!("MC_grpGetDisplayInfo({reserved:#x}, {out_ptr:#x})");

    assert_eq!(reserved, 0);

    let (width, height) = {
        let platform = context.system().platform();
        let screen = platform.screen();
        (screen.width(), screen.height())
    };

    // The reference's MC_grpGetDisplayInfo (@0x1abcf8) reports width@+8 and
    // height@+0xc, and for a 240/320/400-wide panel subtracts the status strip
    // from the height when the annunciator properties are set - so what a title
    // reads here is the drawing area, not the panel. `annunciator_rows` is that
    // strip, and zero unless the platform reserves one, which leaves the full
    // height reported as before. The colour fields are the driver's direct
    // RGB565 format (matched by Rgb565Pixel), reported here as fixed masks.
    let strip = annunciator_rows(context, width).min(height);
    let info = WIPICDisplayInfo {
        bpp: FRAMEBUFFER_DEPTH,
        depth: 16,
        width,
        height: height - strip,
        bpl: 2 * width,
        color_type: 1, // 1==MC_GRP_DIRECT_COLOR_TYPE
        red_mask: 0xf800,
        green_mask: 0x7e0,
        blue_mask: 0x1f,
    };

    write_generic(context, out_ptr, info)?;
    Ok(1)
}

#[allow(clippy::too_many_arguments)]
pub async fn copy_area(
    context: &mut dyn WIPICContext,
    dst: WIPICIndirectPtr,
    dx: i32,
    dy: i32,
    w: i32,
    h: i32,
    x: i32,
    y: i32,
    pgc: WIPICWord,
) -> Result<()> {
    tracing::debug!("MC_grpCopyArea({:#x}, {dx}, {dy}, {w}, {h}, {x}, {y}, {pgc:#x})", dst.0);

    if w < 0 || h < 0 {
        tracing::warn!("Skipping negative dimension");

        return Ok(());
    }

    let framebuffer = FrameBuffer(read_generic(context, context.data_ptr(dst)?)?);
    let gctx = read_context(context, context.graphics_context_layout(), pgc)?;
    let (offset_x, offset_y) = context_offset(&gctx);
    let (dx, dy) = (dx + offset_x, dy + offset_y);

    let Some((dx, dy, w, h, x, y)) = clipped_blit(&context_clip(&gctx), dx, dy, w, h, x, y) else {
        return Ok(());
    };

    let image = framebuffer.image(context)?;
    let mut canvas = framebuffer.canvas(context)?;

    let clip = Clip {
        x: dx as _,
        y: dy as _,
        width: w as _,
        height: h as _,
    };

    canvas.draw(dx as _, dy as _, w as _, h as _, &*image, x as _, y as _, clip);
    canvas.flush()?;

    Ok(())
}

/// Extra rows allocated beneath a draw surface, never reported in its
/// dimensions. A title draws into a surface (the screen framebuffer or an
/// off-screen buffer) with the clip context's default `0x7fff` bound - i.e.
/// effectively unclipped - and its own software blitter does not clamp to the
/// surface height, so a sprite placed near the bottom (MapleStory 도적편
/// rotates its title character down to y≈300 in a 320-row buffer) writes tens
/// of rows past the end. On the reference that overdraw lands in slack the
/// memory map leaves after the surface; here the next allocation sits there, so
/// the overdraw smashes it - and 4 bytes past the screen buffer is the *list
/// allocator header* of the title's work arena, so the stray pixels clear its
/// in-use bit, the heap then re-hands that live arena out, and the resource
/// load into the re-issued block wipes the menu the title just built there.
/// Padding the surface with owned rows keeps that overdraw benign, as it is on
/// the device. Measured worst case for 도적편 is ~24 rows; 256 is generous
/// headroom and still trivial against the heap.
const SURFACE_GUARD_ROWS: u32 = 256;

/// Build a draw surface with `SURFACE_GUARD_ROWS` of owned slack under its
/// reported height. The width/height/stride the surface reports are the real
/// ones, so a title's addressing is identical; the extra rows only exist to
/// absorb its unclamped overdraw instead of the next allocation.
fn new_guarded_surface(context: &mut dyn WIPICContext, width: u32, height: u32) -> Result<FrameBuffer> {
    let mut framebuffer = FrameBuffer::new(context, width, height.saturating_add(SURFACE_GUARD_ROWS), FRAMEBUFFER_DEPTH)?;
    framebuffer.0.height = height as _;
    Ok(framebuffer)
}

/// Height of the status strip above the drawing area, as the platform stored it
/// in `ANNUNCIATOR_ROWS_PTR`, and zero when it stored nothing. The reference
/// only reserves the strip on the panel widths its own table lists, so a title
/// on any other panel sees the whole display exactly as it does today.
fn annunciator_rows(context: &dyn WIPICContext, width: u32) -> u32 {
    if !matches!(width, 240 | 320 | 400) {
        return 0;
    }

    read_generic(context, ANNUNCIATOR_ROWS_PTR).unwrap_or(0)
}

/// Build the screen surface with the status strip above the drawing area.
///
/// The strip is part of the panel, not of the title's drawing area, and the
/// reference splits the two: `MC_grpGetFrameBufferHeight` and
/// `MC_grpGetDisplayInfo` report the drawing area (the panel less the strip),
/// every `MC_grp*` primitive lands inside it, and only
/// `MC_grpGetFrameBufferPointer` steps back to the panel's own first row -
/// `wipic_get_frame_pointer` resolves the screen surface through its *parent*,
/// so the address a title blits to starts at the strip.
///
/// A title that blits straight to that pointer therefore skips the strip
/// itself. MapleStory 도적편 does: it keeps the strip height in a field and adds
/// it to every row index it derives from the pointer, which is why its splash
/// logos, HUD icons and shortcut numbers sat a strip's worth too low here while
/// everything drawn through the `MC_grp*` calls stayed where it belonged.
fn new_screen_surface(context: &mut dyn WIPICContext, width: u32, height: u32) -> Result<FrameBuffer> {
    let strip = annunciator_rows(context, width).min(height);

    // The surface spans the whole panel; the framebuffer reports - and every
    // path but the pointer getter uses - the drawing area below the strip.
    let mut framebuffer = FrameBuffer::new(context, width, height.saturating_add(SURFACE_GUARD_ROWS), FRAMEBUFFER_DEPTH)?;

    // A handset's LCD buffer starts dark; ours starts as whatever the heap was
    // last used for, because `Allocator::alloc` does not clear what it hands
    // out. That only shows on this surface, because it is the one a title can
    // put on the panel without having drawn every pixel of it - a C engine that
    // composes its scene into the top rows leaves the rest untouched, and 던전
    // 앤파이터 격투가 showed a band of old heap under every frame for exactly
    // that reason. Clear it once, here, rather than trusting the title to.
    let (size, _) = buffer_size(width, height.saturating_add(SURFACE_GUARD_ROWS), FRAMEBUFFER_DEPTH / 8)?;
    let base = context.data_ptr(framebuffer.0.buf)?;
    context.write_bytes(base, &vec![0u8; size as usize])?;
    // Where an indirect pointer is an address, the strip is taken out of the
    // framebuffer: the title is told the drawing area and handed a pointer that
    // starts below the strip.
    //
    // On KTF an indirect pointer is a handle whose target holds the address, so
    // neither is possible - adding a row's worth of bytes to a handle names no
    // allocation at all, and 격투가 read its own frame buffer pointer back as 8
    // and stored a pixel through it before it had drawn anything. There the
    // title is told the whole panel and reserves the strip itself, which is
    // what this engine does: told 320 it draws 296, told 296 it draws 272. So
    // the strip comes off at the other end, where the panel is shown - see
    // `screen_surface_bytes`.
    let split = context.data_ptr(framebuffer.0.buf)? == framebuffer.0.buf.0;
    framebuffer.0.height = if split { height - strip } else { height };
    if split {
        framebuffer.0.buf = WIPICIndirectPtr(framebuffer.0.buf.0 + strip * framebuffer.0.bpl);
    }

    Ok(framebuffer)
}

/// The screen surface's pixels, read through raw guest memory.
///
/// A title whose drawing is a C engine writes the LCD frame buffer directly -
/// `memcpy` into the pointer `MC_grpGetScreenFrameBuffer` handed it - and never
/// calls `MC_grpFlushLcd`, because on the handset that buffer *is* the display.
/// Nothing on the WIPI-C side sees those writes, and the Java side cannot reach
/// guest memory at all, so the only place both the frame buffer and the screen
/// are in scope is the emulator's own tick. This is what it reads.
///
/// `data_ptr` resolves an indirect pointer the way the platform does: KTF stores
/// a handle whose target is eight bytes ahead of the data, LGT stores the
/// address itself.
///
/// Answers `None` when the title has not taken a screen frame buffer, which is
/// every title that draws through the Java layer instead.
pub fn screen_surface_bytes(mem: &dyn ByteRead, data_ptr: &dyn Fn(WIPICWord) -> Result<WIPICWord>) -> Result<Option<(u32, u32, Vec<u8>)>> {
    let handle: WIPICWord = read_generic(mem, SCREEN_FRAMEBUFFER_PTR)?;
    if handle == 0 {
        return Ok(None);
    }

    let framebuffer: WIPICFramebuffer = read_generic(mem, data_ptr(handle)?)?;
    if framebuffer.width == 0 || framebuffer.height == 0 || framebuffer.bpp != FRAMEBUFFER_DEPTH || framebuffer.buf.0 == 0 {
        return Ok(None);
    }

    // The panel less the handset's own status strip. On this platform the
    // framebuffer spans the whole panel and the title reserves the strip
    // itself - see `new_screen_surface` - so the rows it leaves at the bottom
    // are the handset's to paint, not a frame, and showing them was a band of
    // whatever had been there under every one of 격투가's frames.
    let strip: u32 = read_generic(mem, ANNUNCIATOR_ROWS_PTR).unwrap_or(0);
    let height = framebuffer.height.saturating_sub(strip);
    if height == 0 {
        return Ok(None);
    }

    let (size, _) = match buffer_size(framebuffer.width, height, framebuffer.bpp / 8) {
        Ok(x) => x,
        Err(_) => return Ok(None),
    };

    let mut bytes = vec![0u8; size as usize];
    mem.read_bytes(data_ptr(framebuffer.buf.0)?, &mut bytes)?;

    Ok(Some((framebuffer.width, height, bytes)))
}

/// The off-screen surfaces a title has asked for and not destroyed, so a flush
/// can say what is on them. See [`trace_offscreen_surfaces`].
static OFFSCREEN_SURFACES: spin::Mutex<Vec<(WIPICWord, i32, i32)>> = spin::Mutex::new(Vec::new());

/// Flushes between one round of off-screen fingerprints and the next.
///
/// A fingerprint reads every pixel of every surface, which is more work than a
/// frame; a screen worth looking at holds still for many frames, so sampling
/// says the same thing for a fraction of the cost.
const OFFSCREEN_TRACE_EVERY: u32 = 30;

/// Flushes so far, for [`OFFSCREEN_TRACE_EVERY`].
static FLUSHES: AtomicU32 = AtomicU32::new(0);

/// How much is on a surface: how many of its pixels are not black, and how many
/// distinct colours they are.
///
/// Enough to tell a surface that was drawn on from one that was not, which is
/// the question a missing sprite asks. Counting colours stops at 512 - past
/// that the answer is "a picture" either way.
fn surface_content(canvas: &dyn Image) -> (usize, u32) {
    use alloc::collections::BTreeSet;

    let mut colours: BTreeSet<u32> = BTreeSet::new();
    let mut non_black: u32 = 0;

    for colour in canvas.colors() {
        let packed = ((colour.r as u32) << 16) | ((colour.g as u32) << 8) | colour.b as u32;
        if packed != 0 {
            non_black += 1;
        }
        if colours.len() <= 512 {
            colours.insert(packed);
        }
    }

    (colours.len(), non_black)
}

/// Characters a thumbnail cell can be, darkest first.
///
/// A ramp rather than a threshold: an icon and the panel it sits on differ in
/// shade more often than they differ in being lit at all.
const THUMBNAIL_RAMP: [u8; 10] = *b" .:-=+*#%@";

/// Widest a thumbnail gets, in characters.
const THUMBNAIL_COLUMNS: u32 = 48;

/// Tallest a thumbnail gets, in rows.
const THUMBNAIL_ROWS: u32 = 48;

/// Draws a surface small enough to read in a log.
///
/// A count of lit pixels says a surface was drawn on; it does not say what was
/// drawn, and "an icon" and "the panel behind where an icon should be" both
/// come back as a few thousand lit pixels. This is the difference, at the only
/// resolution a log can carry: each cell is the mean brightness of the block it
/// stands for, mapped through [`THUMBNAIL_RAMP`].
fn surface_thumbnail(canvas: &dyn Image) -> Vec<String> {
    let (width, height) = (canvas.width(), canvas.height());
    if width == 0 || height == 0 {
        return Vec::new();
    }

    // A character cell is about twice as tall as it is wide, so a thumbnail
    // that kept one row per column would squash a portrait surface flat - which
    // is what a 240x320 screen is. Take the rows from the surface's own shape.
    let columns = THUMBNAIL_COLUMNS.min(width);
    let rows = (columns * height / width / 2).clamp(1, THUMBNAIL_ROWS.min(height));

    // One pass over the pixels, accumulating into the cell each falls in, so a
    // thumbnail costs the read it already does rather than a read per cell.
    let mut sums = vec![0u64; (columns * rows) as usize];
    let mut counts = vec![0u32; (columns * rows) as usize];

    for (index, colour) in canvas.colors().into_iter().enumerate() {
        let index = index as u32;
        let (x, y) = (index % width, index / width);
        let cell = (y * rows / height) * columns + (x * columns / width);

        let Some(sum) = sums.get_mut(cell as usize) else {
            continue;
        };
        // Rounded to the eye rather than to the spec: green reads brightest.
        *sum += (colour.r as u64 * 2 + colour.g as u64 * 5 + colour.b as u64) / 8;
        counts[cell as usize] += 1;
    }

    (0..rows)
        .map(|row| {
            (0..columns)
                .map(|column| {
                    let cell = (row * columns + column) as usize;
                    let mean = if counts[cell] == 0 { 0 } else { sums[cell] / counts[cell] as u64 };
                    let step = (mean * (THUMBNAIL_RAMP.len() as u64 - 1) / 255).min(THUMBNAIL_RAMP.len() as u64 - 1);

                    THUMBNAIL_RAMP[step as usize] as char
                })
                .collect()
        })
        .collect()
}

/// Whether what is at a registered surface's address is still that surface.
///
/// The registry is a static and outlives a game, so a pointer left in it by the
/// title before names an allocation this one now owns for something else. Read
/// back as a framebuffer, that reaches a depth nothing supports and takes the
/// emulator down with it - which is how this diagnostic came to crash a game
/// that was run after a game that had used one.
///
/// A surface that no longer says the size it was registered at, in a depth
/// there is a pixel format for, is not that surface any more.
fn still_the_surface(raw: &WIPICFramebuffer, width: i32, height: i32) -> bool {
    raw.width as i32 == width && raw.height as i32 == height && matches!(raw.bpp, 16 | 32)
}

/// Reports what is on each off-screen surface, every so often.
///
/// Some titles never hand their art back through this API: 오셔너스 takes the
/// pointer out of a surface and writes pixels into it itself, calling no blit,
/// image or string call at all, and composes the screen the same way. A log of
/// the calls it makes therefore says nothing about what it drew, and a sprite
/// that fails to appear looks identical to one that was never asked for.
///
/// These lines are the missing half. A surface that stays black was never drawn
/// on, which puts the fault in the title's own decision to draw; one that holds
/// a picture the screen does not show puts it in how the title got it there.
fn trace_offscreen_surfaces(context: &mut dyn WIPICContext) {
    let flushes = FLUSHES.fetch_add(1, Ordering::Relaxed);
    if !flushes.is_multiple_of(OFFSCREEN_TRACE_EVERY) {
        return;
    }

    // Reading a surface back copies it out of guest memory and counts its
    // colours, so like the frame summary above it is only worth doing for a
    // reader who will see it.
    if !tracing::enabled!(tracing::Level::INFO) {
        return;
    }

    let surfaces = OFFSCREEN_SURFACES.lock().clone();
    let mut stale = Vec::new();

    for (memory, w, h) in surfaces {
        // A diagnostic never gets in the way of the frame it is describing, so
        // a surface that cannot be read is passed over rather than reported.
        let Ok(data_ptr) = context.data_ptr(WIPICIndirectPtr(memory)) else {
            continue;
        };
        let Ok(raw): Result<WIPICFramebuffer> = read_generic(context, data_ptr) else {
            continue;
        };

        if !still_the_surface(&raw, w, h) {
            stale.push(memory);
            continue;
        }

        let Ok(canvas) = FrameBuffer(raw).image(context) else {
            continue;
        };

        let (colours, non_black) = surface_content(&*canvas);

        tracing::info!("OFFSCREEN {memory:#x} {w}x{h} colours={colours} non_black={non_black}");

        // And what it is, not just how much of it there is. A surface nothing
        // drew on has already said so in the line above; drawing its emptiness
        // as sixteen rows of spaces would only crowd out the ones that matter.
        if non_black == 0 {
            continue;
        }

        for line in surface_thumbnail(&*canvas) {
            tracing::info!("OFFSCREEN {memory:#x} |{line}|");
        }
    }

    if !stale.is_empty() {
        OFFSCREEN_SURFACES.lock().retain(|(memory, _, _)| !stale.contains(memory));
    }
}

pub async fn create_offscreen_framebuffer(context: &mut dyn WIPICContext, w: i32, h: i32) -> Result<WIPICIndirectPtr> {
    tracing::debug!("MC_grpCreateOffScreenFrameBuffer({w}, {h})");

    let framebuffer = new_guarded_surface(context, w as _, h as _)?;

    let memory = context.alloc(size_of::<WIPICFramebuffer>() as WIPICWord)?;
    write_generic(context, context.data_ptr(memory)?, framebuffer.0)?;

    OFFSCREEN_SURFACES.lock().push((memory.0, w, h));

    Ok(memory)
}

pub async fn destroy_offscreen_framebuffer(context: &mut dyn WIPICContext, framebuffer: WIPICIndirectPtr) -> Result<()> {
    tracing::debug!("MC_grpDestroyOffScreenFrameBuffer({:#x})", framebuffer.0);

    if framebuffer.0 == 0 {
        return Ok(());
    }

    OFFSCREEN_SURFACES.lock().retain(|(memory, _, _)| *memory != framebuffer.0);

    // The surface is two allocations - the descriptor and the pixels it points
    // at - and freeing only the descriptor, as this did, leaks the pixels. A
    // 240x320 surface is a quarter of a megabyte with its guard rows, so a
    // title that takes one per frame walks through the heap: 록맨X does exactly
    // that in its stages and stopped with `net.wie.WieError: Allocation
    // failure` about four hundred frames in, the heap holding 396 surfaces
    // nothing could reach. `MC_grpDestroyImage` beside this frees its planes
    // for the same reason.
    let raw: WIPICFramebuffer = read_generic(context, context.data_ptr(framebuffer)?)?;
    if raw.buf.0 != 0 {
        context.free(raw.buf)?;
    }

    context.free(framebuffer)?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn copy_frame_buffer(
    context: &mut dyn WIPICContext,
    dst: WIPICIndirectPtr,
    dx: i32,
    dy: i32,
    w: i32,
    h: i32,
    src: WIPICIndirectPtr,
    sx: i32,
    sy: i32,
    pgc: WIPICWord,
) -> Result<()> {
    tracing::debug!(
        "MC_grpCopyFrameBuffer({:#x}, {dx}, {dy}, {w}, {h}, {:#x}, {sx}, {sy}, {pgc:#x})",
        dst.0,
        src.0
    );

    let src_framebuffer = FrameBuffer(read_generic(context, context.data_ptr(src)?)?);
    let dst_framebuffer = FrameBuffer(read_generic(context, context.data_ptr(dst)?)?);

    let gctx = read_context(context, context.graphics_context_layout(), pgc)?;
    let Some((dx, dy, w, h, sx, sy)) = clipped_blit(&context_clip(&gctx), dx, dy, w, h, sx, sy) else {
        return Ok(());
    };

    let src_image = src_framebuffer.image(context)?;
    let mut dst_canvas = dst_framebuffer.canvas(context)?;

    blit_magenta_keyed(&mut **dst_canvas, dx, dy, w, h, &*src_image, sx, sy);
    dst_canvas.flush()?;

    Ok(())
}

/// Whether a colour is the magenta (RGB565 `0xF81F`) that feature-phone titles
/// reserve as a transparent colour key. The top five red and blue bits and no
/// green survive the round trip through a 16bpp buffer as exactly `255, 0, 255`.
fn is_transparent_key(color: Color) -> bool {
    color.r >= 0xf8 && color.g <= 0x07 && color.b >= 0xf8
}

/// Copies `src` onto `canvas`, skipping magenta source pixels. A title draws a
/// layer over a magenta fill and blits it expecting the magenta keyed out; the
/// graphics context carries no transparent pixel for these blits, so the
/// convention is honoured here rather than read from it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn blit_magenta_keyed(canvas: &mut dyn Canvas, dx: i32, dy: i32, w: i32, h: i32, src: &dyn Image, sx: i32, sy: i32) {
    let src_w = src.width() as i64;
    let src_h = src.height() as i64;
    let dst_w = canvas.image().width() as i64;
    let dst_h = canvas.image().height() as i64;

    for row in 0..h as i64 {
        let sy_px = sy as i64 + row;
        let dy_px = dy as i64 + row;
        if sy_px < 0 || sy_px >= src_h || dy_px < 0 || dy_px >= dst_h {
            continue;
        }
        for col in 0..w as i64 {
            let sx_px = sx as i64 + col;
            let dx_px = dx as i64 + col;
            if sx_px < 0 || sx_px >= src_w || dx_px < 0 || dx_px >= dst_w {
                continue;
            }

            let color = src.get_pixel(sx_px as i32, sy_px as i32);
            if is_transparent_key(color) {
                continue;
            }
            canvas.put_pixel(dx_px as i32, dy_px as i32, color);
        }
    }
}

/// Draw a string from the handset's own bitmap face.
///
/// `y` is the top of the glyph box, the same origin the outline path draws
/// from, and each glyph is stamped a pixel at a time so the result is the 1-bit
/// shape the face stores rather than an anti-aliased rendering of it.
fn draw_bitmap_string(canvas: &mut dyn Canvas, face: &BitmapFace, string: &str, x: i32, y: i32, color: Color, clip: Clip) {
    let mut pen = x;
    for c in string.chars() {
        let Some(glyph) = face.glyph(c) else {
            continue;
        };

        for row in 0..face.height {
            let py = y + row as i32;
            if py < clip.y || py >= clip.y + clip.height as i32 {
                continue;
            }

            for col in 0..glyph.width() {
                if !glyph.pixel(col, row) {
                    continue;
                }

                let px = pen + col as i32;
                if px < clip.x || px >= clip.x + clip.width as i32 {
                    continue;
                }

                canvas.put_pixel(px, py, color);
            }
        }

        pen += glyph.advance as i32;
    }
}

/// The single font size the WIPI-C text path uses, in device pixels. All of
/// `MC_grpGetFontHeight`, `MC_grpGetStringWidth` and `MC_grpDrawString` must
/// agree on it: a title measures text with the first two and lays it out
/// against the third, so any mismatch makes glyphs overlap. It matches the
/// 10px ascent + 2px descent the metric getters below report.
const FONT_PX_HEIGHT: f32 = 12.0;

/// Pixel height the vendor's `MC_grpGetFont` assigns to each size selector.
///
/// The reference (`MC_grpGetFont` in `liblgt_system.so`) maps the size flag to
/// one of seven glyph heights; a title picks a size for a heading or a HUD and
/// then lays it out with `MC_grpGetStringWidth`/`MC_grpGetFontHeight`, so all
/// three have to agree. We had a single 12px face, which shrank every heading
/// to body size and threw off any layout measured against the real height.
fn font_size_px(size: i32) -> i32 {
    match size {
        0x8 => 10,
        0x10 => 14,
        0x1000 => 16,
        0x2000 => 18,
        0x4000 => 19,
        0x8000 => 22,
        _ => 12,
    }
}

/// The pixel height carried by a font handle. `MC_grpGetFont` returns the
/// height itself as the handle, so decoding is the identity for a real handle
/// and the default face for 0 (an unset `SetContext` font).
fn font_handle_height(font: i32) -> f32 {
    if (8..=64).contains(&font) { font as f32 } else { FONT_PX_HEIGHT }
}

/// Ascent for a face of the given height, keeping the reference's 10:2 split for
/// the 12px face (its metric getters report 10px ascent, 2px descent).
fn font_ascent_px(height: f32) -> f32 {
    (height * 5.0 / 6.0).round()
}

pub async fn get_font(_: &mut dyn WIPICContext, face: i32, size: i32, style: i32) -> Result<i32> {
    // The reference picks one of the seven faces its font module carries from
    // the size flag, and falls back to the default face for a flag that names
    // none. With the BIOS faces installed we do the same, and every metric
    // below then reports the face the title was actually handed.
    let height = match bitmap_font::face_for_size(size as u32) {
        Some(face) => face.height as i32,
        None => font_size_px(size),
    };
    tracing::debug!("MC_grpGetFont({face}, {size}, {style}) -> {height}px");

    // The handle is the pixel height; SetContext stores it, and the draw/measure
    // paths read it back (see font_handle_height).
    Ok(height)
}

pub async fn get_font_height(_: &mut dyn WIPICContext, font: i32) -> Result<i32> {
    tracing::trace!("MC_grpGetFontHeight({font})");

    if let Some(face) = bitmap_font::face_for_height(font as u32) {
        return Ok(face.height as i32);
    }

    Ok(font_handle_height(font) as i32)
}

pub async fn get_font_ascent(_: &mut dyn WIPICContext, font: i32) -> Result<i32> {
    tracing::trace!("MC_grpGetFontAscent({font})");

    if let Some(face) = bitmap_font::face_for_height(font as u32) {
        return Ok(face.ascent as i32);
    }

    Ok(font_ascent_px(font_handle_height(font)) as i32)
}

pub async fn get_font_descent(_: &mut dyn WIPICContext, font: i32) -> Result<i32> {
    tracing::trace!("MC_grpGetFontDescent({font})");

    if let Some(face) = bitmap_font::face_for_height(font as u32) {
        return Ok(face.descent as i32);
    }

    let height = font_handle_height(font);
    Ok((height - font_ascent_px(height)) as i32)
}

pub async fn get_string_width(context: &mut dyn WIPICContext, font: i32, ptr_string: WIPICWord, length: i32) -> Result<i32> {
    tracing::trace!("MC_grpGetStringWidth({font}, {ptr_string:#x}, {length})");

    let string = read_wipi_string(context, ptr_string, length)?;
    if let Some(face) = bitmap_font::face_for_height(font as u32) {
        return Ok(face.string_width(&string) as i32);
    }

    Ok(string_width_px(&string, font_handle_height(font)) as i32)
}

/// Read a UTF-16LE WIPI string. `length == -1` means NUL-terminated (a `0`
/// code unit); `length >= 0` reads exactly that many 16-bit code units. This is
/// the wide-character counterpart to `read_wipi_string`; the reference's
/// `MC_grpGetUnicodeStringWidth` takes UCS-2 rather than the EUC-KR of the
/// byte-string calls.
fn read_wipi_unicode_string(context: &mut dyn WIPICContext, ptr: WIPICWord, length: i32) -> Result<String> {
    if ptr == 0 {
        return Ok(String::new());
    }

    let units: Vec<u16> = if length >= 0 {
        let mut buf = vec![0u8; (length as usize) * 2];
        context.read_bytes(ptr, &mut buf)?;
        buf.as_chunks::<2>().0.iter().copied().map(u16::from_le_bytes).collect()
    } else {
        let mut out = Vec::new();
        let mut addr = ptr;
        loop {
            let unit: u16 = read_generic(context, addr)?;
            if unit == 0 {
                break;
            }
            out.push(unit);
            addr += 2;
        }
        out
    };

    Ok(String::from_utf16_lossy(&units))
}

/// `MC_grpGetUnicodeStringWidth(font, ustr, len)` - the UCS-2 counterpart to
/// `MC_grpGetStringWidth`. A title that lays out Unicode text measures it with
/// this; left unmapped it returned the diagnostic-stub 0, collapsing every such
/// string to zero width so the glyphs stacked on one another.
pub async fn get_unicode_string_width(context: &mut dyn WIPICContext, font: i32, ptr_string: WIPICWord, length: i32) -> Result<i32> {
    tracing::trace!("MC_grpGetUnicodeStringWidth({font}, {ptr_string:#x}, {length})");

    let string = read_wipi_unicode_string(context, ptr_string, length)?;
    if let Some(face) = bitmap_font::face_for_height(font as u32) {
        return Ok(face.string_width(&string) as i32);
    }

    Ok(string_width_px(&string, font_handle_height(font)) as i32)
}

pub async fn draw_string(
    context: &mut dyn WIPICContext,
    dst: WIPICIndirectPtr,
    x: i32,
    y: i32,
    ptr_string: WIPICWord,
    length: i32,
    pgc: WIPICWord,
) -> Result<()> {
    tracing::debug!("MC_grpDrawString({:#x}, {x}, {y}, {ptr_string:#x}, {length}, {pgc:#x})", dst.0);

    let string = read_wipi_string(context, ptr_string, length)?;

    draw_text(context, dst, x, y, &string, pgc).await
}

/// `MC_grpDrawUnicodeString(dst, x, y, ustr, len, pgc)` - the UCS-2 counterpart
/// to `MC_grpDrawString`.
///
/// The same text in the same face; only how the title spells it differs.
/// `MC_grpGetUnicodeStringWidth` already measures these, so a title that lays
/// out Unicode text and then draws it now gets both halves.
pub async fn draw_unicode_string(
    context: &mut dyn WIPICContext,
    dst: WIPICIndirectPtr,
    x: i32,
    y: i32,
    ptr_string: WIPICWord,
    length: i32,
    pgc: WIPICWord,
) -> Result<()> {
    tracing::debug!("MC_grpDrawUnicodeString({:#x}, {x}, {y}, {ptr_string:#x}, {length}, {pgc:#x})", dst.0);

    let string = read_wipi_unicode_string(context, ptr_string, length)?;

    draw_text(context, dst, x, y, &string, pgc).await
}

/// Draws text into a framebuffer in the face the graphics context selected.
///
/// Shared by the byte-string and the UCS-2 call, which differ only in how the
/// characters were spelled in guest memory.
async fn draw_text(context: &mut dyn WIPICContext, dst: WIPICIndirectPtr, x: i32, y: i32, string: &str, pgc: WIPICWord) -> Result<()> {
    if string.is_empty() {
        return Ok(());
    }

    let framebuffer = FrameBuffer(read_generic(context, context.data_ptr(dst)?)?);
    let gctx = read_context(context, context.graphics_context_layout(), pgc)?;
    let (offset_x, offset_y) = context_offset(&gctx);
    let (x, y) = (x + offset_x, y + offset_y);

    let clip = Clip {
        x: 0,
        y: 0,
        width: framebuffer.0.width,
        height: framebuffer.0.height,
    }
    .intersect(&context_clip(&gctx));

    let color = context_color(&framebuffer, &gctx);

    // The two platforms put `y` in different places, and the reference spells
    // both out: LGT's `MC_grpDrawString` takes the top of the glyph box and
    // derives the baseline from it (`baseline := y + face.Ascent`), KTF's takes
    // the baseline itself (`baseline := registers[2]`). Drawing a KTF title's
    // text from the top puts every string an ascent too low - which is what sat
    // 격투가's menu labels against the bottom of their own boxes and cut the
    // second line off its dialogue.
    //
    // A handset platform is the one whose indirect pointers are handles rather
    // than addresses, the same question `new_screen_surface` asks.
    let baseline_origin = context.data_ptr(dst)? != dst.0;

    // The handset's own face when the BIOS supplied one, drawn a pixel at a
    // time exactly as it is stored. The face is the one the title selected with
    // `SetContext(font)`, so a heading it asked `MC_grpGetFont` for a larger
    // size for is drawn - and measured - in that size.
    if let Some(face) = bitmap_font::face_for_height(gctx.font) {
        let top = if baseline_origin { y - face.ascent as i32 } else { y };
        let mut canvas = framebuffer.canvas(context)?;
        draw_bitmap_string(&mut **canvas, &face, string, x, top, color, clip);
        canvas.flush()?;

        return Ok(());
    }

    // The size the title selected with SetContext(font); 0 keeps the default face.
    let font_height = font_handle_height(gctx.font as i32);
    let baseline = font_ascent_px(font_height);
    let top = if baseline_origin { y - baseline as i32 } else { y };

    let mut canvas = framebuffer.canvas(context)?;
    canvas.draw_text(string, x, top, font_height, baseline, TextAlignment::Left, color, clip);

    // `flush` writes back only the glyph pixels themselves (see write_diff), so
    // a background the title blitted straight into this buffer shows through the
    // gaps between and around the letters instead of being re-stamped black.
    canvas.flush()?;

    Ok(())
}

/// The largest encoding the reference will hand back, and the same limit here.
const MAX_ENCODED_IMAGE_BYTES: usize = 0x0200_0000;

/// `MC_grpEncodeImage(src, x, y, w, h, out_len)` - a rectangle of a framebuffer
/// as an image file.
///
/// Answers the address of a freshly allocated guest buffer holding the encoded
/// bytes and writes their length through `out_len`, or 0 when it cannot, in
/// which case the length it already cleared stays 0. The buffer is the title's
/// to free.
///
/// The contract is the reference emulator's, read out of
/// `ktf.ktfWIPICGraphicsEncodeImage`: six arguments; `*out_len` is cleared
/// before anything else and only written again on success; `x` and `y` must not
/// be negative, `w` and `h` must be positive, and `x + w` / `y + h` must stay
/// inside the framebuffer; the encoding is `image/bmp`; and an empty result, or
/// one past 32 MiB, is refused.
///
/// The BMP itself is 24-bit bottom-up BGR with rows padded to four bytes,
/// written by the same rules as `org.kwis.msp.lcdui.Graphics.encodeImage`, which
/// was derived from the same native encoder - so a title that saves a screenshot
/// through either door gets the same file.
pub async fn encode_image(
    context: &mut dyn WIPICContext,
    src: WIPICIndirectPtr,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    out_len: WIPICWord,
) -> Result<WIPICIndirectPtr> {
    tracing::debug!("MC_grpEncodeImage({:#x}, {x}, {y}, {width}, {height}, {out_len:#x})", src.0);

    // Cleared first, so a caller that only reads the length sees 0 on every
    // failure below without this having to remember to write it again.
    if out_len != 0 {
        write_generic(context, out_len, 0u32)?;
    }

    if src.0 == 0 {
        return Ok(WIPICIndirectPtr(0));
    }

    let framebuffer = FrameBuffer(read_generic(context, context.data_ptr(src)?)?);

    if x < 0 || y < 0 || width <= 0 || height <= 0 {
        return Ok(WIPICIndirectPtr(0));
    }

    let right = (x as i64) + width as i64;
    let bottom = (y as i64) + height as i64;
    if right > framebuffer.0.width as i64 || bottom > framebuffer.0.height as i64 {
        return Ok(WIPICIndirectPtr(0));
    }

    // `image` has no answer for anything but 16- and 32-bit pixels, and a title
    // asking about a framebuffer it built some other way should be told no
    // rather than bringing the run down.
    if framebuffer.0.bpp != 16 && framebuffer.0.bpp != 32 {
        tracing::warn!("MC_grpEncodeImage: nothing to encode from a {}-bit framebuffer", framebuffer.0.bpp);

        return Ok(WIPICIndirectPtr(0));
    }

    let encoded = encode_bmp(&*framebuffer.image(context)?, x, y, width as usize, height as usize);
    if encoded.is_empty() || encoded.len() > MAX_ENCODED_IMAGE_BYTES {
        return Ok(WIPICIndirectPtr(0));
    }

    let memory = context.alloc(encoded.len() as WIPICWord)?;
    let address = context.data_ptr(memory)?;
    context.write_bytes(address, &encoded)?;

    if out_len != 0 {
        write_generic(context, out_len, encoded.len() as u32)?;
    }

    tracing::debug!("MC_grpEncodeImage -> {address:#x}, {} bytes", encoded.len());

    Ok(memory)
}

/// A rectangle of an image as a 24-bit BMP file.
///
/// Bottom-up with four-byte row padding, and the 16-bit source channels widened
/// by masking rather than by replicating their low bits - which is what the
/// native encoder does, and what the WIPI-Java `encodeImage` here already did.
fn encode_bmp(image: &dyn Image, x: i32, y: i32, width: usize, height: usize) -> Vec<u8> {
    let row_stride = (width * 3 + 3) & !3;
    let image_size = row_stride * height;
    let file_size = image_size + 54;

    let mut out = vec![0u8; file_size];

    // BITMAPFILEHEADER
    out[0] = b'B';
    out[1] = b'M';
    out[2..6].copy_from_slice(&(file_size as u32).to_le_bytes());
    out[10..14].copy_from_slice(&54u32.to_le_bytes());

    // BITMAPINFOHEADER
    out[14..18].copy_from_slice(&40u32.to_le_bytes());
    out[18..22].copy_from_slice(&(width as i32).to_le_bytes());
    out[22..26].copy_from_slice(&(height as i32).to_le_bytes());
    out[26..28].copy_from_slice(&1u16.to_le_bytes());
    out[28..30].copy_from_slice(&24u16.to_le_bytes());
    out[30..34].copy_from_slice(&0u32.to_le_bytes());
    out[34..38].copy_from_slice(&(image_size as u32).to_le_bytes());

    for output_row in 0..height {
        let source_y = y + (height - 1 - output_row) as i32;
        let destination_row = 54 + output_row * row_stride;

        for column in 0..width {
            let source_x = x + column as i32;
            if source_x < 0 || source_y < 0 || source_x as u32 >= image.width() || source_y as u32 >= image.height() {
                continue;
            }

            let pixel = image.get_pixel(source_x, source_y);

            let destination = destination_row + column * 3;
            out[destination] = pixel.b & 0xf8;
            out[destination + 1] = pixel.g & 0xfc;
            out[destination + 2] = pixel.r & 0xf8;
        }
    }

    out
}

pub async fn repaint(context: &mut dyn WIPICContext, lcd: i32, x: i32, y: i32, width: i32, height: i32) -> Result<()> {
    tracing::debug!("MC_grpRepaint({lcd}, {x}, {y}, {width}, {height})");

    let platform = context.system().platform();
    let screen = platform.screen();
    screen.request_redraw().unwrap();

    Ok(())
}

/// Row length in bytes and the stride to advance by, or `None` when the call
/// asks for nothing that can be delivered.
///
/// `ipl` is a destination stride, but a handset asked less of it than the name
/// suggests: LGT's own runtime checks only that it is positive and then
/// discards it, writing rows packed at `w * 4`. Titles are written against
/// that. Zenonia reads single pixels with `w = 1, ipl = 1`, and rejecting
/// those left it reading uninitialised stack as pixels - which is what its
/// collision checks were deciding on.
///
/// A stride wide enough to be one is still honoured, since nothing says the
/// other handsets discarded it too; anything smaller falls back to packed.
fn destination_stride(w: i32, h: i32, ipl: i32) -> Option<(i32, i32)> {
    if w <= 0 || h <= 0 {
        return None;
    }
    if ipl <= 0 {
        tracing::warn!("MC_grpGetRGBPixels: invalid ipl {ipl}");
        return None;
    }

    let row_bytes = i32::try_from((w as i64).checked_mul(4)?).ok()?;

    Some((row_bytes, ipl.max(row_bytes)))
}

#[allow(clippy::too_many_arguments)]
pub async fn get_rgb_pixels(
    context: &mut dyn WIPICContext,
    src: WIPICIndirectPtr,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    pd: WIPICWord,
    ipl: i32,
) -> Result<()> {
    tracing::debug!("MC_grpGetRGBPixels({:#x}, {x}, {y}, {w}, {h}, {pd:#x}, {ipl})", src.0);

    let Some((row_bytes, ipl)) = destination_stride(w, h, ipl) else {
        return Ok(());
    };

    let framebuffer = FrameBuffer(read_generic(context, context.data_ptr(src)?)?);
    let image = framebuffer.image(context)?;

    let mut row = vec![0u8; row_bytes as usize];
    for dy in 0..h {
        for dx in 0..w {
            let sx = x + dx;
            let sy = y + dy;
            let color = if sx < 0 || sy < 0 || sx >= image.width() as i32 || sy >= image.height() as i32 {
                Color { a: 0, r: 0, g: 0, b: 0 }
            } else {
                image.get_pixel(sx, sy)
            };
            // WIPI spec: pixels are 0x00RRGGBB (top byte zero).
            let rgb = Rgb8Pixel::from_color(color);
            let off = (dx as usize) * 4;
            row[off..off + 4].copy_from_slice(&rgb.to_le_bytes());
        }
        let row_offset = match (dy as u32).checked_mul(ipl as u32) {
            Some(n) => n,
            None => {
                tracing::warn!("MC_grpGetRGBPixels: row offset overflow (dy={dy}, ipl={ipl})");
                return Ok(());
            }
        };
        let dst_addr = match pd.checked_add(row_offset) {
            Some(n) => n,
            None => {
                tracing::warn!("MC_grpGetRGBPixels: destination address overflow (pd={pd:#x}, row_offset={row_offset})");
                return Ok(());
            }
        };
        context.write_bytes(dst_addr, &row)?;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn set_rgb_pixels(
    context: &mut dyn WIPICContext,
    dst: WIPICIndirectPtr,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    psrc: WIPICWord,
    ibpl: i32,
    pgc: WIPICWord,
) -> Result<()> {
    tracing::debug!("MC_grpSetRGBPixels({:#x}, {x}, {y}, {w}, {h}, {psrc:#x}, {ibpl})", dst.0);

    if w <= 0 || h <= 0 {
        return Ok(());
    }
    let row_bytes = match (w as usize).checked_mul(4) {
        Some(n) => n,
        None => {
            tracing::warn!("MC_grpSetRGBPixels: row size overflow (w={w})");
            return Ok(());
        }
    };
    if ibpl < row_bytes as i32 {
        tracing::warn!("MC_grpSetRGBPixels: invalid ibpl {ibpl} (need >= {row_bytes})");
        return Ok(());
    }
    let total_bytes = match row_bytes.checked_mul(h as usize) {
        Some(n) => n,
        None => {
            tracing::warn!("MC_grpSetRGBPixels: total size overflow (w={w}, h={h})");
            return Ok(());
        }
    };

    let mut buf = vec![0u8; total_bytes];
    for dy in 0..h {
        let off = (dy as usize) * row_bytes;
        let row_offset = match (dy as u32).checked_mul(ibpl as u32) {
            Some(n) => n,
            None => {
                tracing::warn!("MC_grpSetRGBPixels: row offset overflow (dy={dy}, ibpl={ibpl})");
                return Ok(());
            }
        };
        let src_addr = match psrc.checked_add(row_offset) {
            Some(n) => n,
            None => {
                tracing::warn!("MC_grpSetRGBPixels: source address overflow (psrc={psrc:#x}, row_offset={row_offset})");
                return Ok(());
            }
        };
        context.read_bytes(src_addr, &mut buf[off..off + row_bytes])?;
    }

    let gctx = read_context(context, context.graphics_context_layout(), pgc)?;
    let clip = context_clip(&gctx);

    let framebuffer = FrameBuffer(read_generic(context, context.data_ptr(dst)?)?);
    let mut canvas = framebuffer.canvas(context)?;
    for dy in 0..h {
        for dx in 0..w {
            if !clip.allows(x + dx, y + dy) {
                continue;
            }

            let off = ((dy as usize) * (w as usize) + dx as usize) * 4;
            // WIPI spec: pixels are 0x00RRGGBB.
            let rgb = u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]);
            let color = Rgb8Pixel::to_color(rgb);
            canvas.put_pixel(x + dx, y + dy, color);
        }
    }
    canvas.flush()?;

    Ok(())
}

pub async fn get_image_framebuffer(_context: &mut dyn WIPICContext, image: WIPICIndirectPtr) -> Result<WIPICIndirectPtr> {
    tracing::debug!("MC_grpGetImageFrameBuffer({:#x})", image.0);

    // WIPICImage starts with `img: WIPICFramebuffer` at offset 0,
    // so the image handle doubles as a framebuffer handle.
    Ok(image)
}

/// A property of an image, or nothing at all when there is no image.
///
/// A null handle is a slot a title never filled, not a fault. 드래곤아이즈2 lays
/// its character-select screen out from a table of images and asks every entry
/// its width, including the one it leaves empty; the handset reads the zero at
/// address zero and answers nothing, where this walked from that address and
/// took the run down. Size is what a null answers to, so it measures as
/// nothing and [`draw_image`] draws it as nothing.
pub async fn get_image_property(context: &mut dyn WIPICContext, image: WIPICIndirectPtr, property: i32) -> Result<i32> {
    tracing::debug!("MC_grpGetImageProperty({:#x}, {property})", image.0);

    if image.0 == 0 {
        return Ok(0);
    }

    let image: WIPICImage = read_generic(context, context.data_ptr(image)?)?;

    Ok(match property {
        4 => image.img.width as _,
        5 => image.img.height as _,
        _ => {
            tracing::warn!("unknown property {property}");
            0
        }
    })
}

pub async fn draw_rect(context: &mut dyn WIPICContext, dst: WIPICIndirectPtr, x: i32, y: i32, w: i32, h: i32, pgc: WIPICWord) -> Result<()> {
    tracing::debug!("MC_grpDrawRect({:#x}, {x}, {y}, {w}, {h}, {pgc:#x})", dst.0);

    if w <= 0 || h <= 0 {
        return Ok(());
    }

    let framebuffer = FrameBuffer(read_generic(context, context.data_ptr(dst)?)?);
    let gctx = read_context(context, context.graphics_context_layout(), pgc)?;
    let (offset_x, offset_y) = context_offset(&gctx);
    let (x, y) = (x + offset_x, y + offset_y);
    let mut canvas = framebuffer.canvas(context)?;

    let clip = Clip {
        x: x as _,
        y: y as _,
        width: w as _,
        height: h as _,
    }
    .intersect(&context_clip(&gctx));

    let color = context_color(&framebuffer, &gctx);
    canvas.draw_rect(x as _, y as _, w as _, h as _, color, clip);
    canvas.flush()?;

    Ok(())
}

pub async fn draw_line(context: &mut dyn WIPICContext, dst: WIPICIndirectPtr, x1: i32, y1: i32, x2: i32, y2: i32, pgc: WIPICWord) -> Result<()> {
    tracing::debug!("MC_grpDrawLine({:#x}, {x1}, {y1}, {x2}, {y2}, {pgc:#x})", dst.0);

    let framebuffer = FrameBuffer(read_generic(context, context.data_ptr(dst)?)?);
    let gctx = read_context(context, context.graphics_context_layout(), pgc)?;
    let (offset_x, offset_y) = context_offset(&gctx);
    let (x1, y1, x2, y2) = (x1 + offset_x, y1 + offset_y, x2 + offset_x, y2 + offset_y);
    let context_clip = context_clip(&gctx);
    let color = context_color(&framebuffer, &gctx);

    // A line along an axis is a one-pixel-thick rectangle, and writing it as
    // one skips staging the surface. This is the call a C engine draws its
    // sprites with - it lays each row of a sprite as a run - and 지크2 makes
    // fifty thousand of them a second. Bresenham on a line with no slope walks
    // exactly the pixels between its ends and nothing else, which is what a
    // fill of that span covers, so the two agree pixel for pixel.
    //
    // A sloped line still goes through the canvas: its pixels are the
    // rasteriser's to choose, and standing in for it would be guessing at them.
    if color.a == 0xff && (x1 == x2 || y1 == y2) {
        let (left, top) = (x1.min(x2), y1.min(y2));
        let width = (x1.max(x2) as i64 - left as i64) + 1;
        let height = (y1.max(y2) as i64 - top as i64) + 1;

        if let (Ok(width), Ok(height)) = (i32::try_from(width), i32::try_from(height)) {
            // The span the clip leaves is a rectangle too, so the fast path
            // still covers exactly the pixels Bresenham would have.
            let Some((left, top, width, height)) = clipped_rect(&context_clip, left, top, width, height) else {
                return Ok(());
            };

            if framebuffer.fill_rect_direct(context, left, top, width as _, height as _, color)? {
                return Ok(());
            }
        }
    }

    let mut canvas = framebuffer.canvas(context)?;

    let clip = Clip {
        x: 0,
        y: 0,
        width: framebuffer.0.width as _,
        height: framebuffer.0.height as _,
    }
    .intersect(&context_clip);

    canvas.draw_line(x1 as _, y1 as _, x2 as _, y2 as _, color, clip);
    canvas.flush()?;

    Ok(())
}

pub async fn post_event(context: &mut dyn WIPICContext, id: i32, r#type: i32, param1: i32, param2: i32) -> Result<i32> {
    tracing::debug!("MC_grpPostEvent({id}, {type}, {param1}, {param2})");

    context.system().event_queue().push(Event::Notify { r#type, param1, param2 });

    Ok(0)
}

// it's not documented api, but lgt apps gets pointer via api call
/// What the reference's framebuffer getters answer for a handle of zero.
///
/// `wipic_get_frame_pointer` (@0x1ad7b0), `_width` (@0x1aaea4), `_height`
/// (@0x1aacd8) and `_bpl` (@0x1ac044) all open by testing the handle and
/// returning before they touch it - `cmp r0, #0` / `mvneq r0, #0` in two of
/// them, `subs`/`subeq r0, r0, #1` in the others. Every one of them hands back
/// minus one.
///
/// A title reaches them with zero. These getters sit in a direct-blit inner
/// loop, called with whatever register happens to be live rather than with a
/// framebuffer, which is the same reason `get_framebuffer_bpp` ignores its
/// argument outright. 열혈택시 does it while loading its images and, with the
/// handle dereferenced instead, the read of address zero failed the whole VM
/// rather than the one call - `net.wie.WieError: Invalid memory access;
/// address: 0` before its first frame.
pub const NO_FRAMEBUFFER: i32 = -1;

pub async fn get_framebuffer_pointer(context: &mut dyn WIPICContext, framebuffer: WIPICIndirectPtr) -> Result<WIPICWord> {
    tracing::debug!("MC_GRP_GET_FRAME_BUFFER_POINTER({:#x})", framebuffer.0);

    if framebuffer.0 == 0 {
        return Ok(NO_FRAMEBUFFER as WIPICWord);
    }

    let handle = framebuffer;
    let framebuffer: WIPICFramebuffer = read_generic(context, context.data_ptr(handle)?)?;

    Ok(framebuffer.buf.0 - screen_pointer_lead(context, handle.0, framebuffer.bpl))
}

/// Bytes between the pointer the screen framebuffer hands a title and the
/// drawing area it reports - the status strip - and zero for every other
/// framebuffer. See `new_screen_surface`.
///
/// Generic over the reader so a platform's synchronous fast path for
/// `MC_grpGetFrameBufferPointer` reports the same address this one does.
pub fn screen_pointer_lead<R>(reader: &R, handle: WIPICWord, bpl: WIPICWord) -> WIPICWord
where
    R: ?Sized + wie_util::ByteRead,
{
    let screen: u32 = read_generic(reader, SCREEN_FRAMEBUFFER_PTR).unwrap_or(0);
    if screen == 0 || screen != handle {
        return 0;
    }

    read_generic::<u32, _>(reader, ANNUNCIATOR_ROWS_PTR).unwrap_or(0) * bpl
}

pub async fn get_framebuffer_width(context: &mut dyn WIPICContext, framebuffer: WIPICIndirectPtr) -> Result<i32> {
    tracing::debug!("MC_GRP_GET_FRAME_BUFFER_WIDTH({:#x})", framebuffer.0);

    if framebuffer.0 == 0 {
        return Ok(NO_FRAMEBUFFER);
    }

    let framebuffer: WIPICFramebuffer = read_generic(context, context.data_ptr(framebuffer)?)?;

    Ok(framebuffer.width as _)
}

pub async fn get_framebuffer_height(context: &mut dyn WIPICContext, framebuffer: WIPICIndirectPtr) -> Result<i32> {
    tracing::debug!("MC_GRP_GET_FRAME_BUFFER_HEIGHT({:#x})", framebuffer.0);

    if framebuffer.0 == 0 {
        return Ok(NO_FRAMEBUFFER);
    }

    let framebuffer: WIPICFramebuffer = read_generic(context, context.data_ptr(framebuffer)?)?;

    Ok(framebuffer.height as _)
}

pub async fn get_framebuffer_bpl(context: &mut dyn WIPICContext, framebuffer: WIPICIndirectPtr) -> Result<i32> {
    tracing::debug!("MC_GRP_GET_FRAME_BUFFER_BPL({:#x})", framebuffer.0);

    if framebuffer.0 == 0 {
        return Ok(NO_FRAMEBUFFER);
    }

    let framebuffer: WIPICFramebuffer = read_generic(context, context.data_ptr(framebuffer)?)?;

    Ok(framebuffer.bpl as _)
}

pub async fn get_framebuffer_bpp(_context: &mut dyn WIPICContext, framebuffer: WIPICIndirectPtr) -> Result<i32> {
    tracing::debug!("MC_GRP_GET_FRAME_BUFFER_BPP({:#x})", framebuffer.0);

    // The vendor `wipic_get_frame_bpp` ignores its argument and returns the
    // display's depth from a global. Titles rely on that: their direct-blit
    // inner loop calls this with whatever register happens to be live - the
    // frame pointer, the width - not a framebuffer handle, then uses the result
    // as the pixel stride. Dereferencing that argument as a handle here read a
    // garbage struct and returned a garbage depth, so a title's glyph and
    // sprite writes landed at the wrong offset and never appeared while its
    // `MC_grpFillRect` panels, which never touch this call, drew fine. Every
    // framebuffer this runtime hands out is `FRAMEBUFFER_DEPTH`, so report it
    // directly and ignore the argument, as the vendor does.
    Ok(FRAMEBUFFER_DEPTH as _)
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use wie_util::{ByteWrite, read_generic, write_generic};

    use wie_backend::canvas::{ArgbPixel, Color, Image, PixelType, Rgb565Pixel, VecImageBuffer};

    use super::ContextLayout;
    use super::WIPICGraphicsContextIdx as Idx;
    use super::{
        create_image, destination_stride, destroy_image, draw_image, draw_string, get_context, get_image_property, get_string_width,
        get_unicode_string_width, init_context, set_context, surface_content, surface_thumbnail,
    };
    use crate::context::{WIPICContext, test::TestContext};

    /// A flush that names part of the frame leaves the rest of the panel
    /// standing.
    ///
    /// LOA-혼돈의 서곡's gameplay loop is the case: it copies a whole 240x320
    /// back buffer over the screen buffer, redraws the HP and SP bars, and
    /// flushes `240x295`. Its status bar lives in the rows below that and was
    /// drawn and flushed in full on an earlier frame, so the panel is where it
    /// still is. Shown the whole frame buffer, it flickered at whatever rate
    /// the title happened to redraw it.
    #[test]
    fn a_flush_that_names_part_of_the_frame_keeps_the_rest_of_the_panel() {
        let mut panel = None;

        // Two rows of a 4x2 16bpp frame, each pixel numbered so a row that
        // moved is visible.
        let first: Vec<u8> = (0..16u8).collect();
        let shown = super::panel_after_flush(&mut panel, &first, 4, 2, 2, 0, 0, 4, 2);

        // The first flush covers the frame, so the frame itself is what goes up
        // - and is what the panel now holds.
        assert_eq!(shown, None);

        // Now a frame whose every byte differs, flushed one row high.
        let second: Vec<u8> = (0..16u8).map(|byte| byte + 100).collect();
        let shown = super::panel_after_flush(&mut panel, &second, 4, 2, 2, 0, 0, 4, 1).expect("a panel");

        assert_eq!(&shown[..8], &second[..8], "the flushed row is the new frame's");
        assert_eq!(&shown[8..], &first[8..], "and the row below it is what the full flush left");

        // A third frame, flushed the same way, leaves that row alone again.
        let third: Vec<u8> = (0..16u8).map(|byte| byte + 200).collect();
        let shown = super::panel_after_flush(&mut panel, &third, 4, 2, 2, 0, 0, 4, 1).expect("a panel");

        assert_eq!(&shown[..8], &third[..8]);
        assert_eq!(&shown[8..], &first[8..]);
    }

    /// A flush that covers the frame becomes the panel, and is what a later
    /// partial flush keeps.
    ///
    /// This is the half the first attempt at it got wrong: a full flush threw
    /// the panel away instead of storing it, so every partial flush after one
    /// built on a blank panel and the band stayed as black as before.
    #[test]
    fn a_full_flush_becomes_the_panel() {
        let mut panel = None;

        let full: Vec<u8> = (0..16u8).collect();
        assert_eq!(
            super::panel_after_flush(&mut panel, &full, 4, 2, 2, 0, 0, 4, 2),
            None,
            "a frame that covers the panel is shown as it is"
        );
        assert!(panel.is_some(), "and is kept, for the flush after it");

        let partial: Vec<u8> = (0..16u8).map(|byte| byte + 100).collect();
        let shown = super::panel_after_flush(&mut panel, &partial, 4, 2, 2, 0, 0, 4, 1).expect("a panel");

        assert_eq!(&shown[..8], &partial[..8], "the flushed row");
        assert_eq!(&shown[8..], &full[8..], "and the row the full flush left there");
    }

    /// A rectangle this cannot read shows the frame rather than an empty panel.
    #[test]
    fn an_unreadable_flush_shows_the_frame() {
        let frame: Vec<u8> = (0..16u8).collect();

        for (x, y, w, h) in [(0, 0, 0, 2), (0, 0, 4, 0), (0, 0, -1, 2)] {
            let mut panel = None;
            assert_eq!(
                super::panel_after_flush(&mut panel, &frame, 4, 2, 2, x, y, w, h),
                None,
                "({x},{y},{w},{h})"
            );
        }

        // A depth with no pixel type behind it, and a frame shorter than it says.
        let mut panel = None;
        assert_eq!(super::panel_after_flush(&mut panel, &frame, 4, 2, 3, 0, 0, 4, 1), None);
        assert_eq!(super::panel_after_flush(&mut panel, &frame[..4], 4, 2, 2, 0, 0, 4, 1), None);
    }

    /// A frame of another shape is another title's, and the panel goes with it.
    #[test]
    fn a_panel_of_another_shape_is_not_kept_under_this_frame() {
        let mut panel = None;

        let wide: Vec<u8> = (0..16u8).collect();
        super::panel_after_flush(&mut panel, &wide, 4, 2, 2, 0, 0, 4, 1).expect("a panel");

        let tall: Vec<u8> = (0..16u8).map(|byte| byte + 100).collect();
        let shown = super::panel_after_flush(&mut panel, &tall, 2, 4, 2, 0, 0, 2, 1).expect("a panel");

        assert_eq!(shown.len(), 16);
        assert_eq!(&shown[..4], &tall[..4]);
        assert_eq!(&shown[4..], &[0; 12], "the rest is a fresh panel, not the last title's rows");
    }

    /// A 2x2 red PNG, the smallest thing `create_image` will take.
    const TINY_PNG: [u8; 73] = [
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00,
        0x02, 0x08, 0x02, 0x00, 0x00, 0x00, 0xfd, 0xd4, 0x9a, 0x73, 0x00, 0x00, 0x00, 0x10, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf,
        0xc0, 0x00, 0x44, 0x0c, 0x10, 0x0a, 0x00, 0x1f, 0xee, 0x03, 0xfd, 0x8b, 0x5f, 0x14, 0xd4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44,
        0xae, 0x42, 0x60, 0x82,
    ];

    /// Destroying an image gives back everything creating it took, the encoded
    /// bytes the title handed over included.
    ///
    /// A title that keeps loading sprites - LOA-혼돈의 서곡 does, 6645 times in
    /// one 50-second capture - only frees that block itself when the create
    /// failed, so a create that succeeded and a destroy that keeps the block
    /// leak one buffer per image for as long as the title runs.
    #[futures_test::test]
    async fn destroying_an_image_gives_back_the_bytes_it_was_made_from() {
        let mut context = TestContext::new();

        let source = context.alloc(TINY_PNG.len() as u32).unwrap();
        let ptr_source = context.data_ptr(source).unwrap();
        context.write_bytes(ptr_source, &TINY_PNG).unwrap();

        let ptr_image = context.alloc_raw(4).unwrap();

        let before = context.live_allocations().len();
        assert_eq!(create_image(&mut context, ptr_image, source, 0, TINY_PNG.len() as u32).await.unwrap(), 1);
        assert!(context.live_allocations().len() > before);

        let image: super::WIPICWord = read_generic(&context, ptr_image).unwrap();
        destroy_image(&mut context, super::WIPICIndirectPtr(image)).await.unwrap();

        // Only the source and the caller's own image slot were left standing
        // before the create, and both are accounted for now: the slot is still
        // the title's, and the source went with the image.
        assert_eq!(context.live_allocations().len(), before - 1);
        assert!(!context.live_allocations().iter().any(|&(address, _)| address == source.0));
    }

    /// A block the title took back is given back once, by the title, and the
    /// platform does not give it back a second time.
    ///
    /// 겟앰프드 frees it on the very next call after every create. The
    /// allocator then hands that address straight out again - to that title's
    /// own next resource, which becomes its next image - so a second free
    /// releases a block that is live. Its run died a few frames later on a
    /// double free of a framebuffer plane, behind 240 warnings about this one.
    #[futures_test::test]
    async fn a_source_the_title_took_back_is_not_freed_twice() {
        let mut context = TestContext::new();

        let source = context.alloc(TINY_PNG.len() as u32).unwrap();
        let ptr_source = context.data_ptr(source).unwrap();
        context.write_bytes(ptr_source, &TINY_PNG).unwrap();

        let ptr_image = context.alloc_raw(4).unwrap();
        assert_eq!(create_image(&mut context, ptr_image, source, 0, TINY_PNG.len() as u32).await.unwrap(), 1);

        // The title gives the block back itself, the way 겟앰프드 does.
        crate::api::kernel::free(&mut context, source).await.unwrap();
        assert_eq!(context.frees_of(source.0), 1);

        let image: super::WIPICWord = read_generic(&context, ptr_image).unwrap();
        destroy_image(&mut context, super::WIPICIndirectPtr(image)).await.unwrap();

        assert_eq!(
            context.frees_of(source.0),
            1,
            "the destroy left the block alone; only the title gave it back"
        );

        // And it still took back everything that was its own: the planes and
        // the handle it allocated itself.
        assert_eq!(context.frees_of(image), 1);
    }

    /// An image slot a title never filled measures as nothing and draws as
    /// nothing, rather than faulting on the read from address zero.
    ///
    /// 드래곤아이즈2 builds its character-select screen from a table of images
    /// and asks every entry for its size, the empty one included. Asking took
    /// the run down under its own menu.
    #[futures_test::test]
    async fn a_null_image_measures_and_draws_as_nothing() {
        let mut context = TestContext::new();

        for property in [4, 5, 99] {
            assert_eq!(get_image_property(&mut context, super::WIPICIndirectPtr(0), property).await.unwrap(), 0);
        }

        // And drawing it is a call that returns. The destination is never
        // reached, so it need not be a framebuffer.
        draw_image(
            &mut context,
            super::WIPICIndirectPtr(0),
            0,
            0,
            16,
            16,
            super::WIPICIndirectPtr(0),
            0,
            0,
            0,
        )
        .await
        .unwrap();
    }

    /// A pixel operation of the title's own decides what a blit writes.
    ///
    /// 드래곤하트2 plants one and draws its hit effect through it. WIE stored
    /// the pointer and drew as though it were not there, so the effect came
    /// down as an opaque disc over the field. Here the stand-in adds the two
    /// pixels, and the destination has to come out added rather than
    /// overwritten.
    #[futures_test::test]
    async fn a_blit_goes_through_the_titles_own_pixel_operation() {
        let mut context = test_context();

        // Adds the RGB565 channels, which is one of the two 드래곤하트2 plants.
        context.set_guest_function(|_, args| super::pixel_op::additive(args[0] as u16, args[1] as u16) as u32);

        let pgc_handle = context.alloc(core::mem::size_of::<super::WIPICGraphicsContext>() as u32).unwrap();
        let pgc = context.data_ptr(pgc_handle).unwrap();
        init_context(&mut context, pgc).await.unwrap();

        // Two different greys, so a destination read as nothing would show up
        // as the source alone rather than passing by luck.
        let destination = framebuffer_of(&mut context, 2, 2, &[0xff40_4040; 4]).await;
        let source = framebuffer_of(&mut context, 2, 2, &[0xff20_2020; 4]).await;

        let image_handle = context.alloc(core::mem::size_of::<wipi_types::wipic::WIPICImage>() as u32).unwrap();
        let image_address = context.data_ptr(image_handle).unwrap();
        let image = wipi_types::wipic::WIPICImage {
            img: read_generic(&context, context.data_ptr(source).unwrap()).unwrap(),
            mask: super::FrameBuffer::empty().0,
            loop_count: 0,
            delay: 0,
            animated: 0,
            buf: super::WIPICIndirectPtr(0),
            offset: 0,
            current: 0,
            len: 0,
        };
        write_generic(&mut context, image_address, image).unwrap();

        set_context(&mut context, pgc, Idx::PixelopIdx, 0x1234).await.unwrap();

        draw_image(&mut context, destination, 0, 0, 2, 2, image_handle, 0, 0, pgc).await.unwrap();

        let handle = read_generic(&context, context.data_ptr(destination).unwrap()).unwrap();
        let drawn = super::FrameBuffer(handle).image(&mut context).unwrap();

        let expected = Rgb565Pixel::to_color(super::pixel_op::additive(
            Rgb565Pixel::from_color(Color {
                a: 0xff,
                r: 0x40,
                g: 0x40,
                b: 0x40,
            }),
            Rgb565Pixel::from_color(Color {
                a: 0xff,
                r: 0x20,
                g: 0x20,
                b: 0x20,
            }),
        ));

        let out = drawn.get_pixel(0, 0);
        assert_eq!((out.r, out.g, out.b), (expected.r, expected.g, expected.b));
    }

    /// The operation is asked about the source first, then the destination.
    ///
    /// 마스터오브소드4 writes its Korean by blitting jamo out of white-on-magenta
    /// strips through an operation that reads `if (a != white) return a; else
    /// return textColour` - it recolours the one white the strips are drawn in.
    /// `a` can only be the source: asked about the destination it answers the
    /// box the glyph lands on, unchanged, and the title's dialogue came out
    /// empty.
    #[futures_test::test]
    async fn a_pixel_operation_is_asked_about_the_source_first() {
        let mut context = test_context();

        context.set_pixel_op_takes_source_first(true);

        // The shape 마스터오브소드4 plants: the first argument decides, and a
        // white one is answered with a colour neither side carries.
        const RECOLOURED: u32 = 0x001f; // pure blue in RGB565
        context.set_guest_function(|_, args| if args[0] == 0xffff { RECOLOURED } else { args[0] });

        // An address of its own: what an operation turned out to be is
        // remembered across calls, and the whole suite shares that memory.

        let pgc_handle = context.alloc(core::mem::size_of::<super::WIPICGraphicsContext>() as u32).unwrap();
        let pgc = context.data_ptr(pgc_handle).unwrap();
        init_context(&mut context, pgc).await.unwrap();

        let destination = framebuffer_of(&mut context, 1, 1, &[0xff20_2020]).await;
        let source = framebuffer_of(&mut context, 1, 1, &[0xffff_ffff]).await;

        let image_handle = context.alloc(core::mem::size_of::<wipi_types::wipic::WIPICImage>() as u32).unwrap();
        let image_address = context.data_ptr(image_handle).unwrap();
        let image = wipi_types::wipic::WIPICImage {
            img: read_generic(&context, context.data_ptr(source).unwrap()).unwrap(),
            mask: super::FrameBuffer::empty().0,
            loop_count: 0,
            delay: 0,
            animated: 0,
            buf: super::WIPICIndirectPtr(0),
            offset: 0,
            current: 0,
            len: 0,
        };
        write_generic(&mut context, image_address, image).unwrap();

        set_context(&mut context, pgc, Idx::PixelopIdx, 0x5678).await.unwrap();

        draw_image(&mut context, destination, 0, 0, 1, 1, image_handle, 0, 0, pgc).await.unwrap();

        let handle = read_generic(&context, context.data_ptr(destination).unwrap()).unwrap();
        let drawn = super::FrameBuffer(handle).image(&mut context).unwrap();

        let expected = Rgb565Pixel::to_color(RECOLOURED as u16);
        let out = drawn.get_pixel(0, 0);
        assert_eq!((out.r, out.g, out.b), (expected.r, expected.g, expected.b));
    }

    /// A fade of the title's own is recognised and done here, not asked about
    /// per pixel.
    ///
    /// This is the shape LOA-혼돈의 서곡 plants for its story transitions: its
    /// operation @0x122104 unpacks the pixel with `MC_grpGetRGBFromPixel`,
    /// mixes each component towards white or black and packs it again. Asked
    /// once per pixel it cost that title two platform calls and an emulated
    /// call for each of the ~38,000 pixels it blended per frame, which is where
    /// its story screen stopped looking like it was moving.
    ///
    /// The count is what this is really about: forty calls is the probing, and
    /// the four pixels of the blit add none of their own.
    #[futures_test::test]
    async fn a_fade_of_the_titles_own_is_recognised_rather_than_asked_per_pixel() {
        use core::sync::atomic::{AtomicUsize, Ordering};

        static ASKED: AtomicUsize = AtomicUsize::new(0);

        /// Half of the way to black, written the way the title writes it.
        fn faded(pixel: u16) -> u16 {
            let component = |value: u32, max: u32| (value * 255 + max / 2) / max;

            let r = component(((pixel >> 11) & 0x1f) as u32, 0x1f) * 127 / 255;
            let g = component(((pixel >> 5) & 0x3f) as u32, 0x3f) * 127 / 255;
            let b = component((pixel & 0x1f) as u32, 0x1f) * 127 / 255;

            (((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3)) as u16
        }

        let mut context = test_context();

        // KTF, which is where this title ran: the operation is handed the
        // source first, so that is the pixel the fade acts on.
        context.set_pixel_op_takes_source_first(true);

        ASKED.store(0, Ordering::SeqCst);
        context.set_guest_function(|_, args| {
            ASKED.fetch_add(1, Ordering::SeqCst);

            faded(args[0] as u16) as u32
        });

        let pgc_handle = context.alloc(core::mem::size_of::<super::WIPICGraphicsContext>() as u32).unwrap();
        let pgc = context.data_ptr(pgc_handle).unwrap();
        init_context(&mut context, pgc).await.unwrap();

        let destination = framebuffer_of(&mut context, 2, 2, &[0xff40_4040; 4]).await;
        let source = framebuffer_of(&mut context, 2, 2, &[0xffc0_8040; 4]).await;

        let image_handle = context.alloc(core::mem::size_of::<wipi_types::wipic::WIPICImage>() as u32).unwrap();
        let image_address = context.data_ptr(image_handle).unwrap();
        let image = wipi_types::wipic::WIPICImage {
            img: read_generic(&context, context.data_ptr(source).unwrap()).unwrap(),
            mask: super::FrameBuffer::empty().0,
            loop_count: 0,
            delay: 0,
            animated: 0,
            buf: super::WIPICIndirectPtr(0),
            offset: 0,
            current: 0,
            len: 0,
        };
        write_generic(&mut context, image_address, image).unwrap();

        // An address of its own: what an operation turned out to be is
        // remembered across calls, and the whole suite shares that memory.
        set_context(&mut context, pgc, Idx::PixelopIdx, 0x0fade1).await.unwrap();

        draw_image(&mut context, destination, 0, 0, 2, 2, image_handle, 0, 0, pgc).await.unwrap();

        let probed = ASKED.load(Ordering::SeqCst);
        assert_eq!(probed, 40, "the probing, and nothing per pixel");

        let handle = read_generic(&context, context.data_ptr(destination).unwrap()).unwrap();
        let drawn = super::FrameBuffer(handle).image(&mut context).unwrap();

        // The destination is the second pixel here and has no part in it; what
        // lands is the source, faded.
        let expected = Rgb565Pixel::to_color(faded(Rgb565Pixel::from_color(Color {
            a: 0xff,
            r: 0xc0,
            g: 0x80,
            b: 0x40,
        })));

        let out = drawn.get_pixel(0, 0);
        assert_eq!((out.r, out.g, out.b), (expected.r, expected.g, expected.b));

        // And a second blit asks once: the remembered answer is held to a pair
        // before it is used again, because an operation is the title's own code
        // and is free to change what it does. One call, not the four pixels.
        draw_image(&mut context, destination, 0, 0, 2, 2, image_handle, 0, 0, pgc).await.unwrap();
        assert_eq!(ASKED.load(Ordering::SeqCst), probed + 1);
    }

    /// A pixel the image does not have is not a pixel the operation is asked
    /// about.
    ///
    /// Drawing through an operation used to write the whole rectangle, so
    /// 마스터오브소드4's glyphs arrived as blocks of the magenta their strips are
    /// keyed on. The plain path has always skipped these.
    #[futures_test::test]
    async fn a_blit_through_an_operation_leaves_a_transparent_pixel_alone() {
        let mut context = test_context();

        // White whatever it is asked, so this says nothing about which pixel
        // goes first - only which pixels are asked about at all.
        context.set_guest_function(|_, _| 0xffff);

        let pgc_handle = context.alloc(core::mem::size_of::<super::WIPICGraphicsContext>() as u32).unwrap();
        let pgc = context.data_ptr(pgc_handle).unwrap();
        init_context(&mut context, pgc).await.unwrap();

        let destination = framebuffer_of(&mut context, 2, 1, &[0xff20_2020, 0xff20_2020]).await;
        // The colour plane carries both pixels; the mask says only the first is
        // there, the way a decoded glyph strip does.
        let colour = framebuffer_of(&mut context, 2, 1, &[0xffff_ffff, 0xffff_00ff]).await;
        let mask = framebuffer_of(&mut context, 2, 1, &[0xffff_ffff, 0x0000_0000]).await;

        let image_handle = context.alloc(core::mem::size_of::<wipi_types::wipic::WIPICImage>() as u32).unwrap();
        let image_address = context.data_ptr(image_handle).unwrap();
        let image = wipi_types::wipic::WIPICImage {
            img: read_generic(&context, context.data_ptr(colour).unwrap()).unwrap(),
            mask: read_generic(&context, context.data_ptr(mask).unwrap()).unwrap(),
            loop_count: 0,
            delay: 0,
            animated: 0,
            buf: super::WIPICIndirectPtr(0),
            offset: 0,
            current: 0,
            len: 0,
        };
        write_generic(&mut context, image_address, image).unwrap();

        set_context(&mut context, pgc, Idx::PixelopIdx, 0x9abc).await.unwrap();

        draw_image(&mut context, destination, 0, 0, 2, 1, image_handle, 0, 0, pgc).await.unwrap();

        let handle = read_generic(&context, context.data_ptr(destination).unwrap()).unwrap();
        let drawn = super::FrameBuffer(handle).image(&mut context).unwrap();

        let written = drawn.get_pixel(0, 0);
        assert_eq!((written.r, written.g, written.b), (0xff, 0xff, 0xff));

        // Untouched, so it is still exactly what was there rather than what a
        // round trip through RGB565 would have left.
        let untouched = drawn.get_pixel(1, 0);
        assert_eq!((untouched.r, untouched.g, untouched.b), (0x20, 0x20, 0x20));
    }

    /// A fill goes through the operation too, not only a blit.
    ///
    /// 드래곤하트2 lays two hundred rectangles through a live operation in one
    /// capture, so a fill that covers what is under it is as wrong there as a
    /// blit that does.
    #[futures_test::test]
    async fn a_fill_goes_through_the_titles_own_pixel_operation() {
        let mut context = test_context();
        context.set_guest_function(|_, args| super::pixel_op::additive(args[0] as u16, args[1] as u16) as u32);

        let pgc_handle = context.alloc(core::mem::size_of::<super::WIPICGraphicsContext>() as u32).unwrap();
        let pgc = context.data_ptr(pgc_handle).unwrap();
        init_context(&mut context, pgc).await.unwrap();

        let destination = framebuffer_of(&mut context, 2, 2, &[0xff40_4040; 4]).await;

        // A fill colour of its own, and an operation that adds it to what is
        // there. The framebuffer is 32bpp, so the colour is written as one.
        set_context(&mut context, pgc, Idx::FgPixelIdx, 0x0020_2020).await.unwrap();
        set_context(&mut context, pgc, Idx::PixelopIdx, 0x1234).await.unwrap();

        super::fill_rect(&mut context, destination, 0, 0, 2, 2, pgc).await.unwrap();

        let handle = read_generic(&context, context.data_ptr(destination).unwrap()).unwrap();
        let filled = super::FrameBuffer(handle).image(&mut context).unwrap();

        let expected = Rgb565Pixel::to_color(super::pixel_op::additive(
            Rgb565Pixel::from_color(Color {
                a: 0xff,
                r: 0x40,
                g: 0x40,
                b: 0x40,
            }),
            Rgb565Pixel::from_color(Color {
                a: 0xff,
                r: 0x20,
                g: 0x20,
                b: 0x20,
            }),
        ));

        let out = filled.get_pixel(0, 0);
        assert_eq!((out.r, out.g, out.b), (expected.r, expected.g, expected.b));
    }

    /// XOR mode draws through the built-in the reference plants for it, even
    /// though there is no guest address to report back for it.
    #[futures_test::test]
    async fn xor_mode_draws_through_the_built_in_operation() {
        let mut context = test_context();

        let pgc_handle = context.alloc(core::mem::size_of::<super::WIPICGraphicsContext>() as u32).unwrap();
        let pgc = context.data_ptr(pgc_handle).unwrap();
        init_context(&mut context, pgc).await.unwrap();

        let destination = framebuffer_of(&mut context, 2, 2, &[0xff00_0000; 4]).await;

        set_context(&mut context, pgc, Idx::XorModeIdx, 1).await.unwrap();
        super::fill_rect(&mut context, destination, 0, 0, 2, 2, pgc).await.unwrap();

        let handle = read_generic(&context, context.data_ptr(destination).unwrap()).unwrap();
        let filled = super::FrameBuffer(handle).image(&mut context).unwrap();

        // Black inverted is white, whatever colour the fill was.
        let out = filled.get_pixel(0, 0);
        assert!(
            out.r > 0xf0 && out.g > 0xf0 && out.b > 0xf0,
            "black was not inverted: {} {} {}",
            out.r,
            out.g,
            out.b
        );
    }

    /// Where the operation and the transparent pixel sit in the struct.
    ///
    /// A clet that fills its own context - 헬싱 does, and never calls
    /// `MC_grpSetContext` once - puts its operation at `+0x2c` and its
    /// transparent pixel at `+0x1c`. Read the other way round, `0xf81f`
    /// (magenta, the key) was taken for the operation and calling it took the
    /// title down on its first frame.
    #[futures_test::test]
    async fn a_context_a_title_filled_itself_is_read_at_the_right_words() {
        let mut context = test_context();

        let pgc_handle = context.alloc(core::mem::size_of::<super::WIPICGraphicsContext>() as u32).unwrap();
        let pgc = context.data_ptr(pgc_handle).unwrap();
        init_context(&mut context, pgc).await.unwrap();

        // The two words as a title writes them, not through the API.
        const TRANSPARENT: u32 = 0x1c;
        const OPERATION: u32 = 0x2c;
        write_generic(&mut context, pgc + TRANSPARENT, 0xf81fu32).unwrap();
        write_generic(&mut context, pgc + OPERATION, 0x1234u32).unwrap();

        let gctx: super::WIPICGraphicsContext = read_generic(&context, pgc).unwrap();
        assert_eq!(gctx.transparent, 0xf81f);
        assert_eq!(gctx.pixel_op_func_ptr, 0x1234);

        // And the API's own words land where the struct says they do.
        set_context(&mut context, pgc, Idx::PixelopIdx, 0x5678).await.unwrap();
        assert_eq!(read_generic::<u32, _>(&context, pgc + OPERATION).unwrap(), 0x5678);
        assert_eq!(read_generic::<u32, _>(&context, pgc + TRANSPARENT).unwrap(), 0xf81f);
    }

    /// A KTF title fills with the second of the two colour words.
    ///
    /// 헬싱 never calls `MC_grpInitContext` or `MC_grpSetContext`: it fills
    /// its own 0x38 bytes and hands them to every draw, with `0` in the first
    /// colour word and the colour it wants in the second. Read the LGT way
    /// round that is a background it is not drawing with, the foreground is
    /// black, and every fill - which is how the title draws its Korean text -
    /// comes out black on black.
    ///
    /// The same bytes on an LGT handset mean the other thing, so both are
    /// asked here.
    #[futures_test::test]
    async fn a_ktf_title_fills_with_the_second_colour_word() {
        /// The two colour words, as a title writes them.
        const FIRST: u32 = 0x10;
        const SECOND: u32 = 0x14;

        for (layout, wanted) in [(ContextLayout::Ktf, 0x6da0u32), (ContextLayout::Lgt, 0x0000)] {
            let mut context = test_context();
            context.set_graphics_context_layout(layout);

            let destination = framebuffer_of(&mut context, 2, 2, &[0xff00_0000; 4]).await;

            let pgc_handle = context.alloc(core::mem::size_of::<super::WIPICGraphicsContext>() as u32).unwrap();
            let pgc = context.data_ptr(pgc_handle).unwrap();
            init_context(&mut context, pgc).await.unwrap();
            write_generic(&mut context, pgc + FIRST, 0x0000u32).unwrap();
            write_generic(&mut context, pgc + SECOND, 0x6da0u32).unwrap();

            super::fill_rect(&mut context, destination, 0, 0, 2, 2, pgc).await.unwrap();

            let handle = read_generic(&context, context.data_ptr(destination).unwrap()).unwrap();
            let framebuffer = super::FrameBuffer(handle);
            let expected = framebuffer.pixel_to_color(wanted);
            let filled = framebuffer.image(&mut context).unwrap();

            let out = filled.get_pixel(0, 0);
            assert_eq!((out.r, out.g, out.b), (expected.r, expected.g, expected.b), "{layout:?}");
        }
    }

    /// XOR mode lives in the operation slot, and a title reading that slot back
    /// is told there is no operation rather than handed our stand-in.
    #[futures_test::test]
    async fn xor_mode_is_the_operation_slot_and_is_not_handed_back() {
        let mut context = test_context();

        let pgc_handle = context.alloc(core::mem::size_of::<super::WIPICGraphicsContext>() as u32).unwrap();
        let pgc = context.data_ptr(pgc_handle).unwrap();
        init_context(&mut context, pgc).await.unwrap();

        let out = context.alloc_raw(4).unwrap();

        set_context(&mut context, pgc, Idx::XorModeIdx, 1).await.unwrap();
        get_context(&mut context, pgc, Idx::XorModeIdx, out).await.unwrap();
        assert_eq!(read_generic::<u32, _>(&context, out).unwrap(), 1);
        get_context(&mut context, pgc, Idx::PixelopIdx, out).await.unwrap();
        assert_eq!(read_generic::<u32, _>(&context, out).unwrap(), 0, "the stand-in is not an address");

        // Turning it off clears the slot.
        set_context(&mut context, pgc, Idx::XorModeIdx, 0).await.unwrap();
        get_context(&mut context, pgc, Idx::XorModeIdx, out).await.unwrap();
        assert_eq!(read_generic::<u32, _>(&context, out).unwrap(), 0);
    }

    /// A string a title has not set yet is the empty one, and measuring or
    /// drawing it does nothing.
    ///
    /// 드래곤로드EX's loading screen measures one, and reading it from address
    /// zero ended the run with `Invalid memory access; address: 0` under the
    /// title's own loading text.
    #[futures_test::test]
    async fn a_null_string_measures_and_draws_as_nothing() {
        let mut context = TestContext::new();

        for length in [-1, 0, 8] {
            assert_eq!(get_string_width(&mut context, 10, 0, length).await.unwrap(), 0);
            assert_eq!(get_unicode_string_width(&mut context, 10, 0, length).await.unwrap(), 0);
        }

        // And drawing one is a call that returns, not a fault. The destination
        // is never reached, so it need not be a framebuffer.
        draw_string(&mut context, super::WIPICIndirectPtr(0), 0, 0, 0, -1, 0).await.unwrap();
    }

    /// A framebuffer of `pixels` (ARGB, row-major) as the guest holds one: the
    /// indirect pointer a WIPI-C call is handed.
    /// Sets the context's clip to `(x1, y1)..(x2, y2)` the way a title does -
    /// four 32-bit words through `MC_grpSetContext`, the corner one past the
    /// last pixel.
    async fn set_clip(context: &mut TestContext, pgc: u32, x1: u32, y1: u32, x2: u32, y2: u32) {
        let rect = context.alloc(16).unwrap();
        let rect = context.data_ptr(rect).unwrap();
        for (i, value) in [x1, y1, x2, y2].into_iter().enumerate() {
            write_generic(context, rect + (i as u32) * 4, value).unwrap();
        }
        set_context(context, pgc, Idx::ClipIdx, rect).await.unwrap();
    }

    /// A blit lands only where the context's clip allows it, and the pixels that
    /// do land are the ones that were going to land there anyway.
    ///
    /// This is how a clet picks one cell out of a sprite sheet: 짜요짜요타이쿤3
    /// keeps its nine menu labels as a 2x9 grid in a single image, sets the clip
    /// to where the label is to go, and hands `MC_grpDrawImage` the whole sheet
    /// with the destination corner moved back so the cell it wants lines up.
    /// Drawn unclipped, all nine labels landed on the screen at once.
    #[futures_test::test]
    async fn a_blit_lands_only_inside_the_contexts_clip() {
        let mut context = test_context();

        let pgc_handle = context.alloc(core::mem::size_of::<super::WIPICGraphicsContext>() as u32).unwrap();
        let pgc = context.data_ptr(pgc_handle).unwrap();
        init_context(&mut context, pgc).await.unwrap();

        // A 2x2 sheet of four distinct colours over a black destination.
        let destination = framebuffer_of(&mut context, 2, 2, &[0xff00_0000; 4]).await;
        let source = framebuffer_of(&mut context, 2, 2, &[0xfff8_0000, 0xff00_fc00, 0xff00_00f8, 0xffff_ffff]).await;

        let image_handle = context.alloc(core::mem::size_of::<wipi_types::wipic::WIPICImage>() as u32).unwrap();
        let image_address = context.data_ptr(image_handle).unwrap();
        let image = wipi_types::wipic::WIPICImage {
            img: read_generic(&context, context.data_ptr(source).unwrap()).unwrap(),
            mask: super::FrameBuffer::empty().0,
            loop_count: 0,
            delay: 0,
            animated: 0,
            buf: super::WIPICIndirectPtr(0),
            offset: 0,
            current: 0,
            len: 0,
        };
        write_generic(&mut context, image_address, image).unwrap();

        // Only the bottom-right pixel of the destination may be written, so the
        // whole sheet blitted at the origin must leave just its own
        // bottom-right pixel there.
        set_clip(&mut context, pgc, 1, 1, 2, 2).await;
        draw_image(&mut context, destination, 0, 0, 2, 2, image_handle, 0, 0, pgc).await.unwrap();

        let handle = read_generic(&context, context.data_ptr(destination).unwrap()).unwrap();
        let drawn = super::FrameBuffer(handle).image(&mut context).unwrap();

        for (x, y) in [(0, 0), (1, 0), (0, 1)] {
            let untouched = drawn.get_pixel(x, y);
            assert_eq!((untouched.r, untouched.g, untouched.b), (0, 0, 0), "({x}, {y})");
        }

        let written = drawn.get_pixel(1, 1);
        assert_eq!((written.r, written.g, written.b), (0xff, 0xff, 0xff));
    }

    /// The same clip governs a fill, a line and a string - every call that takes
    /// a context draws through it, not only the blits.
    #[futures_test::test]
    async fn a_fill_and_a_line_stop_at_the_contexts_clip() {
        let mut context = test_context();

        let pgc_handle = context.alloc(core::mem::size_of::<super::WIPICGraphicsContext>() as u32).unwrap();
        let pgc = context.data_ptr(pgc_handle).unwrap();
        init_context(&mut context, pgc).await.unwrap();

        let destination = framebuffer_of(&mut context, 2, 2, &[0xff00_0000; 4]).await;

        // White, and a rectangle over the whole surface - only the left column
        // may take it.
        set_context(&mut context, pgc, Idx::FgPixelIdx, 0x00ff_ffff).await.unwrap();
        set_clip(&mut context, pgc, 0, 0, 1, 2).await;
        super::fill_rect(&mut context, destination, 0, 0, 2, 2, pgc).await.unwrap();

        let handle = read_generic(&context, context.data_ptr(destination).unwrap()).unwrap();
        {
            let filled = super::FrameBuffer(handle).image(&mut context).unwrap();
            for y in 0..2 {
                let inside = filled.get_pixel(0, y);
                assert_eq!((inside.r, inside.g, inside.b), (0xff, 0xff, 0xff), "(0, {y})");

                let outside = filled.get_pixel(1, y);
                assert_eq!((outside.r, outside.g, outside.b), (0, 0, 0), "(1, {y})");
            }
        }

        // A horizontal line across the top row, clipped to the right column.
        set_clip(&mut context, pgc, 1, 0, 2, 1).await;
        super::draw_line(&mut context, destination, 0, 0, 1, 0, pgc).await.unwrap();

        let drawn = super::FrameBuffer(handle).image(&mut context).unwrap();
        let outside = drawn.get_pixel(0, 1);
        assert_eq!((outside.r, outside.g, outside.b), (0xff, 0xff, 0xff), "the fill is still there");

        let written = drawn.get_pixel(1, 0);
        assert_eq!((written.r, written.g, written.b), (0xff, 0xff, 0xff));

        let untouched = drawn.get_pixel(1, 1);
        assert_eq!((untouched.r, untouched.g, untouched.b), (0, 0, 0));
    }

    /// A primitive is drawn relative to the context's offset.
    ///
    /// The reference installs op 10 as the origin of the graphics object it
    /// draws through - `wipic_grpContext_to_dgraphics` (@0x1aa2e8) - so a title
    /// that moves its origin and then draws at (0, 0) is drawing at the origin,
    /// not at the corner.
    #[futures_test::test]
    async fn a_primitive_lands_where_the_contexts_offset_puts_it() {
        let mut context = test_context();

        let pgc_handle = context.alloc(core::mem::size_of::<super::WIPICGraphicsContext>() as u32).unwrap();
        let pgc = context.data_ptr(pgc_handle).unwrap();
        init_context(&mut context, pgc).await.unwrap();

        let destination = framebuffer_of(&mut context, 2, 2, &[0xff00_0000; 4]).await;

        // One pixel to the right and one down, then white into the corner.
        let offset = context.alloc_raw(8).unwrap();
        write_generic(&mut context, offset, 1u32).unwrap();
        write_generic(&mut context, offset + 4, 1u32).unwrap();
        set_context(&mut context, pgc, Idx::OffsetIdx, offset).await.unwrap();
        set_context(&mut context, pgc, Idx::FgPixelIdx, 0x00ff_ffff).await.unwrap();
        super::fill_rect(&mut context, destination, 0, 0, 1, 1, pgc).await.unwrap();

        let handle = read_generic(&context, context.data_ptr(destination).unwrap()).unwrap();
        let filled = super::FrameBuffer(handle).image(&mut context).unwrap();

        let moved = filled.get_pixel(1, 1);
        assert_eq!((moved.r, moved.g, moved.b), (0xff, 0xff, 0xff), "the fill did not move with the offset");

        let corner = filled.get_pixel(0, 0);
        assert_eq!((corner.r, corner.g, corner.b), (0, 0, 0), "it was drawn at the corner as well");
    }

    async fn framebuffer_of(context: &mut TestContext, width: u32, height: u32, pixels: &[u32]) -> super::WIPICIndirectPtr {
        let image = VecImageBuffer::<ArgbPixel>::from_raw(width, height, pixels.to_vec());
        let framebuffer = super::FrameBuffer::from_image(context, &image).unwrap();

        let handle = context.alloc(core::mem::size_of::<wipi_types::wipic::WIPICFramebuffer>() as u32).unwrap();
        let address = context.data_ptr(handle).unwrap();
        write_generic(context, address, framebuffer.0).unwrap();

        handle
    }

    fn test_context() -> TestContext {
        use alloc::boxed::Box;
        use test_utils::TestPlatform;
        use wie_backend::{DefaultTaskRunner, System};

        TestContext::with_system(System::new(Box::new(TestPlatform::new()), "test-pid", "test-aid", DefaultTaskRunner))
    }

    /// The encoding is a 24-bit bottom-up BMP of the rectangle asked for, and
    /// the length comes back through the pointer beside it.
    #[futures_test::test]
    async fn an_encoded_image_is_a_bottom_up_bmp() {
        let mut context = test_context();

        // Two rows: white on top, red underneath.
        let framebuffer = framebuffer_of(&mut context, 2, 2, &[0xffff_ffff, 0xffff_ffff, 0xfff8_0000, 0xfff8_0000]).await;

        let encoded_buffer = super::encode_image(&mut context, framebuffer, 0, 0, 2, 2, 0x100).await.unwrap();
        assert_ne!(encoded_buffer.0, 0);

        let length: u32 = read_generic(&context, 0x100).unwrap();
        // 54-byte header, rows of 2 pixels padded from 6 to 8 bytes.
        assert_eq!(length, 54 + 8 * 2);

        let mut encoded = alloc::vec![0u8; length as usize];
        let data = context.data_ptr(encoded_buffer).unwrap();
        wie_util::ByteRead::read_bytes(&context, data, &mut encoded).unwrap();

        assert_eq!(&encoded[..2], b"BM");
        assert_eq!(u32::from_le_bytes(encoded[10..14].try_into().unwrap()), 54);
        assert_eq!(i32::from_le_bytes(encoded[18..22].try_into().unwrap()), 2);
        assert_eq!(i32::from_le_bytes(encoded[22..26].try_into().unwrap()), 2);
        assert_eq!(u16::from_le_bytes(encoded[28..30].try_into().unwrap()), 24);

        // Bottom-up: the first stored row is the red one, written BGR.
        assert_eq!(&encoded[54..60], &[0x00, 0x00, 0xf8, 0x00, 0x00, 0xf8]);
        // And the last is the white one.
        assert_eq!(&encoded[62..68], &[0xf8, 0xfc, 0xf8, 0xf8, 0xfc, 0xf8]);
    }

    /// Every way the reference refuses answers 0 and leaves the length at 0,
    /// so a caller that only reads the length is not told a size it cannot
    /// trust.
    #[futures_test::test]
    async fn a_rectangle_that_does_not_fit_is_refused() {
        let mut context = test_context();
        let framebuffer = framebuffer_of(&mut context, 2, 2, &[0xffff_ffff; 4]).await;

        for (x, y, width, height) in [(-1, 0, 2, 2), (0, -1, 2, 2), (0, 0, 0, 2), (0, 0, 2, 0), (1, 0, 2, 2), (0, 1, 2, 2)] {
            write_generic(&mut context, 0x100, 0xffff_ffffu32).unwrap();

            assert_eq!(
                super::encode_image(&mut context, framebuffer, x, y, width, height, 0x100)
                    .await
                    .unwrap()
                    .0,
                0
            );

            let length: u32 = read_generic(&context, 0x100).unwrap();
            assert_eq!(length, 0, "{x},{y} {width}x{height}");
        }
    }

    /// The same text spelled either way paints the same pixels, so a title that
    /// lays out Unicode gets what the byte-string call would have given it.
    #[futures_test::test]
    async fn unicode_text_paints_what_the_byte_string_call_paints() {
        let mut context = test_context();

        let pgc = 0x200;
        init_context(&mut context, pgc).await.unwrap();

        let byte_string = 0x300;
        wie_util::ByteWrite::write_bytes(&mut context, byte_string, b"Hi\0").unwrap();
        let unicode_string = 0x400;
        wie_util::ByteWrite::write_bytes(&mut context, unicode_string, &[b'H', 0, b'i', 0, 0, 0]).unwrap();

        let drawn_as_bytes = framebuffer_of(&mut context, 32, 16, &[0xff00_0000; 32 * 16]).await;
        super::draw_string(&mut context, drawn_as_bytes, 0, 0, byte_string, -1, pgc)
            .await
            .unwrap();

        let drawn_as_unicode = framebuffer_of(&mut context, 32, 16, &[0xff00_0000; 32 * 16]).await;
        super::draw_unicode_string(&mut context, drawn_as_unicode, 0, 0, unicode_string, -1, pgc)
            .await
            .unwrap();

        let from_bytes = super::FrameBuffer(read_generic(&context, context.data_ptr(drawn_as_bytes).unwrap()).unwrap())
            .data(&context)
            .unwrap();
        let from_unicode = super::FrameBuffer(read_generic(&context, context.data_ptr(drawn_as_unicode).unwrap()).unwrap())
            .data(&context)
            .unwrap();

        assert_eq!(from_bytes, from_unicode);
        // And something was actually drawn, so the comparison is not of two
        // empty buffers.
        assert!(from_bytes.iter().any(|&byte| byte != 0));
    }

    /// A surface with `drawn` pixels of one colour on it and the rest black.
    fn surface_with(width: u32, height: u32, drawn: u32, colour: u32) -> impl Image {
        let mut raw: alloc::vec::Vec<u32> = alloc::vec![0xff00_0000; (width * height) as usize];
        for pixel in raw.iter_mut().take(drawn as usize) {
            *pixel = 0xff00_0000 | colour;
        }

        VecImageBuffer::<ArgbPixel>::from_raw(width, height, raw)
    }

    /// A framebuffer as the guest stores one.
    fn described(width: u32, height: u32, bpp: u32) -> wipi_types::wipic::WIPICFramebuffer {
        wipi_types::wipic::WIPICFramebuffer {
            width,
            height,
            bpl: width * (bpp / 8),
            bpp,
            buf: super::WIPICIndirectPtr(0x1000),
        }
    }

    /// The reference's getters test the handle before they touch it and hand
    /// back minus one, so a title that calls them with zero - 열혈택시 does,
    /// out of its image loader - gets an answer instead of a dead VM.
    #[futures_test::test]
    async fn a_null_framebuffer_is_answered_the_way_the_reference_answers_it() {
        use alloc::boxed::Box;
        use test_utils::TestPlatform;
        use wie_backend::{DefaultTaskRunner, System};

        use crate::context::test::TestContext;

        let system = System::new(Box::new(TestPlatform::new()), "test-pid", "test-aid", DefaultTaskRunner);
        let mut context = TestContext::with_system(system);
        let null = super::WIPICIndirectPtr(0);

        assert_eq!(super::get_framebuffer_width(&mut context, null).await.unwrap(), super::NO_FRAMEBUFFER);
        assert_eq!(super::get_framebuffer_height(&mut context, null).await.unwrap(), super::NO_FRAMEBUFFER);
        assert_eq!(super::get_framebuffer_bpl(&mut context, null).await.unwrap(), super::NO_FRAMEBUFFER);
        assert_eq!(
            super::get_framebuffer_pointer(&mut context, null).await.unwrap(),
            super::NO_FRAMEBUFFER as u32
        );
    }

    #[test]
    fn a_surface_that_stopped_being_one_is_not_read_as_one() {
        // What a live 60x60 surface looks like.
        assert!(super::still_the_surface(&described(60, 60, 16), 60, 60));
        assert!(super::still_the_surface(&described(60, 60, 32), 60, 60));

        // And what the allocation looks like once another title has it: zeroed,
        // or holding something whose depth has no pixel format, or the wrong
        // size for what was registered. Reading any of these as a framebuffer
        // is what crashed a game run after one that had used a surface.
        assert!(!super::still_the_surface(&described(0, 0, 0), 60, 60));
        assert!(!super::still_the_surface(&described(60, 60, 8), 60, 60));
        assert!(!super::still_the_surface(&described(61, 60, 16), 60, 60));
    }

    #[test]
    fn a_surface_nothing_drew_on_reads_as_empty() {
        let (colours, non_black) = surface_content(&surface_with(60, 60, 0, 0));

        // One colour - black - and nothing lit. This is what a sprite that was
        // never drawn looks like, and it is the whole point of the line.
        assert_eq!(non_black, 0);
        assert_eq!(colours, 1);
    }

    #[test]
    fn a_thumbnail_of_an_empty_surface_is_blank() {
        let lines = surface_thumbnail(&surface_with(60, 60, 0, 0));

        assert!(!lines.is_empty());
        assert!(lines.iter().all(|line| line.chars().all(|c| c == ' ')));
    }

    #[test]
    fn a_thumbnail_shows_where_the_lit_pixels_are() {
        // The top third of a surface lit white, the rest black: the top rows
        // read bright and the bottom rows blank. This is the whole job - an
        // icon that was drawn looks different from a panel that was not.
        let lines = surface_thumbnail(&surface_with(60, 60, 60 * 20, 0xffffff));

        assert_eq!(lines[0].chars().next(), Some('@'));
        assert!(lines.last().unwrap().chars().all(|c| c == ' '));
    }

    #[test]
    fn a_thumbnail_keeps_the_shape_of_what_it_draws() {
        // A 240x320 screen is taller than it is wide, and a thumbnail that gave
        // it as many rows as a landscape one would squash it flat - which is
        // what makes a panel unreadable in a log.
        let portrait = surface_thumbnail(&surface_with(240, 320, 0, 0)).len();
        let landscape = surface_thumbnail(&surface_with(320, 240, 0, 0)).len();

        assert!(portrait > landscape, "{portrait} rows for a portrait, {landscape} for a landscape");
    }

    #[test]
    fn a_thumbnail_never_gets_wider_than_it_can_be_read_at() {
        for (width, height) in [(60, 60), (240, 320), (8, 4)] {
            for line in surface_thumbnail(&surface_with(width, height, 0, 0)) {
                assert!(line.chars().count() <= super::THUMBNAIL_COLUMNS as usize);
            }
        }
    }

    #[test]
    fn a_surface_something_drew_on_says_how_much() {
        let (colours, non_black) = surface_content(&surface_with(60, 60, 900, 0x3366ff));

        assert_eq!(non_black, 900);
        assert_eq!(colours, 2);
    }

    /// A clet saves the drawing state with `MC_grpGetContext` and later restores
    /// it with `MC_grpSetContext`, so a get has to report back exactly what a set
    /// stored. When this read nothing (the old stub), the saved state was zero and
    /// the restore wrecked every later draw.
    #[futures_test::test]
    async fn get_context_reads_back_what_set_context_stored() {
        const CTX: u32 = 0x1000;
        const OUT: u32 = 0x2000;

        let mut context = TestContext::new();
        init_context(&mut context, CTX).await.unwrap();

        // Scalars are passed to set by value and returned by get through a pointer.
        for (op, value) in [
            (Idx::FgPixelIdx, 0x1234u32),
            (Idx::BgPixelIdx, 0x5678),
            (Idx::AlphaIdx, 0x80),
            (Idx::FontIdx, 0x42),
            (Idx::StyleIdx, 0x3),
        ] {
            set_context(&mut context, CTX, op, value).await.unwrap();
            get_context(&mut context, CTX, op, OUT).await.unwrap();
            assert_eq!(read_generic::<u32, _>(&context, OUT).unwrap(), value, "op {op:?}");
        }

        // The clip is passed through memory both ways as four 32-bit words
        // (x1, y1, x2, y2); set decrements the bottom-right corner on the way in
        // and get reports it one past what is stored, matching liblgt_system.so,
        // so a get-then-set round-trip is the identity.
        write_generic(&mut context, OUT, 10u32).unwrap();
        write_generic(&mut context, OUT + 4, 20u32).unwrap();
        write_generic(&mut context, OUT + 8, 100u32).unwrap();
        write_generic(&mut context, OUT + 12, 200u32).unwrap();
        set_context(&mut context, CTX, Idx::ClipIdx, OUT).await.unwrap();
        get_context(&mut context, CTX, Idx::ClipIdx, OUT).await.unwrap();
        assert_eq!(read_generic::<u32, _>(&context, OUT).unwrap(), 10);
        assert_eq!(read_generic::<u32, _>(&context, OUT + 4).unwrap(), 20);
        assert_eq!(read_generic::<u32, _>(&context, OUT + 8).unwrap(), 100);
        assert_eq!(read_generic::<u32, _>(&context, OUT + 12).unwrap(), 200);

        // The offset is the same two-32-bit-word pair through memory, restored
        // verbatim.
        write_generic(&mut context, OUT, 7u32).unwrap();
        write_generic(&mut context, OUT + 4, 9u32).unwrap();
        set_context(&mut context, CTX, Idx::OffsetIdx, OUT).await.unwrap();
        get_context(&mut context, CTX, Idx::OffsetIdx, OUT).await.unwrap();
        assert_eq!(read_generic::<u32, _>(&context, OUT).unwrap(), 7);
        assert_eq!(read_generic::<u32, _>(&context, OUT + 4).unwrap(), 9);
    }

    /// A null context or destination is a no-op, exactly as the vendor guards it,
    /// not a memory fault.
    #[futures_test::test]
    async fn get_context_tolerates_null_pointers() {
        let mut context = TestContext::new();
        get_context(&mut context, 0, Idx::FgPixelIdx, 0x2000).await.unwrap();
        get_context(&mut context, 0x1000, Idx::FgPixelIdx, 0).await.unwrap();
    }

    /// MC_grpInitContext plants non-zero defaults (full clip, white background,
    /// opaque alpha, param1, the 12px font) rather than zeroing the block, and a
    /// game that never sets those fields draws against them. Reading each one
    /// straight back through GetContext proves the port matches the firmware.

    /// A title's own clip setter hands this no rectangle when the one it wants is
    /// the whole surface, and no rectangle means the clip a context has before
    /// anyone sets one - not sixteen bytes read at address zero.
    #[futures_test::test]
    async fn no_clip_rectangle_clears_the_clip() {
        const CTX: u32 = 0x1000;
        const RECT: u32 = 0x1800;
        const OUT: u32 = 0x2000;

        let mut context = TestContext::new();
        init_context(&mut context, CTX).await.unwrap();

        // Narrow it first, so the clear below has something to undo.
        for (i, v) in [10u32, 20, 30, 40].into_iter().enumerate() {
            write_generic(&mut context, RECT + (i as u32) * 4, v).unwrap();
        }
        set_context(&mut context, CTX, Idx::ClipIdx, RECT).await.unwrap();
        get_context(&mut context, CTX, Idx::ClipIdx, OUT).await.unwrap();
        assert_eq!(read_generic::<u32, _>(&context, OUT).unwrap(), 10);

        set_context(&mut context, CTX, Idx::ClipIdx, 0).await.unwrap();

        get_context(&mut context, CTX, Idx::ClipIdx, OUT).await.unwrap();
        assert_eq!(read_generic::<u32, _>(&context, OUT).unwrap(), 0);
        assert_eq!(read_generic::<u32, _>(&context, OUT + 4).unwrap(), 0);
        assert_eq!(read_generic::<u32, _>(&context, OUT + 8).unwrap(), 0x8000);
        assert_eq!(read_generic::<u32, _>(&context, OUT + 12).unwrap(), 0x8000);
    }
    #[futures_test::test]
    async fn init_context_plants_the_reference_defaults() {
        const CTX: u32 = 0x1000;
        const OUT: u32 = 0x2000;

        let mut context = TestContext::new();
        init_context(&mut context, CTX).await.unwrap();

        for (op, value) in [
            (Idx::FgPixelIdx, 0x0),
            (Idx::BgPixelIdx, 0x00ff_ffff),
            (Idx::AlphaIdx, 0xff),
            (Idx::FontIdx, 12),
            (Idx::StyleIdx, 0x0),
        ] {
            get_context(&mut context, CTX, op, OUT).await.unwrap();
            assert_eq!(read_generic::<u32, _>(&context, OUT).unwrap(), value, "op {op:?}");
        }

        // The clip starts at the whole plane; GetContext reports the corner one
        // past what is stored (0x7fff -> 0x8000).
        get_context(&mut context, CTX, Idx::ClipIdx, OUT).await.unwrap();
        assert_eq!(read_generic::<u32, _>(&context, OUT).unwrap(), 0);
        assert_eq!(read_generic::<u32, _>(&context, OUT + 4).unwrap(), 0);
        assert_eq!(read_generic::<u32, _>(&context, OUT + 8).unwrap(), 0x8000);
        assert_eq!(read_generic::<u32, _>(&context, OUT + 12).unwrap(), 0x8000);
    }

    /// The colour path is the driver's direct RGB565: 5 bits red, 6 green, 5
    /// blue, packed r>>3 << 11 | g>>2 << 5 | b>>3, and it round-trips through the
    /// getter within that precision. This is what MC_grpGetDisplayInfo advertises
    /// (masks 0xf800/0x07e0/0x001f) and what a title's own blitter converts
    /// against, so it has to be exact.
    #[futures_test::test]
    async fn pixel_from_rgb_is_direct_rgb565() {
        let mut context = TestContext::new();
        assert_eq!(super::get_pixel_from_rgb(&mut context, 0xff, 0, 0).await.unwrap(), 0xf800);
        assert_eq!(super::get_pixel_from_rgb(&mut context, 0, 0xff, 0).await.unwrap(), 0x07e0);
        assert_eq!(super::get_pixel_from_rgb(&mut context, 0, 0, 0xff).await.unwrap(), 0x001f);
        assert_eq!(super::get_pixel_from_rgb(&mut context, 0xff, 0xff, 0xff).await.unwrap(), 0xffff);
        assert_eq!(super::get_pixel_from_rgb(&mut context, 0, 0, 0).await.unwrap(), 0x0000);

        // A pure-red pixel decodes back to full red (5-bit max scaled to 8-bit).
        const OUT: u32 = 0x3000;
        super::get_rgb_from_pixel(&mut context, 0xf800, OUT, OUT + 4, OUT + 8).await.unwrap();
        assert_eq!(read_generic::<u32, _>(&context, OUT).unwrap(), 0xff);
        assert_eq!(read_generic::<u32, _>(&context, OUT + 4).unwrap(), 0);
        assert_eq!(read_generic::<u32, _>(&context, OUT + 8).unwrap(), 0);
    }

    /// The size selector maps to the seven glyph heights `MC_grpGetFont` assigns,
    /// and the handle those return round-trips through the height getter.
    #[futures_test::test]
    async fn get_font_reports_the_reference_heights() {
        let mut context = TestContext::new();
        for (size, height) in [
            (0x8, 10),
            (0x10, 14),
            (0x1000, 16),
            (0x2000, 18),
            (0x4000, 19),
            (0x8000, 22),
            (0, 12),
            (0x1234, 12),
        ] {
            let handle = super::get_font(&mut context, 0, size, 0).await.unwrap();
            assert_eq!(handle, height, "size {size:#x}");
            assert_eq!(super::get_font_height(&mut context, handle).await.unwrap(), height, "height of {size:#x}");
        }
        // An unset SetContext font (0) falls back to the default face.
        assert_eq!(super::get_font_height(&mut context, 0).await.unwrap(), 12);
    }

    /// A single pixel read into four bytes of stack, which is how a title asks
    /// whether it has walked into something. LGT's runtime takes `ipl = 1`.
    #[test]
    fn a_one_pixel_probe_is_delivered() {
        assert_eq!(destination_stride(1, 1, 1), Some((4, 4)));
    }

    #[test]
    fn a_real_stride_is_honoured() {
        // Reading 100 pixels into a 240 wide buffer.
        assert_eq!(destination_stride(100, 50, 960), Some((400, 960)));
    }

    /// Too small to be a stride, so the rows pack - which is what the handset
    /// did with any value at all.
    #[test]
    fn a_short_stride_packs_instead_of_dropping_the_call() {
        assert_eq!(destination_stride(8, 4, 3), Some((32, 32)));
    }

    #[test]
    fn nothing_to_read_is_dropped() {
        assert_eq!(destination_stride(0, 4, 16), None);
        assert_eq!(destination_stride(4, 0, 16), None);
        assert_eq!(destination_stride(4, 4, 0), None);
        assert_eq!(destination_stride(4, 4, -1), None);
    }

    #[test]
    fn an_unreasonable_width_is_dropped_rather_than_overflowing() {
        assert_eq!(destination_stride(i32::MAX, 1, i32::MAX), None);
    }
}
