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
    /// and our blits key on magenta. It is named because a clet's own pixel
    /// operation reads it out of the context - 헬싱's, at `0x108ce8`, loads
    /// `[context + 0x1c]`, compares the pixel it was given against it and
    /// answers with the other one when they match, which is the transparency
    /// test the whole title draws through.
    pub transparent: WIPICWord,
    pub param1: WIPICWord,
    pub font: WIPICWord,
    pub style: WIPICWord,
    /// `MC_GrpPixelOpProc`, which the reference also plants for XOR mode.
    ///
    /// At `+0x2c`, after the style. 헬싱 is what says so: it fills this struct
    /// itself rather than through `MC_grpSetContext`, and what it leaves there
    /// is `0x108ce9` - the address of its own operation, inside its own image.
    /// Taken from `+0x1c` instead, the transparent pixel `0xf81f` read as an
    /// operation, and calling it took the title down on its first frame.
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
    use super::WIPICGraphicsContextIdx;

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
}
