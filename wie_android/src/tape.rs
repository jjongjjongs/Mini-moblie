//! Plays the mixer's output faster or slower, the way a tape does.
//!
//! At 2x every output frame takes two frames' worth of the mix, so music,
//! effects and voices all run twice as fast and an octave higher - the same
//! rate the game clock runs at (see [`crate::speed`]), so a jingle a title
//! waits on still ends when the title expects it to. At 1x the mix passes
//! through untouched.
//!
//! Output frames fall between source frames, and each is read off the line
//! between its two neighbours. The source frames an output still needs are
//! kept from one pull to the next, with the fraction of a frame the next pull
//! starts on, so a chunk boundary is not a seam.

use std::collections::VecDeque;

use crate::ma3::CHANNELS;

/// What one pull leaves for the next.
#[derive(Default)]
pub struct Tape {
    /// Source frames not yet passed by, oldest first.
    pub(crate) pending: VecDeque<[i16; CHANNELS]>,
    /// Where the next output frame falls, in frames past the first pending one.
    pub(crate) phase: f64,
}

impl Tape {
    /// `frames` output frames at `speed`, interleaved, or `None` when the
    /// source has nothing sounding and nothing is left over from before.
    ///
    /// `source(n)` renders the next `n` frames of the mix, or `None` while it
    /// is silent.
    pub fn pull(&mut self, frames: usize, speed: f32, mut source: impl FnMut(usize) -> Option<Vec<i16>>) -> Option<Vec<i16>> {
        let speed = f64::from(speed);

        // Real time, and nothing carried over from a pull at another speed:
        // the mix as it is.
        if (speed - 1.0).abs() < 1e-6 && self.pending.is_empty() {
            self.phase = 0.0;
            return source(frames);
        }

        if frames == 0 {
            return None;
        }

        // Back at real time with frames left from a faster or slower pull:
        // play those out first, from the frame nearest to where the tape had
        // got to - half a frame is nothing to hear - and then the mix as it is,
        // so the tape drops out of the path instead of reading every frame off
        // a line from here on.
        if (speed - 1.0).abs() < 1e-6 {
            if self.phase >= 0.5 {
                self.pending.pop_front();
            }
            self.phase = 0.0;

            let taken = frames.min(self.pending.len());
            let mut output: Vec<i16> = self.pending.drain(..taken).flatten().collect();
            if taken < frames {
                match source(frames - taken) {
                    Some(samples) => output.extend(samples),
                    None => output.resize(frames * CHANNELS, 0),
                }
            }

            return Some(output);
        }

        // The last output frame reads the pending frame at its floor and the
        // one after; the frames passed by over the pull can run further still
        // at a high speed.
        let last = self.phase + (frames - 1) as f64 * speed;
        let advance = self.phase + frames as f64 * speed;
        let needed = (last.floor() as usize + 2).max(advance.floor() as usize);

        if self.pending.len() < needed {
            let missing = needed - self.pending.len();
            match source(missing) {
                Some(samples) => {
                    for frame in samples.chunks_exact(CHANNELS) {
                        let mut out = [0i16; CHANNELS];
                        out.copy_from_slice(frame);
                        self.pending.push_back(out);
                    }
                }
                // Gone quiet with nothing left to finish: quiet too, and the
                // next sound starts clean.
                None if self.pending.is_empty() => {
                    self.phase = 0.0;
                    return None;
                }
                None => {}
            }
            // A source that came back short, or silent with a tail still to
            // play, runs out into silence.
            while self.pending.len() < needed {
                self.pending.push_back([0; CHANNELS]);
            }
        }

        let mut output = Vec::with_capacity(frames * CHANNELS);
        for index in 0..frames {
            let at = self.phase + index as f64 * speed;
            let whole = at.floor() as usize;
            let fraction = at - whole as f64;
            let (a, b) = (self.pending[whole], self.pending[whole + 1]);
            for channel in 0..CHANNELS {
                let value = f64::from(a[channel]) + (f64::from(b[channel]) - f64::from(a[channel])) * fraction;
                output.push(value.round() as i16);
            }
        }

        let passed = advance.floor() as usize;
        self.pending.drain(..passed.min(self.pending.len()));
        self.phase = advance - passed as f64;

        Some(output)
    }
}

#[cfg(test)]
mod tests {
    use super::Tape;
    use crate::ma3::CHANNELS;

    /// A ramp, so where an output frame was read from shows in its value.
    fn ramp() -> impl FnMut(usize) -> Option<Vec<i16>> {
        let mut next = 0i16;
        move |frames| {
            let mut samples = Vec::with_capacity(frames * CHANNELS);
            for _ in 0..frames {
                samples.extend([next; CHANNELS]);
                next += 1;
            }
            Some(samples)
        }
    }

    fn lefts(samples: &[i16]) -> Vec<i16> {
        samples.chunks_exact(CHANNELS).map(|frame| frame[0]).collect()
    }

    #[test]
    fn real_time_passes_the_mix_through() {
        let mut tape = Tape::default();
        let mut source = ramp();

        assert_eq!(lefts(&tape.pull(4, 1.0, &mut source).unwrap()), [0, 1, 2, 3]);
        assert_eq!(lefts(&tape.pull(4, 1.0, &mut source).unwrap()), [4, 5, 6, 7]);
    }

    /// Twice as fast takes every other frame, and carries on across pulls
    /// without skipping or repeating one.
    #[test]
    fn double_speed_takes_every_other_frame_across_pulls() {
        let mut tape = Tape::default();
        let mut source = ramp();

        assert_eq!(lefts(&tape.pull(3, 2.0, &mut source).unwrap()), [0, 2, 4]);
        assert_eq!(lefts(&tape.pull(3, 2.0, &mut source).unwrap()), [6, 8, 10]);
    }

    /// Half speed reads between frames.
    #[test]
    fn half_speed_reads_between_frames() {
        let mut tape = Tape::default();
        let mut source = ramp();

        assert_eq!(lefts(&tape.pull(4, 0.5, &mut source).unwrap()), [0, 1, 1, 2]);
        assert_eq!(lefts(&tape.pull(4, 0.5, &mut source).unwrap()), [2, 3, 3, 4]);
    }

    /// Silence with nothing left over is silence, not a buffer of zeros.
    #[test]
    fn a_silent_mix_stays_silent() {
        let mut tape = Tape::default();

        assert!(tape.pull(4, 2.0, |_| None).is_none());
    }

    /// Back to real time, what was already taken from the mix plays out first
    /// and nothing is played twice.
    #[test]
    fn returning_to_real_time_does_not_repeat_the_mix() {
        let mut tape = Tape::default();
        let mut source = ramp();

        assert_eq!(lefts(&tape.pull(2, 1.5, &mut source).unwrap()), [0, 2]);
        let next = lefts(&tape.pull(4, 1.0, &mut source).unwrap());
        assert_eq!(next[0], 3);
        assert!(next.windows(2).all(|pair| pair[1] == pair[0] + 1), "{next:?}");
        assert!(tape.pending.is_empty(), "the tape is still in the path");
    }
}
