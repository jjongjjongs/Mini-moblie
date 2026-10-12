//! SKT GNEX titles: a `.sgs` script beside a `.mod` descriptor, run by a
//! script interpreter rather than a JVM or an ARM core. This is the newer
//! (version 4) script layout; see `docs/gnex_sgs_v4_notes.md`.
#![no_std]
extern crate alloc;

mod files;
mod graphics;
mod host;
mod program;
mod services;
mod vm;

use alloc::{boxed::Box, collections::BTreeMap, string::String, vec::Vec};

use wie_backend::{DefaultTaskRunner, Emulator, Event, Instant, KeyCode, Platform, System, extract_zip};
use wie_util::{Result, WieError};

use crate::{
    files::Files,
    host::{Host, Sound},
    program::Program,
    vm::Vm,
};

/// The longest stretch of guest time one tick catches up on; a host that
/// stalled does not get a burst of timer callbacks after it.
const MAX_STEP_MS: u64 = 250;

pub struct GnexEmulator {
    /// The platform, and the sound player that runs on its task loop.
    system: System,
    /// Each sound resource loaded into the player, and the one sounding.
    clips: BTreeMap<i32, u32>,
    sounding: Option<u32>,
    program: Program,
    vm: Vm,
    host: Host,
    last: Instant,
    /// Set once a callback fails; the title is then left on its last frame.
    stopped: bool,
}

/// The script and descriptor of a GNEX title among an archive's files.
fn script(files: &BTreeMap<String, Vec<u8>>) -> Option<&Vec<u8>> {
    script_entry(files).map(|(_, data)| data)
}

/// The script's file name and bytes.
fn script_entry(files: &BTreeMap<String, Vec<u8>>) -> Option<(&String, &Vec<u8>)> {
    let descriptor = files.iter().find(|(name, data)| {
        name.to_ascii_lowercase().ends_with(".mod") && data.windows(22).any(|x| x.eq_ignore_ascii_case(b"application/x-gnex-sgs"))
    })?;
    let stem = descriptor.0[..descriptor.0.len() - 4].to_ascii_lowercase();
    files.iter().find(|(name, _)| name.to_ascii_lowercase() == alloc::format!("{stem}.sgs"))
}

impl GnexEmulator {
    /// Whether the archive is a GNEX title in the layout this runs.
    pub fn loadable_archive(files: &BTreeMap<String, Vec<u8>>) -> bool {
        script(files).is_some_and(|data| program::unwrap(data).first() == Some(&4))
    }

    /// The panel a title was built for. Most of these titles were built for
    /// the 176x220 handsets of their day: on a larger panel they lay their
    /// fixed-size pictures out at its centre or edges and leave the rest
    /// bare, while on 176x220 every one of them fills the screen as drawn. A
    /// title that sizes itself to whatever panel it runs on is listed with
    /// the larger one it is at its best on.
    pub fn screen_size(archive: &[u8]) -> Option<(u32, u32)> {
        /// Titles that size themselves to the panel, by script name: 놈투(GS).
        const SIZES_THEMSELVES: [&str; 1] = ["0051262682"];

        let files = extract_zip(archive).ok()?;
        let (name, data) = script_entry(&files)?;
        if program::unwrap(data).first() != Some(&4) {
            return None;
        }
        let id = name.rsplit('/').next().unwrap_or(name).split('.').next().unwrap_or("");
        Some(if SIZES_THEMSELVES.contains(&id) { (240, 320) } else { (176, 220) })
    }

    pub fn from_archive(platform: Box<dyn Platform>, files: BTreeMap<String, Vec<u8>>) -> Result<Self> {
        let (name, data) = script_entry(&files).ok_or_else(|| WieError::FatalError("GNEX script not found".into()))?;
        let program = Program::parse(data)?;
        // The script's file is named for the title's id.
        let id = name.rsplit('/').next().unwrap_or(name).split('.').next().unwrap_or("gnex");
        let system = System::new(platform, id, id, DefaultTaskRunner);

        // The sound player runs as tasks on the system's loop. With no task at
        // all that loop spins out its whole slice every tick; one that only
        // sleeps keeps it idle between sounds.
        let keeper = system.clone();
        system.spawn(async move || {
            loop {
                keeper.sleep(60_000).await;
            }
        });
        // The data files a package ships beside its script, for its file
        // services to read; what the title writes goes to the handset's own
        // storage over them.
        for (path, data) in &files {
            let lower = path.to_ascii_lowercase();
            if !(lower.ends_with(".sgs") || lower.ends_with(".mod") || lower.ends_with(".wmr")) {
                system.filesystem().add_virtual(path, data.clone());
            }
        }
        let platform = system.platform();
        tracing::info!("GNEX title {}", program.title);

        let (width, height) = (platform.screen().width() as i32, platform.screen().height() as i32);
        let mut vm = Vm::new(&program);
        let host = Host::new(width, height, Files::new(Some(system.clone())));

        // System variables: the screen's size in 4 and 5, the runtime's version
        // (1.1) in 10.
        let _ = vm.set(4, 0, width);
        let _ = vm.set(5, 0, height);
        let _ = vm.set(10, 0, 1);
        let _ = vm.set(10, 1, 1);

        let last = platform.now();
        let mut emulator = Self {
            system,
            clips: BTreeMap::new(),
            sounding: None,
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
        self.sound();
    }

    /// Carries out what the callback asked of the sound: one sound at a time,
    /// each played once, as the handset did.
    fn sound(&mut self) {
        for sound in core::mem::take(&mut self.host.sounds) {
            if let Some(handle) = self.sounding.take() {
                self.system.audio().stop(handle);
            }
            let Sound::Play(resource) = sound else { continue };
            let handle = match self.clips.get(&resource) {
                Some(&handle) => handle,
                None => {
                    // A two-byte wrapper precedes the SMAF file.
                    let Some(data) = self.vm.resources.get(resource as usize).filter(|x| x.len() > 2) else {
                        tracing::warn!("GNEX sound resource {resource} is not a sound");
                        continue;
                    };
                    let Ok(handle) = self.system.audio().load_smaf(&data[2..]) else {
                        continue;
                    };
                    self.clips.insert(resource, handle);
                    handle
                }
            };
            let system = self.system.clone();
            if self.system.audio().play(&system, handle, false).is_ok() {
                self.sounding = Some(handle);
            }
        }
    }

    fn paint(&mut self) {
        if self.host.graphics.presented {
            self.host.graphics.presented = false;
            self.system.platform().screen().paint(&self.host.graphics.frame());
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
            Event::Redraw => self.system.platform().screen().paint(&self.host.graphics.frame()),
            _ => {}
        }
    }

    fn tick(&mut self) -> Result<()> {
        let now = self.system.platform().now();
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

        // The sound player's own tasks.
        self.system.tick()
    }

    fn is_idle(&self) -> bool {
        let script = self.stopped || self.vm.exited || self.host.next_due().is_none_or(|x| x > 0);
        script && self.system.is_idle()
    }

    fn sleep_hint(&self) -> Option<u64> {
        let script = if self.stopped || self.vm.exited { None } else { self.host.next_due() };
        match (script, self.system.idle_for()) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }
}
