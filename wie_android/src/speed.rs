//! The game clock, and how fast it runs against the wall clock.
//!
//! Everything a title knows about time comes through
//! [`AndroidPlatform::now`](crate::platform::AndroidPlatform): its timers, its
//! sleeps, the frame pacing it does by reading the time. So running a title
//! faster or slower is a matter of this clock alone - at 2x it reads two
//! seconds for every one that passes, the title's timers come due twice as
//! often, and the loop that waits on them waits half as long (see
//! [`real_ms`]). Nothing else in the emulator has to know.
//!
//! At 1x it reads the wall clock exactly, which is what it did before there
//! was a speed to set.

use std::{
    sync::{
        Mutex,
        atomic::{AtomicU32, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

/// The slowest and fastest the player offers.
const MIN_SPEED: f32 = 0.1;
const MAX_SPEED: f32 = 8.0;

/// The speed, as `f32` bits, so the loop can read it without a lock.
static SPEED: AtomicU32 = AtomicU32::new(0x3f80_0000); // 1.0

/// The wall-clock and game-clock readings the clock last agreed on.
///
/// The game clock runs from here at the current speed. Moving the anchor to
/// the present whenever the speed changes keeps the clock continuous and
/// never going backwards: a title that was at 12.000s when it went to 2x is at
/// 12.000s still, and climbs twice as fast from there.
struct Anchor {
    wall_ms: u64,
    game_ms: u64,
}

static ANCHOR: Mutex<Option<Anchor>> = Mutex::new(None);

/// The game clock's reading while a host holds it still - a pause menu over
/// the title - or `None` while it runs.
///
/// Taken before [`ANCHOR`] wherever both are.
static HELD: Mutex<Option<u64>> = Mutex::new(None);

fn wall_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

fn game_ms_at(anchor: &Anchor, wall: u64, speed: f32) -> u64 {
    let elapsed = wall.saturating_sub(anchor.wall_ms);

    anchor.game_ms + (elapsed as f64 * f64::from(speed)) as u64
}

/// How fast the game clock runs, 1.0 being real time.
pub fn speed() -> f32 {
    f32::from_bits(SPEED.load(Ordering::Relaxed))
}

/// Runs the game clock at `speed` from now on, picking up where it is.
pub fn set_speed(speed: f32) {
    let speed = if speed.is_finite() { speed.clamp(MIN_SPEED, MAX_SPEED) } else { 1.0 };

    let held = *HELD.lock().unwrap_or_else(|x| x.into_inner());
    let mut anchor = ANCHOR.lock().unwrap_or_else(|x| x.into_inner());
    let wall = wall_ms();
    let game = held.unwrap_or_else(|| anchor.as_ref().map_or(wall, |anchor| game_ms_at(anchor, wall, self::speed())));
    *anchor = Some(Anchor {
        wall_ms: wall,
        game_ms: game,
    });
    SPEED.store(speed.to_bits(), Ordering::Relaxed);

    tracing::info!("[speed] game clock at {speed}x");
}

/// Puts the game clock back on the wall clock, keeping the speed.
///
/// Called as a title starts, so each run begins at the real time of day
/// whatever an earlier run at another speed left the clock at.
pub fn realign() {
    *HELD.lock().unwrap_or_else(|x| x.into_inner()) = None;
    *ANCHOR.lock().unwrap_or_else(|x| x.into_inner()) = Some(Anchor {
        wall_ms: wall_ms(),
        game_ms: wall_ms(),
    });
}

/// Stops the game clock where it is, or starts it again from there.
///
/// While it is held the title reads the same time throughout, so a pause of
/// any length is no time at all to it: a timer due in a second is still due
/// a second after the clock is let go.
pub fn hold(held: bool) {
    let mut slot = HELD.lock().unwrap_or_else(|x| x.into_inner());
    match (held, *slot) {
        (true, None) => *slot = Some(running_ms()),
        (false, Some(game)) => {
            *slot = None;
            *ANCHOR.lock().unwrap_or_else(|x| x.into_inner()) = Some(Anchor {
                wall_ms: wall_ms(),
                game_ms: game,
            });
        }
        _ => {}
    }
}

/// The game clock, in milliseconds since the epoch.
pub fn now_ms() -> u64 {
    if let Some(game) = *HELD.lock().unwrap_or_else(|x| x.into_inner()) {
        return game;
    }
    running_ms()
}

/// The game clock as its anchor has it, held or not.
fn running_ms() -> u64 {
    let anchor = ANCHOR.lock().unwrap_or_else(|x| x.into_inner());
    let wall = wall_ms();

    match anchor.as_ref() {
        Some(anchor) => game_ms_at(anchor, wall, speed()),
        None => wall,
    }
}

/// How long `game_ms` of game time takes on the wall clock, rounded up so a
/// wait never ends before the timer it is waiting for.
pub fn real_ms(game_ms: u64) -> u64 {
    (game_ms as f64 / f64::from(speed())).ceil() as u64
}

#[cfg(test)]
mod tests {
    use super::{Anchor, game_ms_at, hold, now_ms};

    /// A held clock reads the same throughout, and goes on from there.
    #[test]
    fn a_held_clock_stands_still_and_goes_on_from_where_it_stood() {
        hold(true);
        let held = now_ms();
        std::thread::sleep(std::time::Duration::from_millis(60));
        assert_eq!(now_ms(), held);
        hold(false);
        std::thread::sleep(std::time::Duration::from_millis(20));
        let after = now_ms();
        assert!(after >= held + 15 && after < held + 55, "{held} then {after}");
    }

    /// The clock runs at its speed from the anchor, and only from the anchor.
    #[test]
    fn the_game_clock_runs_at_its_speed_from_its_anchor() {
        let anchor = Anchor {
            wall_ms: 1_000,
            game_ms: 5_000,
        };

        assert_eq!(game_ms_at(&anchor, 1_000, 2.0), 5_000);
        assert_eq!(game_ms_at(&anchor, 1_500, 2.0), 6_000);
        assert_eq!(game_ms_at(&anchor, 1_500, 0.5), 5_250);
        assert_eq!(game_ms_at(&anchor, 1_500, 1.0), 5_500);
        // A wall clock stepped back does not take the game clock with it.
        assert_eq!(game_ms_at(&anchor, 900, 2.0), 5_000);
    }
}
