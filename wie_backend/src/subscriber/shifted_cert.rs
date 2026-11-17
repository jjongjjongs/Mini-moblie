//! Recovery of the subscriber number from a `cert.c2s` that is shifted rather
//! than encrypted.
//!
//! A third form of the file, beside the keyed certificate [`super::cert_c2s`]
//! reads and the own-key one [`super::fixed_key_cert`] does. There is no key
//! here at all: every character is moved by a fixed amount for its kind, and
//! the record is otherwise plain. 록맨X (AID 0103F9F1) ships one, reads
//! `MC_knlGetSystemProperty("PHONENUMBER")` and stops at its own error 2001
//! when what it is told is not the number the certificate names.
//!
//! The record is 52 bytes of fields, and 록맨X's reads:
//!
//! ```text
//!   [0..10)   ten spaces
//!   [10..18)  the application id, `0103f9f1`
//!   [18..20)  two spaces
//!   [20..28)  the title's own tag, `RockmanX`
//!   [28..47)  the subscriber number, right justified in nineteen columns
//!   [47]      a space
//!   [48..52)  a four-byte tail
//! ```
//!
//! and the shift is `+0x30` for a digit, `+0x20` for a letter, and either for a
//! space - `0x20` where the field around it is text and `0x50` where it is the
//! number's own padding. Digits and capitals overlap once shifted (`0x61` is
//! both `1` and `A`), so nothing here decodes a field whose kind it does not
//! already know; only the number is read, and only where the record holds
//! together around it.
//!
//! The tail is left alone. One sample cannot say what it checks, and a guess
//! that rejected a good certificate would cost more than not checking it - the
//! fields the number sits between are structure enough to refuse anything that
//! is not this format.

use alloc::{string::String, vec::Vec};

/// Length of the whole record.
const RECORD_LEN: usize = 52;
/// The number's field, right justified and padded with shifted spaces.
const NUMBER_AT: usize = 28;
const NUMBER_END: usize = 47;
/// A space inside the number's field, which carries the number's own shift.
const PADDING: u8 = 0x50;
/// A space in a text field, which carries no shift.
const SPACE: u8 = 0x20;
/// What a digit is shifted by, so `0` is `0x60` and `9` is `0x69`.
const DIGIT_SHIFT: u8 = 0x30;
/// What a letter is shifted by, so `A` is `0x61` and `a` is `0x81`.
const LETTER_SHIFT: u8 = 0x20;

/// The subscriber number a shifted `cert.c2s` names, or `None` when the bytes
/// are not one.
pub fn recover_phone_number(cert: &[u8]) -> Option<String> {
    if cert.len() != RECORD_LEN {
        return None;
    }

    // The fields around the number, which together are what says this is the
    // format rather than something else of the same length.
    if cert[..10] != [SPACE; 10] || cert[18..20] != [SPACE; 2] || cert[47] != SPACE {
        return None;
    }
    if !cert[10..18].iter().all(|&byte| is_digit(byte) || is_letter(byte)) {
        return None;
    }
    if !cert[20..28].iter().all(|&byte| is_letter(byte) || byte == SPACE) {
        return None;
    }

    let field = &cert[NUMBER_AT..NUMBER_END];
    let digits = field.iter().skip_while(|&&byte| byte == PADDING).copied().collect::<Vec<_>>();
    if !(10..=15).contains(&digits.len()) || !digits.iter().all(|&byte| is_digit(byte)) {
        return None;
    }

    let number = digits.into_iter().map(|byte| (byte - DIGIT_SHIFT) as char).collect::<String>();

    // A subscriber number of this era starts with a zero, and a field that
    // decoded to anything else means the layout was read wrongly.
    number.starts_with('0').then_some(number)
}

/// Whether a byte is a shifted `0` through `9`.
fn is_digit(byte: u8) -> bool {
    (b'0' + DIGIT_SHIFT..=b'9' + DIGIT_SHIFT).contains(&byte)
}

/// Whether a byte is a shifted letter, of either case.
fn is_letter(byte: u8) -> bool {
    (b'A' + LETTER_SHIFT..=b'Z' + LETTER_SHIFT).contains(&byte) || (b'a' + LETTER_SHIFT..=b'z' + LETTER_SHIFT).contains(&byte)
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::recover_phone_number;

    /// 록맨X's own `cert.c2s`, issued for 01096589565.
    const ROCKMAN_X: [u8; 52] = [
        0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x60, 0x61, 0x60, 0x63, 0x86, 0x69, 0x86, 0x61, 0x20, 0x20, 0x72, 0x8f, 0x83,
        0x8b, 0x8d, 0x81, 0x8e, 0x78, 0x50, 0x50, 0x50, 0x50, 0x50, 0x50, 0x50, 0x50, 0x60, 0x61, 0x60, 0x69, 0x66, 0x65, 0x68, 0x69, 0x65, 0x66,
        0x65, 0x20, 0x3b, 0x9a, 0x12, 0xff,
    ];

    #[test]
    fn the_number_the_certificate_names_is_read_out_of_it() {
        assert_eq!(recover_phone_number(&ROCKMAN_X).as_deref(), Some("01096589565"));
    }

    /// The fields around the number are what tells this format from another of
    /// the same length, so each of them refuses on its own.
    #[test]
    fn a_record_that_is_not_this_format_is_refused() {
        assert_eq!(recover_phone_number(&[]), None);
        assert_eq!(recover_phone_number(&[0x20; 52]), None);
        assert_eq!(recover_phone_number(&ROCKMAN_X[..51]), None);

        for at in [0, 9, 18, 47] {
            let mut broken = ROCKMAN_X;
            broken[at] = 0x00;
            assert_eq!(recover_phone_number(&broken), None, "a field at {at} that is not spaces");
        }

        // An application id that is neither digits nor letters, and a tag the
        // same.
        for at in [10, 20] {
            let mut broken = ROCKMAN_X;
            broken[at] = 0xff;
            assert_eq!(recover_phone_number(&broken), None, "a field at {at} that is not text");
        }
    }

    /// The number is right justified, so its padding is not part of it and a
    /// field with nothing after the padding names no number at all.
    #[test]
    fn the_number_is_read_from_the_end_of_its_field() {
        // A shorter number sits further right, with one more column of
        // padding in front of it.
        let mut shorter = ROCKMAN_X;
        shorter[36] = 0x50;
        shorter[37] = 0x60;
        assert_eq!(recover_phone_number(&shorter).as_deref(), Some("0096589565"));

        let mut empty = ROCKMAN_X;
        empty[28..47].copy_from_slice(&[0x50; 19]);
        assert_eq!(recover_phone_number(&empty), None);
    }

    /// A field that decodes to something no subscriber number looks like is a
    /// layout read wrongly, not a number.
    #[test]
    fn a_field_that_does_not_name_a_number_is_refused() {
        let mut not_a_number = ROCKMAN_X;
        not_a_number[36] = 0x69; // a leading 9 rather than a 0
        assert_eq!(recover_phone_number(&not_a_number), None);

        let mut letters = ROCKMAN_X;
        letters[40] = 0x81; // a letter in the middle of the digits
        assert_eq!(recover_phone_number(&letters), None);
    }

    /// Every other certificate the runtime reads must fall through this one.
    #[test]
    fn another_publishers_certificate_is_not_mistaken_for_this_one() {
        let keyed: Vec<u8> = (0..52).map(|x| (x * 7 + 3) as u8).collect();
        assert_eq!(recover_phone_number(&keyed), None);
    }
}
