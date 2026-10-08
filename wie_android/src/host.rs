//! The emulator as a host other than the Android app drives it.
//!
//! These are the calls the JNI bridge in `lib.rs` makes, as plain Rust: the iOS
//! app reaches them through `wie_ios`'s C functions. The runner behind them is
//! the same one, so a title loads, runs, saves and sounds the way it does on
//! Android.

use std::{panic::AssertUnwindSafe, path::PathBuf, time::Duration};

use crate::{
    logging,
    platform::AndroidHandsetInformation,
    runner::{self, with_runner},
    speed,
};

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
    let frame = std::panic::catch_unwind(AssertUnwindSafe(|| with_runner(|runner| runner.take_frame()))).ok()??;

    let mut rgba = Vec::with_capacity(frame.pixels.len() * 4);
    for &pixel in &frame.pixels {
        let pixel = pixel as u16;
        let (r, g, b) = ((pixel >> 11) & 0x1f, (pixel >> 5) & 0x3f, pixel & 0x1f);
        rgba.extend_from_slice(&[(r * 255 / 31) as u8, (g * 255 / 63) as u8, (b * 255 / 31) as u8, 0xff]);
    }

    Some((frame.width, frame.height, rgba))
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
