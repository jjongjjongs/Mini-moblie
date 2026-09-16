use alloc::vec::Vec;

use encoding_rs::EUC_KR;

use crate::time::Instant;

/// How long the same key waits before the next press starts a new character
/// instead of cycling the one in progress.
///
/// A handset had this, and without it the same letter twice in a row cannot be
/// typed at all: every press of the key just advances the cycle. 900ms is what
/// another WIPI runtime measured and shipped.
const COMMIT_DELAY_MS: u64 = 900;

/// The longest a 천지인 vowel's spelling gets: ㅙ is `·ㅡㅣ·ㅣ` and ㅞ is
/// `ㅡ··ㅣㅣ`, both five strokes.
const KOREAN_STROKES: usize = 5;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct KoreanState {
    cho: Option<u8>,
    jung: Option<u8>,
    jong: Option<u8>,
    /// The strokes typed so far for the vowel in progress.
    strokes: [u8; KOREAN_STROKES],
    stroke_len: u8,
    consonant_scan: Option<u8>,
    /// The key the live consonant came from, and how far into that key's ring
    /// it is, so the same key pressed again steps to the next jamo on it.
    consonant_key: Option<i8>,
    ring_index: u8,
    last_key: Option<i8>,
}

/// What a number key does in Korean mode.
enum KoreanPress {
    /// A consonant. `in_place` replaces the live one rather than starting a new
    /// syllable: the same key pressed again inside the multi-tap window, or a
    /// stroke key applied to what is already there.
    Consonant { scan: u8, in_place: bool },
    /// A vowel stroke. `jung` is the vowel the strokes spell so far, which is
    /// nothing yet for a lone dot; `restart` says the stroke could not extend
    /// the vowel in progress, so what is on screen is finished first.
    Vowel { jung: Option<u8>, restart: bool },
}

/// The strokes, as the table below spells them.
const STROKE_I: u8 = 0;
const STROKE_DOT: u8 = 1;
const STROKE_EU: u8 = 2;

/// Every vowel 천지인 spells, and the strokes that spell it.
///
/// Read `I` as ㅣ, `D` as the dot and `E` as ㅡ: ㅏ is ㅣ then a dot, ㅑ is a
/// second dot on that, ㅐ is ㅏ closed with ㅣ. The run in progress extends as
/// long as it is still the start of something here; the first stroke that is
/// not finishes the syllable and starts the next run.
const KOREAN_VOWELS: &[(&[u8], u8)] = &[
    (&[STROKE_I], 29),                                              // ㅣ
    (&[STROKE_I, STROKE_DOT], 3),                                   // ㅏ
    (&[STROKE_I, STROKE_DOT, STROKE_I], 4),                         // ㅐ
    (&[STROKE_I, STROKE_DOT, STROKE_DOT], 5),                       // ㅑ
    (&[STROKE_I, STROKE_DOT, STROKE_DOT, STROKE_I], 6),             // ㅒ
    (&[STROKE_DOT, STROKE_I], 7),                                   // ㅓ
    (&[STROKE_DOT, STROKE_I, STROKE_I], 10),                        // ㅔ
    (&[STROKE_DOT, STROKE_DOT, STROKE_I], 11),                      // ㅕ
    (&[STROKE_DOT, STROKE_DOT, STROKE_I, STROKE_I], 12),            // ㅖ
    (&[STROKE_DOT, STROKE_EU], 13),                                 // ㅗ
    (&[STROKE_DOT, STROKE_EU, STROKE_I], 18),                       // ㅚ
    (&[STROKE_DOT, STROKE_EU, STROKE_I, STROKE_DOT], 14),           // ㅘ
    (&[STROKE_DOT, STROKE_EU, STROKE_I, STROKE_DOT, STROKE_I], 15), // ㅙ
    (&[STROKE_DOT, STROKE_DOT, STROKE_EU], 19),                     // ㅛ
    (&[STROKE_EU], 27),                                             // ㅡ
    (&[STROKE_EU, STROKE_I], 28),                                   // ㅢ
    (&[STROKE_EU, STROKE_DOT], 20),                                 // ㅜ
    (&[STROKE_EU, STROKE_DOT, STROKE_I], 23),                       // ㅟ
    (&[STROKE_EU, STROKE_DOT, STROKE_DOT], 26),                     // ㅠ
    (&[STROKE_EU, STROKE_DOT, STROKE_DOT, STROKE_I], 21),           // ㅝ
    (&[STROKE_EU, STROKE_DOT, STROKE_DOT, STROKE_I, STROKE_I], 22), // ㅞ
];

#[derive(Default)]
pub struct InputMethod {
    current_mode: u32,
    composition_size: usize,
    eng_key: Option<i8>,
    eng_index: usize,
    eng_char: Option<u8>,
    /// When the last multi-tap key was pressed, so the same key pressed again
    /// after [`COMMIT_DELAY`] starts a new character instead of cycling.
    eng_last_press: Option<Instant>,

    ko_cho: Option<u8>,
    ko_jung: Option<u8>,
    ko_jong: Option<u8>,
    ko_strokes: [u8; KOREAN_STROKES],
    ko_stroke_len: u8,
    ko_consonant_scan: Option<u8>,
    ko_consonant_key: Option<i8>,
    ko_ring_index: u8,
    ko_last_key: Option<i8>,
    ko_undo: Vec<KoreanState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputMethodOutput {
    pub handled: bool,
    pub output0: [u8; 8],
    pub output0_len: usize,
    pub output1: [u8; 8],
    pub output1_len: usize,
}

impl Default for InputMethodOutput {
    fn default() -> Self {
        Self {
            handled: false,
            output0: [0; 8],
            output0_len: 0,
            output1: [0; 8],
            output1_len: 0,
        }
    }
}

impl InputMethod {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current_mode(&self) -> u32 {
        self.current_mode
    }

    pub fn composition_size(&self) -> usize {
        self.composition_size
    }

    pub fn set_composition_size(&mut self, size: usize) {
        self.composition_size = size;
    }

    pub fn set_current_mode(&mut self, mode: u32) {
        self.current_mode = mode;
        self.eng_key = None;
        self.eng_index = 0;
        self.eng_char = None;

        self.ko_cho = None;
        self.ko_jung = None;
        self.ko_jong = None;
        self.ko_strokes = [0; KOREAN_STROKES];
        self.ko_stroke_len = 0;
        self.ko_consonant_scan = None;
        self.ko_consonant_key = None;
        self.ko_ring_index = 0;
        self.ko_last_key = None;
        self.ko_undo.clear();
    }

    pub fn handle_input(&mut self, key: i8, event: u32, now: Instant) -> InputMethodOutput {
        if !matches!(event, 2 | 4) {
            return InputMethodOutput::default();
        }

        match self.current_mode {
            0 | 1 => self.handle_english(key, now),
            2 => Self::handle_numeric(key),
            3 => self.handle_korean(key),
            _ => InputMethodOutput::default(),
        }
    }

    /// Presses a key with no guest time passing, which is what every case
    /// written before the commit delay existed meant: the same key pressed
    /// again cycles.
    #[cfg(test)]
    fn press(&mut self, key: i8, event: u32) -> InputMethodOutput {
        self.handle_input(key, event, Instant::from_epoch_millis(0))
    }

    /// Whether the multi-tap character in progress has been left alone long
    /// enough to be finished.
    ///
    /// Measured on the guest clock rather than the host's, so a frontend that
    /// runs a batch of ticks at once types the same text as one running live.
    fn commit_delay_elapsed(last: Option<Instant>, now: Instant) -> bool {
        let Some(last) = last else {
            return true;
        };

        now.raw().saturating_sub(last.raw()) >= COMMIT_DELAY_MS
    }

    fn handle_english(&mut self, key: i8, now: Instant) -> InputMethodOutput {
        if key == -99 {
            let Some(current) = self.eng_char.take() else {
                return InputMethodOutput::default();
            };

            self.eng_key = None;
            self.eng_index = 0;
            self.eng_last_press = None;

            let mut output = InputMethodOutput::default();
            output.output0[0] = current;
            output.output0_len = 1;
            return output;
        }

        let upper = self.current_mode == 1;
        let chars: &[u8] = match key {
            48 => b".,?!",
            49 => b"@:/",
            50 => {
                if upper {
                    b"ABC"
                } else {
                    b"abc"
                }
            }
            51 => {
                if upper {
                    b"DEF"
                } else {
                    b"def"
                }
            }
            52 => {
                if upper {
                    b"GHI"
                } else {
                    b"ghi"
                }
            }
            53 => {
                if upper {
                    b"JKL"
                } else {
                    b"jkl"
                }
            }
            54 => {
                if upper {
                    b"MNO"
                } else {
                    b"mno"
                }
            }
            55 => {
                if upper {
                    b"PQRS"
                } else {
                    b"pqrs"
                }
            }
            56 => {
                if upper {
                    b"TUV"
                } else {
                    b"tuv"
                }
            }
            57 => {
                if upper {
                    b"WXYZ"
                } else {
                    b"wxyz"
                }
            }
            42 => b"*",
            35 => b"#",
            _ => return InputMethodOutput::default(),
        };

        let mut output = InputMethodOutput {
            handled: true,
            ..InputMethodOutput::default()
        };

        // The same key cycles only while the last press is still recent. Once
        // the delay has run out the character it was building is finished and
        // this press starts the next one, which is what makes two of the same
        // letter in a row typable at all.
        let still_cycling = self.eng_key == Some(key) && !Self::commit_delay_elapsed(self.eng_last_press, now);

        if still_cycling {
            self.eng_index = (self.eng_index + 1) % chars.len();
        } else {
            if let Some(previous) = self.eng_char {
                output.output0[0] = previous;
                output.output0_len = 1;
            }

            self.eng_key = Some(key);
            self.eng_index = 0;
        }

        self.eng_last_press = Some(now);

        let current = chars[self.eng_index];
        self.eng_char = Some(current);
        output.output1[0] = current;
        output.output1_len = 1;
        output
    }

    fn korean_cho_index(cho: u8) -> Option<u32> {
        match cho {
            2 => Some(0),   // ㄱ
            3 => Some(1),   // ㄲ
            4 => Some(2),   // ㄴ
            5 => Some(3),   // ㄷ
            6 => Some(4),   // ㄸ
            7 => Some(5),   // ㄹ
            8 => Some(6),   // ㅁ
            9 => Some(7),   // ㅂ
            10 => Some(8),  // ㅃ
            11 => Some(9),  // ㅅ
            12 => Some(10), // ㅆ
            13 => Some(11), // ㅇ
            14 => Some(12), // ㅈ
            15 => Some(13), // ㅉ
            16 => Some(14), // ㅊ
            17 => Some(15), // ㅋ
            18 => Some(16), // ㅌ
            19 => Some(17), // ㅍ
            20 => Some(18), // ㅎ
            _ => None,
        }
    }

    fn korean_jung_index(jung: u8) -> Option<u32> {
        match jung {
            3 => Some(0),   // ㅏ
            4 => Some(1),   // ㅐ
            5 => Some(2),   // ㅑ
            6 => Some(3),   // ㅒ
            7 => Some(4),   // ㅓ
            10 => Some(5),  // ㅔ
            11 => Some(6),  // ㅕ
            12 => Some(7),  // ㅖ
            13 => Some(8),  // ㅗ
            14 => Some(9),  // ㅘ
            15 => Some(10), // ㅙ
            18 => Some(11), // ㅚ
            19 => Some(12), // ㅛ
            20 => Some(13), // ㅜ
            21 => Some(14), // ㅝ
            22 => Some(15), // ㅞ
            23 => Some(16), // ㅟ
            26 => Some(17), // ㅠ
            27 => Some(18), // ㅡ
            28 => Some(19), // ㅢ
            29 => Some(20), // ㅣ
            _ => None,
        }
    }

    fn korean_jong_index(jong: u8) -> Option<u32> {
        match jong {
            1 => Some(0),
            2 => Some(1),
            3 => Some(2),
            4 => Some(3),
            5 => Some(4),
            6 => Some(5),
            7 => Some(6),
            8 => Some(7),
            9 => Some(8),
            10 => Some(9),
            11 => Some(10),
            12 => Some(11),
            13 => Some(12),
            14 => Some(13),
            15 => Some(14),
            16 => Some(15),
            17 => Some(16),
            19 => Some(17),
            20 => Some(18),
            21 => Some(19),
            22 => Some(20),
            23 => Some(21),
            24 => Some(22),
            25 => Some(23),
            26 => Some(24),
            27 => Some(25),
            28 => Some(26),
            29 => Some(27),
            _ => None,
        }
    }

    fn encode_korean_char(ch: char) -> Option<([u8; 2], usize)> {
        let mut utf8 = [0u8; 4];
        let text = ch.encode_utf8(&mut utf8);
        let (encoded, _, had_errors) = EUC_KR.encode(text);
        if had_errors || encoded.is_empty() || encoded.len() > 2 {
            return None;
        }

        let mut bytes = [0u8; 2];
        bytes[..encoded.len()].copy_from_slice(&encoded);
        Some((bytes, encoded.len()))
    }

    fn compose_korean_syllable(cho: u8, jung: u8, jong: u8) -> Option<char> {
        let cho = Self::korean_cho_index(cho)?;
        let jung = Self::korean_jung_index(jung)?;
        let jong = Self::korean_jong_index(jong)?;
        char::from_u32(0xac00 + (cho * 21 + jung) * 28 + jong)
    }

    fn korean_jong_to_cho(jong: u8) -> Option<u8> {
        match jong {
            2 => Some(2),
            3 => Some(3),
            5 => Some(4),
            8 => Some(5),
            9 => Some(7),
            17 => Some(8),
            19 => Some(9),
            21 => Some(11),
            22 => Some(12),
            23 => Some(13),
            24 => Some(14),
            25 => Some(16),
            26 => Some(17),
            27 => Some(18),
            28 => Some(19),
            29 => Some(20),
            _ => None,
        }
    }

    fn combine_korean_jong(first: u8, second: u8) -> Option<u8> {
        match (first, second) {
            (2, 21) => Some(4),   // ㄱ + ㅅ -> ㄳ
            (5, 24) => Some(6),   // ㄴ + ㅈ -> ㄵ
            (5, 29) => Some(7),   // ㄴ + ㅎ -> ㄶ
            (9, 2) => Some(10),   // ㄹ + ㄱ -> ㄺ
            (9, 17) => Some(11),  // ㄹ + ㅁ -> ㄻ
            (9, 19) => Some(12),  // ㄹ + ㅂ -> ㄼ
            (9, 21) => Some(13),  // ㄹ + ㅅ -> ㄽ
            (9, 27) => Some(14),  // ㄹ + ㅌ -> ㄾ
            (9, 28) => Some(15),  // ㄹ + ㅍ -> ㄿ
            (9, 29) => Some(16),  // ㄹ + ㅎ -> ㅀ
            (19, 21) => Some(20), // ㅂ + ㅅ -> ㅄ
            _ => None,
        }
    }

    fn korean_scan_to_jong(scan: u8) -> Option<u8> {
        match scan {
            2 => Some(2),   // ㄱ
            3 => Some(3),   // ㄲ
            4 => Some(5),   // ㄴ
            5 => Some(8),   // ㄷ
            7 => Some(9),   // ㄹ
            8 => Some(17),  // ㅁ
            9 => Some(19),  // ㅂ
            11 => Some(21), // ㅅ
            12 => Some(22), // ㅆ
            13 => Some(23), // ㅇ
            14 => Some(24), // ㅈ
            16 => Some(25), // ㅊ
            17 => Some(26), // ㅋ
            18 => Some(27), // ㅌ
            19 => Some(28), // ㅍ
            20 => Some(29), // ㅎ
            _ => None,
        }
    }

    /// The jamo each number key steps through, in the order the key is engraved
    /// and a handset steps through them: 4 is ㄱ, ㅋ, ㄲ.
    fn korean_consonant_ring(key: i8) -> Option<&'static [u8]> {
        Some(match key {
            52 => &[2, 17, 3],   // ㄱ ㅋ ㄲ
            53 => &[4, 7],       // ㄴ ㄹ
            54 => &[5, 18, 6],   // ㄷ ㅌ ㄸ
            55 => &[9, 19, 10],  // ㅂ ㅍ ㅃ
            56 => &[11, 20, 12], // ㅅ ㅎ ㅆ
            57 => &[14, 16, 15], // ㅈ ㅊ ㅉ
            48 => &[13, 8],      // ㅇ ㅁ
            _ => return None,
        })
    }

    /// The three strokes every 천지인 vowel is spelled out of: 사람 (ㅣ), 하늘
    /// (the dot) and 땅 (ㅡ), on 1, 2 and 3.
    fn korean_stroke(key: i8) -> Option<u8> {
        Some(match key {
            49 => STROKE_I,
            50 => STROKE_DOT,
            51 => STROKE_EU,
            _ => return None,
        })
    }

    /// What a run of strokes spells.
    ///
    /// `None` means no vowel begins that way, so the run has to start over.
    /// `Some(None)` is a run on its way to a vowel without being one yet - the
    /// lone dot, and the two dots before ㅕ - which leaves the syllable showing
    /// its consonant alone, the way a handset does.
    fn korean_vowel_for_strokes(strokes: &[u8]) -> Option<Option<u8>> {
        let mut is_prefix = false;

        for (spelling, jung) in KOREAN_VOWELS {
            if *spelling == strokes {
                return Some(Some(*jung));
            }
            if spelling.starts_with(strokes) {
                is_prefix = true;
            }
        }

        if is_prefix { Some(None) } else { None }
    }

    fn korean_cho_char(cho: u8) -> Option<char> {
        match cho {
            2 => Some('ㄱ'),
            3 => Some('ㄲ'),
            4 => Some('ㄴ'),
            5 => Some('ㄷ'),
            6 => Some('ㄸ'),
            7 => Some('ㄹ'),
            8 => Some('ㅁ'),
            9 => Some('ㅂ'),
            10 => Some('ㅃ'),
            11 => Some('ㅅ'),
            12 => Some('ㅆ'),
            13 => Some('ㅇ'),
            14 => Some('ㅈ'),
            15 => Some('ㅉ'),
            16 => Some('ㅊ'),
            17 => Some('ㅋ'),
            18 => Some('ㅌ'),
            19 => Some('ㅍ'),
            20 => Some('ㅎ'),
            _ => None,
        }
    }

    fn korean_jung_char(jung: u8) -> Option<char> {
        match jung {
            3 => Some('ㅏ'),
            4 => Some('ㅐ'),
            5 => Some('ㅑ'),
            6 => Some('ㅒ'),
            7 => Some('ㅓ'),
            10 => Some('ㅔ'),
            11 => Some('ㅕ'),
            12 => Some('ㅖ'),
            13 => Some('ㅗ'),
            14 => Some('ㅘ'),
            15 => Some('ㅙ'),
            18 => Some('ㅚ'),
            19 => Some('ㅛ'),
            20 => Some('ㅜ'),
            21 => Some('ㅝ'),
            22 => Some('ㅞ'),
            23 => Some('ㅟ'),
            26 => Some('ㅠ'),
            27 => Some('ㅡ'),
            28 => Some('ㅢ'),
            29 => Some('ㅣ'),
            _ => None,
        }
    }

    fn split_korean_jong(jong: u8) -> Option<(u8, u8)> {
        match jong {
            4 => Some((2, 21)),
            6 => Some((5, 24)),
            7 => Some((5, 29)),
            10 => Some((9, 2)),
            11 => Some((9, 17)),
            12 => Some((9, 19)),
            13 => Some((9, 21)),
            14 => Some((9, 27)),
            15 => Some((9, 28)),
            16 => Some((9, 29)),
            20 => Some((19, 21)),
            _ => None,
        }
    }

    fn current_korean_char(&self) -> Option<char> {
        match (self.ko_cho, self.ko_jung) {
            (Some(cho), Some(jung)) => Self::compose_korean_syllable(cho, jung, self.ko_jong.unwrap_or(1)),
            (Some(cho), None) => Self::korean_cho_char(cho),
            (None, Some(jung)) => Self::korean_jung_char(jung),
            (None, None) => None,
        }
    }

    fn put_korean_char(output: &mut [u8; 8], output_len: &mut usize, ch: char) -> bool {
        let Some((bytes, len)) = Self::encode_korean_char(ch) else {
            return false;
        };

        output[..len].copy_from_slice(&bytes[..len]);
        *output_len = len;
        true
    }

    fn reset_korean_composition(&mut self) {
        self.ko_cho = None;
        self.ko_jung = None;
        self.ko_jong = None;
        self.ko_consonant_scan = None;
    }

    fn korean_state(&self) -> KoreanState {
        KoreanState {
            cho: self.ko_cho,
            jung: self.ko_jung,
            jong: self.ko_jong,
            strokes: self.ko_strokes,
            stroke_len: self.ko_stroke_len,
            consonant_scan: self.ko_consonant_scan,
            consonant_key: self.ko_consonant_key,
            ring_index: self.ko_ring_index,
            last_key: self.ko_last_key,
        }
    }

    fn restore_korean_state(&mut self, state: KoreanState) {
        self.ko_cho = state.cho;
        self.ko_jung = state.jung;
        self.ko_jong = state.jong;
        self.ko_strokes = state.strokes;
        self.ko_stroke_len = state.stroke_len;
        self.ko_consonant_scan = state.consonant_scan;
        self.ko_consonant_key = state.consonant_key;
        self.ko_ring_index = state.ring_index;
        self.ko_last_key = state.last_key;
    }

    fn clear_korean_input(&mut self) -> InputMethodOutput {
        let state = self.ko_undo.pop().unwrap_or_default();
        self.restore_korean_state(state);

        let mut output = InputMethodOutput {
            handled: true,
            ..InputMethodOutput::default()
        };

        if let Some(ch) = self.current_korean_char() {
            Self::put_korean_char(&mut output.output1, &mut output.output1_len, ch);
        }

        output
    }

    /// Reads a number key as the jamo it writes.
    ///
    /// The consonant keys carry a ring - 4 is ㄱ, ㅋ, ㄲ and back to ㄱ - which
    /// the same key steps through while the multi-tap window is open, exactly
    /// as the Latin ring does. ✱ and # reach the same jamo the other way, by
    /// adding a stroke or doubling what is live, which is what a handset offers
    /// beside the ring.
    /// Reads a number key as the jamo it writes.
    ///
    /// The consonant keys carry a ring: one press is the jamo on the left of
    /// the key, a second is the one on the right, and on every key but ㄴㄹ and
    /// ㅇㅁ - the two whose jamo have no tense form - a third is that tense
    /// form. A fourth comes back round to the first.
    ///
    /// There is no timer on this. 천지인 steps the ring on the same key however
    /// long the gap, which is why the same consonant twice in a row needs a
    /// press that finishes the character in between - a direction key here,
    /// which the handler sends in as the flush sentinel. Handing the ring a
    /// timeout instead would mean 4 pressed twice slowly wrote ㄱㄱ where the
    /// pad says ㅋ.
    fn press_korean_key(&mut self, key: i8) -> Option<KoreanPress> {
        if let Some(ring) = Self::korean_consonant_ring(key) {
            let cycling = self.ko_consonant_key == Some(key);
            let index = if cycling { (self.ko_ring_index as usize + 1) % ring.len() } else { 0 };

            self.ko_consonant_key = Some(key);
            self.ko_ring_index = index as u8;
            self.ko_consonant_scan = Some(ring[index]);
            self.ko_last_key = Some(key);
            // A consonant ends whatever vowel was being spelled.
            self.ko_stroke_len = 0;

            return Some(KoreanPress::Consonant {
                scan: ring[index],
                in_place: cycling,
            });
        }

        let stroke = Self::korean_stroke(key)?;

        self.ko_consonant_key = None;
        self.ko_last_key = Some(key);

        let length = self.ko_stroke_len as usize;
        if length < KOREAN_STROKES {
            let mut extended = self.ko_strokes;
            extended[length] = stroke;

            if let Some(jung) = Self::korean_vowel_for_strokes(&extended[..length + 1]) {
                self.ko_strokes = extended;
                self.ko_stroke_len = (length + 1) as u8;

                return Some(KoreanPress::Vowel { jung, restart: false });
            }
        }

        self.ko_strokes[0] = stroke;
        self.ko_stroke_len = 1;
        let jung = Self::korean_vowel_for_strokes(&self.ko_strokes[..1]).flatten();

        Some(KoreanPress::Vowel { jung, restart: true })
    }

    fn handle_korean(&mut self, key: i8) -> InputMethodOutput {
        if key == -99 {
            let mut output = InputMethodOutput::default();
            if let Some(ch) = self.current_korean_char() {
                Self::put_korean_char(&mut output.output0, &mut output.output0_len, ch);
            }
            self.reset_korean_composition();
            self.ko_stroke_len = 0;
            self.ko_consonant_key = None;
            self.ko_last_key = None;
            self.ko_undo.clear();
            return output;
        }

        if key == -16 {
            return self.clear_korean_input();
        }

        if key == 35 {
            // # is the space bar on a 천지인 pad, and pressing it finishes the
            // character in progress the way any commit does - so it is also the
            // other way to type the same consonant twice, one that reads off
            // the pad rather than off a direction key.
            let mut output = InputMethodOutput {
                handled: true,
                ..InputMethodOutput::default()
            };

            if let Some(ch) = self.current_korean_char() {
                Self::put_korean_char(&mut output.output0, &mut output.output0_len, ch);
            }

            output.output0[output.output0_len] = b' ';
            output.output0_len += 1;

            self.reset_korean_composition();
            self.ko_stroke_len = 0;
            self.ko_consonant_key = None;
            self.ko_last_key = Some(key);
            self.ko_undo.clear();

            return output;
        }

        // Capture before press_korean_key mutates the key/stroke metadata.
        let before = self.korean_state();

        let Some(press) = self.press_korean_key(key) else {
            return InputMethodOutput::default();
        };

        // A commit normally rebases CLEAR to an empty composition. Jong split
        // is the exception: part of the old jong becomes the new live cho.
        let mut commit_baseline = KoreanState::default();

        let mut output = InputMethodOutput {
            handled: true,
            ..InputMethodOutput::default()
        };

        match press {
            KoreanPress::Consonant { scan, in_place } => {
                if in_place {
                    if self.ko_jong.is_some() {
                        if let Some(jong) = Self::korean_scan_to_jong(scan) {
                            self.ko_jong = Some(jong);
                        }
                    } else if self.ko_cho.is_some() {
                        self.ko_cho = Some(scan);
                    }

                    if let Some(ch) = self.current_korean_char() {
                        Self::put_korean_char(&mut output.output1, &mut output.output1_len, ch);
                    }
                    self.ko_undo.push(before);
                    return output;
                }

                self.ko_consonant_scan = Some(scan);

                match (self.ko_cho, self.ko_jung, self.ko_jong) {
                    (None, None, None) => {
                        self.ko_cho = Some(scan);
                    }
                    (Some(_), None, None) => {
                        if let Some(ch) = self.current_korean_char() {
                            Self::put_korean_char(&mut output.output0, &mut output.output0_len, ch);
                        }
                        self.reset_korean_composition();
                        self.ko_cho = Some(scan);
                        self.ko_consonant_scan = Some(scan);
                    }
                    (Some(_), Some(_), None) => {
                        if let Some(jong) = Self::korean_scan_to_jong(scan) {
                            self.ko_jong = Some(jong);
                        } else {
                            if let Some(ch) = self.current_korean_char() {
                                Self::put_korean_char(&mut output.output0, &mut output.output0_len, ch);
                            }
                            self.reset_korean_composition();
                            self.ko_cho = Some(scan);
                            self.ko_consonant_scan = Some(scan);
                        }
                    }
                    (Some(_), Some(_), Some(jong)) => {
                        let combined = Self::korean_scan_to_jong(scan).and_then(|second| Self::combine_korean_jong(jong, second));

                        if let Some(combined) = combined {
                            self.ko_jong = Some(combined);
                        } else {
                            if let Some(ch) = self.current_korean_char() {
                                Self::put_korean_char(&mut output.output0, &mut output.output0_len, ch);
                            }
                            self.reset_korean_composition();
                            self.ko_cho = Some(scan);
                            self.ko_consonant_scan = Some(scan);
                        }
                    }
                    _ => {
                        self.reset_korean_composition();
                        self.ko_cho = Some(scan);
                        self.ko_consonant_scan = Some(scan);
                    }
                }
            }
            KoreanPress::Vowel { jung, restart } => {
                if let Some(jong) = self.ko_jong {
                    // The vowel belongs to the next syllable, so the consonant
                    // it was sitting under moves there with it.
                    let (Some(old_cho), Some(old_jung)) = (self.ko_cho, self.ko_jung) else {
                        return InputMethodOutput::default();
                    };

                    let (kept, carried) = Self::split_korean_jong(jong).map_or((1, jong), |(first, second)| (first, second));

                    if let Some(committed) = Self::compose_korean_syllable(old_cho, old_jung, kept) {
                        Self::put_korean_char(&mut output.output0, &mut output.output0_len, committed);
                    }

                    self.reset_korean_composition();
                    self.ko_cho = Self::korean_jong_to_cho(carried);
                    self.ko_consonant_scan = self.ko_cho;
                    commit_baseline = self.korean_state();
                    commit_baseline.last_key = before.last_key;
                    // CLEAR back to here is back to the carried consonant
                    // alone, not to it with half a vowel still pending.
                    commit_baseline.stroke_len = 0;

                    self.ko_jung = jung;
                } else if restart {
                    if let Some(ch) = self.current_korean_char() {
                        Self::put_korean_char(&mut output.output0, &mut output.output0_len, ch);
                    }

                    self.reset_korean_composition();
                    self.ko_jung = jung;
                } else {
                    self.ko_jung = jung;
                }
            }
        }

        if let Some(ch) = self.current_korean_char() {
            Self::put_korean_char(&mut output.output1, &mut output.output1_len, ch);
        }

        if output.output0_len != 0 {
            self.ko_undo.clear();
            self.ko_undo.push(commit_baseline);
        } else {
            self.ko_undo.push(before);
        }

        output
    }

    fn handle_numeric(key: i8) -> InputMethodOutput {
        let byte = match key {
            48..=57 | 42 | 35 => key as u8,
            _ => return InputMethodOutput::default(),
        };

        let mut output = InputMethodOutput {
            handled: true,
            ..InputMethodOutput::default()
        };
        output.output0[0] = byte;
        output.output0_len = 1;
        output
    }
}

#[cfg(test)]
mod tests {
    use super::InputMethod;

    #[test]
    fn numeric_mode_matches_native_key_filtering() {
        let mut input = InputMethod::new();
        input.set_current_mode(2);

        for key in [b'0', b'1', b'9', b'*', b'#'] {
            let output = input.press(key as i8, 2);
            assert!(output.handled);
            assert_eq!(output.output0_len, 1);
            assert_eq!(output.output0[0], key);
            assert_eq!(output.output1_len, 0);
        }

        assert!(!input.press(-99, 2).handled);
        assert!(!input.press(b'A' as i8, 2).handled);
        assert!(!input.press(b'1' as i8, 3).handled);
    }
}

#[cfg(test)]
mod commit_delay_tests {
    use super::{COMMIT_DELAY_MS, InputMethod};
    use crate::time::Instant;

    fn at(ms: u64) -> Instant {
        Instant::from_epoch_millis(ms)
    }

    /// The same key pressed again after the delay finishes the character it was
    /// building and starts the next one. Without this there is no way to type
    /// two of the same letter in a row - every press just advances the cycle.
    #[test]
    fn the_same_key_twice_slowly_types_the_letter_twice() {
        let mut input = InputMethod::new();
        input.set_current_mode(0);

        let first = input.handle_input(b'7' as i8, 2, at(0));
        assert_eq!(&first.output1[..first.output1_len], b"p");
        assert_eq!(first.output0_len, 0);

        let second = input.handle_input(b'7' as i8, 2, at(COMMIT_DELAY_MS));
        // The first `p` is finished and handed over, and a second one starts.
        assert_eq!(&second.output0[..second.output0_len], b"p");
        assert_eq!(&second.output1[..second.output1_len], b"p");
    }

    /// Inside the delay it is still one character being cycled, which is what
    /// multi-tap is.
    #[test]
    fn the_same_key_twice_quickly_cycles_one_character() {
        let mut input = InputMethod::new();
        input.set_current_mode(0);

        input.handle_input(b'7' as i8, 2, at(0));
        let second = input.handle_input(b'7' as i8, 2, at(COMMIT_DELAY_MS - 1));

        assert_eq!(second.output0_len, 0);
        assert_eq!(&second.output1[..second.output1_len], b"q");
    }

    /// A different key finishes the character however little time has passed,
    /// which is the behaviour that was there before the delay was.
    #[test]
    fn a_different_key_still_commits_at_once() {
        let mut input = InputMethod::new();
        input.set_current_mode(0);

        input.handle_input(b'7' as i8, 2, at(0));
        let second = input.handle_input(b'2' as i8, 2, at(1));

        assert_eq!(&second.output0[..second.output0_len], b"p");
        assert_eq!(&second.output1[..second.output1_len], b"a");
    }

    /// A flush ends the character and the delay with it, so the next press of
    /// the same key starts clean rather than carrying the old cycle.
    #[test]
    fn a_flush_clears_the_pending_press() {
        let mut input = InputMethod::new();
        input.set_current_mode(0);

        input.handle_input(b'7' as i8, 2, at(0));
        let flushed = input.handle_input(-99, 2, at(1));
        assert_eq!(&flushed.output0[..flushed.output0_len], b"p");

        let next = input.handle_input(b'7' as i8, 2, at(2));
        assert_eq!(next.output0_len, 0);
        assert_eq!(&next.output1[..next.output1_len], b"p");
    }
}

#[cfg(test)]
mod english_tests {
    use super::InputMethod;

    #[test]
    fn english_modes_match_native_multitap_state() {
        let mut input = InputMethod::new();

        input.set_current_mode(0);

        let a = input.press(b'2' as i8, 2);
        assert!(a.handled);
        assert_eq!(&a.output1[..a.output1_len], b"a");
        assert_eq!(a.output0_len, 0);

        let b = input.press(b'2' as i8, 2);
        assert!(b.handled);
        assert_eq!(&b.output1[..b.output1_len], b"b");
        assert_eq!(b.output0_len, 0);

        let c = input.press(b'2' as i8, 2);
        assert_eq!(&c.output1[..c.output1_len], b"c");

        let a_again = input.press(b'2' as i8, 2);
        assert_eq!(&a_again.output1[..a_again.output1_len], b"a");

        let d = input.press(b'3' as i8, 2);
        assert!(d.handled);
        assert_eq!(&d.output0[..d.output0_len], b"a");
        assert_eq!(&d.output1[..d.output1_len], b"d");

        let flush = input.press(-99, 2);
        assert!(!flush.handled);
        assert_eq!(&flush.output0[..flush.output0_len], b"d");
        assert_eq!(flush.output1_len, 0);

        input.set_current_mode(1);
        let upper = input.press(b'7' as i8, 2);
        assert!(upper.handled);
        assert_eq!(&upper.output1[..upper.output1_len], b"P");
    }
}

#[cfg(test)]
mod korean_input_tests {
    use alloc::vec::Vec;

    use super::{InputMethod, InputMethodOutput};

    /// ㄱ, ㅣ, then the dot that turns ㅣ into ㅏ: 가. A consonant after that
    /// lands under it as 각, and the vowel after that carries the ㄱ out of the
    /// jong into the next syllable.
    #[test]
    fn korean_mode_composes_and_commits_syllables() {
        let mut input = InputMethod::new();
        input.set_current_mode(3);

        let giyeok = input.press(b'4' as i8, 2);
        assert!(giyeok.handled);
        assert_eq!(&giyeok.output1[..giyeok.output1_len], &[0xa4, 0xa1]); // ㄱ

        let gi = input.press(b'1' as i8, 2);
        assert_eq!(&gi.output1[..gi.output1_len], &[0xb1, 0xe2]); // 기

        let ga = input.press(b'2' as i8, 2);
        assert_eq!(&ga.output1[..ga.output1_len], &[0xb0, 0xa1]); // 가

        let gak = input.press(b'4' as i8, 2);
        assert_eq!(&gak.output1[..gak.output1_len], &[0xb0, 0xa2]); // 각

        let split = input.press(b'1' as i8, 2);
        assert_eq!(&split.output0[..split.output0_len], &[0xb0, 0xa1]); // 가
        assert_eq!(&split.output1[..split.output1_len], &[0xb1, 0xe2]); // 기
    }

    /// The key pressed again steps to the next jamo engraved on it - 4 is ㄱ,
    /// ㅋ, ㄲ - and the fourth press comes back round to ㄱ.
    #[test]
    fn a_key_pressed_again_steps_through_what_is_engraved_on_it() {
        let mut input = InputMethod::new();
        input.set_current_mode(3);

        let expected: [[u8; 2]; 4] = [[0xa4, 0xa1], [0xa4, 0xbb], [0xa4, 0xa2], [0xa4, 0xa1]]; // ㄱ ㅋ ㄲ ㄱ

        for (press, expected) in expected.iter().enumerate() {
            let output = input.press(b'4' as i8, 2);

            assert!(output.handled);
            assert_eq!(output.output0_len, 0, "press {press} finished a character it should have cycled");
            assert_eq!(&output.output1[..output.output1_len], expected, "press {press}");
        }
    }

    /// The ring is per key: 5 carries ㄴ and ㄹ, and 0 carries ㅇ and ㅁ.
    #[test]
    fn each_key_steps_through_its_own_ring() {
        for (key, expected) in [(b'5', [[0xa4, 0xa4], [0xa4, 0xa9]]), (b'0', [[0xa4, 0xb7], [0xa4, 0xb1]])] {
            let mut input = InputMethod::new();
            input.set_current_mode(3);

            for expected in expected.iter() {
                let output = input.press(key as i8, 2);
                assert_eq!(&output.output1[..output.output1_len], expected, "key {}", key as char);
            }
        }
    }

    /// The dot and ㅡ around ㅣ are what the vowels are made of, and which side
    /// the dot goes on is which vowel it is.
    #[test]
    fn the_strokes_build_the_vowel_they_are_written_in() {
        // ㄱ, then: ㅣ· is ㅏ, ·ㅣ is ㅓ, ·ㅡ is ㅗ, ㅡ· is ㅜ, ㅣ·· is ㅑ.
        for (strokes, expected) in [
            ("12", [0xb0, 0xa1]),  // 가
            ("21", [0xb0, 0xc5]),  // 거
            ("23", [0xb0, 0xed]),  // 고
            ("32", [0xb1, 0xb8]),  // 구
            ("122", [0xb0, 0xbc]), // 갸
        ] {
            let mut input = InputMethod::new();
            input.set_current_mode(3);
            input.press(b'4' as i8, 2);

            let mut last = InputMethodOutput::default();
            for key in strokes.bytes() {
                last = input.press(key as i8, 2);
            }

            assert_eq!(&last.output1[..last.output1_len], &expected, "strokes {strokes}");
        }
    }

    /// A whole word, the way it is typed: ㅅㅅ for ㅎ, ㅣ· for ㅏ, ㄴ under it,
    /// then ㄱ ㅡ and ㄴ stepped once more to ㄹ.
    #[test]
    fn a_word_types_the_way_it_is_spelled() {
        let mut input = InputMethod::new();
        input.set_current_mode(3);

        let mut typed = Vec::new();
        for key in b"881254355" {
            let output = input.press(*key as i8, 2);
            typed.extend_from_slice(&output.output0[..output.output0_len]);
        }

        let flush = input.press(-99, 2);
        typed.extend_from_slice(&flush.output0[..flush.output0_len]);

        assert_eq!(typed, [0xc7, 0xd1, 0xb1, 0xdb]); // 한글
    }

    /// The ring has no clock on it. 천지인 steps the same key however long the
    /// gap, so a slow second press on 4 is still ㅋ and not a second ㄱ - the
    /// pad says ㅋ and it has to mean it.
    #[test]
    fn the_ring_steps_however_long_the_gap() {
        use crate::time::Instant;

        let mut input = InputMethod::new();
        input.set_current_mode(3);

        let first = input.handle_input(b'4' as i8, 2, Instant::from_epoch_millis(0));
        assert_eq!(&first.output1[..first.output1_len], &[0xa4, 0xa1]); // ㄱ

        // A minute later, far past anything a multi-tap window would allow.
        let second = input.handle_input(b'4' as i8, 2, Instant::from_epoch_millis(60_000));
        assert_eq!(second.output0_len, 0, "the gap finished the character");
        assert_eq!(&second.output1[..second.output1_len], &[0xa4, 0xbb]); // ㅋ
    }

    /// Which is why the same consonant twice needs a press that finishes the
    /// character in between - the direction key the handler sends in as the
    /// flush sentinel. Without it the second press just steps the ring.
    #[test]
    fn the_same_consonant_twice_is_separated_by_a_flush() {
        let mut input = InputMethod::new();
        input.set_current_mode(3);

        let first = input.press(b'4' as i8, 2);
        assert_eq!(&first.output1[..first.output1_len], &[0xa4, 0xa1]); // ㄱ

        let flush = input.press(-99, 2);
        assert_eq!(&flush.output0[..flush.output0_len], &[0xa4, 0xa1]); // ㄱ finished

        let second = input.press(b'4' as i8, 2);
        assert_eq!(second.output0_len, 0);
        assert_eq!(&second.output1[..second.output1_len], &[0xa4, 0xa1]); // ㄱ again, not ㅋ
    }

    /// # is the space bar. It finishes the syllable in progress and puts a
    /// space behind it, both in the one commit.
    #[test]
    fn the_hash_key_is_the_space_bar() {
        let mut input = InputMethod::new();
        input.set_current_mode(3);

        input.press(b'4' as i8, 2); // ㄱ
        input.press(b'1' as i8, 2); // 기
        input.press(b'2' as i8, 2); // 가

        let space = input.press(b'#' as i8, 2);
        assert!(space.handled);
        assert_eq!(&space.output0[..space.output0_len], &[0xb0, 0xa1, b' ']); // "가 "
        assert_eq!(space.output1_len, 0);
    }

    /// Pressed with nothing in progress it is just a space.
    #[test]
    fn the_space_bar_on_its_own_is_a_space() {
        let mut input = InputMethod::new();
        input.set_current_mode(3);

        let space = input.press(b'#' as i8, 2);
        assert!(space.handled);
        assert_eq!(&space.output0[..space.output0_len], b" ");
    }

    /// And because it finishes the syllable, it is the other way to type the
    /// same consonant twice - the one written on the pad.
    #[test]
    fn the_space_bar_separates_the_same_consonant() {
        let mut input = InputMethod::new();
        input.set_current_mode(3);

        input.press(b'4' as i8, 2);
        let space = input.press(b'#' as i8, 2);
        assert_eq!(&space.output0[..space.output0_len], &[0xa4, 0xa1, b' ']); // "ㄱ "

        let again = input.press(b'4' as i8, 2);
        assert_eq!(again.output0_len, 0);
        assert_eq!(&again.output1[..again.output1_len], &[0xa4, 0xa1]); // ㄱ, not ㅋ
    }

    #[test]
    fn korean_mode_flushes_composition_with_false_result() {
        let mut input = InputMethod::new();
        input.set_current_mode(3);

        input.press(b'4' as i8, 2);
        input.press(b'1' as i8, 2);
        input.press(b'2' as i8, 2);

        let flush = input.press(-99, 2);
        assert!(!flush.handled);
        assert_eq!(&flush.output0[..flush.output0_len], &[0xb0, 0xa1]); // 가
        assert_eq!(flush.output1_len, 0);
    }

    #[test]
    fn korean_clear_removes_single_live_unit() {
        let mut input = InputMethod::new();
        input.set_current_mode(3);

        let giyeok = input.press(b'4' as i8, 2);
        assert_eq!(&giyeok.output1[..giyeok.output1_len], &[0xa4, 0xa1]);

        let clear = input.press(-16, 2);
        assert!(clear.handled);
        assert_eq!(clear.output0_len, 0);
        assert_eq!(clear.output1_len, 0);
    }

    #[test]
    fn korean_clear_restores_ga_from_gak() {
        let mut input = InputMethod::new();
        input.set_current_mode(3);

        input.press(b'4' as i8, 2);
        input.press(b'1' as i8, 2);
        input.press(b'2' as i8, 2);
        let gak = input.press(b'4' as i8, 2);
        assert_eq!(&gak.output1[..gak.output1_len], &[0xb0, 0xa2]);

        let clear = input.press(-16, 2);
        assert!(clear.handled);
        assert_eq!(clear.output0_len, 0);
        assert_eq!(&clear.output1[..clear.output1_len], &[0xb0, 0xa1]);
    }

    #[test]
    fn korean_clear_after_jong_split_restores_inherited_cho() {
        let mut input = InputMethod::new();
        input.set_current_mode(3);

        input.press(b'4' as i8, 2); // ㄱ
        input.press(b'1' as i8, 2); // 기
        input.press(b'2' as i8, 2); // 가
        input.press(b'4' as i8, 2); // 각

        let split = input.press(b'1' as i8, 2);
        assert_eq!(&split.output0[..split.output0_len], &[0xb0, 0xa1]); // 가
        assert_eq!(&split.output1[..split.output1_len], &[0xb1, 0xe2]); // 기

        let clear = input.press(-16, 2);
        assert!(clear.handled);
        assert_eq!(clear.output0_len, 0);
        assert_eq!(&clear.output1[..clear.output1_len], &[0xa4, 0xa1]); // ㄱ
    }

    #[test]
    fn korean_clear_does_not_cross_normal_commit_boundary() {
        let mut input = InputMethod::new();
        input.set_current_mode(3);

        input.press(b'4' as i8, 2);
        let next = input.press(b'5' as i8, 2);
        assert_ne!(next.output0_len, 0);

        let clear = input.press(-16, 2);
        assert!(clear.handled);
        assert_eq!(clear.output0_len, 0);
        assert_eq!(clear.output1_len, 0);
    }
}

#[cfg(test)]
mod korean_scan_tests {
    use super::InputMethod;

    #[test]
    fn korean_characters_encode_as_euc_kr() {
        assert_eq!(InputMethod::encode_korean_char('가'), Some(([0xb0, 0xa1], 2)));
        assert_eq!(InputMethod::encode_korean_char('나'), Some(([0xb3, 0xaa], 2)));
        assert_eq!(InputMethod::encode_korean_char('A'), Some(([b'A', 0], 1)));
    }

    #[test]
    fn korean_internal_codes_compose_unicode_syllables() {
        assert_eq!(InputMethod::compose_korean_syllable(2, 3, 1), Some('가'));
        assert_eq!(InputMethod::compose_korean_syllable(4, 3, 1), Some('나'));
        assert_eq!(InputMethod::compose_korean_syllable(5, 3, 1), Some('다'));
        assert_eq!(InputMethod::compose_korean_syllable(2, 3, 2), Some('각'));
        assert_eq!(InputMethod::compose_korean_syllable(13, 29, 1), Some('이'));
        assert_eq!(InputMethod::compose_korean_syllable(20, 3, 4), Some('핛'));

        assert_eq!(InputMethod::compose_korean_syllable(1, 3, 1), None);
        assert_eq!(InputMethod::compose_korean_syllable(2, 2, 1), None);
        assert_eq!(InputMethod::compose_korean_syllable(2, 3, 18), None);
    }

    #[test]
    fn korean_jong_to_cho_matches_native_conversion() {
        let expected = [
            (2, 2),
            (3, 3),
            (5, 4),
            (8, 5),
            (9, 7),
            (17, 8),
            (19, 9),
            (21, 11),
            (22, 12),
            (23, 13),
            (24, 14),
            (25, 16),
            (26, 17),
            (27, 18),
            (28, 19),
            (29, 20),
        ];

        for (jong, cho) in expected {
            assert_eq!(InputMethod::korean_jong_to_cho(jong), Some(cho));
        }

        for jong in [4, 6, 7, 10, 11, 12, 13, 14, 15, 16, 20] {
            assert_eq!(InputMethod::korean_jong_to_cho(jong), None);
        }
    }

    #[test]
    fn korean_compound_jong_matches_native_table() {
        let expected = [
            ((2, 21), 4),
            ((5, 24), 6),
            ((5, 29), 7),
            ((9, 2), 10),
            ((9, 17), 11),
            ((9, 19), 12),
            ((9, 21), 13),
            ((9, 27), 14),
            ((9, 28), 15),
            ((9, 29), 16),
            ((19, 21), 20),
        ];

        for ((first, second), combined) in expected {
            assert_eq!(InputMethod::combine_korean_jong(first, second), Some(combined));
        }

        assert_eq!(InputMethod::combine_korean_jong(2, 2), None);
        assert_eq!(InputMethod::combine_korean_jong(5, 21), None);
        assert_eq!(InputMethod::combine_korean_jong(17, 21), None);
    }

    #[test]
    fn korean_consonant_scans_match_native_jong_codes() {
        let expected = [
            (2, 2),
            (3, 3),
            (4, 5),
            (5, 8),
            (7, 9),
            (8, 17),
            (9, 19),
            (11, 21),
            (12, 22),
            (13, 23),
            (14, 24),
            (16, 25),
            (17, 26),
            (18, 27),
            (19, 28),
            (20, 29),
        ];

        for (scan, jong) in expected {
            assert_eq!(InputMethod::korean_scan_to_jong(scan), Some(jong));
        }

        assert_eq!(InputMethod::korean_scan_to_jong(6), None);
        assert_eq!(InputMethod::korean_scan_to_jong(10), None);
        assert_eq!(InputMethod::korean_scan_to_jong(15), None);
    }

    /// The spelling table is the vowel chart: every vowel 천지인 writes is in
    /// it exactly once, and no two spell the same way.
    #[test]
    fn every_vowel_is_spelled_once() {
        use alloc::collections::BTreeSet;

        let mut jungs = BTreeSet::new();
        let mut spellings = BTreeSet::new();

        for (spelling, jung) in super::KOREAN_VOWELS {
            assert!(jungs.insert(*jung), "{jung} is spelled twice");
            assert!(spellings.insert(*spelling), "a spelling is used twice");
            assert!(InputMethod::korean_jung_index(*jung).is_some(), "{jung} is not a jung");
            assert!(!spelling.is_empty() && spelling.len() <= super::KOREAN_STROKES);
        }

        assert_eq!(jungs.len(), 21, "Korean has twenty-one vowels");
    }

    /// A run of strokes is read left to right: it spells a vowel, or it is on
    /// the way to one, or nothing begins that way and it has to start over.
    #[test]
    fn strokes_spell_vowels_or_the_way_to_one() {
        use super::{STROKE_DOT, STROKE_EU, STROKE_I};

        assert_eq!(InputMethod::korean_vowel_for_strokes(&[STROKE_I]), Some(Some(29))); // ㅣ
        assert_eq!(InputMethod::korean_vowel_for_strokes(&[STROKE_I, STROKE_DOT]), Some(Some(3))); // ㅏ
        assert_eq!(InputMethod::korean_vowel_for_strokes(&[STROKE_DOT, STROKE_I]), Some(Some(7))); // ㅓ
        assert_eq!(InputMethod::korean_vowel_for_strokes(&[STROKE_DOT, STROKE_EU]), Some(Some(13))); // ㅗ
        assert_eq!(InputMethod::korean_vowel_for_strokes(&[STROKE_EU, STROKE_DOT]), Some(Some(20))); // ㅜ

        // The dot alone, and the two dots before ㅕ and ㅛ, are on the way to a
        // vowel without being one.
        assert_eq!(InputMethod::korean_vowel_for_strokes(&[STROKE_DOT]), Some(None));
        assert_eq!(InputMethod::korean_vowel_for_strokes(&[STROKE_DOT, STROKE_DOT]), Some(None));

        // Nothing begins with three dots, or with ㅣ then ㅡ.
        assert_eq!(InputMethod::korean_vowel_for_strokes(&[STROKE_DOT, STROKE_DOT, STROKE_DOT]), None);
        assert_eq!(InputMethod::korean_vowel_for_strokes(&[STROKE_I, STROKE_EU]), None);
    }

    /// The keys carry what the pad is engraved with: the strokes on 1-3 and the
    /// jamo pairs and triples on 4-0.
    #[test]
    fn the_number_keys_carry_what_is_engraved_on_them() {
        use super::{STROKE_DOT, STROKE_EU, STROKE_I};

        assert_eq!(InputMethod::korean_stroke(b'1' as i8), Some(STROKE_I));
        assert_eq!(InputMethod::korean_stroke(b'2' as i8), Some(STROKE_DOT));
        assert_eq!(InputMethod::korean_stroke(b'3' as i8), Some(STROKE_EU));
        assert_eq!(InputMethod::korean_stroke(b'4' as i8), None);

        assert_eq!(InputMethod::korean_consonant_ring(b'4' as i8), Some(&[2u8, 17, 3][..])); // ㄱㅋㄲ
        assert_eq!(InputMethod::korean_consonant_ring(b'5' as i8), Some(&[4u8, 7][..])); // ㄴㄹ
        assert_eq!(InputMethod::korean_consonant_ring(b'6' as i8), Some(&[5u8, 18, 6][..])); // ㄷㅌㄸ
        assert_eq!(InputMethod::korean_consonant_ring(b'7' as i8), Some(&[9u8, 19, 10][..])); // ㅂㅍㅃ
        assert_eq!(InputMethod::korean_consonant_ring(b'8' as i8), Some(&[11u8, 20, 12][..])); // ㅅㅎㅆ
        assert_eq!(InputMethod::korean_consonant_ring(b'9' as i8), Some(&[14u8, 16, 15][..])); // ㅈㅊㅉ
        assert_eq!(InputMethod::korean_consonant_ring(b'0' as i8), Some(&[13u8, 8][..])); // ㅇㅁ

        assert_eq!(InputMethod::korean_consonant_ring(b'1' as i8), None);
        assert_eq!(InputMethod::korean_consonant_ring(b'*' as i8), None);

        // Every jamo on a key is one the composer can place.
        for key in [b'4', b'5', b'6', b'7', b'8', b'9', b'0'] {
            for scan in InputMethod::korean_consonant_ring(key as i8).unwrap() {
                assert!(InputMethod::korean_cho_index(*scan).is_some(), "{scan} cannot start a syllable");
            }
        }
    }
}
