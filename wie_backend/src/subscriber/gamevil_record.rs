//! Recovery of the subscriber number a GAMEVIL title last authenticated as.
//!
//! 2011프로야구 (KTF `01042598`) keeps its 인증 in a sixty-byte file it calls
//! `audio.adt`, which holds no audio: `0x10888c` reads it and XORs every byte
//! with the next value of the C library's generator - `state * 0x343fd +
//! 0x269ec3`, bits 16 to 23 - seeded with `0x21c3` (`0x10885c`). What comes out
//! is the record the server issued:
//!
//! ```text
//! [0x28..0x34]  the subscriber number it was issued for, NUL terminated
//! [0x34..0x38]  how many runs it still allows, little-endian
//! [0x38..0x3c]  the month it was issued in
//! ```
//!
//! On start (`0x10891c`) the title compares the number with the handset's
//! `PHONENUMBER`: the same number, or the same month, and it goes on; neither,
//! with no runs left, and it says `인증에 실패하였습니다` and goes no further.
//! A dump carries its owner's record, so the number it names is the one this
//! handset has to report - the same answer [`super::cert_c2s`] gives for a
//! certificate.

use alloc::string::{String, ToString};

/// The file's size, and where the number sits in it.
const RECORD_LEN: usize = 60;
const NUMBER_AT: usize = 0x28;
const NUMBER_LEN: usize = 12;
/// What the title seeds its generator with before it decodes the record.
const SEED: u32 = 0x21c3;

/// The subscriber number a GAMEVIL `audio.adt` record names, or `None` when
/// the bytes are not one.
pub fn recover_phone_number(record: &[u8]) -> Option<String> {
    if record.len() != RECORD_LEN {
        return None;
    }

    let mut state = SEED;
    let plain: alloc::vec::Vec<u8> = record
        .iter()
        .map(|byte| {
            state = state.wrapping_mul(0x343fd).wrapping_add(0x269ec3);
            byte ^ (state >> 16) as u8
        })
        .collect();

    // Nothing but zeroes in front of the number, which is how the title wrote
    // it and what tells a record from a file that only shares its name.
    if plain[..NUMBER_AT].iter().any(|&b| b != 0) {
        return None;
    }

    let field = &plain[NUMBER_AT..NUMBER_AT + NUMBER_LEN];
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

    /// The `audio.adt` 2011프로야구's KTF dump ships: issued for 01068488891 in
    /// September, no runs left.
    #[rustfmt::skip]
    const BASEBALL2011: [u8; 60] = [
        0x67, 0xd3, 0x18, 0x59, 0x59, 0x3b, 0x5f, 0x48, 0x7d, 0x3f, 0x4c, 0xdc, 0xf5, 0x81, 0xc5, 0xf0,
        0x7b, 0xe6, 0xf0, 0xa4, 0x02, 0xa5, 0xf5, 0x0e, 0x9a, 0x9e, 0x05, 0x2c, 0x95, 0xee, 0x9b, 0x1d,
        0xe8, 0xc0, 0x01, 0x91, 0xd4, 0x65, 0x17, 0x7f, 0xf5, 0x99, 0x60, 0x20, 0x42, 0x6f, 0xc5, 0xcd,
        0x5e, 0x0f, 0xff, 0x9e, 0x56, 0xe2, 0x95, 0x28, 0x5c, 0x50, 0x4e, 0x35,
    ];

    #[test]
    fn the_number_the_record_names_is_read_out_of_it() {
        assert_eq!(recover_phone_number(&BASEBALL2011), Some("01068488891".to_string()));
    }

    #[test]
    fn anything_else_is_not_read_as_one() {
        assert_eq!(recover_phone_number(&[0u8; 60]), None);
        assert_eq!(recover_phone_number(&BASEBALL2011[..59]), None);

        let mut damaged = BASEBALL2011;
        damaged[3] ^= 1;
        assert_eq!(recover_phone_number(&damaged), None);
    }
}
