//! A headless probe for a bare J2ME jar, driven by the environment so any jar
//! can be pointed at it without touching the tree. Diagnostic only.
//!
//! - `WIE_J2ME_JAR` - path to the `.jar` to run. Unset, the probe is a no-op.
//! - `WIE_TICKS` - how many ticks to run (default 2000).

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use test_utils::{TestPlatform, TestPlatformEvent};
use wie_backend::{Emulator, Event};
use wie_j2me::J2MEEmulator;

#[test]
fn j2me_jar_probe() {
    let Ok(path) = std::env::var("WIE_J2ME_JAR") else {
        return;
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let ticks_limit: u32 = std::env::var("WIE_TICKS").ok().and_then(|x| x.parse().ok()).unwrap_or(2000);
    let jar = std::fs::read(&path).expect("read jar");
    let jar_filename = std::path::Path::new(&path).file_name().unwrap().to_string_lossy().into_owned();

    let exited = Arc::new(AtomicBool::new(false));
    let exited_clone = exited.clone();
    let platform = Box::new(TestPlatform::with_event_handler(move |event| match event {
        TestPlatformEvent::Stdout(buf) => eprint!("[stdout] {}", String::from_utf8_lossy(&buf)),
        TestPlatformEvent::OpenUrl(url) => eprintln!("[open-url] {url}"),
        TestPlatformEvent::Exit => exited_clone.store(true, Ordering::SeqCst),
    }));

    let mut emulator = match J2MEEmulator::from_jar(platform, &jar_filename, jar) {
        Ok(emulator) => emulator,
        Err(error) => {
            eprintln!("[probe] LOAD FAILED: {error:?}");
            return;
        }
    };

    let mut ticks = 0;
    let mut stopped = None;
    while ticks < ticks_limit && !exited.load(Ordering::SeqCst) {
        if ticks % 40 == 0 {
            emulator.handle_event(Event::Redraw);
        }
        if let Err(error) = emulator.tick() {
            stopped = Some(error);
            break;
        }
        ticks += 1;
    }

    eprintln!("[probe] ticks={ticks} exited={}", exited.load(Ordering::SeqCst));
    match &stopped {
        Some(error) => eprintln!("[probe] STOPPED: {error:?}"),
        None => eprintln!("[probe] ran to the tick limit without stopping"),
    }
}
