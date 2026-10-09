//! A game's saves as files the player handles: exported to and imported from
//! zips in the `saves` folder, erased, and the game itself deleted.
//!
//! The zips are the Android app's save zips (see `wie_android::host`), so a
//! save goes between the phone, the handheld and the PC by copying the file.
//! Each export is a new file named for when it was taken, so older ones stay.
//! On Windows what is removed goes to the recycle bin.

use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use wie_android::host;

/// Where exports are written and imports are looked for.
pub const SAVES_DIR: &str = "saves";

/// Where the runner keeps every title's saves.
pub fn runtime_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_default().join("data")
}

/// What the list calls a game: its file name without the extension.
pub fn game_name(game: &Path) -> String {
    game.file_stem().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default()
}

fn read_game(game: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(game).map_err(|error| format!("게임 파일을 읽을 수 없습니다: {error}"))
}

/// How much a game has saved.
pub struct Summary {
    pub files: usize,
    /// When the newest of them was written.
    pub latest: Option<SystemTime>,
}

pub fn summary(game: &Path) -> Result<Summary, String> {
    let files = host::save_files(&read_game(game)?, &runtime_dir())?;
    let latest = files.iter().filter_map(|path| path.metadata().and_then(|x| x.modified()).ok()).max();
    Ok(Summary { files: files.len(), latest })
}

/// Writes the game's saves to a new zip in the saves folder, named for the
/// game and the time, with `note` after them. The file written, or `None`
/// when the game has saved nothing.
pub fn export(game: &Path, note: &str) -> Result<Option<PathBuf>, String> {
    let Some(zip) = host::export_save(&read_game(game)?, &runtime_dir())? else {
        return Ok(None);
    };
    std::fs::create_dir_all(SAVES_DIR).map_err(|error| format!("saves 폴더를 만들 수 없습니다: {error}"))?;
    let base = format!("{} 세이브 {}{note}", file_safe(&game_name(game)), local(SystemTime::now()).file_label());
    let path = std::iter::once(base.clone())
        .chain((2..).map(|n| format!("{base} ({n})")))
        .map(|name| Path::new(SAVES_DIR).join(format!("{name}.zip")))
        .find(|path| !path.exists())
        .unwrap();
    std::fs::write(&path, zip).map_err(|error| format!("세이브를 쓸 수 없습니다: {error}"))?;
    Ok(Some(path))
}

/// A save zip in the saves folder.
pub struct SaveZip {
    pub path: PathBuf,
    pub name: String,
    pub modified: Option<SystemTime>,
    /// Whether it holds the game's own saves rather than another's.
    pub ours: bool,
}

/// The save zips in the saves folder: the game's own first, then the rest,
/// each newest first. Zips that are not saves are left out.
pub fn list(game: &Path) -> Vec<SaveZip> {
    let data = read_game(game).unwrap_or_default();
    let mut zips: Vec<SaveZip> = std::fs::read_dir(SAVES_DIR)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|path| path.is_file() && path.extension().is_some_and(|x| x.eq_ignore_ascii_case("zip")))
                .filter_map(|path| {
                    let ours = host::save_zip_belongs(&std::fs::read(&path).ok()?, &data)?;
                    Some(SaveZip {
                        name: game_name(&path),
                        modified: path.metadata().and_then(|x| x.modified()).ok(),
                        path,
                        ours,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    zips.sort_by(|a, b| b.ours.cmp(&a.ours).then(b.modified.cmp(&a.modified)));
    zips
}

/// Puts a save zip's saves in place. For the game's own zip, the save it
/// replaces is exported first; that file is returned with the count.
pub fn import(game: &Path, zip: &SaveZip) -> Result<(usize, Option<PathBuf>), String> {
    let data = std::fs::read(&zip.path).map_err(|error| format!("세이브 파일을 읽을 수 없습니다: {error}"))?;
    let backup = if zip.ours { export(game, " (가져오기 전)")? } else { None };
    let restored = host::import_save(&data, &runtime_dir())?;
    Ok((restored, backup))
}

/// Removes the game's saves. How many folders went.
pub fn erase(game: &Path) -> Result<usize, String> {
    let dirs = host::save_dirs(&read_game(game)?, &runtime_dir())?;
    for dir in &dirs {
        remove(dir).map_err(|error| format!("세이브를 지울 수 없습니다: {error}"))?;
    }
    Ok(dirs.len())
}

/// Removes the game's file, and its saves too if `with_saves`.
pub fn delete_game(game: &Path, with_saves: bool) -> Result<(), String> {
    if with_saves {
        erase(game)?;
    }
    remove(game).map_err(|error| format!("게임 파일을 지울 수 없습니다: {error}"))
}

/// Whether a removed file can be got back.
pub const RECOVERABLE: bool = cfg!(windows);

/// Removes a file or a folder: to the recycle bin on Windows, for good
/// elsewhere.
#[cfg(not(windows))]
fn remove(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

#[cfg(windows)]
fn remove(path: &Path) -> std::io::Result<()> {
    use std::{ffi::c_void, os::windows::ffi::OsStrExt};

    /// `SHFILEOPSTRUCTW`, as laid out on 64-bit Windows.
    #[repr(C)]
    struct FileOp {
        window: *mut c_void,
        function: u32,
        from: *const u16,
        to: *const u16,
        flags: u16,
        aborted: i32,
        name_mappings: *mut c_void,
        progress_title: *const u16,
    }

    #[link(name = "shell32")]
    unsafe extern "system" {
        fn SHFileOperationW(op: *mut FileOp) -> i32;
    }

    const FO_DELETE: u32 = 3;
    const FOF_SILENT: u16 = 0x0004;
    const FOF_NOCONFIRMATION: u16 = 0x0010;
    const FOF_ALLOWUNDO: u16 = 0x0040;
    const FOF_NOERRORUI: u16 = 0x0400;

    // The shell wants a full path - not a `\\?\` one - ending in two nuls.
    let full = std::path::absolute(path)?;
    let from: Vec<u16> = full.as_os_str().encode_wide().chain([0, 0]).collect();
    let mut op = FileOp {
        window: std::ptr::null_mut(),
        function: FO_DELETE,
        from: from.as_ptr(),
        to: std::ptr::null(),
        flags: FOF_SILENT | FOF_NOCONFIRMATION | FOF_ALLOWUNDO | FOF_NOERRORUI,
        aborted: 0,
        name_mappings: std::ptr::null_mut(),
        progress_title: std::ptr::null(),
    };
    // SAFETY: a filled-in SHFILEOPSTRUCTW whose path outlives the call.
    let result = unsafe { SHFileOperationW(&mut op) };
    if result != 0 || op.aborted != 0 {
        return Err(std::io::Error::other(format!("휴지통으로 옮길 수 없습니다 (코드 {result:#x})")));
    }
    Ok(())
}

/// `bytes` as the list shows a file's size.
pub fn size_label(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{}KB", bytes.div_ceil(1024))
    }
}

/// A moment on the local clock, to the minute.
#[derive(Debug, PartialEq)]
pub struct Stamp {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
}

impl Stamp {
    /// `2026-10-09 21.14`: sorts by time, and has nothing a file name cannot.
    pub fn file_label(&self) -> String {
        format!("{}-{:02}-{:02} {:02}.{:02}", self.year, self.month, self.day, self.hour, self.minute)
    }

    /// `10/09 21:14`.
    pub fn short_label(&self) -> String {
        format!("{:02}/{:02} {:02}:{:02}", self.month, self.day, self.hour, self.minute)
    }

    /// `10월 9일 21:14`.
    pub fn long_label(&self) -> String {
        format!("{}월 {}일 {:02}:{:02}", self.month, self.day, self.hour, self.minute)
    }
}

/// `time` on the local clock.
pub fn local(time: SystemTime) -> Stamp {
    let seconds = time.duration_since(UNIX_EPOCH).map_or(0, |x| x.as_secs()) as libc::time_t;
    // SAFETY: an all-zero `tm` is a valid one to be filled in.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: both pointers are to live values of the types asked for.
    #[cfg(unix)]
    let converted = unsafe { !libc::localtime_r(&seconds, &mut tm).is_null() };
    #[cfg(windows)]
    let converted = unsafe { libc::localtime_s(&mut tm, &seconds) == 0 };
    if !converted {
        return utc(seconds as i64);
    }
    Stamp {
        year: tm.tm_year + 1900,
        month: (tm.tm_mon + 1) as u32,
        day: tm.tm_mday as u32,
        hour: tm.tm_hour as u32,
        minute: tm.tm_min as u32,
    }
}

/// Seconds since 1970 on the UTC calendar, for when the local clock cannot be
/// read.
fn utc(seconds: i64) -> Stamp {
    let days = seconds.div_euclid(86_400);
    let of_day = seconds.rem_euclid(86_400);
    // Howard Hinnant's days-to-civil.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
    Stamp {
        year,
        month,
        day,
        hour: (of_day / 3600) as u32,
        minute: (of_day % 3600 / 60) as u32,
    }
}

/// `name` as something a file can be called on any of the systems.
fn file_safe(name: &str) -> String {
    let name: String = name
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let name = name.trim().trim_start_matches('.').trim();
    if name.is_empty() { "게임".to_owned() } else { name.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_dates_come_out_on_the_calendar() {
        assert_eq!(
            utc(0),
            Stamp {
                year: 1970,
                month: 1,
                day: 1,
                hour: 0,
                minute: 0
            }
        );
        // 2026-10-09 12:14:30 UTC, and a leap day.
        let stamp = utc(1_791_548_070);
        assert_eq!(stamp.file_label(), "2026-10-09 12.14");
        assert_eq!(stamp.short_label(), "10/09 12:14");
        assert_eq!(stamp.long_label(), "10월 9일 12:14");
        assert_eq!(utc(951_782_400).file_label(), "2000-02-29 00.00");
    }

    #[test]
    fn names_and_sizes_read_as_the_list_shows_them() {
        assert_eq!(file_safe("제노니아2: 새로운 시작?"), "제노니아2_ 새로운 시작_");
        assert_eq!(file_safe(".."), "게임");
        assert_eq!(size_label(1_468_006), "1.4MB");
        assert_eq!(size_label(300), "1KB");
    }
}
