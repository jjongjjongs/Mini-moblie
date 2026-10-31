//! Qualcomm BREW applications, as KTF sold them before WIPI.
//!
//! A BREW download is a module (`.mod`), the module information file that
//! describes it (`.mif`), its signature (`.sig`) and the title's own data
//! files. There is no descriptor, no jar and no Java: the module is ARM code
//! that finds the platform through a table pointer planted just below where it
//! is loaded, and asks for everything else as numbered interfaces.
//!
//! Ported from wfeature's `internal/platform/ktf/native_*.go` (MIT, Copyright
//! (c) 2026 movingwoo), which established the slot and interface numbers this
//! answers by running these titles one unanswered call at a time.
#![no_std]
extern crate alloc;

mod archive;
mod bitmap;
mod emulator;
mod resource;
mod runtime;

pub use archive::{BrewArchive, BrewInfo, is_brew_package};
pub use emulator::BrewEmulator;
