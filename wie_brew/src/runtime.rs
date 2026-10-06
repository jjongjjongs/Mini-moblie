//! The platform a BREW module runs against.
//!
//! A module meets three kinds of thing, and every call it makes lands in one
//! of them:
//!
//! 1. **The flat table below the image.** The word at `IMAGE_BASE - 4` points
//!    at it, and the module reaches it from everywhere through that one word:
//!    `memcpy`, `strlen`, `sprintf`, the allocator, a millisecond clock and
//!    "which application am I". This is the AEE helper table.
//! 2. **The object handed to the entry point** - the shell. Its first three
//!    methods raise a count, drop it and answer a query by interface number;
//!    the rest load resources, post events and hand over a frame callback.
//! 3. **One object per interface number** the module queries: the display,
//!    memory, files, sound, a timed service, the handset.
//!
//! Each is a table of trap stubs here, one stub per slot, and a slot nobody
//! answers returns zero - which is how this was mapped: run, see which slot the
//! module asked for, answer it, run again.

use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    sync::Arc,
    vec,
    vec::Vec,
};

use spin::Mutex;

use wie_backend::{
    System,
    canvas::{ArgbPixel, Canvas, Clip, Color, Image, ImageBufferCanvas, TextAlignment, VecImageBuffer, baseline_px},
};
use wie_core_arm::{Allocator, ArmCore, SvcId};
use wie_util::{ByteRead, ByteWrite, Result, WieError, read_generic, read_null_terminated_string_bytes, write_generic};

use crate::{archive::BrewArchive, bitmap, resource::ResourceFile};

/// Where the module is loaded. It is position independent - it carries no
/// relocations and reaches its own data PC-relatively - so this is the
/// platform's choice; it is the address KTF maps its executables at.
pub const IMAGE_BASE: u32 = 0x0010_0000;

const PAGE: u32 = 0x1000;

/// The page under the image, whose last two words are the interface version
/// and the platform table.
const HEADER_BASE: u32 = IMAGE_BASE - PAGE;
const INTERFACE_VERSION: u32 = 0x10000;

/// Trap tables, one page each, and the scratch words the start-up calls are
/// handed.
const TABLE_BASE: u32 = 0x3000_0000;
const TABLE_SLOTS: u32 = 128;
const MAX_SURFACES: u32 = 16;
const SCRATCH_BASE: u32 = TABLE_BASE + MAX_SURFACES * PAGE;
const ENTRY_OBJECT: u32 = SCRATCH_BASE;
const ENTRY_SPARE: u32 = SCRATCH_BASE + 0x100;
const ENTRY_OUT: u32 = SCRATCH_BASE + 0x200;
const APPLICATION_OUT: u32 = SCRATCH_BASE + 0x300;

pub const SVC_CATEGORY: u32 = 1;

// The flat table.
const SLOT_MEMORY_COPY: u32 = 0x00;
const SLOT_MEMORY_SET: u32 = 0x04;
const SLOT_STRING_COPY: u32 = 0x08;
const SLOT_STRING_JOIN: u32 = 0x0c;
const SLOT_STRING_SIZE: u32 = 0x14;
const SLOT_FORMAT: u32 = 0x20;
const SLOT_CREATE_OBJECT: u32 = 0x64;
const SLOT_ALLOCATE: u32 = 0x68;
const SLOT_FREE: u32 = 0x6c;
const SLOT_REALLOCATE: u32 = 0x74;
const SLOT_INITIAL: u32 = 0x8c;
const SLOT_MILLISECONDS: u32 = 0xac;
const SLOT_ELAPSED: u32 = 0xb0;
const SLOT_DESTROY_OBJECT: u32 = 0xbc;
const SLOT_CURRENT_APPLICATION: u32 = 0xc0;
const SLOT_BOUNDED_COPY: u32 = 0xc8;
const SLOT_COMPARE: u32 = 0xcc;

// Every object's first three methods.
const OBJECT_ADD_REF: u32 = 0x00;
const OBJECT_RELEASE: u32 = 0x04;
const OBJECT_QUERY_INTERFACE: u32 = 0x08;

// The shell.
const SHELL_DISPLAY_INFO: u32 = 0x10;
const SHELL_CLOSE_APPLET: u32 = 0x18;
const SHELL_SCHEDULE: u32 = 0x2c;
const SHELL_RESUME: u32 = 0x30;
const SHELL_LOAD_RESOURCE: u32 = 0x48;
const SHELL_FREE_RESOURCE: u32 = 0x50;
const SHELL_POST_EVENT: u32 = 0x54;

// The interfaces, by the number the module queries.
const INTERFACE_DISPLAY: u32 = 0x100_1001;
const INTERFACE_MEMORY: u32 = 0x100_1002;
const INTERFACE_FILE: u32 = 0x100_1003;
const INTERFACE_TIMED: u32 = 0x100_100b;
const INTERFACE_SOUND: u32 = 0x100_2000;
const INTERFACE_CERTIFICATE: u32 = 0x103_028a;
const INTERFACE_HANDSET: u32 = 0x180_00fe;

const DISPLAY_FONT_METRICS: u32 = 0x08;
const DISPLAY_TEXT: u32 = 0x10;
const DISPLAY_RECTANGLE: u32 = 0x14;
const DISPLAY_BLIT: u32 = 0x18;
const DISPLAY_PRESENT: u32 = 0x1c;
const DISPLAY_KEEP_AWAKE: u32 = 0x24;
const DISPLAY_COLOUR: u32 = 0x28;

const MEMORY_FITS: u32 = 0x18;
const MEMORY_FREE: u32 = 0x1c;

const SOURCE_ADD_LISTENER: u32 = 0x08;

const TIMED_START: u32 = 0x28;
const TIMED_STOP: u32 = 0x2c;
const TIMED_EVENT_KIND: u32 = 1;
const TIMED_EVENT_END: u32 = 2;

const SOUND_SET_CLIP: u32 = 0x0c;
const SOUND_PLAY: u32 = 0x10;
const SOUND_STOP: u32 = 0x14;
const SOUND_EVENT_KIND: u32 = 1;
const SOUND_EVENT_END: u32 = 14;

const CERTIFICATE_CHECK: u32 = 0x08;

const HANDSET_RECORD: u32 = 0x08;
const HANDSET_RECORD_SIZE: usize = 0x26;

// The file interface, and the object one open file is.
const FILES_OPEN: u32 = 0x08;
const FILES_INFORMATION: u32 = 0x0c;
const FILES_CREATE: u32 = 0x10;
const FILES_EXISTS: u32 = 0x1c;
const FILES_LAST_ERROR: u32 = 0x24;
const FILE_CLOSE: u32 = 0x04;
const FILE_READ: u32 = 0x0c;
const FILE_WRITE: u32 = 0x14;
const FILE_STATUS: u32 = 0x18;
const FILE_SEEK: u32 = 0x1c;
const FILE_RECORD_SIZE: usize = 0x0c;
const FILE_LENGTH_OFFSET: usize = 0x08;
const FILE_FAILED: u32 = 1;
const MODE_READ: u32 = 1;
const MODE_WRITE_TRUNCATE: u32 = 4;

/// The class `createObject` is asked for when it is handed a bitmap.
const CLASS_IMAGE: u32 = 0x100_4001;

/// What the title's own object takes as events.
pub const EVENT_START: u32 = 0;
const EVENT_KEY_TYPED: u32 = 0x100;
const EVENT_KEY_DOWN: u32 = 0x101;
const EVENT_KEY_UP: u32 = 0x102;

const MAX_STRING: u32 = 64 << 10;
const MAX_TRANSFER: u32 = 8 << 20;
const MAX_TEXT: i32 = 4 << 10;
const MAX_QUEUED: usize = 1024;

/// The handset's face, in pixels.
const FONT_HEIGHT: f32 = 12.0;

/// Magenta, which the platform's images never draw.
const TRANSPARENT_565: u16 = 0xf81f;

const TEXT_COLOUR_ITEM: u32 = 1;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Surface {
    Platform,
    Shell,
    File,
    Interface(u32),
}

struct Listener {
    source: u32,
    function: u32,
    context: u32,
}

struct Schedule {
    interval: u64,
    function: u32,
    context: u32,
    due: u64,
}

struct KeptImage {
    data: u32,
    bytes: Vec<u8>,
    decoded: VecImageBuffer<ArgbPixel>,
}

struct OpenFile {
    key: String,
    data: Vec<u8>,
    position: usize,
    writable: bool,
}

struct State {
    surfaces: Vec<(Surface, u32)>,
    interfaces: BTreeMap<u32, u32>,
    blocks: BTreeMap<u32, u32>,
    application: u32,
    started: u64,

    width: u32,
    height: u32,
    screen: ImageBufferCanvas<VecImageBuffer<ArgbPixel>>,
    dirty: bool,
    images: BTreeMap<u32, KeptImage>,
    colours: BTreeMap<u32, u32>,

    listeners: Vec<Listener>,
    frame: Option<Schedule>,
    timed_due: Option<u64>,
    posted: Vec<(u32, u32, u32)>,
    resumes: Vec<(u32, u32)>,

    files: BTreeMap<u32, OpenFile>,
    file_table: u32,
    file_failure: u32,
    resources: BTreeMap<String, Option<Arc<ResourceFile>>>,

    clip: Option<u32>,
    sounding: bool,
}

/// What the trap handler and the run loop share.
#[derive(Clone)]
pub struct Brew {
    system: System,
    state: Arc<Mutex<State>>,
}

impl Brew {
    /// Maps the module and the platform it finds below it.
    pub fn new(core: &mut ArmCore, system: &System, archive: &BrewArchive) -> Result<Self> {
        let mapped = (archive.module.len() as u32).next_multiple_of(PAGE);
        core.load(&archive.module, IMAGE_BASE, mapped as usize)?;
        core.map(HEADER_BASE, PAGE)?;
        core.map(TABLE_BASE, (MAX_SURFACES + 1) * PAGE)?;
        Allocator::init(core)?;

        let (width, height) = {
            let screen = system.platform().screen();
            (screen.width(), screen.height())
        };

        let mut screen = ImageBufferCanvas::new(VecImageBuffer::<ArgbPixel>::new(width, height));
        screen.fill_rect(0, 0, width, height, BLACK, full_clip(width, height));

        let brew = Self {
            system: system.clone(),
            state: Arc::new(Mutex::new(State {
                surfaces: Vec::new(),
                interfaces: BTreeMap::new(),
                blocks: BTreeMap::new(),
                application: 0,
                started: system.platform().now().raw(),
                width,
                height,
                screen,
                dirty: true,
                images: BTreeMap::new(),
                colours: BTreeMap::new(),
                listeners: Vec::new(),
                frame: None,
                timed_due: None,
                posted: Vec::new(),
                resumes: Vec::new(),
                files: BTreeMap::new(),
                file_table: 0,
                file_failure: 0,
                resources: BTreeMap::new(),
                clip: None,
                sounding: false,
            })),
        };

        core.register_svc_handler(SVC_CATEGORY, handle_trap, &brew)?;

        let platform = brew.add_surface(core, Surface::Platform)?;
        let shell = brew.add_surface(core, Surface::Shell)?;
        let file = brew.add_surface(core, Surface::File)?;
        brew.state.lock().file_table = file;

        write_generic(core, ENTRY_OBJECT, shell)?;
        write_generic(core, IMAGE_BASE - 8, INTERFACE_VERSION)?;
        write_generic(core, IMAGE_BASE - 4, platform)?;

        Ok(brew)
    }

    /// A trap table: every slot a stub that traps with its surface and slot.
    fn add_surface(&self, core: &mut ArmCore, surface: Surface) -> Result<u32> {
        let index = self.state.lock().surfaces.len() as u32;
        if index >= MAX_SURFACES {
            return Err(WieError::FatalError(format!("BREW module asked for more than {MAX_SURFACES} interfaces")));
        }

        let table = TABLE_BASE + index * PAGE;
        for slot in 0..TABLE_SLOTS {
            let stub = core.make_svc_stub(SVC_CATEGORY, (index << 16) | slot)?;
            write_generic(core, table + slot * 4, stub)?;
        }

        self.state.lock().surfaces.push((surface, table));

        Ok(table)
    }

    /// Calls the entry, has it build the title's object, and sends that object
    /// its start event.
    ///
    /// The entry is `AEEMod_Load(IShell *, void *, IModule **)`: it writes a
    /// module object through its third argument. That object's third method is
    /// `IModule::CreateInstance(IModule *, IShell *, AEECLSID, void **)`, asked
    /// for the ClassID the information file carries, and it writes the title's
    /// object through its last argument - both answers are read off the out
    /// parameter, never the result. A small integer sent to the title's object
    /// is an event, and 0 is the one that starts it.
    pub async fn boot(&self, core: &mut ArmCore, identifier: u32) -> Result<()> {
        core.run_function::<u32>(IMAGE_BASE, &[ENTRY_OBJECT, ENTRY_SPARE, ENTRY_OUT]).await?;

        let module: u32 = read_generic(core, ENTRY_OUT)?;
        if module == 0 {
            return Err(WieError::FatalError("BREW module entry wrote no module object".into()));
        }
        let methods: u32 = read_generic(core, module)?;
        let create_instance: u32 = read_generic(core, methods + 8)?;

        core.run_function::<u32>(create_instance, &[module, ENTRY_OBJECT, identifier, APPLICATION_OUT])
            .await?;

        let application: u32 = read_generic(core, APPLICATION_OUT)?;
        if application == 0 {
            return Err(WieError::FatalError(format!("BREW module refused to create application {identifier:#x}")));
        }
        self.state.lock().application = application;

        tracing::info!("BREW application {identifier} created at {application:#x}");

        self.send_event(core, EVENT_START, 0, 0).await?;

        Ok(())
    }

    async fn send_event(&self, core: &mut ArmCore, event: u32, first: u32, second: u32) -> Result<u32> {
        let application = self.state.lock().application;
        let methods: u32 = read_generic(core, application)?;
        let handle_event: u32 = read_generic(core, methods + 8)?;

        core.run_function::<u32>(handle_event, &[application, event, first, second]).await
    }

    /// Delivers a key: the press and then the key typed, or the release.
    pub async fn key(&self, core: &mut ArmCore, code: u32, pressed: bool) -> Result<()> {
        if pressed {
            let down = self.send_event(core, EVENT_KEY_DOWN, code, 0).await?;
            let typed = self.send_event(core, EVENT_KEY_TYPED, code, 0).await?;
            tracing::debug!("BREW key {code:#x} down: handled {down}, typed: handled {typed}");
        } else {
            let up = self.send_event(core, EVENT_KEY_UP, code, 0).await?;
            tracing::debug!("BREW key {code:#x} up: handled {up}");
        }

        Ok(())
    }

    fn now(&self) -> u64 {
        self.system.platform().now().raw()
    }

    /// Runs whatever is due, and answers how long until something is.
    ///
    /// The title has no loop of its own. Its start event ends by handing the
    /// shell an interval, a function and a context, and everything after that
    /// happens inside that function: it reads the clock, steps its state,
    /// draws, and is called again.
    pub async fn step(&self, core: &mut ArmCore) -> Result<Option<u64>> {
        let posted = core::mem::take(&mut self.state.lock().posted);
        for (event, first, second) in posted {
            self.send_event(core, event, first, second).await?;
        }

        let resumes = core::mem::take(&mut self.state.lock().resumes);
        for (function, context) in resumes {
            core.run_function::<u32>(function, &[context]).await?;
        }

        self.advance_sound(core).await?;
        self.advance_timed(core).await?;

        let now = self.now();
        let frame = self.state.lock().frame.as_ref().map(|x| (x.due, x.interval, x.function, x.context));
        let Some((due, interval, function, context)) = frame else {
            return Ok(None);
        };

        if now < due {
            return Ok(Some(due - now));
        }

        if let Some(frame) = self.state.lock().frame.as_mut() {
            frame.due = now + interval;
        }
        core.run_function::<u32>(function, &[context]).await?;

        Ok(Some(0))
    }

    /// The screen, when something has been drawn on it since it was last
    /// asked for.
    pub fn take_frame(&self, present: impl FnOnce(&dyn Image)) {
        let mut state = self.state.lock();
        if !state.dirty {
            return;
        }
        state.dirty = false;

        present(state.screen.image());
    }

    async fn advance_sound(&self, core: &mut ArmCore) -> Result<()> {
        let (clip, sounding) = {
            let state = self.state.lock();
            (state.clip, state.sounding)
        };
        let Some(clip) = clip else { return Ok(()) };
        if !sounding || self.system.audio().is_playing(clip) {
            return Ok(());
        }

        self.state.lock().sounding = false;
        self.deliver(core, INTERFACE_SOUND, SOUND_EVENT_KIND, SOUND_EVENT_END).await
    }

    async fn advance_timed(&self, core: &mut ArmCore) -> Result<()> {
        let due = self.state.lock().timed_due;
        match due {
            Some(due) if self.now() >= due => {
                self.state.lock().timed_due = None;
                self.deliver(core, INTERFACE_TIMED, TIMED_EVENT_KIND, TIMED_EVENT_END).await
            }
            _ => Ok(()),
        }
    }

    /// Calls the listener the title registered on `source`, if it registered
    /// one.
    async fn deliver(&self, core: &mut ArmCore, source: u32, first: u32, second: u32) -> Result<()> {
        let listener = self
            .state
            .lock()
            .listeners
            .iter()
            .find(|x| x.source == source)
            .map(|x| (x.function, x.context));

        if let Some((function, context)) = listener
            && function != 0
        {
            core.run_function::<u32>(function, &[context, first, second]).await?;
        }

        Ok(())
    }
}

const BLACK: Color = Color { a: 0xff, r: 0, g: 0, b: 0 };

fn full_clip(width: u32, height: u32) -> Clip {
    Clip { x: 0, y: 0, width, height }
}

/// Colours arrive packed as `0xRRGGBBxx`.
fn packed_colour(packed: u32) -> Color {
    Color {
        a: 0xff,
        r: (packed >> 24) as u8,
        g: (packed >> 16) as u8,
        b: (packed >> 8) as u8,
    }
}

fn params<const N: usize>(core: &ArmCore) -> Result<[u32; N]> {
    let mut result = [0; N];
    for (index, value) in result.iter_mut().enumerate() {
        *value = core.read_param(index)?;
    }

    Ok(result)
}

fn read_bytes(core: &ArmCore, address: u32, length: u32) -> Result<Vec<u8>> {
    let mut data = vec![0; length as usize];
    core.read_bytes(address, &mut data)?;

    Ok(data)
}

fn read_string(core: &ArmCore, address: u32) -> Result<Vec<u8>> {
    if address == 0 {
        return Err(WieError::FatalError("BREW library call on a null string".into()));
    }

    let text = read_null_terminated_string_bytes(core, address)?;
    if text.len() as u32 >= MAX_STRING {
        return Err(WieError::FatalError(format!("BREW string at {address:#x} is not terminated")));
    }

    Ok(text)
}

fn decode_euc_kr(bytes: &[u8]) -> String {
    encoding_rs::EUC_KR.decode(bytes).0.to_string()
}

/// The name a file is kept under: its base name, in lower case. A title asks
/// for its own files by bare name and in whatever case its code spelled.
fn file_key(name: &str) -> String {
    let name = name.replace('\\', "/");

    name.rsplit('/').next().unwrap_or(&name).to_ascii_lowercase()
}

async fn handle_trap(core: &mut ArmCore, brew: &mut Brew, id: SvcId) -> Result<u32> {
    let (index, slot) = (id.0 >> 16, id.0 & 0xffff);
    let offset = slot * 4;

    let surface = brew
        .state
        .lock()
        .surfaces
        .get(index as usize)
        .map(|x| x.0)
        .ok_or_else(|| WieError::FatalError(format!("BREW trap on surface {index}, which does not exist")))?;

    if tracing::enabled!(tracing::Level::TRACE) {
        let (_, lr) = core.read_pc_lr()?;
        let arguments: [u32; 4] = params(core)?;
        tracing::trace!("BREW trap {index}:{offset:#x} ({arguments:x?}) from lr={lr:#x}");
    }

    let result = match (surface, offset) {
        (Surface::Platform, SLOT_MEMORY_COPY | SLOT_BOUNDED_COPY) => memory_copy(core),
        (Surface::Platform, SLOT_MEMORY_SET) => memory_set(core),
        (Surface::Platform, SLOT_STRING_COPY) => string_copy(core),
        (Surface::Platform, SLOT_STRING_JOIN) => string_join(core),
        (Surface::Platform, SLOT_STRING_SIZE) => Ok(read_string(core, core.read_param(0)?)?.len() as u32),
        (Surface::Platform, SLOT_FORMAT) => format(core),
        (Surface::Platform, SLOT_COMPARE) => memory_compare(core),
        (Surface::Platform, SLOT_ALLOCATE) => brew.allocate(core, core.read_param(0)?),
        (Surface::Platform, SLOT_FREE) => {
            brew.free(core, core.read_param(0)?)?;
            Ok(0)
        }
        (Surface::Platform, SLOT_REALLOCATE) => {
            let [address, size] = params(core)?;
            brew.reallocate(core, address, size)
        }
        (Surface::Platform, SLOT_INITIAL) => Ok(0),
        (Surface::Platform, SLOT_MILLISECONDS | SLOT_ELAPSED) => Ok((brew.now() - brew.state.lock().started) as u32),
        (Surface::Platform, SLOT_CURRENT_APPLICATION) => {
            let application = brew.state.lock().application;
            if application == 0 {
                Err(WieError::FatalError("BREW module asked for its application before creating it".into()))
            } else {
                Ok(application)
            }
        }
        (Surface::Platform, SLOT_CREATE_OBJECT) => brew.create_object(core),
        (Surface::Platform, SLOT_DESTROY_OBJECT) => {
            let object = core.read_param(0)?;
            if object == 0 {
                Ok(0)
            } else {
                brew.state.lock().images.remove(&object);
                brew.free(core, object)?;
                Ok(1)
            }
        }

        (_, OBJECT_ADD_REF | OBJECT_RELEASE) if surface != Surface::File => Ok(1),
        (Surface::Shell, OBJECT_QUERY_INTERFACE) => brew.query_interface(core),
        (Surface::Shell, SHELL_DISPLAY_INFO) => brew.display_info(core),
        // `ISHELL_CloseApplet`, which is what the title's 게임종료 calls. The
        // title stops scheduling frames once it has asked; the handset goes
        // back to its menu.
        (Surface::Shell, SHELL_CLOSE_APPLET) => {
            tracing::info!("BREW application closed itself");
            brew.state.lock().frame = None;
            brew.system.platform().exit();
            Ok(0)
        }
        (Surface::Shell, SHELL_SCHEDULE) => {
            let [_, interval, function, context] = params(core)?;
            let interval = interval.max(1) as u64;
            brew.state.lock().frame = Some(Schedule {
                interval,
                function,
                context,
                due: brew.now() + interval,
            });
            Ok(1)
        }
        (Surface::Shell, SHELL_RESUME) => {
            let [_, function, context] = params(core)?;
            let mut state = brew.state.lock();
            if function != 0 && state.resumes.len() < MAX_QUEUED {
                state.resumes.push((function, context));
            }
            Ok(1)
        }
        (Surface::Shell, SHELL_POST_EVENT) => {
            let [_, _, _, event, first, second] = params(core)?;
            let mut state = brew.state.lock();
            if state.posted.len() < MAX_QUEUED {
                state.posted.push((event, first, second));
            }
            Ok(1)
        }
        (Surface::Shell, SHELL_LOAD_RESOURCE) => brew.load_resource(core).await,
        (Surface::Shell, SHELL_FREE_RESOURCE) => {
            brew.free(core, core.read_param(1)?)?;
            Ok(0)
        }

        (Surface::Interface(INTERFACE_DISPLAY), DISPLAY_FONT_METRICS) => font_metrics(core),
        (Surface::Interface(INTERFACE_DISPLAY), DISPLAY_TEXT) => brew.draw_text(core),
        (Surface::Interface(INTERFACE_DISPLAY), DISPLAY_RECTANGLE) => brew.draw_rectangle(core),
        (Surface::Interface(INTERFACE_DISPLAY), DISPLAY_BLIT) => brew.blit(core),
        (Surface::Interface(INTERFACE_DISPLAY), DISPLAY_PRESENT) => {
            brew.state.lock().dirty = true;
            Ok(1)
        }
        (Surface::Interface(INTERFACE_DISPLAY), DISPLAY_KEEP_AWAKE) => Ok(1),
        (Surface::Interface(INTERFACE_DISPLAY), DISPLAY_COLOUR) => {
            let [_, item, value] = params(core)?;
            brew.state.lock().colours.insert(item, value);
            Ok(0)
        }

        // The module walks its request up until this says no, and keeps the
        // largest block both this and the allocator will give.
        (Surface::Interface(INTERFACE_MEMORY), MEMORY_FITS) => Ok((core.read_param(1)? <= brew.available()) as u32),
        (Surface::Interface(INTERFACE_MEMORY), MEMORY_FREE) => Ok(brew.available()),

        (Surface::Interface(source @ (INTERFACE_SOUND | INTERFACE_TIMED)), SOURCE_ADD_LISTENER) => {
            let [_, function, context] = params(core)?;
            brew.state.lock().listeners.push(Listener { source, function, context });
            Ok(1)
        }
        (Surface::Interface(INTERFACE_TIMED), TIMED_START) => {
            let milliseconds = core.read_param(1)? as u64;
            brew.state.lock().timed_due = Some(brew.now() + milliseconds);
            Ok(1)
        }
        (Surface::Interface(INTERFACE_TIMED), TIMED_STOP) => {
            brew.state.lock().timed_due = None;
            Ok(1)
        }
        (Surface::Interface(INTERFACE_SOUND), SOUND_SET_CLIP) => brew.set_clip(core),
        (Surface::Interface(INTERFACE_SOUND), SOUND_PLAY) => {
            let clip = brew.state.lock().clip;
            if let Some(clip) = clip {
                let _ = brew.system.audio().play(&brew.system, clip, false);
                brew.state.lock().sounding = true;
            }
            Ok(1)
        }
        (Surface::Interface(INTERFACE_SOUND), SOUND_STOP) => {
            let clip = brew.state.lock().clip;
            if let Some(clip) = clip {
                brew.system.audio().stop(clip);
            }
            brew.state.lock().sounding = false;
            Ok(1)
        }

        (Surface::Interface(INTERFACE_CERTIFICATE), CERTIFICATE_CHECK) => Ok(0),
        (Surface::Interface(INTERFACE_HANDSET), HANDSET_RECORD) => {
            let out = core.read_param(1)?;
            if out == 0 {
                Ok(0)
            } else {
                let mut record = [0u8; HANDSET_RECORD_SIZE];
                let number = wie_backend::subscriber::FALLBACK.as_bytes();
                record[..number.len()].copy_from_slice(number);
                core.write_bytes(out, &record)?;
                Ok(1)
            }
        }

        (Surface::Interface(INTERFACE_FILE), FILES_OPEN) => brew.open_file(core).await,
        (Surface::Interface(INTERFACE_FILE), FILES_INFORMATION) => brew.file_information(core).await,
        (Surface::Interface(INTERFACE_FILE), FILES_EXISTS) => brew.file_exists(core).await,
        (Surface::Interface(INTERFACE_FILE), FILES_CREATE) => brew.create_file(core).await,
        (Surface::Interface(INTERFACE_FILE), FILES_LAST_ERROR) => Ok(brew.state.lock().file_failure),

        (Surface::File, FILE_CLOSE) => brew.close_file(core).await,
        (Surface::File, FILE_READ) => brew.read_file(core),
        (Surface::File, FILE_WRITE) => brew.write_file(core).await,
        (Surface::File, FILE_SEEK) => brew.seek_file(core),
        (Surface::File, FILE_STATUS) => brew.file_status(core),

        _ => {
            let (_, lr) = core.read_pc_lr()?;
            let arguments: [u32; 4] = params(core)?;
            let name = match surface {
                Surface::Platform => "platform".to_string(),
                Surface::Shell => "shell".to_string(),
                Surface::File => "file".to_string(),
                Surface::Interface(id) => format!("interface {id:#x}"),
            };
            tracing::warn!("BREW {name} slot {offset:#x} is not answered ({arguments:x?}) from lr={lr:#x}");

            Ok(0)
        }
    };

    if let Ok(value) = &result {
        tracing::trace!("BREW trap {index}:{offset:#x} answered {value:#x}");
    }

    result
}

fn memory_copy(core: &mut ArmCore) -> Result<u32> {
    let [destination, source, length] = params(core)?;
    if length == 0 {
        return Ok(destination);
    }
    if length > MAX_TRANSFER {
        return Err(WieError::FatalError(format!("BREW copy of {length} bytes from {source:#x}")));
    }

    let data = read_bytes(core, source, length)?;
    core.write_bytes(destination, &data)?;

    Ok(destination)
}

fn memory_set(core: &mut ArmCore) -> Result<u32> {
    let [destination, value, length] = params(core)?;
    if length == 0 {
        return Ok(destination);
    }
    if length > MAX_TRANSFER {
        return Err(WieError::FatalError(format!("BREW fill of {length} bytes at {destination:#x}")));
    }

    core.write_bytes(destination, &vec![value as u8; length as usize])?;

    Ok(destination)
}

fn memory_compare(core: &mut ArmCore) -> Result<u32> {
    let [first, second, length] = params(core)?;
    if length == 0 {
        return Ok(0);
    }
    if length > MAX_TRANSFER {
        return Err(WieError::FatalError(format!("BREW comparison of {length} bytes at {first:#x}")));
    }

    let left = read_bytes(core, first, length)?;
    let right = read_bytes(core, second, length)?;

    Ok(match left.cmp(&right) {
        core::cmp::Ordering::Less => u32::MAX,
        core::cmp::Ordering::Equal => 0,
        core::cmp::Ordering::Greater => 1,
    })
}

fn string_copy(core: &mut ArmCore) -> Result<u32> {
    let [destination, source] = params(core)?;
    let mut text = read_string(core, source)?;
    text.push(0);
    core.write_bytes(destination, &text)?;

    Ok(destination)
}

fn string_join(core: &mut ArmCore) -> Result<u32> {
    let [destination, source] = params(core)?;
    let head = read_string(core, destination)?;
    let mut tail = read_string(core, source)?;
    tail.push(0);
    core.write_bytes(destination + head.len() as u32, &tail)?;

    Ok(destination)
}

/// `sprintf(buffer, format, ...)`, through the WIPI-C formatter, with the
/// variadic words read from `r2` on and then off the stack.
fn format(core: &mut ArmCore) -> Result<u32> {
    let [destination, format_address] = params(core)?;
    let template = read_string(core, format_address)?;

    let mut arguments = Vec::with_capacity(12);
    for index in 2..14 {
        arguments.push(core.read_param(index)?);
    }

    let mut rendered = wie_wipi_c::api::kernel::format_varargs(&template, &arguments, &mut |address| read_string(core, address))?;
    let length = rendered.len() as u32;
    rendered.push(0);
    core.write_bytes(destination, &rendered)?;

    Ok(length)
}

/// The handset face's height, with its ascent and descent written through the
/// two pointers given.
fn font_metrics(core: &mut ArmCore) -> Result<u32> {
    let [_, _, ascent_out, descent_out] = params(core)?;
    let ascent = baseline_px(FONT_HEIGHT) as u32;
    let descent = FONT_HEIGHT as u32 - ascent;

    if ascent_out != 0 {
        write_generic(core, ascent_out, ascent)?;
    }
    if descent_out != 0 {
        write_generic(core, descent_out, descent)?;
    }

    Ok(FONT_HEIGHT as u32)
}

/// How much the title is told it may allocate, in all.
const ARENA_SIZE: u32 = 6 << 20;

impl Brew {
    fn available(&self) -> u32 {
        let used: u32 = self.state.lock().blocks.values().sum();

        ARENA_SIZE.saturating_sub(used)
    }

    fn allocate(&self, core: &mut ArmCore, size: u32) -> Result<u32> {
        if size == 0 || size > MAX_TRANSFER {
            return Err(WieError::FatalError(format!("BREW allocation of {size} bytes")));
        }

        let size = size.next_multiple_of(8);
        let address = Allocator::alloc(core, size)?;
        core.write_bytes(address, &vec![0; size as usize])?;
        self.state.lock().blocks.insert(address, size);

        Ok(address)
    }

    /// Frees a block this platform handed out, and nothing else: a title frees
    /// what it never allocated as often as not.
    fn free(&self, core: &mut ArmCore, address: u32) -> Result<()> {
        let size = self.state.lock().blocks.remove(&address);
        if let Some(size) = size {
            Allocator::free(core, address, size)?;
        }

        Ok(())
    }

    fn reallocate(&self, core: &mut ArmCore, address: u32, size: u32) -> Result<u32> {
        if size == 0 {
            self.free(core, address)?;
            return Ok(0);
        }

        let grown = self.allocate(core, size)?;
        let kept = self.state.lock().blocks.get(&address).copied();
        if let Some(kept) = kept {
            let data = read_bytes(core, address, kept.min(size))?;
            core.write_bytes(grown, &data)?;
        }
        self.free(core, address)?;

        Ok(grown)
    }

    /// Answers an interface by number with an object of its own trap table,
    /// written through the out parameter. The module checks the pointer it
    /// passed rather than the result.
    fn query_interface(&self, core: &mut ArmCore) -> Result<u32> {
        let [_, identifier, out] = params(core)?;

        let existing = self.state.lock().interfaces.get(&identifier).copied();
        let object = match existing {
            Some(object) => object,
            None => {
                let table = self.add_surface(core, Surface::Interface(identifier))?;
                let object = self.allocate(core, 4)?;
                write_generic(core, object, table)?;
                self.state.lock().interfaces.insert(identifier, object);

                tracing::debug!("BREW interface {identifier:#x} at {object:#x}");

                object
            }
        };

        write_generic(core, out, object)?;

        Ok(0)
    }

    fn display_info(&self, core: &mut ArmCore) -> Result<u32> {
        let out = core.read_param(1)?;
        let (width, height) = {
            let state = self.state.lock();
            (state.width, state.height)
        };

        let mut record = [0u8; 0x10];
        record[0..2].copy_from_slice(&(width as u16).to_le_bytes());
        record[2..4].copy_from_slice(&(height as u16).to_le_bytes());
        record[0xe..0x10].copy_from_slice(&16u16.to_le_bytes());
        core.write_bytes(out, &record)?;

        Ok(1)
    }

    /// Fills the rectangle at `r1` (four 16-bit fields), or the whole screen
    /// when that is null, with the packed colour in `r3`.
    fn draw_rectangle(&self, core: &mut ArmCore) -> Result<u32> {
        let [_, rectangle, _, colour] = params(core)?;

        let mut state = self.state.lock();
        let (width, height) = (state.width, state.height);
        let clip = full_clip(width, height);

        let (x, y, w, h) = if rectangle != 0 {
            let record = read_bytes(core, rectangle, 8)?;
            let field = |offset: usize| i16::from_le_bytes([record[offset], record[offset + 1]]) as i32;
            (field(0), field(2), field(4), field(6))
        } else {
            (0, 0, width as i32, height as i32)
        };

        if w > 0 && h > 0 {
            state.screen.fill_rect(x, y, w as u32, h as u32, packed_colour(colour), clip);
        }

        Ok(1)
    }

    /// Draws part of an image: `r1`-`r3` are x, y and width, and the stack
    /// carries the height, the image object, the source x and y and a mode.
    /// Magenta is never drawn.
    fn blit(&self, core: &mut ArmCore) -> Result<u32> {
        let [_, x, y, width, height, object, source_x, source_y] = params(core)?;

        let data = {
            let state = self.state.lock();
            match state.images.get(&object) {
                Some(image) => (image.data, image.bytes.len() as u32),
                None => return Ok(0),
            }
        };

        // The title can write into its bitmap after handing it over, and draws
        // what it wrote; the copy is decoded again when the bytes differ.
        let current = read_bytes(core, data.0, data.1)?;

        let mut state = self.state.lock();
        let (screen_width, screen_height) = (state.width, state.height);
        let State { images, screen, .. } = &mut *state;
        let image = images.get_mut(&object).unwrap();
        if image.bytes != current {
            image.decoded = bitmap::decode(&current)?;
            image.bytes = current;
        }

        let (width, height) = (width as i32, height as i32);
        if width > 0 && height > 0 {
            screen.draw_with_color_key(
                x as i32,
                y as i32,
                width as u32,
                height as u32,
                &image.decoded,
                source_x as i32,
                source_y as i32,
                full_clip(screen_width, screen_height),
                TRANSPARENT_565,
            );
        }

        Ok(1)
    }

    /// Draws EUC-KR text: `r2` is the text and `r3` its length in bytes, or
    /// negative for a terminated string, and the stack carries x and y - the
    /// top of the line.
    fn draw_text(&self, core: &mut ArmCore) -> Result<u32> {
        let [_, _, address, count, x, y] = params(core)?;
        if address == 0 {
            return Ok(1);
        }

        let count = count as i32;
        let raw = if count < 0 {
            let text = read_string(core, address)?;
            padded_text(core, address, text)?
        } else if count == 0 || count > MAX_TEXT {
            return Ok(1);
        } else {
            read_bytes(core, address, count as u32)?
        };
        let text = decode_euc_kr(&raw);

        let mut state = self.state.lock();
        let ink = state.colours.get(&TEXT_COLOUR_ITEM).map_or(BLACK, |x| packed_colour(*x));
        let clip = full_clip(state.width, state.height);
        state.screen.draw_text(
            &text,
            x as i32,
            y as i32,
            FONT_HEIGHT,
            baseline_px(FONT_HEIGHT),
            TextAlignment::Left,
            ink,
            clip,
        );

        Ok(1)
    }

    /// `createObject(class, data)`. The one class answered is the image: the
    /// title hands over a pointer into a bitmap it loaded, and gets an object
    /// whose first word is a copy of it.
    fn create_object(&self, core: &mut ArmCore) -> Result<u32> {
        let [class, data] = params(core)?;
        if class != CLASS_IMAGE {
            tracing::warn!("BREW createObject of class {class:#x} is not answered");
            return Ok(0);
        }

        let header = read_bytes(core, data, bitmap::HEADER_SIZE as u32)?;
        let length = bitmap::bitmap_length(&header).ok_or_else(|| WieError::FatalError(format!("BREW image at {data:#x} is not a bitmap")))?;
        let bytes = read_bytes(core, data, length as u32)?;

        let kept = self.allocate(core, length as u32)?;
        core.write_bytes(kept, &bytes)?;
        let decoded = bitmap::decode(&bytes)?;

        let object = self.allocate(core, 8)?;
        write_generic(core, object, kept)?;
        self.state.lock().images.insert(object, KeptImage { data: kept, bytes, decoded });

        Ok(object)
    }

    async fn contents(&self, name: &str) -> Option<Vec<u8>> {
        let key = file_key(name);
        let filesystem = self.system.filesystem();

        let size = filesystem.size(&key).await?;
        let mut data = vec![0; size];
        filesystem.read(&key, 0, size, &mut data).await?;

        Some(data)
    }

    async fn keep(&self, key: &str, data: &[u8]) {
        let filesystem = self.system.filesystem();
        filesystem.truncate(key, 0).await;
        if !data.is_empty() {
            filesystem.write(key, 0, data).await;
        }
    }

    /// `loadResource(shell, file, number, kind)`: an item out of a resource
    /// file, copied into a block of its own.
    async fn load_resource(&self, core: &mut ArmCore) -> Result<u32> {
        let [_, name, number, kind] = params(core)?;
        let name = String::from_utf8_lossy(&read_string(core, name)?).to_string();

        let cached = self.state.lock().resources.get(&name).cloned();
        let file = match cached {
            Some(file) => file,
            None => {
                let file = match self.contents(&name).await {
                    Some(data) => match ResourceFile::parse(data) {
                        Ok(file) => Some(Arc::new(file)),
                        Err(error) => {
                            tracing::warn!("BREW {name} is not a resource file: {error}");
                            None
                        }
                    },
                    None => None,
                };
                self.state.lock().resources.insert(name.clone(), file.clone());
                file
            }
        };

        let Some(item) = file.as_ref().and_then(|file| file.item(kind as u16, number as u16)) else {
            tracing::debug!("BREW {name} carries no resource {number} of kind {kind}");
            return Ok(0);
        };
        if item.is_empty() {
            return Ok(0);
        }

        let block = self.allocate(core, item.len() as u32)?;
        core.write_bytes(block, item)?;

        Ok(block)
    }

    fn file_result(&self, worked: bool) -> u32 {
        let failure = if worked { 0 } else { FILE_FAILED };
        self.state.lock().file_failure = failure;

        failure
    }

    /// `open(files, name, mode)`: a file object, or null when a file to read
    /// is not there. Writing to a missing file makes it; mode 4 truncates.
    async fn open_file(&self, core: &mut ArmCore) -> Result<u32> {
        let [_, name, mode] = params(core)?;
        let name = String::from_utf8_lossy(&read_string(core, name)?).to_string();
        let key = file_key(&name);
        let writable = mode != MODE_READ;

        let mut data = self.contents(&name).await;
        tracing::debug!("BREW open {name} mode {mode}: {}", if data.is_some() { "found" } else { "missing" });

        if data.is_some() && mode == MODE_WRITE_TRUNCATE {
            self.keep(&key, &[]).await;
            data = Some(Vec::new());
        }
        let data = match data {
            Some(data) => data,
            None if writable => {
                self.keep(&key, &[]).await;
                Vec::new()
            }
            None => return Ok(0),
        };

        let object = self.allocate(core, 4)?;
        let table = self.state.lock().file_table;
        write_generic(core, object, table)?;
        self.state.lock().files.insert(
            object,
            OpenFile {
                key,
                data,
                position: 0,
                writable,
            },
        );

        Ok(object)
    }

    async fn file_information(&self, core: &mut ArmCore) -> Result<u32> {
        let [_, name, out] = params(core)?;
        let name = String::from_utf8_lossy(&read_string(core, name)?).to_string();
        let data = self.contents(&name).await;
        tracing::debug!("BREW information {name}: {:?}", data.as_ref().map(|x| x.len()));

        let mut record = [0u8; FILE_RECORD_SIZE];
        if let Some(data) = &data {
            record[FILE_LENGTH_OFFSET..FILE_LENGTH_OFFSET + 4].copy_from_slice(&(data.len() as u32).to_le_bytes());
        }
        core.write_bytes(out, &record)?;

        Ok(self.file_result(data.is_some()))
    }

    /// `IFILEMGR_Test`: whether a file is there, as a result code - zero when
    /// it is. The module's own wrapper reads it that way (`exists = result ==
    /// 0`), and so does every save and licence check behind it.
    async fn file_exists(&self, core: &mut ArmCore) -> Result<u32> {
        let name = String::from_utf8_lossy(&read_string(core, core.read_param(1)?)?).to_string();
        let found = self.contents(&name).await.is_some();
        tracing::debug!("BREW exists {name}: {found}");

        Ok(self.file_result(found))
    }

    async fn create_file(&self, core: &mut ArmCore) -> Result<u32> {
        let name = String::from_utf8_lossy(&read_string(core, core.read_param(1)?)?).to_string();
        if self.contents(&name).await.is_none() {
            self.keep(&file_key(&name), &[]).await;
        }

        Ok(self.file_result(true))
    }

    async fn close_file(&self, core: &mut ArmCore) -> Result<u32> {
        let object = core.read_param(0)?;
        let file = self.state.lock().files.remove(&object);
        if let Some(file) = file
            && file.writable
        {
            self.keep(&file.key, &file.data).await;
        }
        self.free(core, object)?;

        Ok(0)
    }

    fn file_status(&self, core: &mut ArmCore) -> Result<u32> {
        let [object, out] = params(core)?;
        let length = match self.state.lock().files.get(&object) {
            Some(file) => file.data.len() as u32,
            None => return Err(WieError::FatalError(format!("BREW file call on {object:#x}, which is not an open file"))),
        };

        let mut record = [0u8; FILE_RECORD_SIZE];
        record[FILE_LENGTH_OFFSET..FILE_LENGTH_OFFSET + 4].copy_from_slice(&length.to_le_bytes());
        core.write_bytes(out, &record)?;

        Ok(1)
    }

    fn read_file(&self, core: &mut ArmCore) -> Result<u32> {
        let [object, buffer, length] = params(core)?;

        let chunk = {
            let mut state = self.state.lock();
            let file = state
                .files
                .get_mut(&object)
                .ok_or_else(|| WieError::FatalError(format!("BREW read on {object:#x}, which is not an open file")))?;

            if file.position >= file.data.len() || length == 0 {
                return Ok(0);
            }
            let end = (file.position + length as usize).min(file.data.len());
            let chunk = file.data[file.position..end].to_vec();
            file.position = end;

            chunk
        };
        core.write_bytes(buffer, &chunk)?;

        Ok(chunk.len() as u32)
    }

    async fn write_file(&self, core: &mut ArmCore) -> Result<u32> {
        let [object, buffer, length] = params(core)?;
        if length == 0 {
            return Ok(0);
        }
        if length > MAX_TRANSFER {
            return Err(WieError::FatalError(format!("BREW write of {length} bytes")));
        }
        let chunk = read_bytes(core, buffer, length)?;

        let kept = {
            let mut state = self.state.lock();
            let file = state
                .files
                .get_mut(&object)
                .ok_or_else(|| WieError::FatalError(format!("BREW write on {object:#x}, which is not an open file")))?;
            if !file.writable {
                return Ok(0);
            }

            let end = file.position + chunk.len();
            if end > file.data.len() {
                file.data.resize(end, 0);
            }
            file.data[file.position..end].copy_from_slice(&chunk);
            file.position = end;

            (file.key.clone(), file.data.clone())
        };
        self.keep(&kept.0, &kept.1).await;

        Ok(length)
    }

    /// `seek(file, whence, offset)`: 0 from the start, 1 back from the last
    /// byte, 2 from where it is. A result of zero is success.
    fn seek_file(&self, core: &mut ArmCore) -> Result<u32> {
        let [object, whence, offset] = params(core)?;
        let mut state = self.state.lock();
        let file = state
            .files
            .get_mut(&object)
            .ok_or_else(|| WieError::FatalError(format!("BREW seek on {object:#x}, which is not an open file")))?;

        let offset = offset as i32 as i64;
        let position = match whence {
            0 => offset,
            1 => file.data.len() as i64 - 1 - offset,
            2 => file.position as i64 + offset,
            _ => return Ok(1),
        };
        if position < 0 || position > file.data.len() as i64 {
            return Ok(1);
        }
        file.position = position as usize;

        Ok(0)
    }

    /// `setClip(sound, kind, data)`: a SMAF clip the title loaded, played on
    /// `play`. Anything that is not SMAF is refused.
    fn set_clip(&self, core: &mut ArmCore) -> Result<u32> {
        let address = core.read_param(2)?;
        if address == 0 {
            return Ok(0);
        }

        let header = read_bytes(core, address, 8)?;
        if &header[..4] != b"MMMD" {
            return Ok(0);
        }
        let length = u32::from_be_bytes(header[4..8].try_into().unwrap()) + 8;
        if length > 4 << 20 {
            return Ok(0);
        }
        let data = read_bytes(core, address, length)?;

        let previous = self.state.lock().clip.take();
        let mut audio = self.system.audio();
        if let Some(previous) = previous {
            let _ = audio.close(previous);
        }
        match audio.load_smaf(&data) {
            Ok(handle) => {
                self.state.lock().clip = Some(handle);
                Ok(1)
            }
            Err(_) => Ok(0),
        }
    }
}

/// A terminated string of odd length may be the start of text padded to two
/// bytes per character, with a zero byte in the pad; the run is read on to the
/// first all-zero pair, dropping the zeros.
fn padded_text(core: &ArmCore, address: u32, first: Vec<u8>) -> Result<Vec<u8>> {
    if first.len().is_multiple_of(2) {
        return Ok(first);
    }

    let Ok(raw) = read_bytes(core, address, MAX_TEXT as u32) else {
        return Ok(first);
    };

    let mut run = Vec::with_capacity(raw.len());
    for pair in raw.chunks_exact(2) {
        if pair[0] == 0 && pair[1] == 0 {
            break;
        }
        run.extend(pair.iter().copied().filter(|x| *x != 0));
    }

    Ok(run)
}

/// The code the title acts on for each key: the handset's virtual key codes,
/// `AVK_*`.
///
/// The digits are a block from `0xe021`, then star, pound, power, end, send
/// and clear, then the pad - up, down, left, right, select - and the soft
/// keys. The right soft key is the handset's back key, so it is clear.
pub fn key_code(key: wie_backend::KeyCode) -> Option<u32> {
    use wie_backend::KeyCode;

    const DIGIT_BASE: u32 = 0xe021;
    const STAR: u32 = 0xe02b;
    const POUND: u32 = 0xe02c;
    const END: u32 = 0xe02e;
    const SEND: u32 = 0xe02f;
    const CLEAR: u32 = 0xe030;
    const UP: u32 = 0xe031;
    const DOWN: u32 = 0xe032;
    const LEFT: u32 = 0xe033;
    const RIGHT: u32 = 0xe034;
    const SELECT: u32 = 0xe035;
    const SOFT1: u32 = 0xe036;

    Some(match key {
        KeyCode::UP => UP,
        KeyCode::DOWN => DOWN,
        KeyCode::LEFT => LEFT,
        KeyCode::RIGHT => RIGHT,
        KeyCode::OK => SELECT,
        KeyCode::LEFT_SOFT_KEY => SOFT1,
        KeyCode::CLEAR | KeyCode::RIGHT_SOFT_KEY => CLEAR,
        KeyCode::CALL => SEND,
        KeyCode::HANGUP => END,
        KeyCode::NUM0 => DIGIT_BASE,
        KeyCode::NUM1 => DIGIT_BASE + 1,
        KeyCode::NUM2 => DIGIT_BASE + 2,
        KeyCode::NUM3 => DIGIT_BASE + 3,
        KeyCode::NUM4 => DIGIT_BASE + 4,
        KeyCode::NUM5 => DIGIT_BASE + 5,
        KeyCode::NUM6 => DIGIT_BASE + 6,
        KeyCode::NUM7 => DIGIT_BASE + 7,
        KeyCode::NUM8 => DIGIT_BASE + 8,
        KeyCode::NUM9 => DIGIT_BASE + 9,
        KeyCode::STAR => STAR,
        KeyCode::HASH => POUND,
        _ => return None,
    })
}
