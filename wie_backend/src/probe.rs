//! A bounded control-flow trace, armed from the emulator and read out of a log.
//!
//! Some things a title does cannot be learned from a log of what it asked the
//! platform for, because the answer it was waiting on never existed. A billing
//! reply is the standing example: the gateway these titles talk to has been off
//! for years, so a capture of one of their purchases holds our own stand-in
//! answer and nothing else. What the title would have accepted is written only
//! in its own code, in the branch it takes after reading the reply.
//!
//! This makes that branch visible. Arming the probe records where the ARM core
//! goes next whenever it does not simply fall through to the following
//! instruction, and writes those edges to the log; read against the title's own
//! `binary.mod`, a trace is the parse itself - every compare, and which way each
//! one went. One run on a handset then says what a reply has to look like,
//! where a log alone can only say that the one we sent was wrong.
//!
//! Edges rather than instructions, because a trace has to survive a handset's
//! log buffer: straight-line code is most of what runs and none of what a parse
//! is decided by, and a tight loop is a single edge repeated, which is recorded
//! as one entry and a count. What is left is small enough to read.
//!
//! It is off unless something arms it, and arming is deliberate and bounded:
//! the count is spent and the probe goes quiet again. The cost while it sleeps
//! is one relaxed load per instruction, which the interpreter's loop already
//! pays several of.

use alloc::{format, string::String, vec::Vec};

use core::sync::atomic::{AtomicU32, Ordering};

use spin::Mutex;

/// Edges still to record. Zero - the resting state - is what the core checks,
/// so a sleeping probe costs one relaxed load.
static REMAINING: AtomicU32 = AtomicU32::new(0);

/// Where the core was when it was last looked at, so the next look can tell a
/// fall-through from a jump.
static LAST: AtomicU32 = AtomicU32::new(0);

/// Recorded edges not yet written out, each with how many times it repeated.
static PENDING: Mutex<Vec<Edge>> = Mutex::new(Vec::new());

/// What armed the probe, carried into the trace lines so a log holding several
/// traces says which is which.
static LABEL: Mutex<Option<String>> = Mutex::new(None);

/// A trace to start once the answer it is about has been read, rather than when
/// it was written. See [`arm_when_drained`].
static WHEN_DRAINED: Mutex<Option<(String, u32)>> = Mutex::new(None);

/// Edges per trace line. A line is for reading; this keeps one to about the
/// width of a terminal.
const BATCH: usize = 12;

/// One place the core jumped from and to, and how many times in a row it made
/// that same jump - which is what a loop looks like from here.
#[derive(Clone, Copy, Eq, PartialEq)]
struct Edge {
    from: u32,
    to: u32,
    times: u32,
}

/// Starts recording the next `count` edges, under `label`.
///
/// Arming again while a trace is running replaces it: the interesting run is
/// the one that was just armed, and the tail of a spent trace is noise.
pub fn arm(label: &str, count: u32) {
    flush();

    LAST.store(0, Ordering::Relaxed);
    *LABEL.lock() = Some(String::from(label));
    REMAINING.store(count, Ordering::Relaxed);

    tracing::info!("probe: tracing the next {count} branches for {label}");
}

/// Arms a trace for the moment a queued answer has been read rather than the
/// moment it was written.
///
/// A title reads a reply some way after the gateway put it there - it takes the
/// length first and the body on a later call, and the scheduler runs between
/// them. Tracing from the write would spend the count on that wait; tracing
/// from the last byte leaving the gateway starts it on the parse, which is what
/// the trace is for.
pub fn arm_when_drained(label: &str, count: u32) {
    *WHEN_DRAINED.lock() = Some((String::from(label), count));
}

/// Starts a trace [`arm_when_drained`] queued, if one is waiting.
///
/// Call where a queued answer runs out, which is where the title has all of it
/// and nothing is left to wait for.
pub fn drained() {
    let queued = WHEN_DRAINED.lock().take();

    let Some((label, count)) = queued else {
        return;
    };

    // A trace already running is one someone is waiting on the whole of.
    // Replacing it here would leave two half-traces where a caller asked for
    // one, so the newcomer gives way.
    if is_armed() {
        tracing::info!("probe: {label} was not traced; a trace was already running");
        return;
    }

    arm(&label, count);
}

/// Stops any trace, running or queued, and forgets what it had recorded.
///
/// The probe is one thing shared by the process. A caller that has finished
/// with it - or a test that must not leave one armed behind it - puts it back
/// to rest here.
pub fn disarm() {
    REMAINING.store(0, Ordering::Relaxed);
    LAST.store(0, Ordering::Relaxed);
    PENDING.lock().clear();
    *LABEL.lock() = None;
    *WHEN_DRAINED.lock() = None;
}

/// Whether the core should be recording. One relaxed load, called per
/// instruction.
#[inline(always)]
pub fn is_armed() -> bool {
    REMAINING.load(Ordering::Relaxed) != 0
}

/// Looks at where the core is, recording an edge if it did not get there by
/// falling through from where it was.
///
/// Call once per instruction under [`is_armed`], before the instruction runs.
pub fn observe(pc: u32) {
    let left = REMAINING.load(Ordering::Relaxed);
    if left == 0 {
        return;
    }

    let last = LAST.swap(pc, Ordering::Relaxed);

    // The first look has nothing to compare against, and an instruction reached
    // from the one before it says nothing about a decision. ARM instructions are
    // four bytes and Thumb two or four, so either step is a fall-through.
    if last == 0 || pc == last + 2 || pc == last + 4 {
        return;
    }

    REMAINING.store(left - 1, Ordering::Relaxed);

    let mut pending = PENDING.lock();

    // A loop is this same edge over and over. Count it rather than writing a
    // line per turn.
    if let Some(previous) = pending.last_mut()
        && previous.from == last
        && previous.to == pc
    {
        previous.times += 1;
        return;
    }

    pending.push(Edge {
        from: last,
        to: pc,
        times: 1,
    });

    if pending.len() < BATCH && left > 1 {
        return;
    }

    let line = trace_line(&pending);
    pending.clear();
    drop(pending);

    tracing::info!("{line}");
}

/// Writes out a partly filled batch, so the tail of a trace is not lost.
pub fn flush() {
    let mut pending = PENDING.lock();
    if pending.is_empty() {
        return;
    }

    let line = trace_line(&pending);
    pending.clear();
    drop(pending);

    tracing::info!("{line}");
}

/// One trace line: the label the probe was armed under, then the edges, each
/// `from>to` and - where it repeated - `*times`.
fn trace_line(edges: &[Edge]) -> String {
    let label = LABEL.lock().clone().unwrap_or_default();
    let mut line = format!("probe {label}:");

    for edge in edges {
        line.push_str(&format!(" {:x}>{:x}", edge.from, edge.to));

        if edge.times > 1 {
            line.push_str(&format!("*{}", edge.times));
        }
    }

    line
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The probe is one thing shared by the whole process, so two tests driving
    /// it at once would each see the other's edges. Hold this for the length of
    /// any test that arms it.
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

    /// The edges a walk over `path` records, as a trace line would read them.
    fn trace_of(path: &[u32]) -> String {
        for pc in path {
            observe(*pc);
        }

        let pending = PENDING.lock().clone();
        trace_line(&pending)
    }

    #[test]
    fn a_probe_that_was_never_armed_is_not_armed() {
        let _guard = ONE_AT_A_TIME.lock();
        disarm();

        assert!(!is_armed());
    }

    #[test]
    fn straight_line_code_is_not_a_branch() {
        let _guard = ONE_AT_A_TIME.lock();
        disarm();
        arm("test", 8);

        // Four-byte ARM, then two-byte Thumb: both are fall-through.
        assert_eq!(trace_of(&[0x1a28, 0x1a2c, 0x1a30, 0x1a32]), "probe test:");
        disarm();
    }

    #[test]
    fn a_jump_is_recorded_as_the_edge_it_took() {
        let _guard = ONE_AT_A_TIME.lock();
        disarm();
        arm("test", 8);

        assert_eq!(trace_of(&[0x1a28, 0x1a2c, 0x520b4]), "probe test: 1a2c>520b4");
        disarm();
    }

    #[test]
    fn a_loop_is_one_edge_and_a_count() {
        let _guard = ONE_AT_A_TIME.lock();
        disarm();
        arm("test", 8);

        let mut path = alloc::vec![];
        for _ in 0..3 {
            path.extend_from_slice(&[0x1000, 0x1004, 0x1008]);
        }

        assert_eq!(trace_of(&path), "probe test: 1008>1000*2");
        disarm();
    }

    #[test]
    fn a_trace_stops_after_the_count_of_branches_it_was_armed_for() {
        let _guard = ONE_AT_A_TIME.lock();
        disarm();
        arm("test", 2);

        // Every step here is a jump, so each one spends a branch.
        observe(0x1000);
        observe(0x2000);
        assert!(is_armed());
        observe(0x3000);
        assert!(!is_armed());

        disarm();
    }

    #[test]
    fn a_trace_queued_while_one_is_running_gives_way() {
        let _guard = ONE_AT_A_TIME.lock();
        disarm();

        arm("first", 5);
        arm_when_drained("second", 5);
        drained();

        // Still the first one's trace, and the second is not waiting behind it.
        assert_eq!(LABEL.lock().clone().unwrap(), "first");
        assert!(WHEN_DRAINED.lock().is_none());

        disarm();
    }

    #[test]
    fn a_trace_queued_for_a_drain_waits_for_one() {
        let _guard = ONE_AT_A_TIME.lock();
        disarm();
        arm_when_drained("test", 4);
        assert!(!is_armed());

        drained();
        assert!(is_armed());

        disarm();
    }

    #[test]
    fn a_drain_with_nothing_queued_arms_nothing() {
        let _guard = ONE_AT_A_TIME.lock();
        disarm();

        drained();

        assert!(!is_armed());
    }

    #[test]
    fn a_trace_line_names_what_armed_it() {
        let _guard = ONE_AT_A_TIME.lock();
        disarm();
        arm("서든어택", 8);

        assert_eq!(
            trace_line(&[Edge {
                from: 0x1a28,
                to: 0x520b4,
                times: 1
            }]),
            "probe 서든어택: 1a28>520b4"
        );
        disarm();
    }
}
