//! ALZip archives (`.alz`), the format Korean users packed handset dumps in.
//!
//! A title passed around as an `.alz` is the same files a `.zip` would carry,
//! so it is read into the same map and handed to the same loaders. The format
//! is a run of local entries and nothing else worth reading:
//!
//! - `ALZ\x01` and four more bytes, once;
//! - per entry, `BLZ\x01`, the name's length (u16), attributes (u8), a DOS
//!   time (u32), a descriptor (u8) and one byte more; then, when the
//!   descriptor's high nibble - the width of the two sizes - is not zero, the
//!   method (u8), one byte, the CRC-32 (u32), and the compressed and stored
//!   sizes at that width; then the name and the data;
//! - `CLZ\x01` where the entries end.
//!
//! Names are EUC-KR, all numbers little-endian. Stored and deflated entries
//! are read; an encrypted one, or one packed with bzip2 or ALZip's own method,
//! is reported rather than skipped, so the player is told why the title does
//! not start.

use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};

use wie_util::{Result, WieError};

const ARCHIVE_SIGNATURE: &[u8; 4] = b"ALZ\x01";
const ENTRY_SIGNATURE: &[u8; 4] = b"BLZ\x01";

const ATTRIBUTE_DIRECTORY: u8 = 0x10;
const DESCRIPTOR_ENCRYPTED: u8 = 0x01;

const METHOD_STORED: u8 = 0;
const METHOD_BZIP2: u8 = 1;
const METHOD_DEFLATE: u8 = 2;

/// Whether `data` is an ALZip archive.
pub fn is_alz(data: &[u8]) -> bool {
    data.starts_with(ARCHIVE_SIGNATURE)
}

/// Every file in the archive by its name, `/` between folders, the same as
/// `extract_zip` answers for a zip.
pub fn extract_alz(data: &[u8]) -> Result<BTreeMap<String, Vec<u8>>> {
    let invalid = |what: &str| WieError::FatalError(format!("Invalid alz archive: {what}"));

    let mut files = BTreeMap::new();
    let mut offset = 8;
    while data.get(offset..offset + 4) == Some(ENTRY_SIGNATURE.as_slice()) {
        let header = data.get(offset..offset + 13).ok_or_else(|| invalid("entry header cut short"))?;
        let name_length = u16::from_le_bytes([header[4], header[5]]) as usize;
        let attributes = header[6];
        let descriptor = header[11];
        let size_width = (descriptor >> 4) as usize;
        offset += 13;

        let (method, crc, compressed, stored) = if size_width == 0 {
            (METHOD_STORED, 0, 0, 0)
        } else {
            if !matches!(size_width, 1 | 2 | 4 | 8) {
                return Err(invalid("unknown size width"));
            }
            let sizes = data
                .get(offset..offset + 6 + 2 * size_width)
                .ok_or_else(|| invalid("entry sizes cut short"))?;
            let method = sizes[0];
            let crc = u32::from_le_bytes([sizes[2], sizes[3], sizes[4], sizes[5]]);
            let compressed = read_width(&sizes[6..6 + size_width]);
            let stored = read_width(&sizes[6 + size_width..]);
            offset += 6 + 2 * size_width;

            (method, crc, compressed, stored)
        };

        let name = data.get(offset..offset + name_length).ok_or_else(|| invalid("entry name cut short"))?;
        let (name, _, _) = encoding_rs::EUC_KR.decode(name);
        let name = name.replace('\\', "/");
        offset += name_length;

        let compressed = usize::try_from(compressed).map_err(|_| invalid("entry too large"))?;
        let body = data.get(offset..offset + compressed).ok_or_else(|| invalid("entry data cut short"))?;
        offset += compressed;

        if attributes & ATTRIBUTE_DIRECTORY != 0 || name.ends_with('/') || size_width == 0 && name_length == 0 {
            continue;
        }
        if descriptor & DESCRIPTOR_ENCRYPTED != 0 {
            return Err(WieError::FatalError(format!("{name}: the alz archive is password protected")));
        }

        let contents = match method {
            METHOD_STORED => body.to_vec(),
            METHOD_DEFLATE => {
                miniz_oxide::inflate::decompress_to_vec(body).map_err(|error| invalid(&format!("{name} does not inflate: {error:?}")))?
            }
            METHOD_BZIP2 => return Err(WieError::FatalError(format!("{name}: bzip2 alz entries are not supported"))),
            _ => return Err(WieError::FatalError(format!("{name}: alz method {method} is not supported"))),
        };
        if contents.len() as u64 != stored || crc32(&contents) != crc {
            return Err(invalid(&format!("{name} does not match its checksum")));
        }

        files.insert(name.to_string(), contents);
    }

    if files.is_empty() {
        return Err(invalid("no entries"));
    }

    Ok(files)
}

fn read_width(bytes: &[u8]) -> u64 {
    bytes.iter().rev().fold(0, |value, byte| (value << 8) | *byte as u64)
}

/// The CRC-32 a zip or an alz keeps for each entry.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in data {
        crc ^= *byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }

    !crc
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::{crc32, extract_alz, is_alz};

    /// One entry as ALZip writes it, with 1-byte sizes.
    fn entry(name: &[u8], method: u8, stored: &[u8], body: &[u8]) -> Vec<u8> {
        let mut entry = Vec::new();
        entry.extend_from_slice(b"BLZ\x01");
        entry.extend_from_slice(&(name.len() as u16).to_le_bytes());
        entry.push(0x20);
        entry.extend_from_slice(&[0x1d, 0x9d, 0x29, 0x31]);
        entry.push(0x10);
        entry.push(0);
        entry.push(method);
        entry.push(0);
        entry.extend_from_slice(&crc32(stored).to_le_bytes());
        entry.push(body.len() as u8);
        entry.push(stored.len() as u8);
        entry.extend_from_slice(name);
        entry.extend_from_slice(body);

        entry
    }

    fn archive(entries: &[Vec<u8>]) -> Vec<u8> {
        let mut archive = b"ALZ\x01\x0a\x00\x00\x00".to_vec();
        for entry in entries {
            archive.extend_from_slice(entry);
        }
        archive.extend_from_slice(b"CLZ\x01");

        archive
    }

    #[test]
    fn stored_and_deflated_entries_come_out_by_name() {
        let text = b"MIDlet-Name: test\nMIDlet-1: a,,a\n";
        let deflated = miniz_oxide::deflate::compress_to_vec(text, 6);
        // "바람" in EUC-KR, in a folder ALZip writes with a backslash.
        let korean = [b'd', b'\\', 0xb9, 0xd9, 0xb6, 0xf7, b'.', b'j', b'a', b'd'];
        let data = archive(&[entry(b"DESC.jad", 2, text, &deflated), entry(&korean, 0, b"abc", b"abc")]);

        assert!(is_alz(&data));
        let files = extract_alz(&data).unwrap();

        assert_eq!(files.get("DESC.jad").map(Vec::as_slice), Some(text.as_slice()));
        assert_eq!(files.get("d/바람.jad").map(Vec::as_slice), Some(b"abc".as_slice()));
    }

    #[test]
    fn a_damaged_entry_is_reported() {
        let mut bad = entry(b"a", 0, b"abc", b"abc");
        let last = bad.len() - 1;
        bad[last] = b'x';

        assert!(extract_alz(&archive(&[bad])).is_err());
        assert!(!is_alz(b"PK\x03\x04"));
    }
}
