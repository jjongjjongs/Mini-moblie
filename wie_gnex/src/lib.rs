//! SKT GNEX titles: a `.sgs` script beside a `.mod` descriptor, run by a
//! script interpreter rather than a JVM or an ARM core. This is the newer
//! (version 4) script layout; see `docs/gnex_sgs_v4_notes.md`.
#![no_std]
extern crate alloc;

mod graphics;
mod host;
mod program;
mod services;
mod vm;

use alloc::{boxed::Box, collections::BTreeMap, string::String, vec::Vec};

use wie_backend::{Emulator, Event, Instant, KeyCode, Platform, extract_zip};
use wie_util::{Result, WieError};

use crate::{host::Host, program::Program, vm::Vm};

/// The longest stretch of guest time one tick catches up on; a host that
/// stalled does not get a burst of timer callbacks after it.
const MAX_STEP_MS: u64 = 250;

pub struct GnexEmulator {
    platform: Box<dyn Platform>,
    program: Program,
    vm: Vm,
    host: Host,
    last: Instant,
    /// Set once a callback fails; the title is then left on its last frame.
    stopped: bool,
}

/// The script and descriptor of a GNEX title among an archive's files.
fn script(files: &BTreeMap<String, Vec<u8>>) -> Option<&Vec<u8>> {
    let descriptor = files.iter().find(|(name, data)| {
        name.to_ascii_lowercase().ends_with(".mod") && data.windows(22).any(|x| x.eq_ignore_ascii_case(b"application/x-gnex-sgs"))
    })?;
    let stem = descriptor.0[..descriptor.0.len() - 4].to_ascii_lowercase();
    files
        .iter()
        .find(|(name, _)| name.to_ascii_lowercase() == alloc::format!("{stem}.sgs"))
        .map(|(_, data)| data)
}

impl GnexEmulator {
    /// Whether the archive is a GNEX title in the layout this runs.
    pub fn loadable_archive(files: &BTreeMap<String, Vec<u8>>) -> bool {
        script(files).is_some_and(|data| program::unwrap(data).first() == Some(&4))
    }

    /// GNEX titles of this generation are drawn for a 240x320 panel.
    pub fn screen_size(archive: &[u8]) -> Option<(u32, u32)> {
        let files = extract_zip(archive).ok()?;
        Self::loadable_archive(&files).then_some((240, 320))
    }

    pub fn from_archive(platform: Box<dyn Platform>, files: BTreeMap<String, Vec<u8>>) -> Result<Self> {
        let data = script(&files).ok_or_else(|| WieError::FatalError("GNEX script not found".into()))?;
        let program = Program::parse(data)?;
        tracing::info!("GNEX title {}", program.title);

        let (width, height) = (platform.screen().width() as i32, platform.screen().height() as i32);
        let mut vm = Vm::new(&program);
        let host = Host::new(width, height);

        // System variables: the screen's size in 4 and 5, the runtime's version
        // (1.1) in 10.
        let _ = vm.set(4, 0, width);
        let _ = vm.set(5, 0, height);
        let _ = vm.set(10, 0, 1);
        let _ = vm.set(10, 1, 1);

        let last = platform.now();
        let mut emulator = Self {
            platform,
            program,
            vm,
            host,
            last,
            stopped: false,
        };
        emulator.run(0, 0);
        emulator.paint();

        Ok(emulator)
    }

    /// Runs callback `entry` with `parameter` in variable 0.
    fn run(&mut self, entry: usize, parameter: i32) {
        if self.stopped {
            return;
        }
        let _ = self.vm.set(0, 0, parameter);
        if let Err(error) = self.vm.run(self.program.entries[entry], &mut self.host) {
            tracing::error!("GNEX callback {entry} stopped: {error}");
            self.stopped = true;
        }
    }

    fn paint(&mut self) {
        if self.host.graphics.presented {
            self.host.graphics.presented = false;
            self.platform.screen().paint(&self.host.graphics.frame());
        }
    }

    /// The key number a script receives for a handset key.
    fn key(code: &KeyCode) -> Option<i32> {
        Some(match code {
            KeyCode::NUM1 => 1,
            KeyCode::NUM2 => 2,
            KeyCode::NUM3 => 3,
            KeyCode::NUM4 => 4,
            KeyCode::NUM5 => 5,
            KeyCode::NUM6 => 6,
            KeyCode::NUM7 => 7,
            KeyCode::NUM8 => 8,
            KeyCode::NUM9 => 9,
            KeyCode::NUM0 => 10,
            KeyCode::STAR => 11,
            KeyCode::HASH => 12,
            KeyCode::CLEAR => 13,
            KeyCode::LEFT_SOFT_KEY => 14,
            KeyCode::RIGHT_SOFT_KEY | KeyCode::CALL => 15,
            KeyCode::LEFT => 16,
            KeyCode::RIGHT => 17,
            KeyCode::UP => 18,
            KeyCode::DOWN => 19,
            KeyCode::OK => 20,
            _ => return None,
        })
    }
}

impl Emulator for GnexEmulator {
    fn handle_event(&mut self, event: Event) {
        match event {
            Event::Keydown(code) => {
                if let Some(key) = Self::key(&code) {
                    self.run(3, key);
                    self.paint();
                }
            }
            Event::Redraw => self.platform.screen().paint(&self.host.graphics.frame()),
            _ => {}
        }
    }

    fn tick(&mut self) -> Result<()> {
        let now = self.platform.now();
        let elapsed = (now - self.last).min(MAX_STEP_MS);
        self.last = now;
        let target = self.host.clock + elapsed;

        // Timers fire in order on the guest clock, each at its own due time.
        while !self.stopped && !self.vm.exited {
            let next = self.host.timers.iter().flatten().map(|t| t.due).min();
            match next {
                Some(due) if due <= target => {
                    self.host.clock = self.host.clock.max(due);
                    if let Some(index) = self.host.due_timer() {
                        self.run(2, index as i32);
                    }
                }
                _ => break,
            }
        }
        self.host.clock = target;
        self.paint();

        Ok(())
    }

    fn is_idle(&self) -> bool {
        self.stopped || self.vm.exited || self.host.next_due().is_none_or(|x| x > 0)
    }

    fn sleep_hint(&self) -> Option<u64> {
        if self.stopped || self.vm.exited {
            return None;
        }
        self.host.next_due()
    }
}
