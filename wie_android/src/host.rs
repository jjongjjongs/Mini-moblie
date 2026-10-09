//! The emulator as a host other than the Android app drives it.
//!
//! These are the calls the JNI bridge in `lib.rs` makes, as plain Rust: the iOS
//! app reaches them through `wie_ios`'s C functions. The runner behind them is
//! the same one, so a title loads, runs, saves and sounds the way it does on
//! Android.

use std::{
    io::{Read, Write},
    panic::AssertUnwindSafe,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use crate::{
    logging,
    platform::{AndroidHandsetInformation, Frame},
    runner::{self, with_runner},
    speed,
};

/// The last frame a host took, as the title drew it, for [`show_frame_again`]
/// and [`last_frame_rgba`].
static LAST_FRAME: Mutex<Option<Frame>> = Mutex::new(None);
/// The next take hands over [`LAST_FRAME`] again if nothing newer is painted.
static FRAME_AGAIN: AtomicBool = AtomicBool::new(false);

/// Upper bound on one [`tick`], whatever the host asks for, as for Android.
const MAX_TICK_BUDGET: Duration = Duration::from_millis(200);

/// Runs `f`, turning a panic into a stopped emulator and a message rather than
/// an unwind into the host.
fn guarded(f: impl FnOnce() -> String) -> String {
    std::panic::catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| {
        let message = "에뮬레이터 내부 오류가 발생했습니다.";
        tracing::error!("{message}");
        let _ = std::panic::catch_unwind(AssertUnwindSafe(runner::request_stop));
        message.to_owned()
    })
}

/// Loads `data` - a handset archive or a jar - and starts it, keeping its
/// files under `runtime_dir`. `model` is the handset model the title is told.
/// Returns an empty string on success, otherwise the message to show.
pub fn start(data: Vec<u8>, runtime_dir: PathBuf, model: String) -> String {
    logging::init();
    logging::reset();

    tracing::info!("host start: {} bytes, runtime dir {}", data.len(), runtime_dir.display());
    speed::realign();
    *LAST_FRAME.lock().unwrap_or_else(|x| x.into_inner()) = None;

    let handset_information = AndroidHandsetInformation::new(model);
    guarded(|| with_runner(|runner| runner.start(data, runtime_dir, handset_information)))
}

/// Runs the emulator for up to `budget_ms`. Returns an empty string while the
/// title is healthy, otherwise the message that stopped it.
pub fn tick(budget_ms: u32) -> String {
    let budget = Duration::from_millis(budget_ms as u64).min(MAX_TICK_BUDGET);
    guarded(|| with_runner(|runner| runner.tick(budget)))
}

/// Tears the emulator down. Safe to call when nothing is running.
pub fn stop() {
    let _ = std::panic::catch_unwind(AssertUnwindSafe(runner::request_stop));
}

pub fn running() -> bool {
    std::panic::catch_unwind(AssertUnwindSafe(runner::is_running)).unwrap_or(false)
}

/// What stopped the last run, if anything did.
pub fn last_error() -> String {
    guarded(runner::last_error)
}

/// How long the host may sleep before the title has work again, if it said.
pub fn sleep_hint_ms() -> Option<u64> {
    std::panic::catch_unwind(AssertUnwindSafe(|| with_runner(|runner| runner.sleep_hint()))).unwrap_or(None)
}

/// A key, by the Android keypad's index (see `runner::key_code`).
pub fn key(index: i32, pressed: bool) {
    let _ = std::panic::catch_unwind(AssertUnwindSafe(|| runner::key(index, pressed)));
}

/// The newest painted frame since the last call, as its width, height and
/// RGBA pixels, row by row.
pub fn take_frame_rgba() -> Option<(u32, u32, Vec<u8>)> {
    let frame = take_frame()?;

    Some((frame.width, frame.height, frame.rgba()))
}

/// [`take_frame_rgba`], doubled through hq2x: twice the width and height of
/// the title's screen, its edges smoothed.
pub fn take_frame_rgba_hq2x() -> Option<(u32, u32, Vec<u8>)> {
    let frame = take_frame()?;
    let frame = std::panic::catch_unwind(AssertUnwindSafe(|| frame.hq2x())).ok()?;

    Some((frame.width, frame.height, frame.rgba()))
}

/// Has the next take hand over the last frame again, even if the title paints
/// nothing new - so a change in how it is shown reaches a screen standing
/// still.
pub fn show_frame_again() {
    FRAME_AGAIN.store(true, Ordering::Relaxed);
}

/// The last frame taken, as the title drew it, without taking anything.
pub fn last_frame_rgba() -> Option<(u32, u32, Vec<u8>)> {
    let last = LAST_FRAME.lock().unwrap_or_else(|x| x.into_inner());
    let frame = last.as_ref()?;

    Some((frame.width, frame.height, frame.rgba()))
}

/// `rgba`, `width` by `height`, doubled through hq2x. The pixels are taken as
/// the RGB565 colours the frames are made of.
pub fn hq2x_rgba(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    if rgba.len() != (width * height * 4) as usize {
        return None;
    }
    let pixels = rgba
        .chunks_exact(4)
        .map(|x| ((u16::from(x[0]) >> 3) << 11 | (u16::from(x[1]) >> 2) << 5 | u16::from(x[2]) >> 3) as i16)
        .collect();
    let frame = Frame { width, height, pixels };

    std::panic::catch_unwind(AssertUnwindSafe(|| frame.hq2x().rgba())).ok()
}

fn take_frame() -> Option<Frame> {
    let fresh = std::panic::catch_unwind(AssertUnwindSafe(|| with_runner(|runner| runner.take_frame()))).ok()?;
    let mut last = LAST_FRAME.lock().unwrap_or_else(|x| x.into_inner());
    match fresh {
        Some(frame) => {
            FRAME_AGAIN.store(false, Ordering::Relaxed);
            *last = Some(frame.clone());
            Some(frame)
        }
        None if FRAME_AGAIN.swap(false, Ordering::Relaxed) => last.clone(),
        None => None,
    }
}

/// Renders up to `frames` frames of the mixer's stereo 44.1kHz output as
/// interleaved little-endian sixteen-bit samples, or nothing while nothing
/// sounds. Called from the host's audio thread; it does not take the lock a
/// tick holds.
pub fn render_audio(frames: usize) -> Vec<u8> {
    std::panic::catch_unwind(AssertUnwindSafe(|| crate::audio::render_audio_bytes(frames))).unwrap_or_default()
}

/// The next queued output command - only vibration now; see `audio` - or
/// `None` when the queue is empty. A host that does not vibrate still drains it.
pub fn take_output() -> Option<Vec<u8>> {
    std::panic::catch_unwind(AssertUnwindSafe(|| with_runner(|runner| runner.take_audio()))).ok()?
}

/// The log collected for this run.
pub fn log() -> String {
    guarded(logging::snapshot)
}

/// The carrier `data` runs under - `"KTF"`, `"LGT"`, `"SKT"`, `"DRM"` for a
/// locked download, or `""` - for the library's badge and filter.
pub fn carrier(data: &[u8]) -> String {
    std::panic::catch_unwind(AssertUnwindSafe(|| runner::carrier(data).to_owned())).unwrap_or_default()
}

/// A touch on the screen, at `x`, `y` in the frame's pixels: `action` 0 for a
/// press, 1 for a release, 2 for a drag. Dropped unless touch is on.
pub fn pointer(action: i32, x: i32, y: i32) {
    let _ = std::panic::catch_unwind(AssertUnwindSafe(|| runner::pointer(action, x, y)));
}

/// Turns touches on the screen on or off for the running title.
pub fn set_touch(enabled: bool) {
    let _ = std::panic::catch_unwind(AssertUnwindSafe(|| runner::set_touch(enabled)));
}

pub fn touch() -> bool {
    wie_backend::touch_enabled()
}

/// How fast the title runs, 1.0 being real time.
pub fn set_speed(value: f32) {
    let _ = std::panic::catch_unwind(AssertUnwindSafe(|| speed::set_speed(value)));
}

pub fn speed() -> f32 {
    speed::speed()
}

/// Whether the sound plays one thing at a time - the music held back while an
/// effect plays - rather than everything mixed together.
pub fn set_one_sound_at_a_time(enabled: bool) {
    let _ = std::panic::catch_unwind(AssertUnwindSafe(|| crate::audio::set_one_at_a_time(enabled)));
}

/// Holds the title's clock still while `held` - for a pause menu over it -
/// so no time has passed for it when it goes on.
pub fn hold_clock(held: bool) {
    let _ = std::panic::catch_unwind(AssertUnwindSafe(|| speed::hold(held)));
}

// --- saves ---------------------------------------------------------------------
//
// The layout and the zip are the Android app's (`SaveExporter`/`SaveImporter`):
// a title's record stores live under `<runtime>/db/<product id>` and the files
// it wrote under `<runtime>/fs/<application id>`, and an export keeps that split
// in its entry paths. So a save taken on one host goes back on the other.

/// Every directory `data`'s saves live in, as its path inside the zip and on
/// disk.
fn save_roots(data: &[u8], runtime_dir: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let ids = runner::save_ids(data).ok_or_else(|| "이 파일의 저장 위치를 알 수 없습니다.".to_owned())?;

    let mut roots: Vec<(String, PathBuf)> = Vec::new();
    for (kind, id) in [("db", &ids.records), ("fs", &ids.files), ("fs", &ids.records)] {
        let name = format!("{kind}/{id}");
        let path = runtime_dir.join(kind).join(id);
        if path.is_dir() && !roots.iter().any(|(existing, _)| *existing == name) {
            roots.push((name, path));
        }
    }

    Ok(roots)
}

/// Every file under `dir`, with its path relative to `dir`, `/`-separated.
fn files_under(dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = format!("{prefix}/{}", entry.file_name().to_string_lossy());
        let path = entry.path();
        if path.is_dir() {
            files_under(&path, &name, out)?;
        } else {
            out.push((name, path));
        }
    }
    Ok(())
}

/// `data`'s saves as an Android-compatible save zip, or `None` when the title
/// has saved nothing.
pub fn export_save(data: &[u8], runtime_dir: &Path) -> Result<Option<Vec<u8>>, String> {
    let mut files = Vec::new();
    for (name, path) in save_roots(data, runtime_dir)? {
        files_under(&path, &name, &mut files).map_err(|error| format!("세이브를 읽을 수 없습니다: {error}"))?;
    }
    if files.is_empty() {
        return Ok(None);
    }

    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, path) in files {
        let contents = std::fs::read(&path).map_err(|error| format!("세이브를 읽을 수 없습니다: {error}"))?;
        zip.start_file(name, options)
            .map_err(|error| format!("세이브를 묶을 수 없습니다: {error}"))?;
        zip.write_all(&contents).map_err(|error| format!("세이브를 묶을 수 없습니다: {error}"))?;
    }
    let cursor = zip.finish().map_err(|error| format!("세이브를 묶을 수 없습니다: {error}"))?;

    Ok(Some(cursor.into_inner()))
}

/// Puts a save zip's `db/...` and `fs/...` entries back under `runtime_dir`,
/// overwriting what is there, and returns how many files it restored. Each
/// entry's path names the title it belongs to, so this needs no title.
pub fn import_save(zip_data: &[u8], runtime_dir: &Path) -> Result<usize, String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_data)).map_err(|error| format!("세이브 파일을 열 수 없습니다: {error}"))?;

    let mut restored = 0;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("세이브 파일을 읽을 수 없습니다: {error}"))?;
        if entry.is_dir() {
            continue;
        }
        // A zip is untrusted: only the save trees are restored, and nothing
        // that would leave them.
        let Some(relative) = entry.enclosed_name() else {
            return Err("세이브 파일에 잘못된 경로가 들어 있습니다.".to_owned());
        };
        if !(relative.starts_with("db") || relative.starts_with("fs")) || relative.components().count() < 3 {
            continue;
        }

        let target = runtime_dir.join(&relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| format!("폴더를 만들 수 없습니다: {error}"))?;
        }
        let mut contents = Vec::new();
        entry
            .read_to_end(&mut contents)
            .map_err(|error| format!("세이브 파일을 읽을 수 없습니다: {error}"))?;
        std::fs::write(&target, contents).map_err(|error| format!("세이브를 쓸 수 없습니다: {error}"))?;
        restored += 1;
    }

    if restored == 0 {
        return Err("세이브 데이터가 없는 파일입니다.".to_owned());
    }
    Ok(restored)
}

/// Every file `data` has saved, for a host to say whether there is a save and
/// when it was last written.
pub fn save_files(data: &[u8], runtime_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for (name, path) in save_roots(data, runtime_dir)? {
        files_under(&path, &name, &mut files).map_err(|error| format!("세이브를 읽을 수 없습니다: {error}"))?;
    }
    Ok(files.into_iter().map(|(_, path)| path).collect())
}

/// The directories `data`'s saves live in, for a host that removes them its
/// own way (to a recycle bin) rather than with [`erase_save`].
pub fn save_dirs(data: &[u8], runtime_dir: &Path) -> Result<Vec<PathBuf>, String> {
    Ok(save_roots(data, runtime_dir)?.into_iter().map(|(_, path)| path).collect())
}

/// What a save zip holds: `None` when it is no save zip at all, else whether
/// any of it is `data`'s - so a host can sort a folder of them by title.
pub fn save_zip_belongs(zip_data: &[u8], data: &[u8]) -> Option<bool> {
    let archive = zip::ZipArchive::new(std::io::Cursor::new(zip_data)).ok()?;
    let roots: Vec<String> = archive
        .file_names()
        .filter_map(|name| {
            let mut parts = name.split('/');
            match (parts.next(), parts.next(), parts.next()) {
                (Some(kind @ ("db" | "fs")), Some(id), Some(rest)) if !id.is_empty() && !rest.is_empty() => Some(format!("{kind}/{id}")),
                _ => None,
            }
        })
        .collect();
    if roots.is_empty() {
        return None;
    }
    let Some(ids) = runner::save_ids(data) else {
        return Some(false);
    };
    let ours = [format!("db/{}", ids.records), format!("fs/{}", ids.files), format!("fs/{}", ids.records)];
    Some(roots.iter().any(|root| ours.contains(root)))
}

/// Removes `data`'s saves, and returns how many directories it removed.
pub fn erase_save(data: &[u8], runtime_dir: &Path) -> Result<usize, String> {
    let roots = save_roots(data, runtime_dir)?;
    for (_, path) in &roots {
        std::fs::remove_dir_all(path).map_err(|error| format!("세이브를 지울 수 없습니다: {error}"))?;
    }
    Ok(roots.len())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{erase_save, export_save, import_save, save_dirs, save_files, save_zip_belongs};

    fn runtime_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wie-host-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A save taken out comes back byte for byte, under the paths the Android
    /// app uses, after the title's saves were erased.
    #[test]
    fn an_exported_save_imports_back_after_an_erase() {
        let archive = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../test_data/helloworld_ktf.zip")).unwrap();
        let ids = crate::runner::save_ids(&archive).expect("ids");
        let runtime = runtime_dir("roundtrip");

        let save = runtime.join("fs").join(&ids.files).join("save").join("slot.dat");
        std::fs::create_dir_all(save.parent().unwrap()).unwrap();
        std::fs::write(&save, b"progress").unwrap();
        let record = runtime.join("db").join(&ids.records).join("scores").join("1");
        std::fs::create_dir_all(record.parent().unwrap()).unwrap();
        std::fs::write(&record, b"1234").unwrap();

        assert_eq!(save_files(&archive, &runtime).unwrap().len(), 2);
        assert!(save_dirs(&archive, &runtime).unwrap().len() >= 2);
        let zip = export_save(&archive, &runtime).unwrap().expect("something saved");
        // The zip is told apart as this title's save, and a title's own file
        // as no save at all.
        assert_eq!(save_zip_belongs(&zip, &archive), Some(true));
        assert_eq!(save_zip_belongs(&archive, &archive), None);
        assert!(erase_save(&archive, &runtime).unwrap() >= 2);
        assert!(!save.exists() && !record.exists());

        assert_eq!(import_save(&zip, &runtime).unwrap(), 2);
        assert_eq!(std::fs::read(&save).unwrap(), b"progress");
        assert_eq!(std::fs::read(&record).unwrap(), b"1234");

        // Nothing saved is nothing to export.
        erase_save(&archive, &runtime).unwrap();
        assert!(export_save(&archive, &runtime).unwrap().is_none());
        assert!(save_files(&archive, &runtime).unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&runtime);
    }

    /// Another title's save is a save, but not this title's.
    #[test]
    fn a_save_zip_of_another_title_is_told_apart() {
        let archive = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../test_data/helloworld_ktf.zip")).unwrap();
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip.start_file("db/not-this-title/scores/1", zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut zip, b"1").unwrap();
        let data = zip.finish().unwrap().into_inner();

        assert_eq!(save_zip_belongs(&data, &archive), Some(false));
    }

    /// An entry outside the save trees, or one climbing out of them, is not
    /// written.
    #[test]
    fn an_import_writes_nothing_outside_the_save_trees() {
        let runtime = runtime_dir("traversal");
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("readme.txt", options).unwrap();
        std::io::Write::write_all(&mut zip, b"x").unwrap();
        let data = zip.finish().unwrap().into_inner();

        assert!(import_save(&data, &runtime).is_err());
        assert!(!runtime.join("readme.txt").exists());

        let _ = std::fs::remove_dir_all(&runtime);
    }
}
