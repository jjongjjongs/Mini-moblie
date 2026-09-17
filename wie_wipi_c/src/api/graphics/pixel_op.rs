//! The pixel operation a title plants in its graphics context.
//!
//! `MC_grpSetContext(ctx, PixelopIdx, f)` hands the runtime a function of the
//! title's own, and every pixel a blit would write goes through it first:
//! `f(destination, source)` answers what to write. A title that draws a glow or
//! a shadow does it this way - there is no blend mode in the API, only this.
//!
//! WIE stored the pointer and drew as though it were not there, so 드래곤하트2's
//! hit effect came down as an opaque blue disc over the field and its ground
//! decals as brown blocks. Turning the title's graphics quality down made them
//! go away, because at low quality it stops drawing them at all.
//!
//! Calling into the guest for every pixel of every blit is what the reference
//! does - it is native ARM there - and here it is an emulated call per pixel.
//! So the two operations titles actually plant are recognised and done in Rust
//! at native speed, and anything else is still asked. The recognition is not a
//! guess at the code: the function is called with pixels whose answers tell the
//! two apart, and it is only believed when every one of them matches.

use alloc::vec::Vec;

use spin::Mutex;
use wie_util::Result;
use wipi_types::wipic::WIPICWord;

use crate::WIPICContext;

/// What a title's pixel operation turned out to be.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PixelOp {
    /// Each RGB565 channel added and held at its maximum - a glow.
    Additive,
    /// `out = max - (max - dst) * (max - src) / max` per channel - a wash that
    /// lightens without ever darkening.
    Screen,
    /// Something else. The title is asked for every pixel.
    Guest,
}

fn red(pixel: u16) -> u16 {
    (pixel >> 11) & 0x1f
}

fn green(pixel: u16) -> u16 {
    (pixel >> 5) & 0x3f
}

fn blue(pixel: u16) -> u16 {
    pixel & 0x1f
}

fn pack(r: u16, g: u16, b: u16) -> u16 {
    ((r & 0x1f) << 11) | ((g & 0x3f) << 5) | (b & 0x1f)
}

/// Saturating add, channel by channel.
pub fn additive(destination: u16, source: u16) -> u16 {
    pack(
        (red(destination) + red(source)).min(0x1f),
        (green(destination) + green(source)).min(0x3f),
        (blue(destination) + blue(source)).min(0x1f),
    )
}

/// The inverses multiplied and inverted back, which is what the title's own
/// code does: the product is shifted down by the channel's width rather than
/// divided by its maximum, so this matches it exactly rather than nearly.
pub fn screen(destination: u16, source: u16) -> u16 {
    let channel = |max: u16, shift: u32, destination: u16, source: u16| {
        let product = (max - destination) * (max - source);
        (max - (product >> shift)) & max
    };

    pack(
        channel(0x1f, 5, red(destination), red(source)),
        channel(0x3f, 6, green(destination), green(source)),
        channel(0x1f, 5, blue(destination), blue(source)),
    )
}

/// Pixel pairs whose answers tell the operations apart.
///
/// Chosen so that neither a plain copy nor either blend agrees with another on
/// all of them: pairs that saturate, pairs that do not, and pairs where one
/// side is black or white.
const PROBES: [(u16, u16); 8] = [
    (0x0000, 0x39e7),
    (0x39e7, 0x0000),
    (0x7bef, 0x4210),
    (0xffff, 0x0841),
    (0x18c3, 0x39e7),
    (0x2145, 0x6b4a),
    (0xf800, 0x001f),
    (0x8410, 0x8410),
];

/// What has already been asked, so a title that sets its operation on every
/// draw - 드래곤하트2 sets one eight thousand times in a capture - is asked
/// once per function rather than once per call.
static KNOWN: Mutex<Vec<(WIPICWord, PixelOp)>> = Mutex::new(Vec::new());

/// A title that keeps planting new functions must not grow this without end.
const KNOWN_LIMIT: usize = 32;

/// Asks the title's operation what it does, and remembers the answer.
pub async fn classify(context: &mut dyn WIPICContext, function: WIPICWord) -> Result<PixelOp> {
    if let Some(known) = KNOWN.lock().iter().find(|(address, _)| *address == function) {
        return Ok(known.1);
    }

    let mut answers = Vec::with_capacity(PROBES.len());
    for (destination, source) in PROBES {
        let answer = context.call_function(function, &[destination as WIPICWord, source as WIPICWord]).await?;

        answers.push(answer as u16);
    }

    let matches = |model: fn(u16, u16) -> u16| {
        PROBES
            .iter()
            .zip(answers.iter())
            .all(|(&(destination, source), &answer)| model(destination, source) == answer)
    };

    let operation = if matches(additive) {
        PixelOp::Additive
    } else if matches(screen) {
        PixelOp::Screen
    } else {
        PixelOp::Guest
    };

    tracing::debug!("pixel operation at {function:#x} is {operation:?}");

    let mut known = KNOWN.lock();
    if known.len() < KNOWN_LIMIT {
        known.push((function, operation));
    }

    Ok(operation)
}

/// Applies a recognised operation. `Guest` has no answer here - it is asked.
pub fn apply(operation: PixelOp, destination: u16, source: u16) -> Option<u16> {
    match operation {
        PixelOp::Additive => Some(additive(destination, source)),
        PixelOp::Screen => Some(screen(destination, source)),
        PixelOp::Guest => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{PixelOp, additive, apply, pack, screen};

    /// The channels add and stop at their maximum rather than wrapping past it.
    #[test]
    fn additive_saturates_each_channel() {
        assert_eq!(additive(0x0000, 0x39e7), 0x39e7);
        assert_eq!(additive(0x39e7, 0x0000), 0x39e7);

        // Every channel already full stays full rather than carrying into the
        // channel above it, which is what wrapping would do.
        assert_eq!(additive(0xffff, 0xffff), 0xffff);
        assert_eq!(additive(0xf800, 0xf800), 0xf800);

        // Half and half, with nothing to saturate.
        assert_eq!(additive(0x1084, 0x1084), 0x2108);
    }

    /// Screening only ever lightens, and white stays white.
    ///
    /// It does not quite leave black alone: the title shifts the product down
    /// by the channel's width rather than dividing by its maximum - 32 where it
    /// means 31 - so screening with black lifts a channel by one. That is the
    /// title's own arithmetic and the point of matching it exactly; rounding it
    /// "correctly" here would draw something the handset never drew.
    #[test]
    fn screen_lightens_and_never_darkens() {
        assert_eq!(screen(0xffff, 0x39e7), 0xffff);
        assert_eq!(screen(0x39e7, 0xffff), 0xffff);

        // Black over black comes out one step up in every channel rather than
        // black, which is the shift showing through.
        assert_eq!(screen(0x0000, 0x0000), pack(1, 1, 1));

        for destination in [0x0000u16, 0x18c3, 0x39e7, 0x7bef, 0xffff] {
            for source in [0x0000u16, 0x2145, 0x6b4a, 0xffff] {
                let out = screen(destination, source);

                assert!(super::red(out) >= super::red(destination));
                assert!(super::green(out) >= super::green(destination));
                assert!(super::blue(out) >= super::blue(destination));
            }
        }
    }

    /// The two are told apart by the probe pairs, which is what the recognition
    /// rests on: if any pair agreed for both, a title's glow could be taken for
    /// a wash.
    #[test]
    fn the_probes_tell_the_operations_apart() {
        assert!(
            super::PROBES
                .iter()
                .any(|&(destination, source)| additive(destination, source) != screen(destination, source))
        );

        // And neither is a plain copy, which is what WIE did before.
        assert!(super::PROBES.iter().any(|&(d, s)| additive(d, s) != s));
        assert!(super::PROBES.iter().any(|&(d, s)| screen(d, s) != s));
    }

    #[test]
    fn a_guest_operation_has_no_answer_of_its_own() {
        assert_eq!(apply(PixelOp::Additive, 0x1084, 0x1084), Some(0x2108));
        assert_eq!(apply(PixelOp::Screen, 0x0000, 0x39e7), Some(screen(0x0000, 0x39e7)));
        assert_eq!(apply(PixelOp::Guest, 0x1084, 0x1084), None);
    }
}
