use alloc::{boxed::Box, collections::BTreeMap, format, string::String, vec::Vec};
use core::pin::Pin;

use wie_backend::{Emulator, Event, Platform, System, TaskRunner};
use wie_core_arm::ArmCore;
use wie_util::{Result, WieError};

use crate::{
    archive::{BrewArchive, is_brew_package},
    runtime::{Brew, IMAGE_BASE, key_code},
};

const PANEL: (u32, u32) = (176, 220);

/// Titles drawn for a smaller handset than [`PANEL`], by the application id
/// their information file is named after. They draw their screens at a fixed
/// size from the top-left corner whatever the device information says.
const SMALLER_PANELS: &[(u32, (u32, u32))] = &[
    // 카트라이더레이싱: its title picture is 120x146, and its menus and race
    // fill that much of the panel and leave the rest black.
    (26735, (120, 146)),
];

/// The licence file a title binds to the handset it was bought on.
const HANDSET_LICENCE: &str = "kenviron.cfg";

/// How long the run loop waits when the title has handed over no frame yet.
const IDLE_POLL_MS: u64 = 10;
/// The longest the run loop waits before looking for keys again. A title
/// can leave nothing but a far timer: 카트라이더레이싱's 아이템을 충전
/// 하셨습니다 stops its frames and waits for a key with only its fifty-second
/// time-out armed, and the loop slept through every key pressed.
const KEY_POLL_MS: u64 = 16;

struct BrewTaskRunner {
    core: ArmCore,
}

#[async_trait::async_trait]
impl TaskRunner for BrewTaskRunner {
    async fn run(&self, future: Pin<Box<dyn Future<Output = Result<()>> + Send>>) -> Result<()> {
        self.core.run_in_thread(async move || future.await)?.await
    }
}

pub struct BrewEmulator {
    core: ArmCore,
    system: System,
    brew: Brew,
}

impl BrewEmulator {
    pub fn loadable_archive(files: &BTreeMap<String, Vec<u8>>) -> bool {
        is_brew_package(files)
    }

    /// The panel the title was drawn for. Most KTF handsets these titles were
    /// sold on were 176x220, and 카샨 lays every screen out in that; a title
    /// drawn for a smaller one is listed in [`SMALLER_PANELS`].
    pub fn screen_size(archive: &[u8]) -> Option<(u32, u32)> {
        let files = wie_backend::extract_zip(archive).ok()?;
        if !is_brew_package(&files) {
            return None;
        }

        let id = BrewArchive::open(files).ok().map(|x| x.info.application_id);
        Some(
            SMALLER_PANELS
                .iter()
                .find(|(title, _)| Some(*title) == id)
                .map_or(PANEL, |(_, panel)| *panel),
        )
    }

    /// The name the title's own writes are kept under: its ClassID.
    pub fn save_id(files: &BTreeMap<String, Vec<u8>>) -> Option<String> {
        if !is_brew_package(files) {
            return None;
        }

        let archive = BrewArchive::open(files.clone()).ok()?;

        Some(format!("{}", archive.info.application_id))
    }

    pub fn from_archive(platform: Box<dyn Platform>, files: BTreeMap<String, Vec<u8>>) -> Result<Self> {
        let archive = BrewArchive::open(files)?;
        let identifier = archive
            .info
            .application_identifier()
            .ok_or_else(|| WieError::FatalError("BREW module information file carries no application identifier".into()))?;

        // The ClassID is what the package is filed under, and so where the
        // title's own writes are kept.
        let id = format!("{}", archive.info.application_id);
        tracing::info!(
            "Loading BREW app {} ({id}) by {}, class {identifier}",
            archive.info.name,
            archive.info.vendor
        );

        let mut core = ArmCore::new(false, None)?;
        let system = System::new(platform, &id, &id, BrewTaskRunner { core: core.clone() });

        system.filesystem().enable_case_insensitive_reads();
        for (name, data) in &archive.files {
            // The handset the package was bought on is written into this file
            // the first time the title runs, and checked against the handset
            // every time after. Shipped, it names the buyer's handset rather
            // than this one and the title stops on `실행 권한이 없습니다`;
            // left out, the title writes it afresh as it did on its first run.
            if name.eq_ignore_ascii_case(HANDSET_LICENCE) {
                continue;
            }
            system.filesystem().add_virtual(&name.to_ascii_lowercase(), data.clone());
        }

        let brew = Brew::new(&mut core, &system, &archive)?;

        let mut core_clone = core.clone();
        let system_clone = system.clone();
        let brew_clone = brew.clone();
        system.spawn(async move || Self::run(&mut core_clone, &system_clone, &brew_clone, identifier).await);

        Ok(Self { core, system, brew })
    }

    async fn run(core: &mut ArmCore, system: &System, brew: &Brew, identifier: u32) -> Result<()> {
        brew.boot(core, identifier).await?;

        loop {
            loop {
                let event = system.event_queue().pop();
                let Some(event) = event else { break };

                match event {
                    Event::Keydown(key) => {
                        if let Some(code) = key_code(key) {
                            brew.key(core, code, true).await?;
                        }
                    }
                    Event::Keyup(key) => {
                        if let Some(code) = key_code(key) {
                            brew.key(core, code, false).await?;
                        }
                    }
                    _ => {}
                }
            }

            match brew.step(core).await? {
                Some(0) => system.yield_now().await,
                Some(wait) => system.sleep(wait.min(KEY_POLL_MS)).await,
                None => system.sleep(IDLE_POLL_MS).await,
            }
        }
    }
}

impl Emulator for BrewEmulator {
    fn handle_event(&mut self, event: Event) {
        self.system.event_queue().push(event)
    }

    fn is_idle(&self) -> bool {
        self.system.is_idle()
    }

    fn sleep_hint(&self) -> Option<u64> {
        self.system.idle_for()
    }

    fn tick(&mut self) -> Result<()> {
        self.system.tick().map_err(|x| {
            let reg_stack = self.core.dump_reg_stack(IMAGE_BASE);
            match x {
                WieError::FatalError(msg) => WieError::FatalError(format!("{msg}\n{reg_stack}")),
                _ => WieError::FatalError(format!("{x}\n{reg_stack}")),
            }
        })?;

        let system = &self.system;
        self.brew.take_frame(|image| wie_backend::present(system, image));

        Ok(())
    }
}
