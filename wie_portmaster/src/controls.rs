//! Which handset key each of the handheld's buttons presses.
//!
//! The defaults are the Android app's pad defaults - the D-pad on the
//! directions, A on the confirm key, B on back, X and Y on * and #, each
//! shoulder on the soft key on its side - with START on confirm too, since a
//! handheld player reaches for it on a title screen. A handheld has no number
//! pad, so SELECT held turns the buttons into one: see [`DEFAULT_FILE`].
//!
//! `controls.txt` beside the port can move any of them; it is written out
//! with the defaults the first time, so there is a file to edit.

use std::path::Path;

/// A button on the handheld, as SDL's game controller names it by position.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Button {
    Up,
    Down,
    Left,
    Right,
    A,
    B,
    X,
    Y,
    L1,
    R1,
    L2,
    R2,
    L3,
    R3,
    Start,
    Select,
    Guide,
}

pub const BUTTON_COUNT: usize = 17;

const BUTTON_NAMES: [(Button, &str); BUTTON_COUNT] = [
    (Button::Up, "UP"),
    (Button::Down, "DOWN"),
    (Button::Left, "LEFT"),
    (Button::Right, "RIGHT"),
    (Button::A, "A"),
    (Button::B, "B"),
    (Button::X, "X"),
    (Button::Y, "Y"),
    (Button::L1, "L1"),
    (Button::R1, "R1"),
    (Button::L2, "L2"),
    (Button::R2, "R2"),
    (Button::L3, "L3"),
    (Button::R3, "R3"),
    (Button::Start, "START"),
    (Button::Select, "SELECT"),
    (Button::Guide, "GUIDE"),
];

impl Button {
    pub fn index(self) -> usize {
        self as usize
    }

    fn named(name: &str) -> Option<Button> {
        BUTTON_NAMES.iter().find(|(_, x)| x.eq_ignore_ascii_case(name)).map(|(button, _)| *button)
    }

    /// `SDL_GameControllerButton`.
    pub fn from_sdl(button: u8) -> Option<Button> {
        Some(match button {
            0 => Button::A,
            1 => Button::B,
            2 => Button::X,
            3 => Button::Y,
            4 => Button::Select,
            5 => Button::Guide,
            6 => Button::Start,
            7 => Button::L3,
            8 => Button::R3,
            9 => Button::L1,
            10 => Button::R1,
            11 => Button::Up,
            12 => Button::Down,
            13 => Button::Left,
            14 => Button::Right,
            _ => return None,
        })
    }
}

/// The handset keys, by the runner's key index (`wie_android::host::key`).
const KEY_NAMES: [(&str, i32); 22] = [
    ("UP", 0),
    ("DOWN", 1),
    ("LEFT", 2),
    ("RIGHT", 3),
    ("OK", 4),
    ("SOFT_LEFT", 5),
    ("SOFT_RIGHT", 6),
    ("CLEAR", 7),
    ("0", 8),
    ("1", 9),
    ("2", 10),
    ("3", 11),
    ("4", 12),
    ("5", 13),
    ("6", 14),
    ("7", 15),
    ("8", 16),
    ("9", 17),
    ("STAR", 18),
    ("HASH", 19),
    ("CALL", 20),
    ("HANGUP", 21),
];

fn key_name(key: i32) -> &'static str {
    KEY_NAMES.iter().find(|(_, index)| *index == key).map_or("NONE", |(name, _)| name)
}

/// What the settings screens call a handset key.
pub fn key_label(key: Option<i32>) -> &'static str {
    match key {
        None => "-",
        Some(0) => "▲",
        Some(1) => "▼",
        Some(2) => "◀",
        Some(3) => "▶",
        Some(4) => "확인",
        Some(5) => "좌소프트",
        Some(6) => "우소프트",
        Some(7) => "취소",
        Some(18) => "*",
        Some(19) => "#",
        Some(20) => "통화",
        Some(21) => "종료",
        Some(key @ 8..=17) => ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"][(key - 8) as usize],
        Some(_) => "?",
    }
}

/// The buttons the settings table lists, and what it calls them. SELECT is
/// the layer key and GUIDE opens the menu, so neither is mapped there.
pub const TABLE_BUTTONS: [(Button, &str); 15] = [
    (Button::Up, "D▲"),
    (Button::Down, "D▼"),
    (Button::Left, "D◀"),
    (Button::Right, "D▶"),
    (Button::A, "A"),
    (Button::B, "B"),
    (Button::X, "X"),
    (Button::Y, "Y"),
    (Button::L1, "L1"),
    (Button::R1, "R1"),
    (Button::L2, "L2"),
    (Button::R2, "R2"),
    (Button::L3, "L3"),
    (Button::R3, "R3"),
    (Button::Start, "START"),
];

pub const DEFAULT_FILE: &str = "\
# MiniMobile 버튼 설정
#
# 게임 목록에서 Y, 게임 중에는 MENU 버튼으로 여는 설정 화면에서 바꿀 수
# 있고, 이 파일을 직접 고쳐도 됩니다.
#
#   버튼 = 폰 키
#   SELECT+버튼 = 폰 키      (SELECT를 누른 채로 누를 때)
#
# 버튼: UP DOWN LEFT RIGHT A B X Y L1 R1 L2 R2 L3 R3 START
# 폰 키: UP DOWN LEFT RIGHT OK CLEAR SOFT_LEFT SOFT_RIGHT
#        0 1 2 3 4 5 6 7 8 9 STAR HASH CALL HANGUP NONE
#
# SELECT+START는 게임을 끝내고 목록으로 돌아갑니다 (바꿀 수 없음).

UP = UP
DOWN = DOWN
LEFT = LEFT
RIGHT = RIGHT
A = OK
B = CLEAR
X = STAR
Y = HASH
L1 = SOFT_LEFT
R1 = SOFT_RIGHT
L2 = SOFT_LEFT
R2 = SOFT_RIGHT
START = OK

# SELECT를 누른 채로: 숫자 키
SELECT+UP = 2
SELECT+DOWN = 8
SELECT+LEFT = 4
SELECT+RIGHT = 6
SELECT+A = 5
SELECT+B = 0
SELECT+X = 1
SELECT+Y = 3
SELECT+L1 = 7
SELECT+R1 = 9
SELECT+L2 = STAR
SELECT+R2 = HASH
";

#[derive(Clone, PartialEq, Eq)]
pub struct Controls {
    plain: [Option<i32>; BUTTON_COUNT],
    with_select: [Option<i32>; BUTTON_COUNT],
}

impl Controls {
    /// The mapping in `path`, written there with the defaults first if there
    /// is no file yet.
    pub fn load(path: &Path) -> Controls {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(_) => {
                if let Err(error) = std::fs::write(path, DEFAULT_FILE) {
                    eprintln!("{}에 기본 버튼 설정을 쓸 수 없습니다: {error}", path.display());
                }
                DEFAULT_FILE.to_owned()
            }
        };
        Controls::parse(&text)
    }

    /// The defaults, with each line of `text` moving one button.
    pub fn parse(text: &str) -> Controls {
        let mut controls = Controls {
            plain: [None; BUTTON_COUNT],
            with_select: [None; BUTTON_COUNT],
        };
        controls.apply(DEFAULT_FILE);
        controls.apply(text);
        controls
    }

    fn apply(&mut self, text: &str) {
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let Some((button, key)) = line.split_once('=') else {
                eprintln!("버튼 설정을 읽을 수 없습니다: {line}");
                continue;
            };
            let (layer, button) = match button.trim().split_once('+') {
                Some((modifier, button)) if modifier.trim().eq_ignore_ascii_case("SELECT") => (&mut self.with_select, button.trim()),
                Some(_) => {
                    eprintln!("SELECT+ 외의 조합은 쓸 수 없습니다: {line}");
                    continue;
                }
                None => (&mut self.plain, button.trim()),
            };
            let Some(button) = Button::named(button) else {
                eprintln!("모르는 버튼입니다: {line}");
                continue;
            };
            let key = key.trim();
            if key.eq_ignore_ascii_case("NONE") {
                layer[button.index()] = None;
            } else if let Some((_, index)) = KEY_NAMES.iter().find(|(name, _)| name.eq_ignore_ascii_case(key)) {
                layer[button.index()] = Some(*index);
            } else {
                eprintln!("모르는 폰 키입니다: {line}");
            }
        }
    }

    /// The key `button` is set to press itself in one layer - SELECT held or
    /// not - without falling back to the plain layer.
    pub fn get(&self, button: Button, with_select: bool) -> Option<i32> {
        if with_select {
            self.with_select[button.index()]
        } else {
            self.plain[button.index()]
        }
    }

    pub fn set(&mut self, button: Button, with_select: bool, key: Option<i32>) {
        if with_select {
            self.with_select[button.index()] = key;
        } else {
            self.plain[button.index()] = key;
        }
    }

    /// The mapping as a controls file: the default file's notes, then every
    /// button in both layers, so the file says all of it whatever the
    /// defaults become.
    pub fn to_text(&self) -> String {
        let mut text: String = DEFAULT_FILE
            .lines()
            .take_while(|line| line.is_empty() || line.starts_with('#'))
            .map(|line| format!("{line}\n"))
            .collect();
        for (prefix, layer) in [("", &self.plain), ("SELECT+", &self.with_select)] {
            text.push('\n');
            for (button, name) in BUTTON_NAMES {
                if button == Button::Select || button == Button::Guide {
                    continue;
                }
                let key = layer[button.index()].map_or("NONE", key_name);
                text.push_str(&format!("{prefix}{name} = {key}\n"));
            }
        }
        text
    }

    /// The key `button` presses, with SELECT held or not. A button with
    /// nothing under SELECT keeps its own key there.
    pub fn key(&self, button: Button, select_held: bool) -> Option<i32> {
        let index = button.index();
        if select_held && button != Button::Select {
            self.with_select[index].or(self.plain[index])
        } else {
            self.plain[index]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_the_android_pad() {
        let controls = Controls::parse("");
        assert_eq!(controls.key(Button::A, false), Some(4));
        assert_eq!(controls.key(Button::B, false), Some(7));
        assert_eq!(controls.key(Button::X, false), Some(18));
        assert_eq!(controls.key(Button::L2, false), Some(5));
        assert_eq!(controls.key(Button::Select, false), None);
        // SELECT held: the number layer, and the D-pad's own keys where it
        // names nothing.
        assert_eq!(controls.key(Button::A, true), Some(13));
        assert_eq!(controls.key(Button::Up, true), Some(10));
        assert_eq!(controls.key(Button::Start, true), Some(4));
    }

    #[test]
    fn the_file_written_reads_back_the_same() {
        let mut controls = Controls::parse("");
        controls.set(Button::A, false, Some(7));
        controls.set(Button::X, true, None);
        controls.set(Button::L3, false, Some(13));
        assert!(Controls::parse(&controls.to_text()) == controls);
        assert!(Controls::parse(&Controls::parse("").to_text()) == Controls::parse(""));
    }

    #[test]
    fn a_file_moves_and_clears_buttons() {
        let controls = Controls::parse("A = CLEAR\nB = ok  # swapped\nSELECT+A = NONE\nY=none\nnonsense\n");
        assert_eq!(controls.key(Button::A, false), Some(7));
        assert_eq!(controls.key(Button::B, false), Some(4));
        assert_eq!(controls.key(Button::Y, false), None);
        assert_eq!(controls.key(Button::A, true), Some(7));
    }
}
