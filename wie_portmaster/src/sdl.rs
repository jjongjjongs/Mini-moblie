//! The SDL2 calls the port makes, looked up in the handheld's own SDL2 when it
//! starts.
//!
//! Each firmware ships an SDL2 built for its own display (KMS/DRM, Mali fbdev,
//! a Wayland compositor) and pad, so the port uses that one rather than
//! bringing its own. Opening it at run time also keeps the build free of any
//! SDL2 to link against, which is what lets one cross-compiled binary serve
//! every firmware.
//!
//! Only the SDL2 2.0 ABI is assumed, which every SDL2 since 2.0.0 keeps.

use std::ffi::{c_char, c_int, c_void};

use libloading::Library;

pub const INIT_AUDIO: u32 = 0x0000_0010;
pub const INIT_VIDEO: u32 = 0x0000_0020;
pub const INIT_JOYSTICK: u32 = 0x0000_0200;
pub const INIT_GAMECONTROLLER: u32 = 0x0000_2000;
pub const INIT_EVENTS: u32 = 0x0000_4000;

pub const WINDOWPOS_CENTERED: c_int = 0x2FFF_0000;
pub const WINDOW_FULLSCREEN_DESKTOP: u32 = 0x0000_1001;
pub const WINDOW_SHOWN: u32 = 0x0000_0004;
pub const WINDOW_RESIZABLE: u32 = 0x0000_0020;

pub const RENDERER_SOFTWARE: u32 = 0x0000_0001;
pub const RENDERER_ACCELERATED: u32 = 0x0000_0002;

/// Four bytes a pixel in R, G, B, A order, as the runner hands frames over.
pub const PIXELFORMAT_ABGR8888: u32 = 0x1676_2004;
pub const TEXTUREACCESS_STREAMING: c_int = 1;

pub const AUDIO_S16LSB: u16 = 0x8010;

pub const QUIT: u32 = 0x100;
pub const WINDOWEVENT: u32 = 0x200;
pub const DROPFILE: u32 = 0x1000;
pub const KEYDOWN: u32 = 0x300;
pub const KEYUP: u32 = 0x301;
pub const CONTROLLERAXISMOTION: u32 = 0x650;
pub const CONTROLLERBUTTONDOWN: u32 = 0x651;
pub const CONTROLLERBUTTONUP: u32 = 0x652;
pub const CONTROLLERDEVICEADDED: u32 = 0x653;

#[repr(C)]
pub struct Rect {
    pub x: c_int,
    pub y: c_int,
    pub w: c_int,
    pub h: c_int,
}

/// `SDL_Event`: a 56-byte union, read here by the offsets of the members the
/// port looks at.
#[repr(C, align(8))]
pub struct Event {
    raw: [u8; 56],
}

impl Event {
    pub fn new() -> Self {
        Self { raw: [0; 56] }
    }

    pub fn kind(&self) -> u32 {
        self.u32_at(0)
    }

    fn u32_at(&self, offset: usize) -> u32 {
        u32::from_ne_bytes(self.raw[offset..offset + 4].try_into().unwrap())
    }

    fn i32_at(&self, offset: usize) -> i32 {
        i32::from_ne_bytes(self.raw[offset..offset + 4].try_into().unwrap())
    }

    /// `SDL_DropEvent.file`: a path SDL allocated, for the caller to free.
    pub fn dropped_file(&self) -> *mut c_char {
        usize::from_ne_bytes(self.raw[8..8 + size_of::<usize>()].try_into().unwrap()) as *mut c_char
    }

    /// `SDL_KeyboardEvent.repeat`.
    pub fn key_repeat(&self) -> bool {
        self.raw[13] != 0
    }

    /// `SDL_KeyboardEvent.keysym.scancode`.
    pub fn scancode(&self) -> i32 {
        self.i32_at(16)
    }

    /// `which` of a controller or joystick event: the device index for an
    /// added device, the instance id otherwise.
    pub fn which(&self) -> i32 {
        self.i32_at(8)
    }

    /// `SDL_ControllerButtonEvent.button` / `SDL_ControllerAxisEvent.axis`.
    pub fn control(&self) -> u8 {
        self.raw[12]
    }

    /// `SDL_ControllerAxisEvent.value`.
    pub fn axis_value(&self) -> i16 {
        i16::from_ne_bytes(self.raw[16..18].try_into().unwrap())
    }
}

pub type AudioCallback = unsafe extern "C" fn(userdata: *mut c_void, stream: *mut u8, len: c_int);

#[repr(C)]
pub struct AudioSpec {
    pub freq: c_int,
    pub format: u16,
    pub channels: u8,
    pub silence: u8,
    pub samples: u16,
    pub padding: u16,
    pub size: u32,
    pub callback: Option<AudioCallback>,
    pub userdata: *mut c_void,
}

macro_rules! functions {
    ($($field:ident = $name:literal: fn($($arg:ty),*) $(-> $ret:ty)?;)*) => {
        /// The opened library and the functions taken from it.
        pub struct Sdl {
            _library: Library,
            $(pub $field: unsafe extern "C" fn($($arg),*) $(-> $ret)?,)*
        }

        impl Sdl {
            /// Opens the system's SDL2 and finds every function the port uses.
            pub fn load() -> Result<Self, String> {
                let library = open()?;
                // SAFETY: each symbol is the SDL2 function of that name, whose
                // C signature is the one it is given here, and the library
                // stays open for as long as the pointers are kept.
                unsafe {
                    $(
                        let $field = *library
                            .get::<unsafe extern "C" fn($($arg),*) $(-> $ret)?>(concat!($name, "\0").as_bytes())
                            .map_err(|error| format!("SDL2에 {}가 없습니다: {error}", $name))?;
                    )*
                    Ok(Self { _library: library, $($field,)* })
                }
            }
        }
    };
}

functions! {
    init = "SDL_Init": fn(u32) -> c_int;
    quit = "SDL_Quit": fn();
    get_error = "SDL_GetError": fn() -> *const c_char;
    set_hint = "SDL_SetHint": fn(*const c_char, *const c_char) -> c_int;
    show_cursor = "SDL_ShowCursor": fn(c_int) -> c_int;
    create_window = "SDL_CreateWindow": fn(*const c_char, c_int, c_int, c_int, c_int, u32) -> *mut c_void;
    create_renderer = "SDL_CreateRenderer": fn(*mut c_void, c_int, u32) -> *mut c_void;
    set_window_title = "SDL_SetWindowTitle": fn(*mut c_void, *const c_char);
    set_window_fullscreen = "SDL_SetWindowFullscreen": fn(*mut c_void, u32) -> c_int;
    set_window_size = "SDL_SetWindowSize": fn(*mut c_void, c_int, c_int);
    set_window_position = "SDL_SetWindowPosition": fn(*mut c_void, c_int, c_int);
    get_display_usable_bounds = "SDL_GetDisplayUsableBounds": fn(c_int, *mut Rect) -> c_int;
    free = "SDL_free": fn(*mut c_void);
    get_renderer_output_size = "SDL_GetRendererOutputSize": fn(*mut c_void, *mut c_int, *mut c_int) -> c_int;
    create_texture = "SDL_CreateTexture": fn(*mut c_void, u32, c_int, c_int, c_int) -> *mut c_void;
    destroy_texture = "SDL_DestroyTexture": fn(*mut c_void);
    update_texture = "SDL_UpdateTexture": fn(*mut c_void, *const Rect, *const c_void, c_int) -> c_int;
    set_render_draw_color = "SDL_SetRenderDrawColor": fn(*mut c_void, u8, u8, u8, u8) -> c_int;
    render_clear = "SDL_RenderClear": fn(*mut c_void) -> c_int;
    render_copy = "SDL_RenderCopy": fn(*mut c_void, *mut c_void, *const Rect, *const Rect) -> c_int;
    render_present = "SDL_RenderPresent": fn(*mut c_void);
    poll_event = "SDL_PollEvent": fn(*mut Event) -> c_int;
    num_joysticks = "SDL_NumJoysticks": fn() -> c_int;
    is_game_controller = "SDL_IsGameController": fn(c_int) -> c_int;
    joystick_name_for_index = "SDL_JoystickNameForIndex": fn(c_int) -> *const c_char;
    game_controller_open = "SDL_GameControllerOpen": fn(c_int) -> *mut c_void;
    game_controller_name = "SDL_GameControllerName": fn(*mut c_void) -> *const c_char;
    game_controller_add_mappings_from_rw = "SDL_GameControllerAddMappingsFromRW": fn(*mut c_void, c_int) -> c_int;
    rw_from_file = "SDL_RWFromFile": fn(*const c_char, *const c_char) -> *mut c_void;
    open_audio_device = "SDL_OpenAudioDevice": fn(*const c_char, c_int, *const AudioSpec, *mut AudioSpec, c_int) -> u32;
    pause_audio_device = "SDL_PauseAudioDevice": fn(u32, c_int);
}

fn open() -> Result<Library, String> {
    let mut failures = Vec::new();
    // The Windows build ships SDL2.dll beside the program.
    let names: &[&str] = if cfg!(windows) {
        &["SDL2.dll"]
    } else {
        &["libSDL2-2.0.so.0", "libSDL2-2.0.so", "libSDL2.so"]
    };
    for name in names {
        // SAFETY: SDL2's initialisers do nothing a process cannot take.
        match unsafe { Library::new(*name) } {
            Ok(library) => return Ok(library),
            Err(error) => failures.push(format!("{name}: {error}")),
        }
    }
    Err(format!("SDL2를 찾을 수 없습니다 ({})", failures.join(", ")))
}
