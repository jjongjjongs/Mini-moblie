use core::mem;

use bytemuck::{Pod, Zeroable};

use wipi_types::wipic::WIPICWord;

use crate::{WIPICContext, method::ParamConverter};

/// A title's drawing state, laid out the way the reference lays it out.
///
/// A clet does not only pass this struct to the `MC_grp*` calls - one with its
/// own blitter reads the fields straight out of it, to clip and to pick up the
/// colours it draws with - so the layout is part of the ABI, not an internal
/// detail. `MC_grpSetContext` (@0x1aaba8) and `MC_grpGetContext` (@0x1a9e94)
/// give it exactly: every field one 32-bit word, the clip rectangle first, and
/// `MC_grpInitContext` (@0x1abc0c) fills the same 0x38 bytes.
///
/// The shape carried here before had a leading word the reference does not have
/// and packed the clip into 16-bit halves, which put every field from the
/// foreground colour on at the wrong offset - so a title reading its own
/// context back got another field's value, and a clet clipping its blits by
/// hand clipped against nonsense.
///
/// Op 3 (`TransPixelIdx`) has no field: the reference neither stores nor
/// reports it.
///
/// The two handsets do not agree about the two colour words - see
/// [`ContextLayout`] - so a context is read and written through
/// [`WIPICGraphicsContext::in_layout`] rather than straight off the wire.
#[repr(C)]
#[derive(Default, Clone, Copy, Pod, Zeroable)]
pub struct WIPICGraphicsContext {
    /// Top-left x, y and bottom-right x, y - the corner stored decremented, as
    /// the reference stores it, and re-incremented when reported back.
    pub clip: [WIPICWord; 4],
    pub fgpxl: WIPICWord,
    pub bgpxl: WIPICWord,
    pub alpha: WIPICWord,
    /// The pixel a blit treats as transparent - op 3.
    ///
    /// Nothing here writes it: a title that names one does so through
    /// `MC_grpSetContext`, which the reference neither stores nor reads back,
    /// and our blits key on magenta. It is named because 헬싱 fills this
    /// struct itself and leaves `0xf81f` here - magenta, the key it draws
    /// everything against - which is what says the word is a colour and not
    /// the operation LGT keeps at this offset. See [`ContextLayout`].
    pub transparent: WIPICWord,
    pub param1: WIPICWord,
    pub font: WIPICWord,
    pub style: WIPICWord,
    /// `MC_GrpPixelOpProc`, which the reference also plants for XOR mode.
    ///
    /// At `+0x2c` on KTF. 헬싱 is what says so: it fills this struct itself
    /// rather than through `MC_grpSetContext`, and what it leaves there is
    /// `0x108ce9` - the address of its own operation, inside its own image.
    /// Taken from `+0x1c` instead, the transparent pixel `0xf81f` read as an
    /// operation, and calling it took the title down on its first frame.
    ///
    /// LGT keeps it at `+0x1c` and a flag at `+0x2c` instead:
    /// `wipic_grpContext_to_dgraphics` (@0x1aa2e8) installs `[ctx + 0x1c]`
    /// with `[ctx + 0x20]` as its parameter and tests `[ctx + 0x2c]` for XOR
    /// mode. No LGT title reaches those words except through
    /// `MC_grpSetContext`, which stores and reads back whichever offset this
    /// names, so the two are not told apart here yet.
    ///
    /// XOR mode has no word of its own: it is this slot holding
    /// [`BUILT_IN_XOR`].
    pub pixel_op_func_ptr: WIPICWord,
    /// x, y
    pub offset: [WIPICWord; 2],
}

/// What stands in the operation slot for XOR mode.
///
/// Op 9 does not set a flag - the reference installs its own built-in
/// operation, and the slot is where that goes. We have no guest address for
/// that operation, so this stands in its place: recognised here, and never
/// handed back to a title that reads the slot.
pub const BUILT_IN_XOR: WIPICWord = 0xffff_ffff;

/// Which handset's field order a context on the wire is in.
///
/// The struct is the API's, not a runtime's, so nearly all of it is the same
/// on both: the clip first with its corner decremented, then two colours, the
/// alpha, a word, the parameter, the font, the style, a word and the offset -
/// 0x38 bytes in all. The two colours are where they part.
///
/// LGT puts the foreground first. Its firmware says so twice:
/// `MC_grpSetContext` (@0x1aaba8) stores op 1 at `+0x10` and op 2 at `+0x14`,
/// and `MC_grpPutPixel` (@0x1ad240) draws with `[ctx + 0x10]`.
///
/// KTF puts the background first. 헬싱 is what says so, because it never
/// calls `MC_grpInitContext` or `MC_grpSetContext` at all: it fills its own
/// 0x38 bytes at `0x11dfe0` and hands that to every draw, so those words are
/// the handset's own layout and nothing of this runtime's. What it leaves is
/// `[0, 0, 175, 219]` for its 176x220 screen, then `0` and the colour it is
/// drawing with, `0xff` alpha, `0xf81f` magenta, `0xff` parameter, `12` font,
/// `0` style and its own operation - every other field exactly where the API
/// puts it, and the colour it draws with in the *second* word.
///
/// Read the LGT way that colour is the background and the foreground is black,
/// so every fill 헬싱 lays comes out black - which is how it draws its
/// Korean text, and why none of it could be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextLayout {
    /// Foreground at `+0x10`, background at `+0x14`.
    ForegroundFirst,
    /// Background at `+0x10`, foreground at `+0x14`.
    BackgroundFirst,
}

impl WIPICGraphicsContext {
    /// The same context with its two colour words in the handset's order.
    ///
    /// Used on the way in and on the way out - swapping twice is the identity,
    /// so one function serves both and they cannot drift apart.
    pub fn in_layout(mut self, layout: ContextLayout) -> Self {
        if layout == ContextLayout::BackgroundFirst {
            mem::swap(&mut self.fgpxl, &mut self.bgpxl);
        }

        self
    }
}

#[repr(u32)]
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum WIPICGraphicsContextIdx {
    ClipIdx = 0,
    FgPixelIdx = 1,
    BgPixelIdx = 2,
    TransPixelIdx = 3,
    AlphaIdx = 4,
    PixelopIdx = 5,
    PixelParam1Idx = 6,
    FontIdx = 7,
    StyleIdx = 8,
    XorModeIdx = 9,
    OffsetIdx = 10,
    OutlineIdx = 11,

    /// Unknown values are mapped to this enum value.
    /// Note that this field doesn't exist in WIPI and the choice of this ordinal is arbitrary.
    Invalid = 0xff,
}

impl WIPICGraphicsContextIdx {
    /// The op a raw argument names, or `Invalid` for one that names none.
    ///
    /// Taken apart from the `ParamConverter` so the emulator's synchronous
    /// fast path, which has the guest's registers but no `WIPICContext`, reads
    /// the argument the same way the generic dispatch does.
    pub fn from_raw(raw: WIPICWord) -> Self {
        if raw >= (Self::ClipIdx as WIPICWord) && raw <= (Self::OutlineIdx as WIPICWord) {
            // SAFETY: WIPICGraphicsContextIdx has CWord repr and is unit only.
            unsafe { mem::transmute(raw) }
        } else {
            Self::Invalid
        }
    }
}

impl ParamConverter<WIPICGraphicsContextIdx> for WIPICGraphicsContextIdx {
    fn convert(_context: &mut dyn WIPICContext, raw: WIPICWord) -> WIPICGraphicsContextIdx {
        Self::from_raw(raw)
    }
}

#[cfg(test)]
mod test {
    use super::{ContextLayout, WIPICGraphicsContext, WIPICGraphicsContextIdx};

    /// Every op the reference has, and nothing else.
    ///
    /// The fast path reads its op straight out of a register, so this is the
    /// only thing standing between a wild argument and a transmute.
    #[test]
    fn an_op_outside_the_table_is_invalid() {
        for raw in 0..=11u32 {
            assert_eq!(WIPICGraphicsContextIdx::from_raw(raw) as u32, raw);
        }
        for raw in [12u32, 0xff, 0x1000, u32::MAX] {
            assert!(matches!(WIPICGraphicsContextIdx::from_raw(raw), WIPICGraphicsContextIdx::Invalid));
        }
    }

    /// KTF's two colour words are the other way round, and nothing else moves.
    ///
    /// The words are 헬싱's own, read out of the context it fills at
    /// `0x11dfe0` and hands to every draw: `0` and then `0x6da0`, the colour it
    /// is drawing with. Read LGT's way round the colour is the background and
    /// every fill comes out black.
    #[test]
    fn ktf_keeps_the_background_in_the_first_colour_word() {
        let on_the_wire = WIPICGraphicsContext {
            clip: [0, 0, 175, 219],
            fgpxl: 0,
            bgpxl: 0x6da0,
            alpha: 0xff,
            transparent: 0xf81f,
            param1: 0xff,
            font: 12,
            style: 0,
            pixel_op_func_ptr: 0x108ce9,
            offset: [0, 0],
        };

        let ktf = on_the_wire.in_layout(ContextLayout::BackgroundFirst);
        assert_eq!(ktf.fgpxl, 0x6da0, "the colour it draws with is the foreground");
        assert_eq!(ktf.bgpxl, 0);

        // Everything else is the API's and is where the API puts it.
        assert_eq!(ktf.clip, [0, 0, 175, 219]);
        assert_eq!(ktf.alpha, 0xff);
        assert_eq!(ktf.transparent, 0xf81f);
        assert_eq!(ktf.param1, 0xff);
        assert_eq!(ktf.font, 12);
        assert_eq!(ktf.pixel_op_func_ptr, 0x108ce9);

        let lgt = on_the_wire.in_layout(ContextLayout::ForegroundFirst);
        assert_eq!(lgt.fgpxl, 0);
        assert_eq!(lgt.bgpxl, 0x6da0);
    }

    /// The swap is its own inverse, which is what lets one function serve the
    /// read and the write.
    #[test]
    fn a_context_put_into_a_layout_twice_is_itself_again() {
        for layout in [ContextLayout::ForegroundFirst, ContextLayout::BackgroundFirst] {
            let context = WIPICGraphicsContext {
                fgpxl: 0x1234,
                bgpxl: 0x5678,
                ..Default::default()
            };

            let round_trip = context.in_layout(layout).in_layout(layout);
            assert_eq!(round_trip.fgpxl, 0x1234);
            assert_eq!(round_trip.bgpxl, 0x5678);
        }
    }
}
