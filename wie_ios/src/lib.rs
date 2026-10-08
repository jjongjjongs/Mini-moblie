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
    let Ok(mut pending) = PENDING_FRAME.lock() else {
        return false;
    };
    // A newer frame replaces one still waiting.
    if let Some(frame) = host::take_frame_rgba() {
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
