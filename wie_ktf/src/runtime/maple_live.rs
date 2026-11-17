//! 메이플스토리 Live (`01042D83`): the warehouse widened past its slots.
//!
//! The warehouse keeps its size in the word at `+0xc` of the account record
//! (`[game + 0x2c]`) and its slots in a fixed array of 96 behind it, with the
//! next table starting right where the 96th slot ends. 창고인벤늘리기 adds six
//! slots, and both of its checks - the candy shop's before it charges, and the
//! grant after - let the purchase through while the size is at most 96. Bought
//! at 96, then, the size becomes 102, and everything that walks the warehouse
//! walks six slots of the next table as well. The candy shop's character
//! upload sizes the warehouse that way the next time it opens, and dereferences
//! a null pointer in one of those six.
//!
//! The two checks are tightened to refuse at more than 90, so the size stops at
//! the 96 slots there are, and the candy shop shows its own "cannot widen any
//! further" message instead. A save already at 102 is read back as 96: the call
//! that loads the size is pointed at a few instructions that cap what it
//! returns. Only the six slots that never existed are lost.

use wie_core_arm::ArmCore;
use wie_util::{ByteRead, ByteWrite, Result};

const AID: &str = "01042D83";

/// The candy shop's check before it charges: `ldr r3,[r0,#0x2c]; ldr
/// r3,[r3,#0xc]; cmp r3,#0x60; ble`, refusing with message `0x5f` above it.
const SHOP_CHECK: u32 = 0x177100;
/// The grant after the purchase, the same check, followed by the `adds r3,#6`
/// that widens.
const GRANT_CHECK: u32 = 0x14cf14;
const GRANT: u32 = 0x14cf2c;

/// Where the save loader reads the size, `bl READ_WORD; str r0,[r4,#0xc]`, and
/// then the 96 slots (`cmp r5,#0x5f; ble`).
const LOAD_SIZE: u32 = 0x11013e;
const LOAD_SLOTS: u32 = 0x110160;
/// The save reader's word read.
const READ_WORD: u32 = 0x10ae20;

/// The slots the warehouse has, and the size at which one more purchase still
/// fits.
const SLOTS: u16 = 96;
const LAST_WIDENABLE: u16 = SLOTS - 6;

/// Installs the fix if this is the title, and the code is the build this was
/// written against. `cap_base` is unmapped memory within a Thumb `bl` of the
/// loader, for the cap.
pub(crate) fn install_warehouse_cap(core: &mut ArmCore, aid: &str, cap_base: u32) -> Result<()> {
    if !aid.eq_ignore_ascii_case(AID) {
        return Ok(());
    }
    if !layout_matches(core)? {
        tracing::warn!("메이플 Live warehouse cap: code differs from the known build; not installed");
        return Ok(());
    }

    let cmp = 0x2b00 | LAST_WIDENABLE; // cmp r3,#90
    core.write_bytes(SHOP_CHECK + 4, &cmp.to_le_bytes())?;
    core.write_bytes(GRANT_CHECK + 4, &cmp.to_le_bytes())?;

    let cap: alloc::vec::Vec<u8> = cap(cap_base).iter().flat_map(|x| x.to_le_bytes()).collect();
    core.load(&cap, cap_base, cap.len())?;
    let call: alloc::vec::Vec<u8> = bl(LOAD_SIZE, cap_base).iter().flat_map(|x| x.to_le_bytes()).collect();
    core.write_bytes(LOAD_SIZE, &call)?;

    tracing::info!("메이플 Live: the warehouse now stops at its {SLOTS} slots");

    Ok(())
}

/// The save reader's word read, capped at the warehouse's slots.
fn cap(base: u32) -> [u16; 7] {
    let [read_hi, read_lo] = bl(base + 2, READ_WORD);

    [
        0xb500, // push {lr}
        read_hi,
        read_lo,        // bl READ_WORD
        0x2800 | SLOTS, // cmp r0,#96
        0xdd00,         // ble +0
        0x2000 | SLOTS, // movs r0,#96
        0xbd00,         // pop {pc}
    ]
}

fn bl(from: u32, to: u32) -> [u16; 2] {
    let offset = to.wrapping_sub(from + 4) as i32;
    debug_assert!((-0x40_0000..0x40_0000).contains(&offset));

    [0xf000 | ((offset >> 12) as u16 & 0x7ff), 0xf800 | ((offset >> 1) as u16 & 0x7ff)]
}

fn layout_matches(core: &ArmCore) -> Result<bool> {
    let shop = halfwords_at(core, SHOP_CHECK, &[0x6ac3, 0x68db, 0x2b60, 0xddbe])?;
    let grant_check = halfwords_at(core, GRANT_CHECK, &[0x6ac3, 0x68db, 0x2b60, 0xdd00])?;
    let grant = halfwords_at(core, GRANT, &[0x6ac2, 0x68d3, 0x3306, 0x60d3])?;
    let load = halfwords_at(core, LOAD_SIZE, &[bl(LOAD_SIZE, READ_WORD)[0], bl(LOAD_SIZE, READ_WORD)[1], 0x60e0])?;
    let slots = halfwords_at(core, LOAD_SLOTS, &[0x9301, 0x2d00 | (SLOTS - 1), 0xddef])?;

    Ok(shop && grant_check && grant && load && slots)
}

fn halfwords_at(core: &ArmCore, address: u32, expected: &[u16]) -> Result<bool> {
    let mut bytes = alloc::vec![0; expected.len() * 2];
    core.read_bytes(address, &mut bytes)?;

    Ok(bytes.chunks(2).map(|x| u16::from_le_bytes([x[0], x[1]])).eq(expected.iter().copied()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_loader_still_calls_the_word_read() {
        // The original call, as it is in the build.
        assert_eq!(bl(LOAD_SIZE, READ_WORD), [0xf7fa, 0xfe6f]);
    }

    #[test]
    fn the_cap_returns_at_most_the_slots_there_are() {
        let cap = cap(0x3ac000);

        assert_eq!(cap[3], 0x2860);
        assert_eq!(cap[5], 0x2060);
        assert_eq!(0x2b00 | LAST_WIDENABLE, 0x2b5a);
    }
}
