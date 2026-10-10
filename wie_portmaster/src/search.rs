//! Finding a game on the list by name: what a query matches, and the keys a
//! pad types it with.
//!
//! A query matches a run of the title's letters, spaces left out on both
//! sides and case ignored. A Hangul consonant in it stands for any syllable
//! that starts with it, so "ㅎㄱ" finds 학교가는길 and "ㅎ2007" 해적왕2007, as
//! Korean players type to find a title.

/// The initial consonants, in the order Unicode composes syllables by.
const INITIALS: [char; 19] = [
    'ㄱ', 'ㄲ', 'ㄴ', 'ㄷ', 'ㄸ', 'ㄹ', 'ㅁ', 'ㅂ', 'ㅃ', 'ㅅ', 'ㅆ', 'ㅇ', 'ㅈ', 'ㅉ', 'ㅊ', 'ㅋ', 'ㅌ', 'ㅍ', 'ㅎ',
];

/// The initial consonant `c` opens with, when it is a Hangul syllable.
fn initial(c: char) -> Option<char> {
    let code = c as u32;
    (0xAC00..=0xD7A3).contains(&code).then(|| INITIALS[((code - 0xAC00) / 588) as usize])
}

fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
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
    let fits = |wanted: char, (_, c): &(usize, char)| wanted == *c || (INITIALS.contains(&wanted) && initial(*c) == Some(wanted));
    (0..=letters.len().checked_sub(query.len())?)
        .find(|start| query.iter().zip(&letters[*start..]).all(|(wanted, letter)| fits(*wanted, letter)))
        .map(|start| letters[start..start + query.len()].iter().map(|(at, _)| *at).collect())
}

/// Which keys the pad's keyboard shows.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Mode {
    Initials,
    Letters,
    Digits,
}

impl Mode {
    fn rows(self) -> &'static [&'static [&'static str]] {
        match self {
            Mode::Initials => &[&["ㄱ", "ㄴ", "ㄷ", "ㄹ", "ㅁ", "ㅂ", "ㅅ"], &["ㅇ", "ㅈ", "ㅊ", "ㅋ", "ㅌ", "ㅍ", "ㅎ"]],
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
            Mode::Initials => "ㄱㄴㄷ",
            Mode::Letters => "ABC",
            Mode::Digits => "123",
        }
    }

    /// The two other modes, for the keys that switch to them.
    fn others(self) -> [Mode; 2] {
        match self {
            Mode::Initials => [Mode::Letters, Mode::Digits],
            Mode::Letters => [Mode::Initials, Mode::Digits],
            Mode::Digits => [Mode::Initials, Mode::Letters],
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

/// The keys of `mode`'s keyboard, `width` units across: seven a row, and a
/// row under them to switch, space, erase and finish.
pub(crate) fn keys(mode: Mode, width: f32) -> Vec<Key> {
    let mut keys = Vec::new();
    let rows = mode.rows();
    let key_width = (width - 6.0 * GAP) / 7.0;
    for (row, labels) in rows.iter().enumerate() {
        for (column, label) in labels.iter().enumerate() {
            keys.push(Key {
                label,
                action: Action::Type(label),
                x: column as f32 * (key_width + GAP),
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
    }

    #[test]
    fn the_keyboard_moves_between_rows_by_the_nearest_key() {
        let keys = keys(Mode::Initials, 280.0);
        let at = |label: &str| keys.iter().position(|key| key.label == label).unwrap();
        assert_eq!(step(&keys, at("ㄱ"), 1, 0), at("ㄴ"));
        assert_eq!(step(&keys, at("ㄱ"), 0, 1), at("ㅇ"));
        assert_eq!(step(&keys, at("ㅇ"), 0, 1), at("ABC"));
        assert_eq!(step(&keys, at("ㅎ"), 0, 1), at("완료"));
        assert_eq!(step(&keys, at("ㅅ"), 1, 0), at("ㅅ"));
        assert_eq!(height(Mode::Initials), 3.0 * KEY_HEIGHT + 2.0 * GAP);
    }
}
