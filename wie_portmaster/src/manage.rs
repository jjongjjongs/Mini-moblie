//! What the list does to a game's files: deleting the game, and its saves -
//! exported to the saves folder, imported from it, erased (see `saves`).
//!
//! Anything that removes or overwrites asks first, in a box whose first
//! answer, picked to start with, is the one that does nothing.

use std::{
    path::Path,
    time::{Duration, Instant},
};

use wie_backend::canvas::{Color, TextAlignment};

use crate::{
    App, FRAME,
    controls::Button,
    library::{self, BAR, HIGHLIGHT, LINE, MUTED, Screen, TEXT, fit},
    open_folder,
    saves::{self, SAVES_DIR, SaveZip},
    settings::{ACCENT, DANGER, DIM, EDGE, PANEL, Repeat, panel_bar, scroll, scroll_marks},
};

/// How long the note on what was just done stays up.
const NOTE: Duration = Duration::from_millis(2500);

/// A question in a box: lines on what it is about, and the answers.
pub struct Dialog {
    pub title: String,
    pub lines: Vec<(String, Color)>,
    /// Each answer, and whether it removes or overwrites something.
    pub options: Vec<(String, bool)>,
    /// A last word under the answers, if any.
    pub note: String,
}

/// A row of the save menu.
#[derive(Clone, Copy, PartialEq)]
enum SaveRow {
    Export,
    Import,
    Folder,
    Erase,
}

/// A row of the import list: a heading, or a zip by its place in the list.
enum ImportRow {
    Heading(String),
    Zip(usize),
}

impl App {
    /// Asks `dialog` over `back` (or a blank screen). The answer picked, or
    /// `None` when backed out of.
    pub(crate) fn ask(&mut self, dialog: &Dialog, back: Option<(u32, u32, &[u8])>) -> Option<usize> {
        let mut cursor: usize = 0;
        let mut repeat = Repeat::new();
        let mut dirty = true;
        loop {
            for button in self.presses(&mut repeat) {
                dirty = true;
                match button {
                    Button::Up => cursor = cursor.checked_sub(1).unwrap_or(dialog.options.len() - 1),
                    Button::Down => cursor = (cursor + 1) % dialog.options.len(),
                    Button::A => return Some(cursor),
                    Button::B | Button::Guide => return None,
                    _ => {}
                }
            }

            if dirty || self.take_redraw() {
                self.draw_dialog(dialog, cursor, back);
                dirty = false;
            }
            std::thread::sleep(FRAME);
        }
    }

    fn draw_dialog(&mut self, dialog: &Dialog, cursor: usize, back: Option<(u32, u32, &[u8])>) {
        let (width, height, _) = self.menu_size();
        let mut screen = Screen::new(width, height);
        if let Some((back_width, back_height, rgba)) = back {
            screen.backdrop(rgba, back_width, back_height);
            screen.fill(0, 0, width, height, DIM);
        }

        let note_height = if dialog.note.is_empty() { 0 } else { LINE };
        let panel_width = (width - 16).min(280);
        let lines_height = dialog.lines.len() as i32 * LINE + 6;
        let panel_height = (2 * BAR + 8 + lines_height + dialog.options.len() as i32 * LINE + note_height) as u32;
        let x = (width - panel_width) as i32 / 2;
        let y = ((height as i32 - panel_height as i32) / 2).max(0);
        screen.fill(x - 1, y - 1, panel_width + 2, panel_height + 2, EDGE);
        screen.fill(x, y, panel_width, panel_height, PANEL);
        panel_bar(&mut screen, x, y, panel_width, &dialog.title, "");

        let middle = x + panel_width as i32 / 2;
        let room = panel_width as f32 - 16.0;
        for (index, (line, color)) in dialog.lines.iter().enumerate() {
            screen.text(&fit(line, room), middle, y + BAR + 6 + index as i32 * LINE, TextAlignment::Center, *color);
        }
        let top = y + BAR + 4 + lines_height;
        for (index, (label, danger)) in dialog.options.iter().enumerate() {
            let row_y = top + index as i32 * LINE;
            let picked = index == cursor;
            if picked {
                screen.fill(x, row_y, panel_width, LINE as u32, HIGHLIGHT);
            }
            let color = if picked {
                TEXT
            } else if *danger {
                DANGER
            } else {
                MUTED
            };
            screen.text(&fit(label, room), x + 8, row_y + 1, TextAlignment::Left, color);
        }
        if !dialog.note.is_empty() {
            let note_y = top + dialog.options.len() as i32 * LINE + 1;
            screen.text(&fit(&dialog.note, room), middle, note_y, TextAlignment::Center, MUTED);
        }

        let (choose, cancel) = (self.hint("A 선택", "Enter 선택"), self.hint("B 취소", "Esc 취소"));
        panel_bar(&mut screen, x, y + panel_height as i32 - BAR, panel_width, choose, cancel);
        self.present(screen, width, height);
    }

    /// Asks whether to delete `game`, and with its saves or without, and does
    /// it. What became of it, unless nothing was done.
    pub(crate) fn delete_game(&mut self, game: &Path, back: Option<(u32, u32, &[u8])>) -> Option<String> {
        let name = saves::game_name(game);
        let file = game.file_name().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
        let saved = saves::summary(game).is_ok_and(|summary| summary.files > 0);
        let size = game.metadata().map_or(0, |x| x.len());

        let mut options = vec![("취소".to_owned(), false)];
        if saved {
            options.push(("게임만 삭제 (세이브는 남김)".to_owned(), true));
            options.push(("게임과 세이브 모두 삭제".to_owned(), true));
        } else {
            options.push(("삭제".to_owned(), true));
        }
        let dialog = Dialog {
            title: "게임 삭제".to_owned(),
            lines: vec![
                (name.clone(), TEXT),
                (format!("{file} · {}", saves::size_label(size)), MUTED),
                if saved {
                    ("세이브 데이터 있음".to_owned(), ACCENT)
                } else {
                    ("세이브 데이터 없음".to_owned(), MUTED)
                },
            ],
            options,
            note: removal_note(self.desktop).to_owned(),
        };

        let with_saves = match self.ask(&dialog, back) {
            Some(1) => false,
            Some(2) => true,
            _ => return None,
        };
        Some(match saves::delete_game(game, with_saves, self.desktop) {
            Ok(()) => {
                self.store.forget(&file);
                format!("삭제했습니다: {name}{}", if with_saves { " (세이브 포함)" } else { "" })
            }
            Err(error) => error,
        })
    }

    /// The save menu for `game`: export, import, the folder, erase.
    pub(crate) fn save_manager(&mut self, game: &Path) {
        let mut rows = vec![SaveRow::Export, SaveRow::Import];
        if self.desktop {
            rows.push(SaveRow::Folder);
        }
        rows.push(SaveRow::Erase);

        let mut cursor: usize = 0;
        let mut note: Option<(String, Instant)> = None;
        let mut summary = saves::summary(game).ok();
        let mut ours = saves::list(game).iter().filter(|zip| zip.ours).count();
        let mut repeat = Repeat::new();
        let mut dirty = true;
        loop {
            for button in self.presses(&mut repeat) {
                dirty = true;
                let done = match button {
                    Button::Up => {
                        cursor = cursor.checked_sub(1).unwrap_or(rows.len() - 1);
                        None
                    }
                    Button::Down => {
                        cursor = (cursor + 1) % rows.len();
                        None
                    }
                    Button::B | Button::Guide => return,
                    Button::X if self.desktop => {
                        open_saves_folder();
                        None
                    }
                    Button::A => match rows[cursor] {
                        SaveRow::Export => Some(match saves::export(game, "") {
                            Ok(Some(_)) => "saves 폴더에 저장했습니다".to_owned(),
                            Ok(None) => "내보낼 세이브가 없습니다".to_owned(),
                            Err(error) => error,
                        }),
                        SaveRow::Import => self.import_saves(game),
                        SaveRow::Folder => {
                            open_saves_folder();
                            None
                        }
                        SaveRow::Erase => self.erase_saves(game, summary.as_ref().map_or(0, |x| x.files)),
                    },
                    _ => None,
                };
                if let Some(text) = done {
                    note = Some((text, Instant::now() + NOTE));
                    summary = saves::summary(game).ok();
                    ours = saves::list(game).iter().filter(|zip| zip.ours).count();
                }
            }
            if note.as_ref().is_some_and(|(_, until)| Instant::now() >= *until) {
                note = None;
                dirty = true;
            }

            if dirty || self.take_redraw() {
                let text = note.as_ref().map(|(text, _)| text.as_str());
                self.draw_save_manager(game, &rows, cursor, summary.as_ref(), ours, text);
                dirty = false;
            }
            std::thread::sleep(FRAME);
        }
    }

    fn draw_save_manager(&mut self, game: &Path, rows: &[SaveRow], cursor: usize, summary: Option<&saves::Summary>, ours: usize, note: Option<&str>) {
        let (width, height, _) = self.menu_size();
        let mut screen = Screen::new(width, height);
        let panel_width = (width - 16).min(288);
        let body_height = 2 * LINE + 8;
        let panel_height = (2 * BAR + 8 + body_height + rows.len() as i32 * LINE) as u32;
        let x = (width - panel_width) as i32 / 2;
        let y = ((height as i32 - panel_height as i32) / 2).max(0);
        screen.fill(x - 1, y - 1, panel_width + 2, panel_height + 2, EDGE);
        screen.fill(x, y, panel_width, panel_height, PANEL);
        panel_bar(
            &mut screen,
            x,
            y,
            panel_width,
            "세이브 관리",
            &fit(&saves::game_name(game), panel_width as f32 / 2.0),
        );

        let middle = x + panel_width as i32 / 2;
        let (first, second) = match summary {
            Some(summary) if summary.files > 0 => (
                (format!("세이브 있음 · 파일 {}개", summary.files), ACCENT),
                summary
                    .latest
                    .map_or(String::new(), |time| format!("마지막 저장 {}", saves::local(time).long_label())),
            ),
            Some(_) => (("세이브 없음".to_owned(), MUTED), "게임에서 저장하면 생깁니다".to_owned()),
            None => (("세이브 위치를 알 수 없는 게임입니다".to_owned(), MUTED), String::new()),
        };
        screen.text(&first.0, middle, y + BAR + 6, TextAlignment::Center, first.1);
        screen.text(&second, middle, y + BAR + 6 + LINE, TextAlignment::Center, MUTED);

        for (index, row) in rows.iter().enumerate() {
            let (label, value) = match row {
                SaveRow::Export => ("내보내기", "saves 폴더로".to_owned()),
                SaveRow::Import => ("가져오기", if ours > 0 { format!("{ours}개") } else { String::new() }),
                SaveRow::Folder => ("saves 폴더 열기", "F2".to_owned()),
                SaveRow::Erase => ("세이브 지우기", String::new()),
            };
            let row_y = y + BAR + 4 + body_height + index as i32 * LINE;
            let picked = index == cursor;
            if picked {
                screen.fill(x, row_y, panel_width, LINE as u32, HIGHLIGHT);
            }
            let color = if picked {
                TEXT
            } else if *row == SaveRow::Erase {
                DANGER
            } else {
                MUTED
            };
            screen.text(label, x + 8, row_y + 1, TextAlignment::Left, color);
            if !value.is_empty() {
                screen.text(
                    &value,
                    x + panel_width as i32 - 8,
                    row_y + 1,
                    TextAlignment::Right,
                    if picked { TEXT } else { ACCENT },
                );
            }
        }
        let (choose, back) = (self.hint("A 선택", "Enter 선택"), self.hint("B 뒤로", "Esc 뒤로"));
        panel_bar(&mut screen, x, y + panel_height as i32 - BAR, panel_width, choose, back);

        let rgba = screen.rgba();
        let rgba = match note {
            Some(text) => library::with_toast(&rgba, width, height, text),
            None => rgba,
        };
        self.show(&rgba, width, height);
    }

    /// Asks before erasing `game`'s `files` saved files, and does it.
    fn erase_saves(&mut self, game: &Path, files: usize) -> Option<String> {
        if files == 0 {
            return Some("지울 세이브가 없습니다".to_owned());
        }
        let dialog = Dialog {
            title: "세이브 지우기".to_owned(),
            lines: vec![(saves::game_name(game), TEXT), (format!("세이브 파일 {files}개"), MUTED)],
            options: vec![("취소".to_owned(), false), ("세이브 지우기".to_owned(), true)],
            note: removal_note(self.desktop).to_owned(),
        };
        if self.ask(&dialog, None) != Some(1) {
            return None;
        }
        Some(match saves::erase(game, self.desktop) {
            Ok(_) => "세이브를 지웠습니다".to_owned(),
            Err(error) => error,
        })
    }

    /// The save zips in the saves folder, one to bring in picked and asked
    /// about. What became of it, unless nothing was done.
    fn import_saves(&mut self, game: &Path) -> Option<String> {
        let mut zips = saves::list(game);
        let mut cursor: usize = 0;
        let mut top = 0;
        let mut repeat = Repeat::new();
        let mut dirty = true;
        loop {
            for button in self.presses(&mut repeat) {
                dirty = true;
                match button {
                    Button::Up if !zips.is_empty() => cursor = cursor.checked_sub(1).unwrap_or(zips.len() - 1),
                    Button::Down if !zips.is_empty() => cursor = (cursor + 1) % zips.len(),
                    Button::B | Button::Guide => return None,
                    // The folder, to drop zips in; they show on coming back.
                    Button::X if self.desktop => {
                        open_saves_folder();
                        zips = saves::list(game);
                        cursor = 0;
                    }
                    Button::A if !zips.is_empty() => {
                        let zip = &zips[cursor];
                        let mut lines = vec![(zip.name.clone(), TEXT)];
                        if zip.ours {
                            lines.extend([
                                ("지금 세이브를 덮어씁니다.".to_owned(), MUTED),
                                ("덮기 전 세이브는 saves 폴더에".to_owned(), MUTED),
                                ("자동으로 보관합니다.".to_owned(), MUTED),
                            ]);
                        } else {
                            lines.extend([
                                ("다른 게임의 세이브입니다.".to_owned(), MUTED),
                                ("그 게임의 세이브를 덮어씁니다.".to_owned(), MUTED),
                            ]);
                        }
                        let dialog = Dialog {
                            title: "세이브 가져오기".to_owned(),
                            lines,
                            options: vec![("취소".to_owned(), false), ("가져오기".to_owned(), true)],
                            note: String::new(),
                        };
                        let (width, height, back) = self.draw_import(game, &zips, cursor, &mut top);
                        if self.ask(&dialog, Some((width, height, &back))) == Some(1) {
                            return Some(match saves::import(game, zip) {
                                Ok((count, _)) => format!("가져왔습니다 (파일 {count}개)"),
                                Err(error) => error,
                            });
                        }
                    }
                    _ => {}
                }
            }

            if dirty || self.take_redraw() {
                let (width, height, rgba) = self.draw_import(game, &zips, cursor, &mut top);
                self.show(&rgba, width, height);
                dirty = false;
            }
            std::thread::sleep(FRAME);
        }
    }

    fn draw_import(&self, game: &Path, zips: &[SaveZip], cursor: usize, top: &mut usize) -> (u32, u32, Vec<u8>) {
        let (width, height, _) = self.menu_size();
        let mut screen = Screen::new(width, height);
        screen.bar(0, "세이브 가져오기", "saves 폴더");
        let (choose, back) = (self.hint("A 가져오기", "Enter 가져오기  F2 폴더 열기"), self.hint("B 뒤로", "Esc 뒤로"));
        screen.bar(height as i32 - BAR, choose, back);

        if zips.is_empty() {
            screen.paragraph(
                "saves 폴더에 세이브 파일이 없습니다.\n\n폰이나 다른 기기에서 내보낸\n세이브 zip을 saves 폴더에\n넣어 주세요.",
                BAR + 30,
                MUTED,
            );
            return (width, height, screen.rgba());
        }

        let mut rows = Vec::new();
        for (index, zip) in zips.iter().enumerate() {
            let heading = match index.checked_sub(1).map(|before| zips[before].ours) {
                None if zip.ours => Some(format!("이 게임 ({})", saves::game_name(game))),
                None => Some("다른 게임".to_owned()),
                Some(true) if !zip.ours => Some("다른 게임".to_owned()),
                _ => None,
            };
            if let Some(heading) = heading {
                rows.push(ImportRow::Heading(heading));
            }
            rows.push(ImportRow::Zip(index));
        }

        let list_top = BAR + 4;
        let shown = ((height as i32 - list_top - BAR - 2) / LINE).max(1) as usize;
        let at = rows
            .iter()
            .position(|row| matches!(row, ImportRow::Zip(index) if *index == cursor))
            .unwrap_or(0);
        scroll(at, shown, top);
        // The heading over the first of its zips comes into view with it.
        if *top > 0 && *top == at && shown > 1 && matches!(rows[*top - 1], ImportRow::Heading(_)) {
            *top -= 1;
        }

        let own_prefix = format!("{} 세이브 ", saves::game_name(game));
        for (index, row) in rows.iter().enumerate().skip(*top).take(shown) {
            let y = list_top + (index - *top) as i32 * LINE;
            match row {
                ImportRow::Heading(text) => {
                    let text = fit(text, width as f32 - 40.0);
                    screen.text(&text, 8, y + 1, TextAlignment::Left, ACCENT);
                    let end = 8 + wie_backend::canvas::string_width_px(&text, 16.0) as i32 + 6;
                    if end < width as i32 - 8 {
                        screen.fill(end, y + LINE / 2, (width as i32 - 8 - end) as u32, 1, EDGE);
                    }
                }
                ImportRow::Zip(zip_index) => {
                    let zip = &zips[*zip_index];
                    let picked = *zip_index == cursor;
                    if picked {
                        screen.fill(0, y, width, LINE as u32, HIGHLIGHT);
                    }
                    let when = zip.modified.map_or(String::new(), |time| saves::local(time).short_label());
                    let color = if picked || zip.ours { TEXT } else { MUTED };
                    // Under the game's own heading, its exports by what follows
                    // the game's name: when they were taken.
                    let label = zip
                        .name
                        .strip_prefix(&own_prefix)
                        .filter(|rest| zip.ours && !rest.is_empty())
                        .unwrap_or(&zip.name);
                    screen.text(&fit(label, width as f32 - 120.0), 16, y + 1, TextAlignment::Left, color);
                    screen.text(&when, width as i32 - 8, y + 1, TextAlignment::Right, if picked { TEXT } else { MUTED });
                }
            }
        }
        scroll_marks(&mut screen, width, list_top, shown, *top, rows.len());
        (width, height, screen.rgba())
    }
}

/// What the player is told before something is removed.
fn removal_note(desktop: bool) -> &'static str {
    if saves::trash_available(desktop) {
        "지운 파일은 휴지통으로 갑니다"
    } else {
        "지우면 되돌릴 수 없습니다"
    }
}

/// Opens the saves folder, making it first if there is none yet.
fn open_saves_folder() {
    let _ = std::fs::create_dir_all(SAVES_DIR);
    open_folder(&std::path::absolute(SAVES_DIR).unwrap_or_else(|_| SAVES_DIR.into()));
}
