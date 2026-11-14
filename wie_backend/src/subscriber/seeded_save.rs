//! Recovery of the subscriber number 2011슈퍼사커's saves were written for.
//!
//! 2011슈퍼사커 (KTF `01042963`) keeps every save - `SC_option.dat`,
//! `SC_season.dat` and the rest - in one shape, written by `0x10fc10` and read
//! by `0x10fcc0`:
//!
//! ```text
//! [0..4]   the file's own length, little-endian
//! [4..8]   a generator seed, little-endian
//! [8..]    the body, every byte XORed with table[rand() % 16]
//! ```
//!
//! `rand()` is the C library's, `state * 0x343fd + 0x269ec3` taken from bit
//! 16, run from the seed in the file; the sixteen-byte table is the title's
//! own. The body opens with the `PHONENUMBER` the save was written on, twelve
//! bytes NUL padded, and on loading the title compares it with the handset's:
//! a save from another number is thrown away and rewritten from defaults, and
//! the rewrite of `SC_season.dat` dereferences a null on the way and ends the
//! game in `startApp`.
//!
//! The table need not be known. A save is mostly zeroes, and a zero byte comes
//! out as the table entry that masked it, so each entry is the byte that turns
//! up most often among the positions it masks. A dump carries its owner's
//! saves, so the number they name is the one this handset has to report.

use alloc::string::{String, ToString};

/// The header ahead of the body, and the number field that opens it.
const HEADER_LEN: usize = 8;
const NUMBER_LEN: usize = 12;
/// How many body bytes past the number a save needs before its zeroes say
/// enough about every table entry.
const MIN_TAIL: usize = 64;

/// The subscriber number a 2011슈퍼사커 save was written for, or `None` when
/// the bytes are not one.
pub fn recover_phone_number(save: &[u8]) -> Option<String> {
    if save.len() < HEADER_LEN + NUMBER_LEN + MIN_TAIL {
        return None;
    }
    let length = u32::from_le_bytes(save[..4].try_into().ok()?) as usize;
    if length != save.len() {
        return None;
    }

    let mut state = u32::from_le_bytes(save[4..8].try_into().ok()?);
    let indices: alloc::vec::Vec<usize> = save[HEADER_LEN..]
        .iter()
        .map(|_| {
            state = state.wrapping_mul(0x343fd).wrapping_add(0x269ec3);
            ((state >> 16) & 0xf) as usize
        })
        .collect();

    let mut counts = [[0u32; 256]; 16];
    for (&byte, &index) in save[HEADER_LEN + NUMBER_LEN..].iter().zip(&indices[NUMBER_LEN..]) {
        counts[index][byte as usize] += 1;
    }
    let mut table = [0u8; 16];
    for (entry, count) in table.iter_mut().zip(&counts) {
        // An entry that masks nothing past the number leaves the number
        // unreadable.
        let (byte, &most) = count.iter().enumerate().max_by_key(|&(_, &n)| n)?;
        if most == 0 {
            return None;
        }
        *entry = byte as u8;
    }

    let field: alloc::vec::Vec<u8> = save[HEADER_LEN..HEADER_LEN + NUMBER_LEN]
        .iter()
        .zip(&indices)
        .map(|(&byte, &index)| byte ^ table[index])
        .collect();
    let end = field.iter().position(|&b| b == 0)?;
    let number = &field[..end];
    if !(10..=11).contains(&number.len()) || number[0] != b'0' || !number.iter().all(u8::is_ascii_digit) || field[end..].iter().any(|&b| b != 0) {
        return None;
    }

    Some(core::str::from_utf8(number).ok()?.to_string())
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use super::recover_phone_number;

    /// The `SC_NoBackUp.dat` 2011슈퍼사커's KTF dump ships, written on
    /// 01068488891.
    #[rustfmt::skip]
    const NO_BACKUP: [u8; 112] = [
        0x70, 0x00, 0x00, 0x00, 0xcf, 0x67, 0x68, 0xaa, 0x79, 0x43, 0xa0, 0xe7, 0xb1, 0x56, 0xe9, 0x5a,
        0xa8, 0xb8, 0xd0, 0x81, 0x92, 0x81, 0x19, 0x49, 0x8a, 0xe1, 0x90, 0x19, 0xd1, 0xd0, 0x90, 0x90,
        0x89, 0x46, 0x19, 0x81, 0x83, 0xf6, 0x19, 0x89, 0x89, 0xd1, 0x89, 0x90, 0x56, 0x90, 0x81, 0x09,
        0x83, 0x56, 0x83, 0x62, 0x49, 0x09, 0x56, 0xd1, 0x83, 0x8a, 0x19, 0x81, 0x92, 0x83, 0x8a, 0x49,
        0xe1, 0xf6, 0x83, 0xd1, 0x19, 0x56, 0xd1, 0xe1, 0x09, 0xd1, 0x62, 0x49, 0x46, 0x62, 0x92, 0x62,
        0xe1, 0x56, 0x46, 0x46, 0x89, 0x62, 0x90, 0x83, 0x49, 0x46, 0x8a, 0xd1, 0x46, 0x83, 0x49, 0x83,
        0x72, 0x62, 0xd1, 0x19, 0x49, 0x19, 0x46, 0xf6, 0x49, 0x92, 0x62, 0x83, 0x4b, 0x2d, 0xed, 0xdf,
    ];

    #[test]
    fn the_number_the_save_was_written_on_is_read_out_of_it() {
        assert_eq!(recover_phone_number(&NO_BACKUP), Some("01068488891".to_string()));
    }

    #[test]
    fn anything_else_is_not_read_as_one() {
        assert_eq!(recover_phone_number(&[0u8; 112]), None);
        assert_eq!(recover_phone_number(&NO_BACKUP[..111]), None);

        let mut damaged = NO_BACKUP;
        damaged[9] ^= 0x40;
        assert_eq!(recover_phone_number(&damaged), None);
    }
}
