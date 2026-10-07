#![no_std]
extern crate alloc;

mod audio_sink;
pub mod billing;
pub mod canvas;
mod database;
mod executor;
pub mod gz;
mod local_network;
mod platform;
pub mod probe;
mod quirks;
mod screen;
pub mod subscriber;
mod system;
mod task;
mod task_runner;
mod time;

pub use self::{
    audio_sink::AudioSink,
    database::{Database, DatabaseRepository, RecordId},
    executor::{AsyncCallable, AsyncCallableResult},
    local_network::{
        AckEndpoint, CaptureAddress, CaptureEndpoint, DragonEyesEndpoint, Framing, FunterEndpoint, LocalConnection, LocalEndpoint, LocalNetwork,
        LocalRead, is_local_descriptor,
    },
    platform::{
        Filesystem, FilesystemMkdirError, FilesystemRenameError, FilesystemRmDirError, FilesystemSetModeError, Network, NetworkError, NetworkEvent,
        NetworkPoll, Platform,
    },
    quirks::{TitlePlatform, TitleQuirks, title_quirks, title_quirks_for_descriptor, title_quirks_on_panel},
    screen::{Screen, present, quarter_turn_left},
    system::{Event, FilesystemOverlay, InputMethodOutput, KeyCode, System},
    task::YieldFuture,
    task_runner::{DefaultTaskRunner, TaskRunner},
    time::Instant,
};

use alloc::{
    boxed::Box,
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};

use wie_util::{Result, WieError};

pub trait Emulator {
    fn handle_event(&mut self, event: Event);
    fn tick(&mut self) -> Result<()>;
    /// Whether the emulator has no work to run until a timer fires. A host that
    /// runs `tick` to a time budget can stop as soon as this is true and sleep
    /// the remainder instead of busy-waiting. The default is a conservative
    /// `false` (never idle), so a host keeps its existing run-to-budget
    /// behaviour for any emulator that does not report idleness.
    fn is_idle(&self) -> bool {
        false
    }

    /// How long the host may sleep before this emulator has work again, in
    /// milliseconds, when [`is_idle`](Self::is_idle) is true.
    ///
    /// A host that polls a fixed interval pays one poll for every interval a
    /// title's timer spans; told how long the wait actually is, it can wake
    /// once, on time. `None` means "keep to your own interval", which is the
    /// default and what an emulator that does not track wake-ups reports.
    fn sleep_hint(&self) -> Option<u64> {
        None
    }
}

pub struct ProfileSample {
    /// Leaf-first call stack: [pc, lr, lr_prev, ...].
    pub stack: Vec<u32>,
    pub count: u64,
}

/// Called periodically during execution with a batch of samples that the
/// profiler accumulated since the previous flush. The callback also fires once
/// more when the runtime shuts down to drain anything still in the buffer.
pub type ProfileCallback = Box<dyn FnMut(Vec<ProfileSample>) + Send + Sync>;

pub struct Options {
    pub enable_gdbserver: bool,
    pub profile: Option<ProfileCallback>,
    /// Whether the handset shows its status strip ("annunciator") above the
    /// drawing area a title is given. On the reference this is runtime state the
    /// common UI turns on and off, and a title inherits whatever it is when it
    /// asks for the screen framebuffer - so titles are written for one or the
    /// other and cannot both be served at once.
    ///
    /// `None` lets the platform decide from the title, which is what a handset
    /// effectively does for anyone who never touches the setting; `Some` forces
    /// it either way.
    pub annunciator: Option<bool>,
}

/// What a download is wrapped in, when it is not the title itself.
///
/// A handset's download could be delivered under OMA DRM, and what the store
/// handed over is then a container: the title's own bytes are inside it, and
/// the key that unlocks them is in a rights object issued to one handset, not
/// in the file. Nothing here can open one - this only recognises it, so a
/// player can say so instead of reporting a broken archive as if the title were
/// at fault.
///
/// The container is OMA's DRM Content Format: `odcf`, then an `odrm` box
/// holding the headers that describe the content and the content itself. Only
/// the two markers are read; what is inside is not ours to interpret.
pub fn protected_container(data: &[u8]) -> Option<&'static str> {
    (data.get(..4)? == b"odcf" && data.get(12..16)? == b"odrm").then_some("OMA DRM (DCF)")
}

/// What a protected container is, told enough about to point the player at the
/// edition that will run.
pub struct DrmContainer {
    /// A human name for the format - the one [`protected_container`] gives.
    pub format: &'static str,
    /// The content id written in the clear in the header, when there is one.
    ///
    /// A WIPI DCF carries one like `00WIPI000000000001036D08`, whose last eight
    /// characters are the title's own id. The non-DRM edition of the same title
    /// files under that id as its AID, so it is the edition a player should look
    /// for - see [`DrmContainer::edition_aid`].
    pub content_id: Option<String>,
}

impl DrmContainer {
    /// The AID a non-DRM edition of this title carries, read off the tail of
    /// the content id. `None` when the id is missing or does not end in an
    /// eight-character hex id.
    pub fn edition_aid(&self) -> Option<&str> {
        let id = self.content_id.as_deref()?;
        let tail = id.get(id.len().checked_sub(8)?..)?;

        tail.bytes().all(|b| b.is_ascii_hexdigit()).then_some(tail)
    }
}

/// Reads a protected container's header, without opening or decrypting it.
///
/// Only the clear-text header is looked at, and only the region before the
/// encrypted payload box (`odda`) is scanned, bounded so a malformed file
/// cannot make this walk far. The content id is lifted out as the ASCII token
/// around its `WIPI` marker; nothing here interprets, unwraps or unlocks the
/// content itself.
pub fn drm_container(data: &[u8]) -> Option<DrmContainer> {
    let format = protected_container(data)?;

    Some(DrmContainer {
        format,
        content_id: dcf_content_id(data),
    })
}

/// The clear-text content id of a DCF, or `None`.
///
/// The id sits in the header before the `odda` payload box and, for a WIPI
/// title, holds `WIPI`. The header is scanned for that marker and the ASCII
/// alphanumeric run around it is returned, both ends bounded so untrusted
/// bytes cannot drive this past the header.
fn dcf_content_id(data: &[u8]) -> Option<String> {
    const SCAN_LIMIT: usize = 8192;

    let payload = data.windows(4).position(|w| w == b"odda").unwrap_or(data.len());
    let header = &data[..payload.min(SCAN_LIMIT).min(data.len())];

    let marker = header.windows(4).position(|w| w == b"WIPI")?;

    let is_token = |b: u8| b.is_ascii_alphanumeric();
    let mut start = marker;
    while start > 0 && is_token(header[start - 1]) {
        start -= 1;
    }
    let mut end = marker + 4;
    while end < header.len() && is_token(header[end]) {
        end += 1;
    }

    // A lone "WIPI" with nothing around it is not an id worth reporting.
    (end - start > 4).then(|| String::from_utf8_lossy(&header[start..end]).into_owned())
}

/// The names of an archive's entries, without unpacking any of them.
///
/// Only the central directory is read, so this costs nothing next to
/// [`extract_zip`] - which is the point: finding one entry by name should not
/// mean decompressing nine hundred others.
pub fn zip_entry_names(zip: &[u8]) -> Result<Vec<String>> {
    extern crate std; // XXX

    use std::io::Cursor;
    use zip::ZipArchive;

    // The same re-read `extract_zip` does, and for the same reason.
    let patched;
    let zip = match ZipArchive::new(Cursor::new(zip)) {
        Ok(_) => zip,
        Err(error) => {
            patched = drop_unicode_path_fields(zip).ok_or_else(|| WieError::FatalError(format!("Invalid zip archive: {error}")))?;

            &patched
        }
    };

    let archive = ZipArchive::new(Cursor::new(zip)).map_err(|x| WieError::FatalError(format!("Invalid zip archive: {x}")))?;

    Ok(archive.file_names().map(String::from).collect())
}

pub fn extract_zip(zip: &[u8]) -> Result<BTreeMap<String, Vec<u8>>> {
    extern crate std; // XXX

    use std::io::{Cursor, Read};
    use zip::ZipArchive;

    // Korean handset archives carry filenames in EUC-KR alongside an Info-ZIP
    // Unicode Path extra field whose checksum does not match, because the name
    // it was computed over is not the one in the header. That is fatal to a
    // strict reader, so the field is dropped and the archive read again.
    let patched;
    let zip = match ZipArchive::new(Cursor::new(zip)) {
        Ok(_) => zip,
        Err(error) => {
            patched = drop_unicode_path_fields(zip).ok_or_else(|| WieError::FatalError(format!("Invalid zip archive: {error}")))?;

            tracing::warn!("Rereading archive without its Unicode path fields: {error}");

            &patched
        }
    };

    let mut archive = ZipArchive::new(Cursor::new(zip)).map_err(|x| WieError::FatalError(format!("Invalid zip archive: {x}")))?;

    (0..archive.len())
        .filter_map(|x| {
            let mut file = match archive.by_index(x) {
                Ok(file) => file,
                Err(err) => return Some(Err(WieError::FatalError(format!("Failed to read zip entry {x}: {err}")))),
            };
            if !file.is_file() {
                return None;
            }

            let mut data = Vec::new();
            if let Err(err) = file.read_to_end(&mut data) {
                return Some(Err(WieError::FatalError(format!("Failed to read zip entry {}: {err}", file.name()))));
            }

            Some(Ok((file.name().to_string(), data)))
        })
        .collect::<Result<_>>()
        .map(strip_common_directory)
}

/// Info-ZIP Unicode Path, the extra field whose checksum these archives get
/// wrong.
const EXTRA_FIELD_UNICODE_PATH: u16 = 0x7075;

/// Header id no writer assigns, so a reader skips the field instead of
/// checking it.
const EXTRA_FIELD_IGNORED: u16 = 0x9999;

const LOCAL_HEADER_SIGNATURE: u32 = 0x0403_4b50;
const CENTRAL_HEADER_SIGNATURE: u32 = 0x0201_4b50;

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(data.get(offset..offset + 2)?.try_into().ok()?))
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(offset..offset + 4)?.try_into().ok()?))
}

/// Renames every Unicode Path extra field so a reader ignores it.
///
/// The edit is in place and the same length, so nothing an archive records
/// about where its entries are moves. `None` if the bytes do not walk cleanly
/// as a zip, in which case the caller reports the original error.
fn drop_unicode_path_fields(zip: &[u8]) -> Option<Vec<u8>> {
    let mut patched = zip.to_vec();

    // Walk the local headers, which sit at the front and are self describing
    // as long as none of them is corrupt.
    let mut offset = 0;
    while read_u32(&patched, offset) == Some(LOCAL_HEADER_SIGNATURE) {
        let compressed_size = read_u32(&patched, offset + 18)? as usize;
        let name_length = read_u16(&patched, offset + 26)? as usize;
        let extra_length = read_u16(&patched, offset + 28)? as usize;
        let extra = offset + 30 + name_length;

        patch_extra_field(&mut patched, extra, extra_length)?;

        offset = extra + extra_length + compressed_size;
    }

    // Then the central directory, wherever the walk left off.
    while read_u32(&patched, offset) == Some(CENTRAL_HEADER_SIGNATURE) {
        let name_length = read_u16(&patched, offset + 28)? as usize;
        let extra_length = read_u16(&patched, offset + 30)? as usize;
        let comment_length = read_u16(&patched, offset + 32)? as usize;
        let extra = offset + 46 + name_length;

        patch_extra_field(&mut patched, extra, extra_length)?;

        offset = extra + extra_length + comment_length;
    }

    Some(patched)
}

/// Rewrites the ids in one extra field block.
fn patch_extra_field(data: &mut [u8], start: usize, length: usize) -> Option<()> {
    let mut offset = start;
    let end = start + length;

    while offset + 4 <= end {
        let id = read_u16(data, offset)?;
        let size = read_u16(data, offset + 2)? as usize;

        if id == EXTRA_FIELD_UNICODE_PATH {
            data.get_mut(offset..offset + 2)?.copy_from_slice(&EXTRA_FIELD_IGNORED.to_le_bytes());
        }

        offset += 4 + size;
    }

    Some(())
}

/// Feature phone archives are sometimes repacked with every entry below a
/// single directory (`P/app_info`, `0002A4B1/app_info`, ...). The loaders all
/// expect the app descriptor at the archive root, so drop a leading directory
/// component when *every* entry shares it.
fn strip_common_directory(files: BTreeMap<String, Vec<u8>>) -> BTreeMap<String, Vec<u8>> {
    let mut prefix: Option<&str> = None;

    for name in files.keys() {
        let Some((directory, rest)) = name.split_once('/') else {
            return files;
        };
        if rest.is_empty() {
            return files;
        }
        match prefix {
            Some(prefix) if prefix != directory => return files,
            Some(_) => {}
            None => prefix = Some(directory),
        }
    }

    let Some(prefix) = prefix.map(|x| x.to_string()) else {
        return files;
    };

    tracing::debug!("Stripping common archive directory {prefix}/");

    files
        .into_iter()
        .map(|(name, data)| (name[prefix.len() + 1..].to_string(), data))
        .collect()
}

#[cfg(test)]
mod tests {
    use alloc::{
        collections::BTreeMap,
        string::{String, ToString},
        vec::Vec,
    };

    use super::strip_common_directory;

    fn archive(names: &[&str]) -> BTreeMap<String, Vec<u8>> {
        names.iter().map(|x| (x.to_string(), Vec::new())).collect()
    }

    #[test]
    fn strips_shared_directory() {
        let files = strip_common_directory(archive(&["P/app_info", "P/0002A4B1.jar"]));

        assert!(files.contains_key("app_info"));
        assert!(files.contains_key("0002A4B1.jar"));
    }

    #[test]
    fn keeps_root_entries() {
        let files = strip_common_directory(archive(&["app_info", "res/0.png"]));

        assert!(files.contains_key("app_info"));
        assert!(files.contains_key("res/0.png"));
    }

    #[test]
    fn keeps_multiple_directories() {
        let files = strip_common_directory(archive(&["a/app_info", "b/0002A4B1.jar"]));

        assert!(files.contains_key("a/app_info"));
        assert!(files.contains_key("b/0002A4B1.jar"));
    }
}

#[cfg(test)]
mod protected_container_tests {
    use super::protected_container;

    /// The head of 화이트데이's download, which is a container and not the jar
    /// its name says it is.
    #[test]
    fn an_oma_container_is_recognised_by_its_two_markers() {
        let mut data = *b"odcf\x00\x02\x00\x00\x00\x00\x00\x01odrm";

        assert_eq!(protected_container(&data), Some("OMA DRM (DCF)"));

        data[13] = b'x';
        assert_eq!(protected_container(&data), None);
    }

    #[test]
    fn an_ordinary_archive_is_not_a_container() {
        assert_eq!(protected_container(b"PK\x03\x04and then a jar"), None);
        assert_eq!(protected_container(b"odcf"), None);
        assert_eq!(protected_container(b""), None);
    }

    /// 절묘한타이밍 01.00.05's header: the clear-text content id is read out, and
    /// its last eight characters are the AID the non-DRM edition files under.
    #[test]
    fn a_container_hands_back_its_content_id_and_the_edition_aid() {
        // The header as a WIPI DCF lays it out: the content id field is
        // NUL-terminated (its length counts the NUL), and the rights-issuer URL
        // and textual headers follow, so the id ends at the first NUL.
        let mut data = b"odcf\x00\x02\x00\x00\x00\x00\x00\x01odrmodheohdr".to_vec();
        data.extend_from_slice(b"\x01\x02\x00\x00\x00\x00\x00\x00\x00\x00\x87\xda");
        data.extend_from_slice(b"\x00\x19\x00\x01\x00\x0c");
        data.extend_from_slice(b"00WIPI000000000001036D08\x00");
        data.extend_from_slice(b"\x00");
        data.extend_from_slice(b"ContentURL:\x00");
        data.extend_from_slice(b"oddathen the encrypted bytes WIPI never mind these");

        let container = super::drm_container(&data).unwrap();
        assert_eq!(container.content_id.as_deref(), Some("00WIPI000000000001036D08"));
        assert_eq!(container.edition_aid(), Some("01036D08"));
    }

    /// A container with no readable id still recognises as one, and names no
    /// edition rather than guessing.
    #[test]
    fn a_container_without_a_content_id_names_no_edition() {
        let data = b"odcf\x00\x02\x00\x00\x00\x00\x00\x01odrmodheohdr\x01\x02\x00".to_vec();

        let container = super::drm_container(&data).unwrap();
        assert_eq!(container.content_id, None);
        assert_eq!(container.edition_aid(), None);
    }

    /// The payload is never scanned for the id: a `WIPI` that only turns up
    /// inside the encrypted `odda` bytes is not read as a content id.
    #[test]
    fn the_encrypted_payload_is_not_scanned_for_an_id() {
        let mut data = b"odcf\x00\x02\x00\x00\x00\x00\x00\x01odrmodheohdr\x01\x02\x00".to_vec();
        data.extend_from_slice(b"odda");
        data.extend_from_slice(b"00WIPI000000000009999999");

        assert_eq!(super::drm_container(&data).unwrap().content_id, None);
    }

    #[test]
    fn an_ordinary_archive_has_no_drm_header() {
        assert!(super::drm_container(b"PK\x03\x04and then a jar").is_none());
    }
}
