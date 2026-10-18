//! SKT's native titles: WIPI-C programs a handset kept as a `.daf` beside its
//! `.mod`, where SK-VM titles are a `.jar` and its `.msd`.
//!
//! The format and the platform interface are described in [`daf`] and
//! [`interface`]. The title is started and handed its keys through a small
//! Java wrapper on the WIPI-Java display, as LGT's native titles are.

mod clet;
mod context;
pub mod daf;
mod interface;
mod lcd;
mod libc;

use core::pin::Pin;

use alloc::{
    borrow::ToOwned,
    boxed::Box,
    collections::BTreeMap,
    format,
    string::{String, ToString},
    sync::Arc,
    vec,
    vec::Vec,
};

use jvm::{Result as JvmResult, runtime::JavaLangString};
use jvm_rust::ClassDefinitionImpl;

use wie_backend::{Platform, System, TaskRunner};
use wie_core_arm::{Allocator, ArmCore};
use wie_jvm_support::{JvmSupport, RustJavaJvmImplementation};
use wie_util::{Result, WieError};

use self::{
    clet::{CletContext, SktClet, SktCletCard},
    daf::{DATA_BASE, DafImage, Resources},
    interface::SvcState,
};

pub use self::lcd::LcdWatcher;

/// Where text is loaded. The image is linked with text at zero, so anywhere
/// clear of data and the platform's own regions will do; this is where KTF
/// puts its images too.
const TEXT_BASE: u32 = 0x10_0000;

/// The entry points' places in the header - see [`daf`].
const START_CLET: usize = 0;
const PAINT_CLET: usize = 4;
const HANDLE_CLET_EVENT: usize = 5;

struct SktNativeTaskRunner {
    core: ArmCore,
}

#[async_trait::async_trait]
impl TaskRunner for SktNativeTaskRunner {
    async fn run(&self, future: Pin<Box<dyn Future<Output = Result<()>> + Send>>) -> Result<()> {
        self.core.run_in_thread(async move || future.await)?.await
    }
}

/// Whether an archive is a native title: it carries a `.daf`.
pub fn loadable_archive(files: &BTreeMap<String, Vec<u8>>) -> bool {
    files.keys().any(|name| name.to_ascii_lowercase().ends_with(".daf"))
}

/// The pieces a loaded native title runs on, for the emulator to keep.
pub struct Loaded {
    pub core: ArmCore,
    pub system: System,
}

pub fn load(platform: Box<dyn Platform>, files: BTreeMap<String, Vec<u8>>) -> Result<Loaded> {
    let (daf_name, daf) = files
        .iter()
        .find(|(name, _)| name.to_ascii_lowercase().ends_with(".daf"))
        .ok_or_else(|| WieError::FatalError("no .daf in the archive".into()))?;

    // An archive made off a handset keeps the title in a folder named for
    // it, saves and all; the title opens its saves by bare name, so the
    // folder comes off every entry.
    let (directory, file_name) = match daf_name.rsplit_once('/') {
        Some((directory, file_name)) => (Some(directory.to_owned() + "/"), file_name),
        None => (None, daf_name.as_str()),
    };
    let id = file_name.rsplit_once('.').map(|x| x.0).unwrap_or(file_name).to_string();

    tracing::info!("Loading SKT native title {id}");

    let mut image = DafImage::parse(daf)?;
    tracing::info!(
        "text {:#x}, data {:#x}, bss {:#x}, {} relocations, {} resources",
        image.text.len(),
        image.data.len(),
        image.bss_size,
        image.relocations.len(),
        image.resources.len()
    );
    image.relocate(TEXT_BASE, DATA_BASE)?;

    let mut core = ArmCore::new(false, None)?;
    write_annunciator_rows(&mut core)?;

    let system = System::new(platform, &id, &id, SktNativeTaskRunner { core: core.clone() });

    for (name, data) in &files {
        let name = match &directory {
            Some(directory) => name.strip_prefix(directory.as_str()).unwrap_or(name),
            None => name,
        };
        if name.is_empty() || name.ends_with('/') || name.to_ascii_lowercase().ends_with(".daf") {
            continue;
        }

        system.filesystem().add_virtual(name, data.clone());
    }

    core.load(&image.text, TEXT_BASE, image.text.len())?;
    core.load(&image.data, DATA_BASE, image.data.len() + image.bss_size as usize)?;

    Allocator::init(&mut core)?;

    let entries = image.entries.map(|entry| TEXT_BASE + entry);
    let resources = Arc::new(core::mem::take(&mut image.resources));

    let mut core_clone = core.clone();
    let mut system_clone = system.clone();
    system.spawn(async move || start(&mut core_clone, &mut system_clone, entries, resources).await);

    Ok(Loaded { core, system })
}

/// The title draws over the whole panel; nothing is kept back for a status
/// strip.
fn write_annunciator_rows(core: &mut ArmCore) -> Result<()> {
    wie_util::write_generic(core, wie_wipi_c::api::graphics::ANNUNCIATOR_ROWS_PTR, 0u32)
}

#[tracing::instrument(name = "start", skip_all)]
async fn start(core: &mut ArmCore, system: &mut System, entries: [u32; 6], resources: Arc<Resources>) -> Result<()> {
    let protos = [wie_midp::get_protos().into(), wie_wipi_java::get_protos().into()];
    let jvm = JvmSupport::new_jvm(system, None, Box::new(protos), &[], RustJavaJvmImplementation).await?;

    // The handler first: a stub can only be made for a category that has one.
    interface::register_svc_handler(core, SvcState::new(system.clone(), jvm.clone(), resources))?;
    interface::install(core)?;

    let context = CletContext { core: core.clone() };
    for proto in [SktClet::as_proto(), SktCletCard::as_proto()] {
        let class = ClassDefinitionImpl::from_class_proto(proto, Box::new(context.clone()) as Box<_>);
        jvm.register_class(Box::new(class), None)
            .await
            .map_err(|x| WieError::FatalError(format!("Failed to register the clet wrapper: {x:?}")))?;
    }

    for (field, index) in [
        ("startClet", START_CLET),
        ("paintClet", PAINT_CLET),
        ("handleCletEvent", HANDLE_CLET_EVENT),
    ] {
        jvm.put_static_field("net/wie/SktClet", field, "I", entries[index] as i32)
            .await
            .map_err(|x| WieError::FatalError(format!("Failed to set {field}: {x:?}")))?;
    }

    let main_class = JavaLangString::from_rust_string(&jvm, "net/wie/SktClet").await.unwrap();
    let mut args = jvm.instantiate_array("Ljava/lang/String;", 1).await.unwrap();
    jvm.store_array(&mut args, 0, vec![main_class]).await.unwrap();

    let result: JvmResult<()> = jvm
        .invoke_static("org/kwis/msp/lcdui/Main", "main", "([Ljava/lang/String;)V", (args,))
        .await;

    if let Err(x) = result {
        return Err(JvmSupport::to_wie_err(&jvm, x).await);
    }

    Ok(())
}
