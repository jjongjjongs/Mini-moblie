//! The settings screens: the menu (Y or Esc on the game list, MENU or Esc in
//! a game), the keyboard and pad tables, the key pickers and the presets.
//!
//! They are driven by the pad's raw buttons and a fixed set of keyboard keys
//! (see `key_button`), never through the mapping they edit, so a mapping
//! however wrong cannot lock the player out of fixing it.

use std::{
    path::Path,
    time::{Duration, Instant},
};

use wie_android::host;
use wie_backend::canvas::{Color, TextAlignment};

pub(crate) use crate::ui::{ACCENT, DANGER, DIM, EDGE, PANEL};
use crate::{
    App, FRAME, Input,
    controls::{Button, DELETE, ESCAPE, KEYS_PER_KEY, TABLE_BUTTONS, TABLE_KEYS, key_label, mappable, scancode_label},
    icons, key_button,
    pointer::{Mouse, Target},
    presets::{DEFAULT_NAME, SCREEN_MAX},
    ui::{self, BAR, BODY, GREEN, HIGHLIGHT, INK, LINE, MUTED, Picture, ROW, SMALL, Screen, TEXT, TINY, TINY_BOLD, WHITE, fit, fit_with, rgb},
};

/// A game's file name, which what is kept for it is filed under.
fn file_of(game: &Path) -> String {
    game.file_name().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Where the settings were opened from.
pub struct Context<'a> {
    /// The game picked on the list, or the one running.
    pub game: Option<&'a Path>,
    /// The running game's last frame, shown dimmed behind the menu. `None`
    /// on the list.
    pub frame: Option<(u32, u32, &'a [u8])>,
}

impl Context<'_> {
    fn in_game(&self) -> bool {
        self.frame.is_some()
    }

    /// The game's file name, which a fixed preset is filed under.
    fn game_file(&self) -> Option<String> {
        self.game.and_then(|x| x.file_name()).map(|x| x.to_string_lossy().into_owned())
    }

    /// The game's name, which a new preset is called.
    fn game_name(&self) -> Option<String> {
        self.game.and_then(|x| x.file_stem()).map(|x| x.to_string_lossy().into_owned())
    }
}

/// How the menu was left.
pub enum Outcome {
    /// Back to the game, or to the list.
    Close,
    /// "게임 끝내기".
    EndGame,
    /// "MiniMobile 종료".
    QuitApp,
    /// The game picked on the list was deleted; what to say about it.
    Deleted(String),
}

#[derive(Clone, Copy, PartialEq)]
enum Item {
    Resume,
    Keyboard,
    Layout,
    Preset,
    Fix,
    Screen,
    Speed,
    Quality,
    Saves,
    Delete,
    EndGame,
    Close,
    QuitApp,
}

/// A pad button held down keeps repeating after a moment: the directions,
/// for moving through a list. (A held keyboard key repeats by itself.)
pub(crate) struct Repeat {
    held: Option<(Button, Instant)>,
}

impl Repeat {
    pub(crate) fn new() -> Repeat {
        Repeat { held: None }
    }
}

/// The window sizes the screen setting steps through: 2x to 4x of 320x240,
/// then the full screen.
fn screen_steps() -> Vec<u32> {
    (2..=SCREEN_MAX).chain(Some(0)).collect()
}

/// The slowest and fastest a game plays, in tenths: 0.1x and 4x. The menu
/// and F5/F6 step a tenth at a time between them, and the ruler under the
/// menu's speed row spans them.
pub(crate) const SPEED_MIN: u32 = 1;
pub(crate) const SPEED_MAX: u32 = 40;

/// The speed a tenth faster (or slower) than `speed`, stopping at the ends.
pub(crate) fn speed_step(speed: f32, faster: bool) -> f32 {
    let tenths = (speed * 10.0).round() as i32 + if faster { 1 } else { -1 };
    tenths.clamp(SPEED_MIN as i32, SPEED_MAX as i32) as f32 / 10.0
}

/// `1.5x`, `2x`, `0.75x`.
pub(crate) fn speed_label(speed: f32) -> String {
    let text = format!("{speed:.2}");
    format!("{}x", text.trim_end_matches('0').trim_end_matches('.'))
}

/// How a game's screen is enlarged, numbered as the Android app keeps it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Quality {
    /// Smoothed: "기본".
    Smooth = 0,
    /// Pixel for pixel, as the screen always was.
    Dot = 1,
    /// Doubled through hq2x first, then smoothed.
    Hq2x = 2,
}

impl Quality {
    pub(crate) fn from_index(index: u8) -> Quality {
        match index {
            0 => Quality::Smooth,
            2 => Quality::Hq2x,
            _ => Quality::Dot,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Quality::Smooth => "기본",
            Quality::Dot => "도트",
            Quality::Hq2x => "HQ2X",
        }
    }

    /// What the choice does, in a line.
    fn detail(self) -> &'static str {
        match self {
            Quality::Smooth => "부드럽게 확대해요",
            Quality::Dot => "픽셀을 그대로 키워요",
            Quality::Hq2x => "계단진 테두리를 다듬어요",
        }
    }

    /// The next choice round, or the one before.
    fn step(self, forward: bool) -> Quality {
        Quality::from_index(((self as u8) + if forward { 1 } else { 2 }) % 3)
    }
}

fn screen_label(scale: u32) -> String {
    if scale == 0 {
        "전체화면".to_owned()
    } else {
        format!("창 {scale}배")
    }
}

impl App {
    /// The pad's words or the keyboard's for a hint, whichever this is.
    pub(crate) fn hint(&self, pad: &'static str, keys: &'static str) -> &'static str {
        if self.desktop { keys } else { pad }
    }

    /// The buttons pressed since the last call, a held direction repeating,
    /// and the keyboard's keys as the buttons they stand for. Closing the
    /// window ends the program.
    pub(crate) fn presses(&mut self, repeat: &mut Repeat) -> Vec<Button> {
        let mut pressed = Vec::new();
        for input in self.poll() {
            match input {
                Input::Quit => {
                    host::stop();
                    std::process::exit(0);
                }
                // A key the system repeats moves the cursor on, and does
                // nothing else again.
                Input::Key(code, true, repeated) => pressed.extend(
                    key_button(code).filter(|button| !repeated || matches!(button, Button::Up | Button::Down | Button::Left | Button::Right)),
                ),
                Input::Key(..) | Input::Drop(_) | Input::Text(_) | Input::Editing(_) => {}
                Input::Mouse(mouse) => self.mouse_presses(mouse, &mut pressed),
                Input::Button(button, true) => {
                    pressed.push(button);
                    repeat.held = matches!(button, Button::Up | Button::Down | Button::Left | Button::Right)
                        .then(|| (button, Instant::now() + Duration::from_millis(400)));
                }
                Input::Button(button, false) => {
                    if repeat.held.is_some_and(|(held, _)| held == button) {
                        repeat.held = None;
                    }
                }
            }
        }
        if let Some((button, at)) = repeat.held
            && Instant::now() >= at
        {
            pressed.push(button);
            repeat.held = Some((button, Instant::now() + Duration::from_millis(70)));
        }
        pressed
    }

    /// Whether the screen has to be drawn again for the window's sake.
    pub(crate) fn take_redraw(&mut self) -> bool {
        std::mem::take(&mut self.redraw)
    }

    pub(crate) fn present(&mut self, screen: Screen, width: u32, height: u32) {
        let rgba = screen.rgba();
        self.show(&rgba, width, height);
    }

    /// The menu, until it is closed or the game or the program is ended from
    /// it.
    pub fn settings_menu(&mut self, context: &Context) -> Outcome {
        let mut items = Vec::new();
        if context.in_game() {
            items.extend([Item::Resume, Item::Speed, Item::Quality]);
        }
        if self.desktop {
            items.push(Item::Keyboard);
        }
        items.extend([Item::Layout, Item::Preset]);
        if context.game.is_some() {
            items.push(Item::Fix);
        }
        // What is done to a game's files is done from the list, with the game
        // stopped.
        if context.game.is_some() && !context.in_game() {
            items.extend([Item::Speed, Item::Quality, Item::Saves, Item::Delete]);
        }
        if self.desktop {
            items.push(Item::Screen);
        }
        if context.in_game() {
            items.push(Item::EndGame);
        } else {
            items.extend([Item::Close, Item::QuitApp]);
        }

        let mut cursor: usize = 0;
        // The first row shown, when there are more than fit.
        let mut top: usize = 0;
        let mut repeat = Repeat::new();
        let mut dirty = true;
        let icon = context.game.and_then(icons::extract);
        loop {
            let pressed = self.presses(&mut repeat);
            if let Some((row, _)) = self.take_pointed() {
                dirty |= row != cursor;
                cursor = row.min(items.len() - 1);
            }
            if let Some(tenths) = self.take_ruler()
                && let Some(game) = context.game
            {
                self.set_game_speed(game, tenths as f32 / 10.0, context.in_game());
                dirty = true;
            }
            for button in pressed {
                dirty = true;
                match button {
                    Button::Up => cursor = cursor.checked_sub(1).unwrap_or(items.len() - 1),
                    Button::Down => cursor = (cursor + 1) % items.len(),
                    Button::B | Button::Guide => return Outcome::Close,
                    Button::Left | Button::Right if items[cursor] == Item::Quality => {
                        if let Some(game) = context.game {
                            let quality = self.game_quality(game).step(button == Button::Right);
                            self.set_game_quality(game, quality);
                        }
                    }
                    Button::Left | Button::Right if items[cursor] == Item::Preset => {
                        self.cycle_preset(button == Button::Right);
                    }
                    Button::Left | Button::Right if items[cursor] == Item::Screen => {
                        self.cycle_screen(button == Button::Right);
                    }
                    Button::Left | Button::Right if items[cursor] == Item::Speed => {
                        if let Some(game) = context.game {
                            let speed = speed_step(self.game_speed(game), button == Button::Right);
                            self.set_game_speed(game, speed, context.in_game());
                        }
                    }
                    Button::A => match items[cursor] {
                        Item::Resume | Item::Close => return Outcome::Close,
                        Item::EndGame => return Outcome::EndGame,
                        Item::QuitApp => return Outcome::QuitApp,
                        Item::Keyboard => self.keyboard_layout(),
                        Item::Layout => self.layout(),
                        Item::Preset => self.presets(context),
                        Item::Screen => self.cycle_screen(true),
                        Item::Speed => {
                            if let Some(game) = context.game {
                                // On past the fastest, back round to the slowest.
                                let current = self.game_speed(game);
                                let next = speed_step(current, true);
                                let speed = if next == current { SPEED_MIN as f32 / 10.0 } else { next };
                                self.set_game_speed(game, speed, context.in_game());
                            }
                        }
                        Item::Quality => {
                            if let Some(game) = context.game {
                                let quality = self.game_quality(game).step(true);
                                self.set_game_quality(game, quality);
                            }
                        }
                        Item::Saves => {
                            if let Some(game) = context.game {
                                self.save_manager(game);
                            }
                        }
                        Item::Delete => {
                            if let Some(game) = context.game
                                && let Some(done) = self.delete_game(game, None)
                            {
                                return Outcome::Deleted(done);
                            }
                        }
                        Item::Fix => {
                            if let Some(game) = context.game_file() {
                                let fixed = self.store.fixed(&game).map(str::to_owned);
                                let active = self.store.active().to_owned();
                                self.store.set_fixed(&game, if fixed.is_some() { None } else { Some(&active) });
                            }
                        }
                    },
                    _ => {}
                }
            }

            if dirty || self.take_redraw() {
                self.draw_menu(context, icon.as_ref(), &items, cursor, &mut top);
                dirty = false;
            }
            std::thread::sleep(FRAME);
        }
    }

    /// Loads the preset after (or before) the one in use.
    fn cycle_preset(&mut self, forward: bool) {
        let names = self.store.names();
        let current = names.iter().position(|x| x == self.store.active()).unwrap_or(0);
        let next = if forward {
            (current + 1) % names.len()
        } else {
            current.checked_sub(1).unwrap_or(names.len() - 1)
        };
        self.store.apply(&names[next]);
    }

    /// Steps the window to the next (or previous) size.
    fn cycle_screen(&mut self, forward: bool) {
        let steps = screen_steps();
        let current = steps.iter().position(|x| *x == self.store.screen()).unwrap_or(0);
        let next = if forward {
            (current + 1) % steps.len()
        } else {
            current.checked_sub(1).unwrap_or(steps.len() - 1)
        };
        self.set_screen(steps[next]);
    }

    /// The speed `game` plays at.
    pub(crate) fn game_speed(&self, game: &Path) -> f32 {
        let file = game.file_name().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
        self.store.speed(&file)
    }

    /// Remembers the speed `game` plays at, and puts the clock on it now if
    /// the game is `running`.
    pub(crate) fn set_game_speed(&mut self, game: &Path, speed: f32, running: bool) {
        let file = game.file_name().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
        self.store.set_speed(&file, speed);
        if running {
            host::set_speed(speed);
        }
    }

    /// How `game`'s screen is enlarged.
    pub(crate) fn game_quality(&self, game: &Path) -> Quality {
        Quality::from_index(self.store.quality(&file_of(game)))
    }

    /// Remembers how `game`'s screen is enlarged. A game running takes it up
    /// with its next frame, and the frame on screen is drawn again for it.
    fn set_game_quality(&mut self, game: &Path, quality: Quality) {
        self.store.set_quality(&file_of(game), quality as u8);
        host::show_frame_again();
    }

    /// The active preset's name, starred once the mapping has moved off it.
    fn active_label(&self) -> String {
        let star = if self.store.modified() { "*" } else { "" };
        format!("{}{star}", self.store.active())
    }

    /// What a row of the menu is called and the mark beside it.
    fn item_face(&self, item: Item) -> (&'static str, &'static str) {
        match item {
            Item::Resume => ("▶", "게임으로 돌아가기"),
            Item::Keyboard => ("⇥", "키보드 배치 바꾸기"),
            Item::Layout if self.desktop => ("●", "패드 버튼 배치 바꾸기"),
            Item::Layout => ("●", "버튼 배치 바꾸기"),
            Item::Preset => ("★", "프리셋"),
            Item::Fix => ("✓", "이 게임에 프리셋 고정"),
            Item::Screen => ("▢", "화면"),
            Item::Speed => ("»", "배속"),
            Item::Quality => ("◇", "화질"),
            Item::Saves => ("♡", "세이브 관리"),
            Item::Delete => ("×", "이 게임 삭제"),
            Item::EndGame => ("⏏", "게임 끝내기"),
            Item::Close => ("←", "닫기"),
            Item::QuitApp => ("⏏", "MiniMobile 종료"),
        }
    }

    /// What a row of the menu is set to, if it is set to anything.
    fn item_value(&self, context: &Context, item: Item) -> String {
        match item {
            Item::Preset => self.active_label(),
            Item::Screen => screen_label(self.store.screen()),
            Item::Speed => speed_label(context.game.map_or(1.0, |game| self.game_speed(game))),
            Item::Quality => context.game.map_or(Quality::Dot, |game| self.game_quality(game)).label().to_owned(),
            Item::Fix => {
                let fixed = context.game_file().and_then(|game| self.store.fixed(&game).map(str::to_owned));
                fixed.unwrap_or_else(|| "끔".to_owned())
            }
            _ => String::new(),
        }
    }

    /// The menu: a drawer down the left, over the game when one is running,
    /// with what the row picked sets shown large beside it.
    fn draw_menu(&mut self, context: &Context, icon: Option<&Picture>, items: &[Item], cursor: usize, top: &mut usize) {
        let (width, height, _) = self.menu_size();
        let mut screen = Screen::new(width, height);
        self.clear_hits();
        let (w, h) = (width as f32, height as f32);
        let side = (w * 0.47).clamp(150.0, 220.0);

        // The game behind, dimmed, and the drawer over its left.
        if let Some((frame_width, frame_height, rgba)) = context.frame {
            screen.backdrop(rgba, frame_width, frame_height);
            screen.fill(0, 0, width, height, Color { a: 0x60, r: 0, g: 0, b: 0 });
        } else {
            screen.gradient(rgb(0x1e, 0x26, 0x2d), rgb(0x0a, 0x0c, 0x10));
        }
        screen.fill(
            0,
            0,
            side as u32,
            height,
            Color {
                a: 0xf6,
                r: 0x10,
                g: 0x14,
                b: 0x1a,
            },
        );
        screen.fill(side as i32, 0, 1, height, EDGE);

        let name = context.game_name().unwrap_or_else(|| "MiniMobile".to_owned());
        let note = if context.in_game() { "일시정지됨" } else { "설정" };
        let text_x = match icon {
            Some(icon) => {
                screen.picture(icon, 10.0, 7.0, 22.0, 22.0, 5.0, 1.0);
                38.0
            }
            None => 12.0,
        };
        screen.text_styled(
            &fit_with(&name, side - text_x - 8.0, SMALL),
            text_x,
            6.0,
            SMALL,
            TextAlignment::Left,
            TEXT,
        );
        screen.text_styled(note, text_x, 19.0, TINY, TextAlignment::Left, MUTED);

        // As many rows as the drawer has room for, scrolled to the cursor.
        let row_height = 22.0;
        let list_top = 36.0;
        let rows = (((h - list_top - BAR as f32 - 4.0) / row_height) as usize).clamp(1, items.len());
        scroll(cursor, rows, top);
        for (index, item) in items.iter().enumerate().skip(*top).take(rows) {
            let y = list_top + (index - *top) as f32 * row_height;
            let picked = index == cursor;
            let (mark, label) = self.item_face(*item);
            let value = self.item_value(context, *item);
            let steps = matches!(item, Item::Preset | Item::Screen | Item::Speed | Item::Quality);
            if picked {
                screen.round(6.0, y, side - 12.0, row_height - 2.0, 6.0, WHITE);
            }
            let danger = matches!(item, Item::Delete | Item::QuitApp | Item::EndGame);
            let ink = if picked {
                INK
            } else if danger {
                DANGER
            } else {
                rgb(0xd9, 0xdf, 0xe3)
            };
            let mark_color = if picked { GREEN } else { rgb(0x7f, 0x8b, 0x93) };
            screen.text_styled(mark, 17.0, y + 3.0, SMALL, TextAlignment::Center, mark_color);
            let value_width = if value.is_empty() {
                0.0
            } else {
                ui::text_width(&value, SMALL).min(side * 0.4) + if picked && steps { 24.0 } else { 6.0 }
            };
            screen.text_styled(&fit(label, side - 40.0 - value_width), 28.0, y + 2.0, BODY, TextAlignment::Left, ink);
            self.hit(0, y as i32, side as i32, row_height as i32, Target::Row(index));
            if !value.is_empty() {
                let right = side - 14.0;
                let value = fit_with(&value, side * 0.4, SMALL);
                if picked && steps {
                    // ‹ and › step the value, as the pad's ◀ and ▶ do.
                    screen.text_styled("›", right, y + 3.0, SMALL, TextAlignment::Right, MUTED);
                    let value_right = right - 10.0;
                    let shown = screen.text_styled(&value, value_right, y + 3.0, SMALL, TextAlignment::Right, GREEN);
                    screen.text_styled("‹", value_right - shown - 4.0, y + 3.0, SMALL, TextAlignment::Right, MUTED);
                    let left = (value_right - shown - 12.0) as i32;
                    self.hit(left - 4, y as i32, 14, row_height as i32, Target::Step(index, false));
                    self.hit(right as i32 - 10, y as i32, 18, row_height as i32, Target::Step(index, true));
                } else {
                    screen.text_styled(&value, right, y + 3.0, SMALL, TextAlignment::Right, if picked { GREEN } else { ACCENT });
                }
            }
        }
        // More rows above or below than are shown.
        if *top > 0 {
            screen.text_styled("▲", side - 12.0, list_top - 9.0, TINY, TextAlignment::Center, MUTED);
        }
        if *top + rows < items.len() {
            screen.text_styled(
                "▼",
                side - 12.0,
                list_top + rows as f32 * row_height - 3.0,
                TINY,
                TextAlignment::Center,
                MUTED,
            );
        }

        // What the row picked sets, large, beside the drawer.
        let card_x = side + 14.0;
        let card_width = w - side - 26.0;
        if card_width >= 100.0 {
            match items[cursor] {
                Item::Speed => {
                    if let Some(game) = context.game {
                        let row = items.iter().position(|item| *item == Item::Speed).unwrap_or(0);
                        self.draw_speed_card(&mut screen, card_x, (h - 62.0) / 2.0 - 10.0, card_width, self.game_speed(game), row);
                    }
                }
                Item::Quality => {
                    let quality = context.game.map_or(Quality::Dot, |game| self.game_quality(game));
                    let choices = [Quality::Smooth, Quality::Dot, Quality::Hq2x];
                    let picked = choices.iter().position(|x| *x == quality).unwrap_or(1);
                    draw_choice_card(
                        &mut screen,
                        card_x,
                        (h - 62.0) / 2.0 - 10.0,
                        card_width,
                        "화질",
                        &["기본", "도트", "HQ2X"],
                        picked,
                        quality.detail(),
                    );
                }
                Item::Screen => {
                    let steps = screen_steps();
                    let labels: Vec<String> = steps
                        .iter()
                        .map(|scale| if *scale == 0 { "전체".to_owned() } else { format!("{scale}배") })
                        .collect();
                    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
                    let picked = steps.iter().position(|x| *x == self.store.screen()).unwrap_or(0);
                    draw_choice_card(
                        &mut screen,
                        card_x,
                        (h - 62.0) / 2.0 - 10.0,
                        card_width,
                        "화면",
                        &labels,
                        picked,
                        "창 크기 · F11 전체화면",
                    );
                }
                _ => {}
            }
        }

        let hint = match items[cursor] {
            Item::Preset => self.hint("A 목록  ◀▶ 바꾸기", "Enter 목록  ◀▶ 바꾸기"),
            Item::Screen => self.hint("◀▶ 바꾸기", "◀▶ 바꾸기  F11"),
            Item::Speed if context.in_game() => self.hint("◀▶ 0.1씩", "◀▶ 0.1씩  F5 F6"),
            Item::Speed => self.hint("◀▶ 0.1씩", "◀▶ 0.1씩"),
            Item::Quality => self.hint("◀▶ 바꾸기", "◀▶ 바꾸기"),
            _ => self.hint("A 선택", "Enter 선택"),
        };
        let back = self.hint("B 닫기", "Esc 닫기");
        screen.bar(height as i32 - BAR, hint, back);
        self.back_hit(width, height);
        self.present(screen, width, height);
    }

    /// The speed in a white card: the number large, and the ruler under it -
    /// a tick for every tenth from 0.1x to 4x, taller at the halves and
    /// tallest, numbered, at the whole speeds. Clicked or dragged, the ruler
    /// sets the speed of the menu's row `row`.
    fn draw_speed_card(&self, screen: &mut Screen, x: f32, y: f32, width: f32, speed: f32, row: usize) {
        screen.round(x, y + 1.5, width, 62.0, 8.0, Color { a: 0x50, r: 0, g: 0, b: 0 });
        screen.round(x, y, width, 62.0, 8.0, WHITE);
        screen.text_styled("배속", x + 10.0, y + 6.0, SMALL, TextAlignment::Left, INK);
        let big = ui::Style {
            size: 17.0,
            line: 18.0,
            bold: true,
        };
        screen.text_styled(&speed_label(speed), x + width - 10.0, y + 4.0, big, TextAlignment::Right, GREEN);

        let (left, right) = (x + 12.0, x + width - 12.0);
        let span = (SPEED_MAX - SPEED_MIN) as f32;
        let at = |tenths: u32| left + (tenths - SPEED_MIN) as f32 * (right - left) / span;
        let base = y + 42.0;
        for tenths in SPEED_MIN..=SPEED_MAX {
            let (length, color) = if tenths % 10 == 0 {
                (8.0, rgb(0x3b, 0x46, 0x50))
            } else if tenths % 5 == 0 {
                (5.0, rgb(0x9a, 0xa5, 0xad))
            } else {
                (3.0, rgb(0xc4, 0xcc, 0xd2))
            };
            screen.round(at(tenths) - 0.35, base - length, 0.7, length, 0.0, color);
            if tenths % 10 == 0 {
                screen.text_styled(
                    &format!("{}x", tenths / 10),
                    at(tenths),
                    base + 2.0,
                    TINY,
                    TextAlignment::Center,
                    rgb(0x64, 0x75, 0x68),
                );
            }
        }
        let current = ((speed * 10.0).round() as u32).clamp(SPEED_MIN, SPEED_MAX);
        screen.round(at(current) - 0.8, base - 14.0, 1.6, 15.0, 0.8, GREEN);
        screen.round(at(current) - 3.5, base - 17.0, 7.0, 4.0, 2.0, GREEN);
        self.hit(
            left as i32 - 6,
            (base - 20.0) as i32,
            (right - left) as i32 + 12,
            34,
            Target::Ruler(row, left as i32, (right - left) as i32),
        );
    }

    /// The screen's bottom bar goes back when its right half is clicked, as
    /// its right end says.
    pub(crate) fn back_hit(&self, width: u32, height: u32) {
        self.hit(width as i32 / 2, height as i32 - BAR, width as i32 / 2, BAR, Target::Press(Button::B));
    }

    /// A panel's bottom bar, which goes back - as `right` says - when its right
    /// half is clicked.
    pub(crate) fn back_bar(&self, screen: &mut Screen, x: i32, y: i32, width: u32, left: &str, right: &str) {
        screen.fill(x + 8, y, width - 16, 1, EDGE);
        screen.hints(x, y, width, left, right);
        if !right.is_empty() {
            self.hit(x + width as i32 / 2, y, width as i32 / 2, BAR, Target::Press(Button::B));
        }
    }

    /// The table of every pad button, plain and with SELECT held.
    fn layout(&mut self) {
        let mut row: usize = 0;
        let mut with_select = false;
        let mut top = 0;
        let mut repeat = Repeat::new();
        let mut dirty = true;
        loop {
            let pressed = self.presses(&mut repeat);
            if let Some((pointed, column)) = self.take_pointed() {
                row = pointed.min(TABLE_BUTTONS.len() - 1);
                with_select = column.map_or(with_select, |column| column == 1);
                dirty = true;
            }
            for button in pressed {
                dirty = true;
                match button {
                    Button::Up => row = row.checked_sub(1).unwrap_or(TABLE_BUTTONS.len() - 1),
                    Button::Down => row = (row + 1) % TABLE_BUTTONS.len(),
                    Button::Left | Button::Right => with_select = !with_select,
                    Button::B | Button::Guide => return,
                    Button::A => {
                        let (target, name) = TABLE_BUTTONS[row];
                        let current = self.store.controls().get(target, with_select);
                        let title = format!("{name} 버튼 ({})", if with_select { "SELECT+" } else { "그냥" });
                        if let Some(key) = self.pick_key(&title, current, row, with_select, top) {
                            self.store.set(target, with_select, key);
                        }
                    }
                    _ => {}
                }
            }

            if dirty || self.take_redraw() {
                let (screen, width, height) = self.draw_layout(row, with_select, &mut top);
                self.present(screen, width, height);
                dirty = false;
            }
            std::thread::sleep(FRAME);
        }
    }

    fn draw_layout(&self, row: usize, with_select: bool, top: &mut usize) -> (Screen, u32, u32) {
        let (width, height, _) = self.menu_size();
        let mut screen = Screen::new(width, height);
        self.clear_hits();
        let preset = format!("프리셋: {}{}", self.store.active(), if self.store.modified() { " (바뀜)" } else { "" });
        let title = if self.desktop { "패드 버튼 배치" } else { "버튼 배치" };
        screen.bar(0, title, &fit(&preset, width as f32 - 136.0));

        let (c1, c2, c3) = (8, (width * 29 / 100) as i32, (width * 64 / 100) as i32);
        screen.text("버튼", c1, BAR + 2, TextAlignment::Left, MUTED);
        screen.text("그냥", c2, BAR + 2, TextAlignment::Left, MUTED);
        screen.text("SELECT+", c3, BAR + 2, TextAlignment::Left, MUTED);
        screen.fill(0, BAR + LINE + 1, width, 1, EDGE);

        let list_top = BAR + LINE + 4;
        let rows = ((height as i32 - list_top - BAR - 2) / LINE).max(1) as usize;
        scroll(row, rows, top);

        let controls = self.store.controls();
        for (index, (button, name)) in TABLE_BUTTONS.iter().enumerate().skip(*top).take(rows) {
            let y = list_top + (index - *top) as i32 * LINE;
            let picked = index == row;
            if picked {
                screen.fill(0, y, width, LINE as u32, ROW);
                let (x, w) = if with_select {
                    (c3 - 4, width as i32 - c3 + 4)
                } else {
                    (c2 - 4, c3 - c2 - 4)
                };
                screen.fill(x, y, w as u32, LINE as u32, HIGHLIGHT);
            }
            screen.text(name, c1, y + 1, TextAlignment::Left, TEXT);
            // The name and the plain key are the first column, SELECT+ the second.
            self.hit(0, y, c3 - 4, LINE, Target::Cell(index, 0));
            self.hit(c3 - 4, y, width as i32 - c3 + 4, LINE, Target::Cell(index, 1));
            let plain = controls.get(*button, false);
            screen.text(
                key_label(plain),
                c2,
                y + 1,
                TextAlignment::Left,
                if picked && !with_select { TEXT } else { ACCENT },
            );
            // A button with nothing under SELECT keeps its own key there, shown
            // muted.
            match controls.get(*button, true) {
                Some(key) => screen.text(
                    key_label(Some(key)),
                    c3,
                    y + 1,
                    TextAlignment::Left,
                    if picked && with_select { TEXT } else { ACCENT },
                ),
                None => screen.text(
                    key_label(plain),
                    c3,
                    y + 1,
                    TextAlignment::Left,
                    if picked && with_select { TEXT } else { MUTED },
                ),
            }
        }
        scroll_marks(&mut screen, width, list_top, rows, *top, TABLE_BUTTONS.len());

        let hint = self.hint("A 바꾸기  ◀▶ 칸", "Enter 바꾸기  ◀▶ 칸");
        screen.bar(height as i32 - BAR, hint, self.hint("B 뒤로", "Esc 뒤로"));
        self.back_hit(width, height);
        (screen, width, height)
    }

    /// Picks a handset key for one cell of the pad table, over it. `None` when
    /// the pick is cancelled, `Some(None)` for no key.
    fn pick_key(&mut self, title: &str, current: Option<i32>, row: usize, with_select: bool, top: usize) -> Option<Option<i32>> {
        let cells = picker_cells();
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
                    Button::Up | Button::Down | Button::Left | Button::Right => cursor = step(&cells, cursor, button),
                    Button::A => return Some(cells[cursor].key),
                    Button::B | Button::Guide => return None,
                    _ => {}
                }
            }

            if dirty || self.take_redraw() {
                let mut top = top;
                let (mut screen, width, height) = self.draw_layout(row, with_select, &mut top);
                screen.fill(0, 0, width, height, DIM);
                // Only the picker answers the mouse while it is up.
                self.clear_hits();

                let (panel_width, panel_height) = (272u32, 196u32);
                let x = (width - panel_width) as i32 / 2;
                let y = (height - panel_height) as i32 / 2;
                screen.panel(x, y, panel_width, panel_height);
                panel_bar(&mut screen, x, y, panel_width, title, &format!("지금: {}", key_label(current)));
                for (index, cell) in cells.iter().enumerate() {
                    let fill = if index == cursor { HIGHLIGHT } else { ROW };
                    screen.round((x + cell.x) as f32, (y + cell.y) as f32, cell.width as f32, cell.height as f32, 4.0, fill);
                    self.hit(x + cell.x, y + cell.y, cell.width, cell.height, Target::Row(index));
                    let color = if cell.key.is_none() && index != cursor { MUTED } else { TEXT };
                    let label = if cell.key.is_none() { "없음" } else { key_label(cell.key) };
                    screen.text(
                        label,
                        x + cell.x + cell.width / 2,
                        y + cell.y + (cell.height - 16) / 2,
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

    /// The keyboard table: each handset key and the keyboard keys for it.
    fn keyboard_layout(&mut self) {
        let mut row: usize = 0;
        let mut slot: usize = 0;
        let mut top = 0;
        let mut status = String::new();
        let mut repeat = Repeat::new();
        let mut dirty = true;
        loop {
            let pressed = self.presses(&mut repeat);
            if let Some((pointed, column)) = self.take_pointed() {
                row = pointed.min(TABLE_KEYS.len() - 1);
                slot = column.unwrap_or(slot).min(KEYS_PER_KEY - 1);
                dirty = true;
            }
            for button in pressed {
                dirty = true;
                status.clear();
                let handset = TABLE_KEYS[row];
                match button {
                    Button::Up => row = row.checked_sub(1).unwrap_or(TABLE_KEYS.len() - 1),
                    Button::Down => row = (row + 1) % TABLE_KEYS.len(),
                    Button::Left => slot = slot.checked_sub(1).unwrap_or(KEYS_PER_KEY - 1),
                    Button::Right => slot = (slot + 1) % KEYS_PER_KEY,
                    Button::B | Button::Guide => return,
                    // Delete, or Y on a pad: empty the cell.
                    Button::Y => {
                        self.store.set_keyboard(handset, slot, None);
                    }
                    Button::A => {
                        if let Some(code) = self.capture_key(handset, slot, row, top)
                            && let Some(from) = self.store.set_keyboard(handset, slot, code)
                        {
                            status = format!("{}: '{}'에서 옮겨 왔습니다.", scancode_label(code), key_label(Some(from)));
                        }
                    }
                    _ => {}
                }
            }

            if dirty || self.take_redraw() {
                let (screen, width, height) = self.draw_keyboard(row, slot, &mut top, &status);
                self.present(screen, width, height);
                dirty = false;
            }
            std::thread::sleep(FRAME);
        }
    }

    fn draw_keyboard(&self, row: usize, slot: usize, top: &mut usize, status: &str) -> (Screen, u32, u32) {
        let (width, height, _) = self.menu_size();
        let mut screen = Screen::new(width, height);
        self.clear_hits();
        let preset = format!("프리셋: {}{}", self.store.active(), if self.store.modified() { " (바뀜)" } else { "" });
        screen.bar(0, "키보드 배치", &fit(&preset, width as f32 - 112.0));

        let columns = [(width * 31 / 100) as i32, (width * 66 / 100) as i32];
        screen.text("폰 키", 8, BAR + 2, TextAlignment::Left, MUTED);
        screen.text("키 1", columns[0], BAR + 2, TextAlignment::Left, MUTED);
        screen.text("키 2", columns[1], BAR + 2, TextAlignment::Left, MUTED);
        screen.fill(0, BAR + LINE + 1, width, 1, EDGE);

        let list_top = BAR + LINE + 4;
        let bottom = height as i32 - if status.is_empty() { BAR } else { 2 * BAR };
        let rows = ((bottom - list_top - 2) / LINE).max(1) as usize;
        scroll(row, rows, top);

        let controls = self.store.controls();
        for (index, handset) in TABLE_KEYS.iter().enumerate().skip(*top).take(rows) {
            let y = list_top + (index - *top) as i32 * LINE;
            let picked = index == row;
            if picked {
                screen.fill(0, y, width, LINE as u32, ROW);
                let x = columns[slot] - 4;
                let w = if slot + 1 < KEYS_PER_KEY {
                    columns[slot + 1] - columns[slot] - 4
                } else {
                    width as i32 - x
                };
                screen.fill(x, y, w as u32, LINE as u32, HIGHLIGHT);
            }
            screen.text(key_label(Some(*handset)), 8, y + 1, TextAlignment::Left, TEXT);
            // The handset key's name goes with the first key.
            self.hit(0, y, columns[1] - 4, LINE, Target::Cell(index, 0));
            self.hit(columns[1] - 4, y, width as i32 - columns[1] + 4, LINE, Target::Cell(index, 1));
            for (column, x) in columns.iter().enumerate() {
                let code = controls.keyboard(*handset, column);
                let color = if picked && column == slot {
                    TEXT
                } else if code.is_some() {
                    ACCENT
                } else {
                    MUTED
                };
                let room = if column + 1 < KEYS_PER_KEY {
                    columns[column + 1] - x - 8
                } else {
                    width as i32 - x - 16
                };
                screen.text(&fit(&scancode_label(code), room as f32), *x, y + 1, TextAlignment::Left, color);
            }
        }
        scroll_marks(&mut screen, width, list_top, rows, *top, TABLE_KEYS.len());

        if !status.is_empty() {
            screen.fill(0, height as i32 - 2 * BAR, width, BAR as u32, PANEL);
            screen.text(
                &fit(status, width as f32 - 16.0),
                8,
                height as i32 - 2 * BAR + 2,
                TextAlignment::Left,
                ACCENT,
            );
        }
        let hint = self.hint("A 바꾸기  Y 지우기", "Enter 바꾸기  Del 지우기");
        screen.bar(height as i32 - BAR, hint, self.hint("B 뒤로", "Esc 뒤로"));
        self.back_hit(width, height);
        (screen, width, height)
    }

    /// Waits for the keyboard key to put in one cell of the keyboard table.
    /// `None` when cancelled (Esc, or B on a pad), `Some(None)` to empty the
    /// cell (Delete, or Y on a pad).
    fn capture_key(&mut self, handset: i32, slot: usize, row: usize, top: usize) -> Option<Option<i32>> {
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
                let mut top = top;
                let (mut screen, width, height) = self.draw_keyboard(row, slot, &mut top, "");
                screen.fill(0, 0, width, height, DIM);
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

    /// The presets, loaded, saved, overwritten and deleted.
    fn presets(&mut self, context: &Context) {
        let mut cursor = self.store.names().iter().position(|x| x == self.store.active()).unwrap_or(0);
        let mut top = 0;
        let mut status = String::new();
        // A delete waits for a second press of Y on the same preset.
        let mut confirm: Option<String> = None;
        let mut repeat = Repeat::new();
        let mut dirty = true;
        loop {
            let names = self.store.names();
            // The last row saves the mapping as a new preset.
            let count = names.len() + 1;
            cursor = cursor.min(count - 1);
            let pressed = self.presses(&mut repeat);
            if let Some((pointed, _)) = self.take_pointed() {
                cursor = pointed.min(count - 1);
                dirty = true;
            }
            let name = names.get(cursor).cloned();

            for button in pressed {
                dirty = true;
                if button != Button::Y {
                    confirm = None;
                }
                match button {
                    Button::Up => cursor = cursor.checked_sub(1).unwrap_or(count - 1),
                    Button::Down => cursor = (cursor + 1) % count,
                    Button::B | Button::Guide => return,
                    Button::A => match &name {
                        Some(name) => {
                            self.store.apply(name);
                            status = format!("불러왔습니다: {name}");
                        }
                        None => {
                            let saved = self.store.save_new(&context.game_name().unwrap_or_else(|| "프리셋".to_owned()));
                            cursor = self.store.names().iter().position(|x| *x == saved).unwrap_or(0);
                            status = format!("저장했습니다: {saved}");
                        }
                    },
                    Button::X => match &name {
                        Some(name) if name != DEFAULT_NAME => {
                            self.store.overwrite(name);
                            status = format!("덮어썼습니다: {name}");
                        }
                        Some(_) => status = "기본 프리셋은 바꿀 수 없습니다.".to_owned(),
                        None => {}
                    },
                    Button::Y => match &name {
                        Some(name) if name != DEFAULT_NAME => {
                            if confirm.as_ref() == Some(name) {
                                self.store.delete(name);
                                status = format!("지웠습니다: {name}");
                                confirm = None;
                            } else {
                                let again = self.hint("Y", "Del");
                                status = format!("{again}를 한 번 더 누르면 지웁니다: {name}");
                                confirm = Some(name.clone());
                            }
                        }
                        Some(_) => status = "기본 프리셋은 지울 수 없습니다.".to_owned(),
                        None => {}
                    },
                    _ => {}
                }
            }

            if dirty || self.take_redraw() {
                self.draw_presets(context, cursor, &mut top, &status);
                dirty = false;
            }
            std::thread::sleep(FRAME);
        }
    }

    fn draw_presets(&mut self, context: &Context, cursor: usize, top: &mut usize, status: &str) {
        let (width, height, _) = self.menu_size();
        let mut screen = Screen::new(width, height);
        self.clear_hits();
        let names = self.store.names();
        let count = names.len() + 1;
        screen.bar(0, "프리셋", &format!("{}/{}", cursor.min(names.len() - 1) + 1, names.len()));

        let list_top = BAR + 4;
        let rows = ((height as i32 - list_top - 2 * BAR - 4) / LINE).max(1) as usize;
        scroll(cursor, rows, top);

        let fixed = context.game_file().and_then(|game| self.store.fixed(&game).map(str::to_owned));
        for index in (*top..count).take(rows) {
            let y = list_top + (index - *top) as i32 * LINE;
            let picked = index == cursor;
            if picked {
                screen.fill(0, y, width, LINE as u32, HIGHLIGHT);
            }
            self.hit(0, y, width as i32, LINE, Target::Row(index));
            let (label, notes) = match names.get(index) {
                Some(name) => {
                    let mut notes = Vec::new();
                    if name == self.store.active() {
                        notes.push(if self.store.modified() { "사용 중*" } else { "사용 중" });
                    }
                    if fixed.as_deref() == Some(name.as_str()) {
                        notes.push("이 게임");
                    }
                    (name.clone(), notes.join(" · "))
                }
                None => ("+ 지금 배치를 새 프리셋으로".to_owned(), String::new()),
            };
            let room = width as f32 - 16.0 - if notes.is_empty() { 0.0 } else { 104.0 };
            screen.text(&fit(&label, room), 8, y + 1, TextAlignment::Left, if picked { TEXT } else { MUTED });
            if !notes.is_empty() {
                screen.text(&notes, width as i32 - 8, y + 1, TextAlignment::Right, if picked { TEXT } else { ACCENT });
            }
        }

        // On a desktop the keyboard's half of a preset is easy to forget.
        let shown = (count - *top).min(rows) as i32;
        if self.desktop && list_top + (shown + 1) * LINE + 2 * LINE < height as i32 - 2 * BAR {
            screen.paragraph(
                "프리셋에는 키보드와 패드 배치가\n함께 저장됩니다.",
                list_top + (shown + 1) * LINE + 6,
                MUTED,
            );
        }

        if !status.is_empty() {
            screen.text(&fit(status, width as f32 - 16.0), 8, height as i32 - 2 * BAR, TextAlignment::Left, ACCENT);
        }
        let hint = if cursor >= names.len() {
            self.hint("A 저장", "Enter 저장")
        } else {
            self.hint("A 불러오기 X 덮어쓰기 Y 지우기", "Enter 불러오기 F2 덮어쓰기 Del 지우기")
        };
        screen.bar(height as i32 - BAR, hint, if self.desktop { "" } else { "B 뒤로" });
        self.present(screen, width, height);
    }
}

/// Moves the first row shown so `row` is on screen.
pub(crate) fn scroll(row: usize, rows: usize, top: &mut usize) {
    if row < *top {
        *top = row;
    } else if row >= *top + rows {
        *top = row + 1 - rows;
    }
}

/// The ▲ and ▼ at the right edge of a table with more rows above or below.
pub(crate) fn scroll_marks(screen: &mut Screen, width: u32, list_top: i32, rows: usize, top: usize, count: usize) {
    if top + rows < count {
        screen.text(
            "▼",
            width as i32 - 8,
            list_top + (rows as i32 - 1) * LINE + 1,
            TextAlignment::Right,
            MUTED,
        );
    }
    if top > 0 {
        screen.text("▲", width as i32 - 8, list_top + 1, TextAlignment::Right, MUTED);
    }
}

/// A panel's title along its top: `left` in bold, `right` muted, a hairline
/// under them.
pub(crate) fn panel_bar(screen: &mut Screen, x: i32, y: i32, width: u32, left: &str, right: &str) {
    let right_width = if right.is_empty() { 0.0 } else { ui::text_width(right, SMALL) + 12.0 };
    let left = fit_with(left, width as f32 - 16.0 - right_width, ui::TITLE);
    screen.text_styled(&left, x as f32 + 10.0, y as f32 + 2.0, ui::TITLE, TextAlignment::Left, TEXT);
    if !right.is_empty() {
        screen.text_styled(
            right,
            (x + width as i32) as f32 - 10.0,
            y as f32 + 4.5,
            SMALL,
            TextAlignment::Right,
            MUTED,
        );
    }
    screen.fill(x + 8, y + BAR, width - 16, 1, EDGE);
}

/// A card of choices in a row, the one in use picked out, with a line on it
/// under them.
#[allow(clippy::too_many_arguments)]
fn draw_choice_card(screen: &mut Screen, x: f32, y: f32, width: f32, title: &str, choices: &[&str], picked: usize, note: &str) {
    screen.round(x, y + 1.5, width, 62.0, 8.0, Color { a: 0x50, r: 0, g: 0, b: 0 });
    screen.round(x, y, width, 62.0, 8.0, WHITE);
    screen.text_styled(title, x + 10.0, y + 6.0, SMALL, TextAlignment::Left, INK);
    let (left, right) = (x + 8.0, x + width - 8.0);
    screen.round(left, y + 23.0, right - left, 17.0, 8.5, rgb(0xee, 0xf3, 0xef));
    let cell = (right - left - 4.0) / choices.len() as f32;
    for (index, choice) in choices.iter().enumerate() {
        let cell_x = left + 2.0 + index as f32 * cell;
        if index == picked {
            screen.round(cell_x, y + 25.0, cell, 13.0, 6.5, GREEN);
        }
        let ink = if index == picked { WHITE } else { rgb(0x64, 0x75, 0x68) };
        screen.text_styled(choice, cell_x + cell / 2.0, y + 25.5, TINY_BOLD, TextAlignment::Center, ink);
    }
    screen.text_styled(note, x + width / 2.0, y + 45.0, TINY, TextAlignment::Center, rgb(0x64, 0x75, 0x68));
}

/// One key in the picker, placed inside its panel.
struct Cell {
    key: Option<i32>,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

/// The picker's keys: a handset's number pad on the left, the other keys in
/// two columns on the right, and "none" under them.
fn picker_cells() -> Vec<Cell> {
    let mut cells = Vec::new();
    // 1 2 3 / 4 5 6 / 7 8 9 / * 0 #, by key index.
    let pad = [9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 8, 19];
    for (index, key) in pad.into_iter().enumerate() {
        cells.push(Cell {
            key: Some(key),
            x: 10 + (index % 3) as i32 * 38,
            y: BAR + 8 + (index / 3) as i32 * 28,
            width: 34,
            height: 24,
        });
    }
    // ▲ ▼ / ◀ ▶ / 확인 취소 / 좌소프트 우소프트 / 통화 종료.
    let keys = [0, 1, 2, 3, 4, 7, 5, 6, 20, 21];
    for (index, key) in keys.into_iter().enumerate() {
        cells.push(Cell {
            key: Some(key),
            x: 130 + (index % 2) as i32 * 68,
            y: BAR + 8 + (index / 2) as i32 * 23,
            width: 64,
            height: 20,
        });
    }
    cells.push(Cell {
        key: None,
        x: 130,
        y: BAR + 8 + 5 * 23,
        width: 132,
        height: 20,
    });
    cells
}

/// The cell a direction leads to from `from`: of the cells wholly past its
/// edge that way, the nearest, distance across the direction counting double;
/// `from` itself at an edge.
fn step(cells: &[Cell], from: usize, direction: Button) -> usize {
    let centre = |cell: &Cell| (cell.x + cell.width / 2, cell.y + cell.height / 2);
    let origin = &cells[from];
    let (fx, fy) = centre(origin);
    cells
        .iter()
        .enumerate()
        .filter(|(_, cell)| match direction {
            Button::Up => cell.y + cell.height <= origin.y,
            Button::Down => cell.y >= origin.y + origin.height,
            Button::Left => cell.x + cell.width <= origin.x,
            _ => cell.x >= origin.x + origin.width,
        })
        .map(|(index, cell)| {
            let (cx, cy) = centre(cell);
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

    fn at(cells: &[Cell], key: Option<i32>) -> usize {
        cells.iter().position(|cell| cell.key == key).unwrap()
    }

    #[test]
    fn the_picker_moves_like_a_keypad() {
        let cells = picker_cells();
        // 1 → 2 → 3 along the top row, 5 down to 8, and from 3 across to ▲.
        assert_eq!(step(&cells, at(&cells, Some(9)), Button::Right), at(&cells, Some(10)));
        assert_eq!(step(&cells, at(&cells, Some(13)), Button::Down), at(&cells, Some(16)));
        assert_eq!(step(&cells, at(&cells, Some(11)), Button::Right), at(&cells, Some(0)));
        // The bottom of the right column reaches "none", and nothing is below it.
        let none = at(&cells, None);
        assert_eq!(step(&cells, at(&cells, Some(20)), Button::Down), none);
        assert_eq!(step(&cells, none, Button::Down), none);
        // The top-left key stays put going up or left.
        assert_eq!(step(&cells, at(&cells, Some(9)), Button::Up), at(&cells, Some(9)));
        assert_eq!(step(&cells, at(&cells, Some(9)), Button::Left), at(&cells, Some(9)));
    }

    #[test]
    fn the_speed_steps_a_tenth_and_stops_at_the_ends() {
        assert_eq!(speed_step(1.0, true), 1.1);
        assert_eq!(speed_step(1.0, false), 0.9);
        assert_eq!(speed_step(4.0, true), 4.0);
        assert_eq!(speed_step(0.1, false), 0.1);
        // A speed kept from before the tenths steps from the nearest tenth.
        assert_eq!(speed_step(0.75, true), 0.9);
        assert_eq!(speed_label(1.0), "1x");
        assert_eq!(speed_label(1.5), "1.5x");
        assert_eq!(speed_label(0.3), "0.3x");
        assert_eq!(speed_label(0.75), "0.75x");
    }

    #[test]
    fn the_quality_steps_round_its_three_choices() {
        assert_eq!(Quality::Dot.step(true), Quality::Hq2x);
        assert_eq!(Quality::Hq2x.step(true), Quality::Smooth);
        assert_eq!(Quality::Smooth.step(false), Quality::Hq2x);
        assert_eq!(Quality::from_index(9), Quality::Dot);
    }

    #[test]
    fn the_screen_steps_through_window_sizes_then_full_screen() {
        assert_eq!(screen_steps(), vec![2, 3, 4, 0]);
        assert_eq!(screen_label(0), "전체화면");
        assert_eq!(screen_label(3), "창 3배");
    }
}
