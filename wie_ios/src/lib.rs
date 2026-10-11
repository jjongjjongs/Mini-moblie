//! The C functions the iOS app calls, declared in `include/wie.h`.
//!
//! Each is a thin wrapper over [`wie_android::host`], which drives the same
//! runner the Android app does; what is here is only the C side of it -
//! pointers, lengths and C strings - kept from unwinding into the caller.

use std::{
    ffi::{CStr, CString, c_char},
    path::PathBuf,
    sync::Mutex,
};

use wie_android::host;

/// A frame taken from the runner that did not fit the caller's buffer, kept
/// for the next call rather than dropped.
static PENDING_FRAME: Mutex<Option<(u32, u32, Vec<u8>)>> = Mutex::new(None);

/// The vibration opcode of the runner's output queue (see `wie_android`'s
/// `audio`): `[8, intensity, duration_ms: u64 LE]`.
const OPCODE_VIBRATE: u8 = 8;

fn message(text: String) -> *mut c_char {
    if text.is_empty() {
        return std::ptr::null_mut();
    }
    let text = text.replace('\0', "?");
    CString::new(text).map_or(std::ptr::null_mut(), CString::into_raw)
}

/// # Safety
/// `ptr` is NULL or a NUL-terminated string valid for the call.
unsafe fn string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    // SAFETY: as the caller promises.
    unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned()
}

/// # Safety
/// `data` points at `length` readable bytes; `runtime_dir` and `model` are
/// NUL-terminated strings (or NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wie_start(data: *const u8, length: usize, runtime_dir: *const c_char, model: *const c_char) -> *mut c_char {
    if data.is_null() {
        return message("게임 파일을 읽을 수 없습니다.".to_owned());
    }
    // SAFETY: as the caller promises.
    let data = unsafe { std::slice::from_raw_parts(data, length) }.to_vec();
    // SAFETY: as the caller promises.
    let (runtime_dir, model) = unsafe { (string(runtime_dir), string(model)) };

    if let Ok(mut pending) = PENDING_FRAME.lock() {
        *pending = None;
    }

    message(host::start(data, PathBuf::from(runtime_dir), model))
}

#[unsafe(no_mangle)]
pub extern "C" fn wie_tick(budget_ms: u32) -> *mut c_char {
    message(host::tick(budget_ms))
}

#[unsafe(no_mangle)]
pub extern "C" fn wie_stop() {
    host::stop();
}

#[unsafe(no_mangle)]
pub extern "C" fn wie_running() -> bool {
    host::running()
}

#[unsafe(no_mangle)]
pub extern "C" fn wie_last_error() -> *mut c_char {
    message(host::last_error())
}

#[unsafe(no_mangle)]
pub extern "C" fn wie_sleep_hint_ms() -> i64 {
    host::sleep_hint_ms().map_or(-1, |ms| ms.min(i64::MAX as u64) as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn wie_key(index: i32, pressed: bool) {
    host::key(index, pressed);
}

/// # Safety
/// `rgba` points at `capacity` writable bytes; `width` and `height` are valid
/// for writes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wie_take_frame(rgba: *mut u8, capacity: usize, width: *mut u32, height: *mut u32) -> bool {
    // SAFETY: as the caller promises.
    unsafe { take_frame(host::take_frame_rgba, rgba, capacity, width, height) }
}

/// [`wie_take_frame`], the frame doubled through hq2x: twice the title's
/// width and height, its edges smoothed.
///
/// # Safety
/// As [`wie_take_frame`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wie_take_frame_hq2x(rgba: *mut u8, capacity: usize, width: *mut u32, height: *mut u32) -> bool {
    // SAFETY: as the caller promises.
    unsafe { take_frame(host::take_frame_rgba_hq2x, rgba, capacity, width, height) }
}

/// Has the next take hand over the last frame again, though the title paints
/// nothing new - for a change in how it is shown.
#[unsafe(no_mangle)]
pub extern "C" fn wie_show_frame_again() {
    host::show_frame_again();
}

/// Copies the last frame taken, as the title drew it, into `rgba` without
/// taking anything. False when there is none or it does not fit.
///
/// # Safety
/// As [`wie_take_frame`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wie_last_frame(rgba: *mut u8, capacity: usize, width: *mut u32, height: *mut u32) -> bool {
    let Some((w, h, pixels)) = host::last_frame_rgba() else {
        return false;
    };
    if rgba.is_null() || width.is_null() || height.is_null() || pixels.len() > capacity {
        return false;
    }
    // SAFETY: as the caller promises, and `pixels` fits in `capacity`.
    unsafe {
        *width = w;
        *height = h;
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), rgba, pixels.len());
    }
    true
}

/// Doubles `rgba`, `width` by `height`, through hq2x into `out`, which holds
/// four times as many bytes. False when it cannot.
///
/// # Safety
/// `rgba` holds `width * height * 4` bytes and `out` four times that.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wie_hq2x(rgba: *const u8, width: u32, height: u32, out: *mut u8) -> bool {
    if rgba.is_null() || out.is_null() || width == 0 || height == 0 {
        return false;
    }
    let length = (width * height * 4) as usize;
    // SAFETY: as the caller promises.
    let source = unsafe { std::slice::from_raw_parts(rgba, length) };
    let Some(doubled) = host::hq2x_rgba(width, height, source) else {
        return false;
    };
    // SAFETY: `out` holds `length * 4` bytes, which is what hq2x makes.
    unsafe { std::ptr::copy_nonoverlapping(doubled.as_ptr(), out, doubled.len().min(length * 4)) };
    true
}

/// A way of taking the newest frame: its width, height and RGBA pixels.
type TakeFrame = fn() -> Option<(u32, u32, Vec<u8>)>;

/// # Safety
/// As [`wie_take_frame`].
unsafe fn take_frame(take: TakeFrame, rgba: *mut u8, capacity: usize, width: *mut u32, height: *mut u32) -> bool {
    let Ok(mut pending) = PENDING_FRAME.lock() else {
        return false;
    };
    // A newer frame replaces one still waiting.
    if let Some(frame) = take() {
        *pending = Some(frame);
    }
    let Some((w, h, pixels)) = pending.as_ref() else {
        return false;
    };

    if !width.is_null() && !height.is_null() {
        // SAFETY: as the caller promises.
        unsafe {
            *width = *w;
            *height = *h;
        }
    }
    if rgba.is_null() || pixels.len() > capacity {
        return false;
    }

    // SAFETY: `rgba` holds `capacity >= pixels.len()` bytes.
    unsafe { std::ptr::copy_nonoverlapping(pixels.as_ptr(), rgba, pixels.len()) };
    *pending = None;
    true
}

/// # Safety
/// `samples` points at `frames * 2` writable samples.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wie_render_audio(samples: *mut i16, frames: usize) -> usize {
    if samples.is_null() || frames == 0 {
        return 0;
    }
    let bytes = host::render_audio(frames);
    let count = (bytes.len() / 2).min(frames * 2);
    for (i, pair) in bytes.chunks_exact(2).take(count).enumerate() {
        // SAFETY: `i < frames * 2`, which the caller made room for.
        unsafe { *samples.add(i) = i16::from_le_bytes([pair[0], pair[1]]) };
    }
    count / 2
}

/// # Safety
/// `duration_ms` is valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wie_take_vibration(duration_ms: *mut u32) -> bool {
    while let Some(command) = host::take_output() {
        if command.first() == Some(&OPCODE_VIBRATE) && command.len() >= 10 {
            let mut duration = [0u8; 8];
            duration.copy_from_slice(&command[2..10]);
            if !duration_ms.is_null() {
                // SAFETY: as the caller promises.
                unsafe { *duration_ms = u64::from_le_bytes(duration).min(u32::MAX as u64) as u32 };
            }
            return true;
        }
    }
    false
}

/// # Safety
/// `data` points at `length` readable bytes.
unsafe fn bytes<'a>(data: *const u8, length: usize) -> &'a [u8] {
    if data.is_null() {
        return &[];
    }
    // SAFETY: as the caller promises.
    unsafe { std::slice::from_raw_parts(data, length) }
}

/// # Safety
/// `data` points at `length` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wie_carrier(data: *const u8, length: usize) -> *mut c_char {
    // SAFETY: as the caller promises.
    let carrier = host::carrier(unsafe { bytes(data, length) });
    CString::new(carrier).map_or(std::ptr::null_mut(), CString::into_raw)
}

#[unsafe(no_mangle)]
pub extern "C" fn wie_set_speed(speed: f32) {
    host::set_speed(speed);
}

#[unsafe(no_mangle)]
pub extern "C" fn wie_speed() -> f32 {
    host::speed()
}

/// # Safety
/// `data` points at `length` readable bytes; the strings are NUL-terminated;
/// `exported` is valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wie_export_save(
    data: *const u8,
    length: usize,
    runtime_dir: *const c_char,
    destination: *const c_char,
    exported: *mut bool,
) -> *mut c_char {
    // SAFETY: as the caller promises.
    let (data, runtime_dir, destination) = unsafe { (bytes(data, length), string(runtime_dir), string(destination)) };
    let result = host::export_save(data, std::path::Path::new(&runtime_dir)).and_then(|zip| match zip {
        Some(zip) => std::fs::write(&destination, zip)
            .map(|_| true)
            .map_err(|error| format!("세이브를 저장할 수 없습니다: {error}")),
        None => Ok(false),
    });
    match result {
        Ok(written) => {
            if !exported.is_null() {
                // SAFETY: as the caller promises.
                unsafe { *exported = written };
            }
            std::ptr::null_mut()
        }
        Err(error) => message(error),
    }
}

/// # Safety
/// `zip` points at `length` readable bytes; `runtime_dir` is NUL-terminated;
/// `restored` is valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wie_import_save(zip: *const u8, length: usize, runtime_dir: *const c_char, restored: *mut usize) -> *mut c_char {
    // SAFETY: as the caller promises.
    let (zip, runtime_dir) = unsafe { (bytes(zip, length), string(runtime_dir)) };
    match host::import_save(zip, std::path::Path::new(&runtime_dir)) {
        Ok(count) => {
            if !restored.is_null() {
                // SAFETY: as the caller promises.
                unsafe { *restored = count };
            }
            std::ptr::null_mut()
        }
        Err(error) => message(error),
    }
}

/// What a save zip is to a game: -1 when it is no save zip, 1 when it holds
/// the game's saves, 0 when another game's; and how many saved files it holds
/// and their size.
///
/// # Safety
/// `zip` points at `zip_length` readable bytes and `data` at `length`; `files`
/// and `size` are null or valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wie_save_zip_info(
    zip: *const u8,
    zip_length: usize,
    data: *const u8,
    length: usize,
    files: *mut usize,
    size: *mut u64,
) -> i32 {
    // SAFETY: as the caller promises.
    let (zip, data) = unsafe { (bytes(zip, zip_length), bytes(data, length)) };
    let (count, total) = host::save_zip_contents(zip);
    if !files.is_null() {
        // SAFETY: as the caller promises.
        unsafe { *files = count };
    }
    if !size.is_null() {
        // SAFETY: as the caller promises.
        unsafe { *size = total };
    }
    match host::save_zip_belongs(zip, data) {
        None => -1,
        Some(false) => 0,
        Some(true) => 1,
    }
}

/// # Safety
/// `data` points at `length` readable bytes; `runtime_dir` is NUL-terminated;
/// `removed` is valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wie_erase_save(data: *const u8, length: usize, runtime_dir: *const c_char, removed: *mut usize) -> *mut c_char {
    // SAFETY: as the caller promises.
    let (data, runtime_dir) = unsafe { (bytes(data, length), string(runtime_dir)) };
    match host::erase_save(data, std::path::Path::new(&runtime_dir)) {
        Ok(count) => {
            if !removed.is_null() {
                // SAFETY: as the caller promises.
                unsafe { *removed = count };
            }
            std::ptr::null_mut()
        }
        Err(error) => message(error),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn wie_pointer(action: i32, x: i32, y: i32) {
    host::pointer(action, x, y);
}

#[unsafe(no_mangle)]
pub extern "C" fn wie_set_touch(enabled: bool) {
    host::set_touch(enabled);
}

#[unsafe(no_mangle)]
pub extern "C" fn wie_touch() -> bool {
    host::touch()
}

#[unsafe(no_mangle)]
pub extern "C" fn wie_log() -> *mut c_char {
    CString::new(host::log().replace('\0', "?")).map_or(std::ptr::null_mut(), CString::into_raw)
}

/// # Safety
/// `string` is NULL or was returned by one of these functions and not yet
/// freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wie_free_string(string: *mut c_char) {
    if !string.is_null() {
        // SAFETY: as the caller promises - it came from `CString::into_raw`.
        drop(unsafe { CString::from_raw(string) });
    }
}
