//! Whether a 컴투스 save the archive ships was made on the handset we report.
//!
//! 미니게임천국5's KTF build (0104261C) keeps its whole game in `gamedata.dat`,
//! sealed twice under the AES-128-CBC IV its `cert.c2s` uses (see
//! [`super::fixed_key_cert`]):
//!
//! ```text
//! <key, 16 bytes><outer: AES-CBC under key>
//! outer = <inner: AES-CBC under PHONENUMBER><MD5 of inner><16 bytes>
//! inner = <game data><PHONENUMBER, 16 bytes><MD5 of what precedes it><16 bytes>
//! ```
//!
//! The loader at `0x16f972` opens the outer layer with the key in front of it
//! and the inner one with `MC_knlGetSystemProperty("PHONENUMBER")` padded to
//! sixteen bytes, and takes the save only when the number sealed inside is the
//! one it was opened with. A save another handset wrote fails there, and the
//! title does not fall back to a new game: it carries on with nothing loaded,
//! writes that back over the file, and from then on has no character and none
//! of the 2500 stars a new game starts with - so not a game it can open.
//!
//! The archive that ships one was dumped from the handset it was made on, and
//! the number it was sealed under is not written anywhere a handset could read
//! it back. So this answers only whether the save opens under the number we
//! report; the caller leaves a save that does not out, and the title starts the
//! new game it starts when it finds none.

use super::fixed_key_cert::decrypt_cbc;

/// Bytes after each layer's payload: its MD5, then sixteen more.
const TRAILER_LEN: usize = 32;
/// The inner layer's PHONENUMBER sits this far before the end of its payload.
const NUMBER_FROM_END: usize = 48;
/// The largest file treated as one of these saves; 미니게임천국5's is 18996
/// bytes. Bounds the decryption a stray packaged file costs.
const MAX_LEN: usize = 64 * 1024;

/// `Some(true)` when `save` is one of these saves and opens under `number`,
/// `Some(false)` when it is one and was sealed under another number, `None`
/// when it is not one at all.
pub fn opens_under(save: &[u8], number: &str) -> Option<bool> {
    if save.len() > MAX_LEN || save.len() < 16 + TRAILER_LEN * 2 + NUMBER_FROM_END {
        return None;
    }
    let key: &[u8; 16] = save[..16].try_into().ok()?;
    let outer_len = save.len() - 16;
    let whole = outer_len - outer_len % 16;
    let outer = decrypt_cbc(&save[16..16 + whole], key);

    // The outer layer is unkeyed, so its MD5 says whether this is a save of this
    // shape at all.
    let inner_len = outer_len - TRAILER_LEN;
    if inner_len + 16 > whole {
        return None;
    }
    if md5::compute(&outer[..inner_len]).0 != outer[inner_len..inner_len + 16] {
        return None;
    }

    let number = number.as_bytes();
    if number.len() > 16 {
        return Some(false);
    }
    let mut padded = [0u8; 16];
    padded[..number.len()].copy_from_slice(number);

    // Only the blocks the sealed number falls in are needed, with the block
    // before them standing in for the IV.
    let at = inner_len - NUMBER_FROM_END;
    let first = at / 16;
    let last = (at + 16).div_ceil(16);
    if first == 0 || last * 16 > inner_len {
        return None;
    }
    let plain = decrypt_cbc(&outer[(first - 1) * 16..last * 16], &padded);
    let offset = at - (first - 1) * 16;

    Some(plain[offset..offset + 16] == padded)
}

#[cfg(test)]
mod tests {
    use super::opens_under;

    /// A save of the same shape, sealed under `01012345678` with the outer key
    /// `a0..af`: the payload's tail carries the number and both layers' MD5s.
    #[rustfmt::skip]
    const SAVE: [u8; 148] = [
        0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xab, 0xac, 0xad, 0xae, 0xaf,
        0xd7, 0x4b, 0x6a, 0xbc, 0xb9, 0xde, 0x02, 0x74, 0x65, 0x20, 0xef, 0x8b, 0x67, 0x47, 0xd1, 0x33,
        0xe5, 0x8f, 0x9a, 0xa5, 0x3e, 0x08, 0x4a, 0x9f, 0x0a, 0xe6, 0x28, 0x6d, 0xb0, 0xe1, 0xbb, 0xc3,
        0x06, 0xfa, 0x49, 0x1e, 0xf2, 0x04, 0xaa, 0x86, 0x53, 0xb9, 0x7d, 0x9a, 0x74, 0x73, 0x27, 0x6c,
        0xb8, 0x85, 0x34, 0x51, 0xcb, 0xd3, 0x79, 0x48, 0x7a, 0xb4, 0x58, 0x7e, 0xac, 0x0a, 0x49, 0xd1,
        0x0c, 0x3a, 0x76, 0x52, 0xdd, 0x52, 0x31, 0x3e, 0x8c, 0x69, 0x76, 0xbc, 0x1e, 0xaa, 0x42, 0xe2,
        0x35, 0xef, 0x5e, 0x63, 0x6d, 0x23, 0x56, 0xf6, 0x2e, 0x2c, 0xb7, 0x6b, 0x0e, 0x40, 0xe5, 0xf5,
        0x26, 0xb1, 0xea, 0x7c, 0xa0, 0xaf, 0xbb, 0xf7, 0xdc, 0x7f, 0xdb, 0x92, 0x21, 0x01, 0xed, 0x8a,
        0xa8, 0x57, 0xcc, 0xa5, 0x21, 0x2f, 0xcf, 0x89, 0x66, 0x3a, 0x17, 0xe9, 0x2a, 0x6b, 0x6e, 0x8d,
        0x00, 0x00, 0x00, 0x00,
    ];

    #[test]
    fn a_save_opens_under_the_number_it_was_sealed_with() {
        assert_eq!(opens_under(&SAVE, "01012345678"), Some(true));
    }

    #[test]
    fn a_save_another_handset_wrote_does_not() {
        assert_eq!(opens_under(&SAVE, "01046119269"), Some(false));
        assert_eq!(opens_under(&SAVE, ""), Some(false));
    }

    #[test]
    fn a_file_of_another_shape_is_not_one() {
        let mut other = SAVE;
        other[40] ^= 1;
        assert_eq!(opens_under(&other, "01012345678"), None);
        assert_eq!(opens_under(&[0u8; 148], "01012345678"), None);
        assert_eq!(opens_under(b"short", "01012345678"), None);
    }
}
