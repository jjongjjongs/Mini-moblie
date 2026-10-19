//! Recovery of the subscriber number an EA Mobile Korea title was registered
//! to, out of its `kickass` store.
//!
//! EA's KTF titles keep their saves in one record store named `kickass`: a
//! `u32`, a table of forty twenty-byte entries - a NUL-padded sixteen-byte name,
//! a byte, how many 256-byte slots the file takes, its size as a little-endian
//! `u16` - and then the files, each starting on its slot, the first right after
//! the table. One of them is the 인증: the subscriber number the copy was
//! registered to, which the title reads on start and `strcmp`s with
//! `PHONENUMBER`, stopping at `[인증실패] 인증실패로 인해 게임을 더이상 진행하실
//! 수 없습니다` when the two differ. A dump carries its owner's, so that is the
//! number this handset has to report.
//!
//! It comes two ways:
//!
//! - EA프로야구2011 (`01042B35`) keeps it as `EBB2_certi.sav`, the number in
//!   twelve bytes as it stands.
//! - 영웅서기5 (`01042B12`) keeps it as `Heroslore5`, sealed the way every file in
//!   its store is (`0x22b64c`): the MD5 of the plaintext, then the plaintext
//!   under DES with the key `0EP@KO91` (`0x16b3dc`) - sixteen bytes, the number
//!   and zero fill. The DES is the standard one but for the halves: the title's
//!   block routine (`0x196cbc`) leaves out the swap before the final
//!   permutation when it encrypts, and swaps the halves after the initial one
//!   when it decrypts.

use alloc::string::{String, ToString};

/// The store's own table: the leading word, and forty entries.
const ENTRIES: usize = 40;
const ENTRY_LEN: usize = 20;
const TABLE_END: usize = 4 + ENTRIES * ENTRY_LEN;
const SLOT_LEN: usize = 256;

/// Where 영웅서기5's sealed record keeps its digest, and the key it is sealed
/// under.
const DIGEST_LEN: usize = 16;
const SEAL_KEY: &[u8; 8] = b"0EP@KO91";

/// The subscriber number an EA `kickass` store names, or `None` when the bytes
/// are not one or name none.
pub fn recover_phone_number(store: &[u8]) -> Option<String> {
    if store.len() < TABLE_END {
        return None;
    }

    let mut slot = 0usize;
    for entry in store[4..TABLE_END].chunks_exact(ENTRY_LEN) {
        let name_end = entry[..16].iter().position(|&b| b == 0).unwrap_or(16);
        let name = &entry[..name_end];
        let slots = entry[17] as usize;
        let size = u16::from_le_bytes([entry[18], entry[19]]) as usize;

        let at = TABLE_END + slot * SLOT_LEN;
        slot += slots;

        if name.is_empty() || size > slots * SLOT_LEN {
            continue;
        }
        let Some(file) = store.get(at..at + size) else {
            continue;
        };

        let number = if name.ends_with(b"_certi.sav") {
            number(file)
        } else if name == b"Heroslore5" {
            unseal(file).and_then(|plain| number(&plain))
        } else {
            None
        };

        if number.is_some() {
            return number;
        }
    }

    None
}

/// A NUL-terminated subscriber number with nothing but zeroes after it.
fn number(bytes: &[u8]) -> Option<String> {
    let end = bytes.iter().position(|&b| b == 0)?;
    let digits = &bytes[..end];
    if !(10..=11).contains(&digits.len()) || digits[0] != b'0' || !digits.iter().all(u8::is_ascii_digit) || bytes[end..].iter().any(|&b| b != 0) {
        return None;
    }

    Some(core::str::from_utf8(digits).ok()?.to_string())
}

/// The plaintext of one of 영웅서기5's sealed files. The digest in front is not
/// checked; [`number`] refuses anything that did not open to a number.
fn unseal(file: &[u8]) -> Option<alloc::vec::Vec<u8>> {
    let sealed = file.get(DIGEST_LEN..)?;
    if sealed.is_empty() || sealed.len() % 8 != 0 {
        return None;
    }

    let subkeys = des::subkeys(u64::from_be_bytes(*SEAL_KEY));
    Some(
        sealed
            .chunks_exact(8)
            .flat_map(|block| des::decrypt_unswapped(u64::from_be_bytes(block.try_into().unwrap()), &subkeys).to_be_bytes())
            .collect(),
    )
}

/// DES, as far as decryption goes, in the variant 영웅서기5's store is sealed
/// with.
mod des {
    const IP: [u8; 64] = [
        58, 50, 42, 34, 26, 18, 10, 2, 60, 52, 44, 36, 28, 20, 12, 4, 62, 54, 46, 38, 30, 22, 14, 6, 64, 56, 48, 40, 32, 24, 16, 8, 57, 49, 41, 33,
        25, 17, 9, 1, 59, 51, 43, 35, 27, 19, 11, 3, 61, 53, 45, 37, 29, 21, 13, 5, 63, 55, 47, 39, 31, 23, 15, 7,
    ];
    const FP: [u8; 64] = [
        40, 8, 48, 16, 56, 24, 64, 32, 39, 7, 47, 15, 55, 23, 63, 31, 38, 6, 46, 14, 54, 22, 62, 30, 37, 5, 45, 13, 53, 21, 61, 29, 36, 4, 44, 12,
        52, 20, 60, 28, 35, 3, 43, 11, 51, 19, 59, 27, 34, 2, 42, 10, 50, 18, 58, 26, 33, 1, 41, 9, 49, 17, 57, 25,
    ];
    const E: [u8; 48] = [
        32, 1, 2, 3, 4, 5, 4, 5, 6, 7, 8, 9, 8, 9, 10, 11, 12, 13, 12, 13, 14, 15, 16, 17, 16, 17, 18, 19, 20, 21, 20, 21, 22, 23, 24, 25, 24, 25,
        26, 27, 28, 29, 28, 29, 30, 31, 32, 1,
    ];
    const P: [u8; 32] = [
        16, 7, 20, 21, 29, 12, 28, 17, 1, 15, 23, 26, 5, 18, 31, 10, 2, 8, 24, 14, 32, 27, 3, 9, 19, 13, 30, 6, 22, 11, 4, 25,
    ];
    const PC1: [u8; 56] = [
        57, 49, 41, 33, 25, 17, 9, 1, 58, 50, 42, 34, 26, 18, 10, 2, 59, 51, 43, 35, 27, 19, 11, 3, 60, 52, 44, 36, 63, 55, 47, 39, 31, 23, 15, 7,
        62, 54, 46, 38, 30, 22, 14, 6, 61, 53, 45, 37, 29, 21, 13, 5, 28, 20, 12, 4,
    ];
    const PC2: [u8; 48] = [
        14, 17, 11, 24, 1, 5, 3, 28, 15, 6, 21, 10, 23, 19, 12, 4, 26, 8, 16, 7, 27, 20, 13, 2, 41, 52, 31, 37, 47, 55, 30, 40, 51, 45, 33, 48, 44,
        49, 39, 56, 34, 53, 46, 42, 50, 36, 29, 32,
    ];
    const SHIFTS: [u32; 16] = [1, 1, 2, 2, 2, 2, 2, 2, 1, 2, 2, 2, 2, 2, 2, 1];
    #[rustfmt::skip]
    const S: [[u8; 64]; 8] = [
        [14, 4, 13, 1, 2, 15, 11, 8, 3, 10, 6, 12, 5, 9, 0, 7, 0, 15, 7, 4, 14, 2, 13, 1, 10, 6, 12, 11, 9, 5, 3, 8,
         4, 1, 14, 8, 13, 6, 2, 11, 15, 12, 9, 7, 3, 10, 5, 0, 15, 12, 8, 2, 4, 9, 1, 7, 5, 11, 3, 14, 10, 0, 6, 13],
        [15, 1, 8, 14, 6, 11, 3, 4, 9, 7, 2, 13, 12, 0, 5, 10, 3, 13, 4, 7, 15, 2, 8, 14, 12, 0, 1, 10, 6, 9, 11, 5,
         0, 14, 7, 11, 10, 4, 13, 1, 5, 8, 12, 6, 9, 3, 2, 15, 13, 8, 10, 1, 3, 15, 4, 2, 11, 6, 7, 12, 0, 5, 14, 9],
        [10, 0, 9, 14, 6, 3, 15, 5, 1, 13, 12, 7, 11, 4, 2, 8, 13, 7, 0, 9, 3, 4, 6, 10, 2, 8, 5, 14, 12, 11, 15, 1,
         13, 6, 4, 9, 8, 15, 3, 0, 11, 1, 2, 12, 5, 10, 14, 7, 1, 10, 13, 0, 6, 9, 8, 7, 4, 15, 14, 3, 11, 5, 2, 12],
        [7, 13, 14, 3, 0, 6, 9, 10, 1, 2, 8, 5, 11, 12, 4, 15, 13, 8, 11, 5, 6, 15, 0, 3, 4, 7, 2, 12, 1, 10, 14, 9,
         10, 6, 9, 0, 12, 11, 7, 13, 15, 1, 3, 14, 5, 2, 8, 4, 3, 15, 0, 6, 10, 1, 13, 8, 9, 4, 5, 11, 12, 7, 2, 14],
        [2, 12, 4, 1, 7, 10, 11, 6, 8, 5, 3, 15, 13, 0, 14, 9, 14, 11, 2, 12, 4, 7, 13, 1, 5, 0, 15, 10, 3, 9, 8, 6,
         4, 2, 1, 11, 10, 13, 7, 8, 15, 9, 12, 5, 6, 3, 0, 14, 11, 8, 12, 7, 1, 14, 2, 13, 6, 15, 0, 9, 10, 4, 5, 3],
        [12, 1, 10, 15, 9, 2, 6, 8, 0, 13, 3, 4, 14, 7, 5, 11, 10, 15, 4, 2, 7, 12, 9, 5, 6, 1, 13, 14, 0, 11, 3, 8,
         9, 14, 15, 5, 2, 8, 12, 3, 7, 0, 4, 10, 1, 13, 11, 6, 4, 3, 2, 12, 9, 5, 15, 10, 11, 14, 1, 7, 6, 0, 8, 13],
        [4, 11, 2, 14, 15, 0, 8, 13, 3, 12, 9, 7, 5, 10, 6, 1, 13, 0, 11, 7, 4, 9, 1, 10, 14, 3, 5, 12, 2, 15, 8, 6,
         1, 4, 11, 13, 12, 3, 7, 14, 10, 15, 6, 8, 0, 5, 9, 2, 6, 11, 13, 8, 1, 4, 10, 7, 9, 5, 0, 15, 14, 2, 3, 12],
        [13, 2, 8, 4, 6, 15, 11, 1, 10, 9, 3, 14, 5, 0, 12, 7, 1, 15, 13, 8, 10, 3, 7, 4, 12, 5, 6, 11, 0, 14, 9, 2,
         7, 11, 4, 1, 9, 12, 14, 2, 0, 6, 10, 13, 15, 3, 5, 8, 2, 1, 14, 7, 4, 10, 8, 13, 15, 12, 9, 0, 3, 5, 6, 11],
    ];

    /// The bits of `input` - `width` of them, the first the most significant -
    /// that `table` names, in its order, one-based as DES writes its tables.
    fn permute(input: u64, table: &[u8], width: u32) -> u64 {
        table.iter().fold(0, |out, &bit| (out << 1) | ((input >> (width - bit as u32)) & 1))
    }

    /// The sixteen round keys, in encryption order.
    pub fn subkeys(key: u64) -> [u64; 16] {
        let permuted = permute(key, &PC1, 64);
        let (mut c, mut d) = (permuted >> 28, permuted & 0x0fff_ffff);

        let mut keys = [0; 16];
        for (round, key) in keys.iter_mut().enumerate() {
            let shift = SHIFTS[round];
            c = ((c << shift) | (c >> (28 - shift))) & 0x0fff_ffff;
            d = ((d << shift) | (d >> (28 - shift))) & 0x0fff_ffff;
            *key = permute((c << 28) | d, &PC2, 56);
        }

        keys
    }

    fn feistel(right: u32, key: u64) -> u32 {
        let expanded = permute(right as u64, &E, 32) ^ key;
        let substituted = (0..8).fold(0u64, |out, box_index| {
            let six = (expanded >> (42 - 6 * box_index)) & 0x3f;
            let row = ((six >> 4) & 2) | (six & 1);
            let column = (six >> 1) & 0xf;
            (out << 4) | S[box_index][(row * 16 + column) as usize] as u64
        });

        permute(substituted, &P, 32) as u32
    }

    /// One block decrypted with the halves taken the other way round after the
    /// initial permutation, which is what undoes an encryption that skipped the
    /// swap before the final one.
    pub fn decrypt_unswapped(block: u64, subkeys: &[u64; 16]) -> u64 {
        let permuted = permute(block, &IP, 64);
        let (mut left, mut right) = (permuted as u32, (permuted >> 32) as u32);

        for key in subkeys.iter().rev() {
            (left, right) = (right, left ^ feistel(right, *key));
        }

        permute(((right as u64) << 32) | left as u64, &FP, 64)
    }
}

#[cfg(test)]
mod tests {
    use alloc::{string::ToString, vec, vec::Vec};

    use super::{SEAL_KEY, des, recover_phone_number};

    /// A store with the given files in its first entries.
    fn store(files: &[(&[u8], &[u8])]) -> Vec<u8> {
        let mut store = vec![0u8; 4 + 40 * 20];
        let mut data = Vec::new();
        for (index, (name, file)) in files.iter().enumerate() {
            let entry = &mut store[4 + index * 20..4 + index * 20 + 20];
            entry[..name.len()].copy_from_slice(name);
            let slots = file.len().div_ceil(256).max(1);
            entry[17] = slots as u8;
            entry[18..20].copy_from_slice(&(file.len() as u16).to_le_bytes());
            let mut padded = file.to_vec();
            padded.resize(slots * 256, 0);
            data.extend_from_slice(&padded);
        }
        store.extend_from_slice(&data);
        store
    }

    /// The block 영웅서기5's store is full of, which is eight zero bytes sealed.
    #[test]
    fn the_seal_opens_its_own_zero_block() {
        let subkeys = des::subkeys(u64::from_be_bytes(*SEAL_KEY));
        assert_eq!(des::decrypt_unswapped(0x930d_32a2_9235_e809, &subkeys), 0);
    }

    /// EA프로야구2011's, as its dump ships it.
    #[test]
    fn a_plain_certificate_is_read() {
        let store = store(&[(b"0.sav", &[0, 7, 1, 1, 1, 1, 2, 0, 0, 0, 1, 1]), (b"EBB2_certi.sav", b"01068488891\0")]);
        assert_eq!(recover_phone_number(&store), Some("01068488891".to_string()));
    }

    /// 영웅서기5's `Heroslore5`, as its dump ships it, behind a file of two slots.
    #[test]
    fn a_sealed_certificate_is_opened() {
        let heroslore5: [u8; 32] = [
            0x13, 0x5b, 0xfe, 0x07, 0x06, 0xe6, 0x44, 0x7b, 0x81, 0x63, 0x59, 0xa2, 0x4b, 0x4f, 0x79, 0x2b, 0xda, 0x55, 0x22, 0xdb, 0x99, 0x6c, 0x39,
            0xce, 0x1e, 0x22, 0x28, 0x3e, 0xf5, 0x9d, 0xed, 0x3a,
        ];
        let store = store(&[(b"M.sav", &[0x5a; 408]), (b"Heroslore5", &heroslore5)]);
        assert_eq!(recover_phone_number(&store), Some("01068488891".to_string()));
    }

    #[test]
    fn a_store_without_one_names_none() {
        assert_eq!(recover_phone_number(&store(&[(b"0.sav", &[1; 12])])), None);
        assert_eq!(recover_phone_number(&[0u8; 100]), None);
    }
}
