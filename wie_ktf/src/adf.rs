use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};

use wie_backend::extract_zip;
use wie_util::{Result, WieError, descriptor_value};

pub struct KtfAdf {
    pub aid: String,
    pub pid: String,
    pub mclass: String,
    /// The panel the title was packaged for, from `DisplaySize:<width>*<height>`.
    ///
    /// A title sizes its own drawing from what the screen reports, so one given
    /// a panel it was not written for lays out on the wrong one. 강철의 연금술사
    /// declares 176*220 and composes for it: run at 240x320 its scene sits in
    /// the top of the screen with a strip of unused panel below it and a
    /// half-drawn dialogue frame showing through. Titles that handle a larger
    /// panel do it by filling the margin - 정무문2 draws its logo and a border
    /// into the space - which is a choice they make, not one to make for them.
    pub display_size: Option<(u32, u32)>,
}

impl KtfAdf {
    pub fn parse(data: &[u8]) -> Self {
        let mut aid = String::new();
        let mut pid = String::new();
        let mut mclass = String::new();
        let mut display_size = None;

        let mut lines = data.split(|x| *x == b'\n');

        for line in &mut lines {
            if let Some(value) = line.strip_prefix(b"AID:") {
                aid = descriptor_value(value);
            } else if let Some(value) = line.strip_prefix(b"PID:") {
                pid = descriptor_value(value);
            } else if let Some(value) = line.strip_prefix(b"MClass:") {
                mclass = descriptor_value(value);
            } else if let Some(value) = line.strip_prefix(b"DisplaySize:") {
                display_size = parse_display_size(&descriptor_value(value));
            }
            // TODO load name, it's in euc-kr..
        }

        Self {
            aid,
            pid,
            mclass,
            display_size,
        }
    }
}

/// `<width>*<height>`, and nothing else.
///
/// Anything this cannot read is left for the caller to fall back on rather than
/// guessed at: a panel is the one thing a title cannot be wrong about and still
/// draw, so half-understanding the field is worse than not reading it.
fn parse_display_size(value: &str) -> Option<(u32, u32)> {
    let (width, height) = value.split_once('*')?;
    let (width, height) = (width.trim().parse().ok()?, height.trim().parse().ok()?);

    if width == 0 || height == 0 { None } else { Some((width, height)) }
}

pub fn find_client_bin(jar: &[u8]) -> Result<(String, Vec<u8>)> {
    let files: BTreeMap<String, Vec<u8>> = extract_zip(jar)?;

    files
        .into_iter()
        .find(|(name, _)| name.starts_with("client.bin"))
        .ok_or_else(|| WieError::FatalError("client.bin* not found in jar".to_string()))
}

pub fn parse_bss_size(filename: &str) -> Result<u32> {
    filename
        .strip_prefix("client.bin")
        .ok_or_else(|| WieError::FatalError(format!("Filename does not start with 'client.bin': {filename}")))?
        .parse::<u32>()
        .map_err(|e| WieError::FatalError(format!("Invalid bss_size in filename {filename}: {e}")))
}

#[cfg(test)]
mod tests {
    use super::{KtfAdf, parse_bss_size};

    #[test]
    fn parse_adf_full() {
        let data = b"AID:foo\nPID:bar\nMClass:baz\n";
        let adf = KtfAdf::parse(data);
        assert_eq!(adf.aid, "foo");
        assert_eq!(adf.pid, "bar");
        assert_eq!(adf.mclass, "baz");
        // A descriptor that names no panel leaves the caller its default.
        assert_eq!(adf.display_size, None);
    }

    /// The panel a title was packaged for. 강철의 연금술사 draws for the one it
    /// declares and not for the one it is given, so reading this is what stops
    /// its scene sitting in the top of a taller screen.
    #[test]
    fn parse_adf_display_size() {
        let adf = KtfAdf::parse(b"AID:01030CEA\nDisplaySize:176*220\nMClass:Clet\n");
        assert_eq!(adf.display_size, Some((176, 220)));

        let adf = KtfAdf::parse(b"DisplaySize:240*320\n");
        assert_eq!(adf.display_size, Some((240, 320)));
    }

    /// Half-reading the field would hand a title a panel nobody wrote down; a
    /// value this cannot read is no value.
    #[test]
    fn an_unreadable_display_size_is_no_answer() {
        for line in [
            &b"DisplaySize:176x220\n"[..],
            b"DisplaySize:176\n",
            b"DisplaySize:0*220\n",
            b"DisplaySize:176*0\n",
            b"DisplaySize:*\n",
            b"DisplaySize:\n",
            b"DisplaySize:wide*tall\n",
        ] {
            assert_eq!(KtfAdf::parse(line).display_size, None, "{:?} should not be read as a panel", line);
        }
    }

    #[test]
    fn parse_adf_empty() {
        let adf = KtfAdf::parse(b"");
        assert!(adf.aid.is_empty());
        assert!(adf.pid.is_empty());
        assert!(adf.mclass.is_empty());
    }

    #[test]
    fn parse_adf_partial() {
        let data = b"AID:only\n";
        let adf = KtfAdf::parse(data);
        assert_eq!(adf.aid, "only");
        assert!(adf.pid.is_empty());
        assert!(adf.mclass.is_empty());
    }

    #[test]
    fn parse_bss_size_ok() {
        assert_eq!(parse_bss_size("client.bin12345").unwrap(), 12345);
        assert_eq!(parse_bss_size("client.bin0").unwrap(), 0);
    }

    #[test]
    fn parse_bss_size_missing_marker() {
        assert!(parse_bss_size("not_a_client_bin_name").is_err());
    }

    #[test]
    fn parse_bss_size_no_digits() {
        assert!(parse_bss_size("client.bin").is_err());
    }

    #[test]
    fn parse_bss_size_non_numeric() {
        assert!(parse_bss_size("client.binABC").is_err());
    }
}
