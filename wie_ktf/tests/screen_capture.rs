//! A headless probe for KTF archives.
//!
//! KTF had no way to run a real title without a window, so a title that dies on
//! its first frame could only be diagnosed by guesswork. This runs an archive
//! the way `wie_cli` would, counts the frames it paints, and reports the error
//! it stopped on.
//!
//! It is driven by the environment so any archive can be pointed at it without
//! touching the tree:
//!
//! - `WIE_KTF_ARCHIVE` - path to the archive to run. Unset, the probe is a
//!   no-op, which is what keeps it out of the way of an ordinary `cargo test`.
//! - `WIE_TICKS` - how many ticks to run (default 20000). A tick is at least
//!   eight milliseconds of guest time and more when the guest executes for
//!   longer, so a tick count is now a duration: 500 is about four seconds of
//!   guest time. It also costs what that time costs - a title that computes
//!   through its whole tick pays for every instruction - so the counts worth
//!   using here are much smaller than they were.
//!
//!   Every run reports the guest time it covered as `guest_ms`, which is the
//!   conversion a script needs. Measured across the local archives a tick is
//!   8.0 to 9.1 milliseconds, so a delay a title measures in guest time - the
//!   input method's 900ms commit, say - is a little over a hundred ticks: press
//!   the same key at tick 200 and tick 400 and the second press starts a new
//!   character, at tick 200 and 260 and it walks the multi-tap ring.
//! - `WIE_SHOT` - where to write the last painted frame, as a binary PPM.
//! - `WIE_KEY`/`WIE_PRESS_TICK` - one key press, to get past a title's notice.
//! - `WIE_SCRIPT` - a walk into the title instead: `tick:KEY` pairs separated
//!   by commas, e.g. `1500:OK,3000:OK,4500:NUM1`. Each press is held 20 ticks.
//! - `WIE_SHOT_DIR` - a frame written ~400 ticks after each scripted press, so
//!   every step of the walk is visible rather than only where it ended.
//! - `WIE_SCRIPT2` - launch the archive twice over one handset's storage,
//!   `WIE_SCRIPT` driving the first launch and this the second. A title that
//!   installs itself on its first run and asks to be started again needs this
//!   to be reachable at all: 드래곤하트 paints
//!   `게임이 설치 되었습니다. 다시 실행해 주세요.` and goes no further, whatever
//!   it is sent. Only the second launch is captured.
//! - `WIE_TICKS2` - the second launch's tick budget, when it needs a different
//!   one from the first (default: the same).
//! - `WIE_REDRAW_ON_REQUEST` - feed a host paint only when the title asks for
//!   one, which is what the Android frontend does. Off, a paint arrives every
//!   forty ticks regardless, and a repaint the runtime loses is covered up.

use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use test_utils::{TestPlatform, TestPlatformEvent, TestPlatformState};
use wie_backend::{
    AudioSink, DatabaseRepository, Emulator, Event, Filesystem, Instant, KeyCode, Network, NetworkError, NetworkPoll, Options, Platform, Screen,
    canvas::Image, extract_zip,
};
use wie_ktf::KtfEmulator;
use wie_util::Result;

#[derive(Default)]
struct Captured {
    frames: u32,
    width: u32,
    height: u32,
    /// How many distinct colours the last frame held - a blank screen is one.
    last_colors: usize,
    last_pixels: Vec<u8>,
}

#[derive(Default, Clone)]
struct CaptureScreen {
    captured: Arc<Mutex<Captured>>,
    /// Set by `request_redraw`, taken by the run loop under
    /// `WIE_REDRAW_ON_REQUEST`.
    requested: Arc<AtomicBool>,
    /// The panel the archive under capture names for itself, when it names one.
    native_size: Option<(u32, u32)>,
}

impl Screen for CaptureScreen {
    fn request_redraw(&self) -> Result<()> {
        self.requested.store(true, Ordering::SeqCst);

        Ok(())
    }

    fn paint(&self, image: &dyn Image) {
        let mut captured = self.captured.lock().unwrap();

        captured.frames += 1;
        captured.width = image.width();
        captured.height = image.height();

        let mut colors = std::collections::BTreeSet::new();
        let mut pixels = Vec::with_capacity((image.width() * image.height() * 3) as usize);
        for color in image.colors() {
            colors.insert(((color.r as u32) << 16) | ((color.g as u32) << 8) | color.b as u32);
            pixels.push(color.r);
            pixels.push(color.g);
            pixels.push(color.b);
        }

        captured.last_colors = colors.len();
        captured.last_pixels = pixels;
    }

    fn width(&self) -> u32 {
        let (width, _) = self.native_size.unwrap_or((240, 320));

        self.captured.lock().unwrap().width.max(width)
    }

    fn height(&self) -> u32 {
        let (_, height) = self.native_size.unwrap_or((240, 320));

        self.captured.lock().unwrap().height.max(height)
    }
}

struct CapturePlatform {
    inner: TestPlatform,
    screen: CaptureScreen,
    clock: Arc<ProbeClock>,
    network: CaptureNetwork,
}

/// A network that hands out descriptors and reaches nothing.
///
/// The runtime answers some connections in process - a local endpoint, or a
/// billing gateway - but it still asks the platform for the socket those
/// connections are carried on, and `Platform::network` is `None` by default. A
/// capture over that default never reaches the in-process paths at all:
/// 데몬헌터's authentication asks for its socket, is told there is no network,
/// and refuses to go on without a byte having been written.
///
/// So this hands out descriptors and nothing else. Everything that would reach
/// a host fails, which is what a capture wants: what it records came from the
/// answer this run gives rather than from somewhere off the machine.
///
/// A connect fails the way a real stack fails it. No stack knows a host is out
/// of reach before it has tried, so the answer is `Pending` and the failure
/// follows as `ConnectFailed`. Returning the error straight from `connect`
/// skipped the callback entirely, and a title that watches only the callback -
/// 드래곤아이즈2 reads none of the return values - waited on a failure it was
/// never told about, which reads in a capture exactly like a title that hangs
/// on its own.
#[derive(Default)]
struct CaptureNetwork {
    next: AtomicU64,
    events: std::sync::Mutex<std::collections::VecDeque<wie_backend::NetworkEvent>>,
}

impl Network for CaptureNetwork {
    fn socket(&self, _family: i32, _socket_type: i32) -> std::result::Result<i32, NetworkError> {
        Ok(self.next.fetch_add(1, Ordering::SeqCst) as i32 + 1)
    }

    fn connect(&self, socket: i32, _address: u32, _port: u16) -> NetworkPoll<()> {
        // A real stack cannot know a host is out of reach before it has tried,
        // so the failure arrives as an event rather than as a return value.
        self.events
            .lock()
            .unwrap_or_else(|x| x.into_inner())
            .push_back(wie_backend::NetworkEvent::ConnectFailed(socket));

        NetworkPoll::Pending
    }

    fn bind(&self, _socket: i32, _address: u32, _port: u16) -> std::result::Result<(), NetworkError> {
        Err(NetworkError::Unsupported)
    }

    fn read(&self, _socket: i32, _buf: &mut [u8]) -> std::result::Result<usize, NetworkError> {
        Err(NetworkError::NotConnected)
    }

    fn write(&self, _socket: i32, _buf: &[u8]) -> std::result::Result<usize, NetworkError> {
        Err(NetworkError::NotConnected)
    }

    fn send_to(&self, _socket: i32, _buf: &[u8], _address: u32, _port: u16) -> std::result::Result<usize, NetworkError> {
        Err(NetworkError::NotConnected)
    }

    fn recv_from(&self, _socket: i32, _buf: &mut [u8]) -> std::result::Result<(usize, u32, u16), NetworkError> {
        Err(NetworkError::NotConnected)
    }

    fn close(&self, _socket: i32) -> std::result::Result<(), NetworkError> {
        Ok(())
    }

    fn resolve_host(&self, _host: &str, _query_id: u32) {}

    fn poll_event(&self) -> Option<wie_backend::NetworkEvent> {
        self.events.lock().unwrap_or_else(|x| x.into_inner()).pop_front()
    }
}

/// The probe's guest clock.
///
/// It used to be a millisecond per *read*, which made time a function of how
/// often a title looked at it: one that polls the clock in a delay loop had time
/// fly, one that never asks had it stand still, and nothing measured in those
/// milliseconds meant anything on a frontend where the clock is real.
///
/// It runs on the guest's own execution now. Within a tick time advances only as
/// the guest executes, which keeps `Executor::tick`'s own eight-millisecond bound
/// both reachable and bounded; at the end of a tick the probe folds that
/// execution into the base and charges at least a tick's worth, so a moment where
/// every task is asleep still ends. Taking whichever of the two had got further
/// instead makes the bound "execute until work catches up with the tick count",
/// which for a title that has been idle is not a bound at all.
#[derive(Default)]
struct ProbeClock {
    base_ms: AtomicU64,
    anchor: AtomicU64,
    reads: AtomicU64,
}

impl ProbeClock {
    fn now_ms(&self) -> u64 {
        let executed = wie_core_arm::EXECUTED_INSTRUCTIONS.load(Ordering::Relaxed);
        let since = executed.saturating_sub(self.anchor.load(Ordering::SeqCst));
        let reads = self.reads.fetch_add(1, Ordering::SeqCst);

        // Execution is what paces this, and reads are only a floor under it. The
        // floor has to be there: a task can make progress without executing a
        // guest instruction - `yield_now` and a sleep of nothing both do - and a
        // tick whose bound is guest time would never end while one of those runs,
        // because the time it is waiting for is time only the guest can buy.
        // A thousand reads to the millisecond is slow enough that a delay loop
        // polling the clock no longer has time fly, which is what the old one
        // gave it at a millisecond each.
        self.base_ms.load(Ordering::SeqCst) + (since / GUEST_STEPS_PER_MS).max(reads / READS_PER_MS)
    }

    /// Ends a tick: what the guest executed becomes time, and an idle tick still
    /// costs one.
    fn advance(&self) {
        let executed = wie_core_arm::EXECUTED_INSTRUCTIONS.load(Ordering::Relaxed);
        let worked = executed.saturating_sub(self.anchor.swap(executed, Ordering::SeqCst)) / GUEST_STEPS_PER_MS;
        let read = self.reads.swap(0, Ordering::SeqCst) / READS_PER_MS;

        self.base_ms.fetch_add(worked.max(read).max(MS_PER_TICK), Ordering::SeqCst);
    }
}

/// Guest instructions to a millisecond of guest time.
///
/// A handset ARM of this era at about one instruction a cycle. The figure is not
/// load-bearing to a factor of ten either way: it only has to be slow enough
/// that a guest which waits by spinning still pays execution for the wait, and
/// fast enough that a real delay fits inside a run worth doing.
const GUEST_STEPS_PER_MS: u64 = 10_000;

/// The least a tick costs, so a run where nothing executes still reaches its
/// deadlines.
const MS_PER_TICK: u64 = 8;

/// Clock reads to a millisecond, the floor under the rate above.
const READS_PER_MS: u64 = 1_000;

impl Platform for CapturePlatform {
    /// The in-process answers this run offers, taken from the environment the
    /// way the LGT probe takes them.
    ///
    /// `WIE_LOCAL_NET_ACK` is `1`/`any` for every connection or `host:port` for
    /// one, then any of `len=`, `type=`, `status=`, `prefix=` to describe its
    /// framing - see `wie_backend::AckEndpoint`. `WIE_LOCAL_NET_CAPTURE` takes
    /// a connection the same way but records what the title sends instead of
    /// answering it, which is how a protocol gets read in the first place: a
    /// recorded connection never answers, so a title waiting on a reply waits.
    ///
    /// KTF titles reach their servers through the same `socket_connect` LGT
    /// does, so a title whose server is gone - 데몬헌터's authentication, say -
    /// can be answered here without a network.
    fn local_endpoints(&self) -> Vec<Box<dyn wie_backend::LocalEndpoint>> {
        let mut endpoints: Vec<Box<dyn wie_backend::LocalEndpoint>> = Vec::new();

        // The approving endpoint first: a run that sets both wants its requests
        // answered, with the recorder behind it for whatever it does not take.
        let ack = std::env::var("WIE_LOCAL_NET_ACK").ok();
        if let Some(endpoint) = wie_backend::AckEndpoint::from_setting(ack.as_deref()) {
            endpoints.push(Box::new(endpoint));
        }

        let capture = std::env::var("WIE_LOCAL_NET_CAPTURE").ok();
        if let Some(endpoint) = wie_backend::CaptureEndpoint::from_setting(capture.as_deref()) {
            endpoints.push(Box::new(endpoint));
        }

        endpoints
    }

    fn network(&self) -> Option<&dyn Network> {
        Some(&self.network)
    }

    fn screen(&self) -> &dyn Screen {
        &self.screen
    }

    fn now(&self) -> Instant {
        Instant::from_epoch_millis(self.clock.now_ms())
    }

    fn database_repository(&self) -> &dyn DatabaseRepository {
        self.inner.database_repository()
    }

    fn filesystem(&self) -> &dyn Filesystem {
        self.inner.filesystem()
    }

    fn audio_sink(&self) -> Box<dyn AudioSink> {
        self.inner.audio_sink()
    }

    fn system_information(&self, key: &str) -> Option<String> {
        self.inner.system_information(key)
    }

    fn open_url(&self, url: &str) -> bool {
        self.inner.open_url(url)
    }

    fn write_stdout(&self, buf: &[u8]) {
        self.inner.write_stdout(buf)
    }

    fn write_stderr(&self, buf: &[u8]) {
        self.inner.write_stderr(buf)
    }

    fn exit(&self) {
        self.inner.exit()
    }

    fn vibrate(&self, duration_ms: u64, intensity: u8) {
        self.inner.vibrate(duration_ms, intensity)
    }

    fn set_backlight_mode(&self, mode: u8) {
        self.inner.set_backlight_mode(mode)
    }
}

/// Maps a `WIE_KEY` name to a key code, so a probe can target any key.
fn key_by_name(name: &str) -> Option<KeyCode> {
    Some(match name.to_ascii_uppercase().as_str() {
        "OK" | "FIRE" => KeyCode::OK,
        "UP" => KeyCode::UP,
        "DOWN" => KeyCode::DOWN,
        "LEFT" => KeyCode::LEFT,
        "RIGHT" => KeyCode::RIGHT,
        "LSK" | "LEFT_SOFT_KEY" => KeyCode::LEFT_SOFT_KEY,
        "RSK" | "RIGHT_SOFT_KEY" => KeyCode::RIGHT_SOFT_KEY,
        "CLEAR" => KeyCode::CLEAR,
        // The keys the handset printed above the pad. The Android frontend's
        // 저장 button is CALL, which is how several titles reach their save
        // screen, so a walk has to be able to press it.
        "CALL" | "SEND" => KeyCode::CALL,
        "HANGUP" | "END" => KeyCode::HANGUP,
        "NUM0" => KeyCode::NUM0,
        "NUM1" => KeyCode::NUM1,
        "NUM2" => KeyCode::NUM2,
        "NUM3" => KeyCode::NUM3,
        "NUM4" => KeyCode::NUM4,
        "NUM5" => KeyCode::NUM5,
        "NUM6" => KeyCode::NUM6,
        "NUM7" => KeyCode::NUM7,
        "NUM8" => KeyCode::NUM8,
        "NUM9" => KeyCode::NUM9,
        "HASH" | "POUND" => KeyCode::HASH,
        "STAR" => KeyCode::STAR,
        _ => return None,
    })
}

/// Parses `WIE_SCRIPT`: `tick:KEY` pairs separated by commas.
fn parse_script(script: Option<&str>) -> Vec<(u32, KeyCode)> {
    let Some(script) = script else {
        return Vec::new();
    };

    script
        .split(',')
        .filter_map(|step| {
            let (tick, key) = step.trim().split_once(':')?;

            Some((tick.trim().parse().ok()?, key_by_name(key.trim())?))
        })
        .collect()
}

fn write_ppm(path: &str, screen: &CaptureScreen) {
    let captured = screen.captured.lock().unwrap();
    if captured.last_pixels.is_empty() {
        return;
    }

    let mut ppm = format!("P6\n{} {}\n255\n", captured.width, captured.height).into_bytes();
    ppm.extend_from_slice(&captured.last_pixels);
    let _ = std::fs::write(path, ppm);
}

#[test]
fn ktf_archive_probe() {
    let Ok(path) = std::env::var("WIE_KTF_ARCHIVE") else {
        eprintln!("WIE_KTF_ARCHIVE unset; nothing to probe");
        return;
    };

    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .try_init();

    let ticks_limit: u32 = std::env::var("WIE_TICKS").ok().and_then(|x| x.parse().ok()).unwrap_or(20000);
    let archive = std::fs::read(&path).expect("archive");
    let files = extract_zip(&archive).expect("extract");
    // The panel the archive names for itself, which is what the Android
    // frontend gives it too. Without this a capture runs a 176x220 title on a
    // 240x320 screen and shows a layout no handset ever did.
    let native_size = KtfEmulator::screen_size(&archive);
    eprintln!(
        "[probe] {path}: {} entries, loadable={}, panel={native_size:?}",
        files.len(),
        KtfEmulator::loadable_archive(&files)
    );

    let mut script = parse_script(std::env::var("WIE_SCRIPT").ok().as_deref());
    if let Some(key) = std::env::var("WIE_KEY").ok().and_then(|name| key_by_name(&name))
        && let Some(tick) = std::env::var("WIE_PRESS_TICK").ok().and_then(|x| x.parse().ok())
    {
        script.push((tick, key));
    }
    script.sort_by_key(|(tick, _)| *tick);

    // One handset's storage, so a title that installs itself on its first run
    // finds what it wrote when it is started again.
    let state = TestPlatformState::default();
    let second = std::env::var("WIE_SCRIPT2").ok();

    if let Some(second) = second {
        eprintln!("[probe] first launch");
        run_once(&files, &state, &script, ticks_limit, None, None, native_size);

        let mut second_script = parse_script(Some(second.as_str()));
        second_script.sort_by_key(|(tick, _)| *tick);
        let second_ticks: u32 = std::env::var("WIE_TICKS2").ok().and_then(|x| x.parse().ok()).unwrap_or(ticks_limit);

        eprintln!("[probe] second launch");
        run_once(
            &files,
            &state,
            &second_script,
            second_ticks,
            std::env::var("WIE_SHOT_DIR").ok().as_deref(),
            std::env::var("WIE_SHOT").ok().as_deref(),
            native_size,
        );

        return;
    }

    run_once(
        &files,
        &state,
        &script,
        ticks_limit,
        std::env::var("WIE_SHOT_DIR").ok().as_deref(),
        std::env::var("WIE_SHOT").ok().as_deref(),
        native_size,
    );
}

/// Runs the archive once over `state`, which is the handset's storage and
/// outlives the launch.
fn run_once(
    files: &BTreeMap<String, Vec<u8>>,
    state: &TestPlatformState,
    script: &[(u32, KeyCode)],
    ticks_limit: u32,
    shot_dir: Option<&str>,
    shot: Option<&str>,
    native_size: Option<(u32, u32)>,
) {
    let exited = Arc::new(AtomicBool::new(false));
    let exited_clone = exited.clone();
    let screen = CaptureScreen {
        native_size,
        ..Default::default()
    };

    let tick_clock = Arc::new(ProbeClock::default());
    let platform = Box::new(CapturePlatform {
        inner: TestPlatform::with_state_and_event_handler(state.clone(), move |event| match event {
            TestPlatformEvent::Stdout(buf) => eprint!("[stdout] {}", String::from_utf8_lossy(&buf)),
            TestPlatformEvent::OpenUrl(url) => eprintln!("[open-url] {url}"),
            TestPlatformEvent::Exit => exited_clone.store(true, Ordering::SeqCst),
        }),
        screen: screen.clone(),
        clock: tick_clock.clone(),
        network: CaptureNetwork::default(),
    });

    let options = Options {
        enable_gdbserver: false,
        profile: None,
        annunciator: None,
    };

    let mut emulator = match KtfEmulator::from_archive(platform, files.clone(), options) {
        Ok(emulator) => emulator,
        Err(error) => {
            eprintln!("[probe] LOAD FAILED: {error:?}");
            return;
        }
    };

    // `request_redraw` only asks; the host is what paints. `wie_cli` turns the
    // request into a window redraw, so a probe that never feeds one back sees
    // a title paint nothing however well it runs.
    // The shipped Android frontend feeds a host paint only when the title asked
    // for one, where this probe feeds one every forty ticks whether or not it
    // did. That difference hides a whole class of fault - a repaint the runtime
    // drops is invisible here and permanent there - so it can be turned off.
    let redraw_on_request = std::env::var("WIE_REDRAW_ON_REQUEST").is_ok();

    let mut ticks = 0;
    let mut stopped = None;
    while ticks < ticks_limit && !exited.load(Ordering::SeqCst) {
        if redraw_on_request {
            if screen.requested.swap(false, Ordering::SeqCst) {
                emulator.handle_event(Event::Redraw);
            }
        } else if ticks % 40 == 0 {
            emulator.handle_event(Event::Redraw);
        }
        for (step, (tick, key)) in script.iter().enumerate() {
            if ticks == *tick {
                eprintln!("[probe] step {step}: pressing {key:?} at tick {ticks}");
                emulator.handle_event(Event::Keydown(*key));
            }
            if ticks == tick.saturating_add(20) {
                emulator.handle_event(Event::Keyup(*key));
            }
            // Far enough past the press that the screen it opened has settled.
            if let Some(dir) = shot_dir
                && ticks == tick.saturating_add(400)
            {
                write_ppm(&format!("{dir}/step_{step}.ppm"), &screen);
            }
        }

        if let Err(error) = emulator.tick() {
            stopped = Some(error);
            break;
        }
        tick_clock.advance();
        ticks += 1;
    }

    let captured = screen.captured.lock().unwrap();
    eprintln!(
        "[probe] ticks={ticks} guest_ms={} frames={} size={}x{} colors_in_last_frame={} exited={}",
        tick_clock.now_ms(),
        captured.frames,
        captured.width,
        captured.height,
        captured.last_colors,
        exited.load(Ordering::SeqCst),
    );
    match &stopped {
        Some(error) => eprintln!("[probe] STOPPED: {error:?}"),
        None => eprintln!("[probe] ran to the tick limit without stopping"),
    }

    if let Some(shot) = shot
        && !captured.last_pixels.is_empty()
    {
        let mut ppm = format!("P6\n{} {}\n255\n", captured.width, captured.height).into_bytes();
        ppm.extend_from_slice(&captured.last_pixels);
        std::fs::write(shot, ppm).expect("shot");
        eprintln!("[probe] wrote {shot}");
    }

    // Keep the probe from being mistaken for a passing assertion.
    let _ = Duration::from_secs(0);
}
