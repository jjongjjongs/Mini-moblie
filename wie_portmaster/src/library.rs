//! The list of games the port opens on, and the screens for a message.
//!
//! Drawn with the emulator's own canvas and the handset font it draws titles'
//! text with, at a size the screen is a whole multiple of.

use std::path::{Path, PathBuf};

use wie_backend::canvas::{ArgbPixel, Canvas, Clip, Color, ImageBufferCanvas, TextAlignment, VecImageBuffer, baseline_px, string_width_px};

use crate::controls::Button;

pub(crate) const FONT: f32 = 16.0;
pub(crate) const LINE: i32 = 18;
pub(crate) const BAR: i32 = 20;

pub(crate) const BACKGROUND: Color = rgb(0x12, 0x16, 0x1c);
pub(crate) const BAR_COLOR: Color = rgb(0x1e, 0x5c, 0x45);
pub(crate) const HIGHLIGHT: Color = rgb(0x2e, 0x7d, 0x5b);
pub(crate) const TEXT: Color = rgb(0xf2, 0xf4, 0xf5);
pub(crate) const MUTED: Color = rgb(0x8c, 0x96, 0x9e);

pub(crate) const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color { a: 0xff, r, g, b }
}

/// The game files in one folder and which of them is picked.
pub struct Menu {
    folder: PathBuf,
    games: Vec<PathBuf>,
    selected: usize,
    /// The first entry on screen.
    top: usize,
    /// How many entries the last drawing had room for.
    rows: usize,
}

impl Menu {
    pub fn new(folder: &Path) -> Menu {
        Menu {
            folder: folder.to_owned(),
            games: Vec::new(),
            selected: 0,
            top: 0,
            rows: 1,
        }
    }

    /// Reads the folder again, keeping the pick on the same file if it is
    /// still there.
    pub fn refresh(&mut self) {
        let current = self.games.get(self.selected).cloned();
        let mut games: Vec<PathBuf> = std::fs::read_dir(&self.folder)
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                    .filter(|path| path.is_file() && is_game(path))
                    .collect()
            })
            .unwrap_or_default();
        games.sort_by_key(|path| name(path).to_lowercase());
        self.selected = current.and_then(|current| games.iter().position(|x| *x == current)).unwrap_or(0);
        self.games = games;
    }

    pub fn selected(&self) -> Option<PathBuf> {
        self.games.get(self.selected).cloned()
    }

    /// Moves the pick for a button. Whether anything changed.
    pub fn navigate(&mut self, button: Button) -> bool {
        if self.games.is_empty() {
            return false;
        }
        let last = self.games.len() - 1;
        let page = self.rows.max(1);
        let selected = match button {
            Button::Up => self.selected.checked_sub(1).unwrap_or(last),
            Button::Down => {
                if self.selected == last {
                    0
                } else {
                    self.selected + 1
                }
            }
            Button::Left | Button::L1 => self.selected.saturating_sub(page),
            Button::Right | Button::R1 => (self.selected + page).min(last),
            _ => return false,
        };
        let changed = selected != self.selected;
        self.selected = selected;
        changed
    }

    /// The list as RGBA pixels, `width` by `height`.
    pub fn draw(&mut self, width: u32, height: u32) -> Vec<u8> {
        let mut canvas = Screen::new(width, height);

        let count = if self.games.is_empty() {
            String::new()
        } else {
            format!("{}/{}", self.selected + 1, self.games.len())
        };
        canvas.bar(0, "MiniMobile", &count);
        canvas.bar(height as i32 - BAR, "A 실행  Y 설정", "SELECT+START 종료");

        if self.games.is_empty() {
            let text = format!(
                "게임이 없습니다.\n\n아래 폴더에 게임 파일\n(.zip, .jar)을 넣어 주세요.\n\n{}",
                self.folder.canonicalize().unwrap_or_else(|_| self.folder.clone()).display()
            );
            canvas.paragraph(&text, BAR + 12, MUTED);
            return canvas.rgba();
        }

        let list_top = BAR + 4;
        self.rows = ((height as i32 - 2 * BAR - 8) / LINE).max(1) as usize;
        if self.selected < self.top {
            self.top = self.selected;
        } else if self.selected >= self.top + self.rows {
            self.top = self.selected + 1 - self.rows;
        }

        for (row, game) in self.games.iter().enumerate().skip(self.top).take(self.rows) {
            let y = list_top + (row - self.top) as i32 * LINE;
            let picked = row == self.selected;
            if picked {
                canvas.fill(0, y, width, LINE as u32, HIGHLIGHT);
            }
            let label = fit(&name(game), width as f32 - 16.0);
            canvas.text(
                &label,
                8,
                y + (LINE - FONT as i32) / 2,
                TextAlignment::Left,
                if picked { TEXT } else { MUTED },
            );
        }

        canvas.rgba()
    }
}

/// A screen of `text`, centred, with `hint` in the bar under it.
pub fn draw_message(text: &str, hint: &str, width: u32, height: u32) -> Vec<u8> {
    let mut canvas = Screen::new(width, height);
    let lines = wrap(text, width as f32 - 24.0);
    let top = ((height as i32 - lines.len() as i32 * LINE) / 2).max(BAR + 4);
    for (index, line) in lines.iter().enumerate() {
        canvas.text(line, width as i32 / 2, top + index as i32 * LINE, TextAlignment::Center, TEXT);
    }
    if !hint.is_empty() {
        canvas.bar(height as i32 - BAR, hint, "");
    }
    canvas.rgba()
}

fn is_game(path: &Path) -> bool {
    path.extension()
        .and_then(|x| x.to_str())
        .is_some_and(|x| x.eq_ignore_ascii_case("zip") || x.eq_ignore_ascii_case("jar"))
}

/// What the list calls a game: its file name without the extension.
fn name(path: &Path) -> String {
    path.file_stem().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default()
}

/// `text`, cut short with an ellipsis if it is wider than `width`.
pub(crate) fn fit(text: &str, width: f32) -> String {
    if string_width_px(text, FONT) <= width {
        return text.to_owned();
    }
    let mut cut: String = text.to_owned();
    while !cut.is_empty() && string_width_px(&format!("{cut}…"), FONT) > width {
        cut.pop();
    }
    format!("{cut}…")
}

/// `text` broken into lines no wider than `width`, at its own line breaks and
/// then wherever a line runs out of room.
fn wrap(text: &str, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for c in paragraph.chars() {
            line.push(c);
            if string_width_px(&line, FONT) > width {
                let last = line.pop().unwrap();
                // A space that does not fit is the break itself.
                if last == ' ' {
                    lines.push(std::mem::take(&mut line));
                    continue;
                }
                // Break at the last space when there is one, else mid-word.
                match line.rfind(' ') {
                    Some(space) if space > 0 => {
                        let rest = line.split_off(space + 1);
                        lines.push(line.trim_end().to_owned());
                        line = rest;
                    }
                    _ => lines.push(std::mem::take(&mut line)),
                }
                line.push(last);
            }
        }
        lines.push(line);
    }
    lines
}

pub(crate) struct Screen {
    canvas: ImageBufferCanvas<VecImageBuffer<ArgbPixel>>,
    width: u32,
    height: u32,
}

impl Screen {
    pub(crate) fn new(width: u32, height: u32) -> Screen {
        let mut screen = Screen {
            canvas: ImageBufferCanvas::new(VecImageBuffer::new(width, height)),
            width,
            height,
        };
        screen.fill(0, 0, width, height, BACKGROUND);
        screen
    }

    pub(crate) fn clip(&self) -> Clip {
        Clip {
            x: 0,
            y: 0,
            width: self.width,
            height: self.height,
        }
    }

    pub(crate) fn fill(&mut self, x: i32, y: i32, width: u32, height: u32, color: Color) {
        let clip = self.clip();
        self.canvas.fill_rect(x, y, width, height, color, clip);
    }

    pub(crate) fn text(&mut self, text: &str, x: i32, y: i32, alignment: TextAlignment, color: Color) {
        let clip = self.clip();
        self.canvas.draw_text(text, x, y, FONT, baseline_px(FONT), alignment, color, clip);
    }

    /// A bar across the screen at `y`, with `left` and `right` at its ends.
    pub(crate) fn bar(&mut self, y: i32, left: &str, right: &str) {
        self.fill(0, y, self.width, BAR as u32, BAR_COLOR);
        let text_y = y + (BAR - FONT as i32) / 2;
        self.text(left, 6, text_y, TextAlignment::Left, TEXT);
        if !right.is_empty() {
            self.text(right, self.width as i32 - 6, text_y, TextAlignment::Right, TEXT);
        }
    }

    /// `rgba`, `width` by `height`, fitted to the whole screen at its own
    /// shape, by whole pixels - the game behind a menu.
    pub(crate) fn backdrop(&mut self, rgba: &[u8], width: u32, height: u32) {
        if width == 0 || height == 0 || rgba.len() < (width * height * 4) as usize {
            return;
        }
        let scale = (self.width as f32 / width as f32).min(self.height as f32 / height as f32);
        let (shown_width, shown_height) = ((width as f32 * scale) as u32, (height as f32 * scale) as u32);
        let (left, top) = ((self.width - shown_width) / 2, (self.height - shown_height) / 2);
        for y in 0..shown_height {
            let source_y = ((y as f32 / scale) as u32).min(height - 1);
            for x in 0..shown_width {
                let source_x = ((x as f32 / scale) as u32).min(width - 1);
                let at = ((source_y * width + source_x) * 4) as usize;
                let color = Color {
                    a: 0xff,
                    r: rgba[at],
                    g: rgba[at + 1],
                    b: rgba[at + 2],
                };
                self.canvas.put_pixel((left + x) as i32, (top + y) as i32, color);
            }
        }
    }

    pub(crate) fn paragraph(&mut self, text: &str, top: i32, color: Color) {
        for (index, line) in wrap(text, self.width as f32 - 24.0).iter().enumerate() {
            self.text(line, self.width as i32 / 2, top + index as i32 * LINE, TextAlignment::Center, color);
        }
    }

    pub(crate) fn rgba(self) -> Vec<u8> {
        self.canvas
            .image()
            .colors()
            .iter()
            .flat_map(|color| [color.r, color.g, color.b, 0xff])
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_breaks_at_spaces_and_newlines() {
        let lines = wrap("가나다 라마바 사아자\n끝", string_width_px("가나다 라마바", FONT) + 1.0);
        assert_eq!(lines, vec!["가나다 라마바", "사아자", "끝"]);
    }

    #[test]
    fn fit_cuts_long_names() {
        let short = fit("레전드 오브 마스터", 1000.0);
        assert_eq!(short, "레전드 오브 마스터");
        let cut = fit("레전드 오브 마스터", string_width_px("레전드…", FONT));
        assert_eq!(cut, "레전드…");
    }
}
