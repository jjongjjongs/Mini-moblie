//! The two layout screens: the handset's keypad drawn with the keyboard keys
//! on each of its keys, and a gamepad drawn with the handset key on each of
//! its buttons. Either can put its half of the mapping back to the defaults.

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

/// The handset key on a gamepad button.
const CHIP: Style = Style {
    size: 8.5,
    line: 10.0,
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

/// Where each of the table's buttons is on the gamepad, in `TABLE_BUTTONS`'
/// order, on a 320-unit drawing: x, y, width, height and how round.
const PAD: [(i32, i32, i32, i32, f32); 15] = [
    (51, 78, 29, 23, 5.0),
    (51, 126, 29, 23, 5.0),
    (20, 102, 29, 23, 5.0),
    (82, 102, 29, 23, 5.0),
    (268, 100, 35, 28, 14.0),
    (237, 124, 35, 28, 14.0),
    (237, 76, 35, 28, 14.0),
    (206, 100, 35, 28, 14.0),
    (82, 50, 56, 22, 6.0),
    (182, 50, 56, 22, 6.0),
    (22, 50, 56, 22, 6.0),
    (242, 50, 56, 22, 6.0),
    (116, 116, 37, 37, 18.5),
    (167, 116, 37, 37, 18.5),
    (161, 85, 48, 20, 10.0),
];

/// Where each direction leads from each of the pad's buttons, in the same
/// order: up, down, left, right. Set by hand, since the buttons sit on
/// diagonals no nearest-box rule reads the way a thumb does.
const PAD_NEXT: [[usize; 4]; 15] = [
    [10, 1, 2, 3],
    [0, 1, 2, 3],
    [0, 1, 2, 3],
    [0, 1, 2, 12],
    [6, 5, 7, 4],
    [6, 5, 7, 4],
    [11, 5, 14, 4],
    [6, 5, 13, 4],
    [8, 0, 10, 9],
    [9, 14, 8, 11],
    [10, 0, 10, 8],
    [11, 6, 9, 11],
    [8, 12, 3, 13],
    [14, 13, 12, 7],
    [9, 13, 3, 6],
];

fn pad_step(from: usize, direction: Button) -> usize {
    let way = match direction {
        Button::Up => 0,
        Button::Down => 1,
        Button::Left => 2,
        _ => 3,
    };
    PAD_NEXT[from][way]
}

/// SELECT, which is not mapped but holds the second layer.
const PAD_SELECT: (i32, i32, i32, i32, f32) = (111, 85, 48, 20, 10.0);

fn pad_rect(left: i32, top: i32, (x, y, width, height, _): (i32, i32, i32, i32, f32)) -> Rect {
    Rect {
        x: left + x,
        y: top + y,
        width,
        height,
    }
}

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

    /// The pad's buttons on a drawing of a gamepad, plain or with SELECT held.
    pub(crate) fn layout(&mut self) {
        // On A, the button most often changed.
        let mut index: usize = 4;
        let mut with_select = false;
        let mut repeat = Repeat::new();
        let mut dirty = true;
        loop {
            let pressed = self.presses(&mut repeat);
            if let Some((pointed, column)) = self.take_pointed() {
                index = pointed.min(TABLE_BUTTONS.len() - 1);
                with_select = column.map_or(with_select, |column| column == 1);
                dirty = true;
            }
            for button in pressed {
                dirty = true;
                match button {
                    Button::Up | Button::Down | Button::Left | Button::Right => index = pad_step(index, button),
                    Button::L1 | Button::L2 => with_select = false,
                    Button::R1 | Button::R2 => with_select = true,
                    Button::Select => with_select = !with_select,
                    Button::B | Button::Guide => return,
                    Button::X | Button::L3 => {
                        if self.confirm_reset(false, &|app: &App| app.draw_layout(index, with_select)) {
                            self.store.reset_pad();
                        }
                    }
                    Button::A => {
                        let (target, name) = TABLE_BUTTONS[index];
                        let current = self.store.controls().get(target, with_select);
                        let title = format!("{name} 버튼 ({})", if with_select { "SELECT+" } else { "그냥" });
                        if let Some(key) = self.pick_key(&title, current, index, with_select) {
                            self.store.set(target, with_select, key);
                        }
                    }
                    _ => {}
                }
            }

            if dirty || self.take_redraw() {
                let (screen, width, height) = self.draw_layout(index, with_select);
                self.present(screen, width, height);
                dirty = false;
            }
            std::thread::sleep(FRAME);
        }
    }

    fn draw_layout(&self, index: usize, with_select: bool) -> (Screen, u32, u32) {
        let (width, height, _) = self.menu_size();
        let mut screen = Screen::new(width, height);
        self.clear_hits();
        self.draw_layout_title(&mut screen, width, if self.desktop { "패드 버튼 배치" } else { "버튼 배치" });
        let (left, top) = origin(width, height);

        // The layer shown: a tab each.
        let mut tab_x = (left + 8) as f32;
        for (tab, label) in [(false, "그냥 누를 때"), (true, "SELECT 누른 채")] {
            let tab_width = ui::text_width(label, TINY_BOLD) + 18.0;
            let tab_y = (top + 25) as f32;
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

        // The gamepad, and on each button what it presses in that layer. A
        // button with nothing under SELECT keeps its own key there, muted.
        let body = (left as f32 + 12.0, top as f32 + 46.0);
        screen.round(body.0, body.1, 296.0, 118.0, 30.0, PANEL);
        screen.outline(body.0, body.1, 296.0, 118.0, 30.0, 1.0, EDGE);
        let controls = self.store.controls();
        let shown = |button: Button, layer: bool| match controls.get(button, layer) {
            Some(key) => (Some(key), true),
            None if layer => (controls.get(button, false), false),
            None => (None, false),
        };
        for (position, ((button, name), place)) in TABLE_BUTTONS.iter().zip(PAD).enumerate() {
            let rect = pad_rect(left, top, place);
            let picked = position == index;
            let (key, own) = shown(*button, with_select);
            let value = if key.is_some() { key_label(key) } else { "없음" };
            let value_color = match (picked, key.is_some() && own) {
                (true, true) => INK,
                (true, false) => MUTED,
                (false, true) => ACCENT,
                (false, false) => NOTHING,
            };
            let name_color = if picked { GREEN } else { MUTED };
            draw_chip(&mut screen, rect, place.4, picked, (name, MICRO, name_color), (value, CHIP, value_color));
            self.hit(rect.x, rect.y, rect.width, rect.height, Target::Select(position));
        }
        let select = pad_rect(left, top, PAD_SELECT);
        draw_chip(
            &mut screen,
            select,
            PAD_SELECT.4,
            false,
            ("SELECT", MICRO, MUTED),
            ("숫자 전환", MICRO, MUTED),
        );

        // The button picked, and what it presses in each layer.
        let (strip_x, strip_y) = ((left + 8) as f32, (top + 170) as f32);
        screen.round(strip_x, strip_y, 304.0, 44.0, 7.0, PANEL);
        screen.outline(strip_x, strip_y, 304.0, 44.0, 7.0, 1.0, EDGE);
        let (button, name) = TABLE_BUTTONS[index];
        let big_width = (ui::text_width(name, SMALL) + 20.0).max(34.0);
        screen.round(strip_x + 9.0, strip_y + 8.0, big_width, 28.0, 14.0, WHITE);
        screen.text_styled(name, strip_x + 9.0 + big_width / 2.0, strip_y + 15.5, SMALL, TextAlignment::Center, INK);
        let mut slot_x = strip_x + 9.0 + big_width + 6.0;
        slot_x += screen.text_styled("→", slot_x, strip_y + 15.5, SMALL, TextAlignment::Left, MUTED) + 10.0;
        for (layer, label) in [(false, "그냥"), (true, "SELECT+")] {
            let (key, own) = shown(button, layer);
            let text = if key.is_some() { key_label(key) } else { "없음" };
            let slot_width = draw_slot(
                &mut screen,
                slot_x,
                strip_y + 5.0,
                label,
                text,
                key.is_some() && own,
                layer == with_select,
                90.0,
            );
            self.hit(slot_x as i32, strip_y as i32, slot_width as i32, 44, Target::Cell(index, layer as usize));
            slot_x += slot_width + 10.0;
        }

        let hint = self.hint("A 바꾸기  L1 R1 탭  X 기본값", "Enter 바꾸기  Tab 탭  F9 기본값");
        screen.bar(height as i32 - BAR, hint, self.hint("B 뒤로", "Esc 뒤로"));
        self.back_hit(width, height);
        (screen, width, height)
    }

    /// Picks a handset key for one button of the pad in one layer, over the
    /// gamepad. `None` when the pick is cancelled, `Some(None)` for no key.
    fn pick_key(&mut self, title: &str, current: Option<i32>, index: usize, with_select: bool) -> Option<Option<i32>> {
        let cells = picker_cells();
        let rects: Vec<Rect> = cells.iter().map(|cell| cell.rect).collect();
        let mut cursor = cells.iter().position(|cell| cell.key == current).unwrap_or(0);
        let mut repeat = Repeat::new();
        let mut dirty = true;
        loop {
            let pressed = self.presses(&mut repeat);
            if let Some((pointed, _)) = self.take_pointed() {
                cursor = pointed.min(cells.len() - 1);
                dirty = true;
            }
            for button in pressed {
                dirty = true;
                match button {
                    Button::Up | Button::Down | Button::Left | Button::Right => cursor = step(&rects, cursor, button),
                    Button::A => return Some(cells[cursor].key),
                    Button::B | Button::Guide => return None,
                    _ => {}
                }
            }

            if dirty || self.take_redraw() {
                let (mut screen, width, height) = self.draw_layout(index, with_select);
                screen.fill(0, 0, width, height, DIM);
                // Only the picker answers the mouse while it is up.
                self.clear_hits();

                let (panel_width, panel_height) = (272u32, 196u32);
                let x = (width - panel_width) as i32 / 2;
                let y = (height - panel_height) as i32 / 2;
                screen.panel(x, y, panel_width, panel_height);
                panel_bar(&mut screen, x, y, panel_width, title, &format!("지금: {}", key_label(current)));
                for (position, cell) in cells.iter().enumerate() {
                    let rect = cell.rect;
                    let fill = if position == cursor { HIGHLIGHT } else { ROW };
                    screen.round((x + rect.x) as f32, (y + rect.y) as f32, rect.width as f32, rect.height as f32, 4.0, fill);
                    self.hit(x + rect.x, y + rect.y, rect.width, rect.height, Target::Row(position));
                    let color = if cell.key.is_none() && position != cursor { MUTED } else { TEXT };
                    let label = if cell.key.is_none() { "없음" } else { key_label(cell.key) };
                    screen.text(
                        label,
                        x + rect.x + rect.width / 2,
                        y + rect.y + (rect.height - 16) / 2,
                        TextAlignment::Center,
                        color,
                    );
                }
                let (choose, cancel) = (self.hint("A 고르기", "Enter 고르기"), self.hint("B 취소", "Esc 취소"));
                self.back_bar(&mut screen, x, y + panel_height as i32 - BAR, panel_width, choose, cancel);
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

/// One key in the picker, placed inside its panel.
struct Cell {
    key: Option<i32>,
    rect: Rect,
}

/// The picker's keys: a handset's number pad on the left, the other keys in
/// two columns on the right, and "none" under them.
fn picker_cells() -> Vec<Cell> {
    let mut cells = Vec::new();
    // 1 2 3 / 4 5 6 / 7 8 9 / * 0 #, by key index.
    let pad = [9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 8, 19];
    for (index, key) in pad.into_iter().enumerate() {
        let rect = Rect {
            x: 10 + (index % 3) as i32 * 38,
            y: BAR + 8 + (index / 3) as i32 * 28,
            width: 34,
            height: 24,
        };
        cells.push(Cell { key: Some(key), rect });
    }
    // ▲ ▼ / ◀ ▶ / 확인 취소 / 좌소프트 우소프트 / 통화 종료.
    let keys = [0, 1, 2, 3, 4, 7, 5, 6, 20, 21];
    for (index, key) in keys.into_iter().enumerate() {
        let rect = Rect {
            x: 130 + (index % 2) as i32 * 68,
            y: BAR + 8 + (index / 2) as i32 * 23,
            width: 64,
            height: 20,
        };
        cells.push(Cell { key: Some(key), rect });
    }
    let rect = Rect {
        x: 130,
        y: BAR + 8 + 5 * 23,
        width: 132,
        height: 20,
    };
    cells.push(Cell { key: None, rect });
    cells
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

    fn rects_of(cells: &[Cell]) -> Vec<Rect> {
        cells.iter().map(|cell| cell.rect).collect()
    }

    #[test]
    fn the_picker_moves_like_a_keypad() {
        let cells = picker_cells();
        let rects = rects_of(&cells);
        let at = |key: Option<i32>| cells.iter().position(|cell| cell.key == key).unwrap();
        // 1 → 2 → 3 along the top row, 5 down to 8, and from 3 across to ▲.
        assert_eq!(step(&rects, at(Some(9)), Button::Right), at(Some(10)));
        assert_eq!(step(&rects, at(Some(13)), Button::Down), at(Some(16)));
        assert_eq!(step(&rects, at(Some(11)), Button::Right), at(Some(0)));
        // The bottom of the right column reaches "none", and nothing is below it.
        let none = at(None);
        assert_eq!(step(&rects, at(Some(20)), Button::Down), none);
        assert_eq!(step(&rects, none, Button::Down), none);
        // The top-left key stays put going up or left.
        assert_eq!(step(&rects, at(Some(9)), Button::Up), at(Some(9)));
        assert_eq!(step(&rects, at(Some(9)), Button::Left), at(Some(9)));
    }

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

    #[test]
    fn the_pad_moves_like_a_gamepad() {
        let at = |name: &str| TABLE_BUTTONS.iter().position(|(_, x)| *x == name).unwrap();
        // Round the face buttons: Y ← A, X over them, B under.
        assert_eq!(pad_step(at("A"), Button::Left), at("Y"));
        assert_eq!(pad_step(at("Y"), Button::Up), at("X"));
        assert_eq!(pad_step(at("X"), Button::Down), at("B"));
        // Up from the D-pad to the shoulder over it, and from D▶ on to L3.
        assert_eq!(pad_step(at("D▲"), Button::Up), at("L2"));
        assert_eq!(pad_step(at("D▶"), Button::Right), at("L3"));
        // Every way leads to a button, and every button is reached.
        let mut reached = [false; 15];
        for ways in PAD_NEXT {
            for next in ways {
                reached[next] = true;
            }
        }
        assert!(reached.iter().all(|x| *x));
        // Every button stays on the 320-unit drawing.
        let rects: Vec<Rect> = PAD.iter().map(|place| pad_rect(0, 0, *place)).collect();
        assert!(
            rects
                .iter()
                .all(|rect| rect.x >= 0 && rect.x + rect.width <= 320 && rect.y + rect.height <= 170)
        );
    }
}
