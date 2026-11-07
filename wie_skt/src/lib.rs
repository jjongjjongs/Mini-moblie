#![no_std]
extern crate alloc;

mod emulator;
mod native;

pub use emulator::SktEmulator;
pub use native::title_id as native_title_id;
