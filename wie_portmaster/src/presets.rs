//! The live button mapping, the presets saved beside it, and which preset
//! each game is fixed to.
//!
//! The live mapping is `controls.txt`, the file the port has always read. A
//! preset is a controls file of its own under `presets/`, named after the game
//! it was saved from; loading one copies it into the live mapping. "기본" is
//! the defaults and is never a file, so it cannot be overwritten or lost.
//! `presets.txt` remembers which preset was loaded last, the presets games
//! are fixed to, and the speed and screen quality each game plays with.

use std::path::{Path, PathBuf};

use crate::controls::{Button, Controls};

pub const DEFAULT_NAME: &str = "기본";

/// The largest window, as a multiple of 320x240.
pub const SCREEN_MAX: u32 = 4;

const CONTROLS_FILE: &str = "controls.txt";
const PRESET_DIR: &str = "presets";
const STATE_FILE: &str = "presets.txt";

pub struct Store {
    controls: Controls,
    /// The preset last loaded or saved, which the live mapping started from.
    active: String,
    /// Game file name, and the preset loaded whenever it starts.
    fixed: Vec<(String, String)>,
    /// Game file name, and the speed it plays at when that is not 1x.
    speeds: Vec<(String, f32)>,
    /// Game file name, and how its screen is enlarged when that is not 도트:
    /// 0 기본, 2 HQ2X, as the Android app numbers them.
    qualities: Vec<(String, u8)>,
    /// How a desktop window is shown: its size as a multiple of 320x240, or
    /// 0 for the full screen.
    screen: u32,
}

impl Store {
    pub fn load() -> Store {
        let mut store = Store {
            controls: Controls::load(Path::new(CONTROLS_FILE)),
            active: DEFAULT_NAME.to_owned(),
            fixed: Vec::new(),
            speeds: Vec::new(),
            qualities: Vec::new(),
            screen: 3,
        };
        if let Ok(text) = std::fs::read_to_string(STATE_FILE) {
            for line in text.lines() {
                let fields: Vec<&str> = line.split('\t').collect();
                match fields.as_slice() {
                    ["active", name] => store.active = (*name).to_owned(),
                    ["game", game, name] => store.fixed.push(((*game).to_owned(), (*name).to_owned())),
                    ["speed", game, speed] => {
                        if let Ok(speed) = speed.parse::<f32>()
                            && speed.is_finite()
                            && speed > 0.0
                        {
                            store.speeds.push(((*game).to_owned(), speed));
                        }
                    }
                    ["quality", game, quality] => {
                        if let Ok(quality) = quality.parse::<u8>()
                            && quality <= 2
                        {
                            store.qualities.push(((*game).to_owned(), quality));
                        }
                    }
                    ["screen", screen] => store.screen = screen.parse().unwrap_or(3).min(SCREEN_MAX),
                    _ => {}
                }
            }
        }
        store
    }

    pub fn controls(&self) -> &Controls {
        &self.controls
    }

    pub fn active(&self) -> &str {
        &self.active
    }

    /// Whether the live mapping is no longer the preset it started from.
    pub fn modified(&self) -> bool {
        self.preset(&self.active).is_none_or(|preset| preset != self.controls)
    }

    /// Every preset: the defaults first, then the saved ones by name.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(PRESET_DIR)
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                    .filter(|path| path.extension().is_some_and(|x| x == "txt"))
                    .filter_map(|path| path.file_stem().map(|x| x.to_string_lossy().into_owned()))
                    .filter(|name| name != DEFAULT_NAME)
                    .collect()
            })
            .unwrap_or_default();
        names.sort_by_key(|name| name.to_lowercase());
        names.insert(0, DEFAULT_NAME.to_owned());
        names
    }

    fn preset(&self, name: &str) -> Option<Controls> {
        if name == DEFAULT_NAME {
            return Some(Controls::parse(""));
        }
        std::fs::read_to_string(preset_path(name)).ok().map(|text| Controls::parse(&text))
    }

    /// Makes `name` the live mapping. Whether there was such a preset.
    pub fn apply(&mut self, name: &str) -> bool {
        let Some(preset) = self.preset(name) else {
            return false;
        };
        self.controls = preset;
        self.active = name.to_owned();
        self.save();
        true
    }

    /// Saves the live mapping as a new preset named after `base`, and returns
    /// the name it got.
    pub fn save_new(&mut self, base: &str) -> String {
        let base = file_safe(base);
        let names = self.names();
        let name = std::iter::once(base.clone())
            .chain((2..).map(|n| format!("{base} {n}")))
            .find(|name| !names.iter().any(|x| x == name))
            .unwrap();
        self.write_preset(&name);
        self.active = name.clone();
        self.save();
        name
    }

    /// Writes the live mapping over `name`. False for the defaults.
    pub fn overwrite(&mut self, name: &str) -> bool {
        if name == DEFAULT_NAME {
            return false;
        }
        self.write_preset(name);
        self.active = name.to_owned();
        self.save();
        true
    }

    /// Deletes `name`, unfixing any game fixed to it. False for the defaults.
    pub fn delete(&mut self, name: &str) -> bool {
        if name == DEFAULT_NAME {
            return false;
        }
        if let Err(error) = std::fs::remove_file(preset_path(name)) {
            eprintln!("프리셋을 지울 수 없습니다: {error}");
            return false;
        }
        self.fixed.retain(|(_, preset)| preset != name);
        if self.active == name {
            self.active = DEFAULT_NAME.to_owned();
        }
        self.save();
        true
    }

    /// Puts a keyboard key on a handset key in the live mapping. The handset
    /// key it was taken from, if one.
    pub fn set_keyboard(&mut self, handset: i32, slot: usize, scancode: Option<i32>) -> Option<i32> {
        let moved_from = self.controls.set_keyboard(handset, slot, scancode);
        self.save();
        moved_from
    }

    pub fn screen(&self) -> u32 {
        self.screen
    }

    pub fn set_screen(&mut self, screen: u32) {
        self.screen = screen.min(SCREEN_MAX);
        self.save();
    }

    /// Changes one button in the live mapping.
    pub fn set(&mut self, button: Button, with_select: bool, key: Option<i32>) {
        self.controls.set(button, with_select, key);
        self.save();
    }

    /// The preset `game` is fixed to.
    pub fn fixed(&self, game: &str) -> Option<&str> {
        self.fixed.iter().find(|(x, _)| x == game).map(|(_, name)| name.as_str())
    }

    pub fn set_fixed(&mut self, game: &str, preset: Option<&str>) {
        self.fixed.retain(|(x, _)| x != game);
        if let Some(preset) = preset {
            self.fixed.push((game.to_owned(), preset.to_owned()));
        }
        self.save();
    }

    /// The speed `game` plays at.
    pub fn speed(&self, game: &str) -> f32 {
        self.speeds.iter().find(|(x, _)| x == game).map_or(1.0, |(_, speed)| *speed)
    }

    pub fn set_speed(&mut self, game: &str, speed: f32) {
        self.speeds.retain(|(x, _)| x != game);
        if speed != 1.0 {
            self.speeds.push((game.to_owned(), speed));
        }
        self.save();
    }

    /// How `game`'s screen is enlarged: 0 기본, 1 도트, 2 HQ2X.
    pub fn quality(&self, game: &str) -> u8 {
        self.qualities.iter().find(|(x, _)| x == game).map_or(1, |(_, quality)| *quality)
    }

    pub fn set_quality(&mut self, game: &str, quality: u8) {
        self.qualities.retain(|(x, _)| x != game);
        if quality != 1 {
            self.qualities.push((game.to_owned(), quality));
        }
        self.save();
    }

    /// Drops what is kept for a game that is gone.
    pub fn forget(&mut self, game: &str) {
        self.fixed.retain(|(x, _)| x != game);
        self.speeds.retain(|(x, _)| x != game);
        self.qualities.retain(|(x, _)| x != game);
        self.save();
    }

    fn write_preset(&self, name: &str) {
        let _ = std::fs::create_dir_all(PRESET_DIR);
        if let Err(error) = std::fs::write(preset_path(name), self.controls.to_text()) {
            eprintln!("프리셋을 저장할 수 없습니다: {error}");
        }
    }

    fn save(&self) {
        if let Err(error) = std::fs::write(CONTROLS_FILE, self.controls.to_text()) {
            eprintln!("버튼 설정을 저장할 수 없습니다: {error}");
        }
        let mut state = format!(
            "# MiniMobile 프리셋 상태 - 설정 화면이 씁니다.\nactive\t{}\nscreen\t{}\n",
            self.active, self.screen
        );
        for (game, preset) in &self.fixed {
            state.push_str(&format!("game\t{game}\t{preset}\n"));
        }
        for (game, speed) in &self.speeds {
            state.push_str(&format!("speed\t{game}\t{speed}\n"));
        }
        for (game, quality) in &self.qualities {
            state.push_str(&format!("quality\t{game}\t{quality}\n"));
        }
        if let Err(error) = std::fs::write(STATE_FILE, state) {
            eprintln!("프리셋 상태를 저장할 수 없습니다: {error}");
        }
    }
}

fn preset_path(name: &str) -> PathBuf {
    Path::new(PRESET_DIR).join(format!("{name}.txt"))
}

/// `name` as something a file can be called: no path separators or tabs, no
/// leading dot, and something rather than nothing.
fn file_safe(name: &str) -> String {
    let name: String = name
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '\t' | '\n' | '\r' | '\0') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let name = name.trim().trim_start_matches('.').trim();
    if name.is_empty() { "프리셋".to_owned() } else { name.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_safe_for_files() {
        assert_eq!(file_safe("레전드 오브 마스터"), "레전드 오브 마스터");
        assert_eq!(file_safe("a/b\\c"), "a_b_c");
        assert_eq!(file_safe(" ..숨김"), "숨김");
        assert_eq!(file_safe(""), "프리셋");
    }
}
