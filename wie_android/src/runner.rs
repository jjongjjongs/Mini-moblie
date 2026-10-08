use std::{
    collections::{BTreeMap, VecDeque},
    fmt::Write as _,
    path::PathBuf,
    sync::{Mutex, TryLockError},
    time::{Duration, Instant},
};

use wie_backend::{Emulator, Event, KeyCode, Options, Platform, drm_container, extract_zip};
use wie_brew::BrewEmulator;
use wie_j2me::J2MEEmulator;
use wie_ktf::KtfEmulator;
use wie_lgt::LgtEmulator;
use wie_skt::SktEmulator;

use crate::platform::{AndroidHandsetInformation, AndroidPlatform, Frame, Shared};

/// Feature phone LCD the WIPI/MIDP APIs are written against. Games query this
/// through `getDisplayInfo`, so it has to stay fixed rather than follow the
/// Android display; `GameView` letterboxes it to whatever the panel is.
pub const SCREEN_WIDTH: u32 = 240;
pub const SCREEN_HEIGHT: u32 = 320;

/// Key indexes as laid out by `MainActivity`'s keypad and D-pad.
fn key_code(index: i32) -> Option<KeyCode> {
    Some(match index {
        0 => KeyCode::UP,
        1 => KeyCode::DOWN,
        2 => KeyCode::LEFT,
        3 => KeyCode::RIGHT,
        4 => KeyCode::OK,
        5 => KeyCode::LEFT_SOFT_KEY,
        6 => KeyCode::RIGHT_SOFT_KEY,
        7 => KeyCode::CLEAR,
        8 => KeyCode::NUM0,
        9 => KeyCode::NUM1,
        10 => KeyCode::NUM2,
        11 => KeyCode::NUM3,
        12 => KeyCode::NUM4,
        13 => KeyCode::NUM5,
        14 => KeyCode::NUM6,
        15 => KeyCode::NUM7,
        16 => KeyCode::NUM8,
        17 => KeyCode::NUM9,
        18 => KeyCode::STAR,
        19 => KeyCode::HASH,
        20 => KeyCode::CALL,
        21 => KeyCode::HANGUP,
        _ => return None,
    })
}

/// Java has no name to give us; `nativeStart` only receives the archive bytes,
/// so the app id is derived from the content. It has to be stable across
/// launches or the game loses its save data, and distinct per game or two
/// games would share one.
fn content_id(data: &[u8]) -> String {
    format!("{:x}", md5::compute(data))
}

/// What to file a bare jar under.
///
/// A title packaged without a descriptor has no name from the outside, and a
/// hash of its bytes is a name nothing else knows: every table this runtime
/// keys by application id - the panel a title is given, the status strip, the
/// per-title billing answers, the directory its saves live in - misses it.
/// An LGT module names itself in its own header, so ask it first.
fn jar_app_id(jar: &[u8]) -> String {
    LgtEmulator::jar_app_id(jar).unwrap_or_else(|| content_id(jar))
}

/// The panel a bare J2ME MIDlet was drawn for, when it is not the default.
///
/// A handset archive names its own screen through its descriptor, which the
/// three archive emulators read; a plain jar carries no such thing, so a title
/// laid out for a smaller panel than today's 240x320 - most MIDP titles of its
/// years were - has to be recognised by content and given it here. Keyed by the
/// same id the jar is filed under, so it holds across launches.
fn j2me_panel(data: &[u8]) -> Option<(u32, u32)> {
    let files = extract_zip(data).ok()?;
    let jar = packaged_jar(&files).unwrap_or_else(|| data.to_vec());

    // Only titles that fall through to the J2ME emulator; a KTF/LGT/SKT jar
    // names its own panel the usual way.
    if KtfEmulator::loadable_jar(&jar) || LgtEmulator::loadable_jar(&jar) || SktEmulator::loadable_jar(&jar) {
        return None;
    }

    match jar_app_id(&jar).as_str() {
        // 호국전기이순신 - a 2004 LG MIDlet. Its screens are laid out for a
        // 176-tall-ish handset; 192 wide keeps the in-battle command menu, which
        // it draws flush to the left edge, from losing its first column.
        "c3fc1679fb1cf977c27e8c534a502b78" => Some((192, 220)),
        // 다운타운 열혈강호 (LGT). YulgangCanvas centres a 120x140 playfield with
        // `rx = (getWidth() - 120) / 2`, `ry = (getHeight() - 140) / 2`, yet only
        // ever `repaint(0, 0, 120, 143)`s the fixed top-left region. On the
        // default 240x320 panel it draws the playfield centred (rx 60, ry 90) but
        // flushes the top-left corner, so the picture lands in the wrong place. A
        // 120x143 panel puts rx at 0 and the flushed region over the whole screen,
        // which the host then scales up to fill the device.
        "de6dad5cb0aecc0679daf4d849538634" => Some((120, 143)),
        // 다운타운 나무 (ManaStone, MIDlet "Tree"), a sibling of 다운타운 열혈강호.
        // It composes into a 120x123 back buffer, then stacks a `bar` strip at
        // y123 and the ManaStone logo at y133 beneath it - the logo only where
        // `getHeight()` clears 133 - so its picture runs to y144. On the default
        // 240x320 panel that all lands in the top-left corner; a 120x144 panel
        // fills it, which the host then scales up to the device.
        "8cfb68600ac24fb8345162bc612e408b" => Some((120, 144)),
        // 다운타운 햄버거타이쿤 (mobileone, MIDlet "Burger"). It hardcodes no
        // resolution and lays itself out from getWidth/getHeight, so it fills any
        // panel - but its art and fonts are a fixed size, so the default 240x320
        // spaces everything out with the menu and the shop drawn small against a
        // lot of empty room. The 128x160 it was drawn for packs the picture back
        // in, which the host then scales up to the device.
        "f70b755c0455e1b42d363976472246c8" => Some((128, 160)),
        // 지혜의검 (LGT, MIDlet "Brain"). Its DESC.jad names the handset it was
        // drawn for - `MIDletX-LCD-Size:176,200` - and the canvas lays itself
        // out from getWidth/getHeight, so the default 240x320 panel puts every
        // screen in the wrong place. Give it the panel it asks for.
        "297b53452f8f2ee2a811b7961939b310" => Some((176, 200)),
        _ => None,
    }
}

/// The jar a download package carries, when the package is only a wrapper.
///
/// Some titles arrive as a zip holding one jar and its three icons rather than
/// as a handset archive: 액션퍼즐패밀리1 is `WEBSYNC1.jar` plus `big.png`,
/// `middle.png` and `small.png`, with no `app_info` between them. With no
/// descriptor no archive format claims it, and the jar branch is then handed
/// the wrapper instead of the jar - so `binary.mod`, one level down, is never
/// seen and a WIPI title is taken for a MIDlet.
///
/// `None` unless the archive holds exactly one jar, which keeps a jar that
/// happens to carry another jar as a resource on its own path.
fn packaged_jar(files: &BTreeMap<String, Vec<u8>>) -> Option<Vec<u8>> {
    let mut jars = files.iter().filter(|(name, _)| {
        name.rsplit('/')
            .next()
            .is_some_and(|name| name.len() > 4 && name[name.len() - 4..].eq_ignore_ascii_case(".jar"))
    });

    let (_, jar) = jars.next()?;
    if jars.next().is_some() {
        return None;
    }

    extract_zip(jar).ok().map(|_| jar.clone())
}

/// Whether `name` ends in `extension`, which starts with its dot, in any case.
fn has_extension(name: &str, extension: &str) -> bool {
    let name = name.rsplit('/').next().unwrap_or(name);

    name.len() > extension.len() && name[name.len() - extension.len()..].eq_ignore_ascii_case(extension)
}

/// Whether a zip handed to the J2ME loader could be a jar at all: one carries a
/// manifest or classes. Anything else has nothing for the loader to start.
fn could_be_jar(jar: &[u8]) -> bool {
    extract_zip(jar).is_ok_and(|files| {
        files
            .keys()
            .any(|name| name.eq_ignore_ascii_case("META-INF/MANIFEST.MF") || has_extension(name, ".class"))
    })
}

/// The DRM container a download is, if it is one.
///
/// The `.jar` a package carries is the title's own file, and under OMA DRM it
/// is a locked container rather than a zip - so it cannot go through
/// [`packaged_jar`], which only hands back a jar that opens as a zip. The one
/// named entry is read directly here, and the raw download too, so a container
/// handed over bare is caught as well.
fn drm_download(files: &BTreeMap<String, Vec<u8>>, data: &[u8]) -> Option<wie_backend::DrmContainer> {
    let mut jars = files.iter().filter(|(name, _)| {
        name.rsplit('/')
            .next()
            .is_some_and(|name| name.len() > 4 && name[name.len() - 4..].eq_ignore_ascii_case(".jar"))
    });

    let jar = match (jars.next(), jars.next()) {
        (Some((_, jar)), None) => jar.as_slice(),
        _ => data,
    };

    drm_container(jar)
}

struct Instance {
    emulator: Box<dyn Emulator + Send>,
    shared: Shared,
}

/// What every caller other than the emulator thread reads and writes.
///
/// [`RUNNER`] is held for the whole of a tick, and a tick is as long as the
/// emulated title makes it: a guest loop that never yields never ends, and
/// every caller waiting on that lock waits as long. That is how one hung title
/// used to take the whole app down - the UI thread blocked on the first key
/// press, and the log the hang was worth could not be written, because writing
/// it asked the same lock. So none of it goes through that lock any more. What
/// a key press, a status query or a stop needs is here instead, behind a lock
/// nothing holds for longer than a push or a pop.
struct Inbox {
    /// Key events waiting for the next tick to hand them to the emulator.
    input: VecDeque<Event>,
    running: bool,
    last_error: String,
    /// Whether the run that just ended was the title's own doing.
    ///
    /// A WIPI title ends itself: it calls `MC_knlExit` and the platform takes
    /// the screen back. Several here do it as part of working normally -
    /// 데몬헌터 builds its data on a first run, says "please start the program
    /// again", and exits when the player presses OK; the run after that goes on
    /// to the game. A fault that stops the emulator looks the same from the
    /// outside - the game is no longer running - and it is not the same thing:
    /// one is worth a saved log and a message, the other is worth neither.
    exited_by_title: bool,
    /// The running title's audio, so a stop can silence a title the emulator
    /// thread is still inside.
    shared: Option<Shared>,
    /// A stop asked for while the emulator thread held [`RUNNER`]; the next
    /// tick to start honours it.
    stop_requested: bool,
}

static INBOX: Mutex<Inbox> = Mutex::new(Inbox {
    input: VecDeque::new(),
    running: false,
    last_error: String::new(),
    exited_by_title: false,
    shared: None,
    stop_requested: false,
});

fn with_inbox<T>(f: impl FnOnce(&mut Inbox) -> T) -> T {
    let mut inbox = INBOX.lock().unwrap_or_else(|x| x.into_inner());

    f(&mut inbox)
}

/// Queues a key press or release for the next tick.
///
/// Called from the UI thread, and so never takes [`RUNNER`]: a touch has to be
/// answered whatever the emulator thread is doing.
pub fn key(index: i32, pressed: bool) {
    let Some(key_code) = key_code(index) else {
        tracing::warn!("Unknown key index {index}");
        return;
    };

    // Every press and release, at info, because a key nobody pressed is a
    // question no capture could answer otherwise: the floods a trace carries
    // fill a bounded window in a fraction of a second, and the moment a phantom
    // key fired has always already scrolled out of it by the time the log is
    // taken. At info it survives any filter, and it says which side to look at
    // - a press that is here came from the panel, and one that is not was
    // invented further in.
    tracing::info!("input: {key_code:?} {}", if pressed { "down" } else { "up" });

    with_inbox(|inbox| {
        if !inbox.running {
            return;
        }

        inbox
            .input
            .push_back(if pressed { Event::Keydown(key_code) } else { Event::Keyup(key_code) });
    });
}

/// Whether a game is loaded.
pub fn is_running() -> bool {
    with_inbox(|inbox| inbox.running)
}

/// The message that stopped the last run, or empty.
pub fn last_error() -> String {
    with_inbox(|inbox| inbox.last_error.clone())
}

/// Whether the run that just ended was the title ending itself.
pub fn exited_by_title() -> bool {
    with_inbox(|inbox| inbox.exited_by_title)
}

/// Ends the run, without waiting for a tick that may never end.
///
/// The run is reported stopped and silenced at once, so the player leaves the
/// game and the library is not played over. The emulator itself is torn down
/// here when the emulator thread is between ticks, and by the next tick to
/// start otherwise - which is never, for a title that has hung, so the memory
/// it holds is not given back until the process goes. That is the price of not
/// making the person wait on it, and it is the right way round: a hang they can
/// leave and report is worth more than one that holds the app until Android
/// kills it.
pub fn request_stop() {
    let shared = with_inbox(|inbox| {
        inbox.input.clear();
        inbox.running = false;
        // The player asked for this one, so there is nothing for the caller to
        // tell them about it.
        inbox.exited_by_title = false;
        inbox.stop_requested = true;

        inbox.shared.take()
    });

    // Otherwise whatever the sequence was holding goes on sounding after the
    // game it belongs to has gone.
    if let Some(shared) = shared {
        shared.mixer().silence();
    }

    match RUNNER.try_lock() {
        Ok(mut runner) => runner.stop(),
        Err(TryLockError::Poisoned(error)) => error.into_inner().stop(),
        // Mid-tick. The tick that is running holds it, and the next one to
        // start will find `stop_requested` and do this itself.
        Err(TryLockError::WouldBlock) => tracing::info!("stop asked for during a tick; the emulator is torn down when it returns"),
    }
}

/// Where a second of the host loop went.
///
/// The loop is the emulator's whole share of the phone: it runs a tick, drains
/// what the tick produced, and waits. A capture shows what the title did inside
/// a tick and nothing at all about the rest, so a frame that takes 68ms when the
/// title asked for 15 could be the guest computing, this loop waiting, or the
/// drain between the two, and the log could not tell them apart.
///
/// This says which, in three parts that add up to the window: `run` is time
/// inside `tick`, `drain` is what the loop did with what the tick produced -
/// the audio commands, the frame, the polls beside them, all of it across JNI -
/// and `sleep` is the wait it chose afterwards. `idle` says how many of the
/// ticks stopped for want of anything to run rather than for want of budget,
/// and `worst gap` is the longest single spell outside, which is what a late
/// frame is made of.
#[derive(Default)]
struct LoopMeter {
    window_began: Option<Instant>,
    /// When the last tick returned, so the next one can say how long the loop
    /// spent away from the emulator.
    left_at: Option<Instant>,
    /// When the loop last asked how long it could sleep, which is the last thing
    /// it does before sleeping. What comes before it is the drain - the audio,
    /// the frame, the polls - and what comes after is the wait itself.
    armed_at: Option<Instant>,
    insns_at_window: u64,
    svcs_at_window: u64,
    fallbacks_at_window: u64,
    ticks: u32,
    inside: Duration,
    /// Draining what the tick produced: the audio commands, the frame, the
    /// handful of polls beside them. All of it crosses JNI once per step.
    drained: Duration,
    /// Waiting, having nothing to do.
    slept: Duration,
    /// The longest single spell outside, which is what a late frame is made of.
    outside_worst: Duration,
    /// Ticks that stopped because the emulator had nothing left to run, against
    /// ones that used their whole budget. A loop that is mostly idle is waiting
    /// on something; one that is mostly out of budget is short of CPU.
    idle: u32,
    frames: u32,
}

impl LoopMeter {
    /// Called as a tick begins; answers when it began.
    fn enter(&mut self) -> Instant {
        let now = Instant::now();

        if let Some(left) = self.left_at {
            let away = now.duration_since(left);
            self.outside_worst = self.outside_worst.max(away);

            // A tick that used its whole budget never asked how long it could
            // sleep and never slept, so all of its time away was the drain.
            match self.armed_at {
                Some(armed) => {
                    self.drained += armed.saturating_duration_since(left);
                    self.slept += now.saturating_duration_since(armed);
                }
                None => self.drained += away,
            }
        }

        self.window_began.get_or_insert(now);

        now
    }

    /// Called as a tick returns, and reports once a second has gone by.
    fn leave(&mut self, began: Instant, stopped_idle: bool) {
        let now = Instant::now();

        self.ticks += 1;
        self.inside += now.duration_since(began);
        self.idle += u32::from(stopped_idle);
        self.left_at = Some(now);
        self.armed_at = None;

        let Some(window_began) = self.window_began else { return };
        let window = now.duration_since(window_began);
        if window < Duration::from_secs(1) {
            return;
        }

        use std::sync::atomic::Ordering;

        let insns = wie_core_arm::EXECUTED_INSTRUCTIONS.load(Ordering::Relaxed);
        let svcs = wie_core_arm::SVC_COUNT.load(Ordering::Relaxed);
        let fallbacks = wie_core_arm::JIT_FALLBACKS.load(Ordering::Relaxed);

        let ran = insns.saturating_sub(self.insns_at_window);
        let mips = ran as f64 / window.as_secs_f64() / 1.0e6;

        // Against the instructions, this says whether the time goes into
        // running guest code or into the round trip out of it. See `SVC_COUNT`.
        let svc_rate = svcs.saturating_sub(self.svcs_at_window) as f64 / window.as_secs_f64();

        tracing::info!(
            "[loop] {} ticks in {:.2}s: run={}ms drain={}ms sleep={}ms (worst gap {}ms) idle={}/{} frames={} \
             {:.1} MIPS on {} ({:.0} svc/s, {} fallbacks)",
            self.ticks,
            window.as_secs_f64(),
            self.inside.as_millis(),
            self.drained.as_millis(),
            self.slept.as_millis(),
            self.outside_worst.as_millis(),
            self.idle,
            self.ticks,
            self.frames,
            mips,
            wie_core_arm::engine_name(),
            svc_rate,
            fallbacks.saturating_sub(self.fallbacks_at_window),
        );

        *self = Self {
            window_began: Some(now),
            left_at: self.left_at,
            insns_at_window: insns,
            svcs_at_window: svcs,
            fallbacks_at_window: fallbacks,
            ..Default::default()
        };
    }
}

pub struct Runner {
    instance: Option<Instance>,
    meter: LoopMeter,
}

static RUNNER: Mutex<Runner> = Mutex::new(Runner {
    instance: None,
    meter: LoopMeter {
        window_began: None,
        left_at: None,
        armed_at: None,
        insns_at_window: 0,
        svcs_at_window: 0,
        fallbacks_at_window: 0,
        ticks: 0,
        inside: Duration::ZERO,
        drained: Duration::ZERO,
        slept: Duration::ZERO,
        outside_worst: Duration::ZERO,
        idle: 0,
        frames: 0,
    },
});

pub fn with_runner<T>(f: impl FnOnce(&mut Runner) -> T) -> T {
    let mut runner = RUNNER.lock().unwrap_or_else(|x| x.into_inner());

    f(&mut runner)
}

impl Runner {
    /// Loads `data` and spawns the emulator. Returns the error message to show
    /// in the player, or an empty string on success.
    pub fn start(&mut self, data: Vec<u8>, runtime_dir: PathBuf, handset_information: AndroidHandsetInformation) -> String {
        self.stop();

        let shared = Shared::default();

        // A title sizes its own drawing from what the screen reports, so a panel
        // it was not written for is one it lays out wrongly - and the screen has
        // to exist before there is an emulator to ask about it. Give the archive
        // the chance to name its own panel first; almost none do, and those fall
        // back to the default.
        let (width, height) = title_panel(&data);
        // Always on: which panel a title is given decides how its fixed-layout
        // screens land, so a capture that does not say the size cannot answer
        // why a title drew where it did.
        tracing::info!("panel: {width}x{height}");

        let platform = Box::new(AndroidPlatform::new(runtime_dir, width, height, shared.clone(), handset_information));

        let options = Options {
            enable_gdbserver: false,
            profile: None,
            annunciator: None,
        };

        match build_emulator(platform, &data, options) {
            Ok(emulator) => {
                with_inbox(|inbox| {
                    inbox.input.clear();
                    inbox.running = true;
                    inbox.last_error.clear();
                    inbox.exited_by_title = false;
                    inbox.shared = Some(shared.clone());
                    inbox.stop_requested = false;
                });

                self.instance = Some(Instance { emulator, shared });
                self.meter = LoopMeter::default();

                String::new()
            }
            Err(error) => {
                with_inbox(|inbox| {
                    inbox.input.clear();
                    inbox.running = false;
                    inbox.last_error = error.clone();
                    inbox.exited_by_title = false;
                    inbox.shared = None;
                    inbox.stop_requested = false;
                });

                error
            }
        }
    }

    pub fn stop(&mut self) {
        // Otherwise whatever the sequence was holding goes on sounding after
        // the game it belongs to has gone.
        if let Some(instance) = self.instance.as_ref() {
            instance.shared.mixer().silence();
        }

        with_inbox(|inbox| {
            inbox.input.clear();
            inbox.running = false;
            inbox.exited_by_title = false;
            inbox.shared = None;
            inbox.stop_requested = false;
        });

        self.instance = None;
    }

    /// Runs the emulator for up to `budget`. Returns a status line for the
    /// player, empty while everything is fine.
    ///
    /// Wraps the run so [`LoopMeter`] can say how the second was split between
    /// the emulator and the loop around it.
    pub fn tick(&mut self, budget: Duration) -> String {
        let began = self.meter.enter();
        let mut stopped_idle = false;
        let status = self.run_tick(budget, &mut stopped_idle);
        self.meter.leave(began, stopped_idle);

        status
    }

    fn run_tick(&mut self, budget: Duration, stopped_idle: &mut bool) -> String {
        // A stop the UI asked for while the previous tick was running. It is
        // answered before anything else, so the tick after a stop never runs
        // the title a person has already left.
        let (stop_requested, input) = with_inbox(|inbox| (inbox.stop_requested, inbox.input.drain(..).collect::<Vec<_>>()));
        if stop_requested {
            self.stop();
            return String::new();
        }

        let Some(instance) = self.instance.as_mut() else {
            return String::new();
        };

        for event in input {
            instance.emulator.handle_event(event);
        }

        if instance.shared.take_redraw_request() {
            instance.emulator.handle_event(Event::Redraw);
        }

        // The synthesiser has to keep producing between the bursts a sequence
        // arrives in, so it is pumped once a tick rather than from the sink.
        instance.shared.render_synth();

        let deadline = Instant::now() + budget;
        loop {
            if let Err(error) = instance.emulator.tick() {
                let message = error.to_string();
                tracing::error!("Emulator stopped: {message}");

                with_inbox(|inbox| {
                    inbox.input.clear();
                    inbox.running = false;
                    inbox.last_error = message.clone();
                    inbox.exited_by_title = false;
                    inbox.shared = None;
                });

                instance.shared.mixer().silence();
                self.instance = None;

                return message;
            }

            if instance.shared.has_exited() {
                tracing::info!("Application exited");

                with_inbox(|inbox| {
                    inbox.input.clear();
                    inbox.running = false;
                    inbox.last_error.clear();
                    inbox.exited_by_title = true;
                    inbox.shared = None;
                });

                // As on every other path out: a sequence the title left playing
                // would otherwise go on sounding over the library.
                instance.shared.mixer().silence();
                self.instance = None;

                return String::new();
            }

            // Nothing runnable until a timer fires: stop rather than busy-wait
            // the rest of the budget. The host's inter-tick delay then acts as a
            // real sleep. A CPU-bound title never reports idle, so it keeps
            // running to the full budget — which is what lifts its duty cycle
            // once the host delay is short.
            if instance.emulator.is_idle() {
                *stopped_idle = true;

                return String::new();
            }

            if Instant::now() >= deadline {
                return String::new();
            }
        }
    }

    /// How long the host loop may sleep before the title has work again, in
    /// milliseconds, or `None` to keep to its own interval. See
    /// [`Emulator::sleep_hint`](wie_backend::Emulator::sleep_hint).
    pub fn sleep_hint(&mut self) -> Option<u64> {
        // The last thing the loop asks before it waits, so this is where the
        // drain ends and the wait begins. See [`LoopMeter`].
        self.meter.armed_at = Some(Instant::now());

        // The title's timers are on the game clock; the loop sleeps on the
        // wall clock, which at 2x reaches them in half the time.
        self.instance.as_ref()?.emulator.sleep_hint().map(crate::speed::real_ms)
    }

    pub fn take_frame(&mut self) -> Option<Frame> {
        let frame = self.instance.as_ref()?.shared.take_frame();
        self.meter.frames += u32::from(frame.is_some());

        frame
    }

    pub fn take_audio(&mut self) -> Option<Vec<u8>> {
        self.instance.as_ref()?.shared.take_audio()
    }

    pub fn take_backlight_mode(&mut self) -> u8 {
        self.instance.as_ref().map(|instance| instance.shared.take_backlight_mode()).unwrap_or(0)
    }

    pub fn take_phone_call(&mut self) -> Option<String> {
        self.instance.as_ref()?.shared.take_phone_call()
    }

    pub fn take_browser_url(&mut self) -> Option<String> {
        self.instance.as_ref()?.shared.take_browser_url()
    }
}

/// The LGT firmware BIOS, bundled into the app so an LGT title can run real
/// firmware code. Injected as a virtual file under the reference's own filename,
/// so `wie_lgt`'s `try_load_bios` finds it - the game archive is never touched.
/// It is proprietary and lives only in this private repository.
const FIRMWARE_BIOS: &[u8] = include_bytes!("../firmware/libarm32_lgt_system.so");
const FIRMWARE_BIOS_NAME: &str = "libarm32_lgt_system.so";

/// The panel `data` is drawn for: the one its archive or jar names, or the
/// default 240x320.
pub fn title_panel(data: &[u8]) -> (u32, u32) {
    LgtEmulator::screen_size(data)
        .or_else(|| BrewEmulator::screen_size(data))
        .or_else(|| SktEmulator::screen_size(data))
        .or_else(|| KtfEmulator::screen_size(data))
        .or_else(|| j2me_panel(data))
        .unwrap_or((SCREEN_WIDTH, SCREEN_HEIGHT))
}

/// Loads `data` - a handset archive, a package around one jar, or a bare jar -
/// onto `platform`. Returns the message to show when it cannot be run.
///
/// Shared with the desktop build, so a title loads there exactly as it does on
/// a handset.
pub fn build_emulator(platform: Box<dyn Platform>, data: &[u8], options: Options) -> Result<Box<dyn Emulator + Send>, String> {
    let mut files = extract_zip(data).map_err(|x| format!("압축을 열 수 없습니다: {x}"))?;

    // The handset's own bitmap faces live in the bundled firmware, and drawing
    // text with them needs nothing else from it - so they are installed here,
    // for whichever platform the archive turns out to be. Only LGT loads the
    // image as firmware, which is why only LGT used to have the faces; every
    // KTF title was drawing its text with an anti-aliased outline instead,
    // which is what made 괴도키리's Korean text look soft against its own
    // pixel art. See `wie_wipi_c::api::graphics::install_bios_font`.
    if wie_wipi_c::api::graphics::install_bios_font(FIRMWARE_BIOS) {
        tracing::info!("handset bitmap faces installed; text is drawn from the handset's own glyphs");
    } else {
        tracing::warn!("no bitmap face in the bundled firmware; text stays on the outline font");
    }

    // A download delivered under OMA DRM is a locked container, not the title:
    // the title's bytes are inside it and the key is in a rights object issued
    // to the handset that bought it, not in the file. No loader here can open
    // one, so say so plainly - naming the AID a non-DRM edition of the same
    // title files under, which the container's own content id carries - rather
    // than letting a carrier loader report it as a broken archive.
    if let Some(drm) = drm_download(&files, data) {
        let mut message = format!("이 파일은 DRM 보호 배포본({})이라 실행할 수 없습니다.", drm.format);
        match drm.edition_aid() {
            Some(aid) => {
                let _ = write!(message, " 같은 게임의 DRM이 없는 일반 배포본(AID {aid})을 받아 주세요.");
            }
            None => message.push_str(" 같은 게임의 DRM이 없는 일반 배포본을 받아 주세요."),
        }

        return Err(message);
    }

    // A BREW application - KTF's earlier platform - is a module and its
    // information file, with no descriptor, so it is told apart from the WIPI
    // archives by that pair before they are tried.
    if BrewEmulator::loadable_archive(&files) {
        return BrewEmulator::from_archive(platform, files)
            .map(|x| Box::new(x) as Box<dyn Emulator + Send>)
            .map_err(|x| format!("BREW 애플리케이션을 실행할 수 없습니다: {x}"));
    }

    // Handset archives are detected by their descriptor. A jar carries no
    // descriptor, so it is only considered once all three archive formats have
    // been ruled out - an apk or jar is itself a zip and would otherwise be
    // mistaken for one.
    if KtfEmulator::loadable_archive(&files) {
        return KtfEmulator::from_archive(platform, files, options)
            .map(|x| Box::new(x) as Box<dyn Emulator + Send>)
            .map_err(|x| format!("KTF 아카이브를 실행할 수 없습니다: {x}"));
    }
    if LgtEmulator::loadable_archive(&files) {
        // Ride the firmware in as a virtual file so the emulator's filesystem
        // exposes it to try_load_bios.
        files.insert(FIRMWARE_BIOS_NAME.to_owned(), FIRMWARE_BIOS.to_vec());
        return LgtEmulator::from_archive(platform, files, options)
            .map(|x| Box::new(x) as Box<dyn Emulator + Send>)
            .map_err(|x| format!("LGT 아카이브를 실행할 수 없습니다: {x}"));
    }
    if SktEmulator::loadable_archive(&files) {
        return SktEmulator::from_archive(platform, files)
            .map(|x| Box::new(x) as Box<dyn Emulator + Send>)
            .map_err(|x| format!("SKT 아카이브를 실행할 수 없습니다: {x}"));
    }

    // A package that is only a wrapper around one jar is opened to the jar,
    // so the formats below read the entries the title actually ships.
    let jar = packaged_jar(&files).unwrap_or_else(|| data.to_vec());
    let id = jar_app_id(&jar);
    let jar_filename = format!("{id}.jar");

    // At info, because which name a title is filed under decides which of this
    // runtime's per-title tables reach it, and a capture that does not say the
    // name cannot answer why none of them did.
    tracing::info!("bare jar filed as {id}");

    if KtfEmulator::loadable_jar(&jar) {
        KtfEmulator::from_jar(platform, &jar_filename, jar, &id, &id, None, options)
            .map(|x| Box::new(x) as Box<dyn Emulator + Send>)
            .map_err(|x| format!("KTF jar를 실행할 수 없습니다: {x}"))
    } else if LgtEmulator::loadable_jar(&jar) {
        LgtEmulator::from_jar(platform, &jar_filename, jar, &id, &id, None, options)
            .map(|x| Box::new(x) as Box<dyn Emulator + Send>)
            .map_err(|x| format!("LGT jar를 실행할 수 없습니다: {x}"))
    } else if SktEmulator::loadable_jar(&jar) {
        SktEmulator::from_jar(platform, &jar_filename, jar, &id, None)
            .map(|x| Box::new(x) as Box<dyn Emulator + Send>)
            .map_err(|x| format!("SKT jar를 실행할 수 없습니다: {x}"))
    } else if !could_be_jar(&jar) {
        Err("지원하지 않는 형식입니다: WIPI 아카이브도 jar도 아닙니다.".to_owned())
    } else {
        J2MEEmulator::from_jar(platform, &jar_filename, jar)
            .map(|x| Box::new(x) as Box<dyn Emulator + Send>)
            .map_err(|x| format!("지원하지 않는 형식입니다: {x}"))
    }
}

/// The two names a title's stored data sits under.
///
/// Record stores are keyed by the product id and the writable filesystem by the
/// application id, and an archive's descriptor gives different values for the
/// two - Legend of Master saves under `PD127080` but writes files under
/// `0002A4B1`. Both are needed to collect everything a title has kept.
pub struct SaveIds {
    pub records: String,
    pub files: String,
}

/// Where `data`'s saves would be, without running it.
///
/// This has to agree with what the emulators pass to `System::new`, or an
/// export would quietly come back empty.
pub fn save_ids(data: &[u8]) -> Option<SaveIds> {
    let files = extract_zip(data).ok()?;

    // The descriptor names both ids for the two archive formats that have one.
    for descriptor in ["app_info", "__adf__"] {
        let Some(contents) = files.get(descriptor) else {
            continue;
        };

        let mut product = String::new();
        let mut application = String::new();
        for line in contents.split(|x| *x == b'\n') {
            let line = String::from_utf8_lossy(line);
            let line = line.trim();

            if let Some(value) = line.strip_prefix("PID:") {
                product = value.trim().to_owned();
            }
            if let Some(value) = line.strip_prefix("AID:") {
                application = value.trim().to_owned();
            }
        }

        if !product.is_empty() || !application.is_empty() {
            return Some(SaveIds {
                records: product.clone(),
                files: if application.is_empty() { product } else { application },
            });
        }
    }

    // A BREW application keeps its writes under its ClassID.
    if let Some(id) = BrewEmulator::save_id(&files) {
        return Some(SaveIds {
            records: id.clone(),
            files: id,
        });
    }

    // An SKT archive names itself in its descriptor, or failing that in the
    // descriptor's own filename, and uses the one name for both.
    if let Some((name, contents)) = files.iter().find(|(name, _)| name.ends_with(".msd")) {
        let declared = contents
            .split(|x| *x == b'\n')
            .map(|line| String::from_utf8_lossy(line).trim().to_owned())
            .find_map(|line| line.strip_prefix("DD-ProgName:").map(|x| x.trim().to_owned()));

        let id = declared.unwrap_or_else(|| name.split('.').next().unwrap_or(name).to_owned());
        if !id.is_empty() {
            return Some(SaveIds {
                records: id.clone(),
                files: id,
            });
        }
    }

    // Anything else runs as a bare jar. Its module names it when it carries
    // one; otherwise the only id it has is derived from its contents.
    //
    // This reads the packaged jar rather than the wrapper around it, because
    // that is what `start` runs and what the emulator is therefore given - a
    // wrapper hashed whole answered a name no save was ever written under.
    let id = jar_app_id(&packaged_jar(&files).unwrap_or_else(|| data.to_vec()));

    Some(SaveIds {
        records: id.clone(),
        files: id,
    })
}

/// The carrier a title runs under, for the library's badge and filter:
/// `"KTF"`, `"LGT"`, `"SKT"`, or `""` when nothing claims it (a plain J2ME jar
/// or an archive no format recognises), which the library shows as 기타.
///
/// This mirrors the order [`build_emulator`] picks an emulator in, so the badge
/// names the emulator the title would actually run under.
pub fn carrier(data: &[u8]) -> &'static str {
    let Ok(files) = extract_zip(data) else {
        return "";
    };

    // A DRM-locked download is marked as such in the library, not filed under
    // the carrier its outer archive looks like, so the badge says why it will
    // not run before the player opens it.
    if drm_download(&files, data).is_some() {
        return "DRM";
    }

    if KtfEmulator::loadable_archive(&files) || BrewEmulator::loadable_archive(&files) {
        return "KTF";
    }
    if LgtEmulator::loadable_archive(&files) {
        return "LGT";
    }
    if SktEmulator::loadable_archive(&files) {
        return "SKT";
    }

    let jar = packaged_jar(&files).unwrap_or_else(|| data.to_vec());
    if KtfEmulator::loadable_jar(&jar) {
        "KTF"
    } else if LgtEmulator::loadable_jar(&jar) {
        "LGT"
    } else if SktEmulator::loadable_jar(&jar) {
        "SKT"
    } else {
        ""
    }
}

/// Describes an archive without running it, for `nativeInspect`. Only used for
/// diagnostics, so every failure is reported as text rather than an error.
pub fn inspect(data: &[u8]) -> String {
    let mut report = String::new();

    let _ = writeln!(report, "size: {} bytes", data.len());
    let _ = writeln!(report, "id: {}", content_id(data));

    let files = match extract_zip(data) {
        Ok(files) => files,
        Err(error) => {
            let _ = writeln!(report, "not a zip: {error}");
            return report;
        }
    };

    // A download under OMA DRM is a locked container, not the title. Say so
    // before reading on, since the carrier guesses below would read its outer
    // zip as an ordinary archive and name a format it cannot run.
    if let Some(drm) = drm_download(&files, data) {
        let _ = writeln!(report, "format: DRM ({})", drm.format);
        if let Some(id) = &drm.content_id {
            let _ = writeln!(report, "content id: {id}");
        }
        if let Some(aid) = drm.edition_aid() {
            let _ = writeln!(report, "non-DRM edition AID: {aid}");
        }
        return report;
    }

    let jar = packaged_jar(&files).unwrap_or_else(|| data.to_vec());
    let format = if BrewEmulator::loadable_archive(&files) {
        "BREW application"
    } else if KtfEmulator::loadable_archive(&files) {
        "KTF archive"
    } else if LgtEmulator::loadable_archive(&files) {
        "LGT archive"
    } else if SktEmulator::loadable_archive(&files) {
        "SKT archive"
    } else if KtfEmulator::loadable_jar(&jar) {
        "KTF jar"
    } else if LgtEmulator::loadable_jar(&jar) {
        "LGT jar"
    } else if SktEmulator::loadable_jar(&jar) {
        "SKT jar"
    } else if could_be_jar(&jar) {
        "J2ME jar (assumed)"
    } else {
        "unknown (not a jar)"
    };
    let _ = writeln!(report, "format: {format}");

    // The descriptor is the only place the app id, product id and main class
    // come from, and a malformed one is the usual reason a zip will not run.
    for descriptor in ["app_info", "__adf__"] {
        let Some(contents) = files.get(descriptor) else {
            continue;
        };

        let _ = writeln!(report, "--- {descriptor} ---");
        for line in contents.split(|x| *x == b'\n') {
            let line = String::from_utf8_lossy(line);
            let line = line.trim();
            if line.starts_with("AID:") || line.starts_with("PID:") || line.starts_with("MClass:") || line.starts_with("Ver:") {
                let _ = writeln!(report, "{line}");
            }
        }
    }

    let _ = writeln!(report, "--- entries ({}) ---", files.len());
    for name in files.keys().take(32) {
        let _ = writeln!(report, "{name}");
    }

    report
}

#[cfg(test)]
mod tests {
    use wie_backend::KeyCode;

    use super::{content_id, could_be_jar, extract_zip, inspect, key_code, packaged_jar, save_ids};

    /// A stored zip of the given entries, which is all these tests need.
    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write as _;

        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, body) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(body).unwrap();
        }

        writer.finish().unwrap().into_inner()
    }

    /// 액션퍼즐패밀리1's shape: one jar and its icons, and no descriptor at all.
    fn packaged_wipi_title() -> (Vec<u8>, Vec<u8>) {
        let jar = zip_of(&[
            ("binary.mod", b"\x00\x00\xa0\xe1"),
            ("META-INF/MANIFEST.MF", b"MIDlet-1: Midlet,/sicon.png,Midlet\n"),
        ]);
        let package = zip_of(&[
            ("WEBSYNC1.jar", &jar),
            ("big.png", b"big"),
            ("middle.png", b"mid"),
            ("small.png", b"small"),
        ]);

        (package, jar)
    }

    #[test]
    fn a_package_holding_one_jar_is_opened_to_it() {
        let (package, jar) = packaged_wipi_title();

        assert_eq!(packaged_jar(&extract_zip(&package).unwrap()).as_deref(), Some(jar.as_slice()));
    }

    #[test]
    fn a_wipi_title_packaged_as_a_jar_is_not_taken_for_a_midlet() {
        let (package, _) = packaged_wipi_title();
        let report = inspect(&package);

        // Its manifest names a MIDlet and it ships no classes at all, so read as
        // one it has nothing to load. The binary.mod a level down is what says
        // otherwise.
        assert!(report.contains("format: LGT jar"), "{report}");
    }

    /// 액션퍼즐패밀리1 itself, as it was handed over: `WEBSYNC1.jar` and three
    /// icons, a MIDlet manifest inside the jar and `binary.mod` beside it.
    #[test]
    fn the_real_package_reads_as_a_wipi_title() {
        let report = inspect(include_bytes!("../../test_data/action_puzzle_family_1.zip"));

        assert!(report.contains("format: LGT jar"), "{report}");
    }

    #[test]
    fn a_jar_carrying_another_jar_is_left_alone() {
        // Only a package of exactly one jar is opened; a title shipping a jar as
        // a resource keeps its own identity.
        let package = zip_of(&[("a.jar", b"not a zip"), ("b.jar", b"nor this")]);

        assert_eq!(packaged_jar(&extract_zip(&package).unwrap()), None);
    }

    #[test]
    fn key_indexes_match_the_java_keypad() {
        assert!(matches!(key_code(0), Some(KeyCode::UP)));
        assert!(matches!(key_code(4), Some(KeyCode::OK)));
        assert!(matches!(key_code(8), Some(KeyCode::NUM0)));
        assert!(matches!(key_code(17), Some(KeyCode::NUM9)));
        assert!(matches!(key_code(19), Some(KeyCode::HASH)));
        // The keypad's call key, which a handset's games treat as save.
        assert!(matches!(key_code(20), Some(KeyCode::CALL)));
        assert!(key_code(22).is_none());
        assert!(key_code(-1).is_none());
    }

    #[test]
    fn content_id_is_stable_and_distinct() {
        assert_eq!(content_id(b"abc"), content_id(b"abc"));
        assert_ne!(content_id(b"abc"), content_id(b"abd"));
    }

    #[test]
    fn inspect_reports_lgt_archive() {
        let report = inspect(include_bytes!("../../test_data/helloworld_lgt.zip"));

        assert!(report.contains("format: LGT archive"), "{report}");
        assert!(report.contains("--- app_info ---"), "{report}");
    }

    #[test]
    fn a_brew_application_is_named_as_one() {
        let package = zip_of(&[
            ("kashan/kashan.mod", b"module"),
            ("kashan/18933.mif", b"info"),
            ("kashan/kashan.sig", b"sig"),
        ]);

        let report = inspect(&package);
        assert!(report.contains("format: BREW application"), "{report}");
    }

    #[test]
    fn a_zip_without_manifest_or_classes_is_not_a_jar() {
        assert!(!could_be_jar(&zip_of(&[("kgraphic.ils", b"data")])));
        assert!(could_be_jar(&zip_of(&[
            ("META-INF/MANIFEST.MF", b"Manifest-Version: 1.0"),
            ("res/0.png", b"png")
        ])));
        assert!(could_be_jar(&zip_of(&[("Main.class", b"\xca\xfe")])));
    }

    #[test]
    fn inspect_reports_non_zip() {
        let report = inspect(b"not a zip at all");

        assert!(report.contains("not a zip"), "{report}");
    }

    /// A download whose jar is an OMA DRM container: a package of exactly one
    /// jar and its icons, the jar an `odcf` container with a WIPI content id.
    fn drm_package() -> Vec<u8> {
        let mut dcf = b"odcf\x00\x02\x00\x00\x00\x00\x00\x01odrmodheohdr".to_vec();
        dcf.extend_from_slice(b"\x01\x02\x00\x00\x00\x00\x00\x00\x00\x00\x87\xda");
        dcf.extend_from_slice(b"\x00\x19\x00\x01\x00\x0c");
        dcf.extend_from_slice(b"00WIPI000000000001036D08\x00\x00ContentURL:\x00odda....");

        zip_of(&[
            ("010261FB.jar", &dcf),
            ("big.icon", b"big"),
            ("middle.icon", b"mid"),
            ("small.icon", b"small"),
        ])
    }

    /// A DRM download reads as DRM, names its content id and the AID the
    /// non-DRM edition files under, and nothing is read on as if it could run.
    #[test]
    fn inspect_reports_a_drm_container() {
        let report = inspect(&drm_package());

        assert!(report.contains("format: DRM (OMA DRM (DCF))"), "{report}");
        assert!(report.contains("content id: 00WIPI000000000001036D08"), "{report}");
        assert!(report.contains("non-DRM edition AID: 01036D08"), "{report}");
        assert!(!report.contains("format: KTF"), "{report}");
    }

    /// A DRM download is badged DRM in the library rather than filed under the
    /// carrier its outer zip resembles.
    #[test]
    fn a_drm_download_is_badged_drm() {
        assert_eq!(super::carrier(&drm_package()), "DRM");
    }

    /// The two ids differ, and reading only one of them would miss half of
    /// what a title has kept.
    #[test]
    fn save_ids_come_from_the_descriptor() {
        for archive in [
            include_bytes!("../../test_data/helloworld_lgt.zip").as_slice(),
            include_bytes!("../../test_data/helloworld_ktf.zip").as_slice(),
        ] {
            let ids = save_ids(archive).expect("this archive has a descriptor");

            assert!(!ids.records.is_empty(), "no product id");
            assert!(!ids.files.is_empty(), "no application id");
            assert!(
                inspect(archive).contains(&format!("PID:{}", ids.records)),
                "the product id is not the descriptor's"
            );
        }
    }

    /// A zip with no descriptor runs as a bare jar, whose id is its content
    /// hash - the same one `build_emulator` would have used.
    #[test]
    fn save_ids_fall_back_to_the_content_id() {
        // A zip that holds nothing a loader recognises.
        let bare = include_bytes!("../../test_data/helloworld_lgt.zip");
        let jar = extract_zip(bare)
            .expect("the archive opens")
            .remove("00000000.jar")
            .expect("it holds a jar");

        let ids = save_ids(&jar).expect("a jar still has an id");

        assert_eq!(ids.records, content_id(&jar));
        assert_eq!(ids.files, ids.records);
    }

    /// A file that cannot be opened has nowhere to have saved to either, and
    /// saying so is what lets the export report it rather than write an empty
    /// zip.
    #[test]
    fn save_ids_reject_what_cannot_be_loaded() {
        assert!(save_ids(b"not a zip at all").is_none());
    }
}
