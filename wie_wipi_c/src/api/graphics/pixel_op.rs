//! The pixel operation a title plants in its graphics context.
//!
//! `MC_grpSetContext(ctx, PixelopIdx, f)` hands the runtime a function of the
//! title's own, and every pixel a draw would write goes through it first:
//! `f(destination, source, param)` answers what to write, where `param` is the
//! context's own `PixelParam1Idx`. A title that draws a glow or a shadow does
//! it this way - there is no blend mode in the API, only this.
//!
//! The reference's `WPGrp_PixelOperation` is where that shape comes from: it
//! loads the destination into the first argument and the source into the
//! second, hands the operation the context's parameter as the third, and
//! stores what comes back over the destination.
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
    /// The first argument inverted, which is what the reference plants when a
    /// title turns XOR mode on: `WPGrp_PutXorPixel` is `mvn r0, r0` and a
    /// return, so the other pixel has no part in it. Drawing the same thing
    /// twice puts back what was there, which is what the mode is for.
    Invert,
    /// The second argument, unchanged. A title whose operation covers a range
    /// of strengths needs one that means "as it was", and this is it.
    Second,
    /// The first argument mixed `weight`/255 of the way towards a grey, on the
    /// eight-bit components `MC_grpGetRGBFromPixel` hands out - a fade to white
    /// or to black. See [`fade`].
    Fade { target: i32, weight: i32 },
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

/// Every bit of the first pixel flipped, the second ignored.
pub fn invert(first: u16, _second: u16) -> u16 {
    !first
}

/// The second pixel, whatever the first was.
pub fn second(_first: u16, second: u16) -> u16 {
    second
}

/// The eight-bit components `MC_grpGetRGBFromPixel` answers with, which spread
/// each field back over 0..255 with rounding rather than by shifting.
fn components(pixel: u16) -> (i32, i32, i32) {
    let spread = |value: u16, max: i32| (value as i32 * 255 + max / 2) / max;

    (spread(red(pixel), 0x1f), spread(green(pixel), 0x3f), spread(blue(pixel), 0x1f))
}

/// The first pixel mixed towards a grey, the way a title writes a fade when the
/// API has no blend mode: unpack with `MC_grpGetRGBFromPixel`, mix each
/// component, pack with `MC_grpGetPixelFromRGB`.
///
/// `weight` is out of 255 and is deliberately not clamped - a title is free to
/// run it past either end, and the truncating signed divide and the eight-bit
/// truncation `MC_grpGetPixelFromRGB` does on its arguments are what the
/// handset's own arithmetic does with the result. Matching it exactly is the
/// point; tidying it up here would draw something the handset never drew.
pub fn fade(first: u16, _second: u16, target: i32, weight: i32) -> u16 {
    let (r, g, b) = components(first);
    let mix = |component: i32| ((component * (255 - weight) + target * weight) / 255) as u32 & 0xff;

    ((((mix(r) >> 3) << 11) | ((mix(g) >> 2) << 5) | (mix(b) >> 3)) & 0xffff) as u16
}

/// The two pixels in the order the title's own operation receives them.
///
/// The two handsets disagree about which comes first - see
/// `WIPICContext::pixel_op_takes_source_first` - and an operation that reads
/// only one of them cannot be recognised, or replayed, without getting that
/// right. Both the probing below and [`apply`] go through here, so they cannot
/// drift apart.
pub fn arguments(source_first: bool, destination: u16, source: u16) -> (u16, u16) {
    if source_first { (source, destination) } else { (destination, source) }
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

/// A wider set, for the models that are fitted rather than fixed.
///
/// A family with free parameters can be talked into agreeing with eight
/// answers by accident - 마스터오브소드4's operation hands back its first pixel
/// for every one of them and only does something for a white one, which reads
/// as a fade of no strength at all. So the fitted models are held to these as
/// well: sixteen colours, each one appearing as the first pixel and as the
/// second, including the extremes an operation is most likely to special-case.
///
/// They are only asked for when the fixed models have already said no, so the
/// titles those cover still pay eight calls and not forty.
const WIDE_PROBES: [(u16, u16); 32] = {
    const COLOURS: [u16; 16] = [
        0x0000, 0xffff, 0xf800, 0x07e0, 0x001f, 0x8410, 0x4208, 0xc618, 0x18c3, 0x39e7, 0x7bef, 0x2145, 0x6b4a, 0xfc00, 0x03ff, 0xfd20,
    ];

    let mut pairs = [(0u16, 0u16); 32];
    let mut index = 0;
    while index < 16 {
        let other = COLOURS[(index + 5) % 16];

        pairs[index * 2] = (COLOURS[index], other);
        pairs[index * 2 + 1] = (other, COLOURS[index]);

        index += 1;
    }

    pairs
};

/// What has already been asked, so a title that sets its operation on every
/// draw - 드래곤하트2 sets one eight thousand times in a capture - is asked
/// once per function rather than once per call.
static KNOWN: Mutex<Vec<((WIPICWord, WIPICWord), PixelOp)>> = Mutex::new(Vec::new());

/// A title that keeps planting new functions must not grow this without end.
///
/// Keyed by the parameter as well as the function, because an operation is
/// free to do something different for each - LOA-혼돈의 서곡's fades to white
/// below one value, hides the draw at another and fades to black above it - so
/// one answer per function would be the wrong answer for most of them. A fade
/// walks its parameter through a handful of steps, so the room is for those.
const KNOWN_LIMIT: usize = 64;

/// Asks the title's operation what it does, and remembers the answer.
pub async fn classify(context: &mut dyn WIPICContext, function: WIPICWord, param: WIPICWord) -> Result<PixelOp> {
    if let Some(known) = KNOWN.lock().iter().find(|(key, _)| *key == (function, param)) {
        return Ok(known.1);
    }

    let source_first = context.pixel_op_takes_source_first();

    let mut asked: Vec<((u16, u16), u16)> = Vec::with_capacity(PROBES.len() + WIDE_PROBES.len());
    let ask = async |context: &mut dyn WIPICContext, asked: &mut Vec<((u16, u16), u16)>, pairs: &[(u16, u16)]| -> Result<()> {
        for &(destination, source) in pairs {
            let (a, b) = arguments(source_first, destination, source);
            let answer = context.call_function(function, &[a as WIPICWord, b as WIPICWord, param]).await?;

            asked.push(((a, b), answer as u16));
        }

        Ok(())
    };

    ask(context, &mut asked, &PROBES).await?;

    let matches = |asked: &[((u16, u16), u16)], model: &dyn Fn(u16, u16) -> u16| asked.iter().all(|&((a, b), answer)| model(a, b) == answer);

    let operation = if matches(&asked, &additive) {
        PixelOp::Additive
    } else if matches(&asked, &screen) {
        PixelOp::Screen
    } else if matches(&asked, &invert) {
        PixelOp::Invert
    } else {
        // Nothing fixed fits, so the fitted models get their turn - and they
        // answer to the wider set as well as this one.
        ask(context, &mut asked, &WIDE_PROBES).await?;

        if matches(&asked, &second) {
            PixelOp::Second
        } else if let Some((target, weight)) = fit_fade(&|model| matches(&asked, model)) {
            PixelOp::Fade { target, weight }
        } else {
            PixelOp::Guest
        }
    };

    tracing::debug!("pixel operation at {function:#x} param {param} is {operation:?}");

    let mut known = KNOWN.lock();
    if known.len() < KNOWN_LIMIT {
        known.push(((function, param), operation));
    }

    Ok(operation)
}

/// Whether a model answers every probe that has been asked.
type Matches<'a> = dyn Fn(&dyn Fn(u16, u16) -> u16) -> bool + 'a;

/// Finds the fade, if the answers are one.
///
/// Only the two greys a fade is ever towards are tried - white and black - and
/// the weight is swept well past either end because a title is free to run it
/// there. Nothing is taken on trust: a candidate is kept only when it answers
/// every probe exactly, and anything that does not is still asked per pixel.
fn fit_fade(matches: &Matches) -> Option<(i32, i32)> {
    for target in [0, 255] {
        for weight in -1024..=1024 {
            if matches(&move |first, second| fade(first, second, target, weight)) {
                return Some((target, weight));
            }
        }
    }

    None
}

/// What a context draws through, if anything.
///
/// A title that plants its own operation gets that one. A title that turns XOR
/// mode on instead gets the built-in the reference plants for it: WIE has no
/// guest address to report back for that one, so the slot a title reads stays
/// empty while the drawing still inverts.
pub async fn of_context(
    context: &mut dyn WIPICContext,
    function: WIPICWord,
    param: WIPICWord,
    xor_mode: WIPICWord,
) -> Result<Option<(PixelOp, WIPICWord)>> {
    if function != 0 {
        return Ok(Some((classify(context, function, param).await?, function)));
    }

    if xor_mode != 0 {
        return Ok(Some((PixelOp::Invert, 0)));
    }

    Ok(None)
}

/// Applies a recognised operation. `Guest` has no answer here - it is asked.
///
/// `source_first` is the handset's argument order, the same one the probing
/// used; see [`arguments`].
pub fn apply(operation: PixelOp, destination: u16, source: u16, source_first: bool) -> Option<u16> {
    let (a, b) = arguments(source_first, destination, source);

    match operation {
        PixelOp::Additive => Some(additive(a, b)),
        PixelOp::Screen => Some(screen(a, b)),
        PixelOp::Invert => Some(invert(a, b)),
        PixelOp::Second => Some(second(a, b)),
        PixelOp::Fade { target, weight } => Some(fade(a, b, target, weight)),
        PixelOp::Guest => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{PixelOp, additive, apply, invert, pack, screen};

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

    /// XOR mode inverts what is there and pays no attention to the source,
    /// which is what `WPGrp_PutXorPixel` does - `mvn r0, r0` and a return. Two
    /// draws put back what was there, which is the point of the mode.
    #[test]
    fn xor_mode_inverts_the_destination_and_ignores_the_source() {
        assert_eq!(invert(0x0000, 0x39e7), 0xffff);
        assert_eq!(invert(0xffff, 0x39e7), 0x0000);

        for destination in [0x0000u16, 0x18c3, 0x39e7, 0x7bef, 0xffff] {
            // The source makes no difference at all.
            assert_eq!(invert(destination, 0x0000), invert(destination, 0xffff));

            // And inverting twice is where it started.
            assert_eq!(invert(invert(destination, 0), 0), destination);
        }
    }

    #[test]
    fn a_guest_operation_has_no_answer_of_its_own() {
        assert_eq!(apply(PixelOp::Additive, 0x1084, 0x1084, false), Some(0x2108));
        assert_eq!(apply(PixelOp::Screen, 0x0000, 0x39e7, false), Some(screen(0x0000, 0x39e7)));
        assert_eq!(apply(PixelOp::Invert, 0x0000, 0x1084, false), Some(0xffff));
        assert_eq!(apply(PixelOp::Guest, 0x1084, 0x1084, false), None);
    }

    /// Which pixel an operation reads is the handset's business, so the two
    /// orderings answer differently for one that reads only its first.
    ///
    /// The commutative ones cannot show this, which is why nothing caught the
    /// probing and the drawing having drifted apart on it.
    #[test]
    fn the_argument_order_reaches_the_answer() {
        assert_eq!(apply(PixelOp::Invert, 0x0000, 0xffff, false), Some(0xffff));
        assert_eq!(apply(PixelOp::Invert, 0x0000, 0xffff, true), Some(0x0000));

        assert_eq!(apply(PixelOp::Second, 0x1234, 0x4321, false), Some(0x4321));
        assert_eq!(apply(PixelOp::Second, 0x1234, 0x4321, true), Some(0x1234));

        // And a commutative one reads the same either way, which is why it
        // stayed right while the order was wrong.
        for source_first in [false, true] {
            assert_eq!(apply(PixelOp::Additive, 0x1084, 0x2108, source_first), Some(additive(0x1084, 0x2108)));
        }
    }

    /// A full-weight fade is the grey it fades to, and a zero-weight one is
    /// what it started as.
    #[test]
    fn a_fade_runs_from_the_pixel_to_the_grey() {
        for pixel in [0x0000u16, 0x18c3, 0x39e7, 0x7bef, 0xf81f, 0xffff] {
            assert_eq!(super::fade(pixel, 0, 0, 255), 0x0000, "{pixel:#06x} to black");
            assert_eq!(super::fade(pixel, 0, 255, 255), 0xffff, "{pixel:#06x} to white");
            assert_eq!(super::fade(pixel, 0, 0, 0), pixel, "{pixel:#06x} unmoved");
            assert_eq!(super::fade(pixel, 0, 255, 0), pixel, "{pixel:#06x} unmoved");
        }
    }

    /// The other pixel has no part in a fade, whatever it is.
    #[test]
    fn a_fade_reads_only_the_pixel_it_fades() {
        for other in [0x0000u16, 0x39e7, 0xffff] {
            assert_eq!(super::fade(0x7bef, other, 0, 128), super::fade(0x7bef, 0x1234, 0, 128));
        }
    }

    /// Halfway to black is about half as bright, in each channel, on the
    /// eight-bit components the API hands out rather than on the packed word.
    #[test]
    fn a_half_fade_is_about_half() {
        let faded = super::fade(0xffff, 0, 0, 128);

        assert!((super::red(faded) as i32 - 15).abs() <= 1, "{faded:#06x}");
        assert!((super::green(faded) as i32 - 31).abs() <= 1, "{faded:#06x}");
        assert!((super::blue(faded) as i32 - 15).abs() <= 1, "{faded:#06x}");
    }
}
