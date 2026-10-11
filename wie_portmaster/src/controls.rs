//! Which handset key each of the handheld's buttons presses.
//!
//! The defaults are the Android app's pad defaults - the D-pad on the
//! directions, A on the confirm key, B on back, X and Y on * and #, each
//! shoulder on the soft key on its side - with START on confirm too, since a
//! handheld player reaches for it on a title screen. A handheld has no number
//! pad, so SELECT held turns the buttons into one: see [`DEFAULT_FILE`].
//!
//! A keyboard - the Windows build's, or one plugged into a handheld - has a
//! mapping of its own: up to two keys for each handset key, the arrows,
//! Enter, the digits and the number pad by default.
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

/// The handset keys the keyboard table lists, in its order.
pub const TABLE_KEYS: [i32; 22] = [0, 1, 2, 3, 4, 7, 5, 6, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 8, 19, 20, 21];

/// How many keyboard keys one handset key can have.
pub const KEYS_PER_KEY: usize = 2;

/// The keyboard keys a mapping can name: SDL's scancode, the name a controls
/// file gives it, and what the settings screens call it.
const SCANCODES: [(i32, &str, &str); 85] = [
    (4, "A", "A"),
    (5, "B", "B"),
    (6, "C", "C"),
    (7, "D", "D"),
    (8, "E", "E"),
    (9, "F", "F"),
    (10, "G", "G"),
    (11, "H", "H"),
    (12, "I", "I"),
    (13, "J", "J"),
    (14, "K", "K"),
    (15, "L", "L"),
    (16, "M", "M"),
    (17, "N", "N"),
    (18, "O", "O"),
    (19, "P", "P"),
    (20, "Q", "Q"),
    (21, "R", "R"),
    (22, "S", "S"),
    (23, "T", "T"),
    (24, "U", "U"),
    (25, "V", "V"),
    (26, "W", "W"),
    (27, "X", "X"),
    (28, "Y", "Y"),
    (29, "Z", "Z"),
    (30, "1", "1"),
    (31, "2", "2"),
    (32, "3", "3"),
    (33, "4", "4"),
    (34, "5", "5"),
    (35, "6", "6"),
    (36, "7", "7"),
    (37, "8", "8"),
    (38, "9", "9"),
    (39, "0", "0"),
    (40, "RETURN", "Enter"),
    (42, "BACKSPACE", "Backspace"),
    (43, "TAB", "Tab"),
    (44, "SPACE", "Space"),
    (45, "MINUS", "-"),
    (46, "EQUALS", "="),
    (47, "LBRACKET", "["),
    (48, "RBRACKET", "]"),
    (49, "BACKSLASH", "\\"),
    (51, "SEMICOLON", ";"),
    (52, "APOSTROPHE", "'"),
    (53, "GRAVE", "`"),
    (54, "COMMA", ","),
    (55, "PERIOD", "."),
    (56, "SLASH", "/"),
    (58, "F1", "F1"),
    (59, "F2", "F2"),
    (60, "F3", "F3"),
    (61, "F4", "F4"),
    (64, "F7", "F7"),
    (65, "F8", "F8"),
    (66, "F9", "F9"),
    (67, "F10", "F10"),
    (69, "F12", "F12"),
    (73, "INSERT", "Insert"),
    (74, "HOME", "Home"),
    (75, "PAGEUP", "PageUp"),
    (77, "END", "End"),
    (78, "PAGEDOWN", "PageDown"),
    (79, "RIGHT", "방향키 ▶"),
    (80, "LEFT", "방향키 ◀"),
    (81, "DOWN", "방향키 ▼"),
    (82, "UP", "방향키 ▲"),
    (84, "KP_DIVIDE", "키패드 /"),
    (85, "KP_MULTIPLY", "키패드 *"),
    (86, "KP_MINUS", "키패드 -"),
    (87, "KP_PLUS", "키패드 +"),
    (88, "KP_ENTER", "키패드 Enter"),
    (89, "KP_1", "키패드 1"),
    (90, "KP_2", "키패드 2"),
    (91, "KP_3", "키패드 3"),
    (92, "KP_4", "키패드 4"),
    (93, "KP_5", "키패드 5"),
    (94, "KP_6", "키패드 6"),
    (95, "KP_7", "키패드 7"),
    (96, "KP_8", "키패드 8"),
    (97, "KP_9", "키패드 9"),
    (98, "KP_0", "키패드 0"),
    (99, "KP_PERIOD", "키패드 ."),
];

/// Keys the port keeps for itself: Esc opens the menu, F5 and F6 slow the
/// game down and speed it up, F11 the full screen, Delete clears a cell while
/// the table is open. On the list Tab opens the search, Backspace takes a
/// letter off it, and F2 opens the games folder.
pub const ESCAPE: i32 = 41;
pub const BACKSPACE: i32 = 42;
pub const TAB: i32 = 43;
pub const F2: i32 = 59;
pub const F5: i32 = 62;
pub const F6: i32 = 63;
pub const F11: i32 = 68;
pub const DELETE: i32 = 76;

/// Modifier keys, which the table above leaves out only to keep it in
/// scancode order.
const MODIFIERS: [(i32, &str, &str); 6] = [
    (224, "LCTRL", "왼쪽 Ctrl"),
    (225, "LSHIFT", "왼쪽 Shift"),
    (226, "LALT", "왼쪽 Alt"),
    (228, "RCTRL", "오른쪽 Ctrl"),
    (229, "RSHIFT", "오른쪽 Shift"),
    (230, "RALT", "오른쪽 Alt"),
];

fn scancodes() -> impl Iterator<Item = &'static (i32, &'static str, &'static str)> {
    SCANCODES.iter().chain(MODIFIERS.iter())
}

/// Whether a keyboard key can be put in the mapping at all.
pub fn mappable(scancode: i32) -> bool {
    scancodes().any(|(code, _, _)| *code == scancode)
}

/// What the settings screens call a keyboard key.
pub fn scancode_label(scancode: Option<i32>) -> String {
    match scancode {
        None => "-".to_owned(),
        Some(code) => scancodes()
            .find(|(x, _, _)| *x == code)
            .map_or_else(|| format!("키 {code}"), |(_, _, label)| (*label).to_owned()),
    }
}

fn scancode_named(name: &str) -> Option<i32> {
    scancodes().find(|(_, x, _)| x.eq_ignore_ascii_case(name)).map(|(code, _, _)| *code)
}

fn scancode_name(scancode: i32) -> Option<&'static str> {
    scancodes().find(|(code, _, _)| *code == scancode).map(|(_, name, _)| *name)
}

pub const DEFAULT_FILE: &str = "\
# MiniMobile 버튼 설정
#
# 설정 화면(게임 목록에서 Y나 Esc, 게임 중에는 MENU나 Esc)에서 바꿀 수
# 있고, 이 파일을 직접 고쳐도 됩니다.
#
#   버튼 = 폰 키
#   SELECT+버튼 = 폰 키      (SELECT를 누른 채로 누를 때)
#   KEYBOARD:폰 키 = 키보드 키, 키보드 키   (두 개까지)
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

# 키보드
KEYBOARD:UP = UP
KEYBOARD:DOWN = DOWN
KEYBOARD:LEFT = LEFT
KEYBOARD:RIGHT = RIGHT
KEYBOARD:OK = RETURN, SPACE
KEYBOARD:CLEAR = BACKSPACE
KEYBOARD:SOFT_LEFT = LSHIFT, LBRACKET
KEYBOARD:SOFT_RIGHT = RSHIFT, RBRACKET
KEYBOARD:1 = 1, KP_1
KEYBOARD:2 = 2, KP_2
KEYBOARD:3 = 3, KP_3
KEYBOARD:4 = 4, KP_4
KEYBOARD:5 = 5, KP_5
KEYBOARD:6 = 6, KP_6
KEYBOARD:7 = 7, KP_7
KEYBOARD:8 = 8, KP_8
KEYBOARD:9 = 9, KP_9
KEYBOARD:STAR = MINUS, KP_MULTIPLY
KEYBOARD:0 = 0, KP_0
KEYBOARD:HASH = EQUALS, KP_DIVIDE
KEYBOARD:CALL = F1
KEYBOARD:HANGUP = F2
";

#[derive(Clone, PartialEq, Eq)]
pub struct Controls {
    plain: [Option<i32>; BUTTON_COUNT],
    with_select: [Option<i32>; BUTTON_COUNT],
    /// The keyboard keys for each handset key, by key index.
    keyboard: [[Option<i32>; KEYS_PER_KEY]; KEY_NAMES.len()],
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
            keyboard: [[None; KEYS_PER_KEY]; KEY_NAMES.len()],
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
            if let Some(handset) = button
                .trim()
                .strip_prefix("KEYBOARD:")
                .or_else(|| button.trim().strip_prefix("keyboard:"))
            {
                let Some((_, index)) = KEY_NAMES.iter().find(|(name, _)| name.eq_ignore_ascii_case(handset.trim())) else {
                    eprintln!("모르는 폰 키입니다: {line}");
                    continue;
                };
                let mut keys = [None; KEYS_PER_KEY];
                let names = key.split(',').map(str::trim).filter(|x| !x.is_empty() && !x.eq_ignore_ascii_case("NONE"));
                for (slot, name) in names.enumerate() {
                    match scancode_named(name) {
                        Some(code) if slot < KEYS_PER_KEY => keys[slot] = Some(code),
                        Some(_) => eprintln!("폰 키 하나에 키보드 키는 {KEYS_PER_KEY}개까지입니다: {line}"),
                        None => eprintln!("모르는 키보드 키입니다: {name}"),
                    }
                }
                // A key named here leaves whatever other handset key had it.
                for code in keys.iter().flatten() {
                    for slots in self.keyboard.iter_mut() {
                        for slot in slots.iter_mut().filter(|x| **x == Some(*code)) {
                            *slot = None;
                        }
                    }
                }
                self.keyboard[*index as usize] = keys;
                continue;
            }
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
        text.push('\n');
        for key in TABLE_KEYS {
            let names: Vec<&str> = self.keyboard[key as usize]
                .iter()
                .flatten()
                .filter_map(|code| scancode_name(*code))
                .collect();
            let names = if names.is_empty() { "NONE".to_owned() } else { names.join(", ") };
            text.push_str(&format!("KEYBOARD:{} = {names}\n", key_name(key)));
        }
        text
    }

    /// The keyboard key in one slot of a handset key.
    pub fn keyboard(&self, handset: i32, slot: usize) -> Option<i32> {
        self.keyboard[handset as usize][slot]
    }

    /// Puts `scancode` in one slot of a handset key, taking it off any other
    /// it was on. The handset key it was taken from, if one.
    pub fn set_keyboard(&mut self, handset: i32, slot: usize, scancode: Option<i32>) -> Option<i32> {
        let mut moved_from = None;
        if let Some(code) = scancode {
            for (index, slots) in self.keyboard.iter_mut().enumerate() {
                for (other, value) in slots.iter_mut().enumerate() {
                    if *value == Some(code) && (index as i32, other) != (handset, slot) {
                        *value = None;
                        if index as i32 != handset {
                            moved_from = Some(index as i32);
                        }
                    }
                }
            }
        }
        self.keyboard[handset as usize][slot] = scancode;
        moved_from
    }

    /// The handset key a keyboard key presses.
    pub fn keyboard_key(&self, scancode: i32) -> Option<i32> {
        self.keyboard
            .iter()
            .position(|slots| slots.contains(&Some(scancode)))
            .map(|index| index as i32)
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
    fn the_keyboard_defaults_and_file() {
        let controls = Controls::parse("");
        assert_eq!(controls.keyboard_key(40), Some(4)); // Enter: OK
        assert_eq!(controls.keyboard_key(44), Some(4)); // Space: OK
        assert_eq!(controls.keyboard_key(89), Some(9)); // keypad 1: 1
        assert_eq!(controls.keyboard_key(82), Some(0)); // up arrow: UP
        assert_eq!(controls.keyboard_key(41), None); // Esc: the menu's

        // A key named for one handset key leaves the one it was on.
        let controls = Controls::parse("KEYBOARD:5 = space, kp_5\nKEYBOARD:CLEAR = none");
        assert_eq!(controls.keyboard_key(44), Some(13));
        assert_eq!(controls.keyboard(4, 0), Some(40));
        assert_eq!(controls.keyboard(4, 1), None);
        assert_eq!(controls.keyboard_key(42), None);
        assert!(Controls::parse(&controls.to_text()) == controls);
    }

    #[test]
    fn setting_a_keyboard_key_moves_it() {
        let mut controls = Controls::parse("");
        // Space onto 5's second slot comes off OK.
        assert_eq!(controls.set_keyboard(13, 1, Some(44)), Some(4));
        assert_eq!(controls.keyboard_key(44), Some(13));
        assert_eq!(controls.keyboard(4, 1), None);
        // Onto its own other slot, it only moves over.
        assert_eq!(controls.set_keyboard(13, 0, Some(44)), None);
        assert_eq!(controls.keyboard(13, 1), None);
        assert_eq!(controls.keyboard(13, 0), Some(44));
        assert!(mappable(225) && !mappable(41) && !mappable(68) && !mappable(76));
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
