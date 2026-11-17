use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};

use wie_util::{Result, WieError};

/// The module information file's extension, and the module's.
const INFO_EXTENSION: &str = ".mif";
const MODULE_EXTENSION: &str = ".mod";

/// Every module information file ends with these four bytes - `mif1` read
/// little-endian. It is the only mark the format has; there is no leading
/// magic.
const INFO_TRAILER: &[u8] = b"1fim";

const MAX_INFO_SPANS: u32 = 64;
const MAX_INFO_TEXT: usize = 4 << 10;

/// What the module information file says, as far as it is understood.
///
/// The span table, the text spans and the trailing identity record are
/// decoded; the numeric spans are carried as words, because what most of them
/// mean is not established. One of them carries the applet's ClassID, which is
/// what the title's class factory is asked for - see
/// [`BrewInfo::application_identifier`].
#[derive(Default, Debug)]
pub struct BrewInfo {
    /// The identity record's second word, which the package names its
    /// information file after.
    pub application_id: u32,
    pub name: String,
    pub vendor: String,
    /// Every numeric span, as words, in file order.
    pub records: Vec<Vec<u32>>,
}

impl BrewInfo {
    /// Decodes a module information file.
    ///
    /// ```text
    /// 0x00  four 16-bit header fields
    /// 0x08  offset of the first header section
    /// 0x0c  offset of the second header section
    /// 0x10  offset of the span table
    /// 0x14  span count
    /// 0x18  offset of the first span, repeating the table's first entry
    /// ```
    ///
    /// The span table holds one offset per span and then the end of the last
    /// one; what follows that end is the identity record, closing with the
    /// trailer.
    pub fn parse(data: &[u8]) -> Result<Self> {
        // One archive's file carries CR LF past its trailer, from something that
        // moved it in text mode before it was packed. Those are dropped and the
        // file is read up to the trailer.
        let mut data = data;
        while let Some((last, rest)) = data.split_last() {
            if *last == b'\r' || *last == b'\n' {
                data = rest;
            } else {
                break;
            }
        }

        if data.len() < 0x20 {
            return Err(WieError::FatalError(format!(
                "module information file is {} bytes, too short",
                data.len()
            )));
        }
        if !data.ends_with(INFO_TRAILER) {
            return Err(WieError::FatalError("module information file does not end with its trailer".into()));
        }

        let word = |offset: usize| -> Option<u32> { data.get(offset..offset + 4).map(|x| u32::from_le_bytes(x.try_into().unwrap())) };

        let table_offset = word(0x10).unwrap() as usize;
        let span_count = word(0x14).unwrap();
        if span_count == 0 || span_count > MAX_INFO_SPANS {
            return Err(WieError::FatalError(format!("module information file declares {span_count} spans")));
        }

        let mut offsets = Vec::with_capacity(span_count as usize + 1);
        for index in 0..=span_count as usize {
            let offset =
                word(table_offset + index * 4).ok_or_else(|| WieError::FatalError("module information file span table runs past its end".into()))?;
            offsets.push(offset as usize);
        }

        if word(0x18) != Some(offsets[0] as u32) {
            return Err(WieError::FatalError(
                "module information file names a first span its table does not".into(),
            ));
        }
        if offsets.windows(2).any(|x| x[1] < x[0]) {
            return Err(WieError::FatalError(
                "module information file has a span that ends before it starts".into(),
            ));
        }

        let record_end = data.len() - INFO_TRAILER.len();
        if *offsets.last().unwrap() > record_end {
            return Err(WieError::FatalError("module information file's last span runs into its trailer".into()));
        }

        let mut info = Self::default();
        for index in 0..span_count as usize {
            let span = &data[offsets[index]..offsets[index + 1]];

            match classify_span(span) {
                SpanKind::Typed => {}
                SpanKind::Text => {
                    let text = decode_euc_kr(&span[2..]);
                    if text.len() > MAX_INFO_TEXT {
                        return Err(WieError::FatalError(format!("module information file span {index} is too long")));
                    }

                    // The vendor is written twice and the title once, so the
                    // first distinct text after the vendor is the name.
                    if info.vendor.is_empty() {
                        info.vendor = text;
                    } else if text != info.vendor && info.name.is_empty() {
                        info.name = text;
                    }
                }
                SpanKind::Numeric => info.records.push(words(span)),
            }
        }

        let trailer = words(&data[*offsets.last().unwrap()..record_end]);
        if trailer.len() > 1 {
            info.application_id = trailer[1];
        }

        Ok(info)
    }

    /// The applet's ClassID, which the module's class factory is asked to
    /// create.
    ///
    /// It is the first word of a five-word record whose middle reads
    /// `0, 0x3e8, 0`. Neither half alone picks it out: the file holds other
    /// round numbers, and the first record's first word is a `0x1000` the
    /// module refuses.
    pub fn application_identifier(&self) -> Option<u32> {
        const RECORD_WORDS: usize = 5;
        const RECORD_MARK: u32 = 0x3e8;

        self.records
            .iter()
            .find(|record| record.len() == RECORD_WORDS && record[0] != 0 && record[1] == 0 && record[2] == RECORD_MARK && record[3] == 0)
            .map(|record| record[0])
    }
}

enum SpanKind {
    Numeric,
    Typed,
    Text,
}

/// A text span opens with two `0xfe` bytes, and a typed span (an icon) with a
/// 16-bit length covering itself, its MIME type and that type's terminator.
fn classify_span(span: &[u8]) -> SpanKind {
    if span.len() >= 2 && span[0] == 0xfe && span[1] == 0xfe {
        return SpanKind::Text;
    }
    if span.len() >= 4 {
        let length = u16::from_le_bytes([span[0], span[1]]) as usize;
        if length <= span.len() && length > 3 && span[length - 1] == 0 {
            return SpanKind::Typed;
        }
    }

    SpanKind::Numeric
}

fn words(span: &[u8]) -> Vec<u32> {
    span.chunks_exact(4).map(|x| u32::from_le_bytes(x.try_into().unwrap())).collect()
}

fn decode_euc_kr(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|x| *x == 0).unwrap_or(bytes.len());

    encoding_rs::EUC_KR.decode(&bytes[..end]).0.to_string()
}

fn extension_of(name: &str) -> String {
    let base = name.rsplit('/').next().unwrap_or(name);

    match base.rfind('.') {
        Some(index) => base[index..].to_ascii_lowercase(),
        None => String::new(),
    }
}

/// Whether the archive's entries are a BREW package: one module information
/// file beside one module. A lone `.mod` is a common enough extension that
/// claiming an archive for it would be a guess; the pair is the package.
pub fn is_brew_package(files: &BTreeMap<String, Vec<u8>>) -> bool {
    if files
        .keys()
        .any(|name| name.eq_ignore_ascii_case("__adf__") || name.eq_ignore_ascii_case("app_info"))
    {
        return false;
    }

    let info = files.keys().filter(|name| extension_of(name) == INFO_EXTENSION).count();
    let module = files.keys().filter(|name| extension_of(name) == MODULE_EXTENSION).count();

    info == 1 && module == 1
}

/// An opened BREW package.
pub struct BrewArchive {
    pub info: BrewInfo,
    pub module: Vec<u8>,
    /// Everything that is not the module or its information file, by the name
    /// the title opens it with - its base name. The files sit beside the
    /// module, under whatever directory the package was repacked in.
    pub files: BTreeMap<String, Vec<u8>>,
}

impl BrewArchive {
    pub fn open(files: BTreeMap<String, Vec<u8>>) -> Result<Self> {
        let mut info = None;
        let mut module = None;
        let mut rest = BTreeMap::new();

        for (name, data) in files {
            match extension_of(&name).as_str() {
                INFO_EXTENSION => {
                    if info.is_some() {
                        return Err(WieError::FatalError("BREW package has two module information files".into()));
                    }
                    info = Some(BrewInfo::parse(&data)?);
                }
                MODULE_EXTENSION => {
                    if module.is_some() {
                        return Err(WieError::FatalError("BREW package has two modules".into()));
                    }
                    if data.is_empty() {
                        return Err(WieError::FatalError(format!("BREW module {name} is empty")));
                    }
                    module = Some(data);
                }
                _ => {
                    let base = name.rsplit('/').next().unwrap_or(&name).to_string();
                    rest.insert(base, data);
                }
            }
        }

        Ok(Self {
            info: info.ok_or_else(|| WieError::FatalError("BREW package has no module information file".into()))?,
            module: module.ok_or_else(|| WieError::FatalError("BREW package has no module".into()))?,
            files: rest,
        })
    }
}

#[cfg(test)]
mod tests {
    use alloc::{collections::BTreeMap, string::String, vec, vec::Vec};

    use super::{BrewInfo, is_brew_package};

    fn names(names: &[&str]) -> BTreeMap<String, Vec<u8>> {
        names.iter().map(|x| (String::from(*x), vec![0u8])).collect()
    }

    #[test]
    fn a_module_beside_its_information_file_is_a_package() {
        assert!(is_brew_package(&names(&["kashan/kashan.mod", "kashan/18933.mif", "kashan/kashan.sig"])));
        assert!(!is_brew_package(&names(&["kashan.mod"])));
        assert!(!is_brew_package(&names(&["__adf__", "a.mod", "b.mif"])));
    }

    /// A file with two spans: a text span naming the vendor and a numeric one
    /// carrying the applet record, then an identity record with the ClassID.
    #[test]
    fn the_applet_record_names_the_class() {
        let mut data = vec![0u8; 0x20];
        let table = 0x20u32;
        let first = table + 3 * 4;
        let text: &[u8] = b"\xfe\xfevendor\0";
        let record: [u32; 5] = [18933, 0, 0x3e8, 0, 0x0010_0000];
        let second = first + text.len() as u32;
        let end = second + 20;

        data[0x10..0x14].copy_from_slice(&table.to_le_bytes());
        data[0x14..0x18].copy_from_slice(&2u32.to_le_bytes());
        data[0x18..0x1c].copy_from_slice(&first.to_le_bytes());
        for offset in [first, second, end] {
            data.extend_from_slice(&offset.to_le_bytes());
        }
        data.extend_from_slice(text);
        for word in record {
            data.extend_from_slice(&word.to_le_bytes());
        }
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&18933u32.to_le_bytes());
        data.extend_from_slice(b"1fim\r\n");

        let info = BrewInfo::parse(&data).unwrap();
        assert_eq!(info.vendor, "vendor");
        assert_eq!(info.application_id, 18933);
        assert_eq!(info.application_identifier(), Some(18933));
    }
}
