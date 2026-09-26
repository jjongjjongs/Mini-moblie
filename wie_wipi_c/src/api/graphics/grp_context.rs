use core::mem;

use bytemuck::{Pod, Zeroable};

use wipi_types::wipic::WIPICWord;

use crate::{WIPICContext, method::ParamConverter};

/// A title's drawing state, in this runtime's own field order.
///
/// A clet does not only pass this struct to the `MC_grp*` calls - one with its
/// own blitter reads the fields straight out of it, to clip and to pick up the
/// colours it draws with - so where each word sits in the title's memory is
/// part of the ABI, not an internal detail. This struct is not that memory:
/// it is what the runtime works with, and [`ContextLayout`] says where each of
/// its fields lives on the wire, because the two handsets do not lay them out
/// the same way.
///
/// Op 3 (`TransPixelIdx`) is never written: the reference neither stores nor
/// reports it. The word is still read back, because a title that fills its own
/// context leaves its colour key there.
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
    /// and our blits key on magenta. It is named because a title that fills
    /// its own context leaves its key in this word - 헬싱 writes `0xf81f`
    /// (magenta) at `+0x1c`, and 액션히어로3D reads its key back from the
    /// same offset.
    pub transparent: WIPICWord,
    pub param1: WIPICWord,
    pub font: WIPICWord,
    pub style: WIPICWord,
    /// `MC_GrpPixelOpProc`, which the reference also plants for XOR mode.
    ///
    /// At `+0x2c` on both handsets - see [`ContextLayout`]. XOR mode has no
    /// word of its own: it is this slot holding [`BUILT_IN_XOR`].
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

/// Where each of a context's words sits in the title's own memory.
///
/// A clet with its own blitter reads these words directly - to clip, to
/// translate, to pick up the colour it draws with and the pixel it keys
/// against - so the offsets are the ABI, not an internal detail. The two
/// handsets do not lay the struct out the same way, and neither of them can be
/// guessed from the other.
///
/// KTF's is `MC_GrpContext` as the WIPI header declares it, with the rectangle
/// and the offset as 32-bit words rather than 16-bit pairs:
///
/// ```text
/// +0x00 mask        +0x14 fgpxl      +0x24 offset x   +0x34 reserved
/// +0x04 clip x1     +0x18 bgpxl      +0x28 offset y   +0x38 font
/// +0x08 clip y1     +0x1c transpxl   +0x2c pixelop    +0x3c style
/// +0x0c clip x2     +0x20 alpha      +0x30 param1
/// +0x10 clip y2
/// ```
///
/// Two titles that fill or read the struct behind the API's back say so, and
/// between them they pin five of those words:
///
/// 액션히어로3D zeroes its context at startup, never writes those words again,
/// calls `MC_grpSetContext(gc, 0, {0,0,240,320})` every frame, and its own
/// blitter then clips each blit against `[gc+0x04]`, `[gc+0x08]`, `[gc+0x0c]`
/// and `[gc+0x10]` as left, top, right and bottom, translates by `[gc+0x24]`
/// and `[gc+0x28]`, keys against `[gc+0x1c]`, and gives up and takes the slow
/// path when `[gc+0x2c]` holds an operation. The only way the rectangle can be
/// there is if the firmware put it there.
///
/// 헬싱 never calls `MC_grpInitContext` or `MC_grpSetContext` at all - it fills
/// its own context at `0x11dfe0` and hands that to every draw - and the only
/// three words it writes are `+0x14` (`0x6da0`, the colour it draws with),
/// `+0x1c` (`0xf81f`, magenta, its key) and `+0x2c` (`0x108ce9`, the address of
/// its own pixel operation).
///
/// LGT's is the same fields in nearly the same order with no leading word, no
/// transparent pixel, and the offset at the end. Its firmware says so outright:
/// `MC_grpSetContext` (@0x1aaba8) stores op 0's four corners at `+0x00`..`+0x0c`,
/// then op 1 at `+0x10`, op 2 at `+0x14`, op 4 at `+0x18`, op 5 at `+0x1c`, op 6
/// at `+0x20`, op 7 at `+0x24`, op 8 at `+0x28`, op 9 at `+0x2c` and op 10 at
/// `+0x30`/`+0x34`, and drops op 3 on the floor.
///
/// Read against LGT's order, a KTF title's clip rectangle came out one word
/// early - so 액션히어로3D read its own rectangle's right edge, 239, as its top
/// clip and clamped every blit it drew by hand to y >= 239, which put its title
/// logo at the foot of the screen instead of above `PRESS ANY KEY` - and its
/// colour came out one word early too, which is why 헬싱's Korean text was
/// black on black.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextLayout {
    /// The clip rectangle first, at `+0x00`, and no transparent pixel.
    Lgt,
    /// `MC_GrpContext` whole: a mask, then the clip rectangle at `+0x04`.
    Ktf,
}

/// Where one layout keeps each field, as a byte offset from the context.
///
/// `clip` and `offset` name the first of their consecutive words.
pub(crate) struct ContextOffsets {
    pub clip: WIPICWord,
    pub fgpxl: WIPICWord,
    pub bgpxl: WIPICWord,
    pub transparent: WIPICWord,
    pub alpha: WIPICWord,
    pub offset: WIPICWord,
    pub pixel_op_func_ptr: WIPICWord,
    pub param1: WIPICWord,
    pub font: WIPICWord,
    pub style: WIPICWord,
    /// Whether op 3 has a word of its own here.
    ///
    /// On KTF it does, and a title's own blitter reads it: 액션히어로3D sets
    /// `0xf81f` through the API and keys its sprites against what it reads back
    /// from `+0x1c`. LGT has no such field - the reference drops op 3 - and
    /// `+0x1c` is its operation slot instead, so storing one there would plant
    /// a colour where an address goes.
    pub keeps_transparent: bool,
}

impl ContextLayout {
    pub(crate) fn offsets(self) -> ContextOffsets {
        match self {
            // Straight off `MC_grpSetContext` (@0x1aaba8). The operation is at
            // `+0x1c` there and a flag at `+0x2c`; no LGT title reaches either
            // word except through the API, which stores and reads back
            // whichever offset this names, so the two are not told apart here
            // yet and the operation keeps the offset KTF's titles use.
            Self::Lgt => ContextOffsets {
                clip: 0x00,
                fgpxl: 0x10,
                bgpxl: 0x14,
                alpha: 0x18,
                transparent: 0x1c,
                param1: 0x20,
                font: 0x24,
                style: 0x28,
                pixel_op_func_ptr: 0x2c,
                offset: 0x30,
                keeps_transparent: false,
            },
            Self::Ktf => ContextOffsets {
                clip: 0x04,
                fgpxl: 0x14,
                bgpxl: 0x18,
                transparent: 0x1c,
                alpha: 0x20,
                offset: 0x24,
                pixel_op_func_ptr: 0x2c,
                param1: 0x30,
                font: 0x38,
                style: 0x3c,
                keeps_transparent: true,
            },
        }
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
            unsafe { mem::transmute::<WIPICWord, Self>(raw) }
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
    use super::{ContextLayout, WIPICGraphicsContextIdx};

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

    /// Every field a layout names sits in its own word, and no two share one.
    ///
    /// The whole point of the table is that a title's own blitter reads these
    /// words directly, so an offset that collided with another would be a
    /// field silently overwriting a field.
    #[test]
    fn a_layout_gives_every_field_a_word_of_its_own() {
        for layout in [ContextLayout::Lgt, ContextLayout::Ktf] {
            let at = layout.offsets();
            let mut taken = alloc::vec::Vec::new();
            for offset in [
                at.fgpxl,
                at.bgpxl,
                at.alpha,
                at.transparent,
                at.param1,
                at.font,
                at.style,
                at.pixel_op_func_ptr,
            ] {
                taken.push(offset);
            }
            for word in 0..4 {
                taken.push(at.clip + 4 * word);
            }
            for word in 0..2 {
                taken.push(at.offset + 4 * word);
            }

            for offset in &taken {
                assert!(offset.is_multiple_of(4), "{offset:#x} is not on a word boundary in {layout:?}");
            }

            let count = taken.len();
            taken.sort_unstable();
            taken.dedup();
            assert_eq!(taken.len(), count, "two fields share a word in {layout:?}");
        }
    }

    /// The three words 헬싱 writes into the context it fills itself.
    ///
    /// It never calls `MC_grpInitContext` or `MC_grpSetContext`: it writes
    /// `0x6da0` at `+0x14`, `0xf81f` at `+0x1c` and `0x108ce9` at `+0x2c`, and
    /// hands that to every draw. Those are the colour it draws with, the
    /// magenta it keys against and its own pixel operation, so on KTF they
    /// have to be the foreground, the transparent pixel and the operation.
    #[test]
    fn ktf_reads_the_three_words_a_title_fills_itself() {
        let at = ContextLayout::Ktf.offsets();

        assert_eq!(at.fgpxl, 0x14);
        assert_eq!(at.transparent, 0x1c);
        assert_eq!(at.pixel_op_func_ptr, 0x2c);
    }

    /// The words 액션히어로3D's own blitter reads out of the context.
    ///
    /// It clips against `+0x04`..`+0x10`, translates by `+0x24`/`+0x28`, keys
    /// against `+0x1c` and stands down when `+0x2c` holds an operation. The
    /// translation is the one that says the offset is not at the end of the
    /// struct the way LGT keeps it: read from there, the title picked up the
    /// pixel parameter and the font as a translation and carried its logo off
    /// the screen.
    #[test]
    fn ktf_translates_by_the_words_a_title_reads() {
        let at = ContextLayout::Ktf.offsets();

        assert_eq!(at.clip, 0x04);
        assert_eq!(at.offset, 0x24);
        assert_eq!(at.transparent, 0x1c);
        assert_eq!(at.pixel_op_func_ptr, 0x2c);
        assert!(at.keeps_transparent, "the title sets its key through op 3");
    }

    /// 액션히어로3D clips its own blits against `+0x04`..`+0x10`.
    ///
    /// It zeroes its context at startup, sets the clip through the API and
    /// reads the four corners back from those words as left, top, right and
    /// bottom. A word earlier - LGT's place - and the rectangle's right edge
    /// becomes its top clip.
    #[test]
    fn ktf_keeps_the_clip_one_word_in() {
        assert_eq!(ContextLayout::Ktf.offsets().clip, 0x04);
        assert_eq!(ContextLayout::Lgt.offsets().clip, 0x00);
        assert!(!ContextLayout::Lgt.offsets().keeps_transparent, "the reference drops op 3");
    }
}
