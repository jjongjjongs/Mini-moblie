//! Finding a game on the list by name: what a query matches, and the keys a
//! pad types it with.
//!
//! A query matches a run of the title's letters, spaces left out on both
//! sides and case ignored. A Hangul consonant in it stands for any syllable
//! that starts with it, so "ㅎㄱ" finds 학교가는길 and "ㅎ2007" 해적왕2007, as
//! Korean players type to find a title; and a syllable with no final stands
//! for any that starts with it, so 하 finds 학교 on the way to typing it.
//!
//! The pad's Hangul keys put syllables together as a keyboard's input method
//! does (`compose`): ㅎ, ㅏ, ㄱ make 학, and a vowel after it takes the ㄱ on to
//! start the next syllable.

/// The initial consonants, in the order Unicode composes syllables by.
const INITIALS: [char; 19] = [
    'ㄱ', 'ㄲ', 'ㄴ', 'ㄷ', 'ㄸ', 'ㄹ', 'ㅁ', 'ㅂ', 'ㅃ', 'ㅅ', 'ㅆ', 'ㅇ', 'ㅈ', 'ㅉ', 'ㅊ', 'ㅋ', 'ㅌ', 'ㅍ', 'ㅎ',
];
/// The vowels, likewise.
const VOWELS: [char; 21] = [
    'ㅏ', 'ㅐ', 'ㅑ', 'ㅒ', 'ㅓ', 'ㅔ', 'ㅕ', 'ㅖ', 'ㅗ', 'ㅘ', 'ㅙ', 'ㅚ', 'ㅛ', 'ㅜ', 'ㅝ', 'ㅞ', 'ㅟ', 'ㅠ', 'ㅡ', 'ㅢ', 'ㅣ',
];
/// The finals, after none.
const FINALS: [char; 27] = [
    'ㄱ', 'ㄲ', 'ㄳ', 'ㄴ', 'ㄵ', 'ㄶ', 'ㄷ', 'ㄹ', 'ㄺ', 'ㄻ', 'ㄼ', 'ㄽ', 'ㄾ', 'ㄿ', 'ㅀ', 'ㅁ', 'ㅂ', 'ㅄ', 'ㅅ', 'ㅆ', 'ㅇ', 'ㅈ', 'ㅊ', 'ㅋ',
    'ㅌ', 'ㅍ', 'ㅎ',
];
/// Two vowels typed one after the other that make one.
const VOWEL_PAIRS: [(char, char, char); 7] = [
    ('ㅗ', 'ㅏ', 'ㅘ'),
    ('ㅗ', 'ㅐ', 'ㅙ'),
    ('ㅗ', 'ㅣ', 'ㅚ'),
    ('ㅜ', 'ㅓ', 'ㅝ'),
    ('ㅜ', 'ㅔ', 'ㅞ'),
    ('ㅜ', 'ㅣ', 'ㅟ'),
    ('ㅡ', 'ㅣ', 'ㅢ'),
];
/// Two finals typed one after the other that make one.
const FINAL_PAIRS: [(char, char, char); 11] = [
    ('ㄱ', 'ㅅ', 'ㄳ'),
    ('ㄴ', 'ㅈ', 'ㄵ'),
    ('ㄴ', 'ㅎ', 'ㄶ'),
    ('ㄹ', 'ㄱ', 'ㄺ'),
    ('ㄹ', 'ㅁ', 'ㄻ'),
    ('ㄹ', 'ㅂ', 'ㄼ'),
    ('ㄹ', 'ㅅ', 'ㄽ'),
    ('ㄹ', 'ㅌ', 'ㄾ'),
    ('ㄹ', 'ㅍ', 'ㄿ'),
    ('ㄹ', 'ㅎ', 'ㅀ'),
    ('ㅂ', 'ㅅ', 'ㅄ'),
];

/// A Hangul syllable's initial, vowel and final (0 for none, else the place
/// in `FINALS` plus one).
fn split(c: char) -> Option<(usize, usize, usize)> {
    let code = (c as u32).checked_sub(0xAC00)?;
    (code < 11172).then(|| ((code / 588) as usize, (code % 588 / 28) as usize, (code % 28) as usize))
}

fn join(initial: usize, vowel: usize, last: usize) -> char {
    char::from_u32(0xAC00 + (initial * 588 + vowel * 28 + last) as u32).unwrap_or('?')
}

fn place(table: &[char], c: char) -> Option<usize> {
    table.iter().position(|x| *x == c)
}

/// The initial consonant `c` opens with, when it is a Hangul syllable.
fn initial(c: char) -> Option<char> {
    split(c).map(|(initial, _, _)| INITIALS[initial])
}

fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Whether a letter of the query stands for a letter of the title.
fn fits(wanted: char, letter: char) -> bool {
    if wanted == letter {
        return true;
    }
    if INITIALS.contains(&wanted) {
        return initial(letter) == Some(wanted);
    }
    // A syllable with no final yet, as one is while it is being typed.
    match (split(wanted), split(letter)) {
        (Some((initial, vowel, 0)), Some((title_initial, title_vowel, _))) => initial == title_initial && vowel == title_vowel,
        _ => false,
    }
}

/// The characters of `title` that `query` matches, by their place in it;
/// `None` when it does not match. An empty query matches with nothing marked.
pub(crate) fn matches(title: &str, query: &str) -> Option<Vec<usize>> {
    let query: Vec<char> = query.chars().filter(|c| !c.is_whitespace()).map(fold).collect();
    if query.is_empty() {
        return Some(Vec::new());
    }
    let letters: Vec<(usize, char)> = title
        .chars()
        .enumerate()
        .filter(|(_, c)| !c.is_whitespace())
        .map(|(at, c)| (at, fold(c)))
        .collect();
    (0..=letters.len().checked_sub(query.len())?)
        .find(|start| query.iter().zip(&letters[*start..]).all(|(wanted, (_, letter))| fits(*wanted, *letter)))
        .map(|start| letters[start..start + query.len()].iter().map(|(at, _)| *at).collect())
}

/// Puts the Hangul letter `jamo` onto the end of `text` as an input method
/// does: a vowel onto a consonant makes a syllable, a consonant onto a
/// syllable its final, and a vowel after a final takes it on to start the
/// next syllable. Anything else is just added.
pub(crate) fn compose(text: &mut String, jamo: char) {
    let last = text.chars().last();
    if let Some(vowel) = place(&VOWELS, jamo) {
        match last {
            Some(c) if place(&INITIALS, c).is_some() => {
                text.pop();
                text.push(join(place(&INITIALS, c).unwrap(), vowel, 0));
                return;
            }
            Some(c) if split(c).is_some() => {
                let (first, middle, end) = split(c).unwrap();
                if end == 0 {
                    if let Some((_, _, pair)) = VOWEL_PAIRS.iter().find(|(a, b, _)| *a == VOWELS[middle] && *b == jamo) {
                        text.pop();
                        text.push(join(first, place(&VOWELS, *pair).unwrap(), 0));
                        return;
                    }
                } else {
                    // The final - or the second half of a double one - moves
                    // on to start the next syllable.
                    let final_letter = FINALS[end - 1];
                    let (stays, moves) = match FINAL_PAIRS.iter().find(|(_, _, pair)| *pair == final_letter) {
                        Some((a, b, _)) => (place(&FINALS, *a).unwrap() + 1, *b),
                        None => (0, final_letter),
                    };
                    if let Some(next) = place(&INITIALS, moves) {
                        text.pop();
                        text.push(join(first, middle, stays));
                        text.push(join(next, vowel, 0));
                        return;
                    }
                }
            }
            _ => {}
        }
    } else if INITIALS.contains(&jamo)
        && let Some((first, middle, end)) = last.and_then(split)
    {
        let made = if end == 0 {
            place(&FINALS, jamo).map(|at| at + 1)
        } else {
            FINAL_PAIRS
                .iter()
                .find(|(a, b, _)| *a == FINALS[end - 1] && *b == jamo)
                .and_then(|(_, _, pair)| place(&FINALS, *pair).map(|at| at + 1))
        };
        if let Some(made) = made {
            text.pop();
            text.push(join(first, middle, made));
            return;
        }
    }
    text.push(jamo);
}

/// Takes the last letter typed off `text`: a syllable's final, then its
/// vowel, leaving its initial consonant; anything else whole.
pub(crate) fn erase(text: &mut String) {
    let Some(last) = text.pop() else {
        return;
    };
    let Some((first, middle, end)) = split(last) else {
        return;
    };
    if end != 0 {
        let final_letter = FINALS[end - 1];
        let stays = FINAL_PAIRS
            .iter()
            .find(|(_, _, pair)| *pair == final_letter)
            .map_or(0, |(a, _, _)| place(&FINALS, *a).unwrap() + 1);
        text.push(join(first, middle, stays));
    } else if let Some((a, _, _)) = VOWEL_PAIRS.iter().find(|(_, _, pair)| *pair == VOWELS[middle]) {
        text.push(join(first, place(&VOWELS, *a).unwrap(), 0));
    } else {
        text.push(INITIALS[first]);
    }
}

/// Which keys the pad's keyboard shows.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Mode {
    Hangul,
    Letters,
    Digits,
}

impl Mode {
    fn rows(self) -> &'static [&'static [&'static str]] {
        match self {
            // The consonants on the left, the vowels on the right.
            Mode::Hangul => &[
                &["ㄱ", "ㄴ", "ㄷ", "ㄹ", "ㅁ", "ㅂ", "ㅅ", "ㅏ", "ㅑ", "ㅓ", "ㅕ"],
                &["ㅇ", "ㅈ", "ㅊ", "ㅋ", "ㅌ", "ㅍ", "ㅎ", "ㅗ", "ㅛ", "ㅜ", "ㅠ"],
                &["ㄲ", "ㄸ", "ㅃ", "ㅆ", "ㅉ", "ㅒ", "ㅖ", "ㅡ", "ㅣ", "ㅐ", "ㅔ"],
            ],
            Mode::Letters => &[
                &["A", "B", "C", "D", "E", "F", "G"],
                &["H", "I", "J", "K", "L", "M", "N"],
                &["O", "P", "Q", "R", "S", "T", "U"],
                &["V", "W", "X", "Y", "Z", "-", "."],
            ],
            Mode::Digits => &[&["1", "2", "3", "4", "5", "6", "7"], &["8", "9", "0", "-", ".", "!", "&"]],
        }
    }

    fn label(self) -> &'static str {
        match self {
            Mode::Hangul => "한글",
            Mode::Letters => "ABC",
            Mode::Digits => "123",
        }
    }

    /// Where the rows split into two blocks, a little apart: the Hangul
    /// keys' consonants and vowels.
    fn split(self) -> Option<usize> {
        (self == Mode::Hangul).then_some(7)
    }

    /// The two other modes, for the keys that switch to them.
    fn others(self) -> [Mode; 2] {
        match self {
            Mode::Hangul => [Mode::Letters, Mode::Digits],
            Mode::Letters => [Mode::Hangul, Mode::Digits],
            Mode::Digits => [Mode::Hangul, Mode::Letters],
        }
    }
}

/// What a key of the pad's keyboard does.
#[derive(Clone, PartialEq, Debug)]
pub(crate) enum Action {
    Type(&'static str),
    Mode(Mode),
    Space,
    Erase,
    Done,
}

/// One key, placed in layout units from the keyboard's top left.
#[derive(Clone, Debug)]
pub(crate) struct Key {
    pub label: &'static str,
    pub action: Action,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

pub(crate) const KEY_HEIGHT: f32 = 20.0;
const GAP: f32 = 3.0;

/// The keys of `mode`'s keyboard, `width` units across, and a row under them
/// to switch, space, erase and finish.
pub(crate) fn keys(mode: Mode, width: f32) -> Vec<Key> {
    let mut keys = Vec::new();
    let rows = mode.rows();
    let columns = rows.iter().map(|row| row.len()).max().unwrap_or(1);
    let apart = if mode.split().is_some() { 2.0 * GAP } else { 0.0 };
    let key_width = (width - (columns - 1) as f32 * GAP - apart) / columns as f32;
    for (row, labels) in rows.iter().enumerate() {
        for (column, label) in labels.iter().enumerate() {
            let past_split = mode.split().is_some_and(|split| column >= split);
            keys.push(Key {
                label,
                action: Action::Type(label),
                x: column as f32 * (key_width + GAP) + if past_split { apart } else { 0.0 },
                y: row as f32 * (KEY_HEIGHT + GAP),
                width: key_width,
                height: KEY_HEIGHT,
            });
        }
    }
    let [first, second] = mode.others();
    let bottom: [(&'static str, Action, f32); 5] = [
        (first.label(), Action::Mode(first), 1.0),
        (second.label(), Action::Mode(second), 1.0),
        ("띄움", Action::Space, 2.0),
        ("← 지움", Action::Erase, 1.3),
        ("완료", Action::Done, 1.3),
    ];
    let shares: f32 = bottom.iter().map(|(_, _, share)| share).sum();
    let unit = (width - 4.0 * GAP) / shares;
    let y = rows.len() as f32 * (KEY_HEIGHT + GAP);
    let mut x = 0.0;
    for (label, action, share) in bottom {
        keys.push(Key {
            label,
            action,
            x,
            y,
            width: unit * share,
            height: KEY_HEIGHT,
        });
        x += unit * share + GAP;
    }
    keys
}

/// How tall `mode`'s keyboard is.
pub(crate) fn height(mode: Mode) -> f32 {
    (mode.rows().len() + 1) as f32 * (KEY_HEIGHT + GAP) - GAP
}

/// The key a direction leads to from `from`: the nearest past its edge that
/// way, distance across the direction counting double; `from` itself at an
/// edge.
pub(crate) fn step(keys: &[Key], from: usize, dx: i32, dy: i32) -> usize {
    let centre = |key: &Key| (key.x + key.width / 2.0, key.y + key.height / 2.0);
    let origin = &keys[from];
    let (fx, fy) = centre(origin);
    keys.iter()
        .enumerate()
        .filter(|(_, key)| match (dx, dy) {
            (0, -1) => key.y + key.height <= origin.y,
            (0, 1) => key.y >= origin.y + origin.height,
            (-1, 0) => key.x + key.width <= origin.x && (key.y - origin.y).abs() < 1.0,
            _ => key.x >= origin.x + origin.width && (key.y - origin.y).abs() < 1.0,
        })
        .map(|(index, key)| {
            let (cx, cy) = centre(key);
            let (along, across) = if dx == 0 {
                ((cy - fy).abs(), cx - fx)
            } else {
                ((cx - fx).abs(), cy - fy)
            };
            (index, along + 2.0 * across.abs())
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(from, |(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_matches_letters_and_initial_consonants() {
        assert_eq!(matches("해적왕2007", "2007"), Some(vec![3, 4, 5, 6]));
        assert_eq!(matches("학교가는길", "ㅎㄱ"), Some(vec![0, 1]));
        assert_eq!(matches("영웅서기 - 빙해의 검사", "빙해"), Some(vec![7, 8]));
        assert_eq!(matches("Let's Golf 2007", "golf"), Some(vec![6, 7, 8, 9]));
        assert_eq!(matches("해적왕2007", "ㅎ2"), None);
        assert_eq!(matches("해적왕2007", "ㅇ2007"), Some(vec![2, 3, 4, 5, 6]));
        assert_eq!(matches("템페스트", "ㅎ"), None);
        assert_eq!(matches("템페스트", " "), Some(vec![]));
        // A syllable still being typed matches the ones it starts.
        assert_eq!(matches("학교가는길", "하"), Some(vec![0]));
        assert_eq!(matches("학교가는길", "학교"), Some(vec![0, 1]));
        assert_eq!(matches("학교가는길", "학ㄱ"), Some(vec![0, 1]));
        assert_eq!(matches("학교가는길", "학구"), None);
    }

    fn typed(jamo: &str) -> String {
        let mut text = String::new();
        for c in jamo.chars() {
            compose(&mut text, c);
        }
        text
    }

    #[test]
    fn hangul_letters_make_syllables() {
        assert_eq!(typed("ㅎㅏㄱㄱㅛ"), "학교");
        assert_eq!(typed("ㅎㅏㄱㅛ"), "하교");
        assert_eq!(typed("ㅎㄱ"), "ㅎㄱ");
        assert_eq!(typed("ㄱㅗㅏ"), "과");
        assert_eq!(typed("ㄷㅏㄹㄱ"), "닭");
        assert_eq!(typed("ㄷㅏㄹㄱㅏ"), "달가");
        assert_eq!(typed("ㅇㅕㅇㅇㅜㅇ"), "영웅");
        assert_eq!(typed("ㅏ2"), "ㅏ2");
        let mut text = typed("ㄷㅏㄹㄱ");
        erase(&mut text);
        assert_eq!(text, "달");
        erase(&mut text);
        assert_eq!(text, "다");
        erase(&mut text);
        assert_eq!(text, "ㄷ");
        erase(&mut text);
        assert_eq!(text, "");
        let mut text = typed("ㄱㅗㅏ");
        erase(&mut text);
        assert_eq!(text, "고");
    }

    #[test]
    fn the_keyboard_moves_between_rows_by_the_nearest_key() {
        let keys = keys(Mode::Hangul, 300.0);
        let at = |label: &str| keys.iter().position(|key| key.label == label).unwrap();
        assert_eq!(step(&keys, at("ㄱ"), 1, 0), at("ㄴ"));
        assert_eq!(step(&keys, at("ㄱ"), 0, 1), at("ㅇ"));
        assert_eq!(step(&keys, at("ㅅ"), 1, 0), at("ㅏ"));
        assert_eq!(step(&keys, at("ㄲ"), 0, 1), at("ABC"));
        assert_eq!(step(&keys, at("ㅔ"), 0, 1), at("완료"));
        assert_eq!(step(&keys, at("ㅕ"), 1, 0), at("ㅕ"));
        assert_eq!(height(Mode::Hangul), 4.0 * KEY_HEIGHT + 3.0 * GAP);
    }
}
