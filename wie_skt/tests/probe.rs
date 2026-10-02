//! A headless probe for an SK-VM archive, driven by the environment. Diagnostic.
//!
//! - `WIE_SKT_ZIP`  - path to the `.zip` archive (msd+jar+...). Unset -> no-op.
//! - `WIE_TICKS`    - how many ticks to run (default 4000).
//! - `WIE_LAST_PPM` - when set, the last painted frame is written here as a PPM.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use test_utils::{TestPlatform, TestPlatformEvent};
use wie_backend::{AudioSink, DatabaseRepository, Emulator, Event, Filesystem, Instant, Platform, Screen, canvas::Image, extract_zip};
use wie_skt::SktEmulator;
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
fn skt_probe() {
    let Ok(path) = std::env::var("WIE_SKT_ZIP") else {
        return;
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .try_init();

    let ticks_limit: u32 = std::env::var("WIE_TICKS").ok().and_then(|x| x.parse().ok()).unwrap_or(4000);
    let archive = std::fs::read(&path).expect("read archive");
    let files = extract_zip(&archive).expect("extract archive");

    // A host sizes its screen from the title's own quirk before the emulator
    // exists; `WIE_SCR_W`/`WIE_SCR_H` force a size instead, to try others.
    let (screen_w, screen_h) = match SktEmulator::screen_size(&archive) {
        Some((w, h)) => (w, h),
        None => (240, 320),
    };
    let screen_w = std::env::var("WIE_SCR_W").ok().and_then(|x| x.parse().ok()).unwrap_or(screen_w);
    let screen_h = std::env::var("WIE_SCR_H").ok().and_then(|x| x.parse().ok()).unwrap_or(screen_h);

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

    let mut emulator = match SktEmulator::from_archive(platform, files) {
        Ok(emulator) => emulator,
        Err(error) => {
            eprintln!("[probe] LOAD FAILED: {error:?}");
            return;
        }
    };

    // `WIE_SCRIPT="tick:KEY,tick:KEY"` presses are held 20 ticks each.
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

    // `WIE_FDUMP_DIR` + `WIE_FDUMP_EVERY` dump a PPM every N ticks into the dir.
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
