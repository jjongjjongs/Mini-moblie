use core::pin::Pin;

use alloc::{borrow::ToOwned, boxed::Box, collections::BTreeMap, format, string::String, vec, vec::Vec};

use jvm::{ClassInstance, Result as JvmResult, runtime::JavaLangString};

use wie_backend::{Emulator, Event, Options, Platform, System, TaskRunner};
use wie_core_arm::{Allocator, ArmCore};
use wie_jvm_support::JvmSupport;
use wie_util::{Result, WieError};

use crate::{
    adf::{KtfAdf, find_client_bin},
    runtime::KtfJvmSupport,
};

pub const IMAGE_BASE: u32 = 0x100000;

/// What a gzip member holds, or `None` when this is not one this can read.
///
/// A download package's `.gz` entries are transport compression - the handset
/// stored them decompressed, under the name without the suffix - so a title
/// reaching for `/snd/B1.mmf` finds nothing when only `snd/B1.mmf.gz` is
/// mounted. 지크's own message for that is `I/O Exception : read null`; what
/// actually happens is a null stream and a `NullPointerException` out of the
/// paint that asked for it.
///
/// RFC 1952: a ten-byte header, optional extra/name/comment/header-CRC behind
/// its flags, the deflate stream, then a CRC32 and the uncompressed length. The
/// length is checked because it is free and catches a truncated member; the
/// CRC32 is not, so a member whose bytes rotted inside a good frame is read as
/// what it says it is.
///
/// `None` rather than an error for everything it cannot read: a file named
/// `.gz` that is not one is still mounted under the name it came with, and a
/// title that wanted those bytes is no worse off than before.
fn inflate_gzip(data: &[u8]) -> Option<Vec<u8>> {
    /// `1f 8b`, and the one compression method RFC 1952 defines.
    const MAGIC: [u8; 2] = [0x1f, 0x8b];
    const DEFLATE: u8 = 8;
    const HEADER: usize = 10;
    /// The trailer: a CRC32 and the uncompressed length.
    const TRAILER: usize = 8;

    const FHCRC: u8 = 0x02;
    const FEXTRA: u8 = 0x04;
    const FNAME: u8 = 0x08;
    const FCOMMENT: u8 = 0x10;

    if data.len() < HEADER + TRAILER || data[..2] != MAGIC || data[2] != DEFLATE {
        return None;
    }

    let flags = data[3];
    let mut at = HEADER;

    if flags & FEXTRA != 0 {
        let length = u16::from_le_bytes([*data.get(at)?, *data.get(at + 1)?]) as usize;
        at = at.checked_add(2)?.checked_add(length)?;
    }
    for flag in [FNAME, FCOMMENT] {
        if flags & flag != 0 {
            // Through the NUL that ends it.
            at += data.get(at..)?.iter().position(|byte| *byte == 0)? + 1;
        }
    }
    if flags & FHCRC != 0 {
        at = at.checked_add(2)?;
    }

    let end = data.len().checked_sub(TRAILER)?;
    let deflated = data.get(at..end)?;

    let inflated = miniz_oxide::inflate::decompress_to_vec(deflated).ok()?;

    let declared = u32::from_le_bytes(data[data.len() - 4..].try_into().ok()?);
    if inflated.len() != declared as usize {
        return None;
    }

    Some(inflated)
}

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
}

impl KtfEmulator {
    pub fn from_archive(platform: Box<dyn Platform>, files: BTreeMap<String, Vec<u8>>, options: Options) -> Result<Self> {
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
        files.contains_key("__adf__")
    }

    pub fn loadable_jar(jar: &[u8]) -> bool {
        find_client_bin(jar).is_ok()
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
        let mut core = ArmCore::new(options.enable_gdbserver, options.profile.take())?;
        let system = System::new(platform, pid, aid, KtfTaskRunner { core: core.clone() });

        for (path, data) in files {
            let path = path.trim_start_matches("P/");

            // A download package carries some of a title's files gzipped, and the
            // handset's download manager wrote them out decompressed - 지크 asks
            // for `/snd/B1.mmf`, and what the package holds is `snd/B1.mmf.gz`.
            // Mount what it asked for, and keep the packaged name as well so a
            // title that wants the bytes as they came still has them.
            if let Some(stored_as) = path.strip_suffix(".gz")
                && let Some(inflated) = inflate_gzip(data)
            {
                system.filesystem().add_virtual(stored_as, inflated);
            }

            system.filesystem().add_virtual(path, data.clone());
        }

        Allocator::init(&mut core)?;

        let mut core_clone = core.clone();
        let mut system_clone = system.clone();
        let jar_filename_clone = jar_filename.to_owned();

        system.spawn(async move || Self::start(&mut core_clone, &mut system_clone, jar_filename_clone, main_class_name).await);

        Ok(Self { core, system })
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
        })
    }
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use super::inflate_gzip;

    /// A gzip member holding `payload`, built as one stored deflate block so the
    /// test needs no compressor of its own.
    fn gzip(payload: &[u8], flags: u8, extras: &[u8]) -> Vec<u8> {
        let mut member = vec![0x1f, 0x8b, 8, flags, 0, 0, 0, 0, 0, 0xff];
        member.extend_from_slice(extras);

        // BFINAL, BTYPE = stored, then the length and its complement.
        member.push(1);
        member.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        member.extend_from_slice(&(!(payload.len() as u16)).to_le_bytes());
        member.extend_from_slice(payload);

        // The CRC32 goes unread, so this leaves it zero; the length does not.
        member.extend_from_slice(&0u32.to_le_bytes());
        member.extend_from_slice(&(payload.len() as u32).to_le_bytes());

        member
    }

    #[test]
    fn a_gzip_member_reads_back_as_what_it_holds() {
        let payload = b"MMMD\x00\x01what a sound file starts with";
        assert_eq!(inflate_gzip(&gzip(payload, 0, &[])).unwrap(), payload);

        // The optional header fields, which a packager may or may not write.
        const FHCRC: u8 = 0x02;
        const FEXTRA: u8 = 0x04;
        const FNAME: u8 = 0x08;
        const FCOMMENT: u8 = 0x10;

        assert_eq!(inflate_gzip(&gzip(payload, FNAME, b"B1.mmf\0")).unwrap(), payload);
        assert_eq!(inflate_gzip(&gzip(payload, FCOMMENT, b"packaged\0")).unwrap(), payload);
        assert_eq!(inflate_gzip(&gzip(payload, FEXTRA, &[2, 0, 0xaa, 0xbb])).unwrap(), payload);
        assert_eq!(inflate_gzip(&gzip(payload, FHCRC, &[0x12, 0x34])).unwrap(), payload);
        assert_eq!(
            inflate_gzip(&gzip(payload, FEXTRA | FNAME | FCOMMENT | FHCRC, b"\x01\x00\x99B1.mmf\0c\0\x12\x34")).unwrap(),
            payload
        );

        // An empty member is still a member.
        assert_eq!(inflate_gzip(&gzip(b"", 0, &[])).unwrap(), b"");
    }

    #[test]
    fn what_is_not_a_gzip_member_is_left_to_be_mounted_as_it_came() {
        // A file named `.gz` that is not one - the packaged `.mmf` itself, say.
        assert_eq!(inflate_gzip(b"MMMD\x00\x01not compressed at all"), None);
        assert_eq!(inflate_gzip(&[]), None);

        // The magic, but a compression method RFC 1952 does not define.
        let mut wrong_method = gzip(b"whatever", 0, &[]);
        wrong_method[2] = 9;
        assert_eq!(inflate_gzip(&wrong_method), None);

        // Cut short: the deflate stream ends before it said it would.
        let truncated = gzip(b"a payload worth its length", 0, &[]);
        assert_eq!(inflate_gzip(&truncated[..truncated.len() - 12]), None);

        // A header whose name field never ends.
        let mut unterminated = gzip(b"x", 0x08, b"B1.mmf");
        unterminated.truncate(10 + 6);
        assert_eq!(inflate_gzip(&unterminated), None);

        // Whole and readable, but not the length it declares.
        let mut lying = gzip(b"eight ch", 0, &[]);
        let end = lying.len();
        lying[end - 4..].copy_from_slice(&7u32.to_le_bytes());
        assert_eq!(inflate_gzip(&lying), None);
    }
}
