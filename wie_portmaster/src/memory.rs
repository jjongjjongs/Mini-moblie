//! The handheld's memory, watched while a game runs.
//!
//! The handhelds PortMaster runs on have a gigabyte of memory or less, shared
//! with the system and its compositor. A game that keeps taking memory there
//! does not stop with an error: the kernel swaps until the whole device stops
//! answering - the screen, the buttons, the power key - and the log just ends.
//! That is what players of some LGT titles on an H700 handheld under ROCKNIX
//! saw, after three minutes of one title and thirteen of another.
//!
//! So while a game runs its resident size and the memory left on the device
//! are read every few seconds and written to the log - which is what says,
//! next time, how fast and where it grew - and once the device is nearly out,
//! the game is stopped and the player is told, while the device can still be
//! used.

use std::time::{Duration, Instant};

/// How often the memory is read.
const CHECK_EVERY: Duration = Duration::from_secs(5);

/// How often it is written to the log while it holds steady.
const LOG_EVERY: Duration = Duration::from_secs(60);

/// How much the game's resident size has to grow since it was last written
/// for it to be written again straight away.
const LOG_GROWTH: u64 = 32 * MIB;

/// The least memory left to the device before the game is stopped: this, or
/// a sixteenth of the device's memory if that is more.
const FLOOR: u64 = 64 * MIB;

const MIB: u64 = 1024 * 1024;

pub struct MemoryWatch {
    checked: Instant,
    logged: Option<(Instant, u64)>,
}

impl MemoryWatch {
    pub fn new() -> Self {
        Self {
            // Read on the first tick.
            checked: Instant::now() - CHECK_EVERY,
            logged: None,
        }
    }

    /// Reads the memory when it is time to. Answers what to tell the player
    /// when the device is nearly out of it and the game has to stop.
    pub fn check(&mut self) -> Option<String> {
        if self.checked.elapsed() < CHECK_EVERY {
            return None;
        }
        self.checked = Instant::now();

        let resident = resident_bytes()?;
        let (total, available) = device_bytes()?;

        let due = match self.logged {
            None => true,
            Some((at, logged)) => at.elapsed() >= LOG_EVERY || resident >= logged + LOG_GROWTH,
        };
        if due {
            eprintln!("[mem] game {}MB, device {}MB free of {}MB", resident / MIB, available / MIB, total / MIB);
            self.logged = Some((Instant::now(), resident));
        }

        if available < FLOOR.max(total / 16) {
            eprintln!(
                "[mem] stopping the game: game {}MB, device {}MB free of {}MB",
                resident / MIB,
                available / MIB,
                total / MIB
            );
            return Some(format!(
                "기기 메모리가 거의 다 차서, 기기가 멈추기 전에 게임을 멈췄어요.\n(게임 {}MB 사용, 남은 메모리 {}MB)",
                resident / MIB,
                available / MIB
            ));
        }

        None
    }
}

/// This process's resident size, from `/proc/self/statm`.
#[cfg(target_os = "linux")]
fn resident_bytes() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    // SAFETY: sysconf only reads a configuration value.
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    Some(pages * u64::try_from(page).ok()?)
}

#[cfg(not(target_os = "linux"))]
fn resident_bytes() -> Option<u64> {
    None
}

/// The device's memory and how much of it is available, from `/proc/meminfo`.
#[cfg(target_os = "linux")]
fn device_bytes() -> Option<(u64, u64)> {
    parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

#[cfg(not(target_os = "linux"))]
fn device_bytes() -> Option<(u64, u64)> {
    None
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_meminfo(meminfo: &str) -> Option<(u64, u64)> {
    let field = |name: &str| -> Option<u64> {
        let line = meminfo.lines().find(|line| line.starts_with(name))?;
        let kib: u64 = line[name.len()..].trim().trim_end_matches("kB").trim().parse().ok()?;
        Some(kib * 1024)
    };

    Some((field("MemTotal:")?, field("MemAvailable:")?))
}

#[cfg(test)]
mod tests {
    use super::parse_meminfo;

    #[test]
    fn meminfo_gives_the_total_and_what_is_available() {
        let meminfo = "MemTotal:        1005772 kB\nMemFree:           42148 kB\nMemAvailable:     312400 kB\nBuffers:            1234 kB\n";

        assert_eq!(parse_meminfo(meminfo), Some((1005772 * 1024, 312400 * 1024)));
        assert_eq!(parse_meminfo("MemTotal: 1 kB\n"), None);
    }
}
