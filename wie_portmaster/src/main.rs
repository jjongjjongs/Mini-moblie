//! MiniMobile on SDL2: the PortMaster port for Linux handhelds, and the
//! Windows build.
//!
//! The emulator is the one the Android and iOS apps run, driven through
//! `wie_android::host` the way the iOS app drives it: a tick at a time, the
//! newest frame taken after each, and the mixer pulled from the audio thread.
//! The screen, sound, pad and keyboard are SDL2's (see `sdl`): the handheld's
//! own, or the SDL2.dll the Windows build ships beside it.
//!
//! It opens on a list of the games in the folder it is given (see `library`),
//! their icons in a row to step through sideways, by carrier or by a name
//! searched for, and goes back to it when a game ends. Its own screens are
//! drawn at the screen's resolution in a face of their own (see `ui`); the
//! games keep their handset fonts. The settings (see `settings`) open from the list with
//! Y or Esc, and over a game with MENU or Esc; a game's saves and the game
//! itself are handled from there (see `manage`), and Delete on the list
//! deletes the game picked. Each game plays at its own speed, which F5 and
//! F6 change as it runs.
//!
//! On a desktop - Windows, or `--windowed` - it is a window: 2x to 4x of
//! 320x240 or the full screen (F11), with games dropped on it copied into the
//! games folder. The mouse works the list and the settings as well as the
//! keys do (see `pointer`), and its right button opens the menu over a game.

// The Windows build is a window, not a console program.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod controls;
mod icons;
mod library;
mod manage;
mod pointer;
mod presets;
mod saves;
mod sdl;
mod search;
mod settings;
mod ui;

use std::{
    ffi::{CStr, CString, c_int, c_void},
    path::{Path, PathBuf},
    ptr,
    time::{Duration, Instant},
};

use wie_android::host;

use self::{
    controls::{BACKSPACE, BUTTON_COUNT, Button, DELETE, ESCAPE, F2, F5, F6, F11, TAB},
    library::{Hit, Menu},
    pointer::{Mouse, Pointer},
    presets::Store,
    sdl::{Event, Rect, Sdl},
    settings::{Context, Outcome, Quality, speed_label, speed_step},
};

/// How often the game loop runs a tick when the title does not say otherwise.
const FRAME: Duration = Duration::from_micros(16_667);
/// How far a stick or trigger has to move to count as pressed, and how far
/// back it has to come to count as let go.
const AXIS_PRESS: i16 = 16_000;
const AXIS_RELEASE: i16 = 12_000;
/// How long the hint over a desktop game stays up as it starts.
const TOAST: Duration = Duration::from_secs(3);
/// How long the speed stays up after F5 or F6.
const SPEED_TOAST: Duration = Duration::from_millis(1500);

fn main() {
    // The runner's log goes to stderr, which the launch script keeps in a file
    // on the SD card; its default level writes far too much for that.
    if std::env::var_os("RUST_LOG").is_none() {
        // SAFETY: no other thread exists yet to read the environment.
        unsafe { std::env::set_var("RUST_LOG", "warn") };
    }

    let mut windowed = false;
    let mut target = None;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--windowed" => windowed = true,
            _ => target = Some(PathBuf::from(arg)),
        }
    }

    // Started from Explorer, the Windows build keeps its games, saves and
    // settings beside itself, wherever it was started from.
    #[cfg(windows)]
    if let Some(dir) = std::env::current_exe().ok().as_deref().and_then(Path::parent) {
        let _ = std::env::set_current_dir(dir);
    }

    let target = target.unwrap_or_else(|| PathBuf::from("games"));
    if let Err(error) = run(&target, cfg!(windows) || windowed) {
        report(&error);
        std::process::exit(1);
    }
}

/// A message that stops the program before it has a screen: on stderr, and
/// in a box on Windows, where nobody sees stderr.
fn report(message: &str) {
    eprintln!("{message}");
    #[cfg(windows)]
    {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn MessageBoxW(window: *mut c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
        }
        let wide = |text: &str| text.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
        // SAFETY: two nul-terminated UTF-16 strings and no owner window.
        unsafe { MessageBoxW(ptr::null_mut(), wide(message).as_ptr(), wide("MiniMobile").as_ptr(), 0x10) };
    }
}

fn run(target: &Path, desktop: bool) -> Result<(), String> {
    let sdl = Sdl::load().map_err(|error| {
        if cfg!(windows) {
            format!("{error}\n\nSDL2.dll이 MiniMobile.exe와 같은 폴더에 있어야 합니다.")
        } else {
            error
        }
    })?;
    let mut app = App::new(sdl, desktop)?;

    // A game named directly is played once; a folder is a list to pick from.
    if target.is_file() {
        if let Err(message) = app.play(target) {
            app.message(&message);
        }
    } else {
        let _ = std::fs::create_dir_all(target);
        let mut menu = Menu::new(target, desktop);
        while let Some(game) = app.choose(&mut menu) {
            if let Err(message) = app.play(&game) {
                app.message(&message);
            }
        }
    }

    // SAFETY: nothing of SDL's is used after this.
    unsafe { (app.sdl.quit)() };
    Ok(())
}

/// What one pass over the pending events turned up.
enum Input {
    /// A pad button, pressed or let go.
    Button(Button, bool),
    /// A keyboard key by SDL scancode: pressed or let go, and whether it is the
    /// system repeating a key held down.
    Key(i32, bool, bool),
    /// A file dropped on the window.
    Drop(PathBuf),
    /// Text typed, through the input method on a desktop.
    Text(String),
    /// What the input method is still composing.
    Editing(String),
    /// The mouse, in the pixels of what is on screen.
    Mouse(Mouse),
    /// The window closed.
    Quit,
}

/// The pad button a keyboard key stands for on the list and in the settings:
/// the arrows, Enter or Space to choose, Esc or Backspace to go back, F2 and
/// Delete for the second and third actions a screen offers.
fn key_button(scancode: i32) -> Option<Button> {
    Some(match scancode {
        82 => Button::Up,
        81 => Button::Down,
        80 => Button::Left,
        79 => Button::Right,
        40 | 88 | 44 => Button::A,
        ESCAPE | 42 => Button::B,
        59 => Button::X,
        controls::DELETE => Button::Y,
        _ => return None,
    })
}

struct App {
    sdl: Sdl,
    window: *mut c_void,
    renderer: *mut c_void,
    texture: *mut c_void,
    texture_size: (i32, i32),
    /// Whether the texture is scaled smoothly - a game shown 기본 or HQ2X -
    /// or pixel for pixel, as the settings always are.
    texture_smooth: bool,
    /// A window on a desktop rather than a handheld's whole screen.
    desktop: bool,
    /// The window size to go back to from the full screen.
    last_window: u32,
    /// Whatever is on screen has to be drawn again: the window changed size
    /// or went full screen.
    redraw: bool,
    /// The button mapping and its presets.
    store: Store,
    /// Which of a button's sources - the button itself, a stick or trigger -
    /// hold it down, one bit each.
    held: [u8; BUTTON_COUNT],
    audio: u32,
    /// The mouse, and the parts of the screen on show to it.
    pointer: Pointer,
}

impl App {
    fn new(sdl: Sdl, desktop: bool) -> Result<App, String> {
        let store = Store::load();
        // SAFETY: plain SDL2 calls on the thread that initialised it, with
        // pointers to live, nul-terminated strings.
        unsafe {
            // Whole pixels, as the phone draws them: a title's pixel font goes
            // soft under any filter. And on Windows, the window's real pixels
            // rather than the system stretching a smaller one.
            (sdl.set_hint)(c"SDL_RENDER_SCALE_QUALITY".as_ptr(), c"0".as_ptr());
            (sdl.set_hint)(c"SDL_WINDOWS_DPI_AWARENESS".as_ptr(), c"permonitorv2".as_ptr());

            let flags = sdl::INIT_VIDEO | sdl::INIT_AUDIO | sdl::INIT_JOYSTICK | sdl::INIT_GAMECONTROLLER | sdl::INIT_EVENTS;
            if (sdl.init)(flags) != 0 {
                // Some firmware has no sound device until something else lets
                // go of it; play on without one rather than not at all.
                if (sdl.init)(flags & !sdl::INIT_AUDIO) != 0 {
                    return Err(format!("SDL을 시작할 수 없습니다: {}", error(&sdl)));
                }
            }

            let window_flags = if desktop {
                sdl::WINDOW_SHOWN | sdl::WINDOW_RESIZABLE
            } else {
                sdl::WINDOW_SHOWN | sdl::WINDOW_FULLSCREEN_DESKTOP
            };
            let window = (sdl.create_window)(
                c"MiniMobile".as_ptr(),
                sdl::WINDOWPOS_CENTERED,
                sdl::WINDOWPOS_CENTERED,
                640,
                480,
                window_flags,
            );
            if window.is_null() {
                return Err(format!("화면을 열 수 없습니다: {}", error(&sdl)));
            }
            let mut renderer = (sdl.create_renderer)(window, -1, sdl::RENDERER_ACCELERATED);
            if renderer.is_null() {
                renderer = (sdl.create_renderer)(window, -1, sdl::RENDERER_SOFTWARE);
            }
            if renderer.is_null() {
                return Err(format!("화면에 그릴 수 없습니다: {}", error(&sdl)));
            }
            if !desktop {
                (sdl.show_cursor)(0);
            }

            // A mapping file beside the port, for a pad the firmware's own
            // mappings do not name. SDL_GAMECONTROLLERCONFIG, which PortMaster
            // sets, is read by SDL itself.
            if Path::new("gamecontrollerdb.txt").is_file() {
                let rw = (sdl.rw_from_file)(c"gamecontrollerdb.txt".as_ptr(), c"rb".as_ptr());
                if !rw.is_null() {
                    (sdl.game_controller_add_mappings_from_rw)(rw, 1);
                }
            }

            let audio = open_audio(&sdl);

            let last_window = match store.screen() {
                0 => 3,
                scale => scale,
            };
            let mut app = App {
                sdl,
                window,
                renderer,
                texture: ptr::null_mut(),
                texture_size: (0, 0),
                texture_smooth: false,
                desktop,
                last_window,
                redraw: true,
                store,
                held: [0; BUTTON_COUNT],
                audio,
                pointer: Pointer::default(),
            };
            app.apply_screen();
            for index in 0..(app.sdl.num_joysticks)() {
                app.open_controller(index);
            }
            Ok(app)
        }
    }

    /// Sizes the desktop window as the settings say: a multiple of 320x240,
    /// as large as the screen leaves room for up to that, or the full screen.
    fn apply_screen(&mut self) {
        if !self.desktop {
            return;
        }
        let scale = self.store.screen();
        // SAFETY: SDL2 calls on the window this program made.
        unsafe {
            if scale == 0 {
                (self.sdl.set_window_fullscreen)(self.window, sdl::WINDOW_FULLSCREEN_DESKTOP);
            } else {
                (self.sdl.set_window_fullscreen)(self.window, 0);
                let mut bounds = Rect { x: 0, y: 0, w: 0, h: 0 };
                let mut fitting = scale;
                if (self.sdl.get_display_usable_bounds)(0, &mut bounds) == 0 {
                    // Room for the title bar too.
                    while fitting > 1 && (320 * fitting as i32 > bounds.w || 240 * fitting as i32 + 40 > bounds.h) {
                        fitting -= 1;
                    }
                }
                (self.sdl.set_window_size)(self.window, 320 * fitting as i32, 240 * fitting as i32);
                (self.sdl.set_window_position)(self.window, sdl::WINDOWPOS_CENTERED, sdl::WINDOWPOS_CENTERED);
            }
        }
        self.redraw = true;
    }

    /// F11: the full screen, or back to the window it came from.
    fn toggle_fullscreen(&mut self) {
        let scale = self.store.screen();
        if scale == 0 {
            self.store.set_screen(self.last_window);
        } else {
            self.last_window = scale;
            self.store.set_screen(0);
        }
        self.apply_screen();
    }

    /// Changes the window to `scale` (0 the full screen) from the settings.
    fn set_screen(&mut self, scale: u32) {
        if scale != 0 {
            self.last_window = scale;
        }
        self.store.set_screen(scale);
        self.apply_screen();
    }

    fn set_title(&self, title: &str) {
        let title = CString::new(title.replace('\0', "")).unwrap_or_default();
        // SAFETY: the program's window and a nul-terminated string.
        unsafe { (self.sdl.set_window_title)(self.window, title.as_ptr()) };
    }

    fn open_controller(&mut self, index: c_int) {
        // SAFETY: SDL2 calls with a device index SDL itself reported.
        unsafe {
            if (self.sdl.is_game_controller)(index) == 0 {
                let name = (self.sdl.joystick_name_for_index)(index);
                eprintln!(
                    "패드로 쓸 수 없는 입력 장치입니다 (매핑 없음): {}",
                    if name.is_null() {
                        "?".into()
                    } else {
                        CStr::from_ptr(name).to_string_lossy()
                    }
                );
                return;
            }
            let controller = (self.sdl.game_controller_open)(index);
            if controller.is_null() {
                eprintln!("패드를 열 수 없습니다: {}", error(&self.sdl));
                return;
            }
            let name = (self.sdl.game_controller_name)(controller);
            eprintln!(
                "패드: {}",
                if name.is_null() {
                    "?".into()
                } else {
                    CStr::from_ptr(name).to_string_lossy()
                }
            );
        }
    }

    /// Everything that happened since the last call. F11 is taken here, on
    /// every screen.
    fn poll(&mut self) -> Vec<Input> {
        let mut inputs = Vec::new();
        let mut event = Event::new();
        // SAFETY: `event` is a buffer of SDL_Event's size and alignment.
        while unsafe { (self.sdl.poll_event)(&mut event) } != 0 {
            match event.kind() {
                sdl::QUIT => inputs.push(Input::Quit),
                sdl::WINDOWEVENT => self.redraw = true,
                sdl::DROPFILE => {
                    let file = event.dropped_file();
                    if !file.is_null() {
                        // SAFETY: SDL hands over a nul-terminated path it
                        // allocated, for this program to free.
                        let path = unsafe { CStr::from_ptr(file) }.to_string_lossy().into_owned();
                        unsafe { (self.sdl.free)(file as *mut c_void) };
                        inputs.push(Input::Drop(PathBuf::from(path)));
                    }
                }
                sdl::CONTROLLERDEVICEADDED => self.open_controller(event.which()),
                sdl::CONTROLLERBUTTONDOWN | sdl::CONTROLLERBUTTONUP => {
                    if let Some(button) = Button::from_sdl(event.control()) {
                        self.hold(button, 1, event.kind() == sdl::CONTROLLERBUTTONDOWN, &mut inputs);
                    }
                }
                sdl::CONTROLLERAXISMOTION => self.axis(event.control(), event.axis_value(), &mut inputs),
                sdl::KEYDOWN if event.scancode() == F11 => {
                    if self.desktop && !event.key_repeat() {
                        self.toggle_fullscreen();
                    }
                }
                sdl::KEYDOWN | sdl::KEYUP => inputs.push(Input::Key(event.scancode(), event.kind() == sdl::KEYDOWN, event.key_repeat())),
                sdl::TEXTINPUT => inputs.push(Input::Text(event.text())),
                sdl::TEXTEDITING => inputs.push(Input::Editing(event.text())),
                sdl::MOUSEMOTION => {
                    let (x, y) = self.window_to_canvas(event.mouse_position());
                    inputs.push(Input::Mouse(Mouse::Move(x, y, event.mouse_held() & 1 != 0)));
                }
                sdl::MOUSEBUTTONDOWN => {
                    let (x, y) = self.window_to_canvas(event.mouse_position());
                    match event.mouse_button() {
                        sdl::BUTTON_LEFT => inputs.push(Input::Mouse(Mouse::Press(x, y))),
                        sdl::BUTTON_RIGHT => inputs.push(Input::Mouse(Mouse::Back)),
                        _ => {}
                    }
                }
                sdl::MOUSEBUTTONUP if event.mouse_button() == sdl::BUTTON_LEFT => inputs.push(Input::Mouse(Mouse::Release)),
                sdl::MOUSEWHEEL if event.wheel() != 0 => inputs.push(Input::Mouse(Mouse::Wheel(event.wheel()))),
                _ => {}
            }
        }
        inputs
    }

    /// Sets or clears one source of `button`, reporting the button when that
    /// changes whether it is down at all.
    fn hold(&mut self, button: Button, source: u8, down: bool, inputs: &mut Vec<Input>) {
        let was = self.held[button.index()] != 0;
        if down {
            self.held[button.index()] |= source;
        } else {
            self.held[button.index()] &= !source;
        }
        let is = self.held[button.index()] != 0;
        if was != is {
            inputs.push(Input::Button(button, is));
        }
    }

    /// The left stick as the D-pad, the triggers as L2 and R2.
    fn axis(&mut self, axis: u8, value: i16, inputs: &mut Vec<Input>) {
        let pairs: &[(Button, bool)] = match axis {
            0 => &[(Button::Left, false), (Button::Right, true)],
            1 => &[(Button::Up, false), (Button::Down, true)],
            4 => &[(Button::L2, true)],
            5 => &[(Button::R2, true)],
            _ => return,
        };
        for &(button, positive) in pairs {
            let toward = if positive { value as i32 } else { -(value as i32) };
            let down = self.held[button.index()] & 2 != 0;
            if !down && toward >= AXIS_PRESS as i32 {
                self.hold(button, 2, true, inputs);
            } else if down && toward < AXIS_RELEASE as i32 {
                self.hold(button, 2, false, inputs);
            }
        }
    }

    fn select_held(&self) -> bool {
        self.held[Button::Select.index()] != 0
    }

    /// The size of the screen, in its own pixels.
    fn output_size(&self) -> (i32, i32) {
        let (mut width, mut height) = (0, 0);
        // SAFETY: a live renderer and two out-parameters.
        unsafe { (self.sdl.get_renderer_output_size)(self.renderer, &mut width, &mut height) };
        (width.max(1), height.max(1))
    }

    /// A point in the window as one in the canvas last shown. The window's
    /// points and the screen's pixels differ on a display that scales.
    fn window_to_canvas(&self, (x, y): (i32, i32)) -> (i32, i32) {
        let (mut window_width, mut window_height) = (0, 0);
        // SAFETY: a live window and two out-parameters.
        unsafe { (self.sdl.get_window_size)(self.window, &mut window_width, &mut window_height) };
        let (output_width, output_height) = self.output_size();
        let x = if window_width > 0 { x * output_width / window_width } else { x };
        let y = if window_height > 0 { y * output_height / window_height } else { y };
        self.pointer.shown.to_canvas(x, y)
    }

    /// Shows one of the port's own screens: `rgba` drawn at the screen's scale
    /// (see `ui`) for a layout `width` by `height` units, pixel for pixel. The
    /// mouse is put back into the layout's units.
    fn show(&mut self, rgba: &[u8], width: u32, height: u32) {
        let scale = ui::scale();
        self.show_scaled(rgba, width * scale, height * scale, false);
        self.pointer.shown.width = width;
        self.pointer.shown.height = height;
    }

    /// [`Self::show`], scaled smoothly when `smooth`.
    fn show_scaled(&mut self, rgba: &[u8], width: u32, height: u32, smooth: bool) {
        let (width, height) = (width as i32, height as i32);
        if width <= 0 || height <= 0 || rgba.len() < (width * height * 4) as usize {
            return;
        }
        let (screen_width, screen_height) = self.output_size();
        let scale = (screen_width as f32 / width as f32).min(screen_height as f32 / height as f32);
        let (shown_width, shown_height) = ((width as f32 * scale) as i32, (height as f32 * scale) as i32);
        let target = Rect {
            x: (screen_width - shown_width) / 2,
            y: (screen_height - shown_height) / 2,
            w: shown_width,
            h: shown_height,
        };
        self.pointer.shown = pointer::Shown {
            width: width as u32,
            height: height as u32,
            x: target.x,
            y: target.y,
            shown_width: target.w,
            shown_height: target.h,
        };

        // SAFETY: SDL2 calls on a live renderer, with a texture made for it and
        // a pixel buffer at least `height` rows of `width * 4` bytes.
        unsafe {
            if self.texture.is_null() || self.texture_size != (width, height) || self.texture_smooth != smooth {
                if !self.texture.is_null() {
                    (self.sdl.destroy_texture)(self.texture);
                }
                // How a texture is scaled is settled as it is made, by the hint
                // standing then - which every SDL2 reads.
                let quality = if smooth { c"1" } else { c"0" };
                (self.sdl.set_hint)(c"SDL_RENDER_SCALE_QUALITY".as_ptr(), quality.as_ptr());
                self.texture = (self.sdl.create_texture)(self.renderer, sdl::PIXELFORMAT_ABGR8888, sdl::TEXTUREACCESS_STREAMING, width, height);
                self.texture_size = (width, height);
                self.texture_smooth = smooth;
                if self.texture.is_null() {
                    eprintln!("화면 버퍼를 만들 수 없습니다: {}", error(&self.sdl));
                    return;
                }
            }
            (self.sdl.update_texture)(self.texture, ptr::null(), rgba.as_ptr() as *const c_void, width * 4);
            (self.sdl.set_render_draw_color)(self.renderer, 0, 0, 0, 255);
            (self.sdl.render_clear)(self.renderer);
            (self.sdl.render_copy)(self.renderer, self.texture, ptr::null(), &target);
            (self.sdl.render_present)(self.renderer);
        }
    }

    /// The pixel size the list and settings are drawn at: the screen divided
    /// by the largest whole number that leaves at least 320 by 240, so the
    /// 16-pixel font lands on whole pixels and every screen has its room.
    fn menu_size(&self) -> (u32, u32, u32) {
        let (width, height) = self.output_size();
        let scale = (height / 240).min(width / 320).max(1);
        ui::set_scale(scale as u32);
        ((width / scale) as u32, (height / scale) as u32, scale as u32)
    }

    /// Turns the desktop's text input - and with it the input method - on for
    /// the search, or off, so it keeps out of a game's keys.
    fn text_input(&self, on: bool) {
        if !self.desktop {
            return;
        }
        // SAFETY: plain SDL2 calls with no arguments.
        unsafe {
            if on {
                (self.sdl.start_text_input)();
            } else {
                (self.sdl.stop_text_input)();
            }
        }
    }

    /// Shows the list until a game is picked, or `None` when the player quits.
    fn choose(&mut self, menu: &mut Menu) -> Option<PathBuf> {
        menu.refresh();
        menu.set_played(self.store.played_all());
        self.set_title("MiniMobile");
        self.text_input(true);
        let picked = self.choose_loop(menu);
        self.text_input(false);
        picked
    }

    fn choose_loop(&mut self, menu: &mut Menu) -> Option<PathBuf> {
        let mut dirty = true;
        let mut repeat: Option<(Button, Instant)> = None;

        loop {
            for input in self.poll() {
                dirty = true;
                let button = match input {
                    Input::Quit => return None,
                    Input::Drop(path) => {
                        menu.add(&path);
                        continue;
                    }
                    // Typing on the list starts a search with it, through the
                    // input method on a desktop; a space alone does not.
                    Input::Text(text) => {
                        if menu.searching() {
                            menu.type_text(&text);
                        } else if !text.trim().is_empty() {
                            menu.open_search(false);
                            menu.type_text(&text);
                        }
                        continue;
                    }
                    Input::Editing(text) => {
                        if !text.is_empty() {
                            menu.open_search(false);
                        }
                        menu.set_editing(&text);
                        continue;
                    }
                    Input::Mouse(Mouse::Back) if menu.searching() => {
                        menu.close_search();
                        continue;
                    }
                    // Esc opens the menu, as Y does - and the mouse's right
                    // button.
                    Input::Mouse(Mouse::Back) => Button::Y,
                    Input::Mouse(Mouse::Move(..) | Mouse::Release) => continue,
                    Input::Mouse(Mouse::Press(x, y)) => {
                        match menu.hit_at(x, y) {
                            Some(Hit::Game(index)) => {
                                // The game in the middle plays; one beside it
                                // comes to the middle.
                                if !menu.select(index)
                                    && let Some(game) = menu.selected()
                                {
                                    return Some(game);
                                }
                            }
                            Some(Hit::Tab(tab)) => {
                                menu.close_search();
                                menu.set_tab(tab);
                            }
                            Some(Hit::Search) => menu.open_search(!self.desktop),
                            Some(Hit::Key(index)) => menu.pad_press(Some(index)),
                            Some(Hit::Folder) => open_folder(menu.folder()),
                            None => {}
                        }
                        continue;
                    }
                    Input::Mouse(Mouse::Wheel(turn)) => {
                        let button = if turn > 0 { Button::Left } else { Button::Right };
                        for _ in 0..turn.unsigned_abs().min(3) {
                            menu.navigate(button);
                        }
                        continue;
                    }
                    // A search typed on the keyboard takes its keys first.
                    Input::Key(code, true, _) if menu.searching() && !menu.pad_keyboard() => {
                        match code {
                            ESCAPE | TAB => menu.close_search(),
                            BACKSPACE => menu.erase(),
                            40 | 88 => {
                                if let Some(game) = menu.selected() {
                                    return Some(game);
                                }
                            }
                            82 | 80 => {
                                menu.navigate(Button::Left);
                            }
                            81 | 79 => {
                                menu.navigate(Button::Right);
                            }
                            _ => {}
                        }
                        continue;
                    }
                    Input::Key(TAB, true, false) => {
                        menu.open_search(false);
                        continue;
                    }
                    Input::Key(F2, true, false) if self.desktop => {
                        open_folder(menu.folder());
                        continue;
                    }
                    Input::Key(ESCAPE, true, false) => Button::Y,
                    // Delete asks to delete the game picked.
                    Input::Key(DELETE, true, false) => {
                        if let Some(game) = menu.selected() {
                            let (width, height, scale) = self.menu_size();
                            let back = menu.draw(width, height);
                            if let Some(done) = self.delete_game(&game, Some((width * scale, height * scale, &back))) {
                                menu.refresh();
                                menu.set_status(done);
                            }
                        }
                        continue;
                    }
                    Input::Key(code, true, repeated) => match key_button(code) {
                        Some(button) if !repeated || matches!(button, Button::Up | Button::Down | Button::Left | Button::Right) => button,
                        _ => continue,
                    },
                    Input::Key(..) => continue,
                    Input::Button(Button::Start, true) if self.select_held() => return None,
                    Input::Button(button, true) => {
                        repeat = Some((button, Instant::now() + Duration::from_millis(400)));
                        button
                    }
                    Input::Button(button, false) => {
                        if repeat.is_some_and(|(held, _)| held == button) {
                            repeat = None;
                        }
                        continue;
                    }
                };

                // The pad's keyboard, while it is up.
                if menu.pad_keyboard() {
                    match button {
                        Button::Up => menu.pad_move(0, -1),
                        Button::Down => menu.pad_move(0, 1),
                        Button::Left => menu.pad_move(-1, 0),
                        Button::Right => menu.pad_move(1, 0),
                        Button::A => menu.pad_press(None),
                        Button::B => menu.erase(),
                        Button::X => menu.close_search(),
                        Button::L1 | Button::L2 => {
                            menu.navigate(Button::Left);
                        }
                        Button::R1 | Button::R2 => {
                            menu.navigate(Button::Right);
                        }
                        Button::Start => {
                            if let Some(game) = menu.selected() {
                                return Some(game);
                            }
                        }
                        Button::Guide => menu.close_search(),
                        _ => {}
                    }
                    continue;
                }

                match button {
                    Button::A | Button::Start => {
                        if let Some(game) = menu.selected() {
                            return Some(game);
                        }
                    }
                    Button::B if menu.searching() => menu.close_search(),
                    Button::X => menu.open_search(true),
                    Button::Y if menu.searching() => menu.close_search(),
                    Button::Y => {
                        repeat = None;
                        let game = menu.selected();
                        let outcome = self.settings_menu(&Context {
                            game: game.as_deref(),
                            frame: None,
                        });
                        match outcome {
                            Outcome::QuitApp => return None,
                            Outcome::Deleted(done) => {
                                menu.refresh();
                                menu.set_status(done);
                            }
                            Outcome::Close | Outcome::EndGame => {}
                        }
                    }
                    _ => {
                        menu.navigate(button);
                    }
                }
            }

            // A direction held down keeps moving.
            if let Some((button, at)) = repeat
                && Instant::now() >= at
            {
                match (menu.pad_keyboard(), button) {
                    (true, Button::Up) => menu.pad_move(0, -1),
                    (true, Button::Down) => menu.pad_move(0, 1),
                    (true, Button::Left) => menu.pad_move(-1, 0),
                    (true, Button::Right) => menu.pad_move(1, 0),
                    (true, _) => {}
                    (false, _) => {
                        menu.navigate(button);
                    }
                }
                dirty = true;
                repeat = Some((button, Instant::now() + Duration::from_millis(90)));
            }

            if menu.poll() || dirty || std::mem::take(&mut self.redraw) {
                let (width, height, _) = self.menu_size();
                let rgba = menu.draw(width, height);
                self.show(&rgba, width, height);
                dirty = false;
            }
            std::thread::sleep(FRAME);
        }
    }

    /// Shows `text` until it is dismissed.
    fn message(&mut self, text: &str) {
        let hint = if self.desktop { "Enter 확인" } else { "A 확인" };
        let mut shown = false;
        loop {
            for input in self.poll() {
                match input {
                    Input::Quit | Input::Button(Button::A | Button::B | Button::Start, true) => return,
                    Input::Mouse(Mouse::Press(..) | Mouse::Back) => return,
                    Input::Key(code, true, false) if matches!(key_button(code), Some(Button::A | Button::B)) => return,
                    _ => {}
                }
            }
            if !shown || std::mem::take(&mut self.redraw) {
                let (width, height, _) = self.menu_size();
                let rgba = library::draw_message(text, hint, width, height);
                self.show(&rgba, width, height);
                shown = true;
            }
            std::thread::sleep(FRAME);
        }
    }

    /// Plays `game` until it ends or the player leaves it. The message that
    /// stopped it, if it stopped on its own.
    fn play(&mut self, game: &Path) -> Result<(), String> {
        let name = game.file_stem().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
        let (width, height, _) = self.menu_size();
        let rgba = library::draw_message(&format!("{name}\n\n불러오는 중…"), "", width, height);
        self.show(&rgba, width, height);
        let speed = self.game_speed(game);
        self.set_title(&game_title(&name, speed, false));
        if let Some(file) = game.file_name() {
            self.store.set_played(&file.to_string_lossy());
        }

        let data = std::fs::read(game).map_err(|error| format!("게임 파일을 읽을 수 없습니다: {error}"))?;
        // A game fixed to a preset plays with it.
        let fixed = game
            .file_name()
            .and_then(|file| self.store.fixed(&file.to_string_lossy()).map(str::to_owned));
        if let Some(preset) = fixed {
            self.store.apply(&preset);
        }
        let runtime_dir = saves::runtime_dir();
        let _ = std::fs::create_dir_all(&runtime_dir);
        // The speed it was last played at; starting puts the clock back on the
        // time of day and runs it from there at that speed.
        host::set_speed(speed);
        let failure = host::start(data, runtime_dir, "Linux".to_owned());
        if !failure.is_empty() {
            return Err(failure);
        }
        // SAFETY: a device id SDL handed out, or 0, which SDL ignores.
        unsafe { (self.sdl.pause_audio_device)(self.audio, 0) };

        let result = self.run_game(game, &name);

        host::stop();
        // SAFETY: as above.
        unsafe { (self.sdl.pause_audio_device)(self.audio, 1) };
        if let Err(error) = std::fs::write("last_game_log.txt", host::log()) {
            eprintln!("로그를 쓸 수 없습니다: {error}");
        }
        self.set_title("MiniMobile");
        result
    }

    /// The menu over a game, with the game and its clock and sound held still
    /// behind it.
    fn pause(&mut self, game: &Path, name: &str, frame: Option<&(u32, u32, Vec<u8>)>) -> Outcome {
        host::hold_clock(true);
        // SAFETY: a device id SDL handed out, or 0, which SDL ignores.
        unsafe { (self.sdl.pause_audio_device)(self.audio, 1) };
        self.set_title(&game_title(name, host::speed(), true));
        let outcome = self.settings_menu(&Context {
            game: Some(game),
            frame: frame.map(|(width, height, rgba)| (*width, *height, rgba.as_slice())),
        });
        self.set_title(&game_title(name, host::speed(), false));
        // SAFETY: as above.
        unsafe { (self.sdl.pause_audio_device)(self.audio, 0) };
        host::hold_clock(false);
        outcome
    }

    fn run_game(&mut self, game: &Path, name: &str) -> Result<(), String> {
        // The handset key each button pressed, so letting it go releases that
        // key even if SELECT was let go in between; and each keyboard key's.
        let mut pressed: [Option<i32>; BUTTON_COUNT] = [None; BUTTON_COUNT];
        let mut keys: Vec<(i32, i32)> = Vec::new();
        let release_all = |pressed: &mut [Option<i32>; BUTTON_COUNT], keys: &mut Vec<(i32, i32)>| {
            for key in pressed.iter_mut().filter_map(Option::take) {
                host::key(key, false);
            }
            for (_, key) in keys.drain(..) {
                host::key(key, false);
            }
        };
        // The newest frame, kept for the menu to show behind it.
        let mut frame: Option<(u32, u32, Vec<u8>)> = None;
        self.clear_hits();
        let mut toast = if self.desktop { "Esc·우클릭 메뉴 · F11 전체화면" } else { "" }.to_owned();
        // When the hint goes, counted from the title's first frame - a title
        // can take a while to draw one.
        let mut toast_until: Option<Instant> = None;
        // Whether the frame on screen carries the hint, to draw it again
        // without once the hint's time is up.
        let mut toast_up = false;

        loop {
            let tick_started = Instant::now();

            for input in self.poll() {
                let pause = match input {
                    Input::Quit => {
                        release_all(&mut pressed, &mut keys);
                        host::stop();
                        std::process::exit(0);
                    }
                    Input::Drop(_) | Input::Text(_) | Input::Editing(_) => false,
                    // Esc on the keyboard, MENU on the pad, the mouse's right
                    // button: the menu.
                    Input::Key(ESCAPE, true, false) | Input::Button(Button::Guide, true) | Input::Mouse(Mouse::Back) => true,
                    Input::Mouse(_) => false,
                    Input::Key(_, _, true) => false,
                    // F5 slower, F6 faster, remembered for the game.
                    Input::Key(code @ (F5 | F6), true, false) => {
                        let speed = speed_step(host::speed(), code == F6);
                        self.set_game_speed(game, speed, true);
                        self.set_title(&game_title(name, speed, false));
                        toast = format!("배속 {}", speed_label(speed));
                        toast_until = Some(Instant::now() + SPEED_TOAST);
                        self.redraw = true;
                        false
                    }
                    Input::Key(code, true, false) => {
                        if let Some(key) = self.store.controls().keyboard_key(code)
                            && !keys.iter().any(|(held, _)| *held == code)
                        {
                            host::key(key, true);
                            keys.push((code, key));
                        }
                        false
                    }
                    Input::Key(code, false, false) => {
                        if let Some(at) = keys.iter().position(|(held, _)| *held == code) {
                            host::key(keys.remove(at).1, false);
                        }
                        false
                    }
                    // SELECT+START: straight back to the list.
                    Input::Button(Button::Start, true) if self.select_held() => {
                        release_all(&mut pressed, &mut keys);
                        return Ok(());
                    }
                    Input::Button(button, true) => {
                        if let Some(key) = self.store.controls().key(button, self.select_held()) {
                            host::key(key, true);
                            pressed[button.index()] = Some(key);
                        }
                        false
                    }
                    Input::Button(button, false) => {
                        if let Some(key) = pressed[button.index()].take() {
                            host::key(key, false);
                        }
                        false
                    }
                };

                if pause {
                    release_all(&mut pressed, &mut keys);
                    if let Outcome::EndGame | Outcome::QuitApp = self.pause(game, name, frame.as_ref()) {
                        return Ok(());
                    }
                    // Nothing on the game's screen answers the mouse.
                    self.clear_hits();
                    self.redraw = true;
                }
            }

            let failure = host::tick(16);
            if !failure.is_empty() {
                return Err(failure);
            }
            // Doubled through hq2x as it is taken when the game is set to it.
            let quality = self.game_quality(game);
            let taken = if quality == Quality::Hq2x {
                host::take_frame_rgba_hq2x()
            } else {
                host::take_frame_rgba()
            };
            if let Some(new_frame) = taken {
                frame = Some(new_frame);
                self.redraw = true;
                toast_until.get_or_insert_with(|| Instant::now() + TOAST);
            }
            let toast_time = toast_until.is_some_and(|until| Instant::now() < until);
            if toast_up && !toast_time {
                toast_up = false;
                self.redraw = true;
            }
            if std::mem::take(&mut self.redraw)
                && let Some((width, height, rgba)) = &frame
            {
                let (width, height) = (*width, *height);
                // The hint over the first seconds of a desktop game.
                toast_up = !toast.is_empty() && toast_time;
                let rgba = if toast_up {
                    ui::with_toast(rgba, width, height, &toast)
                } else {
                    rgba.clone()
                };
                self.show_scaled(&rgba, width, height, quality != Quality::Dot);
            }
            // Vibration: nothing on a handheld to give it to, but the queue
            // still has to be emptied.
            while host::take_output().is_some() {}
            if !host::running() {
                let error = host::last_error();
                return Err(if error.is_empty() {
                    "게임이 종료되었습니다.".to_owned()
                } else {
                    error
                });
            }

            // Sleep out the rest of the frame, or less when the title's next
            // timer comes sooner.
            let target = host::sleep_hint_ms().map_or(FRAME, |hint| Duration::from_millis(hint).min(FRAME));
            if let Some(rest) = target.checked_sub(tick_started.elapsed()) {
                std::thread::sleep(rest);
            }
        }
    }
}

/// The window's title over a game: its name, its speed when that is not 1x,
/// and whether it is paused.
fn game_title(name: &str, speed: f32, paused: bool) -> String {
    let mut title = format!("MiniMobile - {name}");
    if speed != 1.0 {
        title.push_str(&format!(" ({})", speed_label(speed)));
    }
    if paused {
        title.push_str(" (일시정지)");
    }
    title
}

/// Opens `folder` in the desktop's file manager.
fn open_folder(folder: &Path) {
    let program = if cfg!(windows) { "explorer" } else { "xdg-open" };
    if let Err(error) = std::process::Command::new(program).arg(folder).spawn() {
        eprintln!("폴더를 열 수 없습니다: {error}");
    }
}

/// Opens the sound device, playing the runner's mixer. 0 when there is none.
fn open_audio(sdl: &Sdl) -> u32 {
    let desired = sdl::AudioSpec {
        freq: 44_100,
        format: sdl::AUDIO_S16LSB,
        channels: 2,
        silence: 0,
        samples: 1024,
        padding: 0,
        size: 0,
        callback: Some(render_audio),
        userdata: ptr::null_mut(),
    };
    let mut obtained = sdl::AudioSpec { callback: None, ..desired };
    // SAFETY: `desired` asks for exactly the format `render_audio` writes, and
    // no change is allowed, so SDL converts whatever the device takes.
    let device = unsafe { (sdl.open_audio_device)(ptr::null(), 0, &desired, &mut obtained, 0) };
    if device == 0 {
        eprintln!("소리 장치를 열 수 없습니다: {}", error(sdl));
    }
    device
}

/// SDL's audio thread asking for `len` bytes of 44.1kHz stereo sixteen-bit
/// sound: the runner's mixer renders them, and silence fills what it does not.
unsafe extern "C" fn render_audio(_userdata: *mut c_void, stream: *mut u8, len: c_int) {
    let len = len.max(0) as usize;
    // SAFETY: SDL hands over a buffer of `len` bytes to fill.
    let out = unsafe { std::slice::from_raw_parts_mut(stream, len) };
    let rendered = host::render_audio(len / 4);
    let count = rendered.len().min(len);
    out[..count].copy_from_slice(&rendered[..count]);
    out[count..].fill(0);
}

fn error(sdl: &Sdl) -> String {
    // SAFETY: SDL_GetError always returns a nul-terminated string.
    unsafe { CStr::from_ptr((sdl.get_error)()).to_string_lossy().into_owned() }
}
