use alloc::{collections::BTreeMap, vec::Vec};

use crate::graphics::Graphics;

#[derive(Clone, Copy)]
pub struct Timer {
    pub period: u64,
    pub repeat: bool,
    pub due: u64,
}

/// What the script's services act on besides its own memory: the screen, its
/// timers on the guest clock, packed palettes, the random sequence and the
/// handset's save words.
pub struct Host {
    pub graphics: Graphics,
    pub palettes: BTreeMap<i32, Vec<u8>>,
    /// Guest time in milliseconds.
    pub clock: u64,
    pub timers: [Option<Timer>; 3],
    random: u64,
    pub saved: Vec<i32>,
}

impl Host {
    pub fn new(width: i32, height: i32) -> Self {
        Self {
            graphics: Graphics::new(width, height),
            palettes: BTreeMap::new(),
            clock: 0,
            timers: [None; 3],
            random: 0x2545_f491_4f6c_dd1d,
            saved: Vec::new(),
        }
    }

    pub fn start_timer(&mut self, index: usize, period: i32, repeat: bool) {
        let period = period.max(10) as u64;
        self.timers[index] = Some(Timer {
            period,
            repeat,
            due: self.clock + period,
        });
    }

    pub fn seed(&mut self, seed: i32) {
        self.random = (seed as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    }

    /// A number in `0..bound`.
    pub fn random(&mut self, bound: u32) -> u32 {
        // xorshift64*
        self.random ^= self.random >> 12;
        self.random ^= self.random << 25;
        self.random ^= self.random >> 27;
        let value = self.random.wrapping_mul(0x2545_f491_4f6c_dd1d);
        if bound == 0 { 0 } else { (value >> 32) as u32 % bound }
    }

    /// The first timer due at or before now, taken off or rescheduled.
    pub fn due_timer(&mut self) -> Option<usize> {
        let clock = self.clock;
        let index = (0..3).find(|&i| self.timers[i].is_some_and(|t| t.due <= clock))?;
        let timer = self.timers[index].as_mut().unwrap();
        if timer.repeat {
            timer.due = clock + timer.period;
        } else {
            self.timers[index] = None;
        }
        Some(index)
    }

    /// How long until the next timer, if any is running.
    pub fn next_due(&self) -> Option<u64> {
        self.timers.iter().flatten().map(|t| t.due.saturating_sub(self.clock)).min()
    }
}
