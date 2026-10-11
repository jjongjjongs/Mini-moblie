//! Runs a GNEX title for a while and writes the frames it presented, for
//! checking a title by eye. The titles are not in the repository: point
//! `GNEX_ARCHIVE` at one (a zip of its `.mod` and `.sgs`), and `GNEX_FRAMES`
//! at a directory for the frames (`frameNNNNN.ppm`). Without them the test
//! does nothing.

use std::{
    collections::BTreeMap,
    env, fs,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use test_utils::TestPlatform;
use wie_backend::{AudioSink, DatabaseRepository, Emulator, Filesystem, Instant, Platform, Screen, canvas::Image, extract_zip};
use wie_gnex::GnexEmulator;
use wie_util::Result;

struct CaptureScreen {
    frames: Arc<Mutex<Vec<(u64, Vec<u8>)>>>,
    clock: Arc<AtomicU64>,
}

impl Screen for CaptureScreen {
    fn request_redraw(&self) -> Result<()> {
        Ok(())
    }

    fn paint(&self, image: &dyn Image) {
        let mut ppm = format!("P6 {} {} 255\n", image.width(), image.height()).into_bytes();
        for y in 0..image.height() as i32 {
            for x in 0..image.width() as i32 {
                let c = image.get_pixel(x, y);
                ppm.extend([c.r, c.g, c.b]);
            }
        }
        self.frames.lock().unwrap().push((self.clock.load(Ordering::SeqCst), ppm));
    }

    fn width(&self) -> u32 {
        240
    }

    fn height(&self) -> u32 {
        320
    }
}

struct CapturePlatform {
    inner: TestPlatform,
    screen: CaptureScreen,
    clock: Arc<AtomicU64>,
}

impl Platform for CapturePlatform {
    fn screen(&self) -> &dyn Screen {
        &self.screen
    }

    fn now(&self) -> Instant {
        Instant::from_epoch_millis(self.clock.load(Ordering::SeqCst))
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

    fn write_stdout(&self, buf: &[u8]) {
        self.inner.write_stdout(buf)
    }

    fn write_stderr(&self, buf: &[u8]) {
        self.inner.write_stderr(buf)
    }

    fn exit(&self) {}

    fn vibrate(&self, _duration: u64, _intensity: u8) {}

    fn set_backlight_mode(&self, _mode: u8) {}
}

#[test]
fn capture() {
    let (Ok(archive), Ok(out)) = (env::var("GNEX_ARCHIVE"), env::var("GNEX_FRAMES")) else {
        return;
    };
    let files: BTreeMap<String, Vec<u8>> = extract_zip(&fs::read(archive).unwrap()).unwrap();
    assert!(GnexEmulator::loadable_archive(&files));

    let frames = Arc::new(Mutex::new(Vec::new()));
    let clock = Arc::new(AtomicU64::new(1_000_000));
    let platform = CapturePlatform {
        inner: TestPlatform::new(),
        screen: CaptureScreen {
            frames: frames.clone(),
            clock: clock.clone(),
        },
        clock: clock.clone(),
    };
    let mut emulator = GnexEmulator::from_archive(Box::new(platform), files).unwrap();

    let seconds: u64 = env::var("GNEX_SECONDS").ok().and_then(|x| x.parse().ok()).unwrap_or(12);
    for _ in 0..seconds * 1000 / 16 {
        clock.fetch_add(16, Ordering::SeqCst);
        emulator.tick().unwrap();
    }

    fs::create_dir_all(&out).unwrap();
    for (at, ppm) in frames.lock().unwrap().iter() {
        fs::write(format!("{out}/frame{:07}.ppm", at - 1_000_000), ppm).unwrap();
    }
}
