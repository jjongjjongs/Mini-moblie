//! The "업데이트" tile on the list menu: whether the release holds a newer
//! build than this one, and getting it.
//!
//! This build's number is baked in at compile time from `MINIMOBILE_BUILD`
//! (the CI run number) - the same number CI writes to `version.txt` beside the
//! downloads. A check reads that file; a larger number is a newer build.
//!
//! Getting it differs by platform. A PortMaster handheld (aarch64 Linux) has
//! no browser to send the player to, but Linux lets a running program be
//! replaced, so the new build is downloaded and swapped in over the old one -
//! the next launch is the new version. The PC builds (Windows, Mac, Linux)
//! cannot overwrite a running program safely, so they open the release page in
//! the browser, where the archive downloads with a progress bar as usual.

use std::{
    process::Command,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use wie_android::host;

use crate::{App, FRAME, Input, library};

/// The release the CI build publishes to, under the project's public name.
const BASE: &str = "https://github.com/jjongjjongs/Mini-moblie/releases/download/mobile-dev/";
const VERSION_URL: &str = "https://github.com/jjongjjongs/Mini-moblie/releases/download/mobile-dev/version.txt";
const RELEASE_PAGE: &str = "https://github.com/jjongjjongs/Mini-moblie/releases/tag/mobile-dev";
/// The PortMaster port's archive and the binary inside it.
const PORTMASTER_ZIP: &str = "MiniMobile-portmaster.zip";
const PORTMASTER_BINARY: &str = "minimobile/minimobile.aarch64";

/// Whether this build can replace itself in place: only the PortMaster port.
const IN_PLACE: bool = cfg!(all(target_os = "linux", target_arch = "aarch64"));

/// This build's number, baked in from the CI run number, or `None` for a local
/// build that was never numbered - which never looks out of date.
fn installed() -> Option<u32> {
    option_env!("MINIMOBILE_BUILD").and_then(|value| value.trim().parse().ok())
}

/// `curl`, kept from flashing a console window open on Windows.
fn curl() -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new("curl");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command
}

/// Reads the latest published build number, `None` on any failure.
fn fetch_latest() -> Option<u32> {
    let output = curl().args(["-fsSL", "--max-time", "15", VERSION_URL]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines().next()?.trim().parse().ok()
}

/// Opens a page in the desktop's browser.
fn open_url(url: &str) {
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    if let Err(error) = Command::new(program).arg(url).spawn() {
        eprintln!("링크를 열 수 없습니다: {error}");
    }
}

#[derive(PartialEq)]
enum Phase {
    /// Not checked yet this run.
    Idle,
    /// A check is running in the background.
    Checking,
    /// Checked: this is the newest build.
    UpToDate,
    /// Checked: a newer build is out.
    Available,
    /// The check could not reach the release.
    Failed,
}

/// What is known about updates, kept on the app across menu opens.
pub(crate) struct State {
    installed: Option<u32>,
    latest: Option<u32>,
    phase: Phase,
    /// A check in flight, delivering the latest build number (or `None`).
    pending: Option<Receiver<Option<u32>>>,
}

impl State {
    pub(crate) fn new() -> State {
        State {
            installed: installed(),
            latest: None,
            phase: Phase::Idle,
            pending: None,
        }
    }

    /// Starts a check in the background, unless one has run or is running.
    fn start(&mut self) {
        if self.pending.is_some() || self.installed.is_none() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.pending = Some(rx);
        self.phase = Phase::Checking;
        thread::spawn(move || {
            let _ = tx.send(fetch_latest());
        });
    }

    /// Checks once when the menu first opens, quietly.
    pub(crate) fn ensure_checked(&mut self) {
        if self.phase == Phase::Idle {
            self.start();
        }
    }

    /// Checks again now, whatever the last outcome was.
    fn recheck(&mut self) {
        self.latest = None;
        self.phase = Phase::Idle;
        self.start();
    }

    /// Takes the result of a finished check; whether anything changed, so the
    /// menu can redraw the tile.
    pub(crate) fn poll(&mut self) -> bool {
        let Some(rx) = &self.pending else {
            return false;
        };
        match rx.try_recv() {
            Ok(result) => {
                self.pending = None;
                match result {
                    Some(latest) => {
                        self.latest = Some(latest);
                        self.phase = if self.installed.is_some_and(|installed| latest > installed) {
                            Phase::Available
                        } else {
                            Phase::UpToDate
                        };
                    }
                    None => self.phase = Phase::Failed,
                }
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.pending = None;
                self.phase = Phase::Failed;
                true
            }
        }
    }

    /// The build to move up to, when one is out.
    fn available(&self) -> Option<u32> {
        match (self.installed, self.latest) {
            (Some(installed), Some(latest)) if latest > installed => Some(latest),
            _ => None,
        }
    }

    /// The tile's value line.
    pub(crate) fn label(&self) -> String {
        if self.installed.is_none() {
            return "개발판".to_owned();
        }
        match self.phase {
            Phase::Checking => "확인 중".to_owned(),
            Phase::Available => format!("새 버전 {}", self.latest.unwrap_or(0)),
            Phase::UpToDate => "최신".to_owned(),
            Phase::Failed => "다시".to_owned(),
            Phase::Idle => "확인".to_owned(),
        }
    }
}

impl App {
    /// The update tile chosen: get the new build if one is out, otherwise check
    /// (again). A build that cannot name its own number just opens the page.
    pub(crate) fn update_select(&mut self) {
        if self.update.installed.is_none() {
            open_url(RELEASE_PAGE);
            self.message("받는 곳을 열었어요.\n\n새 버전을 받아 지금 프로그램을\n덮어써 주세요.");
            return;
        }
        match self.update.available() {
            Some(_) if IN_PLACE => self.swap_in_place(),
            Some(_) => {
                open_url(RELEASE_PAGE);
                self.message("받는 곳을 열었어요.\n\n새 파일을 받아 지금 프로그램을\n덮어써 주세요. 세이브와 게임은\n그대로예요.");
            }
            None if self.update.phase == Phase::Checking => {}
            None => self.update.recheck(),
        }
    }

    /// Downloads the PortMaster build and swaps it in over this one. Linux lets
    /// a running program's file be replaced; the new one runs on the next
    /// launch. Nothing is touched unless the download and the unpack both work.
    fn swap_in_place(&mut self) {
        let result = self.busy("새 버전 받는 중", || {
            let exe = std::env::current_exe().map_err(|error| format!("프로그램 위치를 알 수 없습니다: {error}"))?;
            let dir = exe.parent().ok_or_else(|| "프로그램 위치를 알 수 없습니다.".to_owned())?;
            let zip = dir.join("MiniMobile-update.zip");

            let status = curl()
                .args(["-fL", "--max-time", "600", "-o"])
                .arg(&zip)
                .arg(format!("{BASE}{PORTMASTER_ZIP}"))
                .status()
                .map_err(|error| format!("내려받을 수 없습니다 (curl): {error}"))?;
            if !status.success() {
                let _ = std::fs::remove_file(&zip);
                return Err("내려받지 못했어요. 인터넷을 확인해 주세요.".to_owned());
            }

            // The binary out of the zip, into a file beside the running one.
            let unpacked = unzip_binary(&zip);
            let _ = std::fs::remove_file(&zip);
            let binary = unpacked?;
            if binary.len() < 100_000 {
                return Err("받은 파일이 올바르지 않아요.".to_owned());
            }

            let staged = exe.with_extension("new");
            std::fs::write(&staged, &binary).map_err(|error| format!("쓸 수 없습니다: {error}"))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755));
            }
            std::fs::rename(&staged, &exe).map_err(|error| format!("바꿀 수 없습니다: {error}"))?;
            Ok(())
        });
        match result {
            Ok(()) => self.message("새 버전을 받았어요!\n\n앱을 다시 켜면 적용돼요."),
            Err(message) => self.message(&format!("업데이트 실패\n\n{message}")),
        }
    }

    /// Runs `work` on another thread, showing `text` with trailing dots until
    /// it finishes, so the window stays alive through a long download.
    fn busy<T: Send + 'static>(&mut self, text: &str, work: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(work());
        });
        let mut dots = 0usize;
        let mut stepped = Instant::now();
        loop {
            for input in self.poll() {
                if let Input::Quit = input {
                    host::stop();
                    std::process::exit(0);
                }
            }
            if let Ok(result) = rx.try_recv() {
                return result;
            }
            if stepped.elapsed() >= Duration::from_millis(350) {
                dots = (dots + 1) % 4;
                stepped = Instant::now();
            }
            let (width, height, _) = self.menu_size();
            let shown = format!("{text}{}", ".".repeat(dots));
            let rgba = library::draw_message(&shown, "", width, height);
            self.show(&rgba, width, height);
            thread::sleep(FRAME);
        }
    }
}

/// `unzip -p`, pulling the one binary's bytes out of the port's zip.
fn unzip_binary(zip: &std::path::Path) -> Result<Vec<u8>, String> {
    let output = Command::new("unzip")
        .arg("-p")
        .arg(zip)
        .arg(PORTMASTER_BINARY)
        .output()
        .map_err(|error| format!("압축을 풀 수 없습니다 (unzip): {error}"))?;
    if !output.status.success() {
        return Err("압축을 풀지 못했어요.".to_owned());
    }
    Ok(output.stdout)
}
