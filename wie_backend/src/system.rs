mod audio;
mod event_queue;
mod file_system;
mod input_method;

use alloc::{borrow::ToOwned, boxed::Box, string::String, sync::Arc};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use spin::{RwLock, RwLockWriteGuard};

use wie_util::Result;

use crate::{
    AsyncCallable,
    executor::Executor,
    local_network::LocalNetwork,
    platform::Platform,
    task::{SleepFuture, YieldFuture},
    task_runner::TaskRunner,
};

use self::{audio::Audio, event_queue::EventQueue, input_method::InputMethod};

pub use self::{
    event_queue::{Event, KeyCode},
    file_system::FilesystemOverlay,
    input_method::InputMethodOutput,
};

#[derive(Clone)]
pub struct System {
    pid: String,
    aid: String,
    executor: Executor,
    platform: Arc<Box<dyn Platform>>,
    filesystem: FilesystemOverlay,
    event_queue: Arc<RwLock<EventQueue>>,
    audio: Arc<RwLock<Audio>>,
    input_method: Arc<RwLock<InputMethod>>,
    task_runner: Arc<dyn TaskRunner>,
    /// The servers this run answers for itself, in place of ones that have been
    /// switched off for years. Empty unless the host registered one.
    local_network: Arc<RwLock<LocalNetwork>>,
    /// Set once the title has been seen drawing into the LCD frame buffer
    /// itself. See [`System::title_drives_lcd`].
    title_drives_lcd: Arc<AtomicBool>,
    /// Whether this title draws its picture sideways into an upright panel.
    /// See [`System::title_draws_sideways`].
    title_draws_sideways: Arc<AtomicBool>,
    /// Whether this title lays its screens out below a status strip.
    /// See [`System::title_expects_annunciator`].
    title_expects_annunciator: Arc<AtomicBool>,
    /// How many rows that strip takes, or zero for the size table's own answer.
    /// See [`System::title_annunciator_rows`].
    title_annunciator_rows: Arc<AtomicU32>,
}

impl System {
    pub fn new<T>(platform: Box<dyn Platform>, pid: &str, aid: &str, task_runner: T) -> Self
    where
        T: TaskRunner + 'static,
    {
        let audio_sink = platform.audio_sink();

        let mut local_network = LocalNetwork::new();
        for endpoint in platform.local_endpoints() {
            local_network.register(endpoint);
        }

        // The servers this emulator answers for itself, behind whatever the host
        // offers: a run that sets one of the diagnostic endpoints is asking to
        // see the exchange rather than to have it answered.
        local_network.register(Box::new(crate::local_network::GpangEndpoint::new()));

        let platform = Arc::new(platform);

        Self {
            pid: pid.to_owned(),
            aid: aid.to_owned(), // TODO create metadata dictionary or something
            executor: Executor::new(),
            filesystem: FilesystemOverlay::new(platform.clone(), aid),
            platform,
            event_queue: Arc::new(RwLock::new(EventQueue::new())),
            audio: Arc::new(RwLock::new(Audio::new(audio_sink))),
            input_method: Arc::new(RwLock::new(InputMethod::new())),
            task_runner: Arc::new(task_runner),
            local_network: Arc::new(RwLock::new(local_network)),
            title_drives_lcd: Arc::new(AtomicBool::new(false)),
            title_draws_sideways: Arc::new(AtomicBool::new(false)),
            title_expects_annunciator: Arc::new(AtomicBool::new(false)),
            title_annunciator_rows: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn tick(&mut self) -> Result<()> {
        let platform = self.platform.clone();
        self.executor.tick(move || platform.now())
    }

    /// Whether the emulator has nothing runnable until a timer fires (every
    /// task asleep with its wake-up in the future). The host loop uses this to
    /// stop early and sleep the leftover budget rather than busy-waiting.
    pub fn is_idle(&self) -> bool {
        self.executor.is_idle()
    }

    pub fn spawn<C>(&self, callable: C)
    where
        C: AsyncCallable<Result<()>> + 'static + Send,
    {
        let runner_clone = self.task_runner.clone();
        self.executor.spawn(async move || runner_clone.run(Box::pin(callable.call())).await);
    }

    pub fn sleep(&self, timeout: u64) -> SleepFuture {
        SleepFuture::new(timeout, &self.executor)
    }

    pub fn current_task_id(&self) -> u64 {
        self.executor.current_task_id()
    }

    pub fn yield_now(&self) -> YieldFuture {
        YieldFuture::waiting(&self.executor)
    }

    /// Unified filesystem view. Reads consult the persistent platform
    /// backend first and fall back to the in-memory virtual layer loaded
    /// from archives; writes always hit the platform backend.
    pub fn filesystem(&self) -> &FilesystemOverlay {
        &self.filesystem
    }

    pub fn pid(&self) -> &str {
        &self.pid
    }

    pub fn aid(&self) -> &str {
        &self.aid
    }

    pub fn local_network(&self) -> RwLockWriteGuard<'_, LocalNetwork> {
        self.local_network.write()
    }

    pub fn platform(&self) -> &dyn Platform {
        self.platform.as_ref().as_ref()
    }

    pub fn audio(&self) -> RwLockWriteGuard<'_, Audio> {
        self.audio.as_ref().write()
    }

    pub fn event_queue(&self) -> RwLockWriteGuard<'_, EventQueue> {
        self.event_queue.write()
    }

    /// Whether the title paints the LCD frame buffer itself.
    ///
    /// A title whose drawing is a C engine writes that buffer directly and never
    /// flushes it, because on the handset the buffer is the display. The
    /// emulator's tick notices the first such frame and says so here, and the
    /// MIDP layer then stops flushing its own screen image over the top - which
    /// is the same reason `Display.disablePaint` exists for the clet wrapper,
    /// reached for a title that never goes through that wrapper.
    ///
    /// Stays false for every title that draws through the Java layer, so their
    /// frames keep reaching the screen the way they always have.
    pub fn title_drives_lcd(&self) -> bool {
        self.title_drives_lcd.load(Ordering::SeqCst)
    }

    pub fn set_title_drives_lcd(&self) {
        self.title_drives_lcd.store(true, Ordering::SeqCst);
    }

    /// Whether the title composes a landscape picture and copies it onto its
    /// upright panel a quarter turn clockwise, because it was written to be
    /// played with the handset held sideways.
    ///
    /// A fact about one title rather than anything the API reports, so it is
    /// looked up in `crate::quirks` and set here by the emulator that loaded
    /// the archive. [`crate::present`] is what reads it.
    pub fn title_draws_sideways(&self) -> bool {
        self.title_draws_sideways.load(Ordering::SeqCst)
    }

    pub fn set_title_draws_sideways(&self, sideways: bool) {
        self.title_draws_sideways.store(sideways, Ordering::SeqCst);
    }

    /// Whether the title lays its screens out below the handset's status strip,
    /// so the strip has to be there for them to land where they belong.
    ///
    /// The same fact the WIPI-C side reads out of `ANNUNCIATOR_ROWS_PTR`, for
    /// the titles that reach the strip through `org.kwis.msp.lwc` instead.
    /// Looked up in `crate::quirks` and set here by the emulator that loaded the
    /// archive.
    pub fn title_expects_annunciator(&self) -> bool {
        self.title_expects_annunciator.load(Ordering::SeqCst)
    }

    pub fn set_title_expects_annunciator(&self, expects: bool) {
        self.title_expects_annunciator.store(expects, Ordering::SeqCst);
    }

    /// How tall that strip is for this title, or `None` to take the height the
    /// platform's own size table gives the panel's width.
    pub fn title_annunciator_rows(&self) -> Option<u32> {
        match self.title_annunciator_rows.load(Ordering::SeqCst) {
            0 => None,
            rows => Some(rows),
        }
    }

    pub fn set_title_annunciator_rows(&self, rows: Option<u32>) {
        self.title_annunciator_rows.store(rows.unwrap_or(0), Ordering::SeqCst);
    }

    pub fn current_input_mode(&self) -> u32 {
        self.input_method.read().current_mode()
    }

    pub fn set_current_input_mode(&self, mode: u32) {
        self.input_method.write().set_current_mode(mode);
    }

    pub fn input_composition_size(&self) -> usize {
        self.input_method.read().composition_size()
    }

    pub fn set_input_composition_size(&self, size: usize) {
        self.input_method.write().set_composition_size(size);
    }

    /// Feeds a keypress to the handset's input method.
    ///
    /// The guest clock goes with it: multi-tap finishes a character when the
    /// same key is left alone long enough, and measuring that on guest time
    /// rather than the host's keeps a frontend that runs ticks in batches
    /// typing the same text as one running live.
    /// Lets go of the character the input method is still building, without
    /// finishing it into anything.
    ///
    /// For a field whose whole text has been set from under it: what it was
    /// composing is either already in that text or was meant to be dropped, so
    /// finishing it would add a second copy or text nobody asked for.
    pub fn reset_input_method_composition(&self) {
        let mode = self.input_method.read().current_mode();

        self.input_method.write().set_current_mode(mode);
    }

    pub fn handle_input_method(&self, key: i8, event: u32) -> InputMethodOutput {
        let now = self.platform().now();

        self.input_method.write().handle_input(key, event, now)
    }
}
