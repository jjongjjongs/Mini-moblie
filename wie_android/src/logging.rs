//! Routes `tracing` output to logcat, so `adb logcat -s WIE` shows the same
//! diagnostics `wie_cli` prints to stderr - and keeps a copy, so a phone with
//! no `adb` attached can still hand the log over.
//!
//! The copy is what the player's log buttons save. It is cleared when a game
//! starts, so what comes out is that one run and not the whole session, and
//! [`start_collecting`] narrows it further to one stretch of play with every
//! area turned on - which is what keeps a new question from needing a new
//! build with new instrumentation in it.
//!
//! The copy is bounded, and what it holds arrives at wildly different rates, so
//! it is kept in three lanes with a share of the bound each - the branch trace,
//! the lines at info and above, and everything else - and merged back by time
//! when it is read. Sharing one bound first-come, the densest lane decided how
//! far back the whole capture reached, and it decided it in milliseconds.

use std::{
    collections::VecDeque,
    io,
    sync::{
        LazyLock, Mutex, Once, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use tracing_subscriber::{EnvFilter, Registry, prelude::*, reload};

static INIT: Once = Once::new();

/// Default log directive used when `RUST_LOG` is unset.
///
/// Captures as much as is useful with a single press of the log button, so most
/// debugging needs no filter change: everything at debug, plus trace for the
/// platform and loader crates - the low-frequency lifecycle, resource and
/// class-loading detail that explains why a title fails to start or misbehaves.
///
/// The known per-frame floods are held back, because at debug/trace they bury the
/// rest and slow the game - and, worse, fill the bounded capture so fast that a
/// key press a second earlier has already scrolled out of it. The ARM
/// interpreter's per-instruction trace (`arm32_cpu`) and the JIT's per-block
/// bookkeeping (`wie_core_arm`) drop to info/warn; every graphics service's
/// per-op drawing stays at info; the standard-library call echo
/// (`java_runtime`: every `Vector.size`, `arraycopy`, `String`, stream read the
/// game makes) drops to info - with `java.lang.System` held back out of that,
/// because `arraycopy` is where a title's own out-of-bounds copy surfaces and
/// its five arguments are the only thing in a capture that says which arrays
/// were involved; 지크 dies in one and the answer was not in the log; and the
/// LGT paint-loop housekeeping that dwarfs
/// everything else - `vm_activate_class`/`vm_thread_reschedule`/
/// `vm_check_stack_overflow` and the per-call `LGT invoke virtual/static` echo,
/// the bulk of a capture - is routed to the `wie_lgt::hot` target and held at
/// warn. Together these are what let a whole save/load cycle (millions of calls)
/// fit the bounded capture instead of scrolling the save moment out. Faults and
/// unimplemented calls in those crates log at warn/error, so they still show, and
/// the input path (event queue, canvas, clet) stays at debug so a press is
/// always captured. Setting `RUST_LOG`, or the in-app log filter (e.g.
/// `wie_lgt::hot=trace`), overrides this entirely.
const DEFAULT_LOG_DIRECTIVE: &str = "debug,wie_lgt=trace,wie_lgt::hot=warn,wie_lgt::runtime::wipi_c=debug,wie_ktf=trace,wie_j2me=trace,wie_skt=trace,wie_core_arm=info,wie_wipi_c::api::graphics=debug,wie_wipi_java::classes::org::kwis::msp::lcdui::graphics=info,wie_midp::classes::javax::microedition::lcdui::graphics=info,java_runtime=info,java_runtime::classes::java::lang::system=debug,arm32_cpu=warn";

/// What a collection window captures: everything, everywhere.
///
/// The window has an end, so it can afford to be far wider than the default -
/// but it is still bounded in *size*, and an end does not protect it from a
/// flood. 액션퍼즐패밀리1 settles into a spin lock that yields through
/// `vm_thread_reschedule` 4.7 million times a second; under a bare `trace` the
/// window filled in about seven milliseconds and threw away 311,687 lines,
/// leaving a capture in which every line was the spin and nothing said what the
/// title was waiting on.
///
/// So the same few floods the default holds back are held back here too. They
/// are not about the emulated title, and they crowd out what is:
///
/// - `arm32_cpu`, whose trace is a line per emulated instruction, millions a
///   second. What the core did is what `wie_backend::probe` is for.
/// - `jni`, which narrates its own binding layer - a line per `NewStringUTF`
///   and the like. The first window taken with this filter was 47% that, and
///   nothing in it was about the game.
/// - `wie_lgt::hot`, the LGT paint-loop housekeeping - `vm_thread_reschedule`,
///   `vm_activate_class`, `vm_check_stack_overflow` and the per-call invoke
///   echo. Nothing there is logged above debug, so `info` keeps the faults and
///   drops the flood.
/// - `wie_core_arm::function`, a line per call into a registered native
///   function, which on a spinning title tracks the spin one for one.
/// - `jvm`, whose trace is a line per bytecode - every field read, every
///   resolve, every invoke and its result. It is the same kind of flood as the
///   interpreter's: a capture of 액션퍼즐패밀리1 taken to find a key press was
///   79% `jvm::jvm` trace and covered two tenths of a second, which is not
///   long enough to contain anything a person noticed. Its debug lines, which
///   name the classes and methods rather than the steps, are kept.
///
/// Everything else still arrives at trace, which is the point of the window.
const COLLECT_LOG_DIRECTIVE: &str = "trace,arm32_cpu=warn,jni=warn,jvm=debug,wie_lgt::hot=info,wie_core_arm::function=debug";

/// Lets the player swap the log filter at runtime, so capturing a module's
/// debug/trace detail no longer means editing the default above and rebuilding.
static RELOAD_HANDLE: OnceLock<reload::Handle<EnvFilter, Registry>> = OnceLock::new();

/// Whether a collection window is open, so the UI can show which button to
/// offer and a second press does not start over.
static COLLECTING: Mutex<bool> = Mutex::new(false);

/// The directive the filter is currently running, so the UI can show it.
static CURRENT_DIRECTIVE: Mutex<String> = Mutex::new(String::new());

/// Installs the subscriber once. Safe to call from every entry point.
pub fn init() {
    INIT.call_once(|| {
        // Honour `RUST_LOG` when it is set and non-empty; otherwise the default.
        let directive = std::env::var(EnvFilter::DEFAULT_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_LOG_DIRECTIVE.to_owned());

        // `EnvFilter::new` is lenient, matching the previous startup behaviour:
        // a bad directive here degrades rather than dropping logging entirely.
        let (filter, handle) = reload::Layer::new(EnvFilter::new(&directive));

        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer().with_ansi(false).with_writer(make_writer))
            .init();

        let _ = RELOAD_HANDLE.set(handle);
        *current_directive() = directive;

        // A panic is caught at the JNI boundary and shown to the player as a
        // generic message, and the default hook prints to stderr, which never
        // reaches the saved log. Route the panic's message and location into the
        // log so a caught panic is diagnosable from the capture alone.
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let location = info
                .location()
                .map(|location| format!("{}:{}", location.file(), location.line()))
                .unwrap_or_else(|| "unknown location".to_owned());

            let message = info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "<non-string panic payload>".to_owned());

            tracing::error!("panic at {location}: {message}");

            default_hook(info);
        }));
    });
}

fn current_directive() -> std::sync::MutexGuard<'static, String> {
    CURRENT_DIRECTIVE.lock().unwrap_or_else(|x| x.into_inner())
}

/// The log filter the capture is currently running.
pub fn filter() -> String {
    current_directive().clone()
}

/// Swaps the live log filter without a rebuild. An empty directive restores the
/// built-in default, so the caller has a single source of truth for it. Returns
/// the reason parsing rejected the directive, so it can be shown. Parsing is
/// strict here (unlike startup) so a typo is reported instead of silently
/// dropped.
pub fn set_filter(directive: &str) -> core::result::Result<(), String> {
    let directive = if directive.trim().is_empty() { DEFAULT_LOG_DIRECTIVE } else { directive };

    let parsed = EnvFilter::builder().parse(directive).map_err(|error| error.to_string())?;

    let handle = RELOAD_HANDLE.get().ok_or_else(|| "logging is not initialised yet".to_owned())?;
    handle.reload(parsed).map_err(|error| error.to_string())?;

    *current_directive() = directive.to_owned();
    Ok(())
}

/// Opens a collection window: throws away what is held, turns everything on,
/// and starts recording from here.
///
/// The pair to [`stop_collecting`]. Between them the log is one bounded stretch
/// of play with nothing filtered out of it, which is what lets one run answer a
/// question about an area nobody had thought to instrument.
///
/// The window opens either way; widening the filter is what can fail, and the
/// reason comes back so it can be shown. A window recorded under the old filter
/// is still a window, and the bounded stretch is the half of this that matters;
/// refusing to start because the filter would not widen would take that away
/// too, and quietly.
///
/// **A filter the player set themselves is kept, not widened over.** The window
/// is bounded in size, so how wide it is decides how long it covers, and the
/// two questions a window gets asked want opposite answers. "Why did this title
/// not start" wants everything about the few hundred milliseconds after launch,
/// which is what widening is for. "Why did that just happen while I was
/// playing" wants the minutes leading up to a moment a person noticed - and at
/// trace, 액션퍼즐패밀리1 fills the whole window in a quarter of a second.
/// Typing `info` in the filter box and collecting narrows it to exactly the
/// second question: the input path, the unimplemented platform calls and the
/// faults all log at info or above, and nothing else does.
///
/// It is no longer the only way to get that far back, though - the record keeps
/// those lines in a lane of their own (see `NOTABLE_SHARE_PERCENT`), so a
/// window taken at full width still reaches back minutes for them. Setting the
/// filter buys the debug detail around them as well.
pub fn start_collecting() -> core::result::Result<(), String> {
    let chosen = filter();
    let widened = if chosen == DEFAULT_LOG_DIRECTIVE {
        set_filter(COLLECT_LOG_DIRECTIVE)
    } else {
        Ok(())
    };

    set_bounds(COLLECT_MAX_LINES, COLLECT_MAX_BYTES);

    reset();
    *collecting() = true;

    // Named in the log itself, because by the time the file is read the filter
    // has been put back and the header would otherwise describe the wrong one.
    tracing::info!("log collection started under {}", filter());

    // The probe is deliberately not armed here. Recording every branch for as
    // long as a window is open was affordable in neither sense: a title spends
    // almost all of its time in a loop that decides nothing, so the trace filled
    // the window with that loop and pushed out the lines a log is opened for -
    // one capture came back thirty thousand lines of `4a96c>4a8de` and three of
    // network - and the core has to interpret every instruction while a trace
    // runs, which took a handset to thirteen tick/s. It also hid the watches:
    // `reached` is only looked at while nothing is armed, so an address hung on
    // `nativeSetProbeWatches` could never fire underneath it.
    //
    // What arms a trace instead is something that knows the moment is worth
    // one: `over_an_unanswered_request` for a request the gateway could not
    // answer, and a watch for an address a question is already about. Both are
    // bounded, and both start at the parse rather than at a button press.

    widened
}

/// Closes the window and puts the filter back where it was.
///
/// The log is left as it stands: the caller saves it, and what it saves is what
/// happened between the two presses.
pub fn stop_collecting() -> core::result::Result<(), String> {
    // Pressed without a window open, this is the plain save the single log
    // button used to be, and putting the filter back would throw away one the
    // player had set by hand. Only a window this opened gets closed.
    if !*collecting() {
        return Ok(());
    }

    tracing::info!("log collection stopped");

    wie_backend::probe::disarm();
    *collecting() = false;

    // Back to the always-on bounds, which are what the crash auto-save is
    // written from. What the window collected is left as it stands - the caller
    // saves it next, and raising the bounds cannot drop any of it.
    set_bounds(MAX_LINES, MAX_BYTES);

    // Closed regardless of whether the filter goes back, for the same reason
    // the window opens regardless of whether it widened.
    set_filter("")
}

/// Whether a collection window is open.
pub fn collecting_now() -> bool {
    *collecting()
}

fn collecting() -> std::sync::MutexGuard<'static, bool> {
    COLLECTING.lock().unwrap_or_else(|x| x.into_inner())
}

/// Lines kept, and bytes, for the always-on capture. A run that overruns either
/// drops its oldest, which is the right end to lose: a title that fails at
/// startup fits whole, and one that hangs is diagnosed from where it stopped.
const MAX_LINES: usize = 300_000;
const MAX_BYTES: usize = 48 << 20;

/// The same, while a window is open.
///
/// Held far lower, because a window is where the memory goes: every area at
/// trace, plus the branches the emulated code takes. At the always-on bounds
/// that is upwards of sixty megabytes of `String` on a handset that is also
/// running a game, and the phone is entitled to end the process over it.
///
/// The window loses only its own beginning, which is the half it can afford to:
/// a window is opened for something at its end. 놈ZERO's authentication landed
/// on line 93,831 of 99,671, and is inside this.
const COLLECT_MAX_LINES: usize = 80_000;
const COLLECT_MAX_BYTES: usize = 12 << 20;

/// The share of the bounds the branch trace is allowed, in percent.
///
/// The trace and the rest of the log are both worth keeping and they arrive at
/// wildly different rates: three windows captured off a handset came back
/// 99.8% branch trace, 50,650 lines of it against 117 of everything else, and
/// the half a million lines that went over the bound were the half the window
/// had been opened to read. One bound shared first-come cannot hold both - the
/// denser one decides the proportion, and it decides it at 500:1.
///
/// So each gets its own, and each keeps its own most recent. The trace still
/// runs to the end of the window and still loses only its beginning; what
/// changes is that losing it no longer costs the log the minutes around it.
/// Sized so the trace keeps roughly what it kept before - it was already
/// getting about a second of the bound - and the log gets back the rest.
const TRACE_SHARE_PERCENT: usize = 60;

/// The share of the bounds the notable lines are allowed, in percent.
///
/// The same argument as the branch trace's, one level down. A capture is mostly
/// debug and trace - the per-call echo of graphics, strings, bytecode and class
/// loading - and that is three orders of magnitude denser than the handful of
/// lines a question asked afterwards is actually about: the input path, the
/// unimplemented platform calls, the faults, the per-second counters. Sharing
/// one bound first-come, the dense half decides how long the capture covers,
/// and it decided it at a quarter of a second. Four windows of 액션퍼즐패밀리1
/// were taken to find a key press the player saw fire by itself; all four came
/// back 26,000 lines of `String` and bytecode covering 0.27 seconds, with
/// 210,132 lines dropped behind them, and the press was in none of them.
///
/// So the lines at info and above get a lane of their own, and losing the flood
/// no longer costs the log the minutes around it. At this title's rate - about
/// 330 a second, itself mostly per-frame warnings - the always-on capture holds
/// a little over two minutes of them and a window holds half a minute, which is
/// long enough to contain something a person noticed and said so.
const NOTABLE_SHARE_PERCENT: usize = 15;

/// Whether a formatted line belongs in that lane, which is decided by its
/// level.
///
/// Read back out of the text for the same reason the branch trace's target is:
/// the record is fed formatted lines, and by the time one arrives its event is
/// gone. The level is the field after the timestamp; a line without one - the
/// header the save writes - is not notable and travels with the log.
fn is_notable(line: &str) -> bool {
    let mut fields = line.split_whitespace();
    let (_time, level) = (fields.next(), fields.next());

    matches!(level, Some("INFO" | "WARN" | "ERROR"))
}

/// How a branch-trace line is told apart from any other, which is by the target
/// the format layer prints it under.
///
/// The record is fed formatted lines - by the time one arrives its event is
/// gone - so the target has to be read back out of the text. It sits between
/// the span list and the message, spaced exactly as written here.
const TRACE_TARGET: &str = " wie_backend::probe: ";

/// Lines the record is currently allowed, and bytes, for everything that is not
/// the branch trace. Swapped by [`start_collecting`] and [`stop_collecting`].
static CAP_LINES: AtomicUsize = AtomicUsize::new(MAX_LINES - MAX_LINES * TRACE_SHARE_PERCENT / 100 - MAX_LINES * NOTABLE_SHARE_PERCENT / 100);
static CAP_BYTES: AtomicUsize = AtomicUsize::new(MAX_BYTES - MAX_BYTES * TRACE_SHARE_PERCENT / 100 - MAX_BYTES * NOTABLE_SHARE_PERCENT / 100);

/// The same, for the branch trace.
static TRACE_CAP_LINES: AtomicUsize = AtomicUsize::new(MAX_LINES * TRACE_SHARE_PERCENT / 100);
static TRACE_CAP_BYTES: AtomicUsize = AtomicUsize::new(MAX_BYTES * TRACE_SHARE_PERCENT / 100);

/// The same, for the lines at info and above.
static NOTABLE_CAP_LINES: AtomicUsize = AtomicUsize::new(MAX_LINES * NOTABLE_SHARE_PERCENT / 100);
static NOTABLE_CAP_BYTES: AtomicUsize = AtomicUsize::new(MAX_BYTES * NOTABLE_SHARE_PERCENT / 100);

/// Sets every budget from one total, giving each lane its share. What is left
/// after the two dense lanes have theirs is the log's.
fn set_bounds(max_lines: usize, max_bytes: usize) {
    let trace_lines = max_lines * TRACE_SHARE_PERCENT / 100;
    let trace_bytes = max_bytes * TRACE_SHARE_PERCENT / 100;
    let notable_lines = max_lines * NOTABLE_SHARE_PERCENT / 100;
    let notable_bytes = max_bytes * NOTABLE_SHARE_PERCENT / 100;

    TRACE_CAP_LINES.store(trace_lines, Ordering::Relaxed);
    TRACE_CAP_BYTES.store(trace_bytes, Ordering::Relaxed);
    NOTABLE_CAP_LINES.store(notable_lines, Ordering::Relaxed);
    NOTABLE_CAP_BYTES.store(notable_bytes, Ordering::Relaxed);
    CAP_LINES.store(max_lines - trace_lines - notable_lines, Ordering::Relaxed);
    CAP_BYTES.store(max_bytes - trace_bytes - notable_bytes, Ordering::Relaxed);
}

/// One bounded queue of lines: what it holds, what it has had to drop, and how
/// much of the bound it is using.
#[derive(Default)]
struct Lines {
    lines: VecDeque<String>,
    bytes: usize,
    /// Lines dropped to stay inside the bounds, so the snapshot can say so.
    dropped: usize,
}

impl Lines {
    fn push(&mut self, line: String, max_lines: usize, max_bytes: usize) {
        self.bytes += line.len() + 1;
        self.lines.push_back(line);

        while self.lines.len() > max_lines || self.bytes > max_bytes {
            let Some(oldest) = self.lines.pop_front() else {
                break;
            };
            self.bytes -= oldest.len() + 1;
            self.dropped += 1;
        }
    }
}

#[derive(Default)]
struct Record {
    log: Lines,
    /// The branch trace, held to its own share of the bounds so it cannot
    /// evict the log around it. Merged back by time in [`snapshot`], so what
    /// is read is still one interleaved run.
    trace: Lines,
    /// The lines at info and above, held apart from the debug and trace flood
    /// for the same reason and merged back the same way. This is the lane a
    /// question asked after the fact is answered from, so it is the one that
    /// has to still cover the moment.
    notable: Lines,
}

impl Record {
    fn push(&mut self, line: String) {
        // The trace first: its lines carry a level too, and what makes one a
        // trace line is where it came from, not how loud it is.
        if line.contains(TRACE_TARGET) {
            self.trace
                .push(line, TRACE_CAP_LINES.load(Ordering::Relaxed), TRACE_CAP_BYTES.load(Ordering::Relaxed));
        } else if is_notable(&line) {
            self.notable
                .push(line, NOTABLE_CAP_LINES.load(Ordering::Relaxed), NOTABLE_CAP_BYTES.load(Ordering::Relaxed));
        } else {
            self.log.push(line, CAP_LINES.load(Ordering::Relaxed), CAP_BYTES.load(Ordering::Relaxed));
        }
    }
}

/// The timestamp a line is ordered by: the leading field the format layer
/// writes. A line without one orders by its own text, which is only the header
/// lines nothing else is interleaved with.
fn line_time(line: &str) -> &str {
    line.split_once(char::is_whitespace).map_or(line, |(time, _)| time)
}

static RECORD: LazyLock<Mutex<Record>> = LazyLock::new(|| Mutex::new(Record::default()));

fn record() -> std::sync::MutexGuard<'static, Record> {
    RECORD.lock().unwrap_or_else(|x| x.into_inner())
}

/// Starts the log over, so what is saved covers one run of one game.
pub fn reset() {
    *record() = Record::default();
}

/// Everything logged since the last [`reset`], the lanes merged back into one
/// run by time.
///
/// They are held apart only so that none can crowd the others out of its
/// bounds; read back they are one run, and a line is worth reading against the
/// branches that followed it and the frames that were being drawn around it.
pub fn snapshot() -> String {
    let record = record();

    let mut out = String::with_capacity(record.log.bytes + record.trace.bytes + record.notable.bytes + 256);

    // Each queue says what it had to drop, because the three ends mean
    // different things: the log's beginning going means the window ran long,
    // the trace's going means the emulated code branched harder than a window
    // can hold, and the notable lane's going is the only one that means the
    // capture no longer reaches back to what was asked about.
    if record.log.dropped > 0 {
        out.push_str(&format!(
            "[{} earlier lines dropped to stay inside the log's bounds]\n",
            record.log.dropped
        ));
    }
    if record.trace.dropped > 0 {
        out.push_str(&format!(
            "[{} earlier branch-trace lines dropped to stay inside the trace's own bounds]\n",
            record.trace.dropped
        ));
    }
    if record.notable.dropped > 0 {
        out.push_str(&format!(
            "[{} earlier info-and-above lines dropped to stay inside their own bounds]\n",
            record.notable.dropped
        ));
    }

    // Ties go to the earliest lane listed, so lines sharing a timestamp keep
    // the order the lanes were written in rather than shuffling between saves.
    let mut lanes = [
        record.log.lines.iter().peekable(),
        record.notable.lines.iter().peekable(),
        record.trace.lines.iter().peekable(),
    ];

    loop {
        let mut oldest: Option<(usize, &String)> = None;
        for (index, lane) in lanes.iter_mut().enumerate() {
            // Copied out of the peek, so holding it does not keep the lane
            // borrowed while the next one is looked at.
            let Some(line) = lane.peek().copied() else { continue };

            match oldest {
                Some((_, best)) if line_time(best) <= line_time(line) => {}
                _ => oldest = Some((index, line)),
            }
        }

        let Some((index, _)) = oldest else { break };
        let Some(line) = lanes[index].next() else { break };

        out.push_str(line);
        out.push('\n');
    }

    out
}

fn make_writer() -> Tee {
    Tee {
        inner: platform_writer(),
        pending: Vec::new(),
    }
}

/// Writes each line to the platform's log and to [`RECORD`].
struct Tee {
    inner: PlatformWriter,
    /// `tracing` writes a line in several pieces, and a line is the unit both
    /// destinations want.
    pending: Vec<u8>,
}

impl io::Write for Tee {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(buf)?;

        self.pending.extend_from_slice(buf);
        while let Some(end) = self.pending.iter().position(|&x| x == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).take(end).collect();
            record().push(String::from_utf8_lossy(&line).into_owned());
        }

        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        if !self.pending.is_empty() {
            let line = std::mem::take(&mut self.pending);
            record().push(String::from_utf8_lossy(&line).into_owned());
        }

        self.inner.flush()
    }
}

impl Drop for Tee {
    fn drop(&mut self) {
        use io::Write as _;

        let _ = self.flush();
    }
}

#[cfg(target_os = "android")]
type PlatformWriter = logcat::Writer;

#[cfg(not(target_os = "android"))]
type PlatformWriter = io::Stderr;

#[cfg(target_os = "android")]
fn platform_writer() -> PlatformWriter {
    logcat::Writer::default()
}

#[cfg(not(target_os = "android"))]
fn platform_writer() -> PlatformWriter {
    io::stderr()
}

#[cfg(target_os = "android")]
mod logcat {
    use std::{
        ffi::{CString, c_char, c_int},
        io,
    };

    // liblog is part of the NDK sysroot and always present on device.
    #[link(name = "log")]
    unsafe extern "C" {
        fn __android_log_write(priority: c_int, tag: *const c_char, text: *const c_char) -> c_int;
    }

    /// ANDROID_LOG_INFO. The level is already in the formatted line, and
    /// logcat filtering happens by tag here.
    const PRIORITY_INFO: c_int = 4;

    /// Buffers until a newline, because logcat treats every write as one
    /// entry while `tracing` writes a line in several pieces.
    #[derive(Default)]
    pub struct Writer {
        buffer: Vec<u8>,
    }

    impl Writer {
        fn emit(&self, line: &[u8]) {
            // Interior nul bytes would truncate the line; they never appear in
            // formatted output, but a lossy replacement is better than a panic.
            let Ok(tag) = CString::new("WIE") else { return };
            let Ok(text) = CString::new(line.iter().map(|&x| if x == 0 { b'?' } else { x }).collect::<Vec<_>>()) else {
                return;
            };

            unsafe { __android_log_write(PRIORITY_INFO, tag.as_ptr(), text.as_ptr()) };
        }
    }

    impl io::Write for Writer {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.buffer.extend_from_slice(buf);

            while let Some(end) = self.buffer.iter().position(|&x| x == b'\n') {
                let line: Vec<u8> = self.buffer.drain(..=end).take(end).collect();
                self.emit(&line);
            }

            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            if !self.buffer.is_empty() {
                let line = std::mem::take(&mut self.buffer);
                self.emit(&line);
            }

            Ok(())
        }
    }

    impl Drop for Writer {
        fn drop(&mut self) {
            use io::Write as _;

            let _ = self.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::sync::atomic::Ordering;

    use super::{DEFAULT_LOG_DIRECTIVE, MAX_LINES, collecting_now, make_writer, reset, snapshot, start_collecting, stop_collecting};

    #[test]
    fn collect_log_directive_parses() {
        // A malformed directive would be refused at the moment the player
        // pressed collect, leaving them recording under the old filter and
        // finding out only when the file arrived without what they needed.
        tracing_subscriber::EnvFilter::builder()
            .parse(super::COLLECT_LOG_DIRECTIVE)
            .expect("collect log directive must be valid");
    }

    /// The window is bounded in size, so the floods that can fill it in
    /// milliseconds have to be held back even though the window has an end.
    #[test]
    fn collect_log_directive_holds_back_the_known_floods() {
        for flood in ["arm32_cpu=", "jni=", "wie_lgt::hot=", "wie_core_arm::function="] {
            assert!(super::COLLECT_LOG_DIRECTIVE.contains(flood), "{flood} is not held back");
        }

        // Still a trace window everywhere else, which is what it is for.
        assert!(super::COLLECT_LOG_DIRECTIVE.starts_with("trace,"));
    }

    #[test]
    fn default_log_directive_parses() {
        // A malformed directive would be dropped, quietly reverting the full-log
        // capture to bare info and losing the debug detail it is meant to keep.
        tracing_subscriber::EnvFilter::builder()
            .parse(DEFAULT_LOG_DIRECTIVE)
            .expect("default log directive must be valid");
    }

    /// The record is one thing shared by the process, so two tests writing to
    /// it at once would each see the other's lines.
    static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn the_log_is_kept_and_bounded() {
        let _guard = ONE_AT_A_TIME.lock().unwrap_or_else(|x| x.into_inner());
        reset();
        assert_eq!(snapshot(), "");

        let mut writer = make_writer();
        // A line arriving in pieces, which is how `tracing` writes one.
        writer.write_all(b"hello ").unwrap();
        writer.write_all(b"world\n").unwrap();

        assert_eq!(snapshot(), "hello world\n");

        // A tail with no newline is still kept, on flush.
        writer.write_all(b"unfinished").unwrap();
        writer.flush().unwrap();
        assert!(snapshot().contains("unfinished"));

        reset();
        // Everything that is not the branch trace is bounded by its own share
        // of the capture, and loses its oldest first.
        let cap = super::CAP_LINES.load(Ordering::Relaxed);
        for index in 0..cap + 50 {
            writer.write_all(format!("line {index}\n").as_bytes()).unwrap();
        }

        let log = snapshot();
        assert!(log.starts_with("[50 earlier lines dropped"), "{}", &log[..64]);
        assert!(log.contains(&format!("line {}", cap + 49)), "the newest line is missing");
        assert!(!log.contains("line 0\n"), "the oldest line was kept");

        reset();
    }

    #[test]
    fn a_window_covers_what_happened_inside_it() {
        let _guard = ONE_AT_A_TIME.lock().unwrap_or_else(|x| x.into_inner());
        reset();

        let mut writer = make_writer();
        writer.write_all(b"before\n").unwrap();

        assert!(!collecting_now());
        // The subscriber is not installed in this test binary, so the filter
        // swap has nowhere to go; what is being checked is the record, which is
        // where the window shows up.
        let _ = start_collecting();
        assert!(collecting_now());

        writer.write_all(b"inside\n").unwrap();

        let _ = stop_collecting();
        assert!(!collecting_now());

        let log = snapshot();
        assert!(log.contains("inside"), "the window's own lines are missing");
        assert!(!log.contains("before"), "the window kept what came before it");

        reset();
    }

    #[test]
    fn a_window_is_held_to_tighter_bounds_than_the_capture_around_it() {
        let _guard = ONE_AT_A_TIME.lock().unwrap_or_else(|x| x.into_inner());
        wie_backend::probe::disarm();

        // Every budget together is the bound; each lane holds its own share.
        assert_eq!(budget(), MAX_LINES);

        let _ = start_collecting();
        // A window is every area at trace plus the branches the code takes, and
        // that is where the memory goes.
        assert_eq!(budget(), super::COLLECT_MAX_LINES);
        assert!(super::COLLECT_MAX_LINES < MAX_LINES);

        let cap = super::CAP_LINES.load(Ordering::Relaxed);
        let mut writer = make_writer();
        for index in 0..cap + 40 {
            writer.write_all(format!("line {index}\n").as_bytes()).unwrap();
        }

        let log = snapshot();
        assert!(log.starts_with("[4"), "the window kept more than its bound: {}", &log[..48]);
        assert!(log.contains(&format!("line {}", cap + 39)), "the newest line is missing");

        let _ = stop_collecting();
        // Back to the bounds the crash auto-save is written from.
        assert_eq!(budget(), MAX_LINES);

        reset();
    }

    /// What every lane is allowed together, which is the bound.
    fn budget() -> usize {
        super::CAP_LINES.load(Ordering::Relaxed) + super::TRACE_CAP_LINES.load(Ordering::Relaxed) + super::NOTABLE_CAP_LINES.load(Ordering::Relaxed)
    }

    /// The branch trace arrives hundreds of times faster than everything else -
    /// three windows off a handset came back 99.8% trace - so sharing one bound
    /// first-come let it evict the whole log around it. Each holding its own
    /// share is what stops that.
    #[test]
    fn a_flood_of_branches_does_not_evict_the_log_around_it() {
        let _guard = ONE_AT_A_TIME.lock().unwrap_or_else(|x| x.into_inner());
        wie_backend::probe::disarm();
        reset();

        let _ = start_collecting();
        let trace_cap = super::TRACE_CAP_LINES.load(Ordering::Relaxed);

        let mut writer = make_writer();
        writer
            .write_all(b"2000-01-01T00:00:00.000000Z  INFO the line the window was opened for\n")
            .unwrap();

        // Enough trace to have swallowed the bound whole under one budget.
        for index in 0..trace_cap * 2 {
            writer
                .write_all(format!("2000-01-01T00:00:01.{index:06}Z  INFO{} branches {index}\n", super::TRACE_TARGET).as_bytes())
                .unwrap();
        }

        let log = snapshot();
        assert!(log.contains("the line the window was opened for"), "the trace evicted the log around it");
        assert!(
            log.contains(&format!("branches {}", trace_cap * 2 - 1)),
            "the trace lost its own most recent"
        );
        assert!(!log.contains("branches 0 "), "the trace kept more than its share");

        let _ = stop_collecting();
        reset();
    }

    /// Held apart only so neither crowds the other out; read back, one run.
    #[test]
    fn the_trace_reads_back_interleaved_with_the_log() {
        let _guard = ONE_AT_A_TIME.lock().unwrap_or_else(|x| x.into_inner());
        wie_backend::probe::disarm();
        reset();

        let mut writer = make_writer();
        for (time, line) in [
            ("00.000000", "first".to_owned()),
            ("01.000000", format!("{}branch", super::TRACE_TARGET)),
            ("02.000000", "second".to_owned()),
            ("03.000000", format!("{}branch again", super::TRACE_TARGET)),
            ("04.000000", "third".to_owned()),
        ] {
            writer.write_all(format!("2000-01-01T00:00:{time}Z  INFO {line}\n").as_bytes()).unwrap();
        }

        let log = snapshot();
        let times: Vec<&str> = log.lines().filter_map(|line| line.get(17..19)).collect();
        assert_eq!(times, ["00", "01", "02", "03", "04"], "the merge did not put the run back in order");

        reset();
    }

    /// The debug and trace flood is to the lines a question is about what the
    /// branch trace is to the log: three orders of magnitude denser, and under
    /// one shared bound it decided how far back a capture reached. Four windows
    /// of 액션퍼즐패밀리1 were taken to find a key press and all four covered
    /// 0.27 seconds of `String` and bytecode, with the press behind them.
    #[test]
    fn a_flood_of_debug_does_not_evict_the_lines_a_question_is_about() {
        let _guard = ONE_AT_A_TIME.lock().unwrap_or_else(|x| x.into_inner());
        wie_backend::probe::disarm();
        reset();

        let _ = start_collecting();
        let flood_cap = super::CAP_LINES.load(Ordering::Relaxed);

        let mut writer = make_writer();
        writer.write_all(b"2000-01-01T00:00:00.000000Z  INFO input: NUM2 down\n").unwrap();

        // Enough per-frame debug to have swallowed the bound whole before.
        for index in 0..flood_cap * 2 {
            writer
                .write_all(format!("2000-01-01T00:00:01.{index:06}Z DEBUG drew a frame {index}\n").as_bytes())
                .unwrap();
        }

        let log = snapshot();
        assert!(log.contains("input: NUM2 down"), "the flood evicted the press the window was opened for");
        assert!(
            log.contains(&format!("frame {}", flood_cap * 2 - 1)),
            "the flood lost its own most recent"
        );
        assert!(!log.contains("frame 0\n"), "the flood kept more than its share");

        let _ = stop_collecting();
        reset();
    }

    /// Held apart only so the flood cannot crowd them out; read back, one run.
    #[test]
    fn the_notable_lines_read_back_interleaved_with_the_flood() {
        let _guard = ONE_AT_A_TIME.lock().unwrap_or_else(|x| x.into_inner());
        wie_backend::probe::disarm();
        reset();

        let mut writer = make_writer();
        for (time, level, line) in [
            ("00.000000", "DEBUG", "drew a frame"),
            ("01.000000", " INFO", "input: NUM2 down"),
            ("02.000000", "DEBUG", "drew another frame"),
            ("03.000000", " WARN", "lgt_java_diag(index=0xfd)"),
            ("04.000000", "DEBUG", "drew one more frame"),
        ] {
            writer.write_all(format!("2000-01-01T00:00:{time}Z {level} {line}\n").as_bytes()).unwrap();
        }

        let log = snapshot();
        let times: Vec<&str> = log.lines().filter_map(|line| line.get(17..19)).collect();
        assert_eq!(times, ["00", "01", "02", "03", "04"], "the merge did not put the run back in order");

        reset();
    }

    /// The level decides the lane, and it is read back out of the formatted
    /// text because the event is gone by the time the record sees the line.
    #[test]
    fn the_level_decides_which_lane_a_line_travels_in() {
        for line in [
            "2000-01-01T00:00:00.000000Z  INFO input: NUM2 down",
            "2000-01-01T00:00:00.000000Z  WARN wie_lgt::runtime::init: lgt_java_diag(index=0x5b)",
            "2000-01-01T00:00:00.000000Z ERROR panic at src/lib.rs:1: nope",
        ] {
            assert!(super::is_notable(line), "{line} should be kept apart from the flood");
        }

        for line in [
            "2000-01-01T00:00:00.000000Z DEBUG jvm::jvm: getfield",
            "2000-01-01T00:00:00.000000Z TRACE jvm::jvm: aload_0",
            // The header the save writes has no level at all.
            "game: 액션퍼즐패밀리1",
            "",
        ] {
            assert!(!super::is_notable(line), "{line} should travel with the flood");
        }
    }

    #[test]
    fn a_window_leaves_the_probe_asleep() {
        let _guard = ONE_AT_A_TIME.lock().unwrap_or_else(|x| x.into_inner());
        wie_backend::probe::disarm();

        // A window used to arm a trace over the whole of itself. A title spends
        // almost all of its time in a loop that decides nothing, so what came
        // back was that loop: one capture was thirty thousand lines of it
        // against three of network, and the core interprets every instruction
        // while a trace runs, which took a handset to thirteen tick/s. A window
        // is for the log; what is worth a trace arms one.
        let _ = start_collecting();
        assert!(!wie_backend::probe::is_armed(), "a window should not trace of its own accord");

        let _ = stop_collecting();
        reset();
    }

    #[test]
    fn a_watch_can_still_fire_inside_a_window() {
        let _guard = ONE_AT_A_TIME.lock().unwrap_or_else(|x| x.into_inner());
        wie_backend::probe::disarm();

        // Which it could not while a window armed one of its own: a watched
        // address is only looked for while nothing is armed, so an address hung
        // on the window's own trace could never be reached.
        let _ = start_collecting();
        wie_backend::probe::set_watches("pc:3a1fc").unwrap();
        wie_backend::probe::reached(0x3a1fc);

        assert!(wie_backend::probe::is_armed(), "a watched address should start a trace inside a window");

        wie_backend::probe::clear_watches();
        let _ = stop_collecting();
        wie_backend::probe::disarm();
        reset();
    }
}
