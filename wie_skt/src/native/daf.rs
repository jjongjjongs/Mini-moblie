//! SKT's native WIPI-C executable, the `.daf` a handset kept beside its `.mod`.
//!
//! Nothing documents it; the layout here is read off the files themselves.
//! Every word is little-endian.
//!
//! ```text
//! 0x00  u32 file header size (0x20)
//! 0x08  16 bytes, the same as the `.mod`'s - a digest of the download
//! 0x20  the image header, its own size in its third word:
//!       +0x00 build time        +0x08 header size (0x50 or 0x6c)
//!       +0x0c flags             +0x1c text size     +0x20 data size
//!       +0x24 bss size          +0x28 relocation size
//!       +0x30 resource size     +0x38 six entry points
//!       +0x50 compressed text size, in the 0x6c header only
//! ```
//!
//! The body follows the image header: text, data, relocations, resources, in
//! that order. The longer `0x6c` header is a title whose text is deflated -
//! its first extra word, at `+0x50`, is how many bytes of zlib stream stand
//! in for the text, and everything after is as it is in the shorter one.
//!
//! The image is linked with text at zero and data at `0x0100_1000`, bss
//! straight after data. Data opens with the word `0xcafecafe`, which the
//! loader overwrites with the pointer a title reaches the platform through -
//! see the runtime.
//!
//! The six entry points are text offsets, Thumb functions with the low bit set:
//! `startClet`, a clean-up the title runs before it goes away, the two halves
//! of a pause (each sending the title an event of its own), `paintClet`, and
//! `handleCletEvent`.

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

use wie_util::{Result, WieError};

/// Where the image's data is linked, and so where it has to be loaded.
pub const DATA_BASE: u32 = 0x0100_1000;

const FILE_HEADER_SIZE_FIELD: usize = 0x00;
const IMAGE_HEADER_SIZE_FIELD: usize = 0x08;
const TEXT_SIZE_FIELD: usize = 0x1c;
const DATA_SIZE_FIELD: usize = 0x20;
const BSS_SIZE_FIELD: usize = 0x24;
const RELOCATION_SIZE_FIELD: usize = 0x28;
const RESOURCE_SIZE_FIELD: usize = 0x30;
const ENTRIES_FIELD: usize = 0x38;
const ENTRY_COUNT: usize = 6;
/// Only in the longer header: the length of the zlib stream the text is
/// stored as.
const COMPRESSED_TEXT_SIZE_FIELD: usize = 0x50;

/// What a relocated word has to have added to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelocationKind {
    /// An address in text: add where text was loaded.
    Text,
    /// An address in data or bss: add how far data moved from `DATA_BASE`.
    Data,
    /// A text address kept as its distance from the word itself, which the
    /// loader turns into the address: add the word's own loaded address. The
    /// ARM veneers' `ldr pc` literals are the ones written this way.
    SelfRelative,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Relocation {
    /// Offset of the word from the start of text; data follows text.
    pub offset: u32,
    pub kind: RelocationKind,
}

pub struct DafImage {
    pub text: Vec<u8>,
    pub data: Vec<u8>,
    pub bss_size: u32,
    pub relocations: Vec<Relocation>,
    /// Text offsets, the Thumb bit set.
    pub entries: [u32; ENTRY_COUNT],
    pub resources: Resources,
}

impl DafImage {
    pub fn parse(file: &[u8]) -> Result<Self> {
        let file_header_size = read_u32(file, FILE_HEADER_SIZE_FIELD)? as usize;
        let header = file
            .get(file_header_size..)
            .ok_or_else(|| WieError::FatalError("daf: no image header".into()))?;
        let header_size = read_u32(header, IMAGE_HEADER_SIZE_FIELD)? as usize;

        let text_size = read_u32(header, TEXT_SIZE_FIELD)? as usize;
        let data_size = read_u32(header, DATA_SIZE_FIELD)? as usize;
        let bss_size = read_u32(header, BSS_SIZE_FIELD)?;
        let relocation_size = read_u32(header, RELOCATION_SIZE_FIELD)? as usize;
        let resource_size = read_u32(header, RESOURCE_SIZE_FIELD)? as usize;

        let mut entries = [0; ENTRY_COUNT];
        for (index, entry) in entries.iter_mut().enumerate() {
            *entry = read_u32(header, ENTRIES_FIELD + index * 4)?;
        }

        let body = header
            .get(header_size..)
            .ok_or_else(|| WieError::FatalError("daf: header runs past the file".into()))?;

        let mut cursor = 0;
        let mut take = |size: usize, what: &str| -> Result<&[u8]> {
            let part = body
                .get(cursor..cursor + size)
                .ok_or_else(|| WieError::FatalError(format!("daf: {what} runs past the body")))?;
            cursor += size;
            Ok(part)
        };

        let text = if header_size > COMPRESSED_TEXT_SIZE_FIELD {
            let compressed = take(read_u32(header, COMPRESSED_TEXT_SIZE_FIELD)? as usize, "compressed text")?;
            let text =
                miniz_oxide::inflate::decompress_to_vec_zlib(compressed).map_err(|x| WieError::FatalError(format!("daf: inflate failed: {x:?}")))?;
            if text.len() != text_size {
                return Err(WieError::FatalError(format!(
                    "daf: text inflated to {:#x} bytes, not {text_size:#x}",
                    text.len()
                )));
            }
            text
        } else {
            take(text_size, "text")?.to_vec()
        };
        let data = take(data_size, "data")?.to_vec();
        let relocations = decode_relocations(take(relocation_size, "relocations")?, text_size as u32)?;
        // A file cut short after its code still runs; it simply has no
        // resources to hand out.
        let resources = take(resource_size, "resources")
            .map(Resources::parse)
            .unwrap_or_else(|_| Ok(Resources::default()))?;

        Ok(Self {
            text,
            data,
            bss_size,
            relocations,
            entries,
            resources,
        })
    }

    /// Rewrites the relocated words of `text` and `data` for an image whose
    /// text is at `text_base` and whose data is at `data_base`.
    pub fn relocate(&mut self, text_base: u32, data_base: u32) -> Result<()> {
        let text_size = self.text.len() as u32;
        let data_bias = data_base.wrapping_sub(DATA_BASE);

        for relocation in &self.relocations {
            let (segment, offset, place) = if relocation.offset < text_size {
                (&mut self.text, relocation.offset, text_base.wrapping_add(relocation.offset))
            } else {
                let offset = relocation.offset - text_size;
                (&mut self.data, offset, data_base.wrapping_add(offset))
            };

            let offset = offset as usize;
            // 도적편's data stream ends one word past its data, on the first
            // word of bss: a word that is zero until the title writes it, so
            // there is nothing there to fix up.
            let Some(word) = segment.get(offset..offset + 4) else {
                tracing::debug!("daf: relocation at {:#x} is past the image; skipped", relocation.offset);
                continue;
            };
            let value = u32::from_le_bytes(word.try_into().unwrap());

            let value = match relocation.kind {
                RelocationKind::Text => value.wrapping_add(text_base),
                RelocationKind::Data => value.wrapping_add(data_bias),
                RelocationKind::SelfRelative => value.wrapping_add(place),
            };

            segment[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }

        Ok(())
    }
}

/// Decodes the relocation block: two lengths, then a bit stream that many
/// bytes long for the words in text and, straight after it, one for the words
/// in data. Each is big-endian, one record per relocated word:
///
/// ```text
/// [2 bits kind][4 bits k][delta bits, if any][2 bits, always 0]
///
/// k 1..=7    the word is k words past the previous one
/// k 8..=11   ((k - 8) << 8 | next 8 bits) words past it
/// k 12       the next 16 bits words past it
/// k 0        the next 24 bits words past it
///
/// kind 0     a text address      kind 1  a data or bss address
/// kind 2     self-relative
/// ```
///
/// Each stream counts from the start of its own segment. 해적편 keeps no
/// pointers in its data and has an empty second stream; 0087472568 keeps a
/// table of function pointers there, and with the stream unread it called
/// one at its unrelocated address.
///
/// The offsets answered are from the start of text, data following text.
pub fn decode_relocations(block: &[u8], text_size: u32) -> Result<Vec<Relocation>> {
    if block.len() < 8 {
        return Ok(Vec::new());
    }

    let text_length = read_u32(block, 0)? as usize;
    let data_length = read_u32(block, 4)? as usize;
    let text_stream = block
        .get(8..8 + text_length)
        .ok_or_else(|| WieError::FatalError("daf: text relocations run past their block".into()))?;
    let data_stream = block
        .get(8 + text_length..8 + text_length + data_length)
        .ok_or_else(|| WieError::FatalError("daf: data relocations run past their block".into()))?;

    let mut relocations = decode_stream(text_stream, 0)?;
    relocations.extend(decode_stream(data_stream, text_size)?);

    Ok(relocations)
}

fn decode_stream(stream: &[u8], base: u32) -> Result<Vec<Relocation>> {
    let mut reader = BitReader { bytes: stream, position: 0 };
    let mut offset = 0u32;
    let mut relocations = Vec::new();

    // A record is at least a byte; whatever is left after the last one is
    // padding.
    while reader.remaining() >= 8 {
        let kind = reader.read(2)?;
        let k = reader.read(4)?;
        let delta = match k {
            1..=7 => k,
            8..=11 => ((k - 8) << 8) | reader.read(8)?,
            12 => reader.read(16)?,
            0 => reader.read(24)?,
            _ => return Err(WieError::FatalError(format!("daf: relocation record with k = {k}"))),
        };
        let _ = reader.read(2)?;

        offset = offset.wrapping_add(delta * 4);
        let kind = match kind {
            0 => RelocationKind::Text,
            1 => RelocationKind::Data,
            2 => RelocationKind::SelfRelative,
            _ => return Err(WieError::FatalError(format!("daf: relocation of kind {kind} at {offset:#x}"))),
        };

        relocations.push(Relocation { offset: base + offset, kind });
    }

    Ok(relocations)
}

struct BitReader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl BitReader<'_> {
    fn remaining(&self) -> usize {
        self.bytes.len() * 8 - self.position
    }

    fn read(&mut self, count: usize) -> Result<u32> {
        if count > self.remaining() {
            return Err(WieError::FatalError("daf: relocation stream ends inside a record".into()));
        }

        let mut value = 0;
        for _ in 0..count {
            let byte = self.bytes[self.position / 8];
            let bit = (byte >> (7 - self.position % 8)) & 1;
            value = (value << 1) | bit as u32;
            self.position += 1;
        }

        Ok(value)
    }
}

/// The files a title ships inside its own executable - what
/// `MC_knlGetResourceID` and `MC_knlGetResource` hand out.
///
/// ```text
/// 0x00  u32 header size (0x10)      0x04  u32 1
/// 0x08  u32 0x10                    0x0c  u32 ? (a size, never needed)
/// 0x10  u32 size of everything from 0x10 to the first file's bytes
/// 0x14  u32 0                       0x18  u32 count
/// 0x1c  count * { u32 offset, u32 size }, offsets from the first file
///       then count NUL-terminated names, in the same order
/// ```
#[derive(Default)]
pub struct Resources {
    /// Name, offset and size, in directory order: a file's place here is the
    /// id the handset hands out for it - see [`Resources::index`].
    files: Vec<(String, usize, usize)>,
    data: Vec<u8>,
}

impl Resources {
    fn parse(block: &[u8]) -> Result<Self> {
        if block.len() < 0x1c {
            return Ok(Self::default());
        }

        let header_size = read_u32(block, 0x00)? as usize;
        let directory_size = read_u32(block, 0x10)? as usize;
        let count = read_u32(block, 0x18)? as usize;
        let data_start = header_size + directory_size;

        let mut names = block.get(0x1c + count * 8..).unwrap_or_default().split(|x| *x == 0);
        let mut files = Vec::new();
        for index in 0..count {
            let offset = read_u32(block, 0x1c + index * 8)? as usize;
            let size = read_u32(block, 0x1c + index * 8 + 4)? as usize;
            let Some(name) = names.next() else {
                break;
            };

            let name = String::from_utf8_lossy(name).to_string();
            files.push((name, data_start + offset, size));
        }

        Ok(Self { files, data: block.to_vec() })
    }

    /// The bytes of the named file, matched the way the handset's
    /// `MC_knlGetResourceID` matched: with or without the leading `/` the
    /// directory keeps, and without regard to case.
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        let (_, offset, size) = &self.files[self.index(name)?];

        self.data.get(*offset..*offset + *size)
    }

    /// The named file's place in the directory, which is the id the handset's
    /// `MC_knlGetResourceID` answers - matched as [`Resources::get`] matches.
    pub fn index(&self, name: &str) -> Option<usize> {
        let wanted = name.trim_start_matches('/');

        self.files
            .iter()
            .position(|(file, _, _)| file.trim_start_matches('/').eq_ignore_ascii_case(wanted))
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    bytes
        .get(offset..offset + 4)
        .map(|x| u32::from_le_bytes(x.try_into().unwrap()))
        .ok_or_else(|| WieError::FatalError(format!("daf: no word at {offset:#x}")))
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use super::{DafImage, Relocation, RelocationKind, Resources, decode_relocations};

    fn block(text: &[u8], data: &[u8]) -> Vec<u8> {
        let mut block = (text.len() as u32).to_le_bytes().to_vec();
        block.extend_from_slice(&(data.len() as u32).to_le_bytes());
        block.extend_from_slice(text);
        block.extend_from_slice(data);
        block
    }

    /// The opening of 메이플스토리 해적편's relocations, checked against the
    /// words it lands on: a data address at `0x414` and `0x418`, a text address
    /// at `0x41c` and `0x420`, then a text address `0x208` bytes on.
    #[test]
    fn short_and_long_records_count_on_from_the_last() {
        let relocations = decode_relocations(&block(&[0x64, 0x14, 0x44, 0x04, 0x04, 0x22, 0x08], &[]), 0x1000).unwrap();

        assert_eq!(
            relocations,
            vec![
                Relocation {
                    offset: 0x414,
                    kind: RelocationKind::Data
                },
                Relocation {
                    offset: 0x418,
                    kind: RelocationKind::Data
                },
                Relocation {
                    offset: 0x41c,
                    kind: RelocationKind::Text
                },
                Relocation {
                    offset: 0x420,
                    kind: RelocationKind::Text
                },
                Relocation {
                    offset: 0x628,
                    kind: RelocationKind::Text
                },
            ]
        );
    }

    /// The two wider forms: k = 12 carries sixteen bits and k = 0
    /// twenty-four. The second record is the one 해적편 reaches its veneer
    /// literals with, byte for byte.
    #[test]
    fn wide_records_carry_sixteen_and_twenty_four_bits() {
        let relocations = decode_relocations(&block(&[0x70, 0x1d, 0x48, 0x80, 0x00, 0x0b, 0xd8], &[]), 0x10_0000).unwrap();

        assert_eq!(
            relocations,
            vec![
                Relocation {
                    offset: 0x0752 * 4,
                    kind: RelocationKind::Data
                },
                Relocation {
                    offset: (0x0752 + 0x2f6) * 4,
                    kind: RelocationKind::SelfRelative
                },
            ]
        );
    }

    /// The second stream counts from the start of data: 0087472568's opens with
    /// two text addresses at data `+0x1518` and `+0x151c`.
    #[test]
    fn the_data_stream_counts_from_the_start_of_data() {
        let relocations = decode_relocations(&block(&[0x04], &[0x30, 0x15, 0x18, 0x04]), 0x2000).unwrap();

        assert_eq!(
            relocations,
            vec![
                Relocation {
                    offset: 4,
                    kind: RelocationKind::Text
                },
                Relocation {
                    offset: 0x2000 + 0x1518,
                    kind: RelocationKind::Text
                },
                Relocation {
                    offset: 0x2000 + 0x151c,
                    kind: RelocationKind::Text
                },
            ]
        );
    }

    #[test]
    fn relocation_adds_the_right_base_to_each_kind() {
        let mut image = DafImage {
            text: [0x40u32, 0x0100_1010, 0xffff_fff0].iter().flat_map(|x| x.to_le_bytes()).collect(),
            data: [0x0100_1020u32].iter().flat_map(|x| x.to_le_bytes()).collect(),
            bss_size: 0,
            relocations: vec![
                Relocation {
                    offset: 0,
                    kind: RelocationKind::Text,
                },
                Relocation {
                    offset: 4,
                    kind: RelocationKind::Data,
                },
                Relocation {
                    offset: 8,
                    kind: RelocationKind::SelfRelative,
                },
                Relocation {
                    offset: 12,
                    kind: RelocationKind::Data,
                },
            ],
            entries: [0; 6],
            resources: Resources::default(),
        };

        image.relocate(0x10_0000, 0x0200_0000).unwrap();

        let word = |bytes: &[u8], at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        assert_eq!(word(&image.text, 0), 0x10_0040);
        assert_eq!(word(&image.text, 4), 0x0200_0010);
        // Sixteen bytes back from the word at text + 8.
        assert_eq!(word(&image.text, 8), 0x10_0008 - 0x10);
        assert_eq!(word(&image.data, 0), 0x0200_0020);
    }

    #[test]
    fn resources_are_found_with_or_without_the_slash() {
        let names = b"/a.dat\0/B.dat\0";
        let directory_size = 0x0c + 8 * 2 + names.len() as u32;

        let mut block = Vec::new();
        for word in [0x10u32, 1, 0x10, 0, directory_size, 0, 2, 0, 3, 3, 2] {
            block.extend_from_slice(&word.to_le_bytes());
        }
        block.extend_from_slice(names);
        block.extend_from_slice(b"abcde");

        let resources = Resources::parse(&block).unwrap();

        assert_eq!(resources.len(), 2);
        assert_eq!(resources.get("a.dat"), Some(&b"abc"[..]));
        assert_eq!(resources.get("/b.dat"), Some(&b"de"[..]));
        assert_eq!(resources.get("c.dat"), None);
    }
}
