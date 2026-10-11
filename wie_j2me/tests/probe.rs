//! A headless probe for a bare J2ME jar, driven by the environment so any jar
//! can be pointed at it without touching the tree. Diagnostic only.
//!
//! - `WIE_J2ME_JAR` - path to the `.jar` to run. Unset, the probe is a no-op.
//! - `WIE_J2ME_JAD` - optional `.jad` beside it, so the MIDlet's own
//!   descriptor (its main class, its LCD size) is read rather than guessed.
//! - `WIE_TICKS` - how many ticks to run (default 2000).
//! - `WIE_SCRIPT` - `"tick:KEY,tick:KEY"` presses, each held 20 ticks.
//! - `WIE_TOUCH` - `"tick:x:y,..."` touches at `x`, `y`, each dragged 3
//!   pixels right after 5 ticks and lifted after 10. Setting it turns touch on.
//! - `WIE_FDUMP_DIR` + `WIE_FDUMP_EVERY` - dump a PPM every N ticks into the dir.
//! - `WIE_LAST_PPM` - write the last painted frame here as a PPM.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use test_utils::{TestPlatform, TestPlatformEvent};
use wie_backend::{AudioSink, DatabaseRepository, Emulator, Event, Filesystem, Instant, Platform, PointerKind, Screen, canvas::Image};
use wie_j2me::J2MEEmulator;
use wie_util::Result;

#[derive(Default)]
struct Captured {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    frames: u32,
}

#[derive(Clone)]
struct CaptureScreen {
    captured: Arc<Mutex<Captured>>,
    width: u32,
    height: u32,
}

impl Screen for CaptureScreen {
    fn request_redraw(&self) -> Result<()> {
        Ok(())
    }
    fn paint(&self, image: &dyn Image) {
        let mut c = self.captured.lock().unwrap();
        c.width = image.width();
        c.height = image.height();
        c.frames += 1;
        let mut pixels = Vec::with_capacity((image.width() * image.height() * 3) as usize);
        for color in image.colors() {
            pixels.push(color.r);
            pixels.push(color.g);
            pixels.push(color.b);
        }
        c.pixels = pixels;
    }
    fn width(&self) -> u32 {
        self.width
    }
    fn height(&self) -> u32 {
        self.height
    }
}

struct CapturePlatform {
    inner: TestPlatform,
    screen: CaptureScreen,
}

impl Platform for CapturePlatform {
    fn screen(&self) -> &dyn Screen {
        &self.screen
    }
    fn now(&self) -> Instant {
        self.inner.now()
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

fn key_by_name(name: &str) -> Option<wie_backend::KeyCode> {
    use wie_backend::KeyCode::*;
    Some(match name.to_ascii_uppercase().as_str() {
        "UP" => UP,
        "DOWN" => DOWN,
        "LEFT" => LEFT,
        "RIGHT" => RIGHT,
        "OK" | "FIRE" => OK,
        "CLEAR" | "CLR" => CLEAR,
        "LSK" | "LEFT_SOFT_KEY" => LEFT_SOFT_KEY,
        "RSK" | "RIGHT_SOFT_KEY" => RIGHT_SOFT_KEY,
        "NUM0" => NUM0,
        "NUM1" => NUM1,
        "NUM2" => NUM2,
        "NUM3" => NUM3,
        "NUM4" => NUM4,
        "NUM5" => NUM5,
        "NUM6" => NUM6,
        "NUM7" => NUM7,
        "NUM8" => NUM8,
        "NUM9" => NUM9,
        "HASH" => HASH,
        "STAR" => STAR,
        _ => return None,
    })
}

#[test]
fn j2me_jar_probe() {
    let Ok(path) = std::env::var("WIE_J2ME_JAR") else {
        return;
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .try_init();

    let ticks_limit: u32 = std::env::var("WIE_TICKS").ok().and_then(|x| x.parse().ok()).unwrap_or(2000);
    let jar = std::fs::read(&path).expect("read jar");
    let jar_filename = std::path::Path::new(&path).file_name().unwrap().to_string_lossy().into_owned();

    let (screen_w, screen_h) = (
        std::env::var("WIE_SCR_W").ok().and_then(|x| x.parse().ok()).unwrap_or(240u32),
        std::env::var("WIE_SCR_H").ok().and_then(|x| x.parse().ok()).unwrap_or(320u32),
    );

    let exited = Arc::new(AtomicBool::new(false));
    let exited_clone = exited.clone();
    let screen = CaptureScreen {
        captured: Default::default(),
        width: screen_w,
        height: screen_h,
    };
    let platform = Box::new(CapturePlatform {
        inner: TestPlatform::with_event_handler(move |event| match event {
            TestPlatformEvent::Stdout(buf) => eprint!("[stdout] {}", String::from_utf8_lossy(&buf)),
            TestPlatformEvent::OpenUrl(url) => eprintln!("[open-url] {url}"),
            TestPlatformEvent::Exit => exited_clone.store(true, Ordering::SeqCst),
        }),
        screen: screen.clone(),
    });

    let emulator = match std::env::var("WIE_J2ME_JAD") {
        Ok(jad_path) => {
            let jad = std::fs::read(&jad_path).expect("read jad");
            J2MEEmulator::from_jad_jar(platform, jad, jar_filename, jar)
        }
        Err(_) => J2MEEmulator::from_jar(platform, &jar_filename, jar),
    };
    let mut emulator = match emulator {
        Ok(emulator) => emulator,
        Err(error) => {
            eprintln!("[probe] LOAD FAILED: {error:?}");
            return;
        }
    };

    let script: Vec<(u32, wie_backend::KeyCode)> = std::env::var("WIE_SCRIPT")
        .ok()
        .map(|s| {
            s.split(',')
                .filter(|x| !x.trim().is_empty())
                .map(|pair| {
                    let (t, k) = pair.split_once(':').expect("tick:KEY");
                    (t.trim().parse().unwrap(), key_by_name(k.trim()).expect("key name"))
                })
                .collect()
        })
        .unwrap_or_default();

    let touches: Vec<(u32, i32, i32)> = std::env::var("WIE_TOUCH")
        .ok()
        .map(|s| {
            s.split(',')
                .filter(|x| !x.trim().is_empty())
                .map(|touch| {
                    let parts: Vec<i32> = touch.split(':').map(|x| x.trim().parse().expect("tick:x:y")).collect();
                    (parts[0] as u32, parts[1], parts[2])
                })
                .collect()
        })
        .unwrap_or_default();
    wie_backend::set_touch_enabled(!touches.is_empty());

    let fdump_dir = std::env::var("WIE_FDUMP_DIR").ok();
    let fdump_every: u32 = std::env::var("WIE_FDUMP_EVERY").ok().and_then(|x| x.parse().ok()).unwrap_or(250);
    if let Some(dir) = &fdump_dir {
        let _ = std::fs::create_dir_all(dir);
    }

    let mut ticks = 0;
    let mut stopped = None;
    while ticks < ticks_limit && !exited.load(Ordering::SeqCst) {
        if ticks % 40 == 0 {
            emulator.handle_event(Event::Redraw);
        }
        if let Some(dir) = &fdump_dir {
            if ticks > 0 && ticks % fdump_every == 0 {
                let c = screen.captured.lock().unwrap();
                if !c.pixels.is_empty() {
                    let mut ppm = format!("P6\n{} {}\n255\n", c.width, c.height).into_bytes();
                    ppm.extend_from_slice(&c.pixels);
                    let _ = std::fs::write(format!("{dir}/t{ticks:06}.ppm"), ppm);
                }
            }
        }
        for &(at, key) in &script {
            if ticks == at {
                emulator.handle_event(Event::Keydown(key));
            }
            if ticks == at + 20 {
                emulator.handle_event(Event::Keyup(key));
            }
        }
        for &(at, x, y) in &touches {
            let kind = match ticks.checked_sub(at) {
                Some(0) => PointerKind::Pressed,
                Some(5) => PointerKind::Dragged,
                Some(10) => PointerKind::Released,
                _ => continue,
            };
            let x = if kind == PointerKind::Pressed { x } else { x + 3 };
            emulator.handle_event(Event::Pointer { kind, x, y });
        }
        if let Err(error) = emulator.tick() {
            stopped = Some(error);
            break;
        }
        ticks += 1;
    }

    let c = screen.captured.lock().unwrap();
    eprintln!(
        "[probe] ticks={ticks} exited={} frames={} {}x{}",
        exited.load(Ordering::SeqCst),
        c.frames,
        c.width,
        c.height
    );
    match &stopped {
        Some(error) => eprintln!("[probe] STOPPED: {error:?}"),
        None => eprintln!("[probe] ran to the tick limit without stopping"),
    }

    if let Ok(out) = std::env::var("WIE_LAST_PPM") {
        if !c.pixels.is_empty() {
            let mut ppm = format!("P6\n{} {}\n255\n", c.width, c.height).into_bytes();
            ppm.extend_from_slice(&c.pixels);
            std::fs::write(&out, ppm).expect("write ppm");
            eprintln!("[probe] wrote frame to {out}");
        }
    }
}
