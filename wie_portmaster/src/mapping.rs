//! The two layout screens, both the handset's keypad: one with the keyboard
//! keys on each of its keys, one with the pad buttons. A key picked asks for
//! the keyboard key or the pad button to put on it, pressed. Either can put
//! its half of the mapping back to the defaults.

use wie_android::host;
use wie_backend::canvas::{Color, TextAlignment};

use crate::{
    App, FRAME, Input,
    controls::{Button, DELETE, ESCAPE, KEYS_PER_KEY, TABLE_BUTTONS, key_label, mappable, scancode_label, short_scancode_label},
    pointer::{Mouse, Target},
    settings::{Repeat, panel_bar},
    ui::{self, ACCENT, BAR, DIM, EDGE, GREEN, HIGHLIGHT, INK, MUTED, PANEL, ROW, SMALL, Screen, Style, TEXT, TINY, TINY_BOLD, WHITE, fit_with, rgb},
};

/// The small print on a key or a button: what is on a key of the keypad,
/// what a gamepad button is called.
const MICRO: Style = Style {
    size: 7.5,
    line: 9.0,
    bold: true,
};

/// A key or a button with nothing on it.
const NOTHING: Color = rgb(0x5f, 0x6a, 0x72);
const KEY_EDGE: Color = rgb(0x3a, 0x43, 0x4b);

/// A box on a screen, in layout units.
#[derive(Clone, Copy)]
pub(crate) struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// The handset's keys as its keypad: the soft keys, the directions and the
/// call keys on the left, the number pad on the right. Each by key index,
/// the column of its block - the left block's three, then the right's - and
/// its row.
const KEYPAD: [(i32, i32, i32); 22] = [
    (5, 0, 0),
    (0, 1, 0),
    (6, 2, 0),
    (2, 0, 1),
    (4, 1, 1),
    (3, 2, 1),
    (20, 0, 2),
    (1, 1, 2),
    (21, 2, 2),
    (7, 1, 3),
    (9, 3, 0),
    (10, 4, 0),
    (11, 5, 0),
    (12, 3, 1),
    (13, 4, 1),
    (14, 5, 1),
    (15, 3, 2),
    (16, 4, 2),
    (17, 5, 2),
    (18, 3, 3),
    (8, 4, 3),
    (19, 5, 3),
];

/// Where the keypad's top row is, under the title, and how far apart its rows
/// are.
const KEYPAD_TOP: i32 = 27;
const KEY_HEIGHT: i32 = 29;
const KEY_PITCH: i32 = 32;

/// The keypad's keys on a screen `width` wide, the two blocks a little apart
/// and as wide as the screen has room for up to 400 units.
fn keypad_rects(width: u32) -> Vec<Rect> {
    let room = width.min(400) as i32 - 16;
    let (gap, block_gap) = (4, 16);
    let key_width = (room - block_gap - 4 * gap) / 6;
    let left = (width as i32 - (6 * key_width + 4 * gap + block_gap)) / 2;
    KEYPAD
        .iter()
        .map(|(_, column, row)| Rect {
            x: left + column * (key_width + gap) + if *column >= 3 { block_gap - gap } else { 0 },
            y: KEYPAD_TOP + row * KEY_PITCH,
            width: key_width,
            height: KEY_HEIGHT,
        })
        .collect()
}

/// Where the pad's keypad starts, under its tabs, and how far apart its rows
/// are: a little tighter than the keyboard's, for the tabs and the line
/// under it to fit.
const PAD_KEYPAD_TOP: i32 = 43;
const PAD_KEY_PITCH: i32 = 29;

/// Where a 320x240 drawing goes on a screen of the settings' size: in the
/// middle of it.
fn origin(width: u32, height: u32) -> (i32, i32) {
    (((width as i32 - 320) / 2).max(0), ((height as i32 - 240) / 2).max(0))
}

/// A key of the keypad or a button of the pad: `top` over `bottom` in a
/// rounded box, white when it is the one picked.
fn draw_chip(screen: &mut Screen, rect: Rect, radius: f32, picked: bool, top: (&str, Style, Color), bottom: (&str, Style, Color)) {
    let (x, y, width, height) = (rect.x as f32, rect.y as f32, rect.width as f32, rect.height as f32);
    let (fill, edge) = if picked { (WHITE, GREEN) } else { (ROW, KEY_EDGE) };
    screen.round(x, y, width, height, radius, fill);
    screen.outline(x, y, width, height, radius, 1.0, edge);
    let text_top = y + (height - top.1.line - bottom.1.line) / 2.0;
    for (index, (text, style, color)) in [top, bottom].into_iter().enumerate() {
        let line_y = text_top + if index == 0 { 0.0 } else { top.1.line };
        screen.text_styled(
            &fit_with(text, width - 4.0, style),
            x + width / 2.0,
            line_y,
            style,
            TextAlignment::Center,
            color,
        );
    }
}

/// One slot of the strip under a layout: its name over the key in it, picked
/// out when it is the slot being changed. How wide it came out.
#[allow(clippy::too_many_arguments)]
fn draw_slot(screen: &mut Screen, x: f32, y: f32, name: &str, key: &str, set: bool, picked: bool, room: f32) -> f32 {
    screen.text_styled(name, x, y, TINY, TextAlignment::Left, MUTED);
    let key = fit_with(key, room - 16.0, SMALL);
    let width = (ui::text_width(&key, SMALL) + 16.0).max(52.0);
    let (fill, edge, ink) = if picked {
        (HIGHLIGHT, ACCENT, TEXT)
    } else {
        (ROW, EDGE, if set { ACCENT } else { NOTHING })
    };
    screen.round(x, y + 13.0, width, 19.0, 5.0, fill);
    screen.outline(x, y + 13.0, width, 19.0, 5.0, 1.0, edge);
    screen.text_styled(&key, x + width / 2.0, y + 16.0, SMALL, TextAlignment::Center, ink);
    width
}

impl App {
    /// A layout screen's title, with the preset in use and the button that
    /// puts the defaults back beside it.
    fn draw_layout_title(&self, screen: &mut Screen, width: u32, title: &str) {
        screen.header(0, 0, width, title, "");
        let label = "↺ 기본값";
        let button_width = ui::text_width(label, TINY_BOLD) + 12.0;
        let button_x = width as f32 - 6.0 - button_width;
        screen.round(button_x, 3.5, button_width, 13.0, 4.0, WHITE);
        screen.text_styled(label, button_x + button_width / 2.0, 4.0, TINY_BOLD, TextAlignment::Center, INK);
        self.hit(button_x as i32 - 2, 0, button_width as i32 + 4, BAR, Target::Press(Button::L3));
        let room = button_x - 8.0 - ui::text_width(title, ui::TITLE) - 24.0;
        let preset = format!("프리셋: {}", self.active_label());
        screen.text_styled(&fit_with(&preset, room, SMALL), button_x - 8.0, 3.5, SMALL, TextAlignment::Right, MUTED);
    }

    /// Asks before putting the keyboard's half of the mapping - or the pad's -
    /// back to the defaults, over the screen `back` draws. Whether to.
    fn confirm_reset(&mut self, keyboard: bool, back: &dyn Fn(&App) -> (Screen, u32, u32)) -> bool {
        // On 되돌리기, since the button for it was just pressed.
        let mut cursor: usize = 1;
        let mut repeat = Repeat::new();
        let mut dirty = true;
        loop {
            let pressed = self.presses(&mut repeat);
            if let Some((pointed, _)) = self.take_pointed() {
                cursor = pointed.min(1);
                dirty = true;
            }
            for button in pressed {
                dirty = true;
                match button {
                    Button::Up | Button::Down | Button::Left | Button::Right => cursor = 1 - cursor,
                    Button::A => return cursor == 1,
                    Button::B | Button::Guide => return false,
                    _ => {}
                }
            }

            if dirty || self.take_redraw() {
                let (mut screen, width, height) = back(self);
                screen.fill(0, 0, width, height, DIM);
                self.clear_hits();

                let panel_width = (width - 16).min(256);
                let panel_height = 112;
                let x = (width - panel_width) as i32 / 2;
                let y = (height as i32 - panel_height) / 2;
                screen.panel(x, y, panel_width, panel_height as u32);
                panel_bar(&mut screen, x, y, panel_width, "↺ 기본값으로 되돌리기", "");
                let (what, other) = if keyboard {
                    ("키보드 배치", Some("패드 버튼 배치는 그대로예요."))
                } else {
                    ("버튼 배치", self.desktop.then_some("키보드 배치는 그대로예요."))
                };
                let room = panel_width as f32 - 20.0;
                let (left, top) = (x as f32 + 10.0, (y + BAR) as f32);
                let question = format!("{what}를 처음 설정으로 되돌릴까요?");
                screen.text_styled(&fit_with(&question, room, SMALL), left, top + 7.0, SMALL, TextAlignment::Left, TEXT);
                let scope = format!(
                    "지금 프리셋 '{}'의 {}만 바뀌어요.",
                    self.store.active(),
                    if keyboard { "키보드 칸" } else { "버튼 칸" }
                );
                screen.text_styled(&fit_with(&scope, room, TINY), left, top + 25.0, TINY, TextAlignment::Left, MUTED);
                if let Some(other) = other {
                    screen.text_styled(other, left, top + 38.0, TINY, TextAlignment::Left, MUTED);
                }

                // 취소 and 되돌리기 along the bottom right.
                let button_y = (y + panel_height) as f32 - 27.0;
                let mut right = (x + panel_width as i32) as f32 - 10.0;
                for (index, label) in [(1, "되돌리기"), (0, "취소")] {
                    let button_width = ui::text_width(label, SMALL) + 22.0;
                    let button_x = right - button_width;
                    let (fill, edge, ink) = if index == cursor { (WHITE, WHITE, INK) } else { (ROW, KEY_EDGE, TEXT) };
                    screen.round(button_x, button_y, button_width, 19.0, 5.0, fill);
                    screen.outline(button_x, button_y, button_width, 19.0, 5.0, 1.0, edge);
                    screen.text_styled(label, button_x + button_width / 2.0, button_y + 3.0, SMALL, TextAlignment::Center, ink);
                    self.hit(button_x as i32, button_y as i32, button_width as i32, 19, Target::Row(index));
                    right = button_x - 6.0;
                }
                screen.bar(
                    height as i32 - BAR,
                    self.hint("A 선택  ◀▶ 고르기", "Enter 선택  ◀▶ 고르기"),
                    self.hint("B 취소", "Esc 취소"),
                );
                self.back_hit(width, height);
                self.present(screen, width, height);
                dirty = false;
            }
            std::thread::sleep(FRAME);
        }
    }

    /// The handset's keypad, each key with the pad buttons that press it in
    /// one layer - plain, or with SELECT held - the way the keyboard's layout
    /// shows its keys: pick a key, then press the button to put on it.
    pub(crate) fn layout(&mut self) {
        // On 확인.
        let mut index = KEYPAD.iter().position(|(key, ..)| *key == 4).unwrap_or(0);
        let mut with_select = false;
        let mut status = String::new();
        let mut repeat = Repeat::new();
        let mut dirty = true;
        loop {
            let pressed = self.presses(&mut repeat);
            if let Some((pointed, column)) = self.take_pointed() {
                index = pointed.min(KEYPAD.len() - 1);
                with_select = column.map_or(with_select, |column| column == 1);
                dirty = true;
            }
            for button in pressed {
                dirty = true;
                status.clear();
                let handset = KEYPAD[index].0;
                match button {
                    Button::Up | Button::Down | Button::Left | Button::Right => {
                        let (width, _, _) = self.menu_size();
                        index = step(&keypad_rects(width), index, button);
                    }
                    Button::L1 | Button::L2 => with_select = false,
                    Button::R1 | Button::R2 => with_select = true,
                    // Tab on the keyboard: the other layer.
                    Button::Select => with_select = !with_select,
                    Button::B | Button::Guide => return,
                    // Delete, or Y on a pad: nothing presses the key in this
                    // layer any more.
                    Button::Y => self.clear_pad_key(handset, with_select),
                    Button::X | Button::L3 => {
                        if self.confirm_reset(false, &|app: &App| app.draw_layout(index, with_select, "")) {
                            self.store.reset_pad();
                            status = "버튼 배치를 기본값으로 되돌렸습니다.".to_owned();
                        }
                    }
                    Button::A => {
                        if let Some(outcome) = self.capture_button(handset, with_select, index) {
                            status = outcome;
                        }
                    }
                    _ => {}
                }
            }

            if dirty || self.take_redraw() {
                let (screen, width, height) = self.draw_layout(index, with_select, &status);
                self.present(screen, width, height);
                dirty = false;
            }
            std::thread::sleep(FRAME);
        }
    }

    /// The pad buttons that press `handset` in one layer, by their names.
    fn pad_buttons_of(&self, handset: i32, with_select: bool) -> Vec<&'static str> {
        let controls = self.store.controls();
        TABLE_BUTTONS
            .iter()
            .filter(|(button, _)| controls.get(*button, with_select) == Some(handset))
            .map(|(_, name)| *name)
            .collect()
    }

    /// Takes every button off `handset` in one layer.
    fn clear_pad_key(&mut self, handset: i32, with_select: bool) {
        for (button, _) in TABLE_BUTTONS {
            if self.store.controls().get(button, with_select) == Some(handset) {
                self.store.set(button, with_select, None);
            }
        }
    }

    fn draw_layout(&self, index: usize, with_select: bool, status: &str) -> (Screen, u32, u32) {
        let (width, height, _) = self.menu_size();
        let mut screen = Screen::new(width, height);
        self.clear_hits();
        self.draw_layout_title(&mut screen, width, if self.desktop { "패드 버튼 배치" } else { "버튼 배치" });
        let (left, top) = origin(width, height);

        // The layer shown: a tab each, over the keypad.
        let mut tab_x = (left + 8) as f32;
        let tab_y = (top + 24) as f32;
        for (tab, label) in [(false, "그냥 누를 때"), (true, "SELECT 누른 채")] {
            let tab_width = ui::text_width(label, TINY_BOLD) + 18.0;
            if tab == with_select {
                screen.round(tab_x, tab_y, tab_width, 15.0, 7.5, WHITE);
            } else {
                screen.round(tab_x, tab_y, tab_width, 15.0, 7.5, PANEL);
                screen.outline(tab_x, tab_y, tab_width, 15.0, 7.5, 1.0, EDGE);
            }
            let ink = if tab == with_select { INK } else { MUTED };
            screen.text_styled(label, tab_x + tab_width / 2.0, tab_y + 1.5, TINY_BOLD, TextAlignment::Center, ink);
            let press = if tab { Button::R1 } else { Button::L1 };
            self.hit(tab_x as i32, tab_y as i32, tab_width as i32, 15, Target::Press(press));
            tab_x += tab_width + 4.0;
        }

        // The keypad, a little lower than the keyboard's to make room for
        // the tabs, each key with the buttons on it in the layer shown.
        for (position, ((handset, _, row), rect)) in KEYPAD.iter().zip(keypad_rects(width)).enumerate() {
            let rect = Rect {
                y: top + PAD_KEYPAD_TOP + row * PAD_KEY_PITCH,
                height: PAD_KEY_PITCH - 3,
                ..rect
            };
            let picked = position == index;
            let buttons = self.pad_buttons_of(*handset, with_select);
            let mapped = if buttons.is_empty() { "없음".to_owned() } else { buttons.join(" · ") };
            let name = key_label(Some(*handset));
            let name_style = if ui::text_width(name, SMALL) <= rect.width as f32 - 6.0 {
                SMALL
            } else {
                TINY_BOLD
            };
            let mapped_color = match (picked, buttons.is_empty()) {
                (true, _) => GREEN,
                (false, true) => NOTHING,
                (false, false) => ACCENT,
            };
            draw_chip(
                &mut screen,
                rect,
                6.0,
                picked,
                (name, name_style, if picked { INK } else { TEXT }),
                (&mapped, MICRO, mapped_color),
            );
            self.hit(rect.x, rect.y, rect.width, rect.height, Target::Select(position));
        }

        if !status.is_empty() {
            screen.text_styled(
                &fit_with(status, width as f32 - 16.0, TINY_BOLD),
                width as f32 / 2.0,
                (top + 157) as f32,
                TINY_BOLD,
                TextAlignment::Center,
                ACCENT,
            );
        }

        // The key picked, and the buttons on it in each layer.
        let (strip_x, strip_y) = ((left + 8) as f32, (top + 170) as f32);
        screen.round(strip_x, strip_y, 304.0, 44.0, 7.0, PANEL);
        screen.outline(strip_x, strip_y, 304.0, 44.0, 7.0, 1.0, EDGE);
        let handset = KEYPAD[index].0;
        let name = key_label(Some(handset));
        let big_width = (ui::text_width(name, SMALL) + 16.0).max(44.0);
        screen.round(strip_x + 9.0, strip_y + 7.0, big_width, 30.0, 6.0, WHITE);
        screen.text_styled(name, strip_x + 9.0 + big_width / 2.0, strip_y + 15.5, SMALL, TextAlignment::Center, INK);
        let mut slot_x = strip_x + 9.0 + big_width + 6.0;
        slot_x += screen.text_styled("→", slot_x, strip_y + 15.5, SMALL, TextAlignment::Left, MUTED) + 10.0;
        for (layer, label) in [(false, "그냥"), (true, "SELECT+")] {
            let buttons = self.pad_buttons_of(handset, layer);
            let text = if buttons.is_empty() { "없음".to_owned() } else { buttons.join(" · ") };
            let slot_width = draw_slot(
                &mut screen,
                slot_x,
                strip_y + 5.0,
                label,
                &text,
                !buttons.is_empty(),
                layer == with_select,
                96.0,
            );
            self.hit(slot_x as i32, strip_y as i32, slot_width as i32, 44, Target::Cell(index, layer as usize));
            slot_x += slot_width + 10.0;
        }

        let hint = self.hint("A 바꾸기  SELECT 칸  Y 지우기  X 기본값", "Enter 바꾸기  Tab 칸  Del 지우기  F9 기본값");
        screen.bar(height as i32 - BAR, hint, self.hint("B 뒤로", "Esc 뒤로"));
        self.back_hit(width, height);
        (screen, width, height)
    }

    /// Waits for the pad button to put on `handset` in one layer and puts it
    /// there: a button on another key moves over, one already on this key
    /// comes off. What happened, to say under the keypad; `None` when
    /// cancelled - Esc, MENU or the mouse's right button, since every other
    /// button is one that can be picked.
    fn capture_button(&mut self, handset: i32, with_select: bool, index: usize) -> Option<String> {
        let layer = if with_select { "SELECT 누른 채" } else { "그냥 누를 때" };
        let mut note = String::new();
        let mut dirty = true;
        loop {
            for input in self.poll() {
                dirty = true;
                match input {
                    Input::Quit => {
                        host::stop();
                        std::process::exit(0);
                    }
                    Input::Key(ESCAPE, true, _) | Input::Button(Button::Guide, true) | Input::Mouse(Mouse::Back) => return None,
                    Input::Key(DELETE, true, _) => {
                        self.clear_pad_key(handset, with_select);
                        return Some(format!("{}: {layer} 버튼을 모두 뺐습니다.", key_label(Some(handset))));
                    }
                    Input::Button(Button::Select, true) => note = "SELECT는 칸 전환이라 쓸 수 없습니다.".to_owned(),
                    Input::Button(Button::Start, true) if with_select => {
                        note = "SELECT+START는 게임 종료라 쓸 수 없습니다.".to_owned();
                    }
                    Input::Button(button, true) => {
                        let Some((_, name)) = TABLE_BUTTONS.iter().find(|(x, _)| *x == button) else {
                            note = "이 버튼은 쓸 수 없습니다.".to_owned();
                            continue;
                        };
                        let before = self.store.controls().get(button, with_select);
                        if before == Some(handset) {
                            self.store.set(button, with_select, None);
                            return Some(format!("{name}: '{}'에서 뺐습니다.", key_label(Some(handset))));
                        }
                        self.store.set(button, with_select, Some(handset));
                        return Some(match before {
                            Some(from) => format!("{name}: '{}'에서 옮겨 왔습니다.", key_label(Some(from))),
                            None => format!("{name}: '{}'에 붙였습니다.", key_label(Some(handset))),
                        });
                    }
                    _ => {}
                }
            }

            if dirty || self.take_redraw() {
                let (mut screen, width, height) = self.draw_layout(index, with_select, "");
                screen.fill(0, 0, width, height, DIM);
                self.clear_hits();
                let (panel_width, panel_height) = (248u32, 120u32);
                let x = (width - panel_width) as i32 / 2;
                let y = (height - panel_height) as i32 / 2;
                screen.panel(x, y, panel_width, panel_height);
                let title = format!("{} ({layer})", key_label(Some(handset)));
                let now = self.pad_buttons_of(handset, with_select);
                let now = if now.is_empty() { "없음".to_owned() } else { now.join(" · ") };
                panel_bar(&mut screen, x, y, panel_width, &title, &format!("지금: {now}"));
                let middle = x + panel_width as i32 / 2;
                screen.text("쓸 버튼을 누르세요", middle, y + BAR + 10, TextAlignment::Center, TEXT);
                let line = if note.is_empty() { "…" } else { note.as_str() };
                let room = panel_width as f32 - 16.0;
                screen.text_styled(
                    &fit_with(line, room, TINY_BOLD),
                    middle as f32,
                    (y + BAR + 32) as f32,
                    TINY_BOLD,
                    TextAlignment::Center,
                    ACCENT,
                );
                let rule = "다른 키의 버튼은 옮겨 오고, 이 키의 버튼은 빠집니다";
                screen.text_styled(
                    &fit_with(rule, room, TINY),
                    middle as f32,
                    (y + BAR + 47) as f32,
                    TINY,
                    TextAlignment::Center,
                    MUTED,
                );
                let clear = if self.desktop { "Del 비우기" } else { "" };
                self.back_bar(
                    &mut screen,
                    x,
                    y + panel_height as i32 - BAR,
                    panel_width,
                    clear,
                    self.hint("MENU 취소", "Esc·MENU 취소"),
                );
                self.present(screen, width, height);
                dirty = false;
            }
            std::thread::sleep(FRAME);
        }
    }

    /// The handset's keypad, each key with the keyboard keys that press it.
    pub(crate) fn keyboard_layout(&mut self) {
        // On 확인.
        let mut index = KEYPAD.iter().position(|(key, ..)| *key == 4).unwrap_or(0);
        let mut slot: usize = 0;
        let mut status = String::new();
        let mut repeat = Repeat::new();
        let mut dirty = true;
        loop {
            let pressed = self.presses(&mut repeat);
            if let Some((pointed, column)) = self.take_pointed() {
                index = pointed.min(KEYPAD.len() - 1);
                slot = column.unwrap_or(slot).min(KEYS_PER_KEY - 1);
                dirty = true;
            }
            for button in pressed {
                dirty = true;
                status.clear();
                let handset = KEYPAD[index].0;
                match button {
                    Button::Up | Button::Down | Button::Left | Button::Right => {
                        let (width, _, _) = self.menu_size();
                        index = step(&keypad_rects(width), index, button);
                    }
                    // Tab: the other of the key's two keyboard keys.
                    Button::Select => slot = (slot + 1) % KEYS_PER_KEY,
                    Button::B | Button::Guide => return,
                    // Delete, or Y on a pad: empty the slot.
                    Button::Y => {
                        self.store.set_keyboard(handset, slot, None);
                    }
                    Button::L3 => {
                        if self.confirm_reset(true, &|app: &App| app.draw_keyboard(index, slot, "")) {
                            self.store.reset_keyboard();
                            status = "키보드 배치를 기본값으로 되돌렸습니다.".to_owned();
                        }
                    }
                    Button::A => {
                        if let Some(code) = self.capture_key(handset, slot, index)
                            && let Some(from) = self.store.set_keyboard(handset, slot, code)
                        {
                            status = format!("{}: '{}'에서 옮겨 왔습니다.", scancode_label(code), key_label(Some(from)));
                        }
                    }
                    _ => {}
                }
            }

            if dirty || self.take_redraw() {
                let (screen, width, height) = self.draw_keyboard(index, slot, &status);
                self.present(screen, width, height);
                dirty = false;
            }
            std::thread::sleep(FRAME);
        }
    }

    fn draw_keyboard(&self, index: usize, slot: usize, status: &str) -> (Screen, u32, u32) {
        let (width, height, _) = self.menu_size();
        let mut screen = Screen::new(width, height);
        self.clear_hits();
        self.draw_layout_title(&mut screen, width, "키보드 배치");
        let (left, top) = origin(width, height);

        let controls = self.store.controls();
        let keys_of = |handset: i32| -> Vec<i32> { (0..KEYS_PER_KEY).filter_map(|slot| controls.keyboard(handset, slot)).collect() };
        for (position, ((handset, ..), rect)) in KEYPAD.iter().zip(keypad_rects(width)).enumerate() {
            let rect = Rect { y: rect.y + top, ..rect };
            let picked = position == index;
            let keys = keys_of(*handset);
            let mapped = if keys.is_empty() {
                "없음".to_owned()
            } else {
                keys.iter().map(|code| short_scancode_label(*code)).collect::<Vec<_>>().join(" · ")
            };
            let name = key_label(Some(*handset));
            let name_style = if ui::text_width(name, SMALL) <= rect.width as f32 - 6.0 {
                SMALL
            } else {
                TINY_BOLD
            };
            let mapped_color = match (picked, keys.is_empty()) {
                (true, _) => GREEN,
                (false, true) => NOTHING,
                (false, false) => ACCENT,
            };
            draw_chip(
                &mut screen,
                rect,
                6.0,
                picked,
                (name, name_style, if picked { INK } else { TEXT }),
                (&mapped, MICRO, mapped_color),
            );
            self.hit(rect.x, rect.y, rect.width, rect.height, Target::Select(position));
        }

        if !status.is_empty() {
            screen.text_styled(
                &fit_with(status, width as f32 - 16.0, TINY_BOLD),
                width as f32 / 2.0,
                (top + 154) as f32,
                TINY_BOLD,
                TextAlignment::Center,
                ACCENT,
            );
        }

        // The key picked, and its two keyboard keys.
        let (strip_x, strip_y) = ((left + 8) as f32, (top + 168) as f32);
        screen.round(strip_x, strip_y, 304.0, 44.0, 7.0, PANEL);
        screen.outline(strip_x, strip_y, 304.0, 44.0, 7.0, 1.0, EDGE);
        let handset = KEYPAD[index].0;
        let name = key_label(Some(handset));
        let big_width = (ui::text_width(name, SMALL) + 16.0).max(44.0);
        screen.round(strip_x + 9.0, strip_y + 7.0, big_width, 30.0, 6.0, WHITE);
        screen.text_styled(name, strip_x + 9.0 + big_width / 2.0, strip_y + 15.5, SMALL, TextAlignment::Center, INK);
        let mut slot_x = strip_x + 9.0 + big_width + 6.0;
        slot_x += screen.text_styled("→", slot_x, strip_y + 15.5, SMALL, TextAlignment::Left, MUTED) + 10.0;
        for column in 0..KEYS_PER_KEY {
            let code = controls.keyboard(handset, column);
            let text = if code.is_some() { scancode_label(code) } else { "없음".to_owned() };
            let label = format!("키 {}", column + 1);
            let slot_width = draw_slot(&mut screen, slot_x, strip_y + 5.0, &label, &text, code.is_some(), column == slot, 96.0);
            self.hit(slot_x as i32, strip_y as i32, slot_width as i32, 44, Target::Cell(index, column));
            slot_x += slot_width + 10.0;
        }

        let hint = self.hint("A 바꾸기  SELECT 칸  Y 지우기", "Enter 바꾸기  Tab 칸  Del 지우기  F9 기본값");
        screen.bar(height as i32 - BAR, hint, self.hint("B 뒤로", "Esc 뒤로"));
        self.back_hit(width, height);
        (screen, width, height)
    }

    /// Waits for the keyboard key to put in one slot of a handset key. `None`
    /// when cancelled (Esc, or B on a pad), `Some(None)` to empty the slot
    /// (Delete, or Y on a pad).
    fn capture_key(&mut self, handset: i32, slot: usize, index: usize) -> Option<Option<i32>> {
        let current = self.store.controls().keyboard(handset, slot);
        let mut note = String::new();
        let mut dirty = true;
        loop {
            for input in self.poll() {
                dirty = true;
                match input {
                    Input::Quit => {
                        host::stop();
                        std::process::exit(0);
                    }
                    Input::Key(ESCAPE, true, _) | Input::Button(Button::B, true) | Input::Mouse(Mouse::Back) => return None,
                    Input::Key(DELETE, true, _) | Input::Button(Button::Y, true) => return Some(None),
                    Input::Key(code, true, false) => {
                        if mappable(code) {
                            return Some(Some(code));
                        }
                        note = "이 키는 쓸 수 없습니다.".to_owned();
                    }
                    _ => {}
                }
            }

            if dirty || self.take_redraw() {
                let (mut screen, width, height) = self.draw_keyboard(index, slot, "");
                screen.fill(0, 0, width, height, DIM);
                self.clear_hits();
                let (panel_width, panel_height) = (240u32, 108u32);
                let x = (width - panel_width) as i32 / 2;
                let y = (height - panel_height) as i32 / 2;
                screen.panel(x, y, panel_width, panel_height);
                let title = format!("{} (키 {})", key_label(Some(handset)), slot + 1);
                panel_bar(&mut screen, x, y, panel_width, &title, &format!("지금: {}", scancode_label(current)));
                screen.text("쓸 키를 누르세요", x + panel_width as i32 / 2, y + BAR + 18, TextAlignment::Center, TEXT);
                let line = if note.is_empty() { "…" } else { note.as_str() };
                screen.text(line, x + panel_width as i32 / 2, y + BAR + 42, TextAlignment::Center, ACCENT);
                self.back_bar(&mut screen, x, y + panel_height as i32 - BAR, panel_width, "Esc 취소", "Del 없음");
                self.present(screen, width, height);
                dirty = false;
            }
            std::thread::sleep(FRAME);
        }
    }
}

/// The box a direction leads to from `from`: of the boxes wholly past its
/// edge that way, the nearest, distance across the direction counting
/// double; `from` itself at an edge.
fn step(rects: &[Rect], from: usize, direction: Button) -> usize {
    let centre = |rect: &Rect| (rect.x + rect.width / 2, rect.y + rect.height / 2);
    let origin = &rects[from];
    let (fx, fy) = centre(origin);
    rects
        .iter()
        .enumerate()
        .filter(|(_, rect)| match direction {
            Button::Up => rect.y + rect.height <= origin.y,
            Button::Down => rect.y >= origin.y + origin.height,
            Button::Left => rect.x + rect.width <= origin.x,
            _ => rect.x >= origin.x + origin.width,
        })
        .map(|(index, rect)| {
            let (cx, cy) = centre(rect);
            let (along, across) = match direction {
                Button::Up | Button::Down => ((cy - fy).abs(), cx - fx),
                _ => ((cx - fx).abs(), cy - fy),
            };
            (index, along + 2 * across.abs())
        })
        .min_by_key(|(_, distance)| *distance)
        .map_or(from, |(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keypad_moves_between_its_blocks() {
        let rects = keypad_rects(320);
        let at = |key: i32| KEYPAD.iter().position(|(x, ..)| *x == key).unwrap();
        // ▲ ◀ 확인 ▶ ▼ round the middle, 취소 under ▼.
        assert_eq!(step(&rects, at(4), Button::Up), at(0));
        assert_eq!(step(&rects, at(4), Button::Left), at(2));
        assert_eq!(step(&rects, at(1), Button::Down), at(7));
        // From the right soft key across to 1, and from 1 back.
        assert_eq!(step(&rects, at(6), Button::Right), at(9));
        assert_eq!(step(&rects, at(9), Button::Left), at(6));
        // The whole keypad fits the narrowest screen.
        assert!(rects.iter().all(|rect| rect.x >= 0 && rect.x + rect.width <= 320));
    }
}
