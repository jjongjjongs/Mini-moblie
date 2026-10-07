use alloc::vec::Vec;

/// 드래곤나이트3 (피엔제이) keeps its options in `dk3option`, eighteen bytes:
/// a check word, then fourteen bytes of settings. The check word ties the file
/// to the handset that wrote it. On start the title reads the file and
/// recomputes the word, and if the two differ it says 조작된 데이타가
/// 발견되어 게임을 종료합니다 and quits.
///
/// The title computes the word in `b.b([BZ)[B`:
/// `b.a(settings) ^ b.a(PHONENUMBER.getBytes())`, big endian.
/// `b.a([B)I` is djb2 over signed bytes, XORed with `0x15591429`; the XORs
/// cancel. A dump carries the word for its owner's number, so this one was
/// rejected on every launch. The settings are the player's, so they are kept,
/// and the word is recomputed for the number this handset reports.
const OPTION_FILE: &str = "dk3option";
const OPTION_LEN: usize = 18;
const CHECK_LEN: usize = 4;

/// djb2 as the title computes it, over Java's signed bytes.
fn djb2(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(5381u32, |hash, &byte| hash.wrapping_mul(33).wrapping_add(byte as i8 as i32 as u32))
}

/// `data` with its handset check rewritten for `subscriber`, when `path` is a
/// file the title binds to the handset. `None` leaves the file as it is.
pub fn rebind(path: &str, data: &[u8], subscriber: &str) -> Option<Vec<u8>> {
    let name = path.rsplit('/').next().unwrap_or(path);
    if name != OPTION_FILE || data.len() != OPTION_LEN {
        return None;
    }

    let check = djb2(&data[CHECK_LEN..]) ^ djb2(subscriber.as_bytes());
    if data[..CHECK_LEN] == check.to_be_bytes() {
        return None;
    }

    tracing::info!("{OPTION_FILE}: rewriting its handset check for {subscriber}");

    let mut rebound = data.to_vec();
    rebound[..CHECK_LEN].copy_from_slice(&check.to_be_bytes());

    Some(rebound)
}

#[cfg(test)]
mod tests {
    use super::{djb2, rebind};

    /// The file a 드래곤나이트3 dump shipped, written on another handset.
    const DUMPED: [u8; 18] = [
        0x9b, 0x98, 0x6a, 0xd2, 0x01, 0x00, 0x01, 0x00, 0x04, 0x04, 0x02, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00,
    ];

    /// The check the title itself accepted for `01046119269` with these
    /// settings, measured on a run.
    #[test]
    fn the_check_is_rewritten_for_this_handset() {
        let rebound = rebind("dk3option", &DUMPED, "01046119269").expect("rewritten");

        assert_eq!(rebound[..4], [0xb8, 0xa9, 0x13, 0xa9]);
        assert_eq!(rebound[4..], DUMPED[4..]);

        // And once it matches, it is left alone.
        assert_eq!(rebind("dk3option", &rebound, "01046119269"), None);
    }

    /// The words the title wrote itself on first runs with no file, for one
    /// set of default settings and six subscriber numbers: XORing out each
    /// number's hash leaves the same settings hash every time, which is the
    /// title's formula and this one agreeing.
    #[test]
    fn it_agrees_with_what_the_title_writes() {
        let written: [(&str, i32); 6] = [
            ("00000000000", 127664955),
            ("00000000001", 127664952),
            ("00000000010", 127665368),
            ("10000000000", -1988535816),
            ("01046119268", -1988587979),
            ("01046119269", -1988587982),
        ];

        for (number, word) in written {
            assert_eq!(word as u32 ^ djb2(number.as_bytes()), 0xb34ad0ce, "{number}");
        }
    }

    #[test]
    fn other_files_are_left_alone() {
        assert_eq!(rebind("alldata104.puf", &DUMPED, "01046119269"), None);
        assert_eq!(rebind("dk3option", &DUMPED[..17], "01046119269"), None);
    }
}
