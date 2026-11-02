//! The shared runner the hosts drive: Android and iOS through the JNI/C
//! bridges, and the PC and PortMaster builds through [`build_emulator`]
//! directly.
//!
//! [`build_emulator`] loads an archive the way the Android app does - the
//! bundled firmware and handset font, packages around one jar, a jar filed
//! under its own id - so a title runs the same on every host. The JNI bridge
//! behind `com.jjongjjongs.minimobile.NativeBridge` lives in `jni_bridge`; it
//! is native only, since `jni` and the socket network do not build for wasm,
//! and is left out of the wasm compile-check.

// On wasm the JNI bridge is gone, so the host glue it alone calls - log
// collection, backlight/phone/URL pickup, inspection - has no caller. It is
// live on every real host; only the wasm compile-check sees it unused.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

mod audio;
mod database;
mod filesystem;
pub mod host;
pub mod hq2x;
pub mod icon;
mod logging;
mod ma3;
// The socket network uses `polling` and `std::net`, neither of which builds
// for wasm; a wasm host falls back to no network (Platform::network's default).
#[cfg(not(target_arch = "wasm32"))]
mod network;
mod oma3;
mod platform;
mod runner;
mod speed;

// The JNI bridge is native only (see jni_bridge.rs).
#[cfg(not(target_arch = "wasm32"))]
mod jni_bridge;

pub use runner::{build_emulator, title_panel};
