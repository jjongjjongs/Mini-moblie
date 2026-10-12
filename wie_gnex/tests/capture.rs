//! Runs a GNEX title for a while and writes the frames it presented, for
//! checking a title by eye. The titles are not in the repository: point
//! `GNEX_ARCHIVE` at one (a zip of its `.mod` and `.sgs`), and `GNEX_FRAMES`
//! at a directory for the frames (`frameNNNNN.ppm`). Without them the test
//! does nothing. `GNEX_SECONDS` sets how long it runs, and `GNEX_KEYS` presses
//! keys on the way, as `ms:KEY` pairs (`3000:OK,5000:OK`).

use std::{
    collections::BTreeMap,
    env, fs,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use test_utils::TestPlatform;
use wie_backend::{AudioSink, DatabaseRepository, Emulator, Event, Filesystem, Instant, KeyCode, Platform, Screen, canvas::Image, extract_zip};
use wie_gnex::GnexEmulator;
use wie_util::Result;

/// Each frame painted, with the guest time it was painted at.
type Frames = Arc<Mutex<Vec<(u64, Vec<u8>)>>>;

/// Counts the notes the titles' sounds play, to tell that they sound at all.
struct CountingSink(Arc<AtomicU64>);

impl AudioSink for CountingSink {
    fn play_wave(&self, _clip: u32, _channel: u8, _sampling_rate: u32, _wave_data: &[i16]) {}

    fn midi_note_on(&self, _voice: u32, _channel_id: u8, _note: u8, _velocity: u8) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    fn midi_note_off(&self, _voice: u32, _channel_id: u8, _note: u8, _velocity: u8) {}

    fn midi_program_change(&self, _voice: u32, _channel_id: u8, _program: u8) {}

    fn midi_control_change(&self, _voice: u32, _channel_id: u8, _control: u8, _value: u8) {}

    fn midi_pitch_bend(&self, _voice: u32, _channel_id: u8, _value: u16) {}

    fn midi_sysex(&self, _voice: u32, _data: &[u8]) {}
}

struct CaptureScreen {
    frames: Frames,
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
    notes: Arc<AtomicU64>,
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
        Box::new(CountingSink(self.notes.clone()))
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
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .try_init();
    let files: BTreeMap<String, Vec<u8>> = extract_zip(&fs::read(archive).unwrap()).unwrap();
    assert!(GnexEmulator::loadable_archive(&files));

    let frames = Arc::new(Mutex::new(Vec::new()));
    let clock = Arc::new(AtomicU64::new(1_000_000));
    let notes = Arc::new(AtomicU64::new(0));
    let platform = CapturePlatform {
        inner: TestPlatform::new(),
        notes: notes.clone(),
        screen: CaptureScreen {
            frames: frames.clone(),
            clock: clock.clone(),
        },
        clock: clock.clone(),
    };
    let mut emulator = GnexEmulator::from_archive(Box::new(platform), files).unwrap();

    let seconds: u64 = env::var("GNEX_SECONDS").ok().and_then(|x| x.parse().ok()).unwrap_or(12);
    let mut keys: Vec<(u64, String)> = env::var("GNEX_KEYS")
        .unwrap_or_default()
        .split(',')
        .filter_map(|x| x.split_once(':').map(|(at, key)| (at.parse().unwrap(), key.to_owned())))
        .collect();
    keys.sort();
    for step in 0..seconds * 1000 / 16 {
        clock.fetch_add(16, Ordering::SeqCst);
        while keys.first().is_some_and(|(at, _)| *at <= step * 16) {
            let (_, key) = keys.remove(0);
            emulator.handle_event(Event::Keydown(KeyCode::parse(&key)));
            emulator.handle_event(Event::Keyup(KeyCode::parse(&key)));
        }
        emulator.tick().unwrap();
    }

    println!("notes played: {}", notes.load(Ordering::SeqCst));
    fs::create_dir_all(&out).unwrap();
    for (at, ppm) in frames.lock().unwrap().iter() {
        fs::write(format!("{out}/frame{:07}.ppm", at - 1_000_000), ppm).unwrap();
    }
}
