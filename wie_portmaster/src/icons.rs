//! A game's icon, out of its archive, for the list: the same pictures the
//! Android app shows on its tiles, found the same way.
//!
//! Which entry is the icon depends on the carrier. LGT archives call theirs
//! `big.png`, `middle.png` and `small.png`; KTF ones `big.icon` and the like,
//! or name them after the application; an SKT archive keeps its menu icon in
//! the `.wmr` resource beside its jar. So an entry's first bytes say whether it
//! is a picture, and its name only which to try first - the largest. An
//! archive with nothing readable still has its jar, whose largest picture of a
//! tile's size is the title's own artwork.

use std::{
    io::{Cursor, Read},
    path::Path,
};

use wie_backend::canvas::decode_image;

use crate::ui::Picture;

/// Entries bigger than this are not looked at as pictures.
const MAX_ICON_BYTES: u64 = 512 * 1024;
/// How many entries are tried before giving up on the archive's own.
const MAX_CANDIDATES: usize = 200;
/// A jar bigger than this is not opened for its pictures.
const MAX_JAR_BYTES: u64 = 16 * 1024 * 1024;

/// The icon of the game in `path`, if it has one that reads.
pub(crate) fn extract(path: &Path) -> Option<Picture> {
    let data = std::fs::read(path).ok()?;
    let mut archive = zip::ZipArchive::new(Cursor::new(data.as_slice())).ok()?;

    let mut candidates: Vec<(u8, usize)> = (0..archive.len())
        .filter_map(|index| {
            let entry = archive.by_index_raw(index).ok()?;
            (!entry.is_dir() && entry.size() <= MAX_ICON_BYTES).then(|| (rank(&entry.name().to_lowercase()), index))
        })
        .collect();
    // The named icons first, largest first; then everything else in the
    // archive's own order.
    candidates.sort_by_key(|(rank, index)| (*rank, *index));

    for (_, index) in candidates.into_iter().take(MAX_CANDIDATES) {
        let Some(bytes) = read(&mut archive, index, MAX_ICON_BYTES) else {
            continue;
        };
        let picture = if is_skvm_icon(&bytes) {
            skvm_icon(&bytes)
        } else if is_image(&bytes) {
            decode(&bytes)
        } else {
            None
        };
        if let Some(picture) = picture.filter(|picture| (12..=256).contains(&picture.width) && (12..=256).contains(&picture.height)) {
            return Some(picture);
        }
    }

    inside_jar(&mut archive)
}

/// Lower is tried first: the names an icon usually has, largest first.
fn rank(path: &str) -> u8 {
    let name = path.rsplit('/').next().unwrap_or(path);
    if name.starts_with("big.") {
        0
    } else if name.starts_with("middle.") {
        1
    } else if name.starts_with("small.") {
        2
    } else if name.ends_with(".icon")
        || name.contains("icon")
        || name.ends_with(".wmr")
        || ["_l.png", "_ad.png", "_m.png", "_s.png"].iter().any(|end| name.ends_with(end))
    {
        3
    } else {
        4
    }
}

fn read<R: Read + std::io::Seek>(archive: &mut zip::ZipArchive<R>, index: usize, limit: u64) -> Option<Vec<u8>> {
    let entry = archive.by_index(index).ok()?;
    let mut bytes = Vec::new();
    entry.take(limit + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= limit).then_some(bytes)
}

/// PNG, GIF, a Windows bitmap or JPEG, by its first bytes.
fn is_image(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG") || bytes.starts_with(b"GIF8") || bytes.starts_with(b"BM") || bytes.starts_with(&[0xff, 0xd8])
}

fn decode(bytes: &[u8]) -> Option<Picture> {
    let image = decode_image(bytes).ok()?;
    let (width, height) = (image.width(), image.height());
    if width == 0 || height == 0 {
        return None;
    }
    let rgba = image.colors().iter().flat_map(|color| [color.r, color.g, color.b, color.a]).collect();
    Some(Picture { width, height, rgba })
}

/// The magic `0xFACEDEAD` an SK-VM icon resource opens with.
fn is_skvm_icon(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xad, 0xde, 0xce, 0xfa])
}

/// The menu icon out of an SK-VM icon resource: after the magic and the
/// length, records of an index and a byte length, each followed by its body.
/// Record 0 is the still icon, a plain BMP.
fn skvm_icon(bytes: &[u8]) -> Option<Picture> {
    let word = |at: usize| bytes.get(at..at + 4).map(|x| u32::from_le_bytes(x.try_into().unwrap()) as usize);
    let mut offset = 8;
    while let (Some(index), Some(length)) = (word(offset), word(offset + 4)) {
        let body = offset + 8;
        let end = body.checked_add(length)?;
        if end > bytes.len() {
            return None;
        }
        if index == 0 {
            return decode(&bytes[body..end]);
        }
        offset = end;
    }
    None
}

/// The width and height a picture's header says, without decoding it.
fn picture_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let be = |at: usize| bytes.get(at..at + 4).map(|x| u32::from_be_bytes(x.try_into().unwrap()));
    let le16 = |at: usize| bytes.get(at..at + 2).map(|x| u16::from_le_bytes(x.try_into().unwrap()) as u32);
    let le32 = |at: usize| bytes.get(at..at + 4).map(|x| i32::from_le_bytes(x.try_into().unwrap()).unsigned_abs());
    if bytes.starts_with(b"\x89PNG") {
        Some((be(16)?, be(20)?))
    } else if bytes.starts_with(b"GIF8") {
        Some((le16(6)?, le16(8)?))
    } else if bytes.starts_with(b"BM") {
        Some((le32(18)?, le32(22)?))
    } else {
        None
    }
}

/// The largest picture of a tile's size in the archive's jar - the title's
/// own artwork - for an archive whose icons are in no format here.
fn inside_jar<R: Read + std::io::Seek>(archive: &mut zip::ZipArchive<R>) -> Option<Picture> {
    let jars: Vec<usize> = (0..archive.len())
        .filter(|index| {
            archive
                .by_index_raw(*index)
                .is_ok_and(|entry| entry.name().to_lowercase().ends_with(".jar") && entry.size() <= MAX_JAR_BYTES)
        })
        .collect();
    for index in jars {
        let Some(jar) = read(archive, index, MAX_JAR_BYTES) else {
            continue;
        };
        // An SKT jar opens with 32 bytes of SK-VM header before its zip.
        let jar = if !jar.starts_with(b"PK") && jar.get(32..34) == Some(b"PK") {
            &jar[32..]
        } else {
            &jar[..]
        };
        let Ok(mut inner) = zip::ZipArchive::new(Cursor::new(jar)) else {
            continue;
        };
        let mut best: Option<(u32, Vec<u8>)> = None;
        for entry in 0..inner.len() {
            let Some(bytes) = read(&mut inner, entry, MAX_ICON_BYTES) else {
                continue;
            };
            if let Some((width, height)) = picture_size(&bytes)
                && (32..=512).contains(&width)
                && (32..=512).contains(&height)
                && best.as_ref().is_none_or(|(area, _)| width * height > *area)
            {
                best = Some((width * height, bytes));
            }
        }
        if let Some(picture) = best.and_then(|(_, bytes)| decode(&bytes)) {
            return Some(picture);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_named_icons_come_first_largest_first() {
        assert!(rank("big.icon") < rank("middle.png"));
        assert!(rank("w/apps/010100d4/middle.icon") < rank("small.icon"));
        assert!(rank("small.png") < rank("0002e1a1_3_s.png"));
        assert!(rank("0002e1a1_3_s.png") < rank("data/map.bin"));
    }

    #[test]
    fn picture_sizes_come_from_their_headers() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&120u32.to_be_bytes());
        png.extend_from_slice(&110u32.to_be_bytes());
        assert_eq!(picture_size(&png), Some((120, 110)));
        assert_eq!(picture_size(b"not a picture"), None);
    }

    #[test]
    fn the_helloworld_archive_reads_or_has_no_icon() {
        let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../test_data/helloworld_ktf.zip"));
        // A test archive may carry no picture at all; it must not fail either way.
        if let Some(picture) = extract(path) {
            assert_eq!(picture.rgba.len(), (picture.width * picture.height * 4) as usize);
        }
    }
}
