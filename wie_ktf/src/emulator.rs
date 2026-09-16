use core::pin::Pin;

use alloc::{borrow::ToOwned, boxed::Box, collections::BTreeMap, format, string::String, vec, vec::Vec};

use bytemuck::pod_collect_to_vec;
use jvm::{ClassInstance, Result as JvmResult, runtime::JavaLangString};

use wie_backend::{
    Emulator, Event, Options, Platform, System, TaskRunner, TitlePlatform,
    canvas::{Rgb565Pixel, VecImageBuffer},
    extract_zip, gz, protected_container, title_quirks,
};
use wie_core_arm::{Allocator, ArmCore};
use wie_jvm_support::JvmSupport;
use wie_util::{Result, WieError, write_generic};

use crate::{
    adf::{KtfAdf, find_client_bin},
    runtime::KtfJvmSupport,
};

/// The directory an archive's `__adf__` sits in, when exactly one does.
///
/// A dump does not always put the descriptor at the root. 소울카드마스터2 arrives
/// as a whole emulator bundle - the title under `game/<name>/`, beside the
/// `backends/` the bundle runs on - and looking only at the root found no
/// descriptor, so the archive was not recognised as one. It fell through to the
/// bare-jar path, which knows no `MClass` and started the title with no main
/// class at all: `Main class not found`, after a full runtime init.
///
/// The descriptor is what says where the archive is, at whatever depth it was
/// filed. Two of them would mean two archives in one zip, and which is the
/// title is not for this to guess.
fn descriptor_directory(files: &BTreeMap<String, Vec<u8>>) -> Option<String> {
    let mut directories = files.keys().filter_map(|path| path.strip_suffix("/__adf__"));

    match (directories.next(), directories.next()) {
        (Some(directory), None) => Some(directory.to_owned()),
        _ => None,
    }
}

/// Moves an archive whose descriptor sits in a subdirectory back to the root.
///
/// What lies under the descriptor's directory is the title. What lies beside it
/// belongs to whatever packed the dump - a bundle ships an emulator's own
/// binaries next to the game - and is dropped rather than folded in under its
/// bare name, so nothing outside the archive can answer for a name inside it.
fn reroot_archive(files: BTreeMap<String, Vec<u8>>) -> BTreeMap<String, Vec<u8>> {
    let Some(directory) = descriptor_directory(&files) else {
        return files;
    };

    tracing::info!("KTF archive is rooted at {directory}/");

    let prefix = format!("{directory}/");

    files
        .into_iter()
        .filter_map(|(path, data)| path.strip_prefix(&prefix).map(|inner| (inner.to_owned(), data)))
        .collect()
}

pub const IMAGE_BASE: u32 = 0x100000;

struct KtfTaskRunner {
    core: ArmCore,
}

#[async_trait::async_trait]
impl TaskRunner for KtfTaskRunner {
    async fn run(&self, future: Pin<Box<dyn Future<Output = Result<()>> + Send>>) -> Result<()> {
        self.core.run_in_thread(async move || future.await)?.await
    }
}

pub struct KtfEmulator {
    core: ArmCore,
    system: System,
    /// What the LCD held the last time it was presented, so an unchanged frame
    /// is not painted again. `None` until the first look. See
    /// [`KtfEmulator::present_lcd`].
    lcd_digest: Option<u64>,
    /// What the last look saw, so a frame is only shown once it has stopped
    /// changing. See [`KtfEmulator::present_lcd`].
    lcd_seen: Option<u64>,
}

impl KtfEmulator {
    pub fn from_archive(platform: Box<dyn Platform>, files: BTreeMap<String, Vec<u8>>, options: Options) -> Result<Self> {
        let files = reroot_archive(files);

        let adf = files
            .get("__adf__")
            .ok_or_else(|| WieError::FatalError("Missing __adf__ in KTF archive".into()))?;
        let adf = KtfAdf::parse(adf);

        tracing::info!("Loading app {}, pid {}, mclass {}", adf.aid, adf.pid, adf.mclass);

        let jar_filename = format!("{}.jar", adf.aid);

        Self::load(platform, &jar_filename, &adf.pid, &adf.aid, Some(adf.mclass), &files, options)
    }

    pub fn from_jar(
        platform: Box<dyn Platform>,
        jar_filename: &str,
        jar: Vec<u8>,
        pid: &str,
        aid: &str,
        main_class_name: Option<String>,
        options: Options,
    ) -> Result<Self> {
        let files = [(jar_filename.to_owned(), jar)].into_iter().collect();

        Self::load(platform, jar_filename, pid, aid, main_class_name, &files, options)
    }

    pub fn loadable_archive(files: &BTreeMap<String, Vec<u8>>) -> bool {
        files.contains_key("__adf__") || descriptor_directory(files).is_some()
    }

    pub fn loadable_jar(jar: &[u8]) -> bool {
        find_client_bin(jar).is_ok()
    }

    /// The panel this archive was packaged for, when its descriptor names one.
    ///
    /// A title sizes its drawing from what the screen reports, so this has to be
    /// known before the screen is built and therefore before there is an
    /// emulator to ask. The same shape as `LgtEmulator::screen_size`, and
    /// answered from the `DisplaySize` line rather than a table because KTF's
    /// descriptor carries it: of seven local archives, four say 240*320 and
    /// three say 176*220.
    ///
    /// A descriptor can still name a panel the title does not draw for - it is
    /// the handset's, and what the title gets is the handset's less the status
    /// strip - so the table answers first for the titles where the two differ.
    /// See `wie_backend::quirks`.
    pub fn screen_size(archive: &[u8]) -> Option<(u32, u32)> {
        let files = reroot_archive(extract_zip(archive).ok()?);
        let adf = KtfAdf::parse(files.get("__adf__")?);

        title_quirks(TitlePlatform::Ktf, &adf.aid).screen_size.or(adf.display_size)
    }

    fn load(
        platform: Box<dyn Platform>,
        jar_filename: &str,
        pid: &str,
        aid: &str,
        main_class_name: Option<String>,
        files: &BTreeMap<String, Vec<u8>>,
        mut options: Options,
    ) -> Result<Self> {
        // What the store delivered could be a DRM container rather than the
        // title. Nothing further on can make sense of one - the jar it is asked
        // to open is not a zip - so it is named here, where the answer can still
        // be the reason rather than a failure further in. See
        // `wie_backend::protected_container`.
        if let Some(jar) = files.get(jar_filename)
            && let Some(container) = protected_container(jar)
        {
            return Err(WieError::FatalError(format!(
                "the title is wrapped in {container}, so there is nothing here to run"
            )));
        }

        let mut core = ArmCore::new(options.enable_gdbserver, options.profile.take())?;

        let system = System::new(platform, pid, aid, KtfTaskRunner { core: core.clone() });
        system.set_title_draws_sideways(title_quirks(TitlePlatform::Ktf, aid).drawn_sideways);

        for (path, data) in files {
            let path = path.trim_start_matches("P/");
            system.filesystem().add_virtual(path, data.clone());

            // A package ships some of its data gzipped and the handset's
            // installer is what unpacks it; the guest only ever asks for the
            // unpacked name. 지크 carries `snd/B1.mmf.gz` through `B13.mmf.gz`
            // that way, beside the plain `snd/B0.mmf` in its jar, and without
            // them its sound thread finds the slot for every sound but the
            // first empty - see `wie_backend::gz`. The packed entry is left in
            // place too, so a title that does ask for it still finds it.
            if let Some(member) = gz::read_member(data)
                && let Some(unpacked) = gz::unpacked_path(path, &member)
            {
                tracing::debug!("unpacked {path} to {unpacked}, {} bytes", member.data.len());
                system.filesystem().add_virtual(&unpacked, member.data);
            }
        }

        // What a download would have delivered, when this package has been
        // through one already. A KTF title that fetches its data over the air
        // offers to do it every launch and quits if refused, and the server it
        // asks has been gone for years - but the archive's `P/` directory is
        // the handset's own copy of what arrived, so the exchange can be
        // answered out of it. See `wie_backend::local_network::funter`.
        let packaged = files
            .iter()
            .filter_map(|(path, data)| Some((path.strip_prefix("P/")?.to_owned(), data.clone())))
            .filter(|(path, _)| !path.is_empty())
            .collect::<BTreeMap<_, _>>();
        let funter = wie_backend::FunterEndpoint::new(packaged);
        if !funter.is_empty() {
            system.local_network().register(Box::new(funter));
        }

        // 드래곤아이즈2 checks its data with its own server before it will
        // start, and waits on the answer for ever. See
        // `wie_backend::local_network::dragoneyes`.
        system.local_network().register(Box::new(wie_backend::DragonEyesEndpoint));

        Allocator::init(&mut core)?;

        // The status strip a title has to work around, published where the
        // WIPI-C graphics layer reads it. See
        // `wie_wipi_c::api::graphics::new_screen_surface`; the reference's own
        // table gives the strip 24 rows on a 240/320/400-wide panel.
        //
        // 던전앤파이터 격투가 is the KTF title that asks for one: its C engine
        // composes a 240x296 scene (`MC_grpFillRect(0, 0, 240, 296)`, last
        // pixel at (239, 295)) onto a 320-row panel, so the bottom 24 rows are
        // never its and kept whatever had been there under every frame.
        const ANNUNCIATOR_ROWS: u32 = 24;
        let annunciator = options
            .annunciator
            .unwrap_or_else(|| title_quirks(TitlePlatform::Ktf, aid).expects_annunciator);
        write_generic(
            &mut core,
            wie_wipi_c::api::graphics::ANNUNCIATOR_ROWS_PTR,
            if annunciator { ANNUNCIATOR_ROWS } else { 0 },
        )?;

        let mut core_clone = core.clone();
        let mut system_clone = system.clone();
        let jar_filename_clone = jar_filename.to_owned();

        system.spawn(async move || Self::start(&mut core_clone, &mut system_clone, jar_filename_clone, main_class_name).await);

        Ok(Self {
            core,
            system,
            lcd_digest: None,
            lcd_seen: None,
        })
    }

    #[tracing::instrument(name = "start", skip_all)]
    async fn start(core: &mut ArmCore, system: &mut System, jar_filename: String, main_class_name: Option<String>) -> Result<()> {
        let (jvm, class_loader) = KtfJvmSupport::init(core, system, Some(&jar_filename)).await?;

        let main_class_name = if let Some(x) = main_class_name {
            x
        } else {
            return Err(WieError::FatalError("Main class not found".into()));
        };

        let main_class_name = main_class_name.replace('.', "/");

        let main_class_name_java = JavaLangString::from_rust_string(&jvm, &main_class_name).await.unwrap();
        let _main_class: Box<dyn ClassInstance> = jvm
            .invoke_virtual(
                &class_loader,
                "loadClass",
                "(Ljava/lang/String;)Ljava/lang/Class;",
                (main_class_name_java.clone(),),
            )
            .await
            .unwrap();

        let mut args_array = jvm.instantiate_array("Ljava/lang/String;", 1).await.unwrap();
        jvm.store_array(&mut args_array, 0, vec![main_class_name_java]).await.unwrap();
        let result: JvmResult<()> = jvm
            .invoke_static("org/kwis/msp/lcdui/Main", "main", "([Ljava/lang/String;)V", (args_array,))
            .await;

        if let Err(x) = result {
            return Err(JvmSupport::to_wie_err(&jvm, x).await);
        }

        Ok(())
    }
}

impl KtfEmulator {
    /// Show the LCD frame buffer when the title has drawn into it itself.
    ///
    /// A title whose drawing is a C engine writes that buffer directly and never
    /// flushes, because on the handset the buffer is the display; 던전앤파이터
    /// 격투가 draws its whole splash sequence that way and nothing here ever saw
    /// it. The Java layer cannot reach guest memory and the WIPI-C side never
    /// sees the writes, so the tick is where it has to be noticed.
    ///
    /// Titles that draw through the Java layer are untouched: they never take a
    /// screen frame buffer, so there is nothing to read, and their frames keep
    /// reaching the screen through the MIDP paint at its own depth. A buffer
    /// that has been taken but not drawn into is all one value and is left alone
    /// too, so taking the pointer alone does not blank a title.
    fn present_lcd(&mut self) {
        let core = self.core.clone();
        let data_ptr = |memory: u32| -> Result<u32> {
            let base: u32 = wie_util::read_generic(&core, memory)?;
            Ok(base + 8)
        };

        let Ok(Some((width, height, bytes))) = wie_wipi_c::api::graphics::screen_surface_bytes(&self.core, &data_ptr) else {
            return;
        };

        let mut digest = 0xcbf2_9ce4_8422_2325u64;
        for chunk in bytes.chunks(8) {
            let mut word = [0u8; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            digest = (digest ^ u64::from_le_bytes(word)).wrapping_mul(0x1000_0000_01b3);
        }

        // The buffer holds whatever the heap left there until the title draws -
        // `Allocator::alloc` does not zero - and that is not a frame. The first
        // look is the baseline, shown to nobody, so the mostly-black noise a
        // fresh allocation carries never reaches the screen.
        let Some(shown) = self.lcd_digest else {
            self.lcd_digest = Some(digest);
            self.lcd_seen = Some(digest);
            return;
        };
        if digest == shown {
            return;
        }

        // Nothing says when the engine has finished a frame, and a tick can land
        // in the middle of one. Waiting for the content to stop changing before
        // showing it costs a tick and is what keeps a half-drawn frame off the
        // screen.
        if self.lcd_seen != Some(digest) {
            self.lcd_seen = Some(digest);
            return;
        }

        // An untouched buffer is one value throughout. Presenting it would paint
        // over whatever the Java layer put on the screen.
        let first = &bytes[..2.min(bytes.len())];
        if bytes.chunks(2).all(|x| x == first) {
            self.lcd_digest = Some(digest);
            return;
        }

        let image = VecImageBuffer::<Rgb565Pixel>::from_raw(width, height, pod_collect_to_vec(&bytes));
        self.system.set_title_drives_lcd();
        wie_backend::present(&self.system, &image);
        self.lcd_digest = Some(digest);
    }
}

impl Emulator for KtfEmulator {
    fn handle_event(&mut self, event: Event) {
        self.system.event_queue().push(event)
    }

    fn tick(&mut self) -> Result<()> {
        self.system.tick().map_err(|x| {
            let reg_stack = self.core.dump_reg_stack(IMAGE_BASE);
            match x {
                WieError::FatalError(msg) => WieError::FatalError(format!("{msg}\n{reg_stack}")),
                _ => WieError::FatalError(format!("{x}\n{reg_stack}")),
            }
        })?;

        self.present_lcd();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use alloc::{
        collections::BTreeMap,
        string::{String, ToString},
        vec,
        vec::Vec,
    };

    use super::{KtfEmulator, descriptor_directory, reroot_archive};

    fn archive(paths: &[&str]) -> BTreeMap<String, Vec<u8>> {
        paths.iter().map(|path| ((*path).to_string(), vec![0u8; 1])).collect()
    }

    /// A descriptor at the root is already where it belongs.
    #[test]
    fn an_archive_at_the_root_is_left_alone() {
        let files = archive(&["__adf__", "__class__", "010261FB.jar", "P/save0"]);

        assert_eq!(descriptor_directory(&files), None);
        assert!(KtfEmulator::loadable_archive(&files));
        assert_eq!(reroot_archive(files.clone()).keys().count(), files.len());
    }

    /// 소울카드마스터2 arrives as a whole emulator bundle, with the title two
    /// directories down and the bundle's own binaries beside it. The title is
    /// what the descriptor's directory holds; the binaries are not.
    #[test]
    fn a_bundled_archive_is_rerooted_to_its_descriptor() {
        let files = archive(&[
            "backends/SDL2.dll",
            "backends/wipi-engine.exe",
            "game/소울카드마스터2/__adf__",
            "game/소울카드마스터2/__class__",
            "game/소울카드마스터2/010261FB.jar",
            "game/소울카드마스터2/P/save0",
        ]);

        assert_eq!(descriptor_directory(&files).as_deref(), Some("game/소울카드마스터2"));
        assert!(KtfEmulator::loadable_archive(&files), "a bundled archive is still an archive");

        let rerooted = reroot_archive(files);
        let mut names: Vec<&str> = rerooted.keys().map(|x| x.as_str()).collect();
        names.sort_unstable();

        assert_eq!(names, ["010261FB.jar", "P/save0", "__adf__", "__class__"]);
    }

    /// Two descriptors are two archives in one zip, and which one is the title
    /// is not for this to guess.
    #[test]
    fn two_descriptors_reroot_to_neither() {
        let files = archive(&["game/one/__adf__", "game/two/__adf__"]);

        assert_eq!(descriptor_directory(&files), None);
        assert!(!KtfEmulator::loadable_archive(&files));
        assert_eq!(reroot_archive(files.clone()).keys().count(), files.len(), "left as it was");
    }
}
