//! MiniMobile for Linux handhelds, as a PortMaster port.
//!
//! The emulator is the one the Android and iOS apps run, driven through
//! `wie_android::host` the way the iOS app drives it: a tick at a time, the
//! newest frame taken after each, and the mixer pulled from the audio thread.
//! The screen, sound and pad are the handheld's own SDL2 (see `sdl`).
//!
//! It opens on a list of the games in the folder it is given and goes back to
//! it when a game ends or SELECT+START is pressed. Y on the list and MENU in a
//! game open the button settings (see `settings`).

mod controls;
mod library;
mod presets;
mod sdl;
mod settings;

use std::{
    ffi::{CStr, c_int, c_void},
    path::{Path, PathBuf},
    ptr,
    time::{Duration, Instant},
};

use wie_android::host;

use self::{
    controls::{BUTTON_COUNT, Button},
    library::Menu,
    presets::Store,
    sdl::{Event, Rect, Sdl},
    settings::{Context, Outcome},
};

/// How often the game loop runs a tick when the title does not say otherwise.
const FRAME: Duration = Duration::from_micros(16_667);
/// How far a stick or trigger has to move to count as pressed, and how far
/// back it has to come to count as let go.
const AXIS_PRESS: i16 = 16_000;
const AXIS_RELEASE: i16 = 12_000;

fn main() {
    // The runner's log goes to stderr, which the launch script keeps in a file
    // on the SD card; its default level writes far too much for that.
    if std::env::var_os("RUST_LOG").is_none() {
        // SAFETY: no other thread exists yet to read the environment.
        unsafe { std::env::set_var("RUST_LOG", "warn") };
    }

    let mut args = std::env::args().skip(1);
    let mut windowed = false;
    let mut target = None;
    for arg in args.by_ref() {
        match arg.as_str() {
            "--windowed" => windowed = true,
            _ => target = Some(PathBuf::from(arg)),
        }
    }
    let target = target.unwrap_or_else(|| PathBuf::from("games"));

    if let Err(error) = run(&target, windowed) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run(target: &Path, windowed: bool) -> Result<(), String> {
    let sdl = Sdl::load()?;
    let mut app = App::new(sdl, windowed)?;

    // A game named directly is played once; a folder is a list to pick from.
    if target.is_file() {
        if let Err(message) = app.play(target) {
            app.message(&message);
        }
    } else {
        let _ = std::fs::create_dir_all(target);
        let mut menu = Menu::new(target);
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
    /// A handheld button, pressed or let go.
    Button(Button, bool),
    /// A keyboard key that stands for a handset key directly, pressed or let go.
    Handset(i32, bool),
    /// The keyboard's escape, or the window closed: leave what is showing.
    Back,
    Quit,
}

struct App {
    sdl: Sdl,
    renderer: *mut c_void,
    texture: *mut c_void,
    texture_size: (i32, i32),
    /// The button mapping and its presets.
    store: Store,
    /// Which of a button's sources - the button itself, a stick or trigger -
    /// hold it down, one bit each.
    held: [u8; BUTTON_COUNT],
    controllers: usize,
    audio: u32,
}

impl App {
    fn new(sdl: Sdl, windowed: bool) -> Result<App, String> {
        // SAFETY: plain SDL2 calls on the thread that initialised it, with
        // pointers to live, nul-terminated strings.
        unsafe {
            let flags = sdl::INIT_VIDEO | sdl::INIT_AUDIO | sdl::INIT_JOYSTICK | sdl::INIT_GAMECONTROLLER | sdl::INIT_EVENTS;
            if (sdl.init)(flags) != 0 {
                // Some firmware has no sound device until something else lets
                // go of it; play on without one rather than not at all.
                if (sdl.init)(flags & !sdl::INIT_AUDIO) != 0 {
                    return Err(format!("SDL을 시작할 수 없습니다: {}", error(&sdl)));
                }
            }

            // Whole pixels, as the phone draws them: a title's pixel font goes
            // soft under any filter.
            (sdl.set_hint)(c"SDL_RENDER_SCALE_QUALITY".as_ptr(), c"0".as_ptr());

            let window_flags = if windowed {
                sdl::WINDOW_SHOWN | sdl::WINDOW_RESIZABLE
            } else {
                sdl::WINDOW_SHOWN | sdl::WINDOW_FULLSCREEN_DESKTOP
            };
            let window = (sdl.create_window)(
                c"MiniMobile".as_ptr(),
                sdl::WINDOWPOS_UNDEFINED,
                sdl::WINDOWPOS_UNDEFINED,
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
            (sdl.show_cursor)(0);

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

            let mut app = App {
                sdl,
                renderer,
                texture: ptr::null_mut(),
                texture_size: (0, 0),
                store: Store::load(),
                held: [0; BUTTON_COUNT],
                controllers: 0,
                audio,
            };
            for index in 0..(app.sdl.num_joysticks)() {
                app.open_controller(index);
            }
            Ok(app)
        }
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
            self.controllers += 1;
        }
    }

    /// Everything that happened since the last call.
    fn poll(&mut self) -> Vec<Input> {
        let mut inputs = Vec::new();
        let mut event = Event::new();
        // SAFETY: `event` is a buffer of SDL_Event's size and alignment.
        while unsafe { (self.sdl.poll_event)(&mut event) } != 0 {
            match event.kind() {
                sdl::QUIT => inputs.push(Input::Quit),
                sdl::CONTROLLERDEVICEADDED => self.open_controller(event.which()),
                sdl::CONTROLLERDEVICEREMOVED => self.controllers = self.controllers.saturating_sub(1),
                sdl::CONTROLLERBUTTONDOWN | sdl::CONTROLLERBUTTONUP => {
                    if let Some(button) = Button::from_sdl(event.control()) {
                        self.hold(button, 1, event.kind() == sdl::CONTROLLERBUTTONDOWN, &mut inputs);
                    }
                }
                sdl::CONTROLLERAXISMOTION => self.axis(event.control(), event.axis_value(), &mut inputs),
                // A firmware that also turns the pad into key presses
                // (gptokeyb) would double every button; with a pad open, the
                // keyboard is left to it.
                sdl::KEYDOWN | sdl::KEYUP if self.controllers == 0 && !event.key_repeat() => {
                    keyboard(event.scancode(), event.kind() == sdl::KEYDOWN, &mut inputs);
                }
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

    /// Shows `rgba`, `width` by `height`, as large as the screen takes at its
    /// own shape, on black.
    fn show(&mut self, rgba: &[u8], width: u32, height: u32) {
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

        // SAFETY: SDL2 calls on a live renderer, with a texture made for it and
        // a pixel buffer at least `height` rows of `width * 4` bytes.
        unsafe {
            if self.texture.is_null() || self.texture_size != (width, height) {
                if !self.texture.is_null() {
                    (self.sdl.destroy_texture)(self.texture);
                }
                self.texture = (self.sdl.create_texture)(self.renderer, sdl::PIXELFORMAT_ABGR8888, sdl::TEXTUREACCESS_STREAMING, width, height);
                self.texture_size = (width, height);
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
        ((width / scale) as u32, (height / scale) as u32, scale as u32)
    }

    /// Shows the list until a game is picked, or `None` when the player quits.
    fn choose(&mut self, menu: &mut Menu) -> Option<PathBuf> {
        menu.refresh();
        let mut dirty = true;
        let mut repeat: Option<(Button, Instant)> = None;

        loop {
            for input in self.poll() {
                match input {
                    Input::Quit | Input::Back => return None,
                    Input::Button(Button::Start, true) if self.select_held() => return None,
                    Input::Button(button, true) => {
                        match button {
                            Button::A | Button::Start => {
                                if let Some(game) = menu.selected() {
                                    return Some(game);
                                }
                            }
                            Button::Y => {
                                let game = menu.selected();
                                self.settings_menu(&Context {
                                    game: game.as_deref(),
                                    frame: None,
                                });
                                repeat = None;
                                dirty = true;
                                continue;
                            }
                            _ => {}
                        }
                        dirty |= menu.navigate(button);
                        repeat = Some((button, Instant::now() + Duration::from_millis(400)));
                    }
                    Input::Button(button, false) => {
                        if repeat.is_some_and(|(held, _)| held == button) {
                            repeat = None;
                        }
                    }
                    Input::Handset(..) => {}
                }
            }

            // A direction held down keeps moving.
            if let Some((button, at)) = repeat
                && Instant::now() >= at
            {
                dirty |= menu.navigate(button);
                repeat = Some((button, Instant::now() + Duration::from_millis(70)));
            }

            if dirty {
                let (width, height, _) = self.menu_size();
                let rgba = menu.draw(width, height);
                self.show(&rgba, width, height);
                dirty = false;
            }
            std::thread::sleep(FRAME);
        }
    }

    /// Shows `text` until A, B or START is pressed.
    fn message(&mut self, text: &str) {
        let (width, height, _) = self.menu_size();
        let rgba = library::draw_message(text, "A 확인", width, height);
        self.show(&rgba, width, height);
        loop {
            for input in self.poll() {
                match input {
                    Input::Quit | Input::Back | Input::Button(Button::A | Button::B | Button::Start, true) => return,
                    _ => {}
                }
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

        let data = std::fs::read(game).map_err(|error| format!("게임 파일을 읽을 수 없습니다: {error}"))?;
        // A game fixed to a preset plays with it.
        let fixed = game
            .file_name()
            .and_then(|file| self.store.fixed(&file.to_string_lossy()).map(str::to_owned));
        if let Some(preset) = fixed {
            self.store.apply(&preset);
        }
        let runtime_dir = std::env::current_dir().unwrap_or_default().join("data");
        let _ = std::fs::create_dir_all(&runtime_dir);
        let failure = host::start(data, runtime_dir, "Linux".to_owned());
        if !failure.is_empty() {
            return Err(failure);
        }
        // SAFETY: a device id SDL handed out, or 0, which SDL ignores.
        unsafe { (self.sdl.pause_audio_device)(self.audio, 0) };

        let result = self.run_game(game);

        host::stop();
        // SAFETY: as above.
        unsafe { (self.sdl.pause_audio_device)(self.audio, 1) };
        if let Err(error) = std::fs::write("last_game_log.txt", host::log()) {
            eprintln!("로그를 쓸 수 없습니다: {error}");
        }
        result
    }

    fn run_game(&mut self, game: &Path) -> Result<(), String> {
        // The newest frame, kept for the menu to show behind it.
        let mut frame: Option<(u32, u32, Vec<u8>)> = None;
        // The handset key each button pressed, so letting it go releases that
        // key even if SELECT was let go in between.
        let mut pressed: [Option<i32>; BUTTON_COUNT] = [None; BUTTON_COUNT];
        let release_all = |pressed: &mut [Option<i32>; BUTTON_COUNT]| {
            for key in pressed.iter_mut().filter_map(Option::take) {
                host::key(key, false);
            }
        };

        loop {
            let started = Instant::now();

            for input in self.poll() {
                match input {
                    Input::Quit => {
                        release_all(&mut pressed);
                        std::process::exit(0);
                    }
                    Input::Back => {
                        release_all(&mut pressed);
                        return Ok(());
                    }
                    // The handheld's menu / hotkey button: the menu, with the
                    // game and its clock and sound held still behind it.
                    Input::Button(Button::Guide, true) => {
                        release_all(&mut pressed);
                        host::hold_clock(true);
                        // SAFETY: a device id SDL handed out, or 0, which SDL ignores.
                        unsafe { (self.sdl.pause_audio_device)(self.audio, 1) };
                        let outcome = self.settings_menu(&Context {
                            game: Some(game),
                            frame: frame.as_ref().map(|(width, height, rgba)| (*width, *height, rgba.as_slice())),
                        });
                        // SAFETY: as above.
                        unsafe { (self.sdl.pause_audio_device)(self.audio, 0) };
                        host::hold_clock(false);
                        if let Outcome::EndGame = outcome {
                            return Ok(());
                        }
                        if let Some((width, height, rgba)) = &frame {
                            let (width, height, rgba) = (*width, *height, rgba.clone());
                            self.show(&rgba, width, height);
                        }
                    }
                    // SELECT+START: straight back to the list.
                    Input::Button(Button::Start, true) if self.select_held() => {
                        release_all(&mut pressed);
                        return Ok(());
                    }
                    Input::Button(button, true) => {
                        if let Some(key) = self.store.controls().key(button, self.select_held()) {
                            host::key(key, true);
                            pressed[button.index()] = Some(key);
                        }
                    }
                    Input::Button(button, false) => {
                        if let Some(key) = pressed[button.index()].take() {
                            host::key(key, false);
                        }
                    }
                    Input::Handset(key, down) => host::key(key, down),
                }
            }

            let failure = host::tick(16);
            if !failure.is_empty() {
                return Err(failure);
            }
            if let Some((width, height, rgba)) = host::take_frame_rgba() {
                self.show(&rgba, width, height);
                frame = Some((width, height, rgba));
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
            if let Some(rest) = target.checked_sub(started.elapsed()) {
                std::thread::sleep(rest);
            }
        }
    }
}

/// A keyboard, for trying the port on a desktop: the desktop build's keys for
/// the handset, the arrows, Enter and Escape for the list, and F3 to F7 for
/// X, Y, SELECT, MENU and START.
fn keyboard(scancode: i32, down: bool, inputs: &mut Vec<Input>) {
    let button = match scancode {
        82 => Some(Button::Up),
        81 => Some(Button::Down),
        80 => Some(Button::Left),
        79 => Some(Button::Right),
        40 | 44 => Some(Button::A),
        42 => Some(Button::B),
        225 => Some(Button::L1),
        229 => Some(Button::R1),
        60 => Some(Button::X),
        61 => Some(Button::Y),
        62 => Some(Button::Select),
        63 => Some(Button::Guide),
        64 => Some(Button::Start),
        _ => None,
    };
    if let Some(button) = button {
        inputs.push(Input::Button(button, down));
        return;
    }
    if scancode == 41 {
        if down {
            inputs.push(Input::Back);
        }
        return;
    }
    // 1 2 3 / Q W E / A S D / Z X C, as the desktop build lays the number pad.
    let key = match scancode {
        30 => 9,
        31 => 10,
        32 => 11,
        20 => 12,
        26 => 13,
        8 => 14,
        4 => 15,
        22 => 16,
        7 => 17,
        29 => 18,
        27 => 8,
        6 => 19,
        _ => return,
    };
    inputs.push(Input::Handset(key, down));
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
