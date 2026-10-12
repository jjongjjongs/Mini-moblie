//! 메이플스토리 시그너스기사단 (`0002E88E`): the save-slot item bought from a
//! town merchant.
//!
//! 세이브슬롯늘리기 (etc item `0xee`) is a cash-shop item, and its effect lives
//! in the cash shop's purchase code: the game adds a save slot (to at most six)
//! once that purchase completes. The item itself does nothing - using it from
//! the bag is not a thing the game knows. A modded copy of this title,
//! "로컬캐시상점", sells cash items in the town merchants' lists instead
//! (`data/market.dat` is the only file it changes). Bought there, the item goes
//! through the merchant's ordinary etc-item purchase and lands in the bag
//! without any effect, so the slot count never moves.
//!
//! This gives the merchant purchase the cash shop's effect. The shop's
//! purchase states are dispatched through a jump table (`0x90d38`); state 3 is
//! the merchant's etc-item purchase after its quantity prompt. Its table entry
//! is pointed at a routine that, for item `0xee` only, does what the cash
//! shop does on a completed purchase - and charges the merchant's price in
//! meso instead of cash. Every other item goes on to the original state 3
//! untouched.
//!
//! The routine adds one slot for each item bought, up to the six slots the
//! game allows, and charges only for the slots it added. With no room left it
//! shows the cash shop's own refusal and charges nothing. The original
//! unmodded title never sells this item from a merchant, so it is unaffected.

use alloc::vec::Vec;

use wie_core_arm::ArmCore;
use wie_util::{ByteRead, ByteWrite, Result, read_generic};

const AID: &str = "0002E88E";

/// Where the routine is placed: unmapped in this title, between its code
/// (which ends below `0xa3000`) and its data (`0x1400000`), and close enough
/// to the code for a Thumb `bl` both ways.
const ROUTINE_BASE: u32 = 0x10_0000;

/// The shop's purchase-state jump table, which starts at state 2, and the
/// state-3 entry in it.
const STATE_TABLE: u32 = 0xa20f0;
const STATE3_ENTRY: u32 = STATE_TABLE + 4;

/// State 3, the merchant's etc-item purchase.
const STATE3: u32 = 0x90df0;
/// The dispatcher's two exits: clear the shop's state to -1 and return, or
/// return keeping the state just set.
const CLEAR_STATE: u32 = 0x9104a;
const KEEP_STATE: u32 = 0x91050;
/// `(inventory, amount, 0)`: adds `amount` meso, returning 0 when a charge
/// is more than the player has.
const ADD_MESO: u32 = 0x1a34c;
/// The two calls the cash shop makes to save after changing the slot count:
/// the account file (`cyguser`), then its index.
const SAVE_USER: u32 = 0xa4d4;
const SAVE_INDEX: u32 = 0x14334;
/// Redraws the shop's meso readout, as a merchant purchase does after paying.
const REFRESH: u32 = 0x53f8c;
/// `(game, -1, text, 3)`: the message box the cash shop reports through.
const SHOW_MESSAGE: u32 = 0x448c;

/// The game's global object, whose `[+0x48]` holds the slot count at `+8`.
const GAME: u32 = 0x150_0004;
/// "세이브 슬롯이 1칸 추가 되었습니다." and "세이브 슬롯을 더 이상 확장할 수
/// 없습니다. 구매에 실패하였습니다.", the cash shop's two messages.
const ADDED_TEXT: u32 = 0x9ab54;
const FULL_TEXT: u32 = 0xa1eb4;

/// Installs the routine if this is the title, and the code it calls and
/// returns to is where this was written against.
pub(crate) fn install_merchant_save_slots(core: &mut ArmCore, aid: &str) -> Result<()> {
    if !aid.eq_ignore_ascii_case(AID) {
        return Ok(());
    }
    if !layout_matches(core)? {
        tracing::warn!("시그너스 merchant save-slot routine: code differs from the known build; not installed");
        return Ok(());
    }

    install(core)?;
    tracing::info!("시그너스: the merchant's 세이브슬롯늘리기 now adds a save slot");

    Ok(())
}

fn install(core: &mut ArmCore) -> Result<()> {
    let code: Vec<u8> = routine().iter().flat_map(|x| x.to_le_bytes()).collect();
    core.load(&code, ROUTINE_BASE, code.len())?;
    // The dispatcher enters a state with `mov pc`, so the entry is the
    // routine's address without the Thumb bit, as the table's own are.
    core.write_bytes(STATE3_ENTRY, &ROUTINE_BASE.to_le_bytes())
}

/// The routine, entered from the dispatcher with `r7` the shop and `r4`-`r7`
/// and `lr` already saved by its prologue, which its exits restore.
fn routine() -> Vec<u16> {
    let mut code = Vec::new();
    let mut emit = |halfwords: &[u16]| code.extend_from_slice(halfwords);

    // Anything but item 0xee goes on to the original state 3.
    emit(&[0x6e3b, 0x33cc, 0x681b, 0x2bee]); // 00 ldr r3,[r7,#0x60]; adds r3,#0xcc; ldr r3,[r3]; cmp r3,#0xee
    emit(&[0xd001]); // 08 beq 0e
    emit(&bl(ROUTINE_BASE + 0x0a, STATE3)); // 0a bl STATE3

    // The quantity bought, and the room left: six slots less those there are.
    emit(&[0x4c1a]); // 0e ldr r4,=GAME
    emit(&[0x68fe, 0x2e00, 0xdd2e]); // 10 ldr r6,[r7,#0xc]; cmp r6,#0; ble done
    emit(&[0x6820, 0x6c80, 0x7a05]); // 16 ldr r0,[r4]; ldr r0,[r0,#0x48]; ldrb r5,[r0,#8]
    emit(&[0x2306, 0x1b5d, 0xdd1d]); // 1c movs r3,#6; subs r5,r3,r5; ble full
    emit(&[0x42ae, 0xda00, 0x1c35]); // 22 cmp r6,r5; bge charge; adds r5,r6,#0

    // Charge the merchant's price for each slot added; a charge the player
    // cannot pay ends the purchase as the original does.
    emit(&[0x6e3b, 0x33d8, 0x6819]); // 28 ldr r3,[r7,#0x60]; adds r3,#0xd8; ldr r1,[r3]
    emit(&[0x4369, 0x4249]); // 2e muls r1,r5,r1; rsbs r1,r1,#0
    emit(&[0x6dfb, 0x6f18, 0x2200]); // 32 ldr r3,[r7,#0x5c]; ldr r0,[r3,#0x70]; movs r2,#0
    emit(&bl(ROUTINE_BASE + 0x38, ADD_MESO)); // 38 bl ADD_MESO
    emit(&[0x0600, 0xd019]); // 3c lsls r0,r0,#24; beq done

    // Add the slots and save, as the cash shop does.
    emit(&[0x6820, 0x6c82, 0x7a13, 0x195b, 0x7213]); // 40 ldr r0,[r4]; ldr r2,[r0,#0x48]; ldrb r3,[r2,#8]; adds r3,r3,r5; strb r3,[r2,#8]
    emit(&bl(ROUTINE_BASE + 0x4a, SAVE_USER)); // 4a bl SAVE_USER
    emit(&[0x6820]); // 4e ldr r0,[r4]
    emit(&bl(ROUTINE_BASE + 0x50, SAVE_INDEX)); // 50 bl SAVE_INDEX
    emit(&[0x6df8]); // 54 ldr r0,[r7,#0x5c]
    emit(&bl(ROUTINE_BASE + 0x56, REFRESH)); // 56 bl REFRESH
    emit(&[0x4a08, 0xe000]); // 5a ldr r2,=ADDED_TEXT; b message
    emit(&[0x4a08]); // 5e full: ldr r2,=FULL_TEXT

    // The message box, under the state the cash shop shows its refusal in.
    emit(&[0x230d, 0x643b]); // 60 message: movs r3,#0xd; str r3,[r7,#0x40]
    emit(&[0x6820, 0x2101, 0x4249, 0x2303]); // 64 ldr r0,[r4]; movs r1,#1; rsbs r1,r1,#0; movs r3,#3
    emit(&bl(ROUTINE_BASE + 0x6c, SHOW_MESSAGE)); // 6c bl SHOW_MESSAGE
    emit(&bl(ROUTINE_BASE + 0x70, KEEP_STATE)); // 70 bl KEEP_STATE
    emit(&bl(ROUTINE_BASE + 0x74, CLEAR_STATE)); // 74 done: bl CLEAR_STATE

    for word in [GAME, ADDED_TEXT, FULL_TEXT] {
        emit(&[word as u16, (word >> 16) as u16]); // 78, 7c, 80
    }

    code
}

/// A Thumb `bl` at `from` to the Thumb code at `to`.
fn bl(from: u32, to: u32) -> [u16; 2] {
    let offset = to.wrapping_sub(from + 4) as i32;
    debug_assert!((-0x40_0000..0x40_0000).contains(&offset));

    [0xf000 | ((offset >> 12) as u16 & 0x7ff), 0xf800 | ((offset >> 1) as u16 & 0x7ff)]
}

/// Whether the code and data the routine relies on are where it expects.
fn layout_matches(core: &ArmCore) -> Result<bool> {
    // The dispatcher's `ldr r2,=table; lsls; ldr; mov pc`, its table, and
    // state 3's entry in it.
    let dispatch = halfwords_at(core, 0x90d46, &[0x4ac4, 0x009b, 0x589b, 0x469f])?;
    let table = read_generic::<u32, _>(core, 0x91058)? == STATE_TABLE;
    let state3 = read_generic::<u32, _>(core, STATE3_ENTRY)? == STATE3;

    // State 3 starts by reading the quantity; the exits.
    let original = halfwords_at(core, STATE3, &[0x68c3, 0x2b00, 0xdc00])?;
    let exits = halfwords_at(core, CLEAR_STATE, &[0x2301, 0x425b, 0x643b, 0xbcf0, 0xbc01, 0x4700])?;

    // The cash shop's own effect: `bl SAVE_USER; ldr r0,[sp,#0x20]; bl
    // SAVE_INDEX`, the meso and refresh calls state 3 makes through its
    // literal pool, and the message call the cash shop's refusal makes.
    let saves = halfwords_at(core, 0x17538, &[0xf7f2, 0xffcc, 0x9808, 0xf7fc, 0xfef9])?;
    let meso = read_generic::<u32, _>(core, 0x91068)? == ADD_MESO | 1;
    let refresh = read_generic::<u32, _>(core, 0x9107c)? == REFRESH | 1;
    let message = read_generic::<u32, _>(core, 0x91074)? == SHOW_MESSAGE | 1;
    let game = read_generic::<u32, _>(core, 0x91070)? == GAME;

    let texts = text_at(core, ADDED_TEXT)? == "세이브 슬롯이 1칸 추가 되었습니다."
        && text_at(core, FULL_TEXT)?.starts_with("세이브 슬롯을 더 이상 확장할 수 없습니다.");

    Ok(dispatch && table && state3 && original && exits && saves && meso && refresh && message && game && texts)
}

fn halfwords_at(core: &ArmCore, address: u32, expected: &[u16]) -> Result<bool> {
    let mut bytes = alloc::vec![0; expected.len() * 2];
    core.read_bytes(address, &mut bytes)?;

    Ok(bytes.chunks(2).map(|x| u16::from_le_bytes([x[0], x[1]])).eq(expected.iter().copied()))
}

fn text_at(core: &ArmCore, address: u32) -> Result<alloc::string::String> {
    let mut bytes = [0; 96];
    core.read_bytes(address, &mut bytes)?;
    let end = bytes.iter().position(|&x| x == 0).unwrap_or(bytes.len());

    Ok(encoding_rs::EUC_KR.decode_without_bom_handling(&bytes[..end]).0.into_owned())
}

#[cfg(test)]
mod test {
    use alloc::{boxed::Box, sync::Arc};
    use core::sync::atomic::{AtomicBool, Ordering};

    use wie_backend::{DefaultTaskRunner, System};
    use wie_core_arm::{Allocator, ArmCore};
    use wie_util::{ByteWrite, Result, read_generic, write_generic};

    use test_utils::TestPlatform;

    use super::*;

    /// Where the stubs record what they were called with: a word each, and
    /// the value each returns beside it.
    const RECORDS: u32 = 0x150_1000;
    const SHOP: u32 = 0x150_2000;
    const ITEM: u32 = 0x150_2100;
    const PLAYER: u32 = 0x150_2200;
    const INVENTORY: u32 = 0x150_2300;
    const GAME_OBJECT: u32 = 0x150_2400;
    const SLOTS: u32 = 0x150_2500;
    /// Written by the stand-in for the original state 3.
    const ORIGINAL: u32 = RECORDS + 0x40;

    struct Stub {
        address: u32,
        /// The register it records: `r0`-`r2`.
        register: u16,
        record: u32,
    }

    const STUBS: [Stub; 5] = [
        Stub {
            address: ADD_MESO,
            register: 1,
            record: RECORDS,
        },
        Stub {
            address: SAVE_USER,
            register: 0,
            record: RECORDS + 8,
        },
        Stub {
            address: SAVE_INDEX,
            register: 0,
            record: RECORDS + 0x10,
        },
        Stub {
            address: REFRESH,
            register: 0,
            record: RECORDS + 0x18,
        },
        Stub {
            address: SHOW_MESSAGE,
            register: 2,
            record: RECORDS + 0x20,
        },
    ];

    fn write_halfwords(core: &mut ArmCore, address: u32, halfwords: &[u16]) -> Result<()> {
        let bytes: alloc::vec::Vec<u8> = halfwords.iter().flat_map(|x| x.to_le_bytes()).collect();
        core.write_bytes(address, &bytes)
    }

    /// The title's dispatcher, exits and state table as they are, and stubs
    /// for what the routine calls.
    fn title(core: &mut ArmCore) -> Result<()> {
        core.load(&[], 0x1000, 0xa2000)?;
        core.load(&[], 0x150_0000, 0x3000)?;

        // push {r4-r7,lr}; ldr r3,[r0,#0x40]; subs r3,#2; adds r7,r0,#0;
        // cmp r3,#0x12; bls; b CLEAR_STATE; ldr r2,=table; lsls r3,r3,#2;
        // ldr r3,[r3,r2]; mov pc,r3
        write_halfwords(
            core,
            0x90d38,
            &[0xb5f0, 0x6c03, 0x3b02, 0x1c07, 0x2b12, 0xd900, 0xe181, 0x4ac4, 0x009b, 0x589b, 0x469f],
        )?;
        write_generic(core, 0x91058, STATE_TABLE)?;
        for state in 0..19 {
            write_generic(core, STATE_TABLE + state * 4, CLEAR_STATE)?;
        }
        write_generic(core, STATE3_ENTRY, STATE3)?;
        write_halfwords(core, CLEAR_STATE, &[0x2301, 0x425b, 0x643b, 0xbcf0, 0xbc01, 0x4700])?;

        // The original state 3: ldr r3,=ORIGINAL; str r7,[r3]; b CLEAR_STATE.
        write_halfwords(core, STATE3, &[0x4b01, 0x601f, 0xe129, 0])?;
        write_generic(core, STATE3 + 8, ORIGINAL)?;

        // Each stub: ldr r3,=record; str rN,[r3]; ldr r0,[r3,#4]; bx lr.
        for stub in &STUBS {
            write_halfwords(core, stub.address, &[0x4b01, 0x6018 | stub.register, 0x6858, 0x4770])?;
            write_generic(core, stub.address + 8, stub.record)?;
            write_generic(core, stub.record, 0u32)?;
            write_generic(core, stub.record + 4, 1u32)?;
        }

        write_generic(core, GAME, GAME_OBJECT)?;
        write_generic(core, GAME_OBJECT + 0x48, SLOTS)?;
        write_generic(core, SHOP + 0x40, 3u32)?;
        write_generic(core, SHOP + 0x5c, PLAYER)?;
        write_generic(core, SHOP + 0x60, ITEM)?;
        write_generic(core, PLAYER + 0x70, INVENTORY)?;
        write_generic(core, ITEM + 0xd8, 50000u32)?;

        install(core)
    }

    struct Purchase {
        item: u32,
        quantity: u32,
        slots: u8,
        can_pay: bool,
    }

    struct Outcome {
        slots: u8,
        state: i32,
        charged: i32,
        saved: bool,
        message: u32,
        original: bool,
    }

    fn buy(purchase: Purchase) -> Outcome {
        let mut system = System::new(Box::new(TestPlatform::new()), "", "", DefaultTaskRunner);
        let done = Arc::new(AtomicBool::new(false));
        let outcome = Arc::new(spin::Mutex::new(None));

        let (done_clone, outcome_clone) = (done.clone(), outcome.clone());
        system.spawn(async move || {
            let mut core = ArmCore::new(false, None)?;
            Allocator::init(&mut core)?;
            let mut context = core.save_context();
            let stack = Allocator::alloc(&mut core, 0x400)?;
            context.sp = stack + 0x400;
            core.restore_context(&context);

            title(&mut core)?;
            write_generic(&mut core, ITEM + 0xcc, purchase.item)?;
            write_generic(&mut core, SHOP + 0xc, purchase.quantity)?;
            write_generic(&mut core, SLOTS + 8, purchase.slots)?;
            write_generic(&mut core, RECORDS + 4, purchase.can_pay as u32)?;

            let _: u32 = core.run_function(0x90d39, &[SHOP]).await?;

            *outcome_clone.lock() = Some(Outcome {
                slots: read_generic(&core, SLOTS + 8)?,
                state: read_generic(&core, SHOP + 0x40)?,
                charged: read_generic(&core, RECORDS)?,
                saved: read_generic::<u32, _>(&core, RECORDS + 8)? == GAME_OBJECT && read_generic::<u32, _>(&core, RECORDS + 0x10)? == GAME_OBJECT,
                message: read_generic(&core, RECORDS + 0x20)?,
                original: read_generic::<u32, _>(&core, ORIGINAL)? == SHOP,
            });
            done_clone.store(true, Ordering::SeqCst);

            Ok::<_, wie_util::WieError>(())
        });

        while !done.load(Ordering::SeqCst) {
            system.tick().unwrap();
        }

        outcome.lock().take().unwrap()
    }

    #[test]
    fn one_bought_adds_a_slot_and_saves() {
        let outcome = buy(Purchase {
            item: 0xee,
            quantity: 1,
            slots: 3,
            can_pay: true,
        });

        assert_eq!(outcome.slots, 4);
        assert_eq!(outcome.charged, -50000);
        assert!(outcome.saved);
        assert_eq!(outcome.message, ADDED_TEXT);
        assert_eq!(outcome.state, 0xd, "the message box's state");
        assert!(!outcome.original);
    }

    #[test]
    fn several_bought_add_only_the_room_left_and_charge_for_those() {
        let outcome = buy(Purchase {
            item: 0xee,
            quantity: 5,
            slots: 4,
            can_pay: true,
        });

        assert_eq!(outcome.slots, 6);
        assert_eq!(outcome.charged, -100000);
    }

    #[test]
    fn no_room_left_is_refused_without_a_charge() {
        let outcome = buy(Purchase {
            item: 0xee,
            quantity: 1,
            slots: 6,
            can_pay: true,
        });

        assert_eq!(outcome.slots, 6);
        assert_eq!(outcome.charged, 0);
        assert!(!outcome.saved);
        assert_eq!(outcome.message, FULL_TEXT);
        assert_eq!(outcome.state, 0xd);
    }

    #[test]
    fn a_charge_the_player_cannot_pay_changes_nothing() {
        let outcome = buy(Purchase {
            item: 0xee,
            quantity: 1,
            slots: 3,
            can_pay: false,
        });

        assert_eq!(outcome.slots, 3);
        assert!(!outcome.saved);
        assert_eq!(outcome.message, 0);
        assert_eq!(outcome.state, -1);
    }

    #[test]
    fn other_items_go_to_the_original_purchase() {
        let outcome = buy(Purchase {
            item: 0xdc,
            quantity: 1,
            slots: 3,
            can_pay: true,
        });

        assert!(outcome.original);
        assert_eq!(outcome.slots, 3);
        assert_eq!(outcome.charged, 0);
        assert_eq!(outcome.state, -1);
    }

    #[test]
    fn the_routine_matches_its_assembly() {
        // Assembled with clang/ld.lld at ROUTINE_BASE.
        let expected: [u16; 66] = [
            0x6e3b, 0x33cc, 0x681b, 0x2bee, 0xd001, 0xf790, 0xfef1, 0x4c1a, 0x68fe, 0x2e00, 0xdd2e, 0x6820, 0x6c80, 0x7a05, 0x2306, 0x1b5d, 0xdd1d,
            0x42ae, 0xda00, 0x1c35, 0x6e3b, 0x33d8, 0x6819, 0x4369, 0x4249, 0x6dfb, 0x6f18, 0x2200, 0xf71a, 0xf988, 0x0600, 0xd019, 0x6820, 0x6c82,
            0x7a13, 0x195b, 0x7213, 0xf70a, 0xfa43, 0x6820, 0xf714, 0xf970, 0x6df8, 0xf753, 0xff99, 0x4a08, 0xe000, 0x4a08, 0x230d, 0x643b, 0x6820,
            0x2101, 0x4249, 0x2303, 0xf704, 0xfa0e, 0xf790, 0xffee, 0xf790, 0xffe9, 0x0004, 0x0150, 0xab54, 0x0009, 0x1eb4, 0x000a,
        ];

        assert_eq!(routine(), expected);
    }
}
