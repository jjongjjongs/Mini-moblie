//! The one compressed form a WIPI package ships assets in.
//!
//! A title's installable tree carries some of its data gzipped - 지크 ships
//! `P/snd/B1.mmf.gz` through `B13.mmf.gz` and `P/dat/m/*.tmb.gz` that way, and
//! the uncompressed `snd/B0.mmf` alongside them in the jar. The handset's
//! installer is what unpacks them: the guest asks for `snd/B1.mmf`, never for
//! the `.gz`, and every one of these carries its own real name in the gzip
//! header's FNAME field to say so.
//!
//! Left packed, the title reads nothing. 지크's sound thread keeps a
//! `byte[][]` of clip data, finds the slot for a sound it could not load
//! empty, and throws a NullPointerException it catches and retries twenty
//! milliseconds later - forever. It never plays another sound after that, its
//! own background music included.

use alloc::{string::String, vec::Vec};

/// What a gzip member says it is and what is inside it.
pub struct GzMember {
    /// The name the member was compressed from, when it carries one. This is
    /// the name the guest asks for.
    pub name: Option<String>,
    pub data: Vec<u8>,
}

const ID1: u8 = 0x1f;
const ID2: u8 = 0x8b;
const DEFLATE: u8 = 8;

const FHCRC: u8 = 0x02;
const FEXTRA: u8 = 0x04;
const FNAME: u8 = 0x08;
const FCOMMENT: u8 = 0x10;

/// Reads one gzip member, or `None` for anything that is not one.
///
/// Deliberately forgiving: a file that only looks gzipped, or whose deflate
/// stream does not decode, is left to be served as it is rather than failing
/// the load of a whole title.
pub fn read_member(data: &[u8]) -> Option<GzMember> {
    if data.len() < 18 || data[0] != ID1 || data[1] != ID2 || data[2] != DEFLATE {
        return None;
    }

    let flags = data[3];
    let mut at = 10;

    if flags & FEXTRA != 0 {
        let length = u16::from_le_bytes([*data.get(at)?, *data.get(at + 1)?]) as usize;
        at = at.checked_add(2)?.checked_add(length)?;
    }

    let mut name = None;
    if flags & FNAME != 0 {
        let end = at + data.get(at..)?.iter().position(|&x| x == 0)?;
        name = Some(String::from_utf8_lossy(&data[at..end]).into_owned());
        at = end + 1;
    }

    if flags & FCOMMENT != 0 {
        at += data.get(at..)?.iter().position(|&x| x == 0)? + 1;
    }

    if flags & FHCRC != 0 {
        at = at.checked_add(2)?;
    }

    // The trailer is the last eight bytes; what is between it and here is the
    // deflate stream.
    let end = data.len().checked_sub(8)?;
    let deflated = data.get(at..end)?;
    let data = miniz_oxide::inflate::decompress_to_vec(deflated).ok()?;

    Some(GzMember { name, data })
}

/// Where a packed member belongs once unpacked: the same directory, under the
/// name the member carries, or the path with `.gz` taken off when it carries
/// none.
pub fn unpacked_path(path: &str, member: &GzMember) -> Option<String> {
    let stripped = path.strip_suffix(".gz")?;

    let Some(name) = member.name.as_deref().filter(|name| !name.is_empty()) else {
        return Some(String::from(stripped));
    };

    // A name with a path separator in it is the archive's business, not the
    // member's; take only the basename either way.
    let name = name.rsplit(['/', '\\']).next().unwrap_or(name);

    Some(match stripped.rfind('/') {
        Some(slash) => alloc::format!("{}/{}", &stripped[..slash], name),
        None => String::from(name),
    })
}

#[cfg(test)]
mod tests {
    use alloc::{string::ToString, vec, vec::Vec};

    use super::{GzMember, read_member, unpacked_path};

    /// One gzip member with FNAME set, holding "hello" in a stored deflate
    /// block - the shape `P/snd/B1.mmf.gz` has.
    fn member(name: &str) -> Vec<u8> {
        let mut out = vec![0x1f, 0x8b, 0x08, 0x08, 0, 0, 0, 0, 0, 0x03];
        out.extend_from_slice(name.as_bytes());
        out.push(0);
        // A single stored block: final, length 5, its complement, then "hello".
        out.extend_from_slice(&[0x01, 0x05, 0x00, 0xfa, 0xff]);
        out.extend_from_slice(b"hello");
        out.extend_from_slice(&0x3610a686u32.to_le_bytes());
        out.extend_from_slice(&5u32.to_le_bytes());
        out
    }

    #[test]
    fn a_member_reads_back_its_name_and_contents() {
        let read = read_member(&member("B1.mmf")).expect("a gzip member");

        assert_eq!(read.name.as_deref(), Some("B1.mmf"));
        assert_eq!(read.data, b"hello");
    }

    #[test]
    fn anything_that_is_not_a_member_is_left_alone() {
        // The uncompressed sibling these ship beside, and a file that starts
        // like one but is not.
        assert!(read_member(b"MMMD\x00\x00\x13Q and the rest of a clip").is_none());
        assert!(read_member(&[0x1f, 0x8b, 0x08, 0x00]).is_none());
    }

    /// The guest asks for the name inside the member, which is why it is there.
    #[test]
    fn a_member_unpacks_under_its_own_name() {
        let read = read_member(&member("B1.mmf")).unwrap();
        assert_eq!(unpacked_path("snd/B1.mmf.gz", &read).as_deref(), Some("snd/B1.mmf"));

        // A member naming a path keeps only its basename, and one naming
        // nothing falls back to the archive's own name without the suffix.
        let read = read_member(&member("a/b/B1.mmf")).unwrap();
        assert_eq!(unpacked_path("snd/B1.mmf.gz", &read).as_deref(), Some("snd/B1.mmf"));

        let anonymous = GzMember {
            name: None,
            data: Vec::new(),
        };
        assert_eq!(unpacked_path("snd/B1.mmf.gz", &anonymous).as_deref(), Some("snd/B1.mmf"));
        assert_eq!(unpacked_path("snd/B1.mmf".to_string().as_str(), &anonymous), None);
    }
}
