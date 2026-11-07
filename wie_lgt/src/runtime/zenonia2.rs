//! 제노니아2 (`0002C004`): a frame drawn after the animation it was copied
//! from has gone.
//!
//! An animation is `[+8]` its state (the frame index in the first byte),
//! `[+0x10]` a table of twelve-byte frames - the frame's image object, then its
//! offset as two halfwords - and `[+0x14]` the frame count. A copy of an
//! animation (`0x70ba8`, `0x70bf4`) takes the table of the one it was copied
//! from rather than one of its own, and marks itself as not owning it. The
//! owner's teardown (`0x70c40`) deletes every frame's image, writes a zero
//! over each one in the table and frees the table, with nothing to tell a copy
//! still holding it.
//!
//! The boss fights are where a copy outlives its owner: a third or so of the
//! boss's health in, the next timer tick drew a copy whose owner - the skill
//! effects `dynamic/SkillEffect*.pzx` load for a skill and let go of after it -
//! had been torn down a moment before. The frame's image read as zero, its
//! draw went through the zero's "vtable" to address 0, and the run ended there
//! (`LR=0x70eef PC=0`, twice in logs from two plays). What the table held
//! after its free was the owner's own zeros, which is what a handset would
//! read there as well.
//!
//! The two frame draws (`0x70e10`, through the image's sixth slot, and
//! `0x70e84`, through its fifth) are rewritten in place to draw nothing for a
//! frame without an image. The rewrite does what the original does, in its
//! space: the image is read before the flip test rather than after it, which
//! leaves room for the check.

use alloc::vec::Vec;

use wie_core_arm::ArmCore;
use wie_util::{ByteRead, ByteWrite, Result};

const AID: &str = "0002C004";

/// The two frame draws, with the offset of the image's vtable slot each one
/// draws through.
const DRAWS: [(u32, u16); 2] = [(0x70e10, 0x14), (0x70e84, 0x10)];

/// What both draws call: the flip of the frame's offset, and the
/// `bx r4` the slot is called through.
const FLIP: u32 = 0x70db8;
const CALL_R4: u32 = 0x807a8;

/// Each draw's length in halfwords.
const LENGTH: usize = 57;

/// Rewrites the frame draws if this is the title, and they are the code this
/// was written against.
pub(crate) fn install_frame_guards(core: &mut ArmCore, aid: &str) -> Result<()> {
    if !aid.eq_ignore_ascii_case(AID) {
        return Ok(());
    }

    for (address, slot) in DRAWS {
        if !halfwords_at(core, address, &original(address, slot))? {
            tracing::warn!("제노니아2 frame draw at {address:#x}: code differs from the known build; not guarded");
            continue;
        }

        let code: Vec<u8> = guarded(address, slot).iter().flat_map(|x| x.to_le_bytes()).collect();
        core.write_bytes(address, &code)?;
    }
    tracing::info!("제노니아2: a frame whose image is gone is skipped rather than drawn");

    Ok(())
}

/// `(animation, x, y, flags, [sp] a, [sp+4] b)`: the frame at the animation's
/// index, its offset flipped by `FLIP` when `flags` asks, drawn through the
/// image's `slot` as `(image, x + dx, y + dy, flags, a, b)`.
fn original(address: u32, slot: u16) -> Vec<u16> {
    let mut code = Vec::with_capacity(LENGTH);

    code.extend_from_slice(&[0xb5f0, 0xb086, 0x9302, 0x6883, 0x9104, 0x9203]); // push; sub sp,#0x18; str r3,[sp,#8]; ldr r3,[r0,#8]; str r1,[sp,#0x10]; str r2,[sp,#0xc]
    code.extend_from_slice(&[0x781a, 0x230c, 0x435a, 0x6903, 0x466f, 0x189c]); // ldrb r2,[r3]; movs r3,#0xc; muls; ldr r3,[r0,#0x10]; mov r7,sp; adds r4,r3,r2
    code.extend_from_slice(&[0x88a3, 0x3716, 0x803b, 0x88e3, 0x9a0b, 0xae05, 0x8033]); // the offset to [sp+0x16], [sp+0x14]
    code.extend_from_slice(&[0x1c39, 0x9200, 0x9b02, 0x1c32]); // adds r1,r7,#0; str r2,[sp]; ldr r3,[sp,#8]; adds r2,r6,#0
    code.extend_from_slice(&bl(address + code.len() as u32 * 2, FLIP));
    code.extend_from_slice(&[0x0600, 0x6825, 0x2800, 0xd005]); // lsls r0,r0,#24; ldr r5,[r4]; cmp r0,#0; beq unflipped
    code.extend_from_slice(&[0x682b, 0x2000, 0x5e39, 0x2000, 0x5e32, 0xe004]); // ldr r3,[r5]; the flipped offset; b call
    code.extend_from_slice(&[0x682b, 0x2004, 0x5e21, 0x2006, 0x5e22]); // unflipped: ldr r3,[r5]; the table's offset
    code.extend_from_slice(&[0x980b, 0x9000, 0x980c, 0x9001, slot_load(slot)]); // call: the stack arguments; ldr r4,[r3,#slot]
    code.extend_from_slice(&[0x9b04, 0x1c28, 0x1859, 0x9b03, 0x189a, 0x9b02]); // r0 the image, r1 x + dx, r2 y + dy, r3 flags
    code.extend_from_slice(&bl(address + code.len() as u32 * 2, CALL_R4));
    code.extend_from_slice(&[0xb006, 0xbcf0, 0xbc01, 0x4700]); // add sp,#0x18; pop {r4-r7}; pop {r0}; bx r0

    code
}

/// [`original`], returning before anything is read through a frame's image
/// when it has none.
fn guarded(address: u32, slot: u16) -> Vec<u16> {
    let mut code = Vec::with_capacity(LENGTH);

    code.extend_from_slice(&[0xb5f0, 0xb086, 0x9302, 0x6883, 0x9104, 0x9203]); // 00 push; sub sp,#0x18; the arguments
    code.extend_from_slice(&[0x781a, 0x230c, 0x435a, 0x6903, 0x189c]); // 0c r4 the frame
    code.extend_from_slice(&[0x6825, 0x2d00, 0xd025]); // 16 ldr r5,[r4]; cmp r5,#0; beq done
    code.extend_from_slice(&[0x466f, 0x3716, 0x88a3, 0x803b, 0x88e3, 0x9a0b, 0xae05, 0x8033]); // 1c mov r7,sp; the offset
    code.extend_from_slice(&[0x1c39, 0x9200, 0x9b02, 0x1c32]); // 2c
    code.extend_from_slice(&bl(address + code.len() as u32 * 2, FLIP)); // 34 bl FLIP
    code.extend_from_slice(&[0x0600, 0xd003]); // 38 lsls r0,r0,#24; beq unflipped
    code.extend_from_slice(&[0x2000, 0x5e39, 0x5e32, 0xe003]); // 3c movs r0,#0; ldrsh r1,[r7,r0]; ldrsh r2,[r6,r0]; b call
    code.extend_from_slice(&[0x2004, 0x5e21, 0x2006, 0x5e22]); // 44 unflipped
    code.extend_from_slice(&[0x682b, 0x980b, 0x9000, 0x980c, 0x9001, slot_load(slot)]); // 4c call: ldr r3,[r5]; ...; ldr r4,[r3,#slot]
    code.extend_from_slice(&[0x9b04, 0x1c28, 0x1859, 0x9b03, 0x189a, 0x9b02]); // 58
    code.extend_from_slice(&bl(address + code.len() as u32 * 2, CALL_R4)); // 64 bl CALL_R4
    code.extend_from_slice(&[0xb006, 0xbcf0, 0xbc01, 0x4700]); // 68 done: add sp,#0x18; pop {r4-r7}; pop {r0}; bx r0
    code.push(0x46c0); // 70 nop, where the original's return ended

    code
}

/// `ldr r4,[r3,#slot]`.
fn slot_load(slot: u16) -> u16 {
    0x681c | ((slot / 4) << 6)
}

/// A Thumb `bl` at `from` to the Thumb code at `to`.
fn bl(from: u32, to: u32) -> [u16; 2] {
    let offset = to.wrapping_sub(from + 4) as i32;
    debug_assert!((-0x40_0000..0x40_0000).contains(&offset));

    [0xf000 | ((offset >> 12) as u16 & 0x7ff), 0xf800 | ((offset >> 1) as u16 & 0x7ff)]
}

fn halfwords_at(core: &ArmCore, address: u32, expected: &[u16]) -> Result<bool> {
    let mut bytes = alloc::vec![0; expected.len() * 2];
    core.read_bytes(address, &mut bytes)?;

    Ok(bytes.chunks(2).map(|x| u16::from_le_bytes([x[0], x[1]])).eq(expected.iter().copied()))
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

    const ANIMATION: u32 = 0x150_0000;
    const STATE: u32 = 0x150_0100;
    const TABLE: u32 = 0x150_0200;
    const IMAGE: u32 = 0x150_0300;
    const VTABLE: u32 = 0x150_0400;
    /// The image's draw stub, and where it records the `x` it was given.
    const DRAW: u32 = 0x150_0500;
    const RECORD: u32 = 0x150_0600;

    #[test]
    fn the_originals_are_the_title_s_code() {
        // As the title has them, read out of `binary.mod`.
        let at_0x70e84: [u16; LENGTH] = [
            0xb5f0, 0xb086, 0x9302, 0x6883, 0x9104, 0x9203, 0x781a, 0x230c, 0x435a, 0x6903, 0x466f, 0x189c, 0x88a3, 0x3716, 0x803b, 0x88e3, 0x9a0b,
            0xae05, 0x8033, 0x1c39, 0x9200, 0x9b02, 0x1c32, 0xf7ff, 0xff81, 0x0600, 0x6825, 0x2800, 0xd005, 0x682b, 0x2000, 0x5e39, 0x2000, 0x5e32,
            0xe004, 0x682b, 0x2004, 0x5e21, 0x2006, 0x5e22, 0x980b, 0x9000, 0x980c, 0x9001, 0x691c, 0x9b04, 0x1c28, 0x1859, 0x9b03, 0x189a, 0x9b02,
            0xf00f, 0xfc5d, 0xb006, 0xbcf0, 0xbc01, 0x4700,
        ];
        assert_eq!(original(0x70e84, 0x10), at_0x70e84);

        let at_0x70e10 = original(0x70e10, 0x14);
        assert_eq!(&at_0x70e10[23..25], &[0xf7ff, 0xffbb]);
        assert_eq!(at_0x70e10[44], 0x695c);
        assert_eq!(&at_0x70e10[51..53], &[0xf00f, 0xfc97]);

        for (address, slot) in DRAWS {
            assert_eq!(guarded(address, slot).len(), LENGTH);
        }
    }

    /// Draws the animation's current frame through the draw at `address`,
    /// returning the `x` the image was drawn at, if it was.
    fn draw(address: u32, slot: u16, frame: u8) -> Option<u32> {
        let mut system = System::new(Box::new(TestPlatform::new()), "", "", DefaultTaskRunner);
        let done = Arc::new(AtomicBool::new(false));
        let drawn = Arc::new(spin::Mutex::new(None));

        let (done_clone, drawn_clone) = (done.clone(), drawn.clone());
        system.spawn(async move || {
            let mut core = ArmCore::new(false, None)?;
            Allocator::init(&mut core)?;
            let mut context = core.save_context();
            let stack = Allocator::alloc(&mut core, 0x400)?;
            context.sp = stack + 0x400;
            core.restore_context(&context);

            core.load(&[], 0x70000, 0x11000)?;
            core.load(&[], 0x150_0000, 0x1000)?;
            let code: Vec<u8> = original(address, slot).iter().flat_map(|x| x.to_le_bytes()).collect();
            core.write_bytes(address, &code)?;
            core.write_bytes(FLIP, &[0x00, 0x20, 0x70, 0x47])?; // movs r0,#0; bx lr
            core.write_bytes(CALL_R4, &[0x20, 0x47])?; // bx r4
            // ldr r3,=RECORD; str r1,[r3]; bx lr
            core.write_bytes(DRAW, &[0x01, 0x4b, 0x19, 0x60, 0x70, 0x47, 0, 0])?;
            write_generic(&mut core, DRAW + 8, RECORD)?;

            write_generic(&mut core, ANIMATION + 8, STATE)?;
            write_generic(&mut core, ANIMATION + 0x10, TABLE)?;
            core.write_bytes(STATE, &[frame])?;
            // Frame 0 has an image, at an offset of (3, 4); frame 1 lost its.
            write_generic(&mut core, TABLE, IMAGE)?;
            core.write_bytes(TABLE + 4, &[3, 0, 4, 0])?;
            write_generic(&mut core, TABLE + 12, 0u32)?;
            write_generic(&mut core, IMAGE, VTABLE)?;
            write_generic(&mut core, VTABLE + slot as u32, DRAW | 1)?;
            write_generic(&mut core, RECORD, u32::MAX)?;

            install_frame_guards(&mut core, AID)?;
            let _: u32 = core.run_function(address | 1, &[ANIMATION, 10, 20, 0]).await?;

            let x: u32 = read_generic(&core, RECORD)?;
            *drawn_clone.lock() = (x != u32::MAX).then_some(x);
            done_clone.store(true, Ordering::SeqCst);

            Ok::<_, wie_util::WieError>(())
        });

        while !done.load(Ordering::SeqCst) {
            system.tick().unwrap();
        }

        drawn.lock().take()
    }

    #[test]
    fn a_frame_with_its_image_is_drawn_at_its_offset() {
        for (address, slot) in DRAWS {
            assert_eq!(draw(address, slot, 0), Some(13), "the draw at {address:#x}");
        }
    }

    #[test]
    fn a_frame_without_one_is_not_drawn() {
        for (address, slot) in DRAWS {
            assert_eq!(draw(address, slot, 1), None, "the draw at {address:#x}");
        }
    }

    #[test]
    fn another_title_is_left_alone() -> Result<()> {
        let mut core = ArmCore::new(false, None)?;
        core.load(&[], 0x70000, 0x1000)?;
        core.write_bytes(0x70e84, &[0xaa; 4])?;

        install_frame_guards(&mut core, "0002E88E")?;
        install_frame_guards(&mut core, AID)?;

        let mut bytes = [0; 4];
        core.read_bytes(0x70e84, &mut bytes)?;
        assert_eq!(bytes, [0xaa; 4]);

        Ok(())
    }
}
