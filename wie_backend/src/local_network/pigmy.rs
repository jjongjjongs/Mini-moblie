//! The 피그미 of 메이플스토리 Live, which hatches into an item the server chose.
//!
//! Having raised one, the title asks for what it hatched into with command
//! `0x0058` (built at `0x2fba2`), whose two bytes are what it raised and a
//! zero. For an ordinary 피그미 the first is the character's level bracket,
//! one for levels 1 to 20 and one more for each ten levels after that up to
//! fourteen for 141 and on; 0x15 to 0x17 are the title's other hatchings,
//! which give what they give themselves (`0x34282`: 파워엘릭서 fifteen times
//! over for one of them) and leave the answer's item unread.
//!
//! The answer, `0x0059`, is read at `0x33fcc`:
//!
//! ```text
//! u8   result, zero to go on
//! u32  the item
//! u32  zero for an equipment item (0x1fb2e), anything else an etc one (0x1f9d0)
//! u32  how many
//! u8   a name's length, and the name
//! 35   bytes, and 10 more
//! u8   a message's length, and the message
//! ```
//!
//! What the server picked is not in the title, so the 피그미 here hatches into
//! a piece of equipment from the title's own table (`/data/iequip/equip0.dat`)
//! for the bracket it was raised in. The table gives each item the level it
//! is worn from.
//!
//! 메이플스토리 시그너스기사단 asks the same with command `0x005a` (built at
//! `0xca8e`) and reads `0x005b` laid out the same. Its first byte is the
//! bracket of the same tens up to ten for levels past 100 (`0xcac6`), or
//! `0x0b` to `0x0d` for its other eggs; the second is zero or one. Its
//! equipment table is `/data/item_equip.dat`, 88 bytes an item, the level at
//! `+0x25`.

use alloc::vec::Vec;

pub(super) const REQUEST: u16 = 0x0058;

const GRANTED: u8 = 0;
const EQUIP: u32 = 0;

/// The brackets the title numbers from one.
const BRACKETS: u8 = 14;

/// The level each item of the equipment table is worn from, by its number, or
/// 255 for one a 피그미 does not hatch into: the cash accessories (`0x0d`)
/// and the medals (`0x06`), the unique items (`+5` past one) and those sold to
/// be crafted from (`(제작)`). Nothing in it is worn from past level 130.
#[rustfmt::skip]
const LEVELS: [u8; 874] = [
    255, 255, 255, 255, 255, 255, 1, 1, 10, 10, 15, 15, 15, 15, 15, 255, 20, 20, 20, 20,
    25, 25, 25, 25, 25, 30, 30, 255, 30, 30, 30, 35, 35, 35, 35, 35, 35, 35, 35, 40,
    255, 40, 40, 40, 40, 40, 255, 45, 45, 45, 45, 45, 50, 50, 50, 50, 50, 50, 55, 55,
    55, 55, 55, 60, 255, 60, 60, 255, 60, 65, 65, 65, 65, 65, 65, 70, 70, 70, 70, 70,
    75, 75, 255, 75, 80, 80, 80, 80, 80, 85, 85, 85, 255, 90, 90, 90, 90, 95, 95, 95,
    255, 95, 100, 100, 100, 100, 100, 105, 255, 105, 105, 105, 110, 110, 110, 110, 110, 115, 115, 115,
    115, 115, 120, 120, 255, 255, 120, 10, 10, 20, 20, 20, 20, 30, 30, 30, 30, 40, 255, 40,
    40, 50, 50, 50, 50, 255, 60, 60, 60, 70, 70, 70, 70, 255, 80, 80, 80, 90, 90, 90,
    90, 100, 255, 100, 100, 255, 110, 110, 110, 120, 120, 120, 120, 255, 130, 130, 130, 130, 130, 130,
    130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130,
    130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 1, 10, 15, 15, 20, 60, 30,
    255, 35, 40, 45, 255, 50, 60, 255, 65, 65, 70, 70, 255, 80, 85, 90, 255, 1, 10, 15,
    15, 255, 60, 30, 35, 255, 45, 50, 255, 65, 65, 70, 70, 80, 85, 90, 100, 105, 110, 255,
    255, 120, 10, 15, 15, 20, 60, 30, 35, 40, 45, 50, 60, 65, 65, 70, 70, 255, 85, 90,
    100, 105, 110, 120, 10, 15, 15, 20, 60, 30, 35, 40, 45, 50, 60, 65, 65, 70, 70, 80,
    85, 90, 100, 105, 110, 120, 255, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 255, 130, 130,
    130, 130, 130, 130, 130, 130, 130, 130, 130, 20, 255, 30, 35, 40, 45, 255, 50, 55, 255, 60,
    65, 70, 75, 255, 80, 85, 90, 95, 255, 100, 105, 255, 110, 255, 120, 255, 30, 35, 40, 45,
    50, 255, 60, 65, 70, 75, 80, 85, 90, 95, 255, 105, 110, 120, 130, 130, 130, 130, 130, 130,
    255, 130, 130, 130, 130, 130, 130, 130, 10, 15, 20, 25, 25, 30, 30, 255, 35, 40, 40, 45,
    45, 255, 50, 50, 55, 255, 255, 65, 70, 70, 75, 255, 80, 80, 85, 90, 255, 95, 100, 100,
    105, 105, 255, 110, 255, 115, 120, 120, 120, 10, 15, 255, 25, 25, 30, 30, 35, 40, 40, 45,
    45, 50, 50, 55, 60, 65, 255, 70, 75, 80, 80, 85, 90, 95, 255, 100, 105, 105, 110, 115,
    120, 120, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 255, 130, 255, 130, 130,
    130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120,
    255, 50, 60, 255, 80, 90, 100, 110, 255, 130, 130, 130, 130, 255, 20, 20, 30, 30, 30, 40,
    40, 40, 50, 255, 50, 50, 255, 60, 60, 70, 70, 70, 80, 255, 80, 80, 90, 90, 90, 100,
    100, 255, 110, 110, 130, 130, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
    10, 10, 10, 10, 10, 10, 10, 10, 20, 30, 255, 40, 40, 50, 255, 50, 60, 60, 255, 70,
    70, 80, 255, 80, 90, 255, 90, 255, 100, 110, 255, 110, 255, 255, 120, 10, 255, 30, 255, 40,
    50, 50, 255, 60, 70, 70, 80, 80, 90, 90, 100, 100, 110, 110, 120, 120, 255, 130, 130, 130,
    130, 130, 255, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 10, 20, 20,
    30, 30, 255, 40, 255, 50, 50, 255, 60, 60, 255, 255, 70, 255, 80, 80, 90, 255, 255, 110,
    110, 255, 120, 255, 255, 10, 20, 255, 30, 30, 40, 40, 50, 50, 60, 60, 70, 255, 80, 80,
    90, 100, 110, 110, 120, 120, 255, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130,
    130, 130, 130, 130, 130, 130, 130, 20, 25, 30, 35, 255, 45, 50, 55, 255, 65, 255, 75, 80,
    85, 90, 95, 255, 255, 110, 115, 20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 75, 80,
    85, 90, 95, 100, 105, 110, 115, 120, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130, 130,
    130, 130, 130, 130, 130, 130, 130, 130, 130, 20, 255, 70, 255, 90, 100, 10, 30, 50, 60, 70,
    80, 90, 100, 110, 120, 120, 120, 130, 130, 130, 130, 255, 130, 130, 130, 130, 255, 130, 130, 130,
    130, 130, 130, 130, 130, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 130,
];

/// 시그너스기사단's equipment table as [`LEVELS`] keeps Live's: the level each
/// item is worn from, or 255 for one a 피그미 does not hatch into - the
/// medals (kind 6), the beginners' items worn from level 0, those to be
/// crafted from (`(제작)`), and the sets a quest gives, the 메이플 ones
/// marked `2` at `+0x23` and the 기사단 ones.
#[rustfmt::skip]
const CYGNUS_LEVELS: [u8; 541] = [
    10, 10, 20, 20, 30, 30, 40, 255, 45, 45, 50, 50, 60, 60, 255, 70, 70, 80, 80, 255,
    90, 90, 100, 100, 255, 110, 110, 255, 10, 20, 20, 25, 30, 30, 35, 40, 40, 255, 45, 50,
    50, 55, 60, 60, 255, 65, 70, 70, 75, 80, 80, 255, 85, 90, 90, 95, 100, 100, 255, 105,
    110, 255, 110, 10, 10, 20, 20, 30, 35, 35, 40, 255, 45, 45, 50, 55, 55, 60, 255, 65,
    65, 70, 75, 75, 80, 80, 255, 90, 90, 100, 255, 110, 255, 10, 10, 20, 20, 30, 35, 35,
    40, 255, 45, 45, 50, 55, 55, 60, 255, 65, 65, 70, 75, 75, 80, 80, 255, 90, 90, 100,
    255, 110, 110, 255, 10, 10, 20, 20, 30, 35, 35, 40, 255, 45, 45, 50, 55, 55, 60, 255,
    65, 65, 70, 75, 75, 80, 80, 255, 90, 90, 100, 255, 110, 255, 10, 20, 25, 25, 30, 35,
    35, 40, 255, 45, 45, 50, 55, 55, 60, 255, 65, 65, 70, 75, 75, 80, 255, 85, 85, 90,
    95, 95, 100, 255, 105, 105, 110, 255, 115, 10, 10, 20, 20, 30, 35, 35, 40, 255, 45, 45,
    50, 55, 55, 60, 255, 65, 65, 70, 76, 76, 80, 80, 255, 90, 90, 100, 255, 110, 255, 10,
    10, 20, 20, 30, 35, 35, 40, 255, 45, 45, 50, 55, 55, 60, 255, 65, 65, 70, 75, 75,
    80, 80, 255, 90, 90, 100, 255, 110, 255, 10, 10, 20, 20, 30, 35, 35, 40, 255, 45, 45,
    50, 55, 55, 60, 255, 65, 65, 70, 75, 75, 80, 80, 255, 90, 90, 100, 255, 110, 255, 10,
    20, 25, 25, 30, 35, 35, 40, 255, 45, 45, 50, 55, 55, 60, 255, 65, 65, 70, 75, 75,
    80, 255, 85, 85, 90, 95, 95, 100, 255, 105, 105, 110, 255, 115, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 10, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 30, 50, 70, 90, 110, 115, 30, 50, 70, 90, 100,
    110, 30, 50, 70, 90, 110, 115, 30, 50, 70, 90, 110, 115, 30, 50, 70, 90, 110, 115, 30,
    50, 70, 90, 110, 115, 30, 50, 70, 90, 110, 115, 30, 50, 70, 90, 110, 115, 30, 50, 70,
    90, 110, 115, 30, 50, 70, 90, 110, 115, 40, 70, 100, 115, 45, 65, 80, 95, 105, 115, 50,
    70, 85, 105, 115, 50, 70, 85, 105, 115, 50, 70, 85, 105, 115, 50, 70, 85, 100, 110, 115,
    50, 70, 85, 105, 115, 50, 70, 85, 105, 115, 50, 70, 85, 105, 115, 50, 70, 85, 100, 110,
    115, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255,
];

/// 시그너스기사단's last bracket, levels 101 and on, which reaches its
/// table's last items, worn from 115.
const CYGNUS_BRACKETS: u8 = 10;

/// What a 피그미 raised as `body` hatches into on its `draw`th hatching, or
/// `None` for a request that is not a hatching.
pub(super) fn answer(body: &[u8], draw: u32) -> Option<Vec<u8>> {
    let [bracket, 0] = *body else {
        return None;
    };

    Some(granted(hatch(bracket, draw)))
}

/// What 시그너스기사단's 피그미 raised as `body` hatches into on its `draw`th
/// hatching: equipment worn from within twenty levels below its bracket's
/// top, or for one of its other eggs anything the table holds.
pub(crate) fn cygnus_answer(body: &[u8], draw: u32) -> Option<Vec<u8>> {
    let [bracket, 0 | 1] = *body else {
        return None;
    };

    let levels = if (1..=CYGNUS_BRACKETS).contains(&bracket) {
        let top = 10 * u32::from(bracket) + if bracket == CYGNUS_BRACKETS { 20 } else { 10 };
        top - 20..=top
    } else {
        0..=u32::from(u8::MAX - 1)
    };
    Some(granted(pick(&CYGNUS_LEVELS, levels, draw)))
}

/// The answer granting `item`, one piece of equipment.
fn granted(item: u16) -> Vec<u8> {
    let mut answer = alloc::vec![GRANTED];
    answer.extend_from_slice(&u32::from(item).to_le_bytes());
    answer.extend_from_slice(&EQUIP.to_le_bytes());
    answer.extend_from_slice(&1u32.to_le_bytes());
    answer.push(0); // the name's length
    answer.extend_from_slice(&[0; 0x23 + 0x0a]);
    answer.push(0); // the message's
    answer
}

/// The item one hatching in `bracket` gives: one worn from up to twenty
/// levels below the bracket's top, which reaches the table's last items from
/// bracket 12 on. The title's own hatchings are answered as the first bracket.
fn hatch(bracket: u8, draw: u32) -> u16 {
    let bracket = if (1..=BRACKETS).contains(&bracket) { bracket } else { 1 };
    let top = 10 * u32::from(bracket) + 10;
    pick(&LEVELS, top - 20..=top, draw)
}

/// The `draw`th of the items of `table` worn from within `levels`.
fn pick(table: &[u8], levels: core::ops::RangeInclusive<u32>, draw: u32) -> u16 {
    let fits = |level: &u8| *level != 255 && levels.contains(&u32::from(*level));

    let count = table.iter().filter(|level| fits(level)).count() as u32;
    if count == 0 {
        return 0;
    }
    let nth = mix(draw) % count;
    table
        .iter()
        .enumerate()
        .filter(|(_, level)| fits(level))
        .nth(nth as usize)
        .map_or(0, |(item, _)| item as u16)
}

/// Spreads consecutive draws over the pool (a 32-bit finaliser of MurmurHash3).
fn mix(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x85eb_ca6b);
    x ^= x >> 13;
    x = x.wrapping_mul(0xc2b2_ae35);
    x ^ (x >> 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_bracket_hatches_equipment_worn_within_it() {
        for bracket in 1..=BRACKETS {
            let top = 10 * u32::from(bracket) + 10;
            for draw in 0..64 {
                let level = u32::from(LEVELS[hatch(bracket, draw) as usize]);
                assert!((top - 20..=top).contains(&level), "bracket {bracket} gave level {level}");
            }
        }
    }

    #[test]
    fn the_answer_is_laid_out_as_the_title_reads_it() {
        let answer = answer(&[3, 0], 7).unwrap();
        assert_eq!(answer.len(), 14 + 0x23 + 0x0a + 1);
        assert_eq!(answer[0], GRANTED);
        assert_eq!(u32::from_le_bytes(answer[1..5].try_into().unwrap()), u32::from(hatch(3, 7)));
        assert_eq!(&answer[5..13], &[0, 0, 0, 0, 1, 0, 0, 0]);

        assert_eq!(super::answer(&[3], 7), None);
        assert_eq!(super::answer(&[3, 1], 7), None);
    }

    #[test]
    fn cygnus_hatches_equipment_worn_within_its_bracket() {
        for bracket in 1..=CYGNUS_BRACKETS {
            let top = 10 * u32::from(bracket) + if bracket == CYGNUS_BRACKETS { 20 } else { 10 };
            let items: Vec<u32> = (0..64)
                .map(|draw| u32::from_le_bytes(cygnus_answer(&[bracket, 0], draw).unwrap()[1..5].try_into().unwrap()))
                .collect();
            for &item in &items {
                let level = u32::from(CYGNUS_LEVELS[item as usize]);
                assert!((top - 20..=top).contains(&level), "bracket {bracket} gave level {level}");
            }
            assert!(items.windows(2).any(|pair| pair[0] != pair[1]), "bracket {bracket} always the same");
        }

        // Item 0, 사각나무방패, is worn from 10 and not the only one there.
        assert_eq!(CYGNUS_LEVELS[0], 10);
        // Another egg, and a first hatching's flag, are hatchings too.
        assert!(cygnus_answer(&[0x0b, 0], 3).is_some());
        assert!(cygnus_answer(&[2, 1], 3).is_some());
        assert_eq!(cygnus_answer(&[2], 3), None);
    }

    #[test]
    fn hatchings_differ() {
        let items: Vec<u16> = (0..8).map(|draw| hatch(5, draw)).collect();
        assert!(items.windows(2).any(|pair| pair[0] != pair[1]));
    }
}
