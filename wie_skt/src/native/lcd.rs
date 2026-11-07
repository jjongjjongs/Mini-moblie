//! The screen as a native title draws it without saying so.
//!
//! 던전앤파이터 composes straight into the screen frame buffer and never
//! flushes it, because on the handset that buffer is the display. Its KTF
//! edition does the same, and this is the KTF watcher (`KtfEmulator::
//! present_lcd`) with SKT's handles: a title that presents its own frames
//! stands it down, and one that has gone quiet has its buffer shown once the
//! content stops changing.

use bytemuck::pod_collect_to_vec;

use wie_backend::{
    Instant, System,
    canvas::{Rgb565Pixel, VecImageBuffer},
};
use wie_core_arm::ArmCore;
use wie_util::{Result, read_generic};

/// How long a title must go without presenting a frame itself before the
/// watcher takes the LCD over.
const QUIET_MS: u64 = 1000;

#[derive(Default)]
pub struct LcdWatcher {
    /// What the LCD held when it was last shown.
    shown: Option<u64>,
    /// What the last look saw, so a frame is shown only once it holds still.
    seen: Option<u64>,
    /// How many frames the title had presented itself as of the last look.
    presents: u32,
    /// When that count last moved.
    present_at: Option<Instant>,
}

impl LcdWatcher {
    pub fn tick(&mut self, core: &ArmCore, system: &System) {
        let presents = wie_wipi_c::api::graphics::explicit_present_count();
        let now = system.platform().now();
        if presents != self.presents {
            self.presents = presents;
            self.present_at = Some(now);
            return;
        }
        if let Some(at) = self.present_at
            && now.raw().saturating_sub(at.raw()) < QUIET_MS
        {
            return;
        }

        // An SKT handle's first word is the data - see `SktWIPICContext`.
        let data_ptr = |handle: u32| -> Result<u32> { read_generic(core, handle) };
        let Ok(Some((width, height, bytes))) = wie_wipi_c::api::graphics::screen_surface_bytes(core, &data_ptr) else {
            return;
        };

        let mut digest = 0xcbf2_9ce4_8422_2325u64;
        for chunk in bytes.chunks(8) {
            let mut word = [0u8; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            digest = (digest ^ u64::from_le_bytes(word)).wrapping_mul(0x1000_0000_01b3);
        }

        // The first look is the baseline: whatever the heap left in a fresh
        // buffer is not a frame.
        let Some(shown) = self.shown else {
            self.shown = Some(digest);
            self.seen = Some(digest);
            return;
        };
        if digest == shown {
            return;
        }

        // Nothing says when a frame is finished; one that has held still for a
        // tick is.
        if self.seen != Some(digest) {
            self.seen = Some(digest);
            return;
        }

        // A buffer that is one value throughout has not been drawn into.
        let first = &bytes[..2.min(bytes.len())];
        if bytes.chunks(2).all(|x| x == first) {
            self.shown = Some(digest);
            return;
        }

        let image = VecImageBuffer::<Rgb565Pixel>::from_raw(width, height, pod_collect_to_vec(&bytes));
        system.set_title_drives_lcd();
        wie_backend::present(system, &image);
        self.shown = Some(digest);
    }
}
