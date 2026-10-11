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

pub(crate) use crate::ui::{ACCENT, DANGER, EDGE};
use crate::{
    App, FRAME, Input,
    controls::Button,
    icons, key_button,
    pointer::Target,
    presets::{DEFAULT_NAME, SCREEN_MAX},
    ui::{self, BAR, GREEN, HIGHLIGHT, INK, LINE, MUTED, Picture, SMALL, Screen, TEXT, TINY, TINY_BOLD, WHITE, fit, fit_with, rgb},
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

/// How many tiles the menu has to a row.
const TILE_COLUMNS: usize = 3;

/// A tile's face, a shade off the drawer.
const TILE: Color = rgb(0x1f, 0x26, 0x2e);

/// The tile a direction leads to from `from`, of `count` tiles three to a
/// row: round to the other end of the row or the column at an edge, and down
/// from a row with nothing straight under it to the last tile.
fn tile_step(from: usize, count: usize, direction: Button) -> usize {
    let column = from % TILE_COLUMNS;
    let last_row = (count - 1) / TILE_COLUMNS;
    match direction {
        Button::Up if from >= TILE_COLUMNS => from - TILE_COLUMNS,
        Button::Up => {
            let bottom = last_row * TILE_COLUMNS + column;
            if bottom < count { bottom } else { bottom - TILE_COLUMNS }
        }
        Button::Down if from + TILE_COLUMNS < count => from + TILE_COLUMNS,
        Button::Down if from / TILE_COLUMNS < last_row => count - 1,
        Button::Down => column,
        Button::Left if column > 0 => from - 1,
        Button::Left => (from + TILE_COLUMNS - 1).min(count - 1),
        _ if column + 1 < TILE_COLUMNS && from + 1 < count => from + 1,
        _ => from - column,
    }
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
        // A value tile - the speed, the quality, the window - taken up to
        // change: ◀ and ▶ step it instead of moving off it.
        let mut adjusting = false;
        let mut repeat = Repeat::new();
        let mut dirty = true;
        let icon = context.game.and_then(icons::extract);
        loop {
            let pressed = self.presses(&mut repeat);
            if let Some((tile, _)) = self.take_pointed() {
                let tile = tile.min(items.len() - 1);
                if tile != cursor {
                    adjusting = false;
                    dirty = true;
                }
                cursor = tile;
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
                    Button::Left | Button::Right if adjusting => {
                        let forward = button == Button::Right;
                        match items[cursor] {
                            Item::Quality => {
                                if let Some(game) = context.game {
                                    let quality = self.game_quality(game).step(forward);
                                    self.set_game_quality(game, quality);
                                }
                            }
                            Item::Screen => self.cycle_screen(forward),
                            Item::Speed => {
                                if let Some(game) = context.game {
                                    let speed = speed_step(self.game_speed(game), forward);
                                    self.set_game_speed(game, speed, context.in_game());
                                }
                            }
                            _ => {}
                        }
                    }
                    Button::Up | Button::Down | Button::Left | Button::Right => {
                        adjusting = false;
                        cursor = tile_step(cursor, items.len(), button);
                    }
                    Button::B | Button::Guide if adjusting => adjusting = false,
                    Button::B | Button::Guide => return Outcome::Close,
                    Button::A if adjusting => adjusting = false,
                    Button::A => match items[cursor] {
                        Item::Resume | Item::Close => return Outcome::Close,
                        Item::EndGame => return Outcome::EndGame,
                        Item::QuitApp => return Outcome::QuitApp,
                        Item::Keyboard => self.keyboard_layout(),
                        Item::Layout => self.layout(),
                        Item::Preset => self.presets(context),
                        Item::Screen | Item::Speed | Item::Quality => adjusting = true,
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
                self.draw_menu(context, icon.as_ref(), &items, cursor, adjusting);
                dirty = false;
            }
            std::thread::sleep(FRAME);
        }
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
    pub(crate) fn active_label(&self) -> String {
        let star = if self.store.modified() { "*" } else { "" };
        format!("{}{star}", self.store.active())
    }

    /// What a tile of the menu is called and the mark over it.
    fn item_face(&self, item: Item) -> (&'static str, &'static str) {
        match item {
            Item::Resume => ("▶", "돌아가기"),
            Item::Keyboard => ("⇥", "키보드 배치"),
            Item::Layout if self.desktop => ("●", "패드 버튼"),
            Item::Layout => ("●", "버튼 배치"),
            Item::Preset => ("★", "프리셋"),
            Item::Fix => ("✓", "프리셋 고정"),
            Item::Screen => ("▢", "화면"),
            Item::Speed => ("»", "배속"),
            Item::Quality => ("◇", "화질"),
            Item::Saves => ("♡", "세이브"),
            Item::Delete => ("×", "게임 삭제"),
            Item::EndGame => ("⏏", "게임 끝내기"),
            Item::Close => ("←", "닫기"),
            Item::QuitApp => ("⏏", "앱 종료"),
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
    /// its items in tiles three to a row so all of them fit at once, with
    /// what the tile picked sets shown large beside it.
    fn draw_menu(&mut self, context: &Context, icon: Option<&Picture>, items: &[Item], cursor: usize, adjusting: bool) {
        let (width, height, _) = self.menu_size();
        let mut screen = Screen::new(width, height);
        self.clear_hits();
        let (w, h) = (width as f32, height as f32);
        let side = (w * 0.575).clamp(184.0, 240.0).min(w);

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

        // The tiles, as tall as the rows of them leave room for.
        let (grid_x, grid_top, gap) = (8.0, 36.0, 4.0);
        let rows = items.len().div_ceil(TILE_COLUMNS);
        let tile_width = (side - 2.0 * grid_x - gap * (TILE_COLUMNS - 1) as f32) / TILE_COLUMNS as f32;
        let room = h - grid_top - BAR as f32 - 6.0;
        let tile_height = ((room - gap * (rows - 1) as f32) / rows as f32).min(56.0);
        for (index, item) in items.iter().enumerate() {
            let x = grid_x + (index % TILE_COLUMNS) as f32 * (tile_width + gap);
            let y = grid_top + (index / TILE_COLUMNS) as f32 * (tile_height + gap);
            let picked = index == cursor;
            let danger = matches!(item, Item::Delete | Item::QuitApp | Item::EndGame);
            let (mark, label) = self.item_face(*item);
            let value = self.item_value(context, *item);
            let (fill, edge) = match (picked, danger) {
                (true, _) => (WHITE, WHITE),
                (false, true) => (TILE, DANGER),
                (false, false) => (TILE, EDGE),
            };
            screen.round(x, y, tile_width, tile_height, 7.0, fill);
            screen.outline(x, y, tile_width, tile_height, 7.0, 1.0, edge);
            if picked && adjusting {
                screen.outline(x - 1.5, y - 1.5, tile_width + 3.0, tile_height + 3.0, 8.5, 1.0, ACCENT);
            }
            self.hit(x as i32, y as i32, tile_width as i32, tile_height as i32, Target::Row(index));

            // The mark over the name over the value, the mark left out when
            // the tile is too short for all three.
            let lines = TINY_BOLD.line + if value.is_empty() { 0.0 } else { TINY_BOLD.line };
            let with_mark = tile_height >= lines + SMALL.line + 6.0;
            let mut line_y = y + (tile_height - lines - if with_mark { SMALL.line } else { 0.0 }) / 2.0;
            let middle = x + tile_width / 2.0;
            let (mark_color, ink, value_color) = match (picked, danger) {
                (true, _) => (GREEN, INK, GREEN),
                (false, true) => (DANGER, DANGER, ACCENT),
                (false, false) => (rgb(0x7f, 0x8b, 0x93), rgb(0xd9, 0xdf, 0xe3), ACCENT),
            };
            if with_mark {
                screen.text_styled(mark, middle, line_y, SMALL, TextAlignment::Center, mark_color);
                line_y += SMALL.line;
            }
            screen.text_styled(
                &fit_with(label, tile_width - 4.0, TINY_BOLD),
                middle,
                line_y,
                TINY_BOLD,
                TextAlignment::Center,
                ink,
            );
            line_y += TINY_BOLD.line;
            if !value.is_empty() {
                if picked && adjusting {
                    // ‹ and › step the value, as the pad's ◀ and ▶ do.
                    let shown = fit_with(&value, tile_width - 22.0, TINY_BOLD);
                    screen.text_styled(&shown, middle, line_y, TINY_BOLD, TextAlignment::Center, value_color);
                    screen.text_styled("‹", x + 6.0, line_y, TINY_BOLD, TextAlignment::Center, MUTED);
                    screen.text_styled("›", x + tile_width - 6.0, line_y, TINY_BOLD, TextAlignment::Center, MUTED);
                    let third = (tile_width / 3.0) as i32;
                    self.hit(x as i32, y as i32, third, tile_height as i32, Target::Step(index, false));
                    self.hit(
                        (x + tile_width) as i32 - third,
                        y as i32,
                        third,
                        tile_height as i32,
                        Target::Step(index, true),
                    );
                } else {
                    let shown = fit_with(&value, tile_width - 6.0, TINY_BOLD);
                    screen.text_styled(&shown, middle, line_y, TINY_BOLD, TextAlignment::Center, value_color);
                }
            }
        }

        // What the tile picked sets, large, beside the drawer.
        let card_x = side + 14.0;
        let card_width = w - side - 26.0;
        if card_width >= 100.0 {
            match items[cursor] {
                Item::Speed => {
                    if let Some(game) = context.game {
                        self.draw_speed_card(&mut screen, card_x, (h - 62.0) / 2.0 - 10.0, card_width, self.game_speed(game), cursor);
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
            Item::Speed if adjusting => self.hint("◀▶ 0.1씩  A 완료", "◀▶ 0.1씩  Enter 완료"),
            _ if adjusting => self.hint("◀▶ 바꾸기  A 완료", "◀▶ 바꾸기  Enter 완료"),
            Item::Speed | Item::Quality | Item::Screen => self.hint("A 바꾸기  ◀▶▲▼ 이동", "Enter 바꾸기  ◀▶▲▼ 이동"),
            Item::Preset => self.hint("A 목록  ◀▶▲▼ 이동", "Enter 목록  ◀▶▲▼ 이동"),
            _ => self.hint("A 선택  ◀▶▲▼ 이동", "Enter 선택  ◀▶▲▼ 이동"),
        };
        let back = if adjusting {
            self.hint("B 뒤로", "Esc 뒤로")
        } else {
            self.hint("B 닫기", "Esc 닫기")
        };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tiles_move_in_rows_of_three() {
        // Eleven tiles: three full rows and two on the last.
        assert_eq!(tile_step(4, 11, Button::Up), 1);
        assert_eq!(tile_step(1, 11, Button::Up), 10);
        assert_eq!(tile_step(2, 11, Button::Up), 8);
        assert_eq!(tile_step(8, 11, Button::Down), 10);
        assert_eq!(tile_step(10, 11, Button::Down), 1);
        assert_eq!(tile_step(3, 11, Button::Left), 5);
        assert_eq!(tile_step(5, 11, Button::Right), 3);
        assert_eq!(tile_step(10, 11, Button::Right), 9);
        assert_eq!(tile_step(9, 11, Button::Left), 10);
        // Seven: the last alone on its row.
        assert_eq!(tile_step(5, 7, Button::Down), 6);
        assert_eq!(tile_step(6, 7, Button::Down), 0);
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
