//! The list of games the port opens on, and the screens for a message.
//!
//! The games stand in a row by their icons, the one picked large in the
//! middle and its neighbours small and dim either side; the background takes
//! the colour of the one picked. Tabs over them narrow the row to a carrier,
//! and a search narrows it to a name (see `search`) - typed on a keyboard, or
//! picked on a pad from a keyboard of initial consonants under the row.
//!
//! What it takes a game's file to know - its icon and its carrier - is read
//! on a thread of its own as the list opens, and each game takes its icon and
//! badge as it comes; the carriers are kept in `cache/carriers.txt` so a large
//! folder is only read through once.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use wie_android::host;
use wie_backend::canvas::{Color, TextAlignment};

use crate::{
    controls::Button,
    icons,
    search::{self, Action, Mode},
    ui::{
        ACCENT, BODY, GREEN, INK, LINE, MUTED, Picture, SMALL, Screen, Style, TEXT, TINY, TINY_BOLD, TITLE, WHITE, fit_with, rgb, text_width, wrap,
    },
};

/// How long what a drop or a delete came to stays up.
const STATUS_TIME: Duration = Duration::from_secs(4);
/// The height of the header and of the hints at the bottom.
const HEADER: f32 = 24.0;
const FOOTER: f32 = 22.0;

/// What is known of a game from its file.
struct Info {
    carrier: String,
    icon: Option<Picture>,
}

/// The carriers the tabs narrow the list to.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tab {
    All,
    Ktf,
    Lgt,
    Skt,
    Other,
}

impl Tab {
    const ALL: [Tab; 5] = [Tab::All, Tab::Ktf, Tab::Lgt, Tab::Skt, Tab::Other];

    fn label(self) -> &'static str {
        match self {
            Tab::All => "전체",
            Tab::Ktf => "KTF",
            Tab::Lgt => "LGT",
            Tab::Skt => "SKT",
            Tab::Other => "기타",
        }
    }

    /// Whether a game of `carrier` is under this tab. A game whose carrier is
    /// not known yet is only under 전체.
    fn admits(self, carrier: Option<&str>) -> bool {
        match (self, carrier) {
            (Tab::All, _) => true,
            (_, None) => false,
            (Tab::Ktf, Some(carrier)) => carrier == "KTF",
            (Tab::Lgt, Some(carrier)) => carrier == "LGT",
            (Tab::Skt, Some(carrier)) => carrier == "SKT",
            (Tab::Other, Some(carrier)) => !["KTF", "LGT", "SKT"].contains(&carrier),
        }
    }
}

/// What the mouse is over.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Hit {
    /// A game in the row, by its place among those shown.
    Game(usize),
    Tab(Tab),
    /// The search field, to open it.
    Search,
    /// A key of the pad's keyboard.
    Key(usize),
    /// The button that opens the games folder on an empty list.
    Folder,
}

/// A search being typed.
struct Search {
    query: String,
    /// What the input method is still composing, shown and matched as if
    /// typed.
    editing: String,
    /// The pad's keyboard, while it is up: its keys and the one picked.
    pad: Option<(Mode, usize)>,
}

/// The games in one folder and which of them is picked.
pub struct Menu {
    folder: PathBuf,
    games: Vec<PathBuf>,
    /// The game in the middle of the row.
    picked: Option<PathBuf>,
    tab: Tab,
    /// A window on a desktop, where the keyboard's keys are the ones to show
    /// and games can be dropped on it.
    desktop: bool,
    /// The games dropped on the window this time.
    added: Vec<PathBuf>,
    /// What the last drop or delete came to, and until when it shows.
    status: Option<(String, Instant)>,
    info: HashMap<PathBuf, Info>,
    /// Where the icons and carriers being read come in.
    loading: Option<mpsc::Receiver<(PathBuf, Info)>>,
    search: Option<Search>,
    /// Game file name, and when it was last started.
    played: HashMap<String, u64>,
    /// The parts of the screen last drawn the mouse can hit, the last on top.
    hits: Vec<((f32, f32, f32, f32), Hit)>,
}

impl Menu {
    pub fn new(folder: &Path, desktop: bool) -> Menu {
        Menu {
            folder: folder.to_owned(),
            games: Vec::new(),
            picked: None,
            tab: Tab::All,
            desktop,
            added: Vec::new(),
            status: None,
            info: HashMap::new(),
            loading: None,
            search: None,
            played: HashMap::new(),
            hits: Vec::new(),
        }
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// Reads the folder again, keeping the pick on the same file if it is
    /// still there, and starts reading what is not known of the games yet.
    pub fn refresh(&mut self) {
        let mut games: Vec<PathBuf> = std::fs::read_dir(&self.folder)
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                    .filter(|path| path.is_file() && is_game(path))
                    .collect()
            })
            .unwrap_or_default();
        games.sort_by_key(|path| name(path).to_lowercase());
        self.games = games;
        self.info.retain(|path, _| path.exists());

        let unknown: Vec<PathBuf> = self.games.iter().filter(|game| !self.info.contains_key(*game)).cloned().collect();
        if !unknown.is_empty() {
            self.loading = Some(read_infos(unknown));
        }
        if self.picked.as_ref().is_none_or(|picked| !self.games.contains(picked)) {
            self.picked = self.visible().first().map(|(game, _)| game.clone());
        }
    }

    /// Copies a file dropped on the window into the games folder and picks
    /// it, saying how that went.
    pub fn add(&mut self, path: &Path) {
        let file_name = path.file_name().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
        if !path.is_file() || !is_game(path) {
            self.set_status(format!("게임 파일(.zip, .jar)이 아닙니다: {file_name}"));
            return;
        }
        let target = self.folder.join(&file_name);
        if target.exists() {
            self.set_status(format!("이미 있습니다: {file_name}"));
        } else {
            match std::fs::copy(path, &target) {
                Ok(_) => {
                    self.set_status(format!("추가했습니다: {file_name}"));
                    self.added.push(target.clone());
                }
                Err(error) => {
                    self.set_status(format!("복사할 수 없습니다: {error}"));
                    return;
                }
            }
        }
        self.search = None;
        self.tab = Tab::All;
        self.refresh();
        self.picked = Some(target);
    }

    /// Says what something done to the list came to, for a moment.
    pub fn set_status(&mut self, status: String) {
        self.status = (!status.is_empty()).then(|| (status, Instant::now() + STATUS_TIME));
    }

    /// When each game was last started, by file name.
    pub fn set_played(&mut self, played: HashMap<String, u64>) {
        self.played = played;
    }

    /// Takes in the icons and carriers read since the last call, and lets a
    /// status that has had its time go. Whether the list has to be drawn
    /// again.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        if let Some(loading) = &self.loading {
            loop {
                match loading.try_recv() {
                    Ok((game, info)) => {
                        self.info.insert(game, info);
                        changed = true;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.loading = None;
                        break;
                    }
                }
            }
        }
        if self.status.as_ref().is_some_and(|(_, until)| Instant::now() >= *until) {
            self.status = None;
            changed = true;
        }
        changed
    }

    fn carrier(&self, game: &Path) -> Option<&str> {
        self.info.get(game).map(|info| info.carrier.as_str())
    }

    /// The games in the row: those the search matches, with the letters it
    /// matched, or else those under the tab.
    fn visible(&self) -> Vec<(PathBuf, Vec<usize>)> {
        match &self.search {
            Some(search) if !search.query.is_empty() || !search.editing.is_empty() => {
                let query = format!("{}{}", search.query, search.editing);
                self.games
                    .iter()
                    .filter_map(|game| search::matches(&name(game), &query).map(|marks| (game.clone(), marks)))
                    .collect()
            }
            _ => self
                .games
                .iter()
                .filter(|game| self.tab.admits(self.carrier(game)))
                .map(|game| (game.clone(), Vec::new()))
                .collect(),
        }
    }

    /// Where the picked game is in `visible`; the first when it is not there.
    fn current(&self, visible: &[(PathBuf, Vec<usize>)]) -> usize {
        self.picked
            .as_ref()
            .and_then(|picked| visible.iter().position(|(game, _)| game == picked))
            .unwrap_or(0)
    }

    pub fn selected(&self) -> Option<PathBuf> {
        let visible = self.visible();
        visible.get(self.current(&visible)).map(|(game, _)| game.clone())
    }

    /// Picks the game at `index` in the row. Whether that changed anything.
    pub fn select(&mut self, index: usize) -> bool {
        let visible = self.visible();
        match visible.get(index) {
            Some((game, _)) if self.picked.as_ref() != Some(game) => {
                self.picked = Some(game.clone());
                true
            }
            _ => false,
        }
    }

    /// The tabs worth showing: 전체, and each carrier some game is of.
    fn tabs(&self) -> Vec<Tab> {
        Tab::ALL
            .into_iter()
            .filter(|tab| *tab == Tab::All || self.games.iter().any(|game| tab.admits(self.carrier(game))))
            .collect()
    }

    pub fn set_tab(&mut self, tab: Tab) -> bool {
        if tab == self.tab {
            return false;
        }
        self.tab = tab;
        let visible = self.visible();
        if !visible.iter().any(|(game, _)| Some(game) == self.picked.as_ref()) {
            self.picked = visible.first().map(|(game, _)| game.clone());
        }
        true
    }

    /// Moves the pick for a button: sideways (or up and down) through the
    /// row, L and R through the tabs. Whether anything changed.
    pub fn navigate(&mut self, button: Button) -> bool {
        match button {
            Button::L1 | Button::R1 if self.search.is_none() => {
                let tabs = self.tabs();
                let at = tabs.iter().position(|tab| *tab == self.tab).unwrap_or(0);
                let next = if button == Button::R1 {
                    (at + 1) % tabs.len()
                } else {
                    at.checked_sub(1).unwrap_or(tabs.len() - 1)
                };
                self.set_tab(tabs[next])
            }
            Button::Left | Button::Up | Button::L1 | Button::Right | Button::Down | Button::R1 => {
                let visible = self.visible();
                if visible.is_empty() {
                    return false;
                }
                let current = self.current(&visible);
                let next = if matches!(button, Button::Right | Button::Down | Button::R1) {
                    (current + 1) % visible.len()
                } else {
                    current.checked_sub(1).unwrap_or(visible.len() - 1)
                };
                self.select(next)
            }
            _ => false,
        }
    }

    /// What is at `x`, `y` on the list as last drawn.
    pub fn hit_at(&self, x: i32, y: i32) -> Option<Hit> {
        let (x, y) = (x as f32, y as f32);
        self.hits
            .iter()
            .rev()
            .find(|((left, top, width, height), _)| x >= *left && x < left + width && y >= *top && y < top + height)
            .map(|(_, hit)| *hit)
    }

    pub fn searching(&self) -> bool {
        self.search.is_some()
    }

    /// Whether the pad's keyboard is up.
    pub fn pad_keyboard(&self) -> bool {
        self.search.as_ref().is_some_and(|search| search.pad.is_some())
    }

    /// Opens the search, with the pad's keyboard when `pad`.
    pub fn open_search(&mut self, pad: bool) {
        let search = self.search.get_or_insert_with(|| Search {
            query: String::new(),
            editing: String::new(),
            pad: None,
        });
        if pad && search.pad.is_none() {
            search.pad = Some((Mode::Initials, 0));
        }
    }

    pub fn close_search(&mut self) {
        self.search = None;
        let visible = self.visible();
        if !visible.iter().any(|(game, _)| Some(game) == self.picked.as_ref()) {
            self.picked = visible.first().map(|(game, _)| game.clone());
        }
    }

    /// Text typed into the search.
    pub fn type_text(&mut self, text: &str) {
        if let Some(search) = &mut self.search {
            search.query.extend(text.chars().filter(|c| !c.is_control()));
            search.editing.clear();
            self.repick();
        }
    }

    /// What the input method is composing.
    pub fn set_editing(&mut self, text: &str) {
        if let Some(search) = &mut self.search {
            search.editing = text.to_owned();
            self.repick();
        }
    }

    /// Takes the last letter off the search; closes it when there was none.
    pub fn erase(&mut self) {
        let Some(search) = &mut self.search else {
            return;
        };
        if search.query.pop().is_none() {
            self.close_search();
        } else {
            self.repick();
        }
    }

    /// Keeps the pick on a game the search still matches.
    fn repick(&mut self) {
        let visible = self.visible();
        if !visible.iter().any(|(game, _)| Some(game) == self.picked.as_ref()) {
            self.picked = visible.first().map(|(game, _)| game.clone());
        }
    }

    /// Moves the pick on the pad's keyboard.
    pub fn pad_move(&mut self, dx: i32, dy: i32) {
        if let Some(Search {
            pad: Some((mode, cursor)), ..
        }) = &mut self.search
        {
            let keys = search::keys(*mode, 280.0);
            *cursor = search::step(&keys, (*cursor).min(keys.len() - 1), dx, dy);
        }
    }

    /// Presses the key picked on the pad's keyboard, or the one at `index`.
    pub fn pad_press(&mut self, index: Option<usize>) {
        let Some(Search {
            pad: Some((mode, cursor)), ..
        }) = &mut self.search
        else {
            return;
        };
        let keys = search::keys(*mode, 280.0);
        let at = index.unwrap_or(*cursor).min(keys.len() - 1);
        *cursor = at;
        match keys[at].action.clone() {
            Action::Type(text) => self.type_text(text),
            Action::Space => self.type_text(" "),
            Action::Erase => self.erase(),
            Action::Mode(next) => {
                if let Some(Search { pad: Some(pad), .. }) = &mut self.search {
                    let keys = search::keys(next, 280.0);
                    *pad = (next, keys.iter().position(|key| key.action == Action::Mode(pad.0)).unwrap_or(0));
                }
            }
            Action::Done => {
                if let Some(search) = &mut self.search {
                    search.pad = None;
                }
            }
        }
    }

    fn hit(&mut self, x: f32, y: f32, width: f32, height: f32, hit: Hit) {
        self.hits.push(((x, y, width, height), hit));
    }

    /// The list as the screen's pixels, `width` by `height` layout units.
    pub fn draw(&mut self, width: u32, height: u32) -> Vec<u8> {
        self.hits.clear();
        let mut screen = Screen::new(width, height);
        let (w, h) = (width as f32, height as f32);

        if self.games.is_empty() {
            self.draw_empty(&mut screen);
            return screen.rgba();
        }

        let visible = self.visible();
        let current = self.current(&visible);
        let picked = visible.get(current).map(|(game, _)| game.clone());
        let tint = picked
            .as_ref()
            .and_then(|game| self.info.get(game))
            .and_then(|info| info.icon.as_ref())
            .map_or(rgb(0x1e, 0x2a, 0x30), Picture::average);
        screen.gradient(scaled(tint, 0.55), rgb(0x0a, 0x0c, 0x10));

        self.draw_header(&mut screen, w);

        let pad = self.search.as_ref().and_then(|search| search.pad);
        let keyboard_top = pad.map(|(mode, _)| h - FOOTER - search::height(mode) - 14.0);

        if visible.is_empty() {
            let message = if self.searching() {
                "찾는 게임이 없어요"
            } else {
                "이 분류에는 게임이 없어요"
            };
            let bottom = keyboard_top.unwrap_or(h - FOOTER);
            screen.text_styled(
                message,
                w / 2.0,
                HEADER + (bottom - HEADER) / 2.0 - 8.0,
                BODY,
                TextAlignment::Center,
                MUTED,
            );
        } else {
            // The row, as large as the room over the keyboard (or the hints)
            // leaves.
            let bottom = keyboard_top.unwrap_or(h - FOOTER);
            let compact = keyboard_top.is_some();
            let below = if compact { 30.0 } else { 54.0 };
            let size = (bottom - HEADER - below - 10.0).clamp(28.0, if compact { 58.0 } else { 84.0 });
            let top = HEADER + ((bottom - HEADER) - (size + below)) / 2.0 + 2.0;
            self.draw_row(&mut screen, &visible, current, w, top, size);

            let (game, marks) = &visible[current];
            let title_style = if compact { BODY } else { TITLE };
            let title_y = top + size + 6.0;
            draw_title(&mut screen, &name(game), marks, w / 2.0, title_y, w - 24.0, title_style);
            let mut info_y = title_y + title_style.line + 3.0;
            if !compact {
                self.draw_details(&mut screen, game, w / 2.0, info_y);
                info_y += 17.0;
            }
            draw_position(&mut screen, visible.len(), current, w / 2.0, info_y + 2.0);
        }

        if let (Some((mode, cursor)), Some(top)) = (pad, keyboard_top) {
            self.draw_keyboard(&mut screen, mode, cursor, w, top);
        }

        if let Some((status, _)) = &self.status {
            screen.toast(status, FOOTER as i32 + 4);
        }
        let (left, right) = self.hint();
        screen.bar(height as i32 - FOOTER as i32, &left, &right);
        screen.rgba()
    }

    fn hint(&self) -> (String, String) {
        let (left, right) = match (&self.search, self.desktop) {
            (Some(Search { pad: Some(_), .. }), _) => ("A 입력  B 지우기  L R 결과 넘기기", "START 실행"),
            (Some(_), true) => ("Enter 실행  ◀▶ 고르기", "Esc 검색 닫기"),
            (Some(_), false) => ("A 실행  ◀▶ 고르기  X 자판", "B 검색 닫기"),
            (None, true) => ("Enter 실행  ◀▶ 고르기  Tab 검색", "Del 삭제  Esc 메뉴"),
            (None, false) => ("A 실행  ◀▶ 고르기  X 검색", "L R 분류  Y 메뉴"),
        };
        (left.to_owned(), right.to_owned())
    }

    fn draw_header(&mut self, screen: &mut Screen, w: f32) {
        if let Some(search) = &self.search {
            // The search field across the top, and how many it found.
            let found = self.visible().len();
            screen.round(10.0, 4.0, w - 20.0, 17.0, 8.5, WHITE);
            draw_magnifier(screen, 17.0, 8.0, GREEN);
            let mut x = 31.0;
            x += screen.text_styled(&search.query, x, 6.0, SMALL, TextAlignment::Left, INK);
            if !search.editing.is_empty() {
                let editing = screen.text_styled(&search.editing, x, 6.0, SMALL, TextAlignment::Left, GREEN);
                screen.fill(x as i32, 18, editing.ceil() as u32, 1, GREEN);
                x += editing;
            }
            screen.round(x + 1.0, 7.5, 1.0, 10.0, 0.5, GREEN);
            if search.query.is_empty() && search.editing.is_empty() {
                let prompt = if search.pad.is_some() {
                    "초성이나 글자를 고르세요"
                } else {
                    "게임 이름이나 초성(ㅎㄱ)을 치세요"
                };
                screen.text_styled(prompt, x + 5.0, 6.0, SMALL, TextAlignment::Left, rgb(0x9a, 0xa5, 0xad));
            }
            let count = format!("{found}개 찾음");
            let count_width = text_width(&count, TINY_BOLD) + 10.0;
            screen.round(w - 15.0 - count_width, 7.0, count_width, 11.0, 5.5, rgb(0xdc, 0xf2, 0xe2));
            screen.text_styled(
                &count,
                w - 15.0 - count_width / 2.0,
                6.5,
                TINY_BOLD,
                TextAlignment::Center,
                rgb(0x22, 0x72, 0x47),
            );
            self.hit(10.0, 4.0, w - 20.0, 17.0, Hit::Search);
            return;
        }

        screen.text_styled("MiniMobile", 10.0, 5.0, SMALL, TextAlignment::Left, TEXT);
        // The search field, small, to open it.
        let field_x = 14.0 + text_width("MiniMobile", SMALL);
        let key = if self.desktop { "Tab" } else { "X" };
        let field_width = 30.0 + text_width("검색", TINY) + text_width(key, TINY_BOLD) + 8.0;
        screen.round(field_x, 4.5, field_width, 15.0, 7.5, rgb(0x2a, 0x31, 0x38));
        draw_magnifier(screen, field_x + 6.0, 8.0, rgb(0xaa, 0xb4, 0xbb));
        screen.text_styled("검색", field_x + 18.0, 6.0, TINY, TextAlignment::Left, rgb(0xaa, 0xb4, 0xbb));
        let cap = text_width(key, TINY_BOLD) + 8.0;
        screen.round(field_x + field_width - cap - 3.0, 7.0, cap, 10.0, 3.0, rgb(0x3a, 0x43, 0x4b));
        screen.text_styled(key, field_x + field_width - cap / 2.0 - 3.0, 6.0, TINY_BOLD, TextAlignment::Center, WHITE);
        self.hit(field_x, 4.5, field_width, 15.0, Hit::Search);

        // The carrier tabs at the right.
        let tabs = self.tabs();
        let mut x = w - 10.0;
        for tab in tabs.iter().rev() {
            let tab_width = text_width(tab.label(), TINY_BOLD) + 10.0;
            x -= tab_width;
            if x < field_x + field_width + 4.0 {
                break;
            }
            if *tab == self.tab {
                screen.round(x, 5.5, tab_width, 13.0, 6.5, WHITE);
                screen.text_styled(tab.label(), x + tab_width / 2.0, 6.0, TINY_BOLD, TextAlignment::Center, INK);
            } else {
                screen.text_styled(tab.label(), x + tab_width / 2.0, 6.0, TINY, TextAlignment::Center, rgb(0xc9, 0xd0, 0xd6));
            }
            self.hit(x, 4.0, tab_width, 16.0, Hit::Tab(*tab));
            x -= 2.0;
        }
    }

    /// The picked game large in the middle at `top`, `size` square, two
    /// neighbours each side smaller and dimmer.
    fn draw_row(&mut self, screen: &mut Screen, visible: &[(PathBuf, Vec<usize>)], current: usize, w: f32, top: f32, size: f32) {
        let gap = size * 0.72;
        for distance in [2usize, 1] {
            for forward in [false, true] {
                let index = if forward {
                    current + distance
                } else {
                    match current.checked_sub(distance) {
                        Some(index) => index,
                        None => continue,
                    }
                };
                let Some((game, _)) = visible.get(index) else {
                    continue;
                };
                let (side, bright) = if distance == 1 { (size * 2.0 / 3.0, 0.6) } else { (size / 2.0, 0.35) };
                let offset = gap + (distance - 1) as f32 * gap * 0.85;
                let centre = w / 2.0 + if forward { offset } else { -offset };
                let (x, y) = (centre - side / 2.0, top + (size - side) / 2.0);
                self.draw_icon(screen, game, x, y, side, bright);
                self.hit(x, y, side, side, Hit::Game(index));
            }
        }
        let (game, _) = &visible[current];
        let x = w / 2.0 - size / 2.0;
        for spread in (1..=3).rev() {
            let spread = spread as f32 * 1.5;
            screen.round(
                x - spread,
                top - spread + 3.0,
                size + 2.0 * spread,
                size + 2.0 * spread,
                9.0 + spread,
                Color { a: 0x26, r: 0, g: 0, b: 0 },
            );
        }
        self.draw_icon(screen, game, x, top, size, 1.0);
        screen.outline(x - 3.0, top - 3.0, size + 6.0, size + 6.0, 11.0, 1.5, WHITE);
        self.hit(x - 3.0, top - 3.0, size + 6.0, size + 6.0, Hit::Game(current));
    }

    fn draw_icon(&self, screen: &mut Screen, game: &Path, x: f32, y: f32, size: f32, bright: f32) {
        match self.info.get(game).and_then(|info| info.icon.as_ref()) {
            Some(icon) => screen.picture(icon, x, y, size, size, size * 0.11, bright),
            None => {
                // A tile in a colour of the name's own, with its first letter.
                let title = name(game);
                let hash = title.bytes().fold(7u32, |hash, byte| hash.wrapping_mul(31).wrapping_add(byte as u32));
                let color = rgb((60 + hash % 100) as u8, (70 + (hash >> 8) % 100) as u8, (80 + (hash >> 16) % 100) as u8);
                screen.round(x, y, size, size, size * 0.11, scaled(color, bright));
                let letter: String = title.chars().take(1).collect();
                let style = Style {
                    size: size * 0.45,
                    line: size,
                    bold: true,
                };
                screen.text_styled(&letter, x + size / 2.0, y, style, TextAlignment::Center, scaled(WHITE, bright));
            }
        }
    }

    /// The carrier's badge and when the game was last played.
    fn draw_details(&self, screen: &mut Screen, game: &Path, centre: f32, y: f32) {
        let carrier = self.carrier(game).filter(|carrier| !carrier.is_empty());
        let file = game.file_name().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
        let detail = if self.added.iter().any(|added| added == game) {
            "새로 추가".to_owned()
        } else if let Some(at) = self.played.get(&file) {
            format!("마지막 플레이 · {}", ago(*at))
        } else {
            String::new()
        };
        let badge_width = carrier.map_or(0.0, |carrier| text_width(carrier, TINY_BOLD) + 8.0);
        let detail_width = text_width(&detail, TINY);
        let space = if badge_width > 0.0 && detail_width > 0.0 { 6.0 } else { 0.0 };
        let mut x = centre - (badge_width + space + detail_width) / 2.0;
        if let Some(carrier) = carrier {
            screen.round(x, y, badge_width, 12.0, 3.0, carrier_color(carrier));
            screen.text_styled(carrier, x + badge_width / 2.0, y, TINY_BOLD, TextAlignment::Center, WHITE);
            x += badge_width + space;
        }
        screen.text_styled(&detail, x, y, TINY, TextAlignment::Left, rgb(0xaa, 0xb4, 0xbb));
    }

    fn draw_keyboard(&mut self, screen: &mut Screen, mode: Mode, cursor: usize, w: f32, top: f32) {
        let panel_width = (w - 8.0).min(312.0);
        let keys_width = panel_width - 12.0;
        let x0 = (w - panel_width) / 2.0;
        screen.round(x0, top, panel_width, search::height(mode) + 12.0, 8.0, rgb(0x15, 0x1a, 0x20));
        let (left, keys_top) = (x0 + 6.0, top + 6.0);
        for (index, key) in search::keys(mode, keys_width).iter().enumerate() {
            let (x, y) = (left + key.x, keys_top + key.y);
            let on = index == cursor;
            let (fill, ink) = match (&key.action, on) {
                (_, true) => (WHITE, INK),
                (Action::Done, false) => (GREEN, WHITE),
                (Action::Type(_), false) => (rgb(0x26, 0x2e, 0x36), rgb(0xe6, 0xeb, 0xee)),
                _ => (rgb(0x1f, 0x26, 0x2d), rgb(0xb9, 0xc2, 0xc8)),
            };
            screen.round(x, y, key.width, key.height, 5.0, fill);
            let style = if matches!(key.action, Action::Type(_)) { SMALL } else { TINY_BOLD };
            screen.text_styled(
                key.label,
                x + key.width / 2.0,
                y + (key.height - style.line) / 2.0,
                style,
                TextAlignment::Center,
                ink,
            );
            self.hit(x, y, key.width, key.height, Hit::Key(index));
        }
    }

    fn draw_empty(&mut self, screen: &mut Screen) {
        let (w, h) = (screen.width() as f32, screen.height() as f32);
        screen.gradient(rgb(0x1e, 0x2a, 0x30), rgb(0x0a, 0x0c, 0x10));
        screen.text_styled("MiniMobile", 10.0, 5.0, SMALL, TextAlignment::Left, TEXT);
        screen.text_styled("게임이 없습니다", w / 2.0, HEADER + 18.0, TITLE, TextAlignment::Center, TEXT);
        if self.desktop {
            screen.paragraph(
                "게임 파일(.zip, .jar)을\n이 창에 끌어다 놓으면 추가됩니다.\n\n또는 games 폴더에 넣으세요.",
                (HEADER + 48.0) as i32,
                MUTED,
            );
            let label = "F2  games 폴더 열기";
            let button_width = text_width(label, SMALL) + 24.0;
            let (x, y) = ((w - button_width) / 2.0, HEADER + 48.0 + 5.0 * LINE as f32 + 6.0);
            screen.round(x, y, button_width, 20.0, 10.0, GREEN);
            screen.text_styled(label, w / 2.0, y + 3.5, SMALL, TextAlignment::Center, WHITE);
            self.hit(x, y, button_width, 20.0, Hit::Folder);
        } else {
            let text = format!(
                "아래 폴더에 게임 파일\n(.zip, .jar)을 넣어 주세요.\n\n{}",
                self.folder.canonicalize().unwrap_or_else(|_| self.folder.clone()).display()
            );
            screen.paragraph(&text, (HEADER + 48.0) as i32, MUTED);
        }
        if let Some((status, _)) = &self.status {
            screen.toast(status, FOOTER as i32 + 4);
        }
        let (left, right) = if self.desktop {
            ("F2 폴더 열기", "Esc 메뉴")
        } else {
            ("Y 메뉴", "SELECT+START 종료")
        };
        screen.bar((h - FOOTER) as i32, left, right);
    }
}

/// The title centred at `centre`, `y`, no wider than `room`, with the letters
/// at `marks` in the accent colour and underlined.
fn draw_title(screen: &mut Screen, title: &str, marks: &[usize], centre: f32, y: f32, room: f32, style: Style) {
    let shown = fit_with(title, room, style);
    let mut x = centre - text_width(&shown, style) / 2.0;
    for (index, c) in shown.chars().enumerate() {
        let letter = c.to_string();
        let marked = marks.contains(&index) && !(shown.ends_with('…') && index + 1 == shown.chars().count());
        let width = screen.text_styled(&letter, x, y, style, TextAlignment::Left, if marked { ACCENT } else { WHITE });
        if marked {
            screen.round(x, y + style.line + 0.5, width, 1.2, 0.5, ACCENT);
        }
        x += width;
    }
}

/// Where the picked game is among `count`: a dot each, or "3 / 42" for many.
fn draw_position(screen: &mut Screen, count: usize, current: usize, centre: f32, y: f32) {
    if count <= 1 {
        return;
    }
    if count > 15 {
        screen.text_styled(&format!("{} / {count}", current + 1), centre, y - 3.0, TINY, TextAlignment::Center, MUTED);
        return;
    }
    let left = centre - (count as f32 * 8.0) / 2.0;
    for index in 0..count {
        let x = left + index as f32 * 8.0;
        if index == current {
            screen.round(x - 2.0, y, 9.0, 3.0, 1.5, WHITE);
        } else {
            screen.round(x, y, 3.0, 3.0, 1.5, rgb(0x5a, 0x64, 0x6c));
        }
    }
}

fn draw_magnifier(screen: &mut Screen, x: f32, y: f32, color: Color) {
    screen.outline(x, y, 6.5, 6.5, 3.25, 1.2, color);
    for step in 0..4 {
        let at = 5.6 + step as f32 * 0.9;
        screen.round(x + at, y + at, 1.6, 1.6, 0.8, color);
    }
}

fn carrier_color(carrier: &str) -> Color {
    match carrier {
        "KTF" => rgb(0x2b, 0x6f, 0xd6),
        "LGT" => rgb(0xc2, 0x3a, 0xa8),
        "SKT" => rgb(0xe0, 0x60, 0x1f),
        "DRM" => rgb(0x99, 0x39, 0x39),
        _ => rgb(0x64, 0x75, 0x68),
    }
}

/// `color` times `by`.
fn scaled(color: Color, by: f32) -> Color {
    Color {
        a: color.a,
        r: (color.r as f32 * by) as u8,
        g: (color.g as f32 * by) as u8,
        b: (color.b as f32 * by) as u8,
    }
}

/// 방금 전, 5분 전, 9시간 전, 어제, 12일 전, 1년 전.
fn ago(at: u64) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |x| x.as_secs());
    let elapsed = now.saturating_sub(at);
    match elapsed {
        0..60 => "방금 전".to_owned(),
        60..3600 => format!("{}분 전", elapsed / 60),
        3600..86400 => format!("{}시간 전", elapsed / 3600),
        86400..172800 => "어제".to_owned(),
        _ if elapsed < 365 * 86400 => format!("{}일 전", elapsed / 86400),
        _ => format!("{}년 전", elapsed / (365 * 86400)),
    }
}

/// Reads the carrier and icon of each of `games` on a thread of its own,
/// sending each as it is done.
fn read_infos(games: Vec<PathBuf>) -> mpsc::Receiver<(PathBuf, Info)> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut cache = CarrierCache::load();
        for game in games {
            let carrier = match cache.get(&game) {
                Some(carrier) => carrier,
                None => {
                    let carrier = std::fs::read(&game).map(|data| host::carrier(&data)).unwrap_or_default();
                    cache.put(&game, &carrier);
                    carrier
                }
            };
            let icon = icons::extract(&game);
            if sender.send((game, Info { carrier, icon })).is_err() {
                break;
            }
        }
        cache.save();
    });
    receiver
}

/// The carriers found before, by file name, size and time written, so a
/// game's whole file is read for it once.
struct CarrierCache {
    entries: HashMap<String, (u64, u64, String)>,
    changed: bool,
}

const CARRIER_CACHE: &str = "cache/carriers.txt";

impl CarrierCache {
    fn load() -> CarrierCache {
        let entries = std::fs::read_to_string(CARRIER_CACHE)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| {
                let fields: Vec<&str> = line.split('\t').collect();
                match fields.as_slice() {
                    [file, size, written, carrier] => Some(((*file).to_owned(), (size.parse().ok()?, written.parse().ok()?, (*carrier).to_owned()))),
                    _ => None,
                }
            })
            .collect();
        CarrierCache { entries, changed: false }
    }

    fn stamp(game: &Path) -> Option<(String, u64, u64)> {
        let metadata = game.metadata().ok()?;
        let written = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs();
        Some((game.file_name()?.to_string_lossy().into_owned(), metadata.len(), written))
    }

    fn get(&self, game: &Path) -> Option<String> {
        let (file, size, written) = Self::stamp(game)?;
        self.entries
            .get(&file)
            .filter(|(kept_size, kept_written, _)| *kept_size == size && *kept_written == written)
            .map(|(_, _, carrier)| carrier.clone())
    }

    fn put(&mut self, game: &Path, carrier: &str) {
        if let Some((file, size, written)) = Self::stamp(game) {
            self.entries.insert(file, (size, written, carrier.to_owned()));
            self.changed = true;
        }
    }

    fn save(&self) {
        if !self.changed {
            return;
        }
        let mut text = String::new();
        for (file, (size, written, carrier)) in &self.entries {
            text.push_str(&format!("{file}\t{size}\t{written}\t{carrier}\n"));
        }
        let _ = std::fs::create_dir_all("cache");
        let _ = std::fs::write(CARRIER_CACHE, text);
    }
}

/// A screen of `text`, centred, with `hint` in the bar under it.
pub fn draw_message(text: &str, hint: &str, width: u32, height: u32) -> Vec<u8> {
    let mut screen = Screen::new(width, height);
    screen.gradient(rgb(0x1a, 0x22, 0x28), rgb(0x0a, 0x0c, 0x10));
    let lines = wrap(text, width as f32 - 24.0);
    let top = ((height as i32 - lines.len() as i32 * LINE) / 2).max(4);
    for (index, line) in lines.iter().enumerate() {
        screen.text(line, width as i32 / 2, top + index as i32 * LINE, TextAlignment::Center, TEXT);
    }
    if !hint.is_empty() {
        screen.bar(height as i32 - FOOTER as i32, hint, "");
    }
    screen.rgba()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_admit_their_carriers() {
        assert!(Tab::All.admits(None));
        assert!(!Tab::Ktf.admits(None));
        assert!(Tab::Ktf.admits(Some("KTF")));
        assert!(Tab::Other.admits(Some("DRM")));
        assert!(!Tab::Other.admits(Some("LGT")));
    }

    #[test]
    fn times_read_as_how_long_ago() {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        assert_eq!(ago(now), "방금 전");
        assert_eq!(ago(now - 9 * 3600), "9시간 전");
        assert_eq!(ago(now - 30 * 3600), "어제");
        assert_eq!(ago(now - 12 * 86400), "12일 전");
    }
}
